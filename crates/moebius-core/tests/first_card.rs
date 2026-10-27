//! The first card through the workflow (issue #12): an issue in a plain git repository is coded by the Coder agent on
//! a real `pi-bolt`, committed on its branch, offered as a change and merged by a person through the desktop API.
//!
//! The model is a recorded OpenAI-compatible upstream: it answers the first request with a call to Pi's `write` tool
//! and the request carrying the tool's result with "Done.". Set `MOEBIUS_PI_BOLT` to the `pi-bolt` executable to run
//! the test; without it the test is skipped.

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

/// The recorded answer to a request: a tool call first, then "Done." once the tool's result is in.
fn answer(body: &Value) -> String {
    let after_tool = body["messages"]
        .as_array()
        .and_then(|m| m.last())
        .is_some_and(|m| m["role"] == "tool");
    let mut sse = String::new();
    if after_tool {
        sse += &chunk(json!({ "role": "assistant", "content": "Done." }), None);
        sse += &chunk(json!({}), Some("stop"));
        sse += &usage(300, 2);
    } else {
        let arguments =
            json!({ "path": "hello.txt", "content": "Hello from Moebius\n" }).to_string();
        sse += &chunk(
            json!({ "role": "assistant", "content": null, "tool_calls": [{
                "index": 0, "id": "call_1", "type": "function",
                "function": { "name": "write", "arguments": "" }
            }]}),
            None,
        );
        sse += &chunk(
            json!({ "tool_calls": [{ "index": 0, "function": { "arguments": arguments } }] }),
            None,
        );
        sse += &chunk(json!({}), Some("tool_calls"));
        sse += &usage(250, 30);
    }
    sse + "data: [DONE]\n\n"
}

async fn recorded_upstream(requests: Arc<Mutex<Vec<Value>>>) -> String {
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

fn origin(dir: &Path) -> PathBuf {
    let origin = dir.join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "A demo repository.\n").unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    origin
}

#[tokio::test(flavor = "multi_thread")]
async fn an_issue_becomes_a_merged_change_through_a_real_pi_bolt() {
    let Some(pi_bolt) = std::env::var_os("MOEBIUS_PI_BOLT") else {
        eprintln!("skipped: set MOEBIUS_PI_BOLT to the pi-bolt executable to run this test");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let origin = origin(dir.path());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let upstream = recorded_upstream(requests.clone()).await;

    let config_text = format!(
        "[board]\ntick_ms = 100\n[router]\nlisten = \"127.0.0.1:0\"\n\
         [[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"{upstream}\"\n\
         [pi_bolt]\nprogram = {pi_bolt:?}\n\
         [agents.Coder]\nmodel = \"fake-coder\"\ntoken_budget = 100000\n",
        pi_bolt = pi_bolt.to_string_lossy()
    );
    let config = Config::parse(&config_text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", api_listener.local_addr().unwrap());
    let router_listener = tokio::net::TcpListener::bind(config.router_listen)
        .await
        .unwrap();
    let router_addr = router_listener.local_addr().unwrap();
    let server = Arc::new(Server::new(config, router_addr).unwrap());
    tokio::spawn(server.clone().serve(api_listener, router_listener));

    let client = reqwest::Client::new();
    let post = |path: String, body: Value| {
        client
            .post(format!("{api}{path}"))
            .bearer_auth(&*server.token)
            .json(&body)
            .send()
    };
    let repo: Value = post(
        "/v1/repositories".into(),
        json!({ "name": "demo", "url": origin.to_str().unwrap() }),
    )
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let card: Value = post(
        format!("/v1/repositories/{}/issues", repo["id"]),
        json!({ "title": "Say hello", "body": "Add hello.txt saying hello." }),
    )
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let card_id = card["id"].as_i64().unwrap();

    // The board takes the card from the backlog through a Coder run to "Ready to merge".
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let card = server.app.card(card_id).unwrap();
        if card.column == "Ready to merge" {
            break;
        }
        let runs = server.app.store.runs_for_card(card_id).unwrap();
        if let Some(run) = runs.iter().find(|r| r.status == "failed") {
            let events = server.app.store.events(run.id).unwrap();
            let stderr = std::fs::read_to_string(
                server
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

    let run = server
        .app
        .store
        .runs_for_card(card_id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        (run.agent.as_str(), run.status.as_str()),
        ("Coder", "succeeded")
    );
    let kinds: Vec<String> = server
        .app
        .store
        .events(run.id)
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert!(
        kinds.contains(&"tool_execution_end".to_string()),
        "{kinds:?}"
    );
    assert_eq!(kinds.last().map(String::as_str), Some("agent_settled"));
    assert!(requests.lock().unwrap().len() >= 2);
    let used: i64 = server
        .app
        .store
        .usage_for_run(run.id)
        .unwrap()
        .iter()
        .map(|u| u.total())
        .sum();
    assert_eq!(used, 250 + 30 + 300 + 2);

    // A person merges the change through the desktop API.
    let change = server
        .app
        .store
        .changes(repo["id"].as_i64().unwrap())
        .unwrap()
        .pop()
        .unwrap();
    let merged = post(
        format!("/v1/changes/{}/merge", change.id),
        json!({ "reason": "ship it" }),
    )
    .await
    .unwrap();
    assert_eq!(merged.status(), 200);
    let clone = PathBuf::from(repo["path"].as_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(clone.join("hello.txt")).unwrap(),
        "Hello from Moebius\n"
    );
    assert_eq!(server.app.card(card_id).unwrap().column, "Done");
    assert!(
        git(&clone, &["log", "--oneline", "-3"])
            .unwrap()
            .contains("Merge #1: Say hello")
    );
}
