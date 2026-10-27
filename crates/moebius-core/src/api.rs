//! The desktop API (spec 2 and 3): HTTP and a WebSocket event stream on the private address, behind a token.
//!
//! - `GET /v1/health`: the server's version.
//! - `GET /v1/snapshot`: every repository with its cards, and the open inbox.
//! - `GET /v1/events`: a WebSocket on which every [`AppEvent`] arrives as one JSON text message.
//! - `POST /v1/repositories` `{name, url}`: clones a repository without a forge.
//! - `POST /v1/repositories/{id}/issues` `{title, body}`: creates an issue, which becomes a card in the backlog.
//! - `POST /v1/changes/{id}/merge` `{reason}`: a person merges a change; its card moves to done.
//!
//! Every request carries `Authorization: Bearer <token>`; anything else is answered with 401.

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

use crate::app::{App, AppError};
use crate::bus::AppEvent;
use crate::store::{Card, Change, InboxItem, Repository};

/// What the API needs from the core.
#[derive(Clone)]
pub struct ApiState {
    pub app: Arc<App>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewIssue {
    pub title: String,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Merge {
    #[serde(default)]
    pub reason: String,
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
            AppError::Git(crate::repos::GitError::Conflict { .. }) => StatusCode::CONFLICT,
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
        .route("/v1/changes/{id}/merge", post(merge_change))
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
            boards.push(Board {
                cards: app.store.cards(repository.id)?,
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
        app.add_repository(&new.name, &new.url)
    })
    .await
}

async fn create_issue(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(issue): axum::Json<NewIssue>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.create_issue(id, &issue.title, &issue.body, "person")
    })
    .await
}

async fn merge_change(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(merge): axum::Json<Merge>,
) -> Result<axum::Json<Merged>, ApiError> {
    blocking(&state.app, move |app| {
        app.merge_change(id, &merge.reason)
            .map(|commit| Merged { commit })
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
        let state = ApiState {
            app: Arc::new(App::new(store, host, &dir.path().join("data"))),
            token: token.into(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener, state.clone()));
        (dir, state, format!("127.0.0.1:{}", addr.port()))
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
        use crate::repos::git;
        let (dir, state, addr) = start().await;
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();

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

        // What a Coder run does, by hand: a worktree, a commit, the change offered.
        let app = state.app.clone();
        let clone = std::path::PathBuf::from(&repo.path);
        let wt = app.repos.add_worktree("demo", &clone, 1, "main").unwrap();
        std::fs::write(wt.path.join("feature.txt"), "done\n").unwrap();
        git(&wt.path, &["add", "."]).unwrap();
        git(&wt.path, &["commit", "--quiet", "-m", "Add the feature"]).unwrap();
        app.repos.remove_worktree(&clone, &wt).unwrap();
        let change = app.offer_change(card.id, &wt.branch).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Ready to merge");
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

    #[test]
    fn a_token_is_created_once_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/token");
        let a = load_or_create_token(&path).unwrap();
        assert_eq!(a.len(), 64);
        assert_eq!(load_or_create_token(&path).unwrap(), a);
    }
}
