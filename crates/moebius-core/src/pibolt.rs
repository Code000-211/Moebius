//! The Pi-Bolt harness (spec 4): each run is `pi-bolt --mode rpc` in the run's worktree, with the router as its only
//! provider. The prompt goes in as one JSON line; every record Pi writes is stored as a run event and published on the
//! bus. Records are split on LF only, never on U+2028 or U+2029, which are valid inside JSON strings. The run ends at
//! `agent_settled`; what the agent changed is committed on the run's branch and the worktree is removed. A run is
//! stopped with `abort`.

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
use crate::repos::{Worktree, git};
use crate::router::{Grant, Protocol, Router};
use crate::store::{Card, Run};

/// How an agent runs on Pi-Bolt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    /// The model's id at the router's upstream.
    pub model: String,
    /// Pi's thinking level: off, minimal, low, medium, high, xhigh or max.
    pub thinking: Option<String>,
    /// Instructions appended to Pi's system prompt.
    pub instructions: Option<String>,
    pub skills: Vec<PathBuf>,
    pub extensions: Vec<PathBuf>,
    /// The protocol Pi speaks to the router.
    pub protocol: Protocol,
    /// Tokens the card may use; none for no limit.
    pub token_budget: Option<i64>,
}

/// Starts and stops Pi-Bolt runs.
pub struct PiBolt {
    /// The `pi-bolt` executable.
    pub program: PathBuf,
    /// The router's base URL, such as `http://127.0.0.1:7172`.
    pub router_url: String,
    pub router: Arc<Router>,
    pub agents: BTreeMap<String, AgentProfile>,
    live: Arc<Mutex<HashMap<i64, Live>>>,
}

/// A run that has not ended yet.
struct Live {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Child>>,
}

impl PiBolt {
    pub fn new(
        program: PathBuf,
        router_url: String,
        router: Arc<Router>,
        agents: BTreeMap<String, AgentProfile>,
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
    /// `grace` is killed.
    pub fn stop(&self, run_id: i64, grace: Duration) -> bool {
        let Some((stdin, child)) = self
            .live
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&run_id)
            .map(|l| (l.stdin.clone(), l.child.clone()))
        else {
            return false;
        };
        send(&stdin, &json!({ "id": "moebius-abort", "type": "abort" }));
        std::thread::spawn(move || {
            let deadline = Instant::now() + grace;
            while Instant::now() < deadline {
                let exited = child.lock().map(|mut c| c.try_wait());
                if !matches!(exited, Ok(Ok(None))) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = child.lock().map(|mut c| c.kill());
        });
        true
    }

    /// Whether the run is still going.
    pub fn is_live(&self, run_id: i64) -> bool {
        self.live
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains_key(&run_id)
    }

    /// Writes the per-run agent directory: `models.json` with the router as the `moebius` provider.
    fn agent_dir(
        &self,
        dir: &Path,
        profile: &AgentProfile,
        token: &str,
    ) -> std::io::Result<PathBuf> {
        let agent_dir = dir.join("agent");
        std::fs::create_dir_all(&agent_dir)?;
        let (api, base_url) = match profile.protocol {
            Protocol::OpenAi => ("openai-completions", format!("{}/v1", self.router_url)),
            Protocol::Anthropic => ("anthropic-messages", self.router_url.clone()),
        };
        let models = json!({
            "providers": {
                "moebius": {
                    "baseUrl": base_url,
                    "api": api,
                    "apiKey": token,
                    "models": [{ "id": profile.model }]
                }
            }
        });
        std::fs::write(
            agent_dir.join("models.json"),
            serde_json::to_vec_pretty(&models)?,
        )?;
        Ok(agent_dir)
    }
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

fn send(stdin: &Mutex<Option<ChildStdin>>, value: &Value) -> bool {
    let mut guard = stdin.lock().unwrap_or_else(|p| p.into_inner());
    let Some(pipe) = guard.as_mut() else {
        return false;
    };
    let mut line = value.to_string();
    line.push('\n');
    pipe.write_all(line.as_bytes())
        .and_then(|_| pipe.flush())
        .is_ok()
}

impl Runs for PiBolt {
    fn start(&self, app: &App, card: &Card, agent: &str, purpose: &str) -> Result<Run, String> {
        let profile = self
            .agents
            .get(agent)
            .cloned()
            .ok_or_else(|| format!("no agent named {agent}"))?;
        let store = app.store.clone();
        let repository = store
            .repository(card.repository_id)
            .map_err(|e| e.to_string())?
            .ok_or("the card's repository is gone")?;
        let run = store
            .add_run(Some(card.id), agent, "pi-bolt", purpose)
            .map_err(|e| e.to_string())?;
        let fail = |message: String| {
            let _ = store.append_event(run.id, "moebius_error", &json!({ "message": message }));
            let _ = store.set_run_status(run.id, "failed");
            message
        };

        let clone = PathBuf::from(&repository.path);
        let worktree = app
            .repos
            .add_worktree(&repository.name, &clone, run.id, &repository.default_branch)
            .map_err(|e| fail(e.to_string()))?;
        store
            .set_run_worktree(run.id, &worktree.path.to_string_lossy())
            .map_err(|e| fail(e.to_string()))?;
        let token = self.router.issue_token(Grant {
            run_id: run.id,
            card_id: Some(card.id),
            budget: profile.token_budget,
        });
        let run_dir = app.data_dir.join("runs").join(run.id.to_string());
        let agent_dir = self
            .agent_dir(&run_dir, &profile, &token)
            .map_err(|e| fail(format!("cannot write the agent directory: {e}")))?;

        let mut command = Command::new(&self.program);
        command
            .current_dir(&worktree.path)
            .env("PI_CODING_AGENT_DIR", &agent_dir)
            .args([
                "--mode",
                "rpc",
                "--provider",
                "moebius",
                "--model",
                &profile.model,
            ])
            .arg("--session-dir")
            .arg(run_dir.join("sessions"));
        if let Some(thinking) = &profile.thinking {
            command.args(["--thinking", thinking]);
        }
        if let Some(instructions) = &profile.instructions {
            command.arg("--append-system-prompt").arg(instructions);
        }
        for skill in &profile.skills {
            command.arg("--skill").arg(skill);
        }
        for extension in &profile.extensions {
            command.arg("--extension").arg(extension);
        }
        let stderr =
            std::fs::File::create(run_dir.join("stderr.log")).map_err(|e| fail(e.to_string()))?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr));
        let mut child = command
            .spawn()
            .map_err(|e| fail(format!("cannot start {}: {e}", self.program.display())))?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let stdin = Arc::new(Mutex::new(child.stdin.take()));
        let child = Arc::new(Mutex::new(child));
        self.live.lock().unwrap_or_else(|p| p.into_inner()).insert(
            run.id,
            Live {
                stdin: stdin.clone(),
                child: child.clone(),
            },
        );

        store
            .set_run_status(run.id, "running")
            .map_err(|e| fail(e.to_string()))?;
        app.bus.publish(AppEvent::new(
            "run_started",
            json!({ "run": run.id, "card": card.id }),
        ));
        send(
            &stdin,
            &json!({ "id": "moebius-prompt", "type": "prompt", "message": prompt_for(card) }),
        );

        let reader = Reader {
            run_id: run.id,
            card: card.clone(),
            app_store: store.clone(),
            bus: app.bus.clone(),
            repos: app.repos.clone(),
            clone,
            worktree,
            stdin,
            child,
            token,
            router: self.router.clone(),
            live: self.live.clone(),
        };
        std::thread::Builder::new()
            .name(format!("pi-bolt run {}", run.id))
            .spawn(move || reader.read(stdout))
            .map_err(|e| fail(e.to_string()))?;
        Ok(store
            .run(run.id)
            .map_err(|e| e.to_string())?
            .expect("the run exists"))
    }
}

/// Reads one run's records and ends the run.
struct Reader {
    run_id: i64,
    card: Card,
    app_store: Arc<crate::store::Store>,
    bus: crate::bus::Bus,
    repos: crate::repos::Repos,
    clone: PathBuf,
    worktree: Worktree,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Child>>,
    token: String,
    router: Arc<Router>,
    live: Arc<Mutex<HashMap<i64, Live>>>,
}

impl Reader {
    fn read(self, stdout: impl Read) {
        let mut lines = BufReader::new(stdout);
        let mut record = Vec::new();
        let mut status = "failed";
        let mut why = "pi-bolt exited before it settled".to_string();
        loop {
            record.clear();
            match lines.read_until(b'\n', &mut record) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => {
                    why = format!("cannot read from pi-bolt: {e}");
                    break;
                }
            }
            while matches!(record.last(), Some(b'\n' | b'\r')) {
                record.pop();
            }
            if record.is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_slice(&record) {
                Ok(v) => v,
                Err(e) => {
                    json!({ "type": "unparsed", "error": e.to_string(), "text": String::from_utf8_lossy(&record) })
                }
            };
            let kind = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let _ = self.app_store.append_event(self.run_id, &kind, &value);
            self.bus.publish(AppEvent::new(
                "run_event",
                json!({ "run": self.run_id, "kind": kind, "data": value }),
            ));
            if kind == "response"
                && value.get("command").and_then(Value::as_str) == Some("prompt")
                && value.get("success") == Some(&Value::Bool(false))
            {
                why = format!("pi-bolt refused the prompt: {}", value["error"]);
                break;
            }
            if kind == "agent_settled" {
                let aborted = value
                    .get("aborted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                status = if aborted { "stopped" } else { "succeeded" };
                break;
            }
        }
        self.end(status, &why);
    }

    fn end(self, mut status: &str, why: &str) {
        // Closing stdin asks Pi to shut down.
        self.stdin.lock().unwrap_or_else(|p| p.into_inner()).take();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let exited = self.child.lock().map(|mut c| c.try_wait());
            match exited {
                Ok(Ok(None)) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Ok(Ok(None)) => {
                    if let Ok(mut c) = self.child.lock() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                    break;
                }
                _ => break,
            }
        }
        self.router.revoke_token(&self.token);
        if status == "succeeded" {
            if let Err(e) = commit(&self.worktree.path, &self.card) {
                let _ = self.app_store.append_event(
                    self.run_id,
                    "moebius_error",
                    &json!({ "message": e.to_string() }),
                );
                status = "failed";
            }
        } else if status == "failed" {
            let _ = self.app_store.append_event(
                self.run_id,
                "moebius_error",
                &json!({ "message": why }),
            );
        }
        let _ = self.repos.remove_worktree(&self.clone, &self.worktree);
        let _ = self.app_store.set_run_status(self.run_id, status);
        self.live
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.run_id);
        self.bus.publish(AppEvent::new(
            "run_ended",
            json!({ "run": self.run_id, "status": status }),
        ));
    }
}

/// Commits what the agent left in the worktree, if anything.
fn commit(worktree: &Path, card: &Card) -> crate::repos::Result<()> {
    git(worktree, &["add", "--all"])?;
    if git(worktree, &["status", "--porcelain"])?.is_empty() {
        return Ok(());
    }
    let message = format!(
        "#{}: {}\n\nCommitted by Moebius for its run.",
        card.number, card.title
    );
    git(worktree, &["commit", "--quiet", "-m", &message]).map(drop)
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
        let profile = AgentProfile {
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
        assert!(!pi.is_live(run.id));

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
    fn an_unknown_agent_or_a_missing_program_fails_cleanly() {
        let (_dir, app, mut pi, repo) = setup();
        let card = app.create_issue(repo, "Anything", "", "person").unwrap();
        assert!(pi.start(&app, &card, "Nobody", "code").is_err());
        pi.program = PathBuf::from("/nonexistent/pi-bolt");
        assert!(pi.start(&app, &card, "Coder", "code").is_err());
        let runs = app.store.runs_for_card(card.id).unwrap();
        assert_eq!(runs.last().unwrap().status, "failed");
    }
}
