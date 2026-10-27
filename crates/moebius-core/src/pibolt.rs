//! The Pi-Bolt harness (spec 4): each run is `pi-bolt --mode rpc` in the run's worktree, with the router as its only
//! provider. The prompt goes in as one JSON line; every record Pi writes is stored as a run event and published on the
//! bus. Records are split on LF only, never on U+2028 or U+2029, which are valid inside JSON strings. The run ends at
//! `agent_settled`; what the agent changed is committed on the run's branch and the worktree is removed. A chat is
//! the exception: its Pi stays up at `agent_settled`, the run waits for the person's next message, and it ends when
//! the person ends or stops it (or Pi exits). A chat works in a worktree on `moebius/chat-<id>` or, without a
//! repository, in an empty directory; what it left is committed on the branch when it ends, and when the branch
//! then has commits beyond the default branch they are offered as a change without a card. A `code` run
//! that changed nothing has failed; other runs, such as a review, are not committed. A run starts from the branch of the
//! card's latest successful `code` run, so it sees the work so far. The text of the agent's final answer is kept as
//! the run's summary. A run is stopped with `abort` and steered with `steer`. Pi runs offline and does not trust
//! configuration found in the worktree. The Moebius bridge (`bridge/moebius-bridge.ts`) is loaded into every run before the agent's own
//! extensions; it asks the check for each tool call and result (see `check`). Every run is told the path of the
//! `moebius` executable as `$MOEBIUS`. The Moebius agent is always available: its instructions are built in, its skill
//! is the guide (see `skill`), and its model comes from `[agents.Moebius]` or else from the Coder.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::app::App;
use crate::blobs::redact;
use crate::board::Runs;
use crate::bus::AppEvent;
use crate::config::AgentConfig;
use crate::repos::git;
use crate::router::{Grant, Protocol, Router};
use crate::skill;
use crate::store::{Card, Repository, Run};

/// How long Pi may take to exit after it was asked to stop.
const STOP_GRACE: Duration = Duration::from_secs(10);

/// The bridge extension, written into each run's directory.
const BRIDGE: &str = include_str!("../bridge/moebius-bridge.ts");

/// The name of the bundled agent that builds on Moebius, which every chat without an agent starts.
pub const MOEBIUS_AGENT: &str = "Moebius";

/// What the Moebius agent is told; the `instructions` of `[agents.Moebius]` come after it.
const MOEBIUS_INSTRUCTIONS: &str = "You are the Moebius agent. You work for the person in this chat and build what they \
     ask for on Moebius: column scripts, rules, agents, plugins and Pi extensions. Use the moebius skill: it holds the \
     guide and the exact references, and says how to try what you build. In a repository you work in a worktree on a \
     branch of its own; when the chat ends, what you left there is offered to the person as a change to merge.";

/// Starts and stops Pi-Bolt runs.
pub struct PiBolt {
    /// The `pi-bolt` executable.
    pub program: PathBuf,
    /// The `moebius` executable, which runs find as `$MOEBIUS`.
    pub moebius: PathBuf,
    /// The router's base URL, such as `http://127.0.0.1:7172`.
    pub router_url: String,
    pub router: Arc<Router>,
    pub agents: BTreeMap<String, AgentConfig>,
    live: Arc<Mutex<HashMap<i64, Live>>>,
}

/// A run whose Pi has not ended yet.
#[derive(Clone)]
struct Live {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Child>>,
    chat: bool,
    /// Why a chat's Pi was asked to exit, so that its exit is no failure.
    closing: Arc<Mutex<Option<Closing>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Closing {
    Stopped,
    Ended,
}

impl PiBolt {
    pub fn new(
        program: PathBuf,
        moebius: PathBuf,
        router_url: String,
        router: Arc<Router>,
        agents: BTreeMap<String, AgentConfig>,
    ) -> Self {
        Self {
            program,
            moebius,
            router_url,
            router,
            agents,
            live: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The agent `name` as a run uses it. The Moebius agent is made of its configuration, or else the Coder's model
    /// and protocol, and always has the built-in instructions and the `moebius` skill.
    fn agent(&self, app: &App, name: &str) -> Result<AgentConfig, String> {
        let configured = self.agents.get(name);
        if name != MOEBIUS_AGENT {
            return configured
                .cloned()
                .ok_or_else(|| format!("no agent named {name}"));
        }
        let mut agent = match (configured, self.agents.get("Coder")) {
            (Some(own), _) => own.clone(),
            (None, Some(coder)) => AgentConfig {
                model: coder.model.clone(),
                protocol: coder.protocol,
                thinking: None,
                instructions: None,
                skills: vec![],
                extensions: vec![],
                token_budget: None,
            },
            (None, None) => {
                return Err(
                    "the Moebius agent needs a model: configure [agents.Moebius] or \
                     [agents.Coder]"
                        .into(),
                );
            }
        };
        agent.instructions = Some(match agent.instructions {
            Some(extra) => format!("{MOEBIUS_INSTRUCTIONS}\n\n{extra}"),
            None => MOEBIUS_INSTRUCTIONS.into(),
        });
        agent.skills.insert(0, skill::dir(&app.data_dir));
        Ok(agent)
    }

    /// The command that starts Pi for one run.
    fn command(
        &self,
        agent: &AgentConfig,
        worktree: &Path,
        run_dir: &Path,
        agent_dir: &Path,
        bridge: &Path,
        token: &str,
    ) -> Command {
        let mut command = Command::new(&self.program);
        command
            .current_dir(worktree)
            .env("PI_CODING_AGENT_DIR", agent_dir)
            .env("MOEBIUS", &self.moebius)
            .env("MOEBIUS_CHECK_URL", &self.router_url)
            .env("MOEBIUS_RUN_TOKEN", token)
            .args(["--mode", "rpc", "--offline", "--no-approve"])
            .args(["--provider", "moebius", "--model", &agent.model])
            .arg("--session-dir")
            .arg(run_dir.join("sessions"))
            .arg("--extension")
            .arg(bridge);
        if let Some(thinking) = &agent.thinking {
            command.args(["--thinking", thinking]);
        }
        if let Some(instructions) = &agent.instructions {
            command.arg("--append-system-prompt").arg(instructions);
        }
        for skill in &agent.skills {
            command.arg("--skill").arg(skill);
        }
        for extension in &agent.extensions {
            command.arg("--extension").arg(extension);
        }
        command
    }

    /// Writes the per-run agent directory: `models.json` with the router as the `moebius` provider.
    fn write_agent_dir(
        &self,
        run_dir: &Path,
        agent: &AgentConfig,
        token: &str,
    ) -> std::io::Result<PathBuf> {
        let agent_dir = run_dir.join("agent");
        std::fs::create_dir_all(&agent_dir)?;
        let (api, base_url) = match agent.protocol {
            Protocol::OpenAi => ("openai-completions", format!("{}/v1", self.router_url)),
            Protocol::Anthropic => ("anthropic-messages", self.router_url.clone()),
        };
        let models = json!({
            "providers": {
                "moebius": {
                    "baseUrl": base_url,
                    "api": api,
                    "apiKey": token,
                    "models": [{ "id": agent.model }]
                }
            }
        });
        std::fs::write(
            agent_dir.join("models.json"),
            serde_json::to_vec_pretty(&models)?,
        )?;
        Ok(agent_dir)
    }

    /// Starts Pi for a run whose worktree and token exist, and the thread that reads it.
    fn launch(
        &self,
        app: &Arc<App>,
        card: Option<&Card>,
        agent: &AgentConfig,
        run: RunningRun,
        prompt: &str,
    ) -> Result<(), String> {
        app.store
            .set_run_worktree(run.id, &run.worktree.to_string_lossy())
            .map_err(|e| e.to_string())?;
        let run_dir = app.data_dir.join("runs").join(run.id.to_string());
        let agent_dir = self
            .write_agent_dir(&run_dir, agent, &run.token)
            .map_err(|e| format!("cannot write the agent directory: {e}"))?;
        let bridge = run_dir.join("moebius-bridge.ts");
        std::fs::write(&bridge, BRIDGE)
            .map_err(|e| format!("cannot write the bridge extension: {e}"))?;
        let stderr =
            std::fs::File::create(run_dir.join("stderr.log")).map_err(|e| e.to_string())?;
        let mut child = self
            .command(
                agent,
                &run.worktree,
                &run_dir,
                &agent_dir,
                &bridge,
                &run.token,
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", self.program.display()))?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let live = Live {
            stdin: Arc::new(Mutex::new(child.stdin.take())),
            child: Arc::new(Mutex::new(child)),
            chat: run.purpose == "chat",
            closing: Arc::new(Mutex::new(None)),
        };
        lock(&self.live).insert(run.id, live.clone());
        app.store
            .set_run_status(run.id, "running")
            .map_err(|e| e.to_string())?;
        app.bus.publish(AppEvent::new(
            "run_started",
            json!({ "run": run.id, "card": card.map(|card| card.id) }),
        ));
        send(
            &live.stdin,
            &json!({ "id": "moebius-prompt", "type": "prompt", "message": prompt }),
        );
        let reader = Reader {
            app: app.clone(),
            run,
            live,
            router: self.router.clone(),
            lives: self.live.clone(),
        };
        let name = format!("pi-bolt run {}", reader.run.id);
        std::thread::Builder::new()
            .name(name)
            .spawn(move || reader.read(stdout))
            .map(drop)
            .map_err(|e| e.to_string())
    }
    /// Launches a run that has its worktree and token and returns it; when Pi cannot be started, what was set up for
    /// the run is undone and the run has failed.
    fn launch_or_undo(
        &self,
        app: &Arc<App>,
        card: Option<&Card>,
        config: &AgentConfig,
        running: RunningRun,
        prompt: &str,
    ) -> Result<Run, String> {
        if let Err(e) = self.launch(app, card, config, running.clone(), prompt) {
            // Nothing reads this run: undo what was set up for it.
            self.router.revoke_token(&running.token);
            running.remove_worktree(app);
            return Err(fail(app, running.id, e));
        }
        Ok(app
            .store
            .run(running.id)
            .map_err(|e| e.to_string())?
            .expect("the run exists"))
    }
}

impl Runs for PiBolt {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
        note: Option<&str>,
    ) -> Result<Run, String> {
        let config = self.agent(app, agent)?;
        let repository = app
            .store
            .repository(card.repository_id)
            .map_err(|e| e.to_string())?
            .ok_or("the card's repository is gone")?;
        let run = app
            .store
            .add_run(Some(card.id), agent, "pi-bolt", purpose)
            .map_err(|e| e.to_string())?;

        let clone = PathBuf::from(&repository.path);
        let base = match app.store.code_branch(card.id) {
            Ok(Some(branch)) => branch,
            Ok(None) => repository.base_ref(),
            Err(e) => return Err(fail(app, run.id, e.to_string())),
        };
        let worktree = app
            .repos
            .add_worktree(&repository.name, &clone, run.id, &base)
            .map_err(|e| fail(app, run.id, e.to_string()))?;
        let running = RunningRun {
            id: run.id,
            clone: Some(clone),
            worktree: worktree.path,
            token: self.router.issue_token(Grant {
                run_id: run.id,
                card_id: Some(card.id),
                budget: config.token_budget,
            }),
            purpose: purpose.to_string(),
            commit_message: format!(
                "#{}: {}\n\nCommitted by Moebius for its run.",
                card.number, card.title
            ),
        };
        self.launch_or_undo(
            app,
            Some(card),
            &config,
            running,
            &prompt_for(card, purpose, note),
        )
    }

    fn start_chat(
        &self,
        app: &Arc<App>,
        agent: &str,
        repository: Option<&Repository>,
        message: &str,
    ) -> Result<Run, String> {
        let config = self.agent(app, agent)?;
        let run = app
            .store
            .add_run(None, agent, "pi-bolt", "chat")
            .map_err(|e| e.to_string())?;
        let (clone, worktree) = match repository {
            Some(repository) => {
                let clone = PathBuf::from(&repository.path);
                let made = app
                    .store
                    .set_run_repository(run.id, repository.id)
                    .map_err(|e| e.to_string())
                    .and_then(|()| {
                        let base = repository.base_ref();
                        let name = &repository.name;
                        let worktree = app.repos.add_chat_worktree(name, &clone, run.id, &base);
                        worktree.map_err(|e| e.to_string())
                    })
                    .map_err(|e| fail(app, run.id, e))?;
                (Some(clone), made.path)
            }
            None => {
                let empty = app.data_dir.join("chats").join(run.id.to_string());
                std::fs::create_dir_all(&empty).map_err(|e| fail(app, run.id, e.to_string()))?;
                (None, empty)
            }
        };
        let running = RunningRun {
            id: run.id,
            clone,
            worktree,
            token: self.router.issue_token(Grant {
                run_id: run.id,
                card_id: None,
                budget: config.token_budget,
            }),
            purpose: "chat".into(),
            commit_message: format!(
                "Chat {}\n\nCommitted by Moebius when the chat ended.",
                run.id
            ),
        };
        let said = json!({ "message": redact(message) });
        let _ = app.record_run_event(run.id, "said", &said);
        self.launch_or_undo(app, None, &config, running, message)
    }

    fn say(&self, run_id: i64, message: &str) -> bool {
        let Some(live) = lock(&self.live).get(&run_id).cloned().filter(|l| l.chat) else {
            return false;
        };
        // Pi starts a run for the prompt, or queues it when the agent is working, so the answer does not depend on
        // whether the agent has just settled.
        send(
            &live.stdin,
            &json!({ "type": "prompt", "message": message, "streamingBehavior": "followUp" }),
        )
    }

    fn end(&self, run_id: i64) -> bool {
        let Some(live) = lock(&self.live).get(&run_id).cloned().filter(|l| l.chat) else {
            return false;
        };
        *lock(&live.closing) = Some(Closing::Ended);
        // Closing stdin asks Pi to shut down.
        lock(&live.stdin).take().is_some()
    }

    fn has_agent(&self, name: &str) -> bool {
        name == MOEBIUS_AGENT || self.agents.contains_key(name)
    }

    fn steer(&self, run_id: i64, message: &str) -> bool {
        let Some(live) = lock(&self.live).get(&run_id).cloned() else {
            return false;
        };
        send(
            &live.stdin,
            &json!({ "id": "moebius-steer", "type": "steer", "message": message }),
        )
    }

    /// Asks Pi to abort; the run ends with status `stopped` once Pi has settled. A Pi that does not exit within
    /// `STOP_GRACE` is killed.
    fn stop(&self, run_id: i64) -> bool {
        let Some(live) = lock(&self.live).get(&run_id).cloned() else {
            return false;
        };
        send(
            &live.stdin,
            &json!({ "id": "moebius-abort", "type": "abort" }),
        );
        if live.chat {
            // A waiting chat has no turn to abort, so Pi is also asked to exit.
            *lock(&live.closing) = Some(Closing::Stopped);
            lock(&live.stdin).take();
        }
        std::thread::spawn(move || {
            if !wait_for_exit(&live.child, STOP_GRACE) {
                let _ = lock(&live.child).kill();
            }
        });
        true
    }
}

/// What a started run holds until it ends.
#[derive(Clone)]
struct RunningRun {
    id: i64,
    /// The repository's clone; none for a chat in an empty directory.
    clone: Option<PathBuf>,
    worktree: PathBuf,
    token: String,
    purpose: String,
    /// The message of the commit of what the agent leaves in the worktree.
    commit_message: String,
}

impl RunningRun {
    /// Removes the worktree, or the chat's empty directory.
    fn remove_worktree(&self, app: &App) {
        match &self.clone {
            Some(clone) => drop(app.repos.remove_worktree(clone, &self.worktree)),
            None => drop(std::fs::remove_dir_all(&self.worktree)),
        }
    }
}

/// Fails a run that could not be started, with an event saying why; returns the message.
fn fail(app: &App, run_id: i64, message: String) -> String {
    let _ = app
        .store
        .append_event(run_id, "moebius_error", &json!({ "message": message }));
    let _ = app.store.set_run_status(run_id, "failed");
    message
}

/// What a `code` run is told after the issue.
const CODE_PROMPT: &str = "You are in a git worktree on the issue's own branch. Make the change, run what checks the \
     repository has, and stop when the issue is done. Moebius commits what you leave in the worktree.";

/// What a `review` run is told after the issue.
const REVIEW_PROMPT: &str = "You are the reviewer. You are in a git worktree on the branch that holds the change for \
     this issue. Review that change against the issue: is it correct, is it tested, and does it do what the issue \
     asks? Do not edit any files. Write your findings, then end your answer with one last line, either \
     `VERDICT: approve` or `VERDICT: changes`.";

/// The prompt for a run of `card`; `note` is added under a heading of its own.
fn prompt_for(card: &Card, purpose: &str, note: Option<&str>) -> String {
    let mut prompt = format!("Work on issue #{}: {}\n", card.number, card.title);
    if !card.body.trim().is_empty() {
        prompt.push('\n');
        prompt.push_str(&card.body);
        prompt.push('\n');
    }
    prompt.push('\n');
    prompt.push_str(if purpose == "review" {
        REVIEW_PROMPT
    } else {
        CODE_PROMPT
    });
    prompt.push('\n');
    if let Some(note) = note {
        prompt.push_str("\nNotes from the last round:\n\n");
        prompt.push_str(note);
        prompt.push('\n');
    }
    prompt
}

/// The text of the last assistant message in an `agent_end` record, if it has any.
fn final_answer(agent_end: &Value) -> Option<String> {
    let messages = agent_end["messages"].as_array()?;
    let last = messages.iter().rev().find(|m| m["role"] == "assistant")?;
    let text: String = last["content"]
        .as_array()?
        .iter()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    Some(text)
}

/// Writes one JSON line to Pi; false when its stdin is closed or broken.
fn send(stdin: &Mutex<Option<ChildStdin>>, value: &Value) -> bool {
    let mut guard = lock(stdin);
    let Some(pipe) = guard.as_mut() else {
        return false;
    };
    let mut line = value.to_string();
    line.push('\n');
    pipe.write_all(line.as_bytes())
        .and_then(|_| pipe.flush())
        .is_ok()
}

/// Waits up to `limit` for the process to exit; true when it did.
fn wait_for_exit(child: &Mutex<Child>, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        // Bound first, so the lock is not held while sleeping.
        let exited = lock(child).try_wait();
        match exited {
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => return false,
            _ => return true,
        }
    }
}

/// Locks a mutex, also when a thread panicked while holding it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

/// Reads one run's records and ends the run.
struct Reader {
    app: Arc<App>,
    run: RunningRun,
    live: Live,
    router: Arc<Router>,
    lives: Arc<Mutex<HashMap<i64, Live>>>,
}

impl Reader {
    fn read(self, stdout: impl Read) {
        let mut lines = BufReader::new(stdout);
        let mut record = Vec::new();
        let mut ending = Err("pi-bolt exited before it settled".to_string());
        loop {
            record.clear();
            match lines.read_until(b'\n', &mut record) {
                Ok(0) => {
                    if let Some(closing) = *lock(&self.live.closing) {
                        ending = Ok(closing == Closing::Stopped);
                    }
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    ending = Err(format!("cannot read from pi-bolt: {e}"));
                    break;
                }
            }
            while matches!(record.last(), Some(b'\n' | b'\r')) {
                record.pop();
            }
            if record.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_slice(&record).unwrap_or_else(|e| {
                json!({ "type": "unparsed", "error": e.to_string(), "text": String::from_utf8_lossy(&record) })
            });
            let kind = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let _ = self.app.record_run_event(self.run.id, &kind, &value);
            if kind == "response" && value["command"] == "prompt" && value["success"] == false {
                ending = Err(format!("pi-bolt refused the prompt: {}", value["error"]));
                break;
            }
            if kind == "agent_end"
                && let Some(answer) = final_answer(&value)
            {
                let _ = self.app.store.set_run_summary(self.run.id, &answer);
            }
            if self.run.purpose == "chat" && kind == "agent_start" {
                let _ = self.app.store.set_run_status(self.run.id, "running");
            }
            if kind == "agent_settled" {
                let aborted = value["aborted"] == true;
                // A chat waits for the person's next message; only an abort ends it.
                if self.run.purpose == "chat" && !aborted {
                    let _ = self.app.store.set_run_status(self.run.id, "waiting");
                    continue;
                }
                ending = Ok(aborted);
                break;
            }
        }
        self.end(ending);
    }

    /// Ends the run: `Ok(aborted)` when Pi settled, `Err(why)` when it failed.
    fn end(self, ending: Result<bool, String>) {
        // Closing stdin asks Pi to shut down.
        lock(&self.live.stdin).take();
        if !wait_for_exit(&self.live.child, Duration::from_secs(10)) {
            let mut child = lock(&self.live.child);
            let _ = child.kill();
            let _ = child.wait();
        }
        self.router.revoke_token(&self.run.token);
        let status = match ending {
            Ok(aborted) if self.run.purpose == "chat" => match self.commit() {
                // A chat that changed nothing has still succeeded.
                Ok(_) if aborted => "stopped",
                Ok(_) => "succeeded",
                Err(e) => self.failed(&e.to_string()),
            },
            Ok(true) => "stopped",
            Ok(false) if self.run.purpose != "code" => "succeeded",
            Ok(false) => match self.commit() {
                Ok(true) => "succeeded",
                Ok(false) => self.failed("the agent changed nothing"),
                Err(e) => self.failed(&e.to_string()),
            },
            Err(why) => self.failed(&why),
        };
        self.run.remove_worktree(&self.app);
        if self.run.purpose == "chat" && status != "failed" {
            // Before the status is set, so that an ended chat has offered what it made.
            if let Err(e) = self.app.offer_chat_change(self.run.id) {
                let why = format!("cannot offer the chat's work as a change: {e}");
                let _ = self.app.record_run_event(
                    self.run.id,
                    "moebius_error",
                    &json!({ "message": why }),
                );
            }
        }
        let _ = self.app.store.set_run_status(self.run.id, status);
        lock(&self.lives).remove(&self.run.id);
        self.app.bus.publish(AppEvent::new(
            "run_ended",
            json!({ "run": self.run.id, "status": status }),
        ));
    }

    /// Commits what the agent left in the worktree of a repository; false when it left nothing. A chat without a
    /// repository has nothing to commit.
    fn commit(&self) -> crate::repos::Result<bool> {
        if self.run.clone.is_none() {
            return Ok(false);
        }
        let worktree = &self.run.worktree;
        git(worktree, &["add", "--all"])?;
        if git(worktree, &["status", "--porcelain"])?.is_empty() {
            return Ok(false);
        }
        git(
            worktree,
            &["commit", "--quiet", "-m", &self.run.commit_message],
        )?;
        Ok(true)
    }

    fn failed(&self, why: &str) -> &'static str {
        let _ = self
            .app
            .record_run_event(self.run.id, "moebius_error", &json!({ "message": why }));
        "failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use moebius_plugin_api::Host;

    fn fake_pi() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi")
    }

    fn setup() -> (tempfile::TempDir, Arc<App>, PiBolt, i64) {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = Arc::new(App::new(
            store.clone(),
            Arc::new(Host::new(Duration::from_secs(1))),
            &dir.path().join("data"),
        ));
        let repo = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        let profile = AgentConfig {
            model: "qwen-local".into(),
            thinking: Some("low".into()),
            instructions: Some("Be brief.".into()),
            skills: vec![],
            extensions: vec![],
            protocol: Protocol::OpenAi,
            token_budget: Some(1000),
        };
        let pi = PiBolt::new(
            fake_pi(),
            "/usr/local/bin/moebius".into(),
            "http://127.0.0.1:9".into(),
            Arc::new(Router::new(store, vec![])),
            BTreeMap::from([("Coder".to_string(), profile)]),
        );
        (dir, app, pi, repo.id)
    }

    fn wait_until_ended(app: &App, run_id: i64) -> Run {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let run = app.store.run(run_id).unwrap().unwrap();
            if run.ended_at.is_some() {
                return run;
            }
            assert!(
                Instant::now() < deadline,
                "the run did not end: {run:?} {:?} {:?}",
                std::fs::read_to_string(app.data_dir.join(format!("runs/{run_id}/stderr.log"))),
                app.store
                    .events(run_id)
                    .unwrap()
                    .iter()
                    .map(|e| &e.kind)
                    .collect::<Vec<_>>()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_run_is_stored_event_by_event_and_its_work_committed() {
        let (_dir, app, pi, repo) = setup();
        let card = app
            .create_issue(repo, "Write done.txt", "", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        assert_eq!(run.status, "running");
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "succeeded");
        // The summary is the text of the last assistant message that agent_end reports.
        assert_eq!(run.summary.as_deref(), Some("line one line two"));

        let events = app.store.events(run.id).unwrap();
        let kinds: Vec<_> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "response",
                "agent_start",
                "message_update",
                "tool_execution_start",
                "tool_execution_end",
                "agent_end",
                "agent_settled"
            ]
        );
        // U+2028 inside a string is part of the record, not a line break.
        assert_eq!(
            events[2].data["assistantMessageEvent"]["delta"],
            "line one\u{2028}line two"
        );

        let clone = PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        let branch = format!("moebius/run-{}", run.id);
        assert_eq!(
            git(&clone, &["show", &format!("{branch}:done.txt")]).unwrap(),
            "done"
        );
        assert!(!PathBuf::from(run.worktree.unwrap()).exists());

        let run_dir = app.data_dir.join("runs").join(run.id.to_string());
        let argv: Vec<String> =
            serde_json::from_slice(&std::fs::read(run_dir.join("agent/argv.json")).unwrap())
                .unwrap();
        assert!(argv.windows(2).any(|w| w == ["--model", "qwen-local"]));
        assert!(argv.iter().any(|a| a == "--no-approve") && argv.iter().any(|a| a == "--offline"));
        assert!(argv.windows(2).any(|w| w == ["--thinking", "low"]));
        // The bridge is loaded before the agent's own extensions.
        let bridge = run_dir.join("moebius-bridge.ts");
        assert!(
            argv.windows(2)
                .any(|w| w == ["--extension", bridge.to_str().unwrap()])
        );
        assert_eq!(std::fs::read_to_string(&bridge).unwrap(), BRIDGE);
        let env: Value =
            serde_json::from_slice(&std::fs::read(run_dir.join("agent/env.json")).unwrap())
                .unwrap();
        assert_eq!(env["MOEBIUS"], "/usr/local/bin/moebius");
        assert_eq!(env["MOEBIUS_CHECK_URL"], "http://127.0.0.1:9");
        assert!(
            env["MOEBIUS_RUN_TOKEN"]
                .as_str()
                .unwrap()
                .starts_with("moebius-run-")
        );
        assert!(
            argv.windows(2)
                .any(|w| w == ["--append-system-prompt", "Be brief."])
        );
        let models: Value =
            serde_json::from_slice(&std::fs::read(run_dir.join("agent/models.json")).unwrap())
                .unwrap();
        let provider = &models["providers"]["moebius"];
        assert_eq!(provider["baseUrl"], "http://127.0.0.1:9/v1");
        assert!(
            provider["apiKey"]
                .as_str()
                .unwrap()
                .starts_with("moebius-run-")
        );
    }

    #[test]
    fn a_review_run_starts_from_the_code_branch_and_commits_nothing() {
        let (_dir, app, mut pi, repo) = setup();
        pi.agents
            .insert("Reviewer".into(), pi.agents["Coder"].clone());
        let card = app
            .create_issue(repo, "Write done.txt", "", "person")
            .unwrap();
        let code = pi.start(&app, &card, "Coder", "code", None).unwrap();
        let code = wait_until_ended(&app, code.id);

        let review = pi
            .start(&app, &card, "Reviewer", "review", Some("Check the edge."))
            .unwrap();
        let review = wait_until_ended(&app, review.id);
        // Nothing was changed, and that is no failure for a review.
        assert_eq!(review.status, "succeeded");
        assert_eq!(
            review.summary.as_deref(),
            Some("Looks fine.\nVERDICT: approve")
        );
        let clone = PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        assert_eq!(
            git(&clone, &["rev-parse", &review.branch()]).unwrap(),
            git(&clone, &["rev-parse", &code.branch()]).unwrap()
        );
        assert_eq!(app.store.code_branch(card.id).unwrap(), Some(code.branch()));

        let prompt_of = |run: &Run| {
            let file = format!("runs/{}/agent/prompt.txt", run.id);
            std::fs::read_to_string(app.data_dir.join(file)).unwrap()
        };
        let prompt = prompt_of(&review);
        assert!(prompt.contains("Work on issue #1: Write done.txt"));
        assert!(prompt.contains("VERDICT: approve"));
        assert!(prompt.contains("Notes from the last round:\n\nCheck the edge."));
        let prompt = prompt_of(&code);
        assert!(prompt.contains("Moebius commits what you leave"));
        assert!(!prompt.contains("Notes from the last round"));
    }

    #[test]
    fn a_run_can_be_stopped() {
        let (_dir, app, pi, repo) = setup();
        let card = app
            .create_issue(repo, "Please [hang]", "", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        // Wait until Pi has taken the prompt.
        let deadline = Instant::now() + Duration::from_secs(20);
        while app.store.events(run.id).unwrap().len() < 2 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(pi.stop(run.id));
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "stopped");
        let last = app.store.events(run.id).unwrap().pop().unwrap();
        assert_eq!(
            (last.kind.as_str(), &last.data["aborted"]),
            ("agent_settled", &json!(true))
        );
    }

    #[test]
    fn a_run_that_changed_nothing_has_failed() {
        let (_dir, app, pi, repo) = setup();
        let card = app
            .create_issue(repo, "Do [nothing]", "", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "failed");
        let last = app.store.events(run.id).unwrap().pop().unwrap();
        assert_eq!(last.data["message"], "the agent changed nothing");
    }

    #[test]
    fn an_unknown_agent_or_a_missing_program_fails_cleanly() {
        let (_dir, app, mut pi, repo) = setup();
        let card = app.create_issue(repo, "Anything", "", "person").unwrap();
        assert!(pi.start(&app, &card, "Nobody", "code", None).is_err());
        pi.program = PathBuf::from("/nonexistent/pi-bolt");
        assert!(pi.start(&app, &card, "Coder", "code", None).is_err());
        let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        assert_eq!(run.status, "failed");
        assert!(!PathBuf::from(run.worktree.unwrap()).exists());
    }
}
