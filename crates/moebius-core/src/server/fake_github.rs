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
use crate::board::{CardRuns, Engine, StartError, Step, Variant};
use crate::config::Config;
use crate::repos::git;
use crate::store::{Card, Change, Repository, Run};

/// A GitHub with one repository, `acme/widgets`: its issues, the ETag they are served with and the
/// `If-None-Match` of every request for them.
pub(super) struct FakeGitHub {
    pub(super) issues: Mutex<(String, Vec<Value>)>,
    pub(super) asked: Mutex<Vec<Option<String>>>,
    pub(super) pulls: Mutex<Pulls>,
    /// The comments it answers for every issue; none makes the request fail.
    pub(super) comments: Mutex<Option<Vec<Value>>>,
    /// How many times the comments of an issue were asked for.
    pub(super) comment_reads: std::sync::atomic::AtomicUsize,
    /// The `Authorization` of every request for something of a repository.
    pub(super) credentials: Mutex<Vec<String>>,
    /// The `x-oauth-scopes` that `GET /user` answers with; none sends no such header, as for a fine-grained token.
    pub(super) scopes: Mutex<Option<String>>,
    /// Whether `GET /user` answers that the token's calls are limited for now (403 with no calls remaining).
    pub(super) limited: std::sync::atomic::AtomicBool,
}

/// The access token the fake accepts at `GET /user`.
pub(super) const TOKEN: &str = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
/// The login the fake says that token belongs to.
pub(super) const TOKEN_LOGIN: &str = "octo-user";

/// The pull requests of the fake: what it was asked to open, update and merge, and what it answers about pull
/// request 7.
pub(super) struct Pulls {
    pub(super) opened: Vec<Value>,
    pub(super) updated: Vec<Value>,
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
        comments: Mutex::new(Some(Vec::new())),
        comment_reads: Default::default(),
        credentials: Mutex::new(Vec::new()),
        scopes: Mutex::new(None),
        limited: std::sync::atomic::AtomicBool::new(false),
        pulls: Mutex::new(Pulls {
            opened: Vec::new(),
            updated: Vec::new(),
            merged: Vec::new(),
            pull: pull("open", false),
            check_runs: Vec::new(),
            reviews: Vec::new(),
        }),
    });
    let app = axum::Router::new()
        .route(
            "/user",
            get(
                |State(fake): State<Arc<FakeGitHub>>, headers: HeaderMap| async move {
                    if fake.limited.load(std::sync::atomic::Ordering::SeqCst) {
                        let mut response = (
                            StatusCode::FORBIDDEN,
                            axum::Json(json!({ "message": "API rate limit exceeded" })),
                        )
                            .into_response();
                        response
                            .headers_mut()
                            .insert("x-ratelimit-remaining", "0".parse().unwrap());
                        return response;
                    }
                    let accepted = headers
                        .get("authorization")
                        .is_some_and(|v| v.to_str().unwrap() == format!("Bearer {TOKEN}"));
                    if !accepted {
                        return (
                            StatusCode::UNAUTHORIZED,
                            axum::Json(json!({ "message": "Bad credentials" })),
                        )
                            .into_response();
                    }
                    let mut response =
                        axum::Json(json!({ "login": TOKEN_LOGIN })).into_response();
                    if let Some(scopes) = fake.scopes.lock().unwrap().clone() {
                        response
                            .headers_mut()
                            .insert("x-oauth-scopes", scopes.parse().unwrap());
                    }
                    response
                },
            ),
        )
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
            "/repos/acme/widgets/issues/{number}/comments",
            get(|State(fake): State<Arc<FakeGitHub>>| async move {
                fake.comment_reads
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match fake.comments.lock().unwrap().clone() {
                    Some(comments) => axum::Json(comments).into_response(),
                    None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                }
            })
            .post(|| async { (StatusCode::CREATED, axum::Json(json!({ "id": 501 }))) }),
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
                    // Pull request 404 was deleted.
                    if number == 404 {
                        return StatusCode::NOT_FOUND.into_response();
                    }
                    assert_eq!(number, 7);
                    axum::Json(fake.pulls.lock().unwrap().pull.clone()).into_response()
                },
            )
            .patch(
                |State(fake): State<Arc<FakeGitHub>>,
                 UrlPath(number): UrlPath<u64>,
                 axum::Json(body): axum::Json<Value>| async move {
                    assert_eq!(number, 7);
                    fake.pulls.lock().unwrap().updated.push(body.clone());
                    axum::Json(body)
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
        .layer(axum::middleware::from_fn_with_state(
            fake.clone(),
            |State(fake): State<Arc<FakeGitHub>>,
             request: axum::extract::Request,
             next: axum::middleware::Next| async move {
                if request.uri().path().starts_with("/repos/acme/widgets/") {
                    let credentials = request
                        .headers()
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or_default()
                        .to_string();
                    fake.credentials.lock().unwrap().push(credentials);
                }
                next.run(request).await
            },
        ))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (url, fake)
}

/// Starts a run that never does anything.
pub(super) struct IdleRuns;

impl CardRuns for IdleRuns {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: (&str, Option<&str>),
        _: Variant,
    ) -> Result<Run, StartError> {
        app.store
            .add_run(Some(card.id), agent, "idle", purpose.0)
            .map_err(|e| StartError::Store(e.to_string()))
    }

    fn has_agent(&self, _: &str) -> bool {
        false
    }

    fn runners(&self) -> Vec<crate::harness::remote::RunnerInfo> {
        Vec::new()
    }

    fn agents(&self) -> Vec<crate::board::AgentInfo> {
        Vec::new()
    }

    fn token_budget(&self, _: &str) -> Option<i64> {
        None
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
    /// The fake's address, `http://127.0.0.1:<port>`.
    pub(super) api: String,
}

impl Fixture {
    pub(super) async fn new() -> Self {
        Self::start(false).await
    }

    /// The same, with Moebius acting as the user of an access token instead of as the App.
    pub(super) async fn with_token() -> Self {
        Self::start(true).await
    }

    async fn start(token: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let (url, fake) = fake_github().await;
        let key = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/github-app-test-key.pem");
        let text = if token {
            std::fs::write(dir.path().join("github-token"), format!("{TOKEN}\n")).unwrap();
            format!("[github]\ntoken_file = \"github-token\"\napi_url = {url:?}\n")
        } else {
            format!(
                "[github]\napp_id = 4242\nprivate_key_file = {:?}\napi_url = {url:?}\n",
                key.display().to_string()
            )
        };
        let config = Config::parse(&text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
        let server = tokio::task::spawn_blocking(move || {
            Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap()
        })
        .await
        .unwrap();
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
            crate::board::Kanban::default(),
            crate::board::Limits::default(),
        ));
        Self {
            _dir: dir,
            server,
            fake,
            engine,
            repository,
            remote,
            api: url,
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

    /// Has the pull request of the card's work opened while the card stays in review, as it is once a round of the
    /// review was approved and before the card moves on.
    pub(super) async fn opened_pull_request(&self, card: &Card) {
        let (app, id) = (self.server.app.clone(), card.id);
        tokio::task::spawn_blocking(move || app.publish_card_change(id, true, None).unwrap())
            .await
            .unwrap();
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::decide::Actor;
    use crate::repos::forge::{Forge, GitHubForge};

    fn credentials(f: &Fixture) -> Vec<String> {
        f.fake.credentials.lock().unwrap().clone()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn with_a_token_issues_are_polled_and_changes_are_opened_and_merged_as_its_user() {
        let f = Fixture::with_token().await;
        let github = f.server.github.clone().unwrap();
        assert!(github.is_token());
        assert_eq!(
            github.login(),
            Some(TOKEN_LOGIN),
            "read from GET /user at start"
        );

        // Polling, pushing the branch, opening the pull request and reading its checks.
        let change = f.offered_change().await;
        assert_eq!(f.fake.pulls.lock().unwrap().opened.len(), 1);
        f.fake.pulls.lock().unwrap().check_runs = vec![check("build", "completed", "success")];
        f.server.poll_github().await;

        // Merging it.
        let app = f.server.app.clone();
        let id = change.id;
        let commit =
            tokio::task::spawn_blocking(move || app.merge_change(&Actor::Person, id, "looks good"))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(commit, "c0ffee");
        assert_eq!(f.fake.pulls.lock().unwrap().merged.len(), 1);

        // Every call for a repository carried the token, and none asked for an installation.
        let carried = credentials(&f);
        assert!(carried.len() >= 4, "{carried:?}");
        assert!(
            carried.iter().all(|c| *c == format!("Bearer {TOKEN}")),
            "{carried:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_pull_request_says_what_the_change_did_and_what_the_review_found() {
        let f = Fixture::new().await;
        let card = f.card_in_review().await;
        let run = f
            .server
            .app
            .store
            .runs_for_card(card.id)
            .unwrap()
            .into_iter()
            .find(|run| run.purpose == "code")
            .unwrap();
        f.server
            .app
            .store
            .set_run_summary(
                run.id,
                "Added the feature.\n\nREPRODUCE: cargo test --test feature\nREPRODUCE-FILE: tests/feature.rs\n",
            )
            .unwrap();
        let (app, id) = (f.server.app.clone(), card.id);
        tokio::task::spawn_blocking(move || {
            app.offer_change_noted(id, Some("Kept the API plain."))
        })
        .await
        .unwrap()
        .unwrap();

        let opened = f.fake.pulls.lock().unwrap().opened.clone();
        assert_eq!(opened.len(), 1);
        let body = opened[0]["body"].as_str().unwrap();
        assert!(
            body.starts_with("Closes #1\n"),
            "the merge closes the issue: {body}"
        );
        assert!(
            body.contains("## What changed\n\nAdded the feature."),
            "{body}"
        );
        assert!(
            !body.contains("REPRODUCE"),
            "the reproduction test is the review's, not the pull request's: {body}"
        );
        assert!(
            body.contains("## Files changed") && body.contains("feature.txt"),
            "{body}"
        );
        assert!(body.contains("## Review\n\nKept the API plain."), "{body}");
        assert!(body.trim_end().ends_with("Made by Moebius."), "{body}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pull_request_is_updated_when_the_branch_moves_on() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;
        assert_eq!(change.pull_request, Some(7));

        // The next round adds to the branch and the change is offered again: the pull request it opened is updated.
        let card = f.card(1).unwrap();
        f.code(&card, "more.txt").await;
        f.opened_pull_request(&card).await;

        let pulls = f.fake.pulls.lock().unwrap();
        assert_eq!(pulls.opened.len(), 1, "no second pull request");
        let updated = &pulls.updated[0]["body"];
        assert!(
            updated.as_str().unwrap().contains("more.txt"),
            "the updated body says what the branch does now: {updated}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn git_gets_the_token_for_the_web_host_only() {
        let f = Fixture::with_token().await;
        let (repos, github) = (f.server.app.repos.clone(), f.server.github.clone().unwrap());
        let host = f.api.trim_start_matches("http://").to_string();
        let (web, elsewhere, local) = tokio::task::spawn_blocking(move || {
            let forge = GitHubForge::new(github, repos, tokio::runtime::Handle::current());
            (
                forge.auth(&format!("https://{host}/acme/widgets.git"), "acme/widgets"),
                forge.auth("https://attacker.example/x.git", "acme/widgets"),
                forge.auth("/srv/widgets.git", "acme/widgets"),
            )
        })
        .await
        .unwrap();
        use base64::Engine;
        let expected =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{TOKEN}"));
        assert_eq!(
            web.unwrap().as_deref(),
            Some(format!("Authorization: Basic {expected}").as_str())
        );
        assert_eq!(elsewhere.unwrap(), None);
        assert_eq!(local.unwrap(), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_comment_written_with_a_token_is_not_stored_twice_when_the_poll_reads_it_back_as_the_user()
     {
        let f = Fixture::with_token().await;
        f.fake
            .set_issues("\"v1\"", vec![issue(1, "open", "leon", "Do it.")]);
        f.server.poll_github().await;
        let card = f.card(1).unwrap();
        let app = f.server.app.clone();
        let id = card.id;
        let written =
            tokio::task::spawn_blocking(move || app.comment_issue(&Actor::Person, id, "Hello."))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(written.author, "person");
        assert_eq!(credentials(&f).last().unwrap(), &format!("Bearer {TOKEN}"));

        // The forge lists it under the token's user.
        let mut changed = issue(1, "open", "leon", "Do it.");
        changed["comments"] = json!(1);
        *f.fake.comments.lock().unwrap() = Some(vec![json!({
            "id": 501, "user": { "login": TOKEN_LOGIN }, "body": "Hello.",
            "created_at": "2026-03-04T05:06:07Z",
        })]);
        f.fake.set_issues("\"v2\"", vec![changed]);
        f.server.poll_github().await;
        let comments = f.server.app.comments(card.id).unwrap();
        assert_eq!(comments.len(), 1, "{comments:?}");
        assert_eq!(
            comments[0].author, "person",
            "known as Moebius's own by the store, not by the login"
        );
    }

    fn start(text: &str, dir: &std::path::Path) -> Result<Server, String> {
        let config = Config::parse(text, dir, &dir.join("moebius.toml")).unwrap();
        Server::new(config, "127.0.0.1:1".parse().unwrap())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_server_does_not_start_with_a_token_github_refuses_or_one_that_cannot_be_read() {
        let (url, fake) = fake_github().await;
        let dir = tempfile::tempdir().unwrap();
        let text = format!("[github]\ntoken_file = \"token\"\napi_url = {url:?}\n");
        let try_start = |text: String, path: std::path::PathBuf| {
            tokio::task::spawn_blocking(move || start(&text, &path).map(|_| ()))
        };

        let error = try_start(text.clone(), dir.path().to_path_buf())
            .await
            .unwrap()
            .unwrap_err();
        assert!(
            error.contains("cannot read") && error.contains("token"),
            "{error}"
        );

        std::fs::write(dir.path().join("token"), "  \n").unwrap();
        let error = try_start(text.clone(), dir.path().to_path_buf())
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error, "the GitHub access token is empty");

        let wrong = "ghp_wrongwrongwrongwrongwrongwrongwrong";
        std::fs::write(dir.path().join("token"), wrong).unwrap();
        let error = try_start(text.clone(), dir.path().to_path_buf())
            .await
            .unwrap()
            .unwrap_err();
        assert!(
            error.contains("GitHub refused the access token (401 Unauthorized): Bad credentials"),
            "{error}"
        );
        assert!(!error.contains(wrong), "the token is not told: {error}");

        // A classic token without the scope repo.
        std::fs::write(dir.path().join("token"), TOKEN).unwrap();
        *fake.scopes.lock().unwrap() = Some("read:org, gist".into());
        let error = try_start(text.clone(), dir.path().to_path_buf())
            .await
            .unwrap()
            .unwrap_err();
        assert!(
            error.contains("read:org, gist") && error.contains("`repo`"),
            "{error}"
        );
        assert!(!error.contains(TOKEN), "{error}");

        // With the scope, and with a secret instead of a file.
        *fake.scopes.lock().unwrap() = Some("gist, repo".into());
        try_start(text, dir.path().to_path_buf())
            .await
            .unwrap()
            .unwrap();
        crate::server::settings::Secrets::new(&dir.path().join(".moebius"))
            .set("github", TOKEN)
            .unwrap();
        let text = format!("[github]\ntoken_secret = \"github\"\napi_url = {url:?}\n");
        try_start(text, dir.path().to_path_buf())
            .await
            .unwrap()
            .unwrap();
        let text = format!("[github]\ntoken_secret = \"nobody\"\napi_url = {url:?}\n");
        let error = try_start(text, dir.path().to_path_buf())
            .await
            .unwrap()
            .unwrap_err();
        assert!(error.contains("the secret nobody is not stored"), "{error}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_server_starts_when_github_limits_the_token_for_now() {
        let (url, fake) = fake_github().await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("token"), TOKEN).unwrap();
        fake.limited
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let text = format!("[github]\ntoken_file = \"token\"\napi_url = {url:?}\n");
        let path = dir.path().to_path_buf();
        tokio::task::spawn_blocking(move || start(&text, &path).map(|_| ()))
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn the_token_is_redacted_wherever_it_would_be_written() {
        let token = "ghe-token-of-no-known-shape-1234567890";
        let _ = crate::repos::github::GitHub::with_token("http://127.0.0.1:1", token);
        assert!(!crate::blobs::redact(&format!("push failed for {token}")).contains(token));
        assert!(!crate::blobs::redact(&format!("Bearer {TOKEN}")).contains(TOKEN));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_pull_request_publishes_the_change_and_opens_its_pull_request_before_the_offer() {
        let f = Fixture::new().await;
        let card = f.card_in_review().await;
        let (app, id) = (f.server.app.clone(), card.id);
        let change = tokio::task::spawn_blocking(move || app.open_pull_request(id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            change.pull_request,
            Some(7),
            "the pull request opened with the publish"
        );
        assert_eq!(f.change(change.id).id, change.id);
        assert_eq!(f.card(1).unwrap().column, "In review", "the card stays");

        // Calling it again opens nothing further: the change already has its pull request.
        let (app, id) = (f.server.app.clone(), card.id);
        let again = tokio::task::spawn_blocking(move || app.open_pull_request(id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(again.id, change.id);
        let pulls = f.fake.pulls.lock().unwrap();
        assert_eq!(pulls.opened.len(), 1, "no second pull request");
        assert!(pulls.updated.is_empty(), "no needless update either");
    }

    /// The review script of the flow a repository can choose: the pull request opens while the review reads the
    /// change, and the offer waits for the forge's checks to be green.
    fn the_review_that_offers_only_a_green_pull_request() -> String {
        r#"function on_card(card)
  moebius.set_label("in-progress", true)
  local run = card.run
  if run ~= nil and (run.status == "starting" or run.status == "running") then
    moebius.wait("reading the change")
    return
  end
  if run == nil or (run.purpose ~= "code" and run.purpose ~= "review")
    or (run.purpose == "code" and run.status ~= "succeeded") then
    moebius.move("In progress")
    return
  end
  if card.review == nil or card.review.verdict ~= "approve" then
    moebius.publish_change()
    moebius.open_pull_request()
    moebius.start_run("Reviewer", "review")
    moebius.wait("reading the change")
    return
  end
  local change = moebius.run_checks()
  if change.checks == "pending" then
    moebius.wait("waiting for the checks of the change")
  elseif change.checks == "failure" then
    moebius.move("In progress", "The checks failed:\n"
      .. (change.failed_checks or change.check_output or ""))
  elseif change.conflict ~= nil then
    moebius.move("In progress", "The change no longer merges cleanly into " .. change.base .. ".")
  else
    moebius.offer_change()
  end
end"#
            .to_string()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_review_script_lets_the_forges_checks_run_while_it_reads_the_change() {
        use crate::harness::repo::testing::{put, use_harness};
        let f = Fixture::new().await;
        use_harness(&f.server.app, f._dir.path());
        put(
            &f.server.app,
            "widgets",
            "review.luau",
            &the_review_that_offers_only_a_green_pull_request(),
        );
        let card = f.card_in_review().await;

        // The script published the change and opened its pull request while the review read it; the checks are not
        // in yet.
        let step = f.settle(&card).await;
        assert_eq!(step.waiting.as_deref(), Some("reading the change"));
        {
            let pulls = f.fake.pulls.lock().unwrap();
            assert_eq!(pulls.opened.len(), 1, "the pull request opened in review");
        }
        let opened = f.server.app.store.open_change(card.id).unwrap().unwrap();
        assert_eq!(opened.pull_request, Some(7));

        // The review approves, and the offer waits for the forge's checks.
        let review = f
            .server
            .app
            .store
            .runs_for_card(card.id)
            .unwrap()
            .into_iter()
            .find(|run| run.purpose == "review")
            .unwrap();
        f.server
            .app
            .store
            .set_run_summary(review.id, "VERDICT: approve")
            .unwrap();
        f.server
            .app
            .store
            .set_run_status(review.id, "succeeded")
            .unwrap();
        let step = f.settle(&card).await;
        assert_eq!(
            step.waiting.as_deref(),
            Some("waiting for the checks of the change")
        );
        assert_eq!(f.card(1).unwrap().column, "In review");

        // The check run of the pull request's head commit fails, and the card goes back to In progress instead of
        // being offered.
        let change = f.server.app.store.open_change(card.id).unwrap().unwrap();
        f.fake.pulls.lock().unwrap().check_runs = vec![check("build", "completed", "failure")];
        f.fake.set_head(change.head.as_deref().unwrap());
        f.server.poll_github().await;
        f.settle(&card).await;
        let done = f.card(1).unwrap();
        assert_eq!(done.column, "In progress");
        assert!(
            done.note
                .as_deref()
                .unwrap_or_default()
                .contains("The checks failed:"),
            "the card went back with the failed checks, note {:#?}",
            done.note
        );
        assert_eq!(
            f.fake.pulls.lock().unwrap().opened.len(),
            1,
            "the failure opened no second pull request"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_review_script_offers_the_change_once_the_forges_checks_are_green() {
        use crate::harness::repo::testing::{put, use_harness};
        let f = Fixture::new().await;
        use_harness(&f.server.app, f._dir.path());
        put(
            &f.server.app,
            "widgets",
            "review.luau",
            &the_review_that_offers_only_a_green_pull_request(),
        );
        let card = f.card_in_review().await;

        f.settle(&card).await; // publish + open_pull_request + start the review
        let review = f
            .server
            .app
            .store
            .runs_for_card(card.id)
            .unwrap()
            .into_iter()
            .find(|run| run.purpose == "review")
            .unwrap();
        f.server
            .app
            .store
            .set_run_summary(review.id, "VERDICT: approve")
            .unwrap();
        f.server
            .app
            .store
            .set_run_status(review.id, "succeeded")
            .unwrap();
        f.settle(&card).await; // the offer waits for the checks
        let change = f.server.app.store.open_change(card.id).unwrap().unwrap();
        f.fake.pulls.lock().unwrap().check_runs = vec![check("build", "completed", "success")];
        f.fake.set_head(change.head.as_deref().unwrap());
        f.server.poll_github().await;

        let step = f.settle(&card).await;
        // The offer moved the card itself, so the step only shows the wait of the merge script that followed it.
        assert!(step.moved.is_none());
        assert_eq!(
            step.waiting.as_deref(),
            Some("waiting for a person to merge")
        );
        assert_eq!(f.card(1).unwrap().column, "Ready to merge");
        let pulls = f.fake.pulls.lock().unwrap();
        assert_eq!(pulls.opened.len(), 1, "the offer opened nothing further");
    }
}
