//! What the tests against a real server share: a server on a free port (with or without the fake Pi), a bare
//! `origin` to add as a repository, a POST with the server's token, and waiting on the stream or the snapshot.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use iced_test::Simulator;
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_desktop::app::{App, Message};
use moebius_desktop::client::{Client, StreamEvent};
use moebius_desktop::settings::Settings;
use serde_json::json;

/// A server with no agents on a free port; returns the settings that connect to it and the server.
pub async fn start(dir: &Path) -> (Settings, Arc<Server>) {
    start_with(dir, "").await
}

/// Like `start`, with `more` added to `moebius.toml`.
pub async fn start_with(dir: &Path, more: &str) -> (Settings, Arc<Server>) {
    // Keys outside a table come before the first table.
    let (top, tables) = more.split_at(more.find('[').unwrap_or(more.len()));
    let text = format!("{top}[router]\nlisten = \"127.0.0.1:0\"\n{tables}");
    std::fs::write(dir.join("moebius.toml"), &text).unwrap();
    let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", api_listener.local_addr().unwrap());
    let router_listener = tokio::net::TcpListener::bind(config.router_listen)
        .await
        .unwrap();
    let router_addr = router_listener.local_addr().unwrap();
    let server = Arc::new(Server::new(config, router_addr).unwrap());
    let token = server.token.to_string();
    tokio::spawn(server.clone().serve(api_listener, router_listener));
    (Settings { address, token }, server)
}

pub fn origin(dir: &Path) -> std::path::PathBuf {
    let origin = dir.join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "A demo repository.\n").unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    origin
}

/// Adds the repository `demo`, cloned from a new `origin` in `dir`; returns what the server answers.
pub async fn add_demo(settings: &Settings, dir: &Path) -> serde_json::Value {
    let origin = origin(dir);
    post(
        settings,
        "/v1/repositories",
        json!({"name": "demo", "url": origin.to_str().unwrap()}),
    )
    .await
}

pub async fn post(settings: &Settings, path: &str, body: serde_json::Value) -> serde_json::Value {
    reqwest::Client::new()
        .post(format!("{}{path}", settings.address))
        .bearer_auth(&settings.token)
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// The next event of the stream; the limit only stops a test that would otherwise hang.
pub async fn next(stream: &mut (impl Stream<Item = StreamEvent> + Unpin)) -> StreamEvent {
    tokio::time::timeout(Duration::from_secs(60), stream.next())
        .await
        .expect("no event in a minute")
        .expect("the stream ended")
}

/// A server whose Coder is the fake Pi of `moebius-core`, which keeps a run going on a prompt that says `[hang]`.
pub async fn start_with_fake_pi(dir: &Path) -> (Settings, Arc<Server>) {
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("../moebius-core/testdata/fake-pi");
    let config = Config::parse(
        &format!(
            "[board]\ntick_ms = 100\n[router]\nlisten = \"127.0.0.1:0\"\n[pi_bolt]\nprogram = {fake_pi:?}\n\
             [agents.Coder]\nmodel = \"fake-coder\"\ntoken_budget = 100000\n"
        ),
        dir,
        &dir.join("moebius.toml"),
    )
    .unwrap();
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", api_listener.local_addr().unwrap());
    let router_listener = tokio::net::TcpListener::bind(config.router_listen)
        .await
        .unwrap();
    let router_addr = router_listener.local_addr().unwrap();
    let server = Arc::new(Server::new(config, router_addr).unwrap());
    let token = server.token.to_string();
    tokio::spawn(server.clone().serve(api_listener, router_listener));
    (Settings { address, token }, server)
}

/// Reads the snapshot until `ready` holds for it; the limit only stops a test that would otherwise hang.
pub async fn snapshot_where(
    client: &Client,
    ready: impl Fn(&moebius_desktop::client::Snapshot) -> bool,
) -> moebius_desktop::client::Snapshot {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let snapshot = client.snapshot().await.unwrap();
            if ready(&snapshot) {
                return snapshot;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the snapshot did not change in a minute")
}

/// A bare harness configuration repository with the agent Coder, a skill and an MCP server.
pub fn harness_origin(dir: &Path) -> std::path::PathBuf {
    let work = dir.join("harness-work");
    std::fs::create_dir_all(work.join("agents")).unwrap();
    std::fs::create_dir_all(work.join("skills/house-style")).unwrap();
    git(&work, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(
        work.join("agents/Coder.toml"),
        "model = \"coder-model\"\nskills = [\"house-style\"]\n",
    )
    .unwrap();
    std::fs::write(work.join("agents/Coder.md"), "Keep changes small.\n").unwrap();
    std::fs::write(
        work.join("skills/house-style/SKILL.md"),
        "---\nname: house-style\ndescription: Write plainly.\n---\n",
    )
    .unwrap();
    std::fs::write(work.join("mcp.toml"), "[docs]\ncommand = \"docs-server\"\n").unwrap();
    git(&work, &["add", "."]).unwrap();
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

/// Runs a task and every task its messages start, feeding the messages to the app.
pub async fn settle(app: &mut App, task: iced::Task<Message>) {
    let mut pending = vec![task];
    while let Some(task) = pending.pop() {
        let Some(mut stream) = iced_runtime::task::into_stream(task) else {
            continue;
        };
        while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Output(message) = action {
                pending.push(app.update(message));
            }
        }
    }
}

pub fn shown(app: &App, text: &str) -> bool {
    Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1440.0, 1800.0),
        app.view(),
    )
    .find(text)
    .is_ok()
}
