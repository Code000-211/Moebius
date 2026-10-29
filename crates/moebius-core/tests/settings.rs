//! Editing `moebius.toml` through the desktop API: a change is written to the file with its comments, keys that are live
//! apply without a restart, the others are reported as needing one, a bad change writes nothing, and an API key typed
//! into the app reaches the router without ever being in the file, an answer, the store or an event.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::http::HeaderMap;
use axum::routing::post;
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::router::{Grant, Protocol};
use moebius_core::server::Server;
use serde_json::{Value, json};

mod common;

const FILE: &str = "# Moebius, edited by hand.\nrun_limit = 3 # runs at a time\n\n# The router.\n[router]\nlisten = \"127.0.0.1:0\"\n\n# How the board ticks.\n[board]\ntick_ms = 2000\nround_limit = 5 # coding rounds\n";

const KEY: &str = "typed-key-8e1f2a7c5d9b4f60a3c1";

struct Fixture {
    dir: tempfile::TempDir,
    server: Arc<Server>,
    serving: tokio::task::JoinHandle<std::io::Result<()>>,
    api: String,
    router: String,
    token: String,
    client: reqwest::Client,
}

impl Fixture {
    async fn start(text: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("moebius.toml");
        std::fs::write(&file, text).unwrap();
        let config = Config::load(&file).unwrap();
        let api = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let router = tokio::net::TcpListener::bind(config.router_listen)
            .await
            .unwrap();
        let (api_addr, router_addr) = (api.local_addr().unwrap(), router.local_addr().unwrap());
        let server = Arc::new(Server::new(config, router_addr).unwrap());
        let token = server.token.to_string();
        let serving = tokio::spawn(server.clone().serve(api, router));
        Self {
            dir,
            server,
            serving,
            api: format!("http://{api_addr}"),
            router: format!("http://{router_addr}"),
            token,
            client: reqwest::Client::new(),
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.path().join("moebius.toml")
    }

    fn text(&self) -> String {
        std::fs::read_to_string(self.file()).unwrap()
    }

    async fn get(&self, path: &str) -> (u16, String) {
        let response = self
            .client
            .get(format!("{}{path}", self.api))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap();
        (response.status().as_u16(), response.text().await.unwrap())
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Value) -> (u16, String) {
        let response = self
            .client
            .request(method, format!("{}{path}", self.api))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        (response.status().as_u16(), response.text().await.unwrap())
    }

    /// `PUT /v1/settings`; the answer is JSON when it succeeds.
    async fn change(&self, set: Value, remove: Value) -> (u16, Value) {
        let (status, text) = self
            .send(
                reqwest::Method::PUT,
                "/v1/settings",
                json!({ "set": set, "remove": remove }),
            )
            .await;
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (status, body)
    }

    async fn settings(&self) -> Value {
        serde_json::from_str(&self.get("/v1/settings").await.1).unwrap()
    }
}

/// The text of every file under `dir`, to look for what must not be there.
fn everything_under(dir: &Path, found: &mut Vec<(PathBuf, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            everything_under(&path, found);
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            found.push((path, text));
        }
    }
}

/// The upstream the router forwards to: it keeps the `Authorization` header of each request.
async fn recording_upstream(seen: Arc<Mutex<Vec<String>>>) -> String {
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(move |headers: HeaderMap| async move {
            let given = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            seen.lock().unwrap().push(given);
            axum::Json(json!({
                "id": "x", "object": "chat.completion", "created": 0, "model": "m",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_is_written_with_its_comments_and_the_answer_lists_every_key() {
    let f = Fixture::start(FILE).await;
    let (status, saved) = f
        .change(
            json!({"board.round_limit": 7, "budgets.day_tokens": 900}),
            json!([]),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved, json!({"restart_needed": false, "restart_keys": []}));
    assert_eq!(
        f.text(),
        FILE.replace("round_limit = 5", "round_limit = 7") + "\n[budgets]\nday_tokens = 900\n"
    );
    // No temporary file stays behind.
    assert!(
        std::fs::read_dir(f.dir.path()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp"))
    );

    let settings = f.settings().await;
    assert_eq!(settings["file"], f.file().display().to_string());
    assert_eq!(
        settings["values"]["board.round_limit"],
        json!({"value": 7, "default": false})
    );
    assert_eq!(
        settings["values"]["budgets.day_tokens"],
        json!({"value": 900, "default": false})
    );
    assert_eq!(
        settings["values"]["checks.timeout_seconds"],
        json!({"value": 1800, "default": true})
    );

    let (status, saved) = f
        .change(json!({}), json!(["budgets.day_tokens", "board.tick_ms"]))
        .await;
    assert_eq!(status, 200, "{saved}");
    assert!(!f.text().contains("day_tokens") && !f.text().contains("tick_ms"));
    assert!(f.text().contains("# coding rounds"));
    assert_eq!(
        f.settings().await["values"]["board.tick_ms"],
        json!({"value": 2000, "default": true})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn board_limits_budgets_and_agents_apply_without_a_restart() {
    let f = Fixture::start(FILE).await;
    let origin = common::origin(f.dir.path());
    let plain = f
        .server
        .app
        .add_repository("plain", origin.to_str().unwrap())
        .unwrap();
    let card = f
        .server
        .app
        .create_issue(plain.id, "Anything", "", "person")
        .unwrap();
    // The card's script tells the limits it is given.
    let clone = Path::new(&plain.path);
    std::fs::create_dir_all(clone.join(".moebius")).unwrap();
    std::fs::write(
        clone.join(".moebius/backlog.luau"),
        "function on_card(card)\n  moebius.wait('limits ' .. moebius.round_limit .. ' ' .. moebius.run_limit .. ' ' .. tostring(moebius.day_budget))\nend\n",
    )
    .unwrap();
    git(clone, &["add", "."]).unwrap();
    git(clone, &["commit", "--quiet", "-m", "script"]).unwrap();
    let waiting = || {
        f.server
            .app
            .waiting(std::slice::from_ref(&card))
            .remove(&card.id)
            .unwrap()
    };
    let server = f.server.clone();
    tokio::task::spawn_blocking(move || server.tick())
        .await
        .unwrap();
    assert_eq!(waiting(), "limits 5 3 nil");

    let (status, saved) = f
        .change(
            json!({
                "board.round_limit": 9, "budgets.day_tokens": 5000, "run_limit": 2,
                "agents.Coder": {"model": "live-model", "token_budget": 100},
                "board.tick_ms": 100,
            }),
            json!([]),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["restart_needed"], false);
    let server = f.server.clone();
    tokio::task::spawn_blocking(move || server.tick())
        .await
        .unwrap();
    assert_eq!(waiting(), "limits 9 2 5000");

    // The boards and the usage screen read the same limits, and the agent is there.
    let snapshot: Value = serde_json::from_str(&f.get("/v1/snapshot").await.1).unwrap();
    assert_eq!(snapshot["boards"][0]["round_limit"], 9);
    let usage: Value = serde_json::from_str(&f.get("/v1/usage").await.1).unwrap();
    assert_eq!(usage["totals"]["day_budget"], 5000);
    let agents: Value = serde_json::from_str(&f.get("/v1/agents").await.1).unwrap();
    let coder = agents
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "Coder")
        .unwrap();
    assert_eq!(
        (coder["model"].as_str(), coder["token_budget"].as_i64()),
        (Some("live-model"), Some(100))
    );
    assert_eq!(
        f.server.live.get().board_tick,
        std::time::Duration::from_millis(100)
    );

    // Removing the agent takes it away again.
    let (status, _) = f.change(json!({}), json!(["agents.Coder"])).await;
    assert_eq!(status, 200);
    let agents: Value = serde_json::from_str(&f.get("/v1/agents").await.1).unwrap();
    assert!(
        agents
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["name"] != "Coder")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_that_needs_a_restart_is_written_and_reported_and_the_server_restarts_on_request() {
    let f = Fixture::start(FILE).await;
    let (status, saved) = f
        .change(
            json!({"listen": "127.0.0.1:9999", "github.app_id": 3, "github.private_key_file": "k.pem", "teacher": {"model": "m"}}),
            json!([]),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["restart_needed"], true);
    assert_eq!(
        saved["restart_keys"],
        json!([
            "listen",
            "teacher.model",
            "github.app_id",
            "github.private_key_file"
        ])
    );
    assert!(f.text().contains("listen = \"127.0.0.1:9999\""));
    // The server still runs with what it started with, and the page can show the banner again.
    assert_eq!(f.server.live.get().listen, f.server.config.listen);
    let settings = f.settings().await;
    assert_eq!(settings["restart_needed"], true);
    assert_eq!(settings["restart_keys"], saved["restart_keys"]);

    // Putting a key back to what the server started with needs no restart.
    let (_, saved) = f
        .change(
            json!({"listen": "127.0.0.1:7171"}),
            json!(["github", "teacher"]),
        )
        .await;
    assert_eq!(saved, json!({"restart_needed": false, "restart_keys": []}));

    // Asking for a restart answers 202, then stops the server's work: `serve` returns and says so, and the binary
    // starts the same program again.
    assert!(!f.server.live.restart_requested());
    let (status, _) = f
        .send(reqwest::Method::POST, "/v1/server/restart", json!({}))
        .await;
    assert_eq!(status, 202);
    let served = tokio::time::timeout(std::time::Duration::from_secs(60), f.serving)
        .await
        .expect("the server stops serving")
        .unwrap();
    assert!(served.is_ok());
    assert!(f.server.live.restart_requested());
    // The Pi processes go before the program is replaced; with no run going this does nothing.
    f.server.harness.kill_all();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upstream_with_a_typed_key_is_used_by_the_router_and_the_key_stays_secret() {
    let f = Fixture::start(FILE).await;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let upstream = recording_upstream(seen.clone()).await;
    let mut events = f.server.app.bus.subscribe();

    // The key is stored by name, and only the name is ever told.
    let (status, names) = f
        .send(
            reqwest::Method::PUT,
            "/v1/secrets/provider",
            json!({"value": KEY}),
        )
        .await;
    assert_eq!(status, 200, "{names}");
    assert_eq!(names, "[\"provider\"]");
    let secrets = f.dir.path().join(".moebius/secrets.toml");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&secrets).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    // A key that is not stored cannot be named.
    let (status, why) = f
        .change(
            json!({"router.upstreams.0": {"protocol": "openai", "base_url": upstream, "api_key_secret": "missing"}}),
            json!([]),
        )
        .await;
    assert_eq!(status, 422);
    assert!(why.as_str().unwrap().contains("missing"), "{why}");

    let (status, saved) = f
        .change(
            json!({"router.upstreams.0": {"protocol": "openai", "base_url": upstream, "api_key_secret": "provider"}}),
            json!([]),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["restart_needed"], false);
    let run = f
        .server
        .app
        .store
        .add_run(None, "Coder", "test", "chat")
        .unwrap();
    let token = f.server.router.issue_token(Grant {
        run_id: run.id,
        card_id: None,
        budget: None,
    });
    let response = f
        .client
        .post(format!("{}/v1/chat/completions", f.router))
        .bearer_auth(&token)
        .json(&json!({"model": "m", "messages": [{"role": "user", "content": "hi"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
    assert_eq!(*seen.lock().unwrap(), [format!("Bearer {KEY}")]);
    let upstreams = f.server.live.get().upstreams.clone();
    assert_eq!(upstreams[0].protocol, Protocol::OpenAi);

    // The key is in secrets.toml and nowhere else: not in the file, an answer, the store, the blobs or an event.
    let mut found = Vec::new();
    everything_under(f.dir.path(), &mut found);
    for (path, text) in &found {
        if path == &secrets {
            assert!(text.contains(KEY));
        } else if !path.ends_with("moebius.sqlite") {
            assert!(!text.contains(KEY), "{} holds the key", path.display());
        }
    }
    assert!(!f.text().contains(KEY));
    let database = std::fs::read(f.dir.path().join(".moebius/moebius.sqlite")).unwrap();
    assert!(!database.windows(KEY.len()).any(|w| w == KEY.as_bytes()));
    for path in [
        "/v1/settings",
        "/v1/models",
        "/v1/judgements?kind=edit_settings",
    ] {
        let (_, text) = f.get(path).await;
        assert!(!text.contains(KEY), "{path}");
    }
    let settings = f.settings().await;
    assert_eq!(settings["secrets"], json!(["provider"]));
    assert_eq!(
        settings["values"]["router.upstreams"]["value"][0]["api_key_secret"],
        "provider"
    );
    while let Ok(event) = events.try_recv() {
        assert!(!event.data.to_string().contains(KEY), "{}", event.kind);
    }

    // The person's edits are decisions, with the changed keys and no secret.
    let judgements = f.server.app.store.judgements("edit_settings").unwrap();
    assert_eq!(judgements.len(), 1);
    assert_eq!(judgements[0].decider, "person");
    let context = f
        .server
        .app
        .store
        .blobs()
        .get(&judgements[0].context[0])
        .unwrap();
    assert!(
        context.contains("router.upstreams.0") && context.contains("provider"),
        "{context}"
    );

    // A key is removed by name; one that is not there is not found.
    let (status, names) = f
        .send(reqwest::Method::DELETE, "/v1/secrets/provider", json!({}))
        .await;
    assert_eq!((status, names.as_str()), (200, "[]"));
    let (status, _) = f
        .send(reqwest::Method::DELETE, "/v1/secrets/provider", json!({}))
        .await;
    assert_eq!(status, 404);
    let (status, _) = f
        .send(
            reqwest::Method::PUT,
            "/v1/secrets/a b",
            json!({"value": KEY}),
        )
        .await;
    assert_eq!(status, 422);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_that_does_not_parse_is_refused_and_nothing_is_written() {
    let f = Fixture::start(FILE).await;
    for (set, remove, mentions) in [
        (json!({"run_limit": 0}), json!([]), "run_limit"),
        (json!({"run_limit": "many"}), json!([]), "run_limit"),
        (json!({"no_such_key": 1}), json!([]), "no_such_key"),
        (
            json!({"router.listen": "0.0.0.0:7172"}),
            json!([]),
            "loopback",
        ),
        (json!({"retro_at": "25:00"}), json!([]), "retro_at"),
        (
            json!({"teacher.model": "m", "teacher.timeout_seconds": 99}),
            json!([]),
            "timeout_seconds",
        ),
        (
            json!({"router.upstreams.0": {"protocol": "openai", "base_url": "http://x", "api_key_env": "MOEBIUS_NO_SUCH_VARIABLE"}}),
            json!([]),
            "MOEBIUS_NO_SUCH_VARIABLE",
        ),
        (
            json!({"router.upstreams.0": {"protocol": "openai", "base_url": "http://x", "api_key_env": "PATH", "api_key_secret": "k"}}),
            json!([]),
            "both",
        ),
        (json!({"run limit": 1}), json!([]), "run limit"),
        (
            json!({"router.upstreams.4": {"protocol": "openai", "base_url": "http://x"}}),
            json!([]),
            "entries",
        ),
    ] {
        let (status, why) = f.change(set.clone(), remove.clone()).await;
        assert_eq!(status, 422, "{set} {why}");
        assert!(why.as_str().unwrap().contains(mentions), "{set}: {why}");
    }
    assert_eq!(f.text(), FILE);
    assert!(
        f.server
            .app
            .store
            .judgements("edit_settings")
            .unwrap()
            .is_empty()
    );
    let (status, _) = f
        .send(
            reqwest::Method::PUT,
            "/v1/settings",
            json!({"set": {"x": 1}, "bogus": 1}),
        )
        .await;
    assert_eq!(status, 422);
    let response = f
        .client
        .put(format!("{}/v1/settings", f.api))
        .json(&json!({"set": {"run_limit": 1}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}
