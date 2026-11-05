//! A project's settings from the desktop API: the files of `repos/<repository name>/` in the harness configuration
//! repository other than the column scripts, listed, saved as commits pushed to its origin, checked first, removed, and
//! the `.moebius/` folder a repository still has listed as left over and never read.

use std::path::{Path, PathBuf};

use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::router::Grant;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use serde_json::{Value, json};

/// A repository with one commit on `main`, and `files` in it.
fn source(dir: &Path, files: &[(&str, &str)]) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(source.join("README.md"), "hello\n").unwrap();
    for (path, text) in files {
        std::fs::create_dir_all(source.join(path).parent().unwrap()).unwrap();
        std::fs::write(source.join(path), text).unwrap();
    }
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
    /// A server with its API; with a harness configuration repository `dir/harness` unless `harness` is false.
    async fn start(dir: &Path, harness: bool) -> Self {
        let text = if harness {
            let repository = dir.join("harness");
            moebius_core::harness::repo::create(&repository, &Default::default(), None).unwrap();
            format!("[harness]\nurl = {:?}\n", repository.display().to_string())
        } else {
            String::new()
        };
        let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
        let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            sessions: Default::default(),
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
        path: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.api))
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

    async fn get(&self, repository: i64) -> Value {
        let (status, view) = self
            .call(
                &self.token,
                reqwest::Method::GET,
                &format!("/v1/repositories/{repository}/settings"),
                None,
            )
            .await;
        assert_eq!(status, 200, "{view}");
        view
    }

    async fn put(&self, repository: i64, file: &str, text: &str) -> (u16, Value) {
        self.call(
            &self.token,
            reqwest::Method::PUT,
            &format!("/v1/repositories/{repository}/settings/{file}"),
            Some(json!({ "text": text, "reason": "because" })),
        )
        .await
    }

    async fn delete(&self, repository: i64, file: &str) -> (u16, Value) {
        self.call(
            &self.token,
            reqwest::Method::DELETE,
            &format!("/v1/repositories/{repository}/settings/{file}"),
            None,
        )
        .await
    }

    fn clone_path(&self) -> PathBuf {
        self.server
            .harness_repo
            .as_ref()
            .unwrap()
            .path()
            .to_path_buf()
    }
}

fn file<'a>(view: &'a Value, name: &str) -> &'a Value {
    view["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["file"] == name)
        .unwrap_or_else(|| panic!("no {name} in {view}"))
}

#[tokio::test]
async fn the_files_of_a_project_are_listed_saved_as_commits_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), true).await;
    let app = &setup.server.app;
    let repo = app
        .add_repository("demo", source(dir.path(), &[]).to_str().unwrap())
        .unwrap();
    let (clone, origin) = (setup.clone_path(), dir.path().join("harness"));

    let view = setup.get(repo.id).await;
    assert_eq!(view["path"], "repos/demo");
    assert_eq!(view["editable"], true);
    let names: Vec<&str> = view["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "rules.md",
            "review.md",
            "lint.toml",
            "checks",
            "test-first",
            "localize"
        ]
    );
    assert!(
        view["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["exists"] == false && f["text"] == "")
    );
    assert_eq!(view["scripts"], json!([]));
    assert_eq!(view["leftover"], json!([]));

    // A save is a commit in the harness configuration repository, pushed to its origin, and it is read back.
    let mut events = app.bus.subscribe();
    let (status, saved) = setup
        .put(repo.id, "review.md", "Prefer small functions.")
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["exists"], true);
    assert_eq!(saved["text"], "Prefer small functions.\n");
    assert_eq!(
        git(&origin, &["show", "main:repos/demo/review.md"]).unwrap(),
        "Prefer small functions."
    );
    assert_eq!(
        git(&clone, &["log", "-1", "--format=%s"]).unwrap(),
        "Update review.md of demo"
    );
    assert_eq!(
        saved["commit"],
        git(&clone, &["rev-parse", "--short", "HEAD"]).unwrap()
    );
    assert_eq!(
        app.project_file(&repo, "review.md").unwrap(),
        "Prefer small functions.\n"
    );
    let event = events.try_recv().unwrap();
    assert_eq!(event.kind, "project_settings_changed");
    assert_eq!(
        event.data,
        json!({ "repository": repo.id, "file": "review.md" })
    );
    let decisions = app.store.judgements("edit_project_file").unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].decider, "person");
    assert_eq!(decisions[0].reason, "because");
    assert_eq!(file(&setup.get(repo.id).await, "review.md")["exists"], true);

    // The same text again changes nothing.
    let head = git(&clone, &["rev-parse", "HEAD"]).unwrap();
    let (status, again) = setup
        .put(repo.id, "review.md", "Prefer small functions.")
        .await;
    assert_eq!(status, 200, "{again}");
    assert!(again.get("commit").is_none(), "{again}");
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]).unwrap(), head);

    // Removing it is a commit too, and the project has nothing to remove after that.
    let (status, removed) = setup.delete(repo.id, "review.md").await;
    assert_eq!(status, 200, "{removed}");
    assert_eq!(removed["exists"], false);
    assert!(!clone.join("repos/demo/review.md").exists());
    assert_eq!(setup.delete(repo.id, "review.md").await.0, 404);
    assert_eq!(app.store.judgements("edit_project_file").unwrap().len(), 2);
}

#[tokio::test]
async fn a_rules_file_or_a_lint_plan_that_fails_its_check_is_refused_and_nothing_is_committed() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), true).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path(), &[]).to_str().unwrap())
        .unwrap();
    let clone = setup.clone_path();
    let head = git(&clone, &["rev-parse", "HEAD"]).unwrap();

    let (status, why) = setup
        .put(repo.id, "rules.md", "## a\none\n## a\ntwo\n")
        .await;
    assert_eq!(status, 422, "{why}");
    assert!(
        why.as_str().unwrap().starts_with("rules.md: line 3"),
        "{why}"
    );
    let (status, why) = setup.put(repo.id, "lint.toml", "unknown = 1\n").await;
    assert_eq!(status, 422, "{why}");
    assert!(why.as_str().unwrap().starts_with("lint.toml: "), "{why}");
    // A column's script has its own route, and no other file is a file of the settings.
    for file in ["ready.luau", "notes.md", "agents"] {
        let (status, why) = setup.put(repo.id, file, "x").await;
        assert_eq!(status, 422, "{file}: {why}");
    }
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]).unwrap(), head);
    assert!(!clone.join("repos").exists());
    assert_eq!(setup.put(repo.id + 9, "checks", "true").await.0, 404);

    // The rules of the project are checked the way the server's are, and recorded as such.
    let (status, saved) = setup
        .put(repo.id, "rules.md", "## small-diffs\nKeep changes small.")
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(
        setup
            .server
            .app
            .store
            .judgements("edit_repository_rules")
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn test_first_is_a_file_that_exists_or_not_and_the_moebius_agent_may_edit_it() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), true).await;
    let app = &setup.server.app;
    let repo = app
        .add_repository("demo", source(dir.path(), &[]).to_str().unwrap())
        .unwrap();
    assert!(!app.test_first(&repo));

    let (status, saved) = setup
        .call(
            &setup.agent_token,
            reqwest::Method::PUT,
            &format!("/v1/repositories/{}/settings/test-first", repo.id),
            Some(json!({ "text": "" })),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["exists"], true);
    assert!(app.test_first(&repo));
    let decisions = app.store.judgements("edit_project_file").unwrap();
    assert_eq!(decisions[0].decider, "agent:Moebius");

    let (status, _) = setup.delete(repo.id, "test-first").await;
    assert_eq!(status, 200);
    assert!(!app.test_first(&repo));
}

#[tokio::test]
async fn a_dot_moebius_folder_of_the_repository_is_listed_as_left_over_and_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), true).await;
    let app = &setup.server.app;
    let repo = app
        .add_repository(
            "demo",
            source(
                dir.path(),
                &[
                    (".moebius/rules.md", "## sneaky\nx\n"),
                    (".moebius/test-first", ""),
                ],
            )
            .to_str()
            .unwrap(),
        )
        .unwrap();
    let view = setup.get(repo.id).await;
    assert_eq!(
        view["leftover"],
        json!([".moebius/rules.md", ".moebius/test-first"])
    );
    assert!(
        view["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["exists"] == false)
    );
    assert!(!app.test_first(&repo));
    let (_, rules) = setup
        .call(&setup.token, reqwest::Method::GET, "/v1/rules", None)
        .await;
    assert_eq!(rules["repositories"][0]["rules"], json!([]));
}

#[tokio::test]
async fn without_a_harness_configuration_repository_nothing_can_be_saved() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), false).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path(), &[]).to_str().unwrap())
        .unwrap();
    let view = setup.get(repo.id).await;
    assert_eq!(view["editable"], false);
    let (status, why) = setup.put(repo.id, "checks", "true").await;
    assert_eq!(status, 409, "{why}");
    assert!(why.as_str().unwrap().contains("[harness]"), "{why}");
    assert_eq!(setup.delete(repo.id, "checks").await.0, 409);
}

#[cfg(unix)] // The origin's hook is a shell script.
#[tokio::test]
async fn a_push_the_origin_refuses_is_a_conflict_and_leaves_the_project_as_it_was() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), true).await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path(), &[]).to_str().unwrap())
        .unwrap();
    let clone = setup.clone_path();
    let hook = dir.path().join("harness/.git/hooks/pre-receive");
    std::fs::write(&hook, "#!/bin/sh\necho 'closed for edits' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let before = git(&clone, &["rev-parse", "HEAD"]).unwrap();
    let (status, why) = setup.put(repo.id, "checks", "true").await;
    assert_eq!(status, 409, "{why}");
    assert!(why.as_str().unwrap().contains("closed for edits"), "{why}");
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]).unwrap(), before);
    assert!(!clone.join("repos/demo/checks").exists());
}
