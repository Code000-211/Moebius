//! A fake GitHub for the tests of the server, and a server that polls it: one repository, `acme/widgets`, with its
//! issues, pull request 7, its checks and reviews.

use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use serde_json::{Value, json};

use super::Server;
use crate::app::App;
use crate::board::{Engine, Runs, Step};
use crate::config::Config;
use crate::repos::git;
use crate::store::{Card, Change, Repository, Run};

/// A GitHub with one repository, `acme/widgets`: its issues, the ETag they are served with and the
/// `If-None-Match` of every request for them.
pub(super) struct FakeGitHub {
    pub(super) issues: Mutex<(String, Vec<Value>)>,
    pub(super) asked: Mutex<Vec<Option<String>>>,
    pub(super) pulls: Mutex<Pulls>,
}

/// The pull requests of the fake: what it was asked to open and merge, and what it answers about pull request 7.
pub(super) struct Pulls {
    pub(super) opened: Vec<Value>,
    pub(super) merged: Vec<Value>,
    pub(super) pull: Value,
    pub(super) check_runs: Vec<Value>,
    pub(super) reviews: Vec<Value>,
}

impl FakeGitHub {
    pub(super) fn set_issues(&self, etag: &str, issues: Vec<Value>) {
        *self.issues.lock().unwrap() = (etag.to_string(), issues);
    }

    pub(super) fn asked(&self) -> Vec<Option<String>> {
        self.asked.lock().unwrap().clone()
    }

    /// Makes pull request 7 look as `state` (`open` or `closed`) and `merged` say.
    pub(super) fn set_pull(&self, state: &str, merged: bool) {
        self.pulls.lock().unwrap().pull = pull(state, merged);
    }

    /// Makes the head of pull request 7 the commit `sha`, as a push does.
    pub(super) fn set_head(&self, sha: &str) {
        self.pulls.lock().unwrap().pull["head"]["sha"] = json!(sha);
    }
}

pub(super) fn pull(state: &str, merged: bool) -> Value {
    json!({ "number": 7, "state": state, "merged": merged,
            "merge_commit_sha": merged.then_some("c0ffee"), "head": { "sha": "deadbeef" } })
}

pub(super) fn check(name: &str, status: &str, conclusion: &str) -> Value {
    json!({ "name": name, "status": status, "conclusion": conclusion })
}

pub(super) fn issue(number: u64, state: &str, login: &str, body: &str) -> Value {
    json!({ "number": number, "title": format!("Issue {number}"), "body": body,
            "state": state, "user": { "login": login } })
}

pub(super) async fn fake_github() -> (String, Arc<FakeGitHub>) {
    let fake = Arc::new(FakeGitHub {
        issues: Mutex::new((String::new(), Vec::new())),
        asked: Mutex::new(Vec::new()),
        pulls: Mutex::new(Pulls {
            opened: Vec::new(),
            merged: Vec::new(),
            pull: pull("open", false),
            check_runs: Vec::new(),
            reviews: Vec::new(),
        }),
    });
    let app = axum::Router::new()
        .route(
            "/repos/acme/widgets/installation",
            get(|| async { axum::Json(json!({ "id": 77 })) }),
        )
        .route(
            "/app/installations/77/access_tokens",
            post(|| async {
                let expires =
                    time::OffsetDateTime::now_utc() + std::time::Duration::from_secs(3600);
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
            "/repos/acme/widgets/issues",
            get(
                |State(fake): State<Arc<FakeGitHub>>, headers: HeaderMap| async move {
                    let asked = headers
                        .get("if-none-match")
                        .map(|v| v.to_str().unwrap().to_string());
                    fake.asked.lock().unwrap().push(asked.clone());
                    let (etag, issues) = fake.issues.lock().unwrap().clone();
                    if asked.as_deref() == Some(etag.as_str()) {
                        return StatusCode::NOT_MODIFIED.into_response();
                    }
                    ([("etag", etag)], axum::Json(issues)).into_response()
                },
            ),
        )
        .route(
            "/repos/acme/widgets/pulls",
            post(
                |State(fake): State<Arc<FakeGitHub>>, axum::Json(body): axum::Json<Value>| async move {
                    fake.pulls.lock().unwrap().opened.push(body);
                    (StatusCode::CREATED, axum::Json(json!({ "number": 7 })))
                },
            ),
        )
        .route(
            "/repos/acme/widgets/pulls/{number}",
            get(
                |State(fake): State<Arc<FakeGitHub>>, UrlPath(number): UrlPath<u64>| async move {
                    assert_eq!(number, 7);
                    axum::Json(fake.pulls.lock().unwrap().pull.clone())
                },
            ),
        )
        .route(
            "/repos/acme/widgets/commits/{sha}/check-runs",
            get(
                |State(fake): State<Arc<FakeGitHub>>, UrlPath(sha): UrlPath<String>| async move {
                    let pulls = fake.pulls.lock().unwrap();
                    assert_eq!(
                        pulls.pull["head"]["sha"], sha,
                        "the checks of the pull request's head"
                    );
                    let runs = pulls.check_runs.clone();
                    axum::Json(json!({ "total_count": runs.len(), "check_runs": runs }))
                },
            ),
        )
        .route(
            "/repos/acme/widgets/pulls/{number}/reviews",
            get(|State(fake): State<Arc<FakeGitHub>>| async move {
                axum::Json(fake.pulls.lock().unwrap().reviews.clone())
            }),
        )
        .route(
            "/repos/acme/widgets/pulls/{number}/merge",
            axum::routing::put(
                |State(fake): State<Arc<FakeGitHub>>, axum::Json(body): axum::Json<Value>| async move {
                    fake.pulls.lock().unwrap().merged.push(body);
                    axum::Json(json!({ "sha": "c0ffee", "merged": true }))
                },
            ),
        )
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (url, fake)
}

/// Starts a run that never does anything.
pub(super) struct IdleRuns;

impl Runs for IdleRuns {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
        _: Option<&str>,
    ) -> Result<Run, String> {
        app.store
            .add_run(Some(card.id), agent, "idle", purpose)
            .map_err(|e| e.to_string())
    }

    fn has_agent(&self, _: &str) -> bool {
        false
    }

    fn steer(&self, _: i64, _: &str) -> bool {
        false
    }

    fn stop(&self, _: i64) -> bool {
        false
    }
}

/// A server polling the fake GitHub for the repository `acme/widgets`, whose trusted author is `leon`.
pub(super) struct Fixture {
    pub(super) _dir: tempfile::TempDir,
    pub(super) server: Server,
    pub(super) fake: Arc<FakeGitHub>,
    pub(super) engine: Arc<Engine>,
    pub(super) repository: Repository,
    /// The bare repository the clone's `origin` is.
    pub(super) remote: std::path::PathBuf,
}

impl Fixture {
    pub(super) async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let (url, fake) = fake_github().await;
        let key = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/github-app-test-key.pem");
        let text = format!(
            "[github]\napp_id = 4242\nprivate_key_file = {:?}\napi_url = {url:?}\n",
            key.display().to_string()
        );
        let config = Config::parse(&text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
        let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        git(
            &origin,
            &["commit", "--quiet", "--allow-empty", "-m", "first"],
        )
        .unwrap();
        let remote = dir.path().join("remote.git");
        git(
            dir.path(),
            &["clone", "--quiet", "--bare", "origin", "remote.git"],
        )
        .unwrap();
        let repository = server
            .app
            .add_github_repository(
                "widgets",
                remote.to_str().unwrap(),
                "acme/widgets",
                &["leon".to_string()],
            )
            .unwrap();
        let engine = Arc::new(Engine::new(
            server.app.clone(),
            Arc::new(IdleRuns),
            crate::board::Workflow::default(),
            crate::board::Limits::default(),
        ));
        Self {
            _dir: dir,
            server,
            fake,
            engine,
            repository,
            remote,
        }
    }

    /// Polls issue 1 into a card, which waits in review with a succeeded `code` run that added `feature.txt`.
    pub(super) async fn card_in_review(&self) -> Card {
        self.fake
            .set_issues("\"v1\"", vec![issue(1, "open", "leon", "Do it.")]);
        self.server.poll_github().await;
        let card = self.card(1).unwrap();
        self.code(&card, "feature.txt").await;
        self.server
            .app
            .store
            .move_card(card.id, "In review")
            .unwrap();
        card
    }

    /// Has a succeeded `code` run of the card add `file` on top of its work so far, as a round of the Coder does.
    pub(super) async fn code(&self, card: &Card, file: &'static str) {
        let (app, repository, card) = (
            self.server.app.clone(),
            self.repository.clone(),
            card.clone(),
        );
        tokio::task::spawn_blocking(move || {
            let store = &app.store;
            let run = store
                .add_run(Some(card.id), "Coder", "test", "code")
                .unwrap();
            let clone = Path::new(&repository.path);
            let base = store
                .code_branch(card.id)
                .unwrap()
                .unwrap_or_else(|| repository.base_ref());
            let worktree = app
                .repos
                .add_worktree("widgets", clone, run.id, &base)
                .unwrap();
            std::fs::write(worktree.path.join(file), "done\n").unwrap();
            git(&worktree.path, &["add", "."]).unwrap();
            git(&worktree.path, &["commit", "--quiet", "-m", file]).unwrap();
            app.repos.remove_worktree(clone, &worktree.path).unwrap();
            store.set_run_status(run.id, "succeeded").unwrap();
        })
        .await
        .unwrap();
    }

    /// Runs the card's column script until it leaves the card, on a thread that may block, as the board does.
    pub(super) async fn settle(&self, card: &Card) -> Step {
        let (engine, id) = (self.engine.clone(), card.id);
        tokio::task::spawn_blocking(move || engine.settle(id).unwrap())
            .await
            .unwrap()
    }

    /// Polls issue 1 into a card and has its work offered as a change.
    pub(super) async fn offered_change(&self) -> Change {
        let card = self.card_in_review().await;
        let app = self.server.app.clone();
        tokio::task::spawn_blocking(move || app.offer_change(card.id).unwrap())
            .await
            .unwrap()
    }

    pub(super) fn change(&self, id: i64) -> Change {
        self.server.app.store.change(id).unwrap().unwrap()
    }

    pub(super) fn card(&self, number: i64) -> Option<Card> {
        self.server
            .app
            .store
            .card_by_number(self.repository.id, number)
            .unwrap()
    }
}
