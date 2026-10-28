//! A repository's files from the project page: listing a directory and reading a file of the default branch, and
//! saving a file as a commit on the default branch (pushed for a repository on a forge, undone when the push is
//! refused), with paths that leave the repository refused.

use std::path::{Path, PathBuf};

use moebius_core::api::{self, ApiState};
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::router::Grant;
use moebius_core::server::Server;
use serde_json::{Value, json};

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
    write(&source, "src/main.rs", "fn main() {}\n");
    write(&source, "src/lib/mod.rs", "\n");
    std::fs::write(source.join("logo.png"), [0x89, b'P', b'N', b'G', 0, 1, 2]).unwrap();
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
        route: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut request = self
            .client
            .request(method, format!("{}{route}", self.api))
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

    async fn get(&self, route: &str) -> (u16, Value) {
        let token = self.token.clone();
        self.call(&token, reqwest::Method::GET, route, None).await
    }

    async fn put(&self, repository: i64, path: &str, body: Value) -> (u16, Value) {
        let token = self.token.clone();
        let route = format!("/v1/repositories/{repository}/files/{path}");
        self.call(&token, reqwest::Method::PUT, &route, Some(body))
            .await
    }
}

fn head(clone: &Path) -> String {
    git(clone, &["rev-parse", "HEAD"]).unwrap()
}

fn names(tree: &Value) -> Vec<(String, String)> {
    tree["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["path"].as_str().unwrap().to_string(),
                e["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_directory_is_listed_directories_first() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();

    let (status, tree) = setup
        .get(&format!("/v1/repositories/{}/tree", repo.id))
        .await;
    assert_eq!(status, 200, "{tree}");
    assert_eq!(tree["path"], "");
    assert_eq!(
        names(&tree),
        [
            ("src".to_string(), "dir".to_string()),
            ("README.md".into(), "file".into()),
            ("logo.png".into(), "file".into()),
        ]
    );
    assert_eq!(tree["entries"][1]["size"], 6);

    let (status, tree) = setup
        .get(&format!("/v1/repositories/{}/tree?path=src", repo.id))
        .await;
    assert_eq!(status, 200, "{tree}");
    assert_eq!(tree["path"], "src");
    assert_eq!(
        names(&tree),
        [
            ("src/lib".to_string(), "dir".to_string()),
            ("src/main.rs".into(), "file".into()),
        ]
    );

    for path in ["nowhere", "README.md"] {
        let route = format!("/v1/repositories/{}/tree?path={path}", repo.id);
        assert_eq!(setup.get(&route).await.0, 404, "{path}");
    }
    let route = format!("/v1/repositories/{}/tree?path=../..", repo.id);
    assert_eq!(setup.get(&route).await.0, 422);
    let route = format!("/v1/repositories/{}/tree", repo.id + 99);
    assert_eq!(setup.get(&route).await.0, 404);
}

#[tokio::test]
async fn a_file_is_read_as_text_or_said_to_be_binary_or_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let source = source(dir.path());
    write(&source, "big.txt", &"x".repeat(1024 * 1024 + 1));
    git(&source, &["add", "-A"]).unwrap();
    git(&source, &["commit", "--quiet", "-m", "big"]).unwrap();
    let setup = Setup::start(dir.path()).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source.to_str().unwrap())
        .unwrap();
    let file = |path: &str| format!("/v1/repositories/{}/files/{path}", repo.id);

    let (status, view) = setup.get(&file("src/main.rs")).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(
        view,
        json!({ "path": "src/main.rs", "size": 13, "text": "fn main() {}\n" })
    );
    let (_, view) = setup.get(&file("logo.png")).await;
    assert_eq!(
        view,
        json!({ "path": "logo.png", "size": 7, "binary": true })
    );
    let (_, view) = setup.get(&file("big.txt")).await;
    assert_eq!(
        view,
        json!({ "path": "big.txt", "size": 1024 * 1024 + 1, "too_large": true })
    );
    assert_eq!(setup.get(&file("nowhere.rs")).await.0, 404);
    assert_eq!(setup.get(&file("src")).await.0, 404);
    assert_eq!(setup.get(&file(".git/config")).await.0, 422);
}

#[tokio::test]
async fn a_file_is_saved_as_a_commit_and_the_decision_names_who() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let app = &setup.server.app;
    let repo = app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let clone = PathBuf::from(&repo.path);
    let mut events = app.bus.subscribe();

    let text = "fn main() {\n    println!(\"hi\");\n}\n";
    let (status, view) = setup
        .put(
            repo.id,
            "src/main.rs",
            json!({ "text": text, "reason": "greet" }),
        )
        .await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["text"], text);
    assert_eq!(
        git(&clone, &["log", "-1", "--format=%s"]).unwrap(),
        "Update src/main.rs"
    );
    assert_eq!(
        std::fs::read_to_string(clone.join("src/main.rs")).unwrap(),
        text
    );
    let event = loop {
        let event = events.recv().await.unwrap();
        if event.kind == "file_changed" {
            break event;
        }
    };
    assert_eq!(
        event.data,
        json!({ "repository": repo.id, "path": "src/main.rs" })
    );

    // A new file in a new directory, with a message of its own.
    let (status, _) = setup
        .put(
            repo.id,
            "docs/notes.md",
            json!({ "text": "notes\n", "message": "Write notes" }),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        git(&clone, &["log", "-1", "--format=%s"]).unwrap(),
        "Write notes"
    );
    let route = format!("/v1/repositories/{}/files/docs/notes.md", repo.id);
    assert_eq!(setup.get(&route).await.1["text"], "notes\n");

    // The same text again changes nothing.
    let saved = head(&clone);
    assert_eq!(
        setup
            .put(repo.id, "docs/notes.md", json!({ "text": "notes\n" }))
            .await
            .0,
        200
    );
    assert_eq!(head(&clone), saved);

    let token = setup.agent_token.clone();
    let route = format!("/v1/repositories/{}/files/README.md", repo.id);
    let body = json!({ "text": "hello again\n" });
    let (status, _) = setup
        .call(&token, reqwest::Method::PUT, &route, Some(body))
        .await;
    assert_eq!(status, 200);

    let decisions = app.store.judgements("edit_file").unwrap();
    let mut deciders: Vec<_> = decisions.iter().map(|d| d.decider.as_str()).collect();
    deciders.sort();
    assert_eq!(deciders, ["agent:Moebius", "person", "person"]);
    assert!(decisions.iter().any(|d| d.reason == "greet"));
}

#[tokio::test]
async fn a_path_out_of_the_repository_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let clone = PathBuf::from(&repo.path);
    let before = head(&clone);
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, clone.join("away")).unwrap();

    for path in [
        "src%2F..%2F..%2Fescape.txt",
        ".git/hooks/pre-commit",
        "%2Fetc%2Fescape",
        "away/escape.txt",
        "src",
    ] {
        let (status, message) = setup.put(repo.id, path, json!({ "text": "x" })).await;
        assert_eq!(status, 422, "{path}: {message}");
    }
    std::fs::remove_file(clone.join("away")).unwrap();
    assert_eq!(head(&clone), before);
    assert!(!dir.path().join("escape.txt").exists());
    assert!(!outside.join("escape.txt").exists());
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    assert!(
        setup
            .server
            .app
            .store
            .judgements("edit_file")
            .unwrap()
            .is_empty()
    );
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
async fn on_a_forge_a_file_is_pushed_and_a_refused_push_undoes_the_commit() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let app = &setup.server.app;
    let (id, clone, origin) = on_a_forge(dir.path(), app);

    let (status, view) = setup
        .put(id, "README.md", json!({ "text": "pushed\n" }))
        .await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["text"], "pushed\n");
    assert_eq!(head(&clone), git(&origin, &["rev-parse", "main"]).unwrap());
    assert_eq!(git(&origin, &["show", "main:README.md"]).unwrap(), "pushed");

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
    let (status, message) = setup
        .put(id, "new.txt", json!({ "text": "refused\n" }))
        .await;
    assert_eq!(status, 409, "{message}");
    let message = message.as_str().unwrap();
    assert!(message.contains("protected branch"), "{message}");
    assert!(
        !message.contains("sk-abcdefghijklmnopqrstuvwxyz123456"),
        "{message}"
    );
    assert_eq!(head(&clone), before);
    assert!(!clone.join("new.txt").exists());
    let route = format!("/v1/repositories/{id}/files/new.txt");
    assert_eq!(setup.get(&route).await.0, 404);
    assert_eq!(app.store.judgements("edit_file").unwrap().len(), 1);
}
