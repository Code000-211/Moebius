//! Editing a column's script from the board: reading the built-in or the repository's script, saving one as a commit
//! on the default branch (pushed for a repository on a forge), refusing one that fails the check, and going back to
//! the built-in script.

use std::path::{Path, PathBuf};

use moebius_core::app::PERSON;
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::router::Grant;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use serde_json::{Value, json};

const HOLD: &str = "function on_card(card) moebius.wait('held from the board') end\n";

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A repository with one commit on `main`.
fn source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&source, "README.md", "hello\n");
    git(&source, &["add", "-A"]).unwrap();
    git(&source, &["commit", "--quiet", "-m", "first"]).unwrap();
    source
}

struct Setup {
    server: Server,
    api: String,
    token: String,
    agent_token: String,
    client: reqwest::Client,
}

impl Setup {
    async fn start(dir: &Path) -> Self {
        let config = Config::parse("", dir, &dir.join("moebius.toml")).unwrap();
        let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            app: server.app.clone(),
            runs: server.harness.clone(),
            harness_repo: server.harness_repo.clone(),
            config: server.live.clone(),
            token: server.token.clone(),
            router: server.router.clone(),
        };
        tokio::spawn(api::serve(listener, state));
        let run = server
            .app
            .store
            .add_run(None, "Moebius", "pi-bolt", "chat")
            .unwrap();
        let agent_token = server.router.issue_token(Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        });
        Self {
            token: server.token.to_string(),
            server,
            api,
            agent_token,
            client: reqwest::Client::new(),
        }
    }

    async fn call(
        &self,
        token: &str,
        method: reqwest::Method,
        repository: i64,
        column: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut request = self
            .client
            .request(
                method,
                format!("{}/v1/repositories/{repository}/scripts/{column}", self.api),
            )
            .bearer_auth(token);
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

    async fn get(&self, repository: i64, column: &str) -> (u16, Value) {
        let token = self.token.clone();
        self.call(&token, reqwest::Method::GET, repository, column, None)
            .await
    }

    async fn put(&self, repository: i64, column: &str, text: &str) -> (u16, Value) {
        let token = self.token.clone();
        let body = json!({ "text": text, "reason": "hold the cards" });
        self.call(&token, reqwest::Method::PUT, repository, column, Some(body))
            .await
    }

    async fn delete(&self, repository: i64, column: &str) -> (u16, Value) {
        let token = self.token.clone();
        self.call(&token, reqwest::Method::DELETE, repository, column, None)
            .await
    }
}

fn head(clone: &Path) -> String {
    git(clone, &["rev-parse", "HEAD"]).unwrap()
}

#[tokio::test]
async fn a_script_is_read_saved_used_and_given_back() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let app = &setup.server.app;
    let repo = app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let clone = PathBuf::from(&repo.path);

    let (status, view) = setup.get(repo.id, "Ready").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["column"], "Ready");
    assert_eq!(view["path"], ".moebius/ready.luau");
    assert_eq!(view["source"], "built-in");
    assert!(view["text"].as_str().unwrap().contains("on_card"));

    let (status, view) = setup.put(repo.id, "Ready", HOLD).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["source"], "repository");
    assert_eq!(view["text"], HOLD);
    assert_eq!(
        git(&clone, &["log", "-1", "--format=%s"]).unwrap(),
        "Change the Ready column's script"
    );
    assert_eq!(
        std::fs::read_to_string(clone.join(".moebius/ready.luau")).unwrap(),
        HOLD
    );
    let (_, view) = setup.get(repo.id, "ready").await;
    assert_eq!(view["source"], "repository");
    assert_eq!(view["text"], HOLD);

    // The board runs the saved script.
    let card = app.create_issue(repo.id, "Held", "", PERSON).unwrap();
    let step = setup.server.engine.settle(card.id).unwrap();
    assert_eq!(step.waiting.as_deref(), Some("held from the board"));

    // The same text again changes nothing.
    let saved = head(&clone);
    assert_eq!(setup.put(repo.id, "Ready", HOLD).await.0, 200);
    assert_eq!(head(&clone), saved);

    let decisions = app.store.judgements("edit_script").unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].decider, "person");
    assert_eq!(decisions[0].reason, "hold the cards");

    let (status, view) = setup.delete(repo.id, "Ready").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["source"], "built-in");
    assert!(!clone.join(".moebius/ready.luau").exists());
    assert_eq!(setup.get(repo.id, "Ready").await.1["source"], "built-in");
    assert_eq!(setup.delete(repo.id, "Ready").await.0, 404);
    assert_eq!(app.store.judgements("edit_script").unwrap().len(), 2);
}

#[tokio::test]
async fn a_script_that_fails_the_check_is_refused_and_nothing_is_committed() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let clone = PathBuf::from(&repo.path);
    let before = head(&clone);

    for text in [
        "function on_card(card) moebius.nonsense() end",
        "this is not luau",
    ] {
        let (status, message) = setup.put(repo.id, "Ready", text).await;
        assert_eq!(status, 422, "{message}");
        assert!(
            message.as_str().unwrap().contains("ready.luau"),
            "{message}"
        );
    }
    assert_eq!(head(&clone), before);
    assert!(!clone.join(".moebius").exists());
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    assert!(
        setup
            .server
            .app
            .store
            .judgements("edit_script")
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn an_unknown_column_or_repository_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    assert_eq!(setup.get(repo.id, "Nowhere").await.0, 404);
    assert_eq!(setup.put(repo.id, "Nowhere", HOLD).await.0, 404);
    assert_eq!(setup.delete(repo.id, "Nowhere").await.0, 404);
    assert_eq!(setup.get(repo.id + 99, "Ready").await.0, 404);
    // A column is found by the name the board shows.
    assert_eq!(
        setup.get(repo.id, "In%20progress").await.1["path"],
        ".moebius/code.luau"
    );
}

#[tokio::test]
async fn the_moebius_agent_may_edit_a_script_and_the_decision_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let token = setup.agent_token.clone();
    let body = json!({ "text": HOLD });
    let (status, view) = setup
        .call(&token, reqwest::Method::PUT, repo.id, "Backlog", Some(body))
        .await;
    assert_eq!(status, 200, "{view}");
    let decisions = setup.server.app.store.judgements("edit_script").unwrap();
    assert_eq!(decisions[0].decider, "agent:Moebius");
}

/// A bare `origin` that `source` has been pushed to, and a repository on a forge whose clone of it Moebius keeps.
fn on_a_forge(dir: &Path, app: &moebius_core::app::App) -> (i64, PathBuf, PathBuf) {
    let origin = dir.join("origin.git");
    git(
        dir,
        &["init", "--quiet", "--bare", "-b", "main", "origin.git"],
    )
    .unwrap();
    git(
        &source(dir),
        &["push", "--quiet", origin.to_str().unwrap(), "main"],
    )
    .unwrap();
    let clone = dir.join("forge-clone");
    git(
        dir,
        &["clone", "--quiet", origin.to_str().unwrap(), "forge-clone"],
    )
    .unwrap();
    let repo = app
        .store
        .add_github_repository(
            "hosted",
            clone.to_str().unwrap(),
            "main",
            "acme/hosted",
            &[],
        )
        .unwrap();
    (repo.id, clone, origin)
}

#[tokio::test]
async fn on_a_forge_a_script_is_pushed_and_a_refused_push_undoes_the_commit() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let app = &setup.server.app;
    let (id, clone, origin) = on_a_forge(dir.path(), app);

    let (status, view) = setup.put(id, "Ready", HOLD).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["source"], "repository");
    assert_eq!(head(&clone), git(&origin, &["rev-parse", "main"]).unwrap());
    assert_eq!(
        git(&origin, &["show", "main:.moebius/ready.luau"]).unwrap(),
        HOLD.trim()
    );

    // The forge refuses the next push.
    let hook = origin.join("hooks/pre-receive");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'protected branch, key sk-abcdefghijklmnopqrstuvwxyz123456' >&2\nexit 1\n",
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let before = head(&clone);
    let (status, message) = setup.put(id, "Backlog", HOLD).await;
    assert_eq!(status, 409, "{message}");
    let message = message.as_str().unwrap();
    assert!(message.contains("protected branch"), "{message}");
    assert!(
        !message.contains("sk-abcdefghijklmnopqrstuvwxyz123456"),
        "{message}"
    );
    assert_eq!(head(&clone), before);
    assert!(!clone.join(".moebius/backlog.luau").exists());
    assert_eq!(setup.get(id, "Backlog").await.1["source"], "built-in");

    let (status, message) = setup.delete(id, "Ready").await;
    assert_eq!(status, 409, "{message}");
    assert_eq!(head(&clone), before);
    assert!(clone.join(".moebius/ready.luau").exists());
    assert_eq!(app.store.judgements("edit_script").unwrap().len(), 1);
}
