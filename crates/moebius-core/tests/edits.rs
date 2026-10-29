//! Editing in place what a person or the Moebius agent wrote: a card's title and body (and its issue on GitHub), the
//! GitHub logins a repository trusts, the brief of a draft research and a repository's own rules. Each edit is
//! recorded as a decision of whoever made it and announced, so the app reads it again.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::router::Grant;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use serde_json::{Value, json};

/// A repository with one commit.
fn source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(source.join("README.md"), "A demo.\n").unwrap();
    git(&source, &["add", "-A"]).unwrap();
    git(&source, &["commit", "--quiet", "-m", "first"]).unwrap();
    source
}

/// What the fake GitHub was asked to change: the issue's number and the request.
#[derive(Default)]
struct FakeGitHub {
    edited: Mutex<Vec<(i64, Value)>>,
}

async fn fake_github() -> (String, Arc<FakeGitHub>) {
    let fake = Arc::new(FakeGitHub::default());
    let app = axum::Router::new()
        .route(
            "/repos/acme/widgets/installation",
            get(|| async { axum::Json(json!({ "id": 77 })) }),
        )
        .route(
            "/app/installations/77/access_tokens",
            post(|| async {
                let expires = time::OffsetDateTime::now_utc() + Duration::from_secs(3600);
                (
                    StatusCode::CREATED,
                    axum::Json(json!({
                        "token": "ghs_token",
                        "expires_at": expires
                            .format(&time::format_description::well_known::Rfc3339)
                            .unwrap(),
                    })),
                )
            }),
        )
        .route(
            "/repos/acme/widgets/issues/{number}",
            patch(
                |State(fake): State<Arc<FakeGitHub>>,
                 UrlPath(number): UrlPath<i64>,
                 axum::Json(body): axum::Json<Value>| async move {
                    if number == 404 {
                        return Err(StatusCode::NOT_FOUND);
                    }
                    fake.edited.lock().unwrap().push((number, body.clone()));
                    Ok(axum::Json(
                        json!({ "number": number, "title": body["title"] }),
                    ))
                },
            ),
        )
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (url, fake)
}

struct Setup {
    server: Server,
    base: String,
    token: String,
    agent_token: String,
    client: reqwest::Client,
}

impl Setup {
    /// A server with `config` and its API, and a run token of the Moebius agent.
    async fn start(dir: &Path, config: &str) -> Self {
        let config = Config::parse(config, dir, &dir.join("moebius.toml")).unwrap();
        let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
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
            base,
            agent_token,
            client: reqwest::Client::new(),
        }
    }

    /// A server with the GitHub App of the fake GitHub at `url` and a repository `widgets` on it.
    async fn github(dir: &Path, url: &str) -> (Self, i64) {
        let key = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/github-app-test-key.pem");
        let config = format!(
            "[github]\napp_id = 4242\nprivate_key_file = {:?}\napi_url = {url:?}\n",
            key.display().to_string()
        );
        let setup = Self::start(dir, &config).await;
        git(
            dir,
            &[
                "clone",
                "--quiet",
                "--bare",
                source(dir).to_str().unwrap(),
                "remote.git",
            ],
        )
        .unwrap();
        let remote = dir.join("remote.git");
        let repo = setup
            .server
            .app
            .add_github_repository(
                "widgets",
                remote.to_str().unwrap(),
                "acme/widgets",
                &["leon".to_string()],
            )
            .unwrap();
        (setup, repo.id)
    }

    async fn call_as(
        &self,
        token: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
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

    async fn put(&self, path: &str, body: Value) -> (u16, Value) {
        self.call_as(&self.token, reqwest::Method::PUT, path, Some(body))
            .await
    }

    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.call_as(&self.token, reqwest::Method::POST, path, Some(body))
            .await
    }

    async fn get(&self, path: &str) -> (u16, Value) {
        self.call_as(&self.token, reqwest::Method::GET, path, None)
            .await
    }

    fn deciders(&self, kind: &str) -> Vec<String> {
        let mut deciders: Vec<String> = self
            .server
            .app
            .store
            .judgements(kind)
            .unwrap()
            .into_iter()
            .map(|j| j.decider)
            .collect();
        deciders.sort();
        deciders
    }
}

#[tokio::test]
async fn a_card_is_edited_in_the_store_recorded_and_announced() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), "").await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let card = setup
        .server
        .app
        .create_issue(repo.id, "Add a buton", "Somewhere.", "person")
        .unwrap();
    let mut events = setup.server.app.bus.subscribe();

    let (status, edited) = setup
        .put(
            &format!("/v1/cards/{}", card.id),
            json!({ "title": " Add a button ", "body": "On the report page.", "reason": "typo" }),
        )
        .await;
    assert_eq!(status, 200, "{edited}");
    assert_eq!(edited["title"], "Add a button");
    assert_eq!(edited["body"], "On the report page.");
    let stored = setup.server.app.card(card.id).unwrap();
    assert_eq!(
        (stored.title.as_str(), stored.body.as_str()),
        ("Add a button", "On the report page.")
    );
    let event = events.recv().await.unwrap();
    assert_eq!(event.kind, "card_updated");
    assert_eq!(event.data["title"], "Add a button");
    let judgements = setup.server.app.store.judgements("edit_card").unwrap();
    assert_eq!(judgements.len(), 1);
    assert_eq!(
        (
            judgements[0].decider.as_str(),
            judgements[0].reason.as_str(),
            judgements[0].card_id
        ),
        ("person", "typo", Some(card.id))
    );

    // The Moebius agent edits as itself; the same text again records nothing.
    let route = format!("/v1/cards/{}", card.id);
    let body = json!({ "title": "Add a button", "body": "On the report page, top right." });
    let (status, _) = setup
        .call_as(
            &setup.agent_token,
            reqwest::Method::PUT,
            &route,
            Some(body.clone()),
        )
        .await;
    assert_eq!(status, 200);
    let (status, _) = setup.put(&route, body).await;
    assert_eq!(status, 200);
    assert_eq!(setup.deciders("edit_card"), ["agent:Moebius", "person"]);

    // A blank title and an unknown card are refused.
    let (status, why) = setup.put(&route, json!({ "title": "  " })).await;
    assert_eq!(status, 422, "{why}");
    let (status, _) = setup.put("/v1/cards/999", json!({ "title": "x" })).await;
    assert_eq!(status, 404);
    assert_eq!(setup.deciders("edit_card").len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_card_on_github_changes_its_issue_and_reads_its_blockers_from_the_body() {
    let dir = tempfile::tempdir().unwrap();
    let (url, fake) = fake_github().await;
    let (setup, repository) = Setup::github(dir.path(), &url).await;
    let app = &setup.server.app;
    let card = app
        .store
        .add_numbered_card(repository, 7, "Export", "", "leon", "Backlog")
        .unwrap();

    let (status, edited) = setup
        .put(
            &format!("/v1/cards/{}", card.id),
            json!({ "title": "Export the report", "body": "As CSV.\n\nBlocked by #3, #7" }),
        )
        .await;
    assert_eq!(status, 200, "{edited}");
    let asked = fake.edited.lock().unwrap().clone();
    assert_eq!(
        asked,
        [(
            7,
            json!({ "title": "Export the report", "body": "As CSV.\n\nBlocked by #3, #7" })
        )]
    );
    assert_eq!(app.store.dependencies(card.id).unwrap(), [3]);
    assert_eq!(setup.deciders("edit_card"), ["person"]);

    // GitHub refusing leaves the card as it was and records nothing.
    let lost = app
        .store
        .add_numbered_card(repository, 404, "Gone", "", "leon", "Backlog")
        .unwrap();
    let (status, _) = setup
        .put(
            &format!("/v1/cards/{}", lost.id),
            json!({ "title": "Still gone" }),
        )
        .await;
    assert_eq!(status, 500);
    assert_eq!(app.card(lost.id).unwrap().title, "Gone");
    assert_eq!(setup.deciders("edit_card"), ["person"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_logins_a_github_repository_trusts_are_edited() {
    let dir = tempfile::tempdir().unwrap();
    let (url, _) = fake_github().await;
    let (setup, repository) = Setup::github(dir.path(), &url).await;
    let mut events = setup.server.app.bus.subscribe();

    let route = format!("/v1/repositories/{repository}");
    let (status, edited) = setup
        .put(
            &route,
            json!({ "trusted": [" mona ", "leon", "Mona", ""], "reason": "a new maintainer" }),
        )
        .await;
    assert_eq!(status, 422, "a blank login is refused: {edited}");
    let (status, edited) = setup
        .put(&route, json!({ "trusted": [" mona ", "leon", "Mona"] }))
        .await;
    assert_eq!(status, 200, "{edited}");
    assert_eq!(edited["trusted"], json!(["mona", "leon"]));
    let stored = setup
        .server
        .app
        .store
        .repository(repository)
        .unwrap()
        .unwrap();
    assert_eq!(stored.trusted, ["mona", "leon"]);
    let event = events.recv().await.unwrap();
    assert_eq!(event.kind, "repository_updated");
    assert_eq!(event.data["trusted"], json!(["mona", "leon"]));
    assert_eq!(setup.deciders("edit_repository"), ["person"]);

    let (status, why) = setup.put(&route, json!({ "trusted": ["no spaces"] })).await;
    assert_eq!(status, 422, "{why}");

    // A repository without a forge trusts no GitHub logins.
    let other = dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let plain = setup
        .server
        .app
        .add_repository("plain", source(&other).to_str().unwrap())
        .unwrap();
    let (status, why) = setup
        .put(
            &format!("/v1/repositories/{}", plain.id),
            json!({ "trusted": ["leon"] }),
        )
        .await;
    assert_eq!(status, 422, "{why}");
    let (status, _) = setup
        .put("/v1/repositories/99", json!({ "trusted": [] }))
        .await;
    assert_eq!(status, 404);
    assert_eq!(setup.deciders("edit_repository"), ["person"]);
}

#[tokio::test]
async fn the_brief_of_a_draft_is_written_anew_until_it_is_confirmed() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), "").await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let (status, draft) = setup
        .post(
            "/v1/research",
            json!({ "repository": repo.id, "goal": "Faster", "measure": "echo 3",
                    "better": "lower", "token_budget": 1000 }),
        )
        .await;
    assert_eq!(status, 200, "{draft}");
    let route = format!("/v1/research/{}", draft["id"]);

    let (status, edited) = setup
        .put(
            &route,
            json!({ "goal": "Faster builds", "measure": "echo 2", "better": "lower",
                    "token_budget": 5000, "reason": "a clearer goal" }),
        )
        .await;
    assert_eq!(status, 200, "{edited}");
    assert_eq!(
        (
            &edited["goal"],
            &edited["measure"],
            &edited["token_budget"],
            &edited["status"]
        ),
        (
            &json!("Faster builds"),
            &json!("echo 2"),
            &json!(5000),
            &json!("draft")
        )
    );
    assert_eq!(setup.deciders("edit_research"), ["person"]);

    let (status, why) = setup
        .put(
            &route,
            json!({ "goal": "Faster", "measure": "echo 2", "better": "sideways", "token_budget": 5 }),
        )
        .await;
    assert_eq!(status, 422, "{why}");

    let (status, _) = setup.post(&format!("{route}/confirm"), json!({})).await;
    assert_eq!(status, 200);
    let (status, why) = setup
        .put(
            &route,
            json!({ "goal": "Slower", "measure": "echo 2", "better": "lower", "token_budget": 5 }),
        )
        .await;
    assert_eq!(status, 409, "{why}");
    let (_, research) = setup.get(&route).await;
    assert_eq!(research["goal"], "Faster builds");
    let (status, _) = setup
        .put(
            "/v1/research/99",
            json!({ "goal": "x", "measure": "y", "better": "lower", "token_budget": 5 }),
        )
        .await;
    assert_eq!(status, 404);
    assert_eq!(setup.deciders("edit_research"), ["person"]);
}

#[tokio::test]
async fn a_repositorys_own_rules_are_checked_before_they_are_committed() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), "").await;
    let repo = setup
        .server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let clone = Path::new(&repo.path);
    let before = git(clone, &["rev-parse", "HEAD"]).unwrap();
    let route = format!("/v1/repositories/{}/files/.moebius/rules.md", repo.id);

    let (status, why) = setup
        .put(
            &route,
            json!({ "text": "## small-diffs\nKeep it small.\n## small-diffs\nAgain.\n" }),
        )
        .await;
    assert_eq!(status, 422, "{why}");
    assert!(
        why.as_str().unwrap().contains("named twice"),
        "the check says why: {why}"
    );
    assert_eq!(git(clone, &["rev-parse", "HEAD"]).unwrap(), before);
    assert!(!clone.join(".moebius/rules.md").exists());

    let text = "## small-diffs\nKeep changes small.\n";
    let (status, saved) = setup.put(&route, json!({ "text": text })).await;
    assert_eq!(status, 200, "{saved}");
    assert_ne!(git(clone, &["rev-parse", "HEAD"]).unwrap(), before);
    assert_eq!(setup.deciders("edit_repository_rules"), ["person"]);
    assert!(setup.deciders("edit_file").is_empty());

    // The Rules page names the repository by id, so a rule can be edited where it is shown.
    let (_, rules) = setup.get("/v1/rules").await;
    assert_eq!(rules["repositories"][0]["repository_id"], repo.id);
    assert_eq!(rules["repositories"][0]["rules"][0]["name"], "small-diffs");
}
