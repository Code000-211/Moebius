//! A fake GitLab for the tests of the server, a server that polls it, and the tests of the GitLab forge through it:
//! one project, `acme/widgets`, with its issues, merge request 7, its pipelines, their failed jobs and its approvals.
//! Every request must carry the access token.

use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::{Path as UrlPath, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use serde_json::{Value, json};

use super::Server;
use super::fake_github::IdleRuns;
use crate::board::{Engine, Step};
use crate::config::Config;
use crate::repos::git;
use crate::store::{Card, Change, Repository, Run};

const TOKEN: &str = "glpat-moebius-test-token";

/// A GitLab with one project, `acme/widgets`.
#[derive(Default)]
pub(super) struct FakeGitLab {
    /// The issues and the ETag they are served with, and the `If-None-Match` of every request for them.
    pub(super) issues: Mutex<(String, Vec<Value>)>,
    pub(super) asked: Mutex<Vec<Option<String>>>,
    /// The issues it was asked to open, and the edits of issues, as `(iid, body)`.
    pub(super) created: Mutex<Vec<Value>>,
    pub(super) edited: Mutex<Vec<(i64, Value)>>,
    /// The merge requests it was asked to open and to merge.
    pub(super) opened: Mutex<Vec<Value>>,
    pub(super) merged: Mutex<Vec<Value>>,
    /// What it answers about merge request 7.
    pub(super) merge_request: Mutex<Value>,
    pub(super) pipelines: Mutex<Vec<Value>>,
    /// The failed jobs of each pipeline, as `(pipeline, jobs)`.
    pub(super) jobs: Mutex<Vec<(i64, Vec<Value>)>>,
    pub(super) approved_by: Mutex<Vec<Value>>,
}

impl FakeGitLab {
    pub(super) fn set_issues(&self, etag: &str, issues: Vec<Value>) {
        *self.issues.lock().unwrap() = (etag.to_string(), issues);
    }

    /// Makes merge request 7 `opened`, `closed` or `merged`.
    pub(super) fn set_state(&self, state: &str) {
        self.merge_request.lock().unwrap()["state"] = json!(state);
    }

    /// Makes the head of merge request 7 the commit `sha`, as a push does.
    pub(super) fn set_head(&self, sha: &str) {
        self.merge_request.lock().unwrap()["sha"] = json!(sha);
    }
}

pub(super) fn issue(iid: i64, state: &str, username: &str, description: &str) -> Value {
    json!({ "iid": iid, "title": format!("Issue {iid}"), "description": description,
            "state": state, "author": { "username": username } })
}

pub(super) fn pipeline(id: i64, sha: &str, status: &str) -> Value {
    json!({ "id": id, "sha": sha, "status": status, "web_url": format!("https://gitlab/p/{id}") })
}

/// Refuses a request without the access token, as GitLab does.
async fn token(request: Request, next: Next) -> Response {
    match request.headers().get("private-token") {
        Some(token) if token == TOKEN => next.run(request).await,
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

/// Answers 404 for any project but `acme/widgets`, whose path arrives as one segment.
fn widgets(project: &str) -> Result<(), StatusCode> {
    (project == "acme/widgets")
        .then_some(())
        .ok_or(StatusCode::NOT_FOUND)
}

type Fake = State<Arc<FakeGitLab>>;

pub(super) async fn fake_gitlab() -> (String, Arc<FakeGitLab>) {
    let fake = Arc::new(FakeGitLab::default());
    *fake.merge_request.lock().unwrap() = json!({ "iid": 7, "state": "opened", "sha": "deadbeef",
        "merge_commit_sha": null, "squash_commit_sha": null });
    let app = axum::Router::new()
        .route(
            "/api/v4/projects/{project}/issues",
            get(
                |State(fake): Fake, UrlPath(project): UrlPath<String>, headers: HeaderMap| async move {
                    widgets(&project)?;
                    let asked = headers
                        .get("if-none-match")
                        .map(|v| v.to_str().unwrap().to_string());
                    fake.asked.lock().unwrap().push(asked.clone());
                    let (etag, issues) = fake.issues.lock().unwrap().clone();
                    if asked.as_deref() == Some(etag.as_str()) {
                        return Ok(StatusCode::NOT_MODIFIED.into_response());
                    }
                    Ok::<_, StatusCode>(([("etag", etag)], axum::Json(issues)).into_response())
                },
            )
            .post(
                |State(fake): Fake, UrlPath(project): UrlPath<String>, axum::Json(body): axum::Json<Value>| async move {
                    widgets(&project)?;
                    let mut created = fake.created.lock().unwrap();
                    created.push(body);
                    Ok::<_, StatusCode>((
                        StatusCode::CREATED,
                        axum::Json(json!({ "iid": 40 + created.len() })),
                    ))
                },
            ),
        )
        .route(
            "/api/v4/projects/{project}/issues/{iid}",
            put(
                |State(fake): Fake,
                 UrlPath((project, iid)): UrlPath<(String, i64)>,
                 axum::Json(body): axum::Json<Value>| async move {
                    widgets(&project)?;
                    fake.edited.lock().unwrap().push((iid, body));
                    Ok::<_, StatusCode>(axum::Json(json!({ "iid": iid })))
                },
            ),
        )
        .route(
            "/api/v4/projects/{project}/merge_requests",
            post(
                |State(fake): Fake, UrlPath(project): UrlPath<String>, axum::Json(body): axum::Json<Value>| async move {
                    widgets(&project)?;
                    fake.opened.lock().unwrap().push(body);
                    Ok::<_, StatusCode>((StatusCode::CREATED, axum::Json(json!({ "iid": 7 }))))
                },
            ),
        )
        .route(
            "/api/v4/projects/{project}/merge_requests/{iid}",
            get(
                |State(fake): Fake, UrlPath((project, iid)): UrlPath<(String, i64)>| async move {
                    widgets(&project)?;
                    assert_eq!(iid, 7);
                    Ok::<_, StatusCode>(axum::Json(fake.merge_request.lock().unwrap().clone()))
                },
            ),
        )
        .route(
            "/api/v4/projects/{project}/merge_requests/{iid}/pipelines",
            get(|State(fake): Fake, UrlPath((project, _)): UrlPath<(String, i64)>| async move {
                widgets(&project)?;
                Ok::<_, StatusCode>(axum::Json(fake.pipelines.lock().unwrap().clone()))
            }),
        )
        .route(
            "/api/v4/projects/{project}/pipelines/{id}/jobs",
            get(
                |State(fake): Fake,
                 UrlPath((project, id)): UrlPath<(String, i64)>,
                 axum::extract::RawQuery(query): axum::extract::RawQuery| async move {
                    widgets(&project)?;
                    assert!(query.unwrap_or_default().contains("scope[]=failed"));
                    let jobs = fake.jobs.lock().unwrap();
                    let failed = jobs.iter().find(|(p, _)| *p == id).map(|(_, j)| j.clone());
                    Ok::<_, StatusCode>(axum::Json(failed.unwrap_or_default()))
                },
            ),
        )
        .route(
            "/api/v4/projects/{project}/merge_requests/{iid}/approvals",
            get(|State(fake): Fake, UrlPath((project, _)): UrlPath<(String, i64)>| async move {
                widgets(&project)?;
                let approved_by = fake.approved_by.lock().unwrap().clone();
                Ok::<_, StatusCode>(axum::Json(json!({ "approved": !approved_by.is_empty(),
                                                       "approved_by": approved_by })))
            }),
        )
        .route(
            "/api/v4/projects/{project}/merge_requests/{iid}/merge",
            put(
                |State(fake): Fake,
                 UrlPath((project, _)): UrlPath<(String, i64)>,
                 axum::Json(body): axum::Json<Value>| async move {
                    widgets(&project)?;
                    fake.merged.lock().unwrap().push(body);
                    let mut merged = fake.merge_request.lock().unwrap().clone();
                    merged["state"] = json!("merged");
                    merged["merge_commit_sha"] = json!("c0ffee");
                    Ok::<_, StatusCode>(axum::Json(merged))
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

/// A server polling the fake GitLab for the project `acme/widgets`, whose trusted author is `leon`.
pub(super) struct Fixture {
    pub(super) _dir: tempfile::TempDir,
    /// The fake's web address.
    pub(super) url: String,
    pub(super) server: Server,
    pub(super) fake: Arc<FakeGitLab>,
    pub(super) engine: Arc<Engine>,
    pub(super) repository: Repository,
    /// The bare repository the clone's `origin` is.
    pub(super) remote: std::path::PathBuf,
}

impl Fixture {
    pub(super) async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let (url, fake) = fake_gitlab().await;
        std::fs::write(dir.path().join("gitlab-token"), format!("{TOKEN}\n")).unwrap();
        let text = format!("[gitlab]\nurl = {url:?}\ntoken_file = \"gitlab-token\"\n");
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
                "gitlab",
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
            .set_issues("W/\"v1\"", vec![issue(1, "opened", "leon", "Do it.")]);
        self.server.poll_gitlab().await;
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
    use crate::repos::forge::{Forge, GitLabForge};

    #[tokio::test]
    async fn issues_become_cards_trusted_by_username_and_an_unchanged_list_changes_nothing() {
        let f = Fixture::new().await;
        f.fake.set_issues(
            "W/\"v1\"",
            vec![
                issue(1, "opened", "leon", "Do it. Blocked by #3, #4"),
                issue(2, "opened", "mallory", "Do it my way."),
                issue(3, "closed", "leon", "Long done."),
            ],
        );
        f.server.poll_gitlab().await;

        assert!(f.card(3).is_none(), "a closed issue gets no card");
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
        f.server.poll_gitlab().await;
        assert_eq!(
            *f.fake.asked.lock().unwrap(),
            [None, Some("W/\"v1\"".to_string())]
        );
        assert!(events.try_recv().is_err(), "a 304 changes nothing");

        f.fake.set_issues(
            "W/\"v2\"",
            vec![
                issue(1, "closed", "leon", ""),
                issue(2, "opened", "mallory", ""),
            ],
        );
        f.server.poll_gitlab().await;
        assert_eq!(f.card(1).unwrap().column, "Done");
        assert!(app.store.dependencies(trusted.id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_project_that_cannot_be_polled_is_reported() {
        let f = Fixture::new().await;
        let app = &f.server.app;
        app.store
            .add_project_repository(
                "missing",
                &f.repository.path,
                "main",
                "gitlab",
                "acme/missing",
                &[],
            )
            .unwrap();
        f.fake
            .set_issues("W/\"v1\"", vec![issue(1, "opened", "leon", "")]);
        let mut events = app.bus.subscribe();
        f.server.poll_gitlab().await;
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
    async fn an_offered_change_is_pushed_and_opened_as_a_merge_request_and_a_merge_there_finishes_it()
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
                opened[0]["source_branch"].as_str(),
                opened[0]["target_branch"].as_str()
            ),
            (Some("Issue 1"), Some("moebius/card-1"), Some("main"))
        );
        assert!(
            opened[0]["description"]
                .as_str()
                .unwrap()
                .starts_with("Closes #1\n")
        );
        assert_eq!(f.card(1).unwrap().column, "Ready to merge");

        f.fake.set_state("merged");
        f.fake.merge_request.lock().unwrap()["merge_commit_sha"] = json!("abc123");
        let mut events = f.server.app.bus.subscribe();
        f.server.poll_gitlab().await;
        assert_eq!(f.change(change.id).state, "merged");
        assert_eq!(f.card(1).unwrap().column, "Done");
        let merged = std::iter::from_fn(|| events.try_recv().ok())
            .find(|e| e.kind == "change_merged")
            .unwrap();
        assert_eq!(merged.data["commit"], "abc123");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_merge_request_closed_without_a_merge_closes_the_change() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;
        f.fake.set_state("closed");
        f.server.poll_gitlab().await;
        assert_eq!(f.change(change.id).state, "closed");
        let inbox = f.server.app.store.inbox().unwrap();
        assert_eq!(
            inbox.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
            ["change_closed"]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_pipeline_of_the_head_and_approvals_are_recorded_and_failed_jobs_send_the_card_back()
     {
        let f = Fixture::new().await;
        let card = f.card_in_review().await;
        let step = f.settle(&card).await;
        assert_eq!(step.waiting.as_deref(), Some("waiting for the checks"));
        let change = f.server.app.store.open_change(card.id).unwrap().unwrap();
        let head = change.head.clone().unwrap();

        // A pipeline of an older commit does not count, and one of the head that runs is pending.
        f.fake.set_head(&head);
        *f.fake.pipelines.lock().unwrap() = vec![
            pipeline(12, &head, "running"),
            pipeline(11, "older", "success"),
        ];
        f.fake
            .approved_by
            .lock()
            .unwrap()
            .push(json!({ "user": { "username": "mona" } }));
        f.server.poll_gitlab().await;
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

        // The pipeline fails: the card goes back, told which job failed and why.
        *f.fake.pipelines.lock().unwrap() = vec![pipeline(12, &head, "failed")];
        *f.fake.jobs.lock().unwrap() = vec![(
            12,
            vec![
                json!({ "name": "lint", "status": "failed", "failure_reason": "script_failure",
                         "web_url": "https://gitlab/j/1", "allow_failure": false }),
            ],
        )];
        f.server.poll_gitlab().await;
        assert_eq!(
            f.change(change.id).failed_checks.as_deref(),
            Some("lint: failed, script_failure (https://gitlab/j/1)")
        );
        f.settle(&card).await;
        let card = f.server.app.card(card.id).unwrap();
        assert_eq!(card.column, "In progress");
        assert!(
            card.note
                .unwrap()
                .starts_with("The checks failed:\nlint: failed")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_person_merges_the_merge_request_and_issues_are_opened_and_edited_on_gitlab() {
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
        assert_eq!(merged[0]["merge_commit_message"], "Merge #1: Issue 1");
        assert_eq!(f.card(1).unwrap().column, "Done");

        assert_eq!(created.number, 41);
        assert_eq!(f.fake.created.lock().unwrap()[0]["title"], "New");
        let edits = f.fake.edited.lock().unwrap().clone();
        assert_eq!(edits[0].0, 41);
        assert_eq!(edits[0].1["description"], "Blocked by #2");
        assert_eq!(f.server.app.store.dependencies(edited.id).unwrap(), [2]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn git_reaches_this_gitlab_with_the_token_and_nothing_else() {
        let f = Fixture::new().await;
        let (repos, gitlab, url) = (
            f.server.app.repos.clone(),
            f.server.gitlab.clone().unwrap(),
            f.url.clone(),
        );
        let app = f.server.app.clone();
        let (on_it, elsewhere, local, through_app) = tokio::task::spawn_blocking(move || {
            let forge = GitLabForge::new(gitlab, repos, tokio::runtime::Handle::current());
            let on_it = forge.auth(&format!("{url}/acme/widgets.git"), "acme/widgets");
            let elsewhere = forge.auth("https://gitlab.com/acme/widgets.git", "acme/widgets");
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
                format!("oauth2:{TOKEN}")
            )
        );
        assert_eq!(on_it.as_deref(), Some(expected.as_str()));
        assert_eq!(through_app, on_it);
        assert_eq!((elsewhere, local), (None, None));
    }
}
