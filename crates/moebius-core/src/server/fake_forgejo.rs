//! A fake Forgejo for the tests of the server, a server that polls it, and the tests of the Forgejo forge through it:
//! one repository, `acme/widgets`, with its issues, pull request 7, the statuses of its head commit and its reviews.
//! Every request must carry the access token.

use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::{Path as UrlPath, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use serde_json::{Value, json};

use super::Server;
use super::fake_github::IdleRuns;
use crate::board::{Engine, Step};
use crate::config::Config;
use crate::repos::git;
use crate::store::{Card, Change, Repository, Run};

const TOKEN: &str = "0123456789abcdef0123456789abcdef01234567";

/// A Forgejo with one repository, `acme/widgets`.
#[derive(Default)]
pub(super) struct FakeForgejo {
    /// The issues and pull requests the issues list holds, and the ETag they are served with, and the `If-None-Match`
    /// and `type` of every request for them.
    pub(super) issues: Mutex<(String, Vec<Value>)>,
    pub(super) asked: Mutex<Vec<(Option<String>, String)>>,
    /// The issues it was asked to open, and the edits of issues, as `(number, body)`.
    pub(super) created: Mutex<Vec<Value>>,
    pub(super) edited: Mutex<Vec<(i64, Value)>>,
    /// The pull requests it was asked to open and to merge.
    pub(super) opened: Mutex<Vec<Value>>,
    pub(super) merged: Mutex<Vec<Value>>,
    /// What it answers about pull request 7.
    pub(super) pull_request: Mutex<Value>,
    /// The statuses of each commit, as `(sha, statuses)`.
    pub(super) statuses: Mutex<Vec<(String, Vec<Value>)>>,
    pub(super) reviews: Mutex<Vec<Value>>,
}

impl FakeForgejo {
    pub(super) fn set_issues(&self, etag: &str, issues: Vec<Value>) {
        *self.issues.lock().unwrap() = (etag.to_string(), issues);
    }

    /// Makes pull request 7 `open` or `closed`, and merged or not.
    pub(super) fn set_state(&self, state: &str, merged: bool) {
        let mut pull_request = self.pull_request.lock().unwrap();
        pull_request["state"] = json!(state);
        pull_request["merged"] = json!(merged);
    }

    /// Makes the head of pull request 7 the commit `sha`, as a push does.
    pub(super) fn set_head(&self, sha: &str) {
        self.pull_request.lock().unwrap()["head"]["sha"] = json!(sha);
    }
}

pub(super) fn issue(number: i64, state: &str, login: &str, body: &str) -> Value {
    json!({ "number": number, "title": format!("Issue {number}"), "body": body,
            "state": state, "user": { "login": login }, "pull_request": null })
}

pub(super) fn status(context: &str, state: &str) -> Value {
    json!({ "context": context, "status": state, "description": format!("{context} {state}"),
            "target_url": format!("https://codeberg/{context}") })
}

/// Refuses a request without the access token, as Forgejo does.
async fn token(request: Request, next: Next) -> Response {
    match request.headers().get("authorization") {
        Some(token) if *token == format!("token {TOKEN}") => next.run(request).await,
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

/// Answers 404 for any repository but `acme/widgets`.
fn widgets(owner: &str, repo: &str) -> Result<(), StatusCode> {
    (owner == "acme" && repo == "widgets")
        .then_some(())
        .ok_or(StatusCode::NOT_FOUND)
}

type Fake = State<Arc<FakeForgejo>>;
type Repo = UrlPath<(String, String)>;
type Numbered = UrlPath<(String, String, i64)>;

pub(super) async fn fake_forgejo() -> (String, Arc<FakeForgejo>) {
    let fake = Arc::new(FakeForgejo::default());
    *fake.pull_request.lock().unwrap() = json!({ "number": 7, "state": "open", "merged": false,
        "merge_commit_sha": null, "head": { "sha": "deadbeef" } });
    let app = axum::Router::new()
        .route(
            "/api/v1/repos/{owner}/{repo}/issues",
            get(
                |State(fake): Fake,
                 UrlPath((owner, repo)): Repo,
                 headers: HeaderMap,
                 axum::extract::Query(query): axum::extract::Query<
                    std::collections::HashMap<String, String>,
                >| async move {
                    widgets(&owner, &repo)?;
                    let asked = headers
                        .get("if-none-match")
                        .map(|v| v.to_str().unwrap().to_string());
                    let kind = query.get("type").cloned().unwrap_or_default();
                    fake.asked
                        .lock()
                        .unwrap()
                        .push((asked.clone(), kind.clone()));
                    let (etag, issues) = fake.issues.lock().unwrap().clone();
                    if asked.as_deref() == Some(etag.as_str()) {
                        return Ok(StatusCode::NOT_MODIFIED.into_response());
                    }
                    let issues: Vec<Value> = issues
                        .into_iter()
                        .filter(|i| kind != "issues" || i["pull_request"].is_null())
                        .collect();
                    Ok::<_, StatusCode>(([("etag", etag)], axum::Json(issues)).into_response())
                },
            )
            .post(
                |State(fake): Fake,
                 UrlPath((owner, repo)): Repo,
                 axum::Json(body): axum::Json<Value>| async move {
                    widgets(&owner, &repo)?;
                    let mut created = fake.created.lock().unwrap();
                    created.push(body);
                    Ok::<_, StatusCode>((
                        StatusCode::CREATED,
                        axum::Json(json!({ "number": 40 + created.len() })),
                    ))
                },
            ),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/issues/{number}",
            patch(
                |State(fake): Fake,
                 UrlPath((owner, repo, number)): Numbered,
                 axum::Json(body): axum::Json<Value>| async move {
                    widgets(&owner, &repo)?;
                    fake.edited.lock().unwrap().push((number, body));
                    Ok::<_, StatusCode>(axum::Json(json!({ "number": number })))
                },
            ),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/pulls",
            post(
                |State(fake): Fake,
                 UrlPath((owner, repo)): Repo,
                 axum::Json(body): axum::Json<Value>| async move {
                    widgets(&owner, &repo)?;
                    fake.opened.lock().unwrap().push(body);
                    Ok::<_, StatusCode>((StatusCode::CREATED, axum::Json(json!({ "number": 7 }))))
                },
            ),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/pulls/{number}",
            get(
                |State(fake): Fake, UrlPath((owner, repo, number)): Numbered| async move {
                    widgets(&owner, &repo)?;
                    assert_eq!(number, 7);
                    Ok::<_, StatusCode>(axum::Json(fake.pull_request.lock().unwrap().clone()))
                },
            ),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/commits/{sha}/status",
            get(
                |State(fake): Fake,
                 UrlPath((owner, repo, sha)): UrlPath<(String, String, String)>| async move {
                    widgets(&owner, &repo)?;
                    let statuses = fake.statuses.lock().unwrap();
                    let of = statuses
                        .iter()
                        .find(|(s, _)| *s == sha)
                        .map(|(_, l)| l.clone());
                    let of = of.unwrap_or_default();
                    Ok::<_, StatusCode>(axum::Json(json!({ "sha": sha, "total_count": of.len(),
                                                           "statuses": of })))
                },
            ),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/pulls/{number}/reviews",
            get(
                |State(fake): Fake, UrlPath((owner, repo, _)): Numbered| async move {
                    widgets(&owner, &repo)?;
                    Ok::<_, StatusCode>(axum::Json(fake.reviews.lock().unwrap().clone()))
                },
            ),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/pulls/{number}/merge",
            post(
                |State(fake): Fake,
                 UrlPath((owner, repo, _)): Numbered,
                 axum::Json(body): axum::Json<Value>| async move {
                    widgets(&owner, &repo)?;
                    fake.merged.lock().unwrap().push(body);
                    let mut pull_request = fake.pull_request.lock().unwrap();
                    pull_request["state"] = json!("closed");
                    pull_request["merged"] = json!(true);
                    pull_request["merge_commit_sha"] = json!("c0ffee");
                    Ok::<_, StatusCode>(StatusCode::OK)
                },
            ),
        )
        .layer(middleware::from_fn(token))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (url, fake)
}

/// A server polling the fake Forgejo for the repository `acme/widgets`, whose trusted author is `leon`.
pub(super) struct Fixture {
    pub(super) _dir: tempfile::TempDir,
    /// The fake's web address.
    pub(super) url: String,
    pub(super) server: Server,
    pub(super) fake: Arc<FakeForgejo>,
    pub(super) engine: Arc<Engine>,
    pub(super) repository: Repository,
    /// The bare repository the clone's `origin` is.
    pub(super) remote: std::path::PathBuf,
}

impl Fixture {
    pub(super) async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let (url, fake) = fake_forgejo().await;
        std::fs::write(dir.path().join("forgejo-token"), format!("{TOKEN}\n")).unwrap();
        let text = format!("[forgejo]\nurl = {url:?}\ntoken_file = \"forgejo-token\"\n");
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
            .add_project_repository(
                "widgets",
                remote.to_str().unwrap(),
                "forgejo",
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
            url,
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
        self.server.poll_forgejo().await;
        let card = self.card(1).unwrap();
        let (app, repository, id) = (self.server.app.clone(), self.repository.clone(), card.id);
        tokio::task::spawn_blocking(move || {
            let store = &app.store;
            let run: Run = store.add_run(Some(id), "Coder", "test", "code").unwrap();
            let clone = Path::new(&repository.path);
            let worktree = app
                .repos
                .add_worktree("widgets", clone, run.id, &repository.base_ref())
                .unwrap();
            std::fs::write(worktree.path.join("feature.txt"), "done\n").unwrap();
            git(&worktree.path, &["add", "."]).unwrap();
            git(&worktree.path, &["commit", "--quiet", "-m", "feature"]).unwrap();
            app.repos.remove_worktree(clone, &worktree.path).unwrap();
            store.set_run_status(run.id, "succeeded").unwrap();
            store.move_card(id, "In review").unwrap();
        })
        .await
        .unwrap();
        card
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

mod tests {
    use super::*;
    use crate::decide::Actor;
    use crate::repos::forge::{Forge, ForgejoForge};

    #[tokio::test]
    async fn issues_become_cards_trusted_by_login_without_pull_requests_and_an_unchanged_list_changes_nothing()
     {
        let f = Fixture::new().await;
        let mut pull = issue(5, "open", "leon", "A pull request");
        pull["pull_request"] = json!({ "merged": false });
        f.fake.set_issues(
            "\"v1\"",
            vec![
                issue(1, "open", "leon", "Do it. Blocked by #3, #4"),
                issue(2, "open", "mallory", "Do it my way."),
                issue(3, "closed", "leon", "Long done."),
                pull,
            ],
        );
        f.server.poll_forgejo().await;

        assert!(f.card(3).is_none(), "a closed issue gets no card");
        assert!(f.card(5).is_none(), "a pull request gets no card");
        let (trusted, outside) = (f.card(1).unwrap(), f.card(2).unwrap());
        assert_eq!(
            (trusted.title.as_str(), trusted.author.as_str()),
            ("Issue 1", "leon")
        );
        let app = &f.server.app;
        assert_eq!(app.store.dependencies(trusted.id).unwrap(), [3, 4]);
        assert!(app.is_trusted(&trusted));
        assert!(!app.is_trusted(&outside));
        f.engine.settle(outside.id).unwrap();
        assert!(
            app.store
                .has_open_inbox_item(outside.id, "approve_issue")
                .unwrap()
        );

        let mut events = app.bus.subscribe();
        f.server.poll_forgejo().await;
        assert_eq!(
            *f.fake.asked.lock().unwrap(),
            [
                (None, "issues".to_string()),
                (Some("\"v1\"".to_string()), "issues".to_string())
            ]
        );
        assert!(events.try_recv().is_err(), "a 304 changes nothing");

        f.fake.set_issues(
            "\"v2\"",
            vec![
                issue(1, "closed", "leon", ""),
                issue(2, "open", "mallory", ""),
            ],
        );
        f.server.poll_forgejo().await;
        assert_eq!(f.card(1).unwrap().column, "Done");
        assert!(app.store.dependencies(trusted.id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_repository_that_cannot_be_polled_is_reported() {
        let f = Fixture::new().await;
        let app = &f.server.app;
        app.store
            .add_project_repository(
                "missing",
                &f.repository.path,
                "main",
                "forgejo",
                "acme/missing",
                &[],
            )
            .unwrap();
        f.fake
            .set_issues("\"v1\"", vec![issue(1, "open", "leon", "")]);
        let mut events = app.bus.subscribe();
        f.server.poll_forgejo().await;
        assert_eq!(events.try_recv().unwrap().kind, "card_added");
        let error = events.try_recv().unwrap();
        assert_eq!(
            (error.kind.as_str(), error.data["repository"].as_str()),
            ("forge_error", Some("acme/missing"))
        );
        assert!(
            error.data["message"].as_str().unwrap().contains("404"),
            "{}",
            error.data
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_offered_change_is_pushed_and_opened_as_a_pull_request_and_a_merge_there_finishes_it()
     {
        let f = Fixture::new().await;
        let change = f.offered_change().await;

        assert!(git(&f.remote, &["rev-parse", "--verify", "moebius/card-1"]).is_ok());
        assert_eq!(f.change(change.id).pull_request, Some(7));
        let opened = f.fake.opened.lock().unwrap().clone();
        assert_eq!(opened.len(), 1);
        assert_eq!(
            (
                opened[0]["title"].as_str(),
                opened[0]["head"].as_str(),
                opened[0]["base"].as_str()
            ),
            (Some("Issue 1"), Some("moebius/card-1"), Some("main"))
        );
        assert!(
            opened[0]["body"]
                .as_str()
                .unwrap()
                .starts_with("Closes #1\n")
        );
        assert_eq!(f.card(1).unwrap().column, "Ready to merge");

        f.fake.set_state("closed", true);
        f.fake.pull_request.lock().unwrap()["merge_commit_sha"] = json!("abc123");
        let mut events = f.server.app.bus.subscribe();
        f.server.poll_forgejo().await;
        assert_eq!(f.change(change.id).state, "merged");
        assert_eq!(f.card(1).unwrap().column, "Done");
        let merged = std::iter::from_fn(|| events.try_recv().ok())
            .find(|e| e.kind == "change_merged")
            .unwrap();
        assert_eq!(merged.data["commit"], "abc123");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pull_request_closed_without_a_merge_closes_the_change() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;
        f.fake.set_state("closed", false);
        f.server.poll_forgejo().await;
        assert_eq!(f.change(change.id).state, "closed");
        let inbox = f.server.app.store.inbox().unwrap();
        assert_eq!(
            inbox.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
            ["change_closed"]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_statuses_of_the_head_and_reviews_are_recorded_and_failed_ones_send_the_card_back()
    {
        let f = Fixture::new().await;
        let card = f.card_in_review().await;
        let step = f.settle(&card).await;
        assert_eq!(step.waiting.as_deref(), Some("waiting for the checks"));
        let change = f.server.app.store.open_change(card.id).unwrap().unwrap();
        let head = change.head.clone().unwrap();

        // Statuses of an older commit do not count, and one of the head that runs is pending.
        f.fake.set_head(&head);
        *f.fake.statuses.lock().unwrap() = vec![
            (
                head.clone(),
                vec![status("ci/test", "pending"), status("ci/lint", "success")],
            ),
            ("older".into(), vec![status("ci/test", "failure")]),
        ];
        *f.fake.reviews.lock().unwrap() = vec![
            json!({ "state": "REQUEST_CHANGES", "dismissed": false }),
            json!({ "state": "APPROVED", "dismissed": false }),
        ];
        f.server.poll_forgejo().await;
        let stored = f.change(change.id);
        assert_eq!(
            (
                stored.checks.as_deref(),
                stored.checks_head.as_deref(),
                stored.review.as_deref()
            ),
            (Some("pending"), Some(head.as_str()), Some("approved"))
        );
        let step = f.settle(&card).await;
        assert_eq!(step.waiting.as_deref(), Some("waiting for the checks"));

        // A status fails: the card goes back, told which check failed and why.
        *f.fake.statuses.lock().unwrap() = vec![(
            head.clone(),
            vec![status("ci/test", "failure"), status("ci/lint", "success")],
        )];
        f.server.poll_forgejo().await;
        assert_eq!(
            f.change(change.id).failed_checks.as_deref(),
            Some("ci/test: failure, ci/test failure (https://codeberg/ci/test)")
        );
        f.settle(&card).await;
        let card = f.server.app.card(card.id).unwrap();
        assert_eq!(card.column, "In progress");
        assert!(
            card.note
                .unwrap()
                .starts_with("The checks failed:\nci/test: failure")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_person_merges_the_pull_request_and_issues_are_opened_and_edited_on_forgejo() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;
        let app = f.server.app.clone();
        let repository = f.repository.id;
        let (commit, created, edited) = tokio::task::spawn_blocking(move || {
            let commit = app
                .merge_change(&Actor::Person, change.id, "looks good")
                .unwrap();
            let created = app
                .create_issue(repository, "New", "Blocked by #1", "person")
                .unwrap();
            let edited = app
                .edit_card(&Actor::Person, created.id, "Newer", "Blocked by #2", "")
                .unwrap();
            (commit, created, edited)
        })
        .await
        .unwrap();

        assert_eq!(commit, "c0ffee");
        let merged = f.fake.merged.lock().unwrap().clone();
        assert_eq!(
            merged[0],
            json!({ "Do": "merge", "MergeTitleField": "Merge #1: Issue 1" })
        );
        assert_eq!(f.card(1).unwrap().column, "Done");

        assert_eq!(created.number, 41);
        assert_eq!(f.fake.created.lock().unwrap()[0]["title"], "New");
        let edits = f.fake.edited.lock().unwrap().clone();
        assert_eq!(edits[0].0, 41);
        assert_eq!(edits[0].1["body"], "Blocked by #2");
        assert_eq!(f.server.app.store.dependencies(edited.id).unwrap(), [2]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn git_reaches_this_forgejo_with_the_token_and_nothing_else() {
        let f = Fixture::new().await;
        let (repos, forgejo, url) = (
            f.server.app.repos.clone(),
            f.server.forgejo.clone().unwrap(),
            f.url.clone(),
        );
        let app = f.server.app.clone();
        let (on_it, elsewhere, local, through_app) = tokio::task::spawn_blocking(move || {
            let forge = ForgejoForge::new(forgejo, repos, tokio::runtime::Handle::current());
            let on_it = forge.auth(&format!("{url}/acme/widgets.git"), "acme/widgets");
            let elsewhere = forge.auth("https://codeberg.org/acme/widgets.git", "acme/widgets");
            let local = forge.auth("/srv/widgets.git", "acme/widgets");
            let through_app = app.git_auth(&format!("{url}/acme/harness.git"));
            (
                on_it.unwrap(),
                elsewhere.unwrap(),
                local.unwrap(),
                through_app.unwrap(),
            )
        })
        .await
        .unwrap();
        let expected = format!(
            "Authorization: Basic {}",
            base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                format!("{TOKEN}:x-oauth-basic")
            )
        );
        assert_eq!(on_it.as_deref(), Some(expected.as_str()));
        assert_eq!(through_app, on_it);
        assert_eq!((elsewhere, local), (None, None));
    }
}
