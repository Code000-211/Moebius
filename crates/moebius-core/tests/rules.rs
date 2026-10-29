//! The rules page: what the API shows of the built-in patterns, the rules in prose, a repository's own rules and the
//! week's refusals, and the edit of the harness configuration repository's `rules.md` as a commit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use moebius_core::config::Config;
use moebius_core::decide::check;
use moebius_core::repos::git;
use moebius_core::router::Grant;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use serde_json::{Value, json};

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn commit_all(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]).unwrap();
    git(dir, &["commit", "--quiet", "-m", message]).unwrap();
}

/// A bare harness configuration repository with one agent and the rules `## no-vendoring`.
fn harness_origin(dir: &Path) -> PathBuf {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&work, "agents/Coder.toml", "model = \"coder-model\"\n");
    write(
        &work,
        "rules.md",
        "Our rules.\n\n## no-vendoring\nDo not vendor.\n",
    );
    commit_all(&work, "Start");
    let bare = dir.join("harness.git");
    git(
        dir,
        &["init", "--quiet", "--bare", "-b", "main", "harness.git"],
    )
    .unwrap();
    git(&work, &["push", "--quiet", bare.to_str().unwrap(), "main"]).unwrap();
    bare
}

/// A repository with rules of its own on its default branch.
fn source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&source, "README.md", "hello\n");
    write(
        &source,
        ".moebius/rules.md",
        "## small-diffs\nKeep changes small.\n",
    );
    commit_all(&source, "first");
    source
}

fn server(dir: &Path, settings: &str) -> Server {
    let config = Config::parse(settings, dir, &dir.join("moebius.toml")).unwrap();
    Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap()
}

fn harness_server(dir: &Path) -> (Server, PathBuf) {
    let origin = harness_origin(dir);
    let settings = format!("[harness]\nurl = {:?}\n", origin.display().to_string());
    (server(dir, &settings), origin)
}

struct Setup {
    api: String,
    checks: String,
    token: String,
    run_token: String,
    client: reqwest::Client,
}

impl Setup {
    /// The API and the check endpoints of `server`, with a card of the repository `demo` that has a run.
    async fn start(server: &Server, source: Option<&Path>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api_url = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            app: server.app.clone(),
            runs: server.harness.clone(),
            harness_repo: server.harness_repo.clone(),
            config: server.live.clone(),
            token: server.token.clone(),
            router: server.router.clone(),
        };
        tokio::spawn(api::serve(listener, state));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let checks = format!("http://{}", listener.local_addr().unwrap());
        let routes = check::routes(
            server.app.clone(),
            server.router.clone(),
            server.config.rules_path(),
        );
        tokio::spawn(async move { axum::serve(listener, routes).await });

        let (card_id, run_id) = match source {
            Some(source) => {
                let repo = server
                    .app
                    .add_repository("demo", source.to_str().unwrap())
                    .unwrap();
                let card = server
                    .app
                    .store
                    .add_card(repo.id, "Fix it", "", "person", "In progress")
                    .unwrap();
                let run = server
                    .app
                    .store
                    .add_run(Some(card.id), "Coder", "pi-bolt", "code")
                    .unwrap();
                (Some(card.id), run.id)
            }
            None => (
                None,
                server
                    .app
                    .store
                    .add_run(None, "Coder", "pi-bolt", "code")
                    .unwrap()
                    .id,
            ),
        };
        let run_token = server.router.issue_token(Grant {
            run_id,
            card_id,
            budget: None,
        });
        Self {
            api: api_url,
            checks,
            token: server.token.to_string(),
            run_token,
            client: reqwest::Client::new(),
        }
    }

    async fn call(&self, method: reqwest::Method, body: Option<Value>) -> (u16, Value) {
        let mut request = self
            .client
            .request(method, format!("{}/v1/rules", self.api))
            .bearer_auth(&self.token);
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

    async fn get(&self) -> Value {
        let (status, view) = self.call(reqwest::Method::GET, None).await;
        assert_eq!(status, 200, "{view}");
        view
    }

    /// Asks the check endpoint about a `bash` call; the reply's decision and reason.
    async fn check(&self, command: &str) -> (String, String) {
        let reply: Value = self
            .client
            .post(format!("{}/moebius/check", self.checks))
            .bearer_auth(&self.run_token)
            .json(&json!({"tool": "bash", "input": {"command": command}, "tool_call_id": "c"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        (
            reply["decision"].as_str().unwrap().into(),
            reply["reason"].as_str().unwrap().into(),
        )
    }
}

/// The `rules` the latest `tool_call` judgement was given.
fn rules_of_latest_check(server: &Server) -> String {
    let judgement = server
        .app
        .store
        .judgements("tool_call")
        .unwrap()
        .pop()
        .unwrap();
    let context: BTreeMap<String, String> =
        serde_json::from_str(&server.app.store.blobs().get(&judgement.context[0]).unwrap())
            .unwrap();
    context["rules"].clone()
}

fn head(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"]).unwrap()
}

#[tokio::test]
async fn the_page_shows_the_patterns_the_rules_and_a_repositorys_own_rules_and_counts_refusals() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _) = harness_server(dir.path());
    let setup = Setup::start(&server, Some(&source(dir.path()))).await;

    let view = setup.get().await;
    let names: Vec<&str> = view["patterns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "no-destructive-commands",
            "no-force-push",
            "stay-in-the-worktree",
            "no-secrets-in-commands",
            "no-dumping-large-files"
        ]
    );
    assert!(
        view["patterns"][1]["description"]
            .as_str()
            .unwrap()
            .contains("rewrites history")
    );
    assert_eq!(view["prose"]["source"], "harness");
    assert_eq!(view["prose"]["editable"], true);
    assert_eq!(
        view["prose"]["text"],
        "Our rules.\n\n## no-vendoring\nDo not vendor.\n"
    );
    assert_eq!(
        view["prose"]["rules"],
        json!([{"name": "no-vendoring", "text": "Do not vendor."}])
    );
    assert_eq!(view["prose"]["error"], Value::Null);
    assert_eq!(
        view["repositories"],
        json!([{
            "repository": "demo",
            "repository_id": 1,
            "text": "## small-diffs\nKeep changes small.",
            "rules": [{"name": "small-diffs", "text": "Keep changes small."}],
            "error": null,
        }])
    );
    assert_eq!(view["refusals"], json!({}));

    // A refusal is counted for its rule; a refusal that names no rule is not.
    for _ in 0..2 {
        let (decision, reason) = setup.check("git push --force").await;
        assert_eq!(decision, "refuse");
        assert!(reason.starts_with("no-force-push: "), "{reason}");
    }
    let (decision, reason) = setup.check("ls").await;
    assert_eq!(decision, "refuse");
    assert!(reason.contains("no teacher"), "{reason}");
    assert_eq!(setup.get().await["refusals"], json!({"no-force-push": 2}));
}

#[tokio::test]
async fn an_edit_is_a_commit_that_is_pushed_and_the_next_check_uses_the_new_rules() {
    let dir = tempfile::tempdir().unwrap();
    let (server, origin) = harness_server(dir.path());
    let setup = Setup::start(&server, None).await;
    let clone = dir.path().join(".moebius/harness");

    setup.check("ls").await;
    assert_eq!(
        rules_of_latest_check(&server),
        "## no-vendoring\nDo not vendor.\n"
    );

    let text = "## leave-ci-alone\nDo not edit .github/.\n";
    let (status, saved) = setup
        .call(reqwest::Method::PUT, Some(json!({ "text": text })))
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Update the rules"
    );
    assert_eq!(
        saved["commit"],
        git(&origin, &["rev-parse", "--short", "main"]).unwrap()
    );
    assert_eq!(saved["prose"]["text"], text);
    assert_eq!(saved["prose"]["rules"][0]["name"], "leave-ci-alone");
    assert_eq!(saved["patterns"].as_array().unwrap().len(), 5);
    assert_eq!(setup.get().await["prose"]["text"], text);
    let judgements = server.app.store.judgements("edit_rules").unwrap();
    assert_eq!(judgements.len(), 1);
    assert_eq!(
        (
            judgements[0].decider.as_str(),
            judgements[0].choice.as_str()
        ),
        ("person", "edit_rules")
    );

    setup.check("ls").await;
    assert_eq!(rules_of_latest_check(&server), text);

    // Text that changes nothing makes no commit, and a missing final new line is the same text.
    let pushed = head(&clone);
    let (status, same) = setup
        .call(
            reqwest::Method::PUT,
            Some(json!({ "text": text.trim_end() })),
        )
        .await;
    assert_eq!(status, 200, "{same}");
    assert!(same.get("commit").is_none());
    assert_eq!(head(&clone), pushed);
    assert_eq!(server.app.store.judgements("edit_rules").unwrap().len(), 1);

    // Emptying the rules is an edit too.
    let (status, _) = setup
        .call(reqwest::Method::PUT, Some(json!({ "text": "" })))
        .await;
    assert_eq!(status, 200);
    assert_eq!(setup.get().await["prose"]["rules"], json!([]));
}

#[tokio::test]
async fn text_that_does_not_parse_is_refused_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (server, origin) = harness_server(dir.path());
    let setup = Setup::start(&server, None).await;
    let clone = dir.path().join(".moebius/harness");
    let start = head(&clone);

    for (text, why) in [
        ("## \nNo name.\n", "line 1: a rule needs a name"),
        (
            "## a\none\n## a\ntwo\n",
            "line 3: the rule \"a\" is named twice",
        ),
    ] {
        let (status, body) = setup
            .call(reqwest::Method::PUT, Some(json!({ "text": text })))
            .await;
        assert_eq!(status, 422, "{body}");
        assert!(body.as_str().unwrap().contains(why), "{body}");
    }
    assert_eq!(head(&clone), start);
    assert_eq!(head(&origin), start);
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    assert!(
        server
            .app
            .store
            .judgements("edit_rules")
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_failed_push_undoes_the_edit_and_records_no_decision() {
    let dir = tempfile::tempdir().unwrap();
    let (server, origin) = harness_server(dir.path());
    let setup = Setup::start(&server, None).await;
    let clone = dir.path().join(".moebius/harness");
    let start = head(&clone);

    write(
        &origin,
        "hooks/pre-receive",
        "#!/bin/sh\necho 'closed for edits' >&2\nexit 1\n",
    );
    #[cfg(unix)]
    std::fs::set_permissions(
        origin.join("hooks/pre-receive"),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let (status, body) = setup
        .call(
            reqwest::Method::PUT,
            Some(json!({ "text": "## new\nNew rule.\n" })),
        )
        .await;
    assert_eq!(status, 500);
    assert!(
        body.as_str().unwrap().contains("closed for edits"),
        "{body}"
    );
    assert_eq!(head(&clone), start);
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    assert_eq!(
        setup.get().await["prose"]["text"],
        "Our rules.\n\n## no-vendoring\nDo not vendor.\n"
    );
    assert!(
        server
            .app
            .store
            .judgements("edit_rules")
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn without_a_harness_repository_the_rules_are_the_servers_file() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "rules.md", "## be-kind\nBe kind.\n");
    let server = server(dir.path(), "rules_file = \"rules.md\"\n");
    let setup = Setup::start(&server, None).await;

    let view = setup.get().await;
    assert_eq!(view["prose"]["source"], "file");
    assert_eq!(view["prose"]["editable"], true);
    assert_eq!(view["prose"]["rules"][0]["name"], "be-kind");
    assert_eq!(view["patterns"].as_array().unwrap().len(), 5);

    let (status, body) = setup
        .call(reqwest::Method::PUT, Some(json!({ "text": "## x\ny" })))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["prose"]["rules"][0]["name"], "x");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("rules.md")).unwrap(),
        "## x\ny\n"
    );

    // Without the key the file is rules.md next to moebius.toml, made by the first save.
    let dir = tempfile::tempdir().unwrap();
    let server = self::server(dir.path(), "");
    let setup = Setup::start(&server, None).await;
    let view = setup.get().await;
    assert_eq!(view["prose"]["editable"], true);
    assert_eq!(view["prose"]["rules"], json!([]));
    let (status, body) = setup
        .call(
            reqwest::Method::PUT,
            Some(json!({ "text": "## be-kind\nBe kind.\n" })),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(dir.path().join("rules.md").exists());
}
