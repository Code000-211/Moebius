//! The desktop API (spec 2 and 3): HTTP and a WebSocket event stream on the private address, behind a token.
//!
//! - `GET /v1/health`: the server's version.
//! - `GET /v1/snapshot`: every repository with its cards, and the open inbox.
//! - `GET /v1/events`: a WebSocket on which every [`AppEvent`] arrives as one JSON text message.
//! - `POST /v1/repositories` `{name, url}`: clones a repository without a forge. With `forge: "github"`, `github:
//!   "owner/repo"` and optionally `trusted: [logins]` it is a repository whose issues are polled from GitHub.
//! - `POST /v1/repositories/{id}/issues` `{title, body}`: creates an issue, which becomes a card in the backlog.
//! - `POST /v1/cards/{id}/approve` `{reason}`: a person approves an outside issue; its card moves to ready.
//! - `POST /v1/changes/{id}/merge` `{reason}`: a person merges a change; its card moves to done.
//! - `POST /v1/changes/{id}/reject` `{reason}`: a person sends a change waiting to be merged back to coding.
//! - `POST /v1/cards/{id}/retry` `{reason}`: a person retries a stuck card, which gets a fresh start.
//! - `POST /v1/runs/{id}/steer` `{message}`: a person sends a message to a running run.
//! - `POST /v1/runs/{id}/stop` `{reason}`: a person stops a running run.
//!
//! Every request carries `Authorization: Bearer <token>`; anything else is answered with 401.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as UrlPath, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;

use crate::app::{App, AppError, PERSON};
use crate::board::Runs;
use crate::bus::AppEvent;
use crate::forge::ForgeError;
use crate::repos::GitError;
use crate::store::{Card, Change, InboxItem, Repository, Run};

/// What the API needs from the core.
#[derive(Clone)]
pub struct ApiState {
    pub app: Arc<App>,
    /// The harness, for steering and stopping runs.
    pub runs: Arc<dyn Runs>,
    pub token: Arc<str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub repository: Repository,
    pub cards: Vec<Card>,
    /// Why each waiting card waits, by card id.
    pub waiting: BTreeMap<i64, String>,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub boards: Vec<Board>,
    pub inbox: Vec<InboxItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewRepository {
    pub name: String,
    pub url: String,
    /// `none` (the default) or `github`.
    #[serde(default)]
    pub forge: Option<String>,
    /// The repository on GitHub as `owner/repo`; needed for the forge `github`.
    #[serde(default)]
    pub github: Option<String>,
    /// The GitHub logins whose issues need no approval.
    #[serde(default)]
    pub trusted: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewIssue {
    pub title: String,
    #[serde(default)]
    pub body: String,
}

/// The body of a person's decision: why they made it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    #[serde(default)]
    pub reason: String,
}

/// The body of a person's message to a running run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Steer {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Merged {
    pub commit: String,
}

/// An operation's failure as the app sees it: a status and a message.
pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        let status = match e {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Forge(ForgeError::Git(GitError::Conflict { .. })) => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self(status, e.to_string())
    }
}

/// Runs a blocking operation on the core off the async threads.
async fn blocking<T: Send + 'static>(
    app: &Arc<App>,
    f: impl FnOnce(&App) -> crate::app::Result<T> + Send + 'static,
) -> Result<axum::Json<T>, ApiError> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || f(&app))
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(axum::Json)
        .map_err(ApiError::from)
}

/// Reads the token from `path`, or creates a new random one there (readable by the owner only).
pub fn load_or_create_token(path: &Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(token) if !token.trim().is_empty() => Ok(token.trim().to_string()),
        Ok(_) => create_token(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => create_token(path),
        Err(e) => Err(e),
    }
}

fn create_token(path: &Path) -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    let token = hex::encode(bytes);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, &token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(token)
}

/// The API's routes, ready to be served.
pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/snapshot", get(snapshot))
        .route("/v1/events", get(events))
        .route("/v1/repositories", post(add_repository))
        .route("/v1/repositories/{id}/issues", post(create_issue))
        .route("/v1/cards/{id}/approve", post(approve_issue))
        .route("/v1/cards/{id}/retry", post(retry_card))
        .route("/v1/changes/{id}/merge", post(merge_change))
        .route("/v1/changes/{id}/reject", post(reject_change))
        .route("/v1/runs/{id}/steer", post(steer_run))
        .route("/v1/runs/{id}/stop", post(stop_run))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state)
}

/// Serves the API on `listener` until the future is dropped.
pub async fn serve(listener: tokio::net::TcpListener, state: ApiState) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}

async fn authenticate(State(state): State<ApiState>, request: Request, next: Next) -> Response {
    let given = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match given {
        Some(token) if same(token.as_bytes(), state.token.as_bytes()) => next.run(request).await,
        _ => (StatusCode::UNAUTHORIZED, "a valid token is needed").into_response(),
    }
}

/// Compares in time that depends only on the lengths.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn health() -> axum::Json<Health> {
    axum::Json(Health {
        version: crate::VERSION.to_string(),
    })
}

async fn snapshot(State(state): State<ApiState>) -> Result<axum::Json<Snapshot>, ApiError> {
    blocking(&state.app, |app| {
        let mut boards = Vec::new();
        for repository in app.store.repositories()? {
            let cards = app.store.cards(repository.id)?;
            boards.push(Board {
                waiting: app.waiting(&cards),
                cards,
                changes: app.store.changes(repository.id)?,
                repository,
            });
        }
        Ok(Snapshot {
            boards,
            inbox: app.store.inbox()?,
        })
    })
    .await
}

async fn add_repository(
    State(state): State<ApiState>,
    axum::Json(new): axum::Json<NewRepository>,
) -> Result<axum::Json<Repository>, ApiError> {
    blocking(&state.app, move |app| {
        match (new.forge.as_deref(), new.github.as_deref()) {
            (None | Some("none"), None) if new.trusted.is_empty() => {
                app.add_repository(&new.name, &new.url)
            }
            (Some("github"), Some(github)) => {
                app.add_github_repository(&new.name, &new.url, github, &new.trusted)
            }
            _ => Err(AppError::Invalid(
                "a repository is either without a forge, or has forge \"github\" with github \"owner/repo\""
                    .into(),
            )),
        }
    })
    .await
}

async fn create_issue(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(issue): axum::Json<NewIssue>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.create_issue(id, &issue.title, &issue.body, PERSON)
    })
    .await
}

async fn approve_issue(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(approval): axum::Json<Reason>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.approve_issue(id, &approval.reason)
    })
    .await
}

async fn merge_change(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(merge): axum::Json<Reason>,
) -> Result<axum::Json<Merged>, ApiError> {
    blocking(&state.app, move |app| {
        app.merge_change(id, &merge.reason)
            .map(|commit| Merged { commit })
    })
    .await
}

async fn retry_card(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(retry): axum::Json<Reason>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| app.retry_card(id, &retry.reason)).await
}

async fn reject_change(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(rejection): axum::Json<Reason>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.reject_change(id, &rejection.reason)
    })
    .await
}

async fn steer_run(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(steer): axum::Json<Steer>,
) -> Result<axum::Json<Run>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.steer_run(&*runs, id, &steer.message)
    })
    .await
}

async fn stop_run(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(stop): axum::Json<Reason>,
) -> Result<axum::Json<Run>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.stop_run(&*runs, id, &stop.reason)
    })
    .await
}

async fn events(State(state): State<ApiState>, upgrade: WebSocketUpgrade) -> Response {
    let rx = state.app.bus.subscribe();
    upgrade.on_upgrade(move |socket| stream(socket, rx))
}

async fn stream(mut socket: WebSocket, mut rx: tokio::sync::broadcast::Receiver<AppEvent>) {
    loop {
        tokio::select! {
            event = rx.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(RecvError::Lagged(missed)) => AppEvent::new("lagged", serde_json::json!({ "missed": missed })),
                    Err(RecvError::Closed) => return,
                };
                let Ok(text) = serde_json::to_string(&event) else { continue };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    return;
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{Engine, Limits, Workflow};
    use crate::config::AgentConfig;
    use crate::pibolt::PiBolt;
    use crate::repos::git;
    use crate::router::{Protocol, Router};
    use crate::store::Store;
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    async fn start() -> (tempfile::TempDir, ApiState, String) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let repo = store
            .add_repository("radiance", "/srv/radiance", "none", "main")
            .unwrap();
        store
            .add_card(repo.id, "First", "", "leon", "Backlog")
            .unwrap();
        let token = load_or_create_token(&dir.path().join("token")).unwrap();
        let host = Arc::new(moebius_plugin_api::Host::new(
            std::time::Duration::from_secs(1),
        ));
        let coder = AgentConfig {
            model: "fake".into(),
            thinking: None,
            instructions: None,
            skills: vec![],
            extensions: vec![],
            protocol: Protocol::OpenAi,
            token_budget: None,
        };
        // The harness is the real one over a Pi that is a script.
        let pi = PiBolt::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi"),
            "http://127.0.0.1:9".into(),
            Arc::new(Router::new(store.clone(), vec![])),
            BTreeMap::from([("Coder".to_string(), coder)]),
        );
        let state = ApiState {
            app: Arc::new(App::new(store, host, &dir.path().join("data"))),
            runs: Arc::new(pi),
            token: token.into(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener, state.clone()));
        (dir, state, format!("127.0.0.1:{}", addr.port()))
    }

    /// A git repository with one commit.
    fn origin(dir: &std::path::Path) -> std::path::PathBuf {
        let origin = dir.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        origin
    }

    /// What a Coder run does, by hand: a worktree, a commit, the change offered.
    fn code_and_offer(app: &App, repo: &Repository, card: &Card) -> Change {
        let clone = std::path::PathBuf::from(&repo.path);
        let run = app
            .store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        app.store.set_run_status(run.id, "succeeded").unwrap();
        let wt = app
            .repos
            .add_worktree(&repo.name, &clone, run.id, "main")
            .unwrap();
        std::fs::write(wt.path.join("feature.txt"), "done\n").unwrap();
        git(&wt.path, &["add", "."]).unwrap();
        git(&wt.path, &["commit", "--quiet", "-m", "Add the feature"]).unwrap();
        app.repos.remove_worktree(&clone, &wt.path).unwrap();
        let change = app.offer_change(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Ready to merge");
        change
    }

    async fn post(
        state: &ApiState,
        addr: &str,
        path: String,
        body: serde_json::Value,
    ) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("http://{addr}{path}"))
            .bearer_auth(&*state.token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    /// Waits until `done` holds.
    async fn until(what: &str, done: impl Fn() -> bool) {
        let give_up = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !done() {
            assert!(std::time::Instant::now() < give_up, "never: {what}");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn a_client_reads_health_and_a_snapshot() {
        let (_dir, state, addr) = start().await;
        let client = reqwest::Client::new();
        let get = |path: &str, token: &str| {
            client
                .get(format!("http://{addr}{path}"))
                .bearer_auth(token)
                .send()
        };
        let health: Health = get("/v1/health", &state.token)
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(health.version, crate::VERSION);
        let snap: Snapshot = get("/v1/snapshot", &state.token)
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(snap.boards.len(), 1);
        assert_eq!(snap.boards[0].cards[0].title, "First");

        assert_eq!(get("/v1/snapshot", "wrong").await.unwrap().status(), 401);
        let bare = client
            .get(format!("http://{addr}/v1/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(bare.status(), 401);
    }

    #[tokio::test]
    async fn events_published_by_the_core_reach_the_client() {
        let (_dir, state, addr) = start().await;
        let mut request = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", state.token).parse().unwrap(),
        );
        let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        // The subscription exists once the upgrade is done; publish until the first event arrives.
        let event = AppEvent::new("card_moved", serde_json::json!({"card": 1, "to": "Ready"}));
        let received = loop {
            state.app.bus.publish(event.clone());
            if let Ok(Some(message)) =
                tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await
            {
                break message.unwrap();
            }
        };
        let back: AppEvent = serde_json::from_str(received.to_text().unwrap()).unwrap();
        assert_eq!(back, event);

        let mut wrong = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        wrong
            .headers_mut()
            .insert("authorization", "Bearer nope".parse().unwrap());
        assert!(tokio_tungstenite::connect_async(wrong).await.is_err());
    }

    #[tokio::test]
    async fn an_issue_becomes_a_card_and_its_merged_change_moves_it_to_done() {
        let (dir, state, addr) = start().await;
        let origin = origin(dir.path());

        let client = reqwest::Client::new();
        let post = |path: String, body: serde_json::Value| {
            client
                .post(format!("http://{addr}{path}"))
                .bearer_auth(&*state.token)
                .json(&body)
                .send()
        };
        let repo: Repository = post(
            "/v1/repositories".into(),
            serde_json::json!({"name": "demo", "url": origin.to_str().unwrap()}),
        )
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(
            (repo.forge.as_str(), repo.default_branch.as_str()),
            ("none", "main")
        );
        let card: Card = post(
            format!("/v1/repositories/{}/issues", repo.id),
            serde_json::json!({"title": "Add a feature", "body": "Please."}),
        )
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(card.column, "Backlog");

        let app = state.app.clone();
        let clone = std::path::PathBuf::from(&repo.path);
        let change = code_and_offer(&app, &repo, &card);
        assert_eq!(app.store.inbox().unwrap().len(), 1);

        let merged = post(
            format!("/v1/changes/{}/merge", change.id),
            serde_json::json!({"reason": "looks good"}),
        )
        .await
        .unwrap();
        assert_eq!(merged.status(), 200);
        let merged: Merged = merged.json().await.unwrap();
        assert_eq!(git(&clone, &["rev-parse", "main"]).unwrap(), merged.commit);
        assert!(clone.join("feature.txt").is_file());

        let snap: Snapshot = client
            .get(format!("http://{addr}/v1/snapshot"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let board = snap
            .boards
            .iter()
            .find(|b| b.repository.id == repo.id)
            .unwrap();
        assert_eq!(board.cards[0].column, "Done");
        assert!(board.waiting.is_empty());
        assert_eq!(board.changes[0].state, "merged");
        assert!(snap.inbox.is_empty());
        let judgements = app.store.judgements("merge").unwrap();
        assert_eq!(
            (judgements.len(), judgements[0].decider.as_str()),
            (1, "person")
        );

        let again = post(
            format!("/v1/changes/{}/merge", change.id),
            serde_json::json!({}),
        )
        .await
        .unwrap();
        assert_eq!(again.status(), 422);
    }

    #[tokio::test]
    async fn a_person_retries_a_stuck_card_once() {
        let (_dir, state, addr) = start().await;
        let store = &state.app.store;
        let card = store.cards(1).unwrap().pop().unwrap();
        let retry = |reason: &str| {
            post(
                &state,
                &addr,
                format!("/v1/cards/{}/retry", card.id),
                serde_json::json!({ "reason": reason }),
            )
        };
        // Nothing to retry before the card is stuck, and no such card.
        assert_eq!(retry("again").await.status(), 422);
        let missing = post(
            &state,
            &addr,
            "/v1/cards/99/retry".into(),
            serde_json::json!({}),
        )
        .await;
        assert_eq!(missing.status(), 404);

        let run = store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        store
            .add_inbox_item("stuck", Some(card.id), "First", &serde_json::json!({}))
            .unwrap();
        state
            .app
            .set_waiting(card.id, Some("the round limit is reached".into()));
        let retried: Card = retry("the flaky test is fixed").await.json().await.unwrap();
        assert_eq!(retried.retried_after, Some(run.id));
        assert!(store.inbox().unwrap().is_empty());
        assert!(state.app.waiting(std::slice::from_ref(&card)).is_empty());
        let judgements = store.judgements("retry").unwrap();
        assert_eq!(
            (
                judgements.len(),
                judgements[0].decider.as_str(),
                judgements[0].choice.as_str(),
                judgements[0].reason.as_str(),
                judgements[0].card_id
            ),
            (
                1,
                "person",
                "retry",
                "the flaky test is fixed",
                Some(card.id)
            )
        );
        assert_eq!(retry("again").await.status(), 422);
    }

    #[tokio::test]
    async fn a_person_rejects_a_change_waiting_to_be_merged() {
        let (dir, state, addr) = start().await;
        let app = &state.app;
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let card = app
            .create_issue(repo.id, "Add a feature", "", PERSON)
            .unwrap();
        let reject = |id: i64, reason: &str| {
            post(
                &state,
                &addr,
                format!("/v1/changes/{id}/reject"),
                serde_json::json!({ "reason": reason }),
            )
        };
        assert_eq!(reject(99, "no").await.status(), 404);

        let change = code_and_offer(app, &repo, &card);
        let rejected: Card = reject(change.id, "Add a test.").await.json().await.unwrap();
        assert_eq!(rejected.column, "In progress");
        assert_eq!(rejected.note.as_deref(), Some("Add a test."));
        assert!(app.store.inbox().unwrap().is_empty());
        assert_eq!(app.store.change(change.id).unwrap().unwrap().state, "open");
        let judgements = app.store.judgements("reject").unwrap();
        assert_eq!(
            (
                judgements.len(),
                judgements[0].decider.as_str(),
                judgements[0].reason.as_str()
            ),
            (1, "person", "Add a test.")
        );
        // The card no longer waits to be merged.
        assert_eq!(reject(change.id, "again").await.status(), 422);

        // A change that was merged cannot be rejected either.
        app.merge_change(change.id, "fine").unwrap();
        assert_eq!(reject(change.id, "too late").await.status(), 422);
    }

    #[tokio::test]
    async fn a_person_steers_and_stops_a_run_and_its_card_waits_in_the_inbox() {
        let (dir, state, addr) = start().await;
        let app = state.app.clone();
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let card = app
            .create_issue(repo.id, "Please [hang]", "", PERSON)
            .unwrap();
        let engine = Arc::new(Engine::new(
            app.clone(),
            state.runs.clone(),
            Workflow::default(),
            Limits::default(),
        ));
        let settle = || {
            let engine = engine.clone();
            async move {
                tokio::task::spawn_blocking(move || engine.settle(card.id).unwrap())
                    .await
                    .unwrap()
            }
        };
        let events = |run: i64| app.store.events(run).unwrap();

        // The board starts the Coder, which waits for a person.
        settle().await;
        let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        until("Pi takes the prompt", || events(run.id).len() >= 2).await;
        let steer = |message: &str| {
            post(
                &state,
                &addr,
                format!("/v1/runs/{}/steer", run.id),
                serde_json::json!({ "message": message }),
            )
        };
        assert_eq!(steer("  ").await.status(), 422);
        let steered: Run = steer("Use tabs.").await.json().await.unwrap();
        assert_eq!(steered.status, "running");
        until("Pi answers the steer", || {
            events(run.id)
                .iter()
                .any(|e| e.kind == "response" && e.data["echo"] == "Use tabs.")
        })
        .await;
        let steered = events(run.id).into_iter().find(|e| e.kind == "steered");
        assert_eq!(
            steered.unwrap().data,
            serde_json::json!({ "message": "Use tabs." })
        );

        let stop = |id: i64| {
            post(
                &state,
                &addr,
                format!("/v1/runs/{id}/stop"),
                serde_json::json!({ "reason": "wrong direction" }),
            )
        };
        assert_eq!(stop(run.id).await.status(), 200);
        until("the run ends", || {
            app.store.run(run.id).unwrap().unwrap().status == "stopped"
        })
        .await;

        // The card is not retried by itself; it waits in the inbox for a person.
        let step = settle().await;
        assert_eq!(step.waiting.as_deref(), Some("stopped by a person"));
        settle().await;
        assert_eq!(app.store.runs_for_card(card.id).unwrap().len(), 1);
        let inbox = app.store.inbox().unwrap();
        assert_eq!((inbox.len(), inbox[0].kind.as_str()), (1, "stuck"));

        // What a person did is recorded, and neither applies to a run that is over.
        for kind in ["steer", "stop"] {
            let judgements = app.store.judgements(kind).unwrap();
            assert_eq!(
                (
                    judgements.len(),
                    judgements[0].decider.as_str(),
                    judgements[0].run_id
                ),
                (1, "person", Some(run.id))
            );
        }
        assert_eq!(
            app.store.judgements("stop").unwrap()[0].reason,
            "wrong direction"
        );
        assert_eq!(steer("Too late.").await.status(), 422);
        assert_eq!(stop(run.id).await.status(), 422);
        assert_eq!(stop(99).await.status(), 404);
    }

    #[tokio::test]
    async fn a_repository_name_that_is_taken_is_refused_before_cloning() {
        let (_dir, state, addr) = start().await;
        let response = reqwest::Client::new()
            .post(format!("http://{addr}/v1/repositories"))
            .bearer_auth(&*state.token)
            .json(&serde_json::json!({"name": "radiance", "url": "/nowhere"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 422);
        assert!(response.text().await.unwrap().contains("is taken"));
        assert!(!state.app.data_dir.join("repos/radiance").exists());
    }

    #[tokio::test]
    async fn a_github_repository_is_accepted_with_its_trusted_logins() {
        use crate::repos::git;
        let (dir, state, addr) = start().await;
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        git(
            &origin,
            &["commit", "--quiet", "--allow-empty", "-m", "first"],
        )
        .unwrap();
        let post = |body: serde_json::Value| {
            reqwest::Client::new()
                .post(format!("http://{addr}/v1/repositories"))
                .bearer_auth(&*state.token)
                .json(&body)
                .send()
        };
        let url = origin.to_str().unwrap();

        let added = post(serde_json::json!({
            "name": "widgets", "url": url, "forge": "github",
            "github": "acme/widgets", "trusted": ["leon"],
        }))
        .await
        .unwrap();
        assert_eq!(added.status(), 200);
        let repo: Repository = added.json().await.unwrap();
        assert_eq!(
            (
                repo.forge.as_str(),
                repo.github.as_deref(),
                repo.trusted.as_slice()
            ),
            (
                "github",
                Some("acme/widgets"),
                ["leon".to_string()].as_slice()
            )
        );
        assert_eq!(state.app.store.repository(repo.id).unwrap(), Some(repo));

        for body in [
            serde_json::json!({"name": "a", "url": url, "forge": "github"}),
            serde_json::json!({"name": "b", "url": url, "forge": "github", "github": "widgets"}),
            serde_json::json!({"name": "c", "url": url, "github": "acme/widgets"}),
        ] {
            assert_eq!(post(body).await.unwrap().status(), 422);
        }
    }

    #[test]
    fn a_token_is_created_once_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/token");
        let a = load_or_create_token(&path).unwrap();
        assert_eq!(a.len(), 64);
        assert_eq!(load_or_create_token(&path).unwrap(), a);
    }
}
