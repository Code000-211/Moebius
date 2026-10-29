//! What the tests of the API's parts share: a server on port 0 over a fake Pi, requests with its token, and waiting
//! for what the harness does.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::{ApiState, load_or_create_token, serve};
use crate::app::App;
use crate::config::{AgentConfig, Config};
use crate::harness::pibolt::PiBolt;
use crate::repos::git;
use crate::router::{Protocol, Router};
use crate::server::settings::Live;
use crate::store::{Card, Change, Repository, Run, Store};

pub(super) async fn start() -> (tempfile::TempDir, ApiState, String) {
    start_with(BTreeMap::from([("Coder".to_string(), agent("fake"))])).await
}

pub(super) fn agent(model: &str) -> AgentConfig {
    AgentConfig {
        model: model.into(),
        thinking: None,
        instructions: None,
        skills: vec![],
        extensions: vec![],
        mcp: BTreeMap::new(),
        protocol: Protocol::OpenAi,
        token_budget: None,
        harness: Default::default(),
        tool_output: Default::default(),
    }
}

/// A server whose harness has the agents `agents`.
pub(super) async fn start_with(
    agents: BTreeMap<String, AgentConfig>,
) -> (tempfile::TempDir, ApiState, String) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
    let repo = store
        .add_repository("radiance", "/srv/radiance", "none", "main")
        .unwrap();
    store
        .add_card(repo.id, "First", "", "leon", "Backlog")
        .unwrap();
    let token = load_or_create_token(&dir.path().join("token")).unwrap();
    let host = Arc::new(moebius_plugin_api::Host::new(
        std::time::Duration::from_secs(1),
    ));
    // The harness is the real one over a Pi that is a script.
    let router = Arc::new(Router::new(store.clone(), vec![]).with_secrets(
        crate::server::settings::Secrets::new(&dir.path().join(".moebius")),
    ));
    let pi = PiBolt::new(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi"),
        "/usr/local/bin/moebius".into(),
        "http://127.0.0.1:9".into(),
        router.clone(),
        agents,
    );
    let state = ApiState {
        app: Arc::new(App::new(store, host, &dir.path().join("data"))),
        runs: Arc::new(pi),
        harness_repo: None,
        token: token.into(),
        router,
        config: Arc::new(Live::new(
            Config::parse(
                "[budgets]\nday_tokens = 10000\n",
                dir.path(),
                &dir.path().join("moebius.toml"),
            )
            .unwrap(),
        )),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, state.clone()));
    (dir, state, format!("127.0.0.1:{}", addr.port()))
}

/// A git repository with one commit.
pub(super) fn origin(dir: &std::path::Path) -> std::path::PathBuf {
    let origin = dir.join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "hello\n").unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    origin
}

/// What a Coder run does, by hand: a worktree, a commit, the change offered.
pub(super) fn code_and_offer(app: &App, repo: &Repository, card: &Card) -> Change {
    let clone = std::path::PathBuf::from(&repo.path);
    let run = app
        .store
        .add_run(Some(card.id), "Coder", "test", "code")
        .unwrap();
    app.store.set_run_status(run.id, "succeeded").unwrap();
    let wt = app
        .repos
        .add_worktree(&repo.name, &clone, run.id, "main")
        .unwrap();
    std::fs::write(wt.path.join("feature.txt"), "done\n").unwrap();
    git(&wt.path, &["add", "."]).unwrap();
    git(&wt.path, &["commit", "--quiet", "-m", "Add the feature"]).unwrap();
    app.repos.remove_worktree(&clone, &wt.path).unwrap();
    let change = app.offer_change(card.id).unwrap();
    assert_eq!(app.card(card.id).unwrap().column, "Ready to merge");
    change
}

pub(super) async fn post(
    state: &ApiState,
    addr: &str,
    path: String,
    body: serde_json::Value,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("http://{addr}{path}"))
        .bearer_auth(&*state.token)
        .json(&body)
        .send()
        .await
        .unwrap()
}

/// Waits until `done` holds.
pub(super) async fn until(what: &str, done: impl Fn() -> bool) {
    let give_up = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !done() {
        assert!(std::time::Instant::now() < give_up, "never: {what}");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

pub(super) async fn get<T: serde::de::DeserializeOwned>(
    state: &ApiState,
    addr: &str,
    path: &str,
) -> T {
    let response = reqwest::Client::new()
        .get(format!("http://{addr}{path}"))
        .bearer_auth(&*state.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "{path}");
    response.json().await.unwrap()
}

/// The kinds of the events of a run, in order.
pub(super) fn kinds(app: &App, run_id: i64) -> Vec<String> {
    let events = app.store.events(run_id).unwrap();
    events.into_iter().map(|e| e.kind).collect()
}

pub(super) fn status(app: &App, run_id: i64) -> String {
    app.store.run(run_id).unwrap().unwrap().status
}

/// Starts a chat with the Coder; returns its run once the agent has answered the first message.
pub(super) async fn chat(
    state: &ApiState,
    addr: &str,
    message: &str,
    repository: Option<i64>,
) -> Run {
    let started = post(
        state,
        addr,
        "/v1/chats".into(),
        serde_json::json!({ "agent": "Coder", "message": message, "repository": repository }),
    )
    .await;
    assert_eq!(started.status(), 200);
    let run: Run = started.json().await.unwrap();
    assert_eq!((run.purpose.as_str(), run.card_id), ("chat", None));
    until("the agent has answered", || {
        status(&state.app, run.id) == "waiting"
    })
    .await;
    run
}

pub(super) async fn say(
    state: &ApiState,
    addr: &str,
    run_id: i64,
    message: &str,
) -> reqwest::Response {
    post(
        state,
        addr,
        format!("/v1/chats/{run_id}/messages"),
        serde_json::json!({ "message": message }),
    )
    .await
}

pub(super) async fn end(state: &ApiState, addr: &str, run_id: i64) -> reqwest::Response {
    post(
        state,
        addr,
        format!("/v1/chats/{run_id}/end"),
        serde_json::json!({}),
    )
    .await
}
