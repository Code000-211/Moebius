//! `GET /v1/start` tells step by step whether the server is ready for a first card, in words and never with a secret.

use std::path::Path;

use moebius_core::config::Config;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use serde_json::Value;

/// A server for `toml`, whose data and relative paths are in `dir`, and the address and token to ask it.
async fn serve(dir: &Path, toml: &str) -> (Server, String, String) {
    let file = dir.join("moebius.toml");
    std::fs::write(&file, toml).unwrap();
    let config = Config::parse(toml, dir, &file).unwrap();
    let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
    let state = ApiState {
        app: server.app.clone(),
        runs: server.harness.clone(),
        harness_repo: None,
        token: server.token.clone(),
        router: server.router.clone(),
        config: server.live.clone(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(api::serve(listener, state));
    let token = server.token.to_string();
    (server, base, token)
}

async fn start(base: &str, token: &str) -> (Value, String) {
    let response = reqwest::Client::new()
        .get(format!("{base}/v1/start"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let text = response.text().await.unwrap();
    (serde_json::from_str(&text).unwrap(), text)
}

/// An executable file `name` in `dir` with `script` as its content.
#[cfg(unix)]
fn program(dir: &Path, name: &str, script: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

/// An upstream that lists models, or refuses with 401, and its address.
#[cfg(unix)]
async fn upstream(models: bool) -> String {
    use axum::response::IntoResponse;
    use serde_json::json;
    let app = axum::Router::new().route(
        "/models",
        axum::routing::get(move || async move {
            if models {
                axum::Json(json!({"data": [{"id": "qwen"}]})).into_response()
            } else {
                (axum::http::StatusCode::UNAUTHORIZED, "no").into_response()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    url
}

fn steps(answer: &Value) -> Vec<(&str, bool, &str)> {
    answer["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["key"].as_str().unwrap(),
                s["done"].as_bool().unwrap(),
                s["detail"].as_str().unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_server_with_nothing_set_up_says_what_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-pi-bolt");
    let toml = format!("[pi_bolt]\nprogram = {:?}\n", missing.display().to_string());
    let (_server, base, token) = serve(dir.path(), &toml).await;
    let (answer, _) = start(&base, &token).await;
    assert_eq!(answer["done"], false);
    let steps = steps(&answer);
    let keys: Vec<_> = steps.iter().map(|s| s.0).collect();
    assert_eq!(
        keys,
        ["upstream", "pi_bolt", "harness", "coder", "repository"]
    );
    assert!(steps.iter().all(|s| !s.1));
    assert_eq!(steps[0].2, "Add an upstream on the Models page.");
    assert!(steps[1].2.contains("not found"), "{}", steps[1].2);
    assert!(steps[1].2.contains("no-such-pi-bolt"));
    assert!(
        steps[2].2.contains("moves the agents and rules"),
        "{}",
        steps[2].2
    );
    assert!(steps[3].2.contains("Agents page"));
    assert_eq!(steps[4].2, "No repository is added yet.");
}

// Unix only: the fake `pi-bolt` is a shell script.
#[cfg(unix)]
#[tokio::test]
async fn an_upstream_that_refuses_and_a_pi_bolt_that_fails_are_told() {
    let dir = tempfile::tempdir().unwrap();
    let failing = program(dir.path(), "pi-bolt", "#!/bin/sh\nexit 3\n");
    let refusing = upstream(false).await;
    let toml = format!(
        "[pi_bolt]\nprogram = \"{failing}\"\n[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"{refusing}\"\napi_key_env = \"PATH\"\n"
    );
    let (_server, base, token) = serve(dir.path(), &toml).await;
    let (answer, text) = start(&base, &token).await;
    let steps = steps(&answer);
    assert!(!steps[0].1);
    assert_eq!(steps[0].2, format!("Could not ask {refusing}: 401."));
    assert!(!steps[1].1);
    assert!(steps[1].2.contains("failed"), "{}", steps[1].2);
    // The key the upstream is asked with is the value of PATH here.
    assert!(!text.contains(&std::env::var("PATH").unwrap()));
}

// Unix only: the fake `pi-bolt` is a shell script.
#[cfg(unix)]
#[tokio::test]
async fn a_server_with_everything_in_place_is_done() {
    let dir = tempfile::tempdir().unwrap();
    let pi_bolt = program(dir.path(), "pi-bolt", "#!/bin/sh\necho 1.0\n");
    let listing = upstream(true).await;
    // The Coder is in a harness configuration repository, which the step names.
    let harness = dir.path().join("harness");
    let coder = Config::parse("[agents.Coder]\nmodel = \"qwen\"\n", dir.path(), dir.path())
        .unwrap()
        .agents;
    moebius_core::harness::repo::create(&harness, &coder, None).unwrap();
    let toml = format!(
        "[pi_bolt]\nprogram = \"{pi_bolt}\"\n[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"{listing}\"\n[harness]\nurl = {:?}\n",
        harness.display().to_string()
    );
    let (server, base, token) = serve(dir.path(), &toml).await;
    let (answer, _) = start(&base, &token).await;
    assert_eq!(answer["done"], false);
    let before = steps(&answer);
    assert_eq!(
        before.iter().map(|s| s.1).collect::<Vec<_>>(),
        [true, true, true, true, false]
    );
    assert_eq!(before[0].2, "1 model is available.");
    assert_eq!(
        before[2].2,
        format!(
            "Agents, skills, MCP servers and rules come from {}.",
            harness.display()
        )
    );

    server
        .app
        .store
        .add_repository("radiance", "/srv/radiance", "none", "main")
        .unwrap();
    let (answer, _) = start(&base, &token).await;
    assert_eq!(answer["done"], true);
    assert!(steps(&answer).iter().all(|s| s.1));
}
