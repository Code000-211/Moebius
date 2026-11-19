//! Editing a column's script from the board: reading the built-in or the project's script, saving one as a commit in
//! the harness configuration repository (pushed to its origin), refusing one that fails the check, and going back to
//! the built-in script; and the migration that moves the scripts a repository's settings had to its project.

use std::path::{Path, PathBuf};

use moebius_core::app::OWNER_AUTHOR;
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
    /// The clone of the harness configuration repository, where the scripts are committed.
    fn clone_path(&self) -> PathBuf {
        self.server
            .harness_repo
            .as_ref()
            .unwrap()
            .path()
            .to_path_buf()
    }

    async fn start(dir: &Path) -> Self {
        // A test may have made the harness configuration repository already, as it was before.
        let harness = dir.join("harness");
        if !harness.exists() {
            moebius_core::harness::repo::create(&harness, &Default::default(), None).unwrap();
        }
        let text = format!("[harness]\nurl = {:?}\n", harness.display().to_string());
        let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
        let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            codes: Default::default(),
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
        project: i64,
        column: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut request = self
            .client
            .request(
                method,
                format!("{}/v1/projects/{project}/scripts/{column}", self.api),
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

    async fn get(&self, project: i64, column: &str) -> (u16, Value) {
        let token = self.token.clone();
        self.call(&token, reqwest::Method::GET, project, column, None)
            .await
    }

    async fn put(&self, project: i64, column: &str, text: &str) -> (u16, Value) {
        let token = self.token.clone();
        let body = json!({ "text": text, "reason": "hold the cards" });
        self.call(&token, reqwest::Method::PUT, project, column, Some(body))
            .await
    }

    async fn delete(&self, project: i64, column: &str) -> (u16, Value) {
        let token = self.token.clone();
        self.call(&token, reqwest::Method::DELETE, project, column, None)
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
    let clone = setup.clone_path();

    let (status, view) = setup.get(repo.home_project_id, "Not started").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["column"], "Not started");
    assert_eq!(view["path"], "projects/demo/ready.luau");
    assert_eq!(view["source"], "built-in");
    assert!(view["text"].as_str().unwrap().contains("on_card"));

    let (status, view) = setup.put(repo.home_project_id, "Not started", HOLD).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["source"], "project");
    assert_eq!(view["text"], HOLD);
    assert_eq!(
        git(&clone, &["log", "-1", "--format=%s"]).unwrap(),
        "Change the Not started column's script of demo"
    );
    assert_eq!(
        std::fs::read_to_string(clone.join("projects/demo/ready.luau")).unwrap(),
        HOLD
    );
    // A column is found by its name in any case.
    let (_, view) = setup.get(repo.home_project_id, "not started").await;
    assert_eq!(view["source"], "project");
    assert_eq!(view["text"], HOLD);

    // The board runs the saved script.
    let card = app.create_issue(repo.id, "Held", "", OWNER_AUTHOR).unwrap();
    let step = setup.server.engine.settle(card.id).unwrap();
    assert_eq!(step.waiting.as_deref(), Some("held from the board"));

    // The same text again changes nothing.
    let saved = head(&clone);
    assert_eq!(
        setup.put(repo.home_project_id, "Not started", HOLD).await.0,
        200
    );
    assert_eq!(head(&clone), saved);

    let decisions = app.store.judgements("edit_script").unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].decider, "person:owner");
    assert_eq!(decisions[0].reason, "hold the cards");

    let (status, view) = setup.delete(repo.home_project_id, "Not started").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["source"], "built-in");
    assert!(!clone.join("projects/demo/ready.luau").exists());
    assert_eq!(
        setup.get(repo.home_project_id, "Not started").await.1["source"],
        "built-in"
    );
    assert_eq!(
        setup.delete(repo.home_project_id, "Not started").await.0,
        404
    );
    assert_eq!(app.store.judgements("edit_script").unwrap().len(), 2);
}

#[tokio::test]
async fn ready_names_the_not_started_column_in_the_scripts_route() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();

    let (status, view) = setup.get(repo.home_project_id, "Ready").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["column"], "Not started");
    assert_eq!(view["path"], "projects/demo/ready.luau");
    let (_, built_in) = setup.get(repo.home_project_id, "Not started").await;
    assert_eq!(view, built_in);
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
    let clone = setup.clone_path();
    let before = head(&clone);

    for text in [
        "function on_card(card) moebius.nonsense() end",
        "this is not luau",
    ] {
        let (status, message) = setup.put(repo.home_project_id, "Not started", text).await;
        assert_eq!(status, 422, "{message}");
        assert!(
            message.as_str().unwrap().contains("ready.luau"),
            "{message}"
        );
    }
    assert_eq!(head(&clone), before);
    assert!(!clone.join("projects").exists());
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
async fn an_unknown_column_or_project_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    assert_eq!(setup.get(repo.home_project_id, "Nowhere").await.0, 404);
    assert_eq!(
        setup.put(repo.home_project_id, "Nowhere", HOLD).await.0,
        404
    );
    assert_eq!(setup.delete(repo.home_project_id, "Nowhere").await.0, 404);
    assert_eq!(
        setup.get(repo.home_project_id + 99, "Not started").await.0,
        404
    );
    // A column is found by the name the board shows.
    assert_eq!(
        setup.get(repo.home_project_id, "In%20progress").await.1["path"],
        "projects/demo/code.luau"
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
        .call(
            &token,
            reqwest::Method::PUT,
            repo.home_project_id,
            "Backlog",
            Some(body),
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let decisions = setup.server.app.store.judgements("edit_script").unwrap();
    assert_eq!(decisions[0].decider, "agent:Moebius");
}

#[tokio::test]
async fn a_script_is_pushed_and_a_refused_push_undoes_the_commit() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let app = &setup.server.app;
    let repo = app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let clone = setup.clone_path();
    let origin = dir.path().join("harness");

    let (status, view) = setup.put(repo.home_project_id, "Not started", HOLD).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["source"], "project");
    assert_eq!(head(&clone), git(&origin, &["rev-parse", "main"]).unwrap());
    assert_eq!(
        git(&origin, &["show", "main:projects/demo/ready.luau"]).unwrap(),
        HOLD.trim()
    );

    // The origin refuses the next push.
    let hook = origin.join(".git/hooks/pre-receive");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'protected branch, key sk-abcdefghijklmnopqrstuvwxyz123456' >&2\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let before = head(&clone);
    let (status, message) = setup.put(repo.home_project_id, "Backlog", HOLD).await;
    assert_eq!(status, 409, "{message}");
    let message = message.to_string();
    assert!(message.contains("protected branch"), "{message}");
    assert!(
        !message.contains("sk-abcdefghijklmnopqrstuvwxyz123456"),
        "{message}"
    );
    assert_eq!(head(&clone), before);
    assert!(!clone.join("projects/demo/backlog.luau").exists());
    assert_eq!(
        setup.get(repo.home_project_id, "Backlog").await.1["source"],
        "built-in"
    );

    let (status, message) = setup.delete(repo.home_project_id, "Not started").await;
    assert_eq!(status, 409, "{message}");
    assert_eq!(head(&clone), before);
    assert!(clone.join("projects/demo/ready.luau").exists());
    assert_eq!(app.store.judgements("edit_script").unwrap().len(), 1);
}

#[tokio::test]
async fn a_project_lists_the_script_of_every_column() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    assert_eq!(
        setup.put(repo.home_project_id, "Backlog", HOLD).await.0,
        200
    );
    let response = setup
        .client
        .get(format!(
            "{}/v1/projects/{}/scripts",
            setup.api, repo.home_project_id
        ))
        .bearer_auth(&setup.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let scripts: Vec<Value> = response.json().await.unwrap();
    let sources: Vec<(&str, &str)> = scripts
        .iter()
        .map(|s| (s["column"].as_str().unwrap(), s["source"].as_str().unwrap()))
        .collect();
    assert_eq!(
        sources,
        [
            ("Backlog", "project"),
            ("Not started", "built-in"),
            ("In progress", "built-in"),
            ("In review", "built-in"),
            ("Ready to merge", "built-in"),
            ("Done", "built-in"),
        ]
    );
    // The scripts are a project's: a repository has no scripts route any more.
    let old = setup
        .client
        .get(format!(
            "{}/v1/repositories/{}/scripts/Backlog",
            setup.api, repo.id
        ))
        .bearer_auth(&setup.token)
        .send()
        .await
        .unwrap();
    assert_eq!(old.status().as_u16(), 404);
}

#[tokio::test]
async fn a_repositorys_scripts_from_before_projects_move_to_its_project_and_run_as_before() {
    let dir = tempfile::tempdir().unwrap();
    // The harness configuration as it was: the repository's settings hold its scripts and its checks.
    let harness = dir.path().join("harness");
    moebius_core::harness::repo::create(&harness, &Default::default(), None).unwrap();
    write(&harness, "repos/demo/ready.luau", HOLD);
    write(&harness, "repos/demo/checks", "make test\n");
    git(&harness, &["add", "-A"]).unwrap();
    git(&harness, &["commit", "--quiet", "-m", "Settings of demo"]).unwrap();
    let setup = Setup::start(dir.path()).await;
    let app = &setup.server.app;
    let repo = app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();

    // The server moves them as it starts and with each refresh; here the repository came after the start.
    app.move_scripts_to_projects();
    let clone = setup.clone_path();
    assert_eq!(
        git(&clone, &["log", "-1", "--format=%s"]).unwrap(),
        "Move the column scripts of each repository to its project"
    );
    assert_eq!(
        git(&harness, &["show", "main:projects/demo/ready.luau"]).unwrap(),
        HOLD.trim()
    );
    assert!(git(&harness, &["show", "main:repos/demo/ready.luau"]).is_err());
    assert_eq!(
        git(&harness, &["show", "main:repos/demo/checks"]).unwrap(),
        "make test"
    );
    assert_eq!(app.store.judgements("move_scripts").unwrap().len(), 1);

    // The board works as it did.
    let card = app.create_issue(repo.id, "Held", "", OWNER_AUTHOR).unwrap();
    let step = setup.server.engine.settle(card.id).unwrap();
    assert_eq!(step.waiting.as_deref(), Some("held from the board"));
    let (_, view) = setup.get(repo.home_project_id, "Not started").await;
    assert_eq!(
        (view["source"].as_str(), view["path"].as_str()),
        (Some("project"), Some("projects/demo/ready.luau"))
    );

    // Moved once: nothing more to commit.
    let moved = head(&clone);
    app.move_scripts_to_projects();
    assert_eq!(head(&clone), moved);
}

#[tokio::test]
async fn a_renamed_project_keeps_its_scripts() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path()).await;
    let app = &setup.server.app;
    let repo = app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    assert_eq!(
        setup.put(repo.home_project_id, "Not started", HOLD).await.0,
        200
    );
    let response = setup
        .client
        .patch(format!(
            "{}/v1/projects/{}",
            setup.api, repo.home_project_id
        ))
        .bearer_auth(&setup.token)
        .json(&json!({ "name": "Demo site" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let clone = setup.clone_path();
    assert_eq!(
        git(&clone, &["log", "-1", "--format=%s"]).unwrap(),
        "Rename the project demo to Demo site"
    );
    assert!(clone.join("projects/Demo site/ready.luau").is_file());
    assert!(!clone.join("projects/demo").exists());
    let card = app.create_issue(repo.id, "Held", "", OWNER_AUTHOR).unwrap();
    let step = setup.server.engine.settle(card.id).unwrap();
    assert_eq!(step.waiting.as_deref(), Some("held from the board"));
    let decisions = app.store.judgements("rename_project").unwrap();
    assert_eq!(decisions[0].decider, "person:owner");
}
