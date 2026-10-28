//! The Retro: a day's errors become a report, and the Retro analyst, a fake Pi here, turns the report into a proposal
//! branch that is pushed to the harness configuration repository's origin and listed by the API.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use moebius_core::api::{self, ApiState};
use moebius_core::bus::AppEvent;
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use serde_json::{Value, json};

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A harness configuration repository with the Coder; returns the bare repository the server clones.
fn harness_origin(dir: &Path) -> PathBuf {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&work, "agents/Coder.toml", "model = \"coder-model\"\n");
    write(&work, "agents/Coder.md", "Keep changes small.\n");
    git(&work, &["add", "-A"]).unwrap();
    git(&work, &["commit", "--quiet", "-m", "Start"]).unwrap();
    let bare = dir.join("harness.git");
    git(
        dir,
        &["init", "--quiet", "--bare", "-b", "main", "harness.git"],
    )
    .unwrap();
    git(&work, &["push", "--quiet", bare.to_str().unwrap(), "main"]).unwrap();
    bare
}

/// A server with the fake Pi, with the harness configuration repository `harness` if there is one.
fn server(dir: &Path, harness: Option<&Path>) -> Arc<Server> {
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi");
    let mut text = format!("[pi_bolt]\nprogram = {:?}\n", fake_pi.display().to_string());
    if let Some(harness) = harness {
        text += &format!("[harness]\nurl = {:?}\n", harness.display().to_string());
    } else {
        text += "[agents.Coder]\nmodel = \"coder-model\"\n";
    }
    let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
    Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap())
}

/// A failed run whose error says `error`, and a refused tool call of another run.
fn a_bad_day(server: &Server, error: &str) {
    let store = &server.app.store;
    let failed = store.add_run(None, "Coder", "pi-bolt", "code").unwrap();
    store
        .append_event(failed.id, "moebius_error", &json!({ "message": error }))
        .unwrap();
    store.set_run_status(failed.id, "failed").unwrap();
    let refused = store.add_run(None, "Coder", "pi-bolt", "code").unwrap();
    let check = json!({ "tool": "bash", "tool_call_id": "c1", "decision": "refuse", "reason": "no-destructive-commands: rm -rf" });
    store
        .append_event(refused.id, "tool_check", &check)
        .unwrap();
    store.set_run_status(refused.id, "succeeded").unwrap();
}

struct Api {
    base: String,
    token: String,
    client: reqwest::Client,
}

impl Api {
    async fn start(server: &Server) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            app: server.app.clone(),
            runs: server.harness.clone(),
            harness_repo: server.harness_repo.clone(),
            token: server.token.clone(),
            round_limit: 5,
            day_budget: None,
        };
        tokio::spawn(api::serve(listener, state));
        Self {
            base,
            token: server.token.to_string(),
            client: reqwest::Client::new(),
        }
    }

    async fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }
}

/// Waits for the `retro_done` event of a Retro that was started after `events` subscribed.
async fn done(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> Value {
    let wait = async {
        loop {
            let event = events.recv().await.unwrap();
            if event.kind == "retro_done" {
                return event.data;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(120), wait)
        .await
        .expect("the Retro did not end")
}

#[tokio::test]
async fn a_day_with_errors_gets_a_report_and_the_analyst_proposes_a_branch_that_is_pushed() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), Some(&origin));
    let day = server.app.store.today().unwrap();
    let branch = format!("proposal/{day}-kinder");
    a_bad_day(
        &server,
        &format!("stalled [propose {branch} agents/Coder.md cards=7,9]"),
    );
    let api = Api::start(&server).await;
    let mut events = server.app.bus.subscribe();

    let (status, started) = api
        .call(reqwest::Method::POST, "/v1/retros", Some(json!({})))
        .await;
    assert_eq!(status, 200, "{started}");
    assert_eq!(started["date"], day);
    let kinds: Vec<&str> = started["report"]["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["tool_refused", "run_failed"]);
    let ended = done(&mut events).await;
    assert_eq!(
        ended,
        json!({ "date": day, "status": "proposed", "proposals": 1 })
    );

    let (_, retro) = api
        .call(reqwest::Method::GET, &format!("/v1/retros/{day}"), None)
        .await;
    assert_eq!(retro["status"], "proposed");
    assert_eq!(retro["warnings"], json!([]));
    let run = retro["run_id"].as_i64().unwrap();
    let run = server.app.store.run(run).unwrap().unwrap();
    assert_eq!(
        (run.purpose.as_str(), run.agent.as_str()),
        ("retro", "Retro-analyst")
    );
    assert_eq!(run.card_id, None);
    assert!(!PathBuf::from(run.worktree.unwrap()).exists());

    let (_, proposals) = api.call(reqwest::Method::GET, "/v1/proposals", None).await;
    let proposal = &proposals[0];
    assert_eq!(proposals.as_array().unwrap().len(), 1);
    assert_eq!(proposal["branch"], branch);
    assert_eq!(proposal["title"], "Change agents/Coder.md");
    assert_eq!(proposal["body"], "It addresses the errors.");
    assert_eq!(proposal["agents"], json!(["Coder"]));
    // The `Cards:` line that ends the commit message is the cards, not part of the body.
    assert_eq!(proposal["cards"], json!([7, 9]));
    assert_eq!(proposal["trials"], json!([]));
    assert_eq!(proposal["status"], "open");
    assert_eq!(retro["proposals"], json!([proposal]));

    // The branch is on the origin and changes only the agent's file; the default branch is as it was.
    let changed = git(
        &origin,
        &["diff", "--name-only", &format!("main...{branch}")],
    )
    .unwrap();
    assert_eq!(changed, "agents/Coder.md");

    // The list has no reports.
    let (_, list) = api.call(reqwest::Method::GET, "/v1/retros", None).await;
    assert_eq!(
        list,
        json!([{ "date": day, "status": "proposed", "run_id": retro["run_id"], "errors": 2,
                 "proposals": 1, "at": retro["at"] }])
    );

    // A day has one Retro.
    let (status, _) = api
        .call(
            reqwest::Method::POST,
            "/v1/retros",
            Some(json!({ "date": day })),
        )
        .await;
    assert_eq!(status, 409);
    let (status, _) = api
        .call(
            reqwest::Method::POST,
            "/v1/retros",
            Some(json!({ "date": "yesterday" })),
        )
        .await;
    assert_eq!(status, 422);
    let (status, _) = api
        .call(reqwest::Method::GET, "/v1/retros/2001-01-01", None)
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_branch_that_does_not_change_harness_configuration_is_left_out_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), Some(&origin));
    let day = server.app.store.today().unwrap();
    let branch = format!("proposal/{day}-code");
    a_bad_day(&server, &format!("[propose {branch} src/main.rs]"));
    let mut events = server.app.bus.subscribe();
    let started = Api::start(&server).await;
    started
        .call(reqwest::Method::POST, "/v1/retros", Some(json!({})))
        .await;
    done(&mut events).await;

    let retro = server.app.store.retro(&day).unwrap().unwrap();
    assert_eq!(retro.status, "proposed");
    assert_eq!(retro.warnings.len(), 1, "{:?}", retro.warnings);
    assert!(
        retro.warnings[0].contains("src/main.rs"),
        "{:?}",
        retro.warnings
    );
    assert!(server.app.store.proposals(None).unwrap().is_empty());
    assert!(git(&origin, &["rev-parse", "--verify", "--quiet", &branch]).is_err());
}

#[tokio::test]
async fn a_day_without_errors_makes_no_run() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), Some(&harness_origin(dir.path())));
    let api = Api::start(&server).await;
    let mut events = server.app.bus.subscribe();
    let (status, retro) = api
        .call(reqwest::Method::POST, "/v1/retros", Some(json!({})))
        .await;
    assert_eq!(status, 200);
    assert_eq!(retro["status"], "reported");
    assert_eq!(retro["report"]["errors"], json!([]));
    assert_eq!(retro["run_id"], Value::Null);
    assert_eq!(done(&mut events).await["status"], "reported");
    assert!(server.app.store.active_runs().unwrap().is_empty());
    assert!(server.app.store.chats().unwrap().is_empty());
    assert!(server.app.store.run(1).unwrap().is_none());
}

#[tokio::test]
async fn without_a_harness_repository_only_the_report_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), None);
    a_bad_day(&server, "stalled");
    let api = Api::start(&server).await;
    let (status, retro) = api
        .call(reqwest::Method::POST, "/v1/retros", Some(json!({})))
        .await;
    assert_eq!(status, 200);
    assert_eq!(retro["status"], "reported");
    assert_eq!(retro["run_id"], Value::Null);
    assert_eq!(retro["report"]["errors"].as_array().unwrap().len(), 2);
    assert_eq!(retro["proposals"], json!([]));
    // Only the two runs of the bad day exist.
    assert!(server.app.store.run(3).unwrap().is_none());
}
