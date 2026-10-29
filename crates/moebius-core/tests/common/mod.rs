//! What the end-to-end tests against a real `pi-bolt` share: a recorded OpenAI-compatible upstream, a git repository
//! to work on, and a running server with one Coder agent. Each test file uses some of it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::post;
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use serde_json::{Value, json};

fn chunk(delta: Value, finish: Option<&str>) -> String {
    let c = json!({
        "id": "chatcmpl-recorded",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": "fake-coder",
        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }]
    });
    format!("data: {c}\n\n")
}

fn usage(input: i64, output: i64) -> String {
    let c = json!({
        "id": "chatcmpl-recorded",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": "fake-coder",
        "choices": [],
        "usage": { "prompt_tokens": input, "completion_tokens": output, "total_tokens": input + output }
    });
    format!("data: {c}\n\n")
}

/// A recorded answer that calls a tool of Pi, costing `input` + 30 tokens.
pub fn tool_call_answer(id: &str, name: &str, arguments: &Value, input: i64) -> String {
    let mut sse = chunk(
        json!({ "role": "assistant", "content": null, "tool_calls": [{
            "index": 0, "id": id, "type": "function",
            "function": { "name": name, "arguments": "" }
        }]}),
        None,
    );
    sse += &chunk(
        json!({ "tool_calls": [{ "index": 0, "function": { "arguments": arguments.to_string() } }] }),
        None,
    );
    sse += &chunk(json!({}), Some("tool_calls"));
    sse += &usage(input, 30);
    sse + "data: [DONE]\n\n"
}

/// A recorded answer that says `text` and stops, costing `input` + 2 tokens.
pub fn text_answer(text: &str, input: i64) -> String {
    let mut sse = chunk(json!({ "role": "assistant", "content": text }), None);
    sse += &chunk(json!({}), Some("stop"));
    sse += &usage(input, 2);
    sse + "data: [DONE]\n\n"
}

/// The last message of a request to the upstream.
pub fn last_message(body: &Value) -> &Value {
    body["messages"]
        .as_array()
        .and_then(|m| m.last())
        .unwrap_or(&Value::Null)
}

/// Serves `/v1/chat/completions` on a free port, answering each request with `answer(body)` and keeping the requests.
pub async fn recorded_upstream(
    requests: Arc<Mutex<Vec<Value>>>,
    answer: impl Fn(&Value) -> String + Clone + Send + Sync + 'static,
) -> String {
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(move |axum::Json(body): axum::Json<Value>| async move {
            let sse = answer(&body);
            requests.lock().unwrap().push(body);
            ([(header::CONTENT_TYPE, "text/event-stream")], sse).into_response()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}")
}

/// A git repository with one commit, standing in for the forge's remote.
pub fn origin(dir: &Path) -> PathBuf {
    let origin = dir.join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "A demo repository.\n").unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    origin
}

/// A running server whose Coder agent talks to `upstream` through the real `pi-bolt`.
pub struct Started {
    pub server: Arc<Server>,
    pub api: String,
    client: reqwest::Client,
}

pub async fn start(dir: &Path, pi_bolt: &std::ffi::OsStr, upstream: &str) -> Started {
    start_as(dir, pi_bolt, upstream, None).await
}

/// Like `start`, with runs told that `moebius` is the `moebius` executable (`$MOEBIUS`) instead of the running one.
pub async fn start_as(
    dir: &Path,
    pi_bolt: &std::ffi::OsStr,
    upstream: &str,
    moebius: Option<&Path>,
) -> Started {
    let agents = "[agents.Coder]\nmodel = \"fake-coder\"\ntoken_budget = 100000\n";
    start_from(dir, pi_bolt, upstream, agents, moebius).await
}

/// Like `start`, with `agents` (the `[agents.*]` tables) instead of the Coder.
pub async fn start_with_agents(
    dir: &Path,
    pi_bolt: &std::ffi::OsStr,
    upstream: &str,
    agents: &str,
) -> Started {
    start_from(dir, pi_bolt, upstream, agents, None).await
}

/// Like `start`, with the agents from the harness configuration repository at `harness` instead of the Coder.
pub async fn start_harness(
    dir: &Path,
    pi_bolt: &std::ffi::OsStr,
    upstream: &str,
    harness: &Path,
) -> Started {
    let harness = format!("[harness]\nurl = {:?}\n", harness.display().to_string());
    start_from(dir, pi_bolt, upstream, &harness, None).await
}

/// Starts a server with the configuration `more` after the keys every test sets.
async fn start_from(
    dir: &Path,
    pi_bolt: &std::ffi::OsStr,
    upstream: &str,
    more: &str,
    moebius: Option<&Path>,
) -> Started {
    let config_text = format!(
        "[board]\ntick_ms = 100\n[router]\nlisten = \"127.0.0.1:0\"\n\
         [[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"{upstream}\"\n\
         [pi_bolt]\nprogram = {pi_bolt:?}\n{more}",
        pi_bolt = pi_bolt.to_string_lossy()
    );
    let mut config = Config::parse(&config_text, dir, &dir.join("moebius.toml")).unwrap();
    if let Some(moebius) = moebius {
        config.moebius = moebius.to_path_buf();
    }
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", api_listener.local_addr().unwrap());
    let router_listener = tokio::net::TcpListener::bind(config.router_listen)
        .await
        .unwrap();
    let router_addr = router_listener.local_addr().unwrap();
    let server = Arc::new(Server::new(config, router_addr).unwrap());
    tokio::spawn(server.clone().serve(api_listener, router_listener));
    Started {
        server,
        api,
        client: reqwest::Client::new(),
    }
}

impl Started {
    /// POSTs `body` to the desktop API as the server's owner.
    pub async fn post(&self, path: &str, body: Value) -> reqwest::Response {
        self.client
            .post(format!("{}{path}", self.api))
            .bearer_auth(&*self.server.token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    /// Reads `path` from the desktop API as the server's owner.
    pub async fn get(&self, path: &str) -> Value {
        self.client
            .get(format!("{}{path}", self.api))
            .bearer_auth(&*self.server.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    /// Adds the repository at `origin` and one issue to it; returns the repository and the card.
    pub async fn add_issue(&self, origin: &Path, title: &str, body: &str) -> (Value, Value) {
        let repo: Value = self
            .post(
                "/v1/repositories",
                json!({ "name": "demo", "url": origin.to_str().unwrap() }),
            )
            .await
            .json()
            .await
            .unwrap();
        let card: Value = self
            .post(
                &format!("/v1/repositories/{}/issues", repo["id"]),
                json!({ "title": title, "body": body }),
            )
            .await
            .json()
            .await
            .unwrap();
        (repo, card)
    }

    /// Waits for the board to take the card to `column`; panics with the failed run's events if a run fails first.
    pub async fn wait_for_column(&self, card_id: i64, column: &str) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let card = self.server.app.card(card_id).unwrap();
            if card.column == column {
                return;
            }
            let runs = self.server.app.store.runs_for_card(card_id).unwrap();
            if let Some(run) = runs.iter().find(|r| r.status == "failed") {
                let events = self.server.app.store.events(run.id).unwrap();
                let stderr = std::fs::read_to_string(
                    self.server
                        .config
                        .data_dir
                        .join(format!("runs/{}/stderr.log", run.id)),
                )
                .unwrap_or_default();
                panic!("the run failed: {events:#?}\nstderr: {stderr}");
            }
            assert!(
                Instant::now() < deadline,
                "the card is still in {:?}; runs: {runs:?}",
                card.column
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
