//! The commands that operate a server, as a person (or the Moebius agent) runs them: the built binary against a real
//! server that listens on port 0.

#[path = "../../moebius-core/tests/common/mod.rs"]
mod common;

use std::path::Path;
use std::process::Output;
use std::sync::Arc;

use moebius_core::config::Config;
use moebius_core::server::Server;
use serde_json::{Value, json};

struct Running {
    dir: tempfile::TempDir,
    url: String,
    token: String,
    server: Arc<Server>,
}

async fn start() -> Running {
    start_with("").await
}

/// Starts a server; `more` is added to its configuration.
async fn start_with(more: &str) -> Running {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("moebius.toml");
    let text = format!(
        "[board]\ntick_ms = 100\n[router]\nlisten = \"127.0.0.1:0\"\n[agents.Coder]\nmodel = \"m\"\n{more}"
    );
    std::fs::write(&file, text).unwrap();
    let mut config = Config::load(&file).unwrap();
    config.moebius = env!("CARGO_BIN_EXE_moebius").into();
    let api = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", api.local_addr().unwrap());
    let router = tokio::net::TcpListener::bind(config.router_listen)
        .await
        .unwrap();
    let router_addr = router.local_addr().unwrap();
    let server = Arc::new(Server::new(config, router_addr).unwrap());
    tokio::spawn(server.clone().serve(api, router));
    Running {
        dir,
        url,
        token: server.token.to_string(),
        server,
    }
}

/// Runs `moebius <args>` in `cwd` with no connection settings but what the arguments and `home` give.
fn moebius_in(cwd: &Path, home: &Path, args: Vec<String>, stdin: &str) -> Output {
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_moebius"))
        .current_dir(cwd)
        .args(args)
        .env_remove("MOEBIUS_URL")
        .env_remove("MOEBIUS_TOKEN")
        .env("XDG_CONFIG_HOME", home)
        .env("HOME", home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

async fn in_dir(cwd: &Path, home: &Path, args: &[&str]) -> Output {
    let (cwd, home) = (cwd.to_path_buf(), home.to_path_buf());
    let args = args.iter().map(|a| a.to_string()).collect();
    tokio::task::spawn_blocking(move || moebius_in(&cwd, &home, args, ""))
        .await
        .unwrap()
}

impl Running {
    async fn run_with(&self, args: &[&str], stdin: &str) -> Output {
        let mut all: Vec<String> = ["--server", &self.url, "--token", &self.token]
            .map(String::from)
            .into();
        all.extend(args.iter().map(|a| a.to_string()));
        let (cwd, home) = (self.dir.path().to_path_buf(), self.dir.path().join("home"));
        let stdin = stdin.to_string();
        tokio::task::spawn_blocking(move || moebius_in(&cwd, &home, all, &stdin))
            .await
            .unwrap()
    }

    async fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, "").await
    }
}

fn out(output: &Output) -> String {
    assert!(
        output.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn json_out(output: &Output) -> Value {
    serde_json::from_str(&out(output)).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn api_calls_any_route_and_prints_the_answer() {
    let s = start().await;
    let snapshot = json_out(&s.run(&["api", "GET", "/v1/snapshot"]).await);
    assert_eq!(snapshot["boards"], json!([]));
    let text = out(&s.run(&["api", "GET", "/v1/health"]).await);
    assert!(text.contains("\n  \"version\""), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repository_and_an_issue_are_added_and_shown_on_the_board() {
    let s = start().await;
    let origin = common::origin(s.dir.path());
    let origin = origin.to_str().unwrap();

    let added = out(&s.run(&["repo", "add", "demo", origin]).await);
    assert!(added.contains("name: demo"), "{added}");
    let repos = out(&s.run(&["repos"]).await);
    assert!(repos.starts_with("1  demo"), "{repos}");

    let card = json_out(
        &s.run(&[
            "--json",
            "issue",
            "create",
            "1",
            "Fix the parser",
            "--body",
            "soon",
        ])
        .await,
    );
    assert_eq!(card["title"], "Fix the parser");
    assert_eq!(card["column"], "Backlog");

    let board = out(&s.run(&["board"]).await);
    assert!(board.starts_with("demo (repository 1)"), "{board}");
    assert!(board.contains("#1  Fix the parser"), "{board}");
    assert!(out(&s.run(&["board", "demo"]).await).starts_with("demo (repository 1)"));
    let card = out(&s.run(&["card", "1"]).await);
    assert!(card.starts_with("card 1 #1 Fix the parser"), "{card}");
    let snapshot = json_out(&s.run(&["--json", "board"]).await);
    assert_eq!(snapshot["boards"][0]["cards"][0]["title"], "Fix the parser");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_setting_is_changed_in_the_file_and_in_the_running_server() {
    let s = start().await;
    let answer = out(&s.run(&["settings", "set", "board.round_limit=7"]).await);
    assert!(answer.contains("restart_needed: false"), "{answer}");
    let file = std::fs::read_to_string(s.dir.path().join("moebius.toml")).unwrap();
    assert!(file.contains("round_limit = 7"), "{file}");
    let settings = out(&s.run(&["settings"]).await);
    assert!(settings.contains("board.round_limit = 7\n"), "{settings}");
    // The boards of the snapshot read the limit live.
    let origin = common::origin(s.dir.path());
    out(&s
        .run(&["repo", "add", "demo", origin.to_str().unwrap()])
        .await);
    let snapshot = json_out(&s.run(&["--json", "board"]).await);
    assert_eq!(snapshot["boards"][0]["round_limit"], 7);
    out(&s
        .run(&["settings", "set", "--remove", "board.round_limit"])
        .await);
    let file = std::fs::read_to_string(s.dir.path().join("moebius.toml")).unwrap();
    assert!(!file.contains("round_limit"), "{file}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_is_read_from_standard_input() {
    let s = start().await;
    let said = out(&s
        .run_with(&["secrets", "set", "anthropic"], "sk-test-123\n")
        .await);
    assert!(said.contains("stored the key anthropic"), "{said}");
    let secrets = std::fs::read_to_string(s.dir.path().join(".moebius/secrets.toml")).unwrap();
    assert!(secrets.contains("sk-test-123"), "{secrets}");
    let settings = out(&s.run(&["settings"]).await);
    assert!(settings.contains("stored keys: anthropic"), "{settings}");
    assert!(!settings.contains("sk-test-123"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_cannot_be_saved_without_a_harness_repository() {
    let s = start().await;
    let agents = out(&s.run(&["agents"]).await);
    assert!(agents.contains("Coder"), "{agents}");
    let output = s.run(&["agent", "save", "Fast", "--model", "m"]).await;
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("the server answered 422")
            && error.contains("configured in moebius.toml, not in a harness repository"),
        "{error}"
    );
    assert!(output.stdout.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_error_status_is_a_failure_with_the_servers_message() {
    let s = start().await;
    let output = s.run(&["api", "POST", "/v1/cards/99/approve"]).await;
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.starts_with("moebius: the server answered 404: "),
        "{error}"
    );
    let home = s.dir.path().join("home");
    let wrong = in_dir(
        s.dir.path(),
        &home,
        &["--server", &s.url, "--token", "wrong", "board"],
    )
    .await;
    assert_eq!(wrong.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("401"));
    // A usage mistake is 2.
    assert_eq!(s.run(&["board", "--nonsense"]).await.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn without_flags_the_configuration_in_the_directory_names_the_server() {
    let s = start().await;
    let other = tempfile::tempdir().unwrap();
    let port = s.url.rsplit(':').next().unwrap();
    let token_file = s.server.config.token_file.display().to_string();
    std::fs::write(
        other.path().join("moebius.toml"),
        format!("listen = \"127.0.0.1:{port}\"\ntoken_file = {token_file:?}\n"),
    )
    .unwrap();
    let home = other.path().join("home");
    let output = in_dir(other.path(), &home, &["api", "GET", "/v1/health"]).await;
    assert!(out(&output).contains("version"));

    // The desktop app's saved settings come first.
    let saved = home.join("moebius");
    std::fs::create_dir_all(&saved).unwrap();
    std::fs::write(
        saved.join("desktop.toml"),
        "address = \"http://127.0.0.1:1\"\ntoken = \"x\"\n",
    )
    .unwrap();
    let output = in_dir(other.path(), &home, &["board"]).await;
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot reach http://127.0.0.1:1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_moebius_agent_operates_moebius_from_its_chat() {
    let fake_pi = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../moebius-core/testdata/fake-pi")
        .canonicalize()
        .unwrap();
    let program = format!("[pi_bolt]\nprogram = {:?}\n", fake_pi.display().to_string());
    let s = start_with(&program).await;
    let origin = common::origin(s.dir.path());
    out(&s
        .run(&["repo", "add", "demo", origin.to_str().unwrap()])
        .await);

    // The chat is with the Moebius agent; the fake Pi runs the commands as the agent's bash tool would.
    let message =
        "Look. [run \"$MOEBIUS\" board] [run \"$MOEBIUS\" settings set board.round_limit=9]";
    let chat = json_out(
        &s.run(&["--json", "chat", "start", message, "--repository", "1"])
            .await,
    );
    assert_eq!(chat["agent"], "Moebius");
    let run = chat["id"].as_i64().unwrap();

    let client = reqwest::Client::new();
    let events = loop {
        let events: Vec<Value> = client
            .get(format!("{}/v1/runs/{run}/events", s.url))
            .bearer_auth(&s.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if events.iter().any(|e| e["kind"] == "agent_settled") {
            break events;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    let answer: String = events
        .iter()
        .filter(|e| e["kind"] == "message_update")
        .filter_map(|e| e["data"]["assistantMessageEvent"]["delta"].as_str())
        .collect();
    assert!(answer.contains("demo (repository 1)"), "{answer}");
    let file = std::fs::read_to_string(s.dir.path().join("moebius.toml")).unwrap();
    assert!(file.contains("round_limit = 9"), "{file}");
    // What the agent decided is recorded as the agent's, with the chat's run.
    let judgements = json_out(
        &s.run(&["api", "GET", "/v1/judgements?kind=edit_settings"])
            .await,
    );
    assert_eq!(judgements[0]["decider"], "agent:Moebius");
    assert_eq!(judgements[0]["run"], run);
    out(&s.run(&["chat", "end", &run.to_string()]).await);
}
