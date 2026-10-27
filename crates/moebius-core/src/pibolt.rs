//! The Pi-Bolt harness (spec 4): each run is `pi-bolt --mode rpc` in the run's worktree, with the router as its only
//! provider. The prompt goes in as one JSON line; every record Pi writes is stored as a run event and published on the
//! bus. Records are split on LF only, never on U+2028 or U+2029, which are valid inside JSON strings. The run ends at
//! `agent_settled`; what the agent changed is committed on the run's branch and the worktree is removed. A run that
//! changed nothing has failed. A run is stopped with `abort`. Pi runs offline and does not trust configuration found
//! in the worktree.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::app::App;
use crate::board::Runs;
use crate::bus::AppEvent;
use crate::config::AgentConfig;
use crate::repos::{Worktree, git};
use crate::router::{Grant, Protocol, Router};
use crate::store::{Card, Run};

/// Starts and stops Pi-Bolt runs.
pub struct PiBolt {
    /// The `pi-bolt` executable.
    pub program: PathBuf,
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
}

impl PiBolt {
    pub fn new(
        program: PathBuf,
        router_url: String,
        router: Arc<Router>,
        agents: BTreeMap<String, AgentConfig>,
    ) -> Self {
        Self {
            program,
            router_url,
            router,
            agents,
            live: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Asks a run to stop; it ends with status `stopped` once Pi has settled. A run that does not settle within
    /// `grace` is killed. Returns false when the run is not going.
    pub fn stop(&self, run_id: i64, grace: Duration) -> bool {
        let Some(live) = lock(&self.live).get(&run_id).cloned() else {
            return false;
        };
        send(
            &live.stdin,
            &json!({ "id": "moebius-abort", "type": "abort" }),
        );
        std::thread::spawn(move || {
            if !wait_for_exit(&live.child, grace) {
                let _ = lock(&live.child).kill();
            }
        });
        true
    }

    /// The command that starts Pi for one run.
    fn command(
        &self,
        agent: &AgentConfig,
        worktree: &Path,
        run_dir: &Path,
        agent_dir: &Path,
    ) -> Command {
        let mut command = Command::new(&self.program);
        command
            .current_dir(worktree)
            .env("PI_CODING_AGENT_DIR", agent_dir)
            .args(["--mode", "rpc", "--offline", "--no-approve"])
            .args(["--provider", "moebius", "--model", &agent.model])
            .arg("--session-dir")
            .arg(run_dir.join("sessions"));
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
        card: &Card,
        agent: &AgentConfig,
        run: RunningRun,
    ) -> Result<(), String> {
        app.store
            .set_run_worktree(run.id, &run.worktree.path.to_string_lossy())
            .map_err(|e| e.to_string())?;
        let run_dir = app.data_dir.join("runs").join(run.id.to_string());
        let agent_dir = self
            .write_agent_dir(&run_dir, agent, &run.token)
            .map_err(|e| format!("cannot write the agent directory: {e}"))?;
        let stderr =
            std::fs::File::create(run_dir.join("stderr.log")).map_err(|e| e.to_string())?;
        let mut child = self
            .command(agent, &run.worktree.path, &run_dir, &agent_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", self.program.display()))?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let live = Live {
            stdin: Arc::new(Mutex::new(child.stdin.take())),
            child: Arc::new(Mutex::new(child)),
        };
        lock(&self.live).insert(run.id, live.clone());
        app.store
            .set_run_status(run.id, "running")
            .map_err(|e| e.to_string())?;
        app.bus.publish(AppEvent::new(
            "run_started",
            json!({ "run": run.id, "card": card.id }),
        ));
        send(
            &live.stdin,
            &json!({ "id": "moebius-prompt", "type": "prompt", "message": prompt_for(card) }),
        );
        let reader = Reader {
            app: app.clone(),
            card: card.clone(),
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
}

impl Runs for PiBolt {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
    ) -> Result<Run, String> {
        let config = self
            .agents
            .get(agent)
            .ok_or_else(|| format!("no agent named {agent}"))?;
        let repository = app
            .store
            .repository(card.repository_id)
            .map_err(|e| e.to_string())?
            .ok_or("the card's repository is gone")?;
        let run = app
            .store
            .add_run(Some(card.id), agent, "pi-bolt", purpose)
            .map_err(|e| e.to_string())?;
        let fail = |message: String| {
            let _ = app
                .store
                .append_event(run.id, "moebius_error", &json!({ "message": message }));
            let _ = app.store.set_run_status(run.id, "failed");
            message
        };

        let clone = PathBuf::from(&repository.path);
        let worktree = app
            .repos
            .add_worktree(&repository.name, &clone, run.id, &repository.base_ref())
            .map_err(|e| fail(e.to_string()))?;
        let token = self.router.issue_token(Grant {
            run_id: run.id,
            card_id: Some(card.id),
            budget: config.token_budget,
        });
        let running = RunningRun {
            id: run.id,
            clone,
            worktree,
            token,
        };
        if let Err(e) = self.launch(app, card, config, running.clone()) {
            // Nothing reads this run: undo what was set up for it.
            self.router.revoke_token(&running.token);
            let _ = app.repos.remove_worktree(&running.clone, &running.worktree);
            return Err(fail(e));
        }
        Ok(app
            .store
            .run(run.id)
            .map_err(|e| e.to_string())?
            .expect("the run exists"))
    }
}

/// What a started run holds until it ends.
#[derive(Clone)]
struct RunningRun {
    id: i64,
    clone: PathBuf,
    worktree: Worktree,
    token: String,
}

/// The prompt for a card.
fn prompt_for(card: &Card) -> String {
    let mut prompt = format!("Work on issue #{}: {}\n", card.number, card.title);
    if !card.body.trim().is_empty() {
        prompt.push('\n');
        prompt.push_str(&card.body);
        prompt.push('\n');
    }
    prompt.push_str(
        "\nYou are in a git worktree on the issue's own branch. Make the change, run what checks the repository has, \
         and stop when the issue is done. Moebius commits what you leave in the worktree.\n",
    );
    prompt
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
    card: Card,
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
                Ok(0) => break,
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
            let _ = self.app.store.append_event(self.run.id, &kind, &value);
            self.app.bus.publish(AppEvent::new(
                "run_event",
                json!({ "run": self.run.id, "kind": kind, "data": value }),
            ));
            if kind == "response" && value["command"] == "prompt" && value["success"] == false {
                ending = Err(format!("pi-bolt refused the prompt: {}", value["error"]));
                break;
            }
            if kind == "agent_settled" {
                ending = Ok(value["aborted"] == true);
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
            Ok(true) => "stopped",
            Ok(false) => match commit(&self.run.worktree.path, &self.card) {
                Ok(true) => "succeeded",
                Ok(false) => self.failed("the agent changed nothing"),
                Err(e) => self.failed(&e.to_string()),
            },
            Err(why) => self.failed(&why),
        };
        let _ = self
            .app
            .repos
            .remove_worktree(&self.run.clone, &self.run.worktree);
        let _ = self.app.store.set_run_status(self.run.id, status);
        lock(&self.lives).remove(&self.run.id);
        self.app.bus.publish(AppEvent::new(
            "run_ended",
            json!({ "run": self.run.id, "status": status }),
        ));
    }

    fn failed(&self, why: &str) -> &'static str {
        let _ =
            self.app
                .store
                .append_event(self.run.id, "moebius_error", &json!({ "message": why }));
        "failed"
    }
}

/// Commits what the agent left in the worktree; false when it left nothing.
fn commit(worktree: &Path, card: &Card) -> crate::repos::Result<bool> {
    git(worktree, &["add", "--all"])?;
    if git(worktree, &["status", "--porcelain"])?.is_empty() {
        return Ok(false);
    }
    let message = format!(
        "#{}: {}\n\nCommitted by Moebius for its run.",
        card.number, card.title
    );
    git(worktree, &["commit", "--quiet", "-m", &message])?;
    Ok(true)
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
        let run = pi.start(&app, &card, "Coder", "code").unwrap();
        assert_eq!(run.status, "running");
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "succeeded");

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
    fn a_run_can_be_stopped() {
        let (_dir, app, pi, repo) = setup();
        let card = app
            .create_issue(repo, "Please [hang]", "", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code").unwrap();
        // Wait until Pi has taken the prompt.
        let deadline = Instant::now() + Duration::from_secs(20);
        while app.store.events(run.id).unwrap().len() < 2 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(pi.stop(run.id, Duration::from_secs(10)));
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
        let run = pi.start(&app, &card, "Coder", "code").unwrap();
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "failed");
        let last = app.store.events(run.id).unwrap().pop().unwrap();
        assert_eq!(last.data["message"], "the agent changed nothing");
    }

    #[test]
    fn an_unknown_agent_or_a_missing_program_fails_cleanly() {
        let (_dir, app, mut pi, repo) = setup();
        let card = app.create_issue(repo, "Anything", "", "person").unwrap();
        assert!(pi.start(&app, &card, "Nobody", "code").is_err());
        pi.program = PathBuf::from("/nonexistent/pi-bolt");
        assert!(pi.start(&app, &card, "Coder", "code").is_err());
        let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        assert_eq!(run.status, "failed");
        assert!(!PathBuf::from(run.worktree.unwrap()).exists());
    }
}
