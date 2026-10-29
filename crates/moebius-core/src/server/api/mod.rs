//! The desktop API (spec 2 and 3): HTTP and a WebSocket event stream on the private address, behind a token.
//!
//! This module holds what every part shares: [`ApiState`], the token check, the router that merges the routes of
//! the parts, how a failure of the core becomes a status, and `GET /v1/health` (the server's version). Each part of
//! the API is a module of its own, whose doc comment lists its routes:
//!
//! - `cards`: the snapshot of every board, issues, cards and their changes, a card's runs and history.
//! - `repositories`: adding and editing repositories, and their column scripts and files.
//! - `runs`: a run and its events, steering and stopping it.
//! - `chats`: chats with an agent, and what grilling chats ended in.
//! - `automations`: chats a schedule starts, and their firings.
//! - `harness`: the agents, skills and MCP servers of the harness configuration repository, and setting one up.
//! - `rules`: the rules page and the rules in prose.
//! - `settings`: `moebius.toml`, the API keys, the restart, and what the Models, Plugins and Get started pages show.
//! - `improve`: usage, judgements and replays, the Retro and its proposals, the decision model, research and
//!   exports.
//! - `lessons`: the lessons the Retro learned from earlier cards, edited and deleted by a person.
//! - `uploads`: zip files for the Moebius agent to import from.
//! - `terminals`: shells and Pi's terminal interface over a WebSocket.
//! - `events`: the WebSocket on which every event of the core arrives.
//! - `webhooks`: the deliveries of the forges, which need no token but a signature, and the secrets to set on the forges.
//!
//! Every request but a forge's webhook delivery carries `Authorization: Bearer <token>`: the server's token, or the run
//! token of a run of the Moebius agent, which acts as that agent (`PUT /v1/secrets/{name}` and the terminal stay with a person); anything else is
//! answered with 401.

mod automations;
mod cards;
mod chats;
mod events;
mod harness;
mod improve;
mod lessons;
mod repositories;
mod rules;
mod runs;
mod settings;
mod terminals;
mod uploads;
mod webhooks;

#[cfg(test)]
mod testing;

pub use cards::{
    AttemptView, Board, CardView, ChangeView, EditedCard, Merged, NewIssue, RunView, Snapshot,
};
pub use chats::{ChatMessage, ConfirmedIssues, NewChat, NewChatSettings};
pub use improve::{DayUsage, UsageAnswer, UsageTotals};
pub use repositories::{EditedRepository, NewRepository};
pub use runs::Steer;
pub use uploads::{UPLOAD_LIMIT, Upload};

use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};

use crate::app::{App, AppError};
use crate::board::Runs;
use crate::decide::Actor;
use crate::harness::pibolt::MOEBIUS_AGENT;
use crate::harness::repo::HarnessRepo;
use crate::repos::GitError;
use crate::repos::forge::ForgeError;
use crate::server::settings::Live;

/// What the API needs from the core.
#[derive(Clone)]
pub struct ApiState {
    pub app: Arc<App>,
    /// The harness, for steering and stopping runs.
    pub runs: Arc<dyn Runs>,
    /// The harness configuration repository, if one is configured; without it agents are edited in `moebius.toml`.
    pub harness_repo: Option<Arc<HarnessRepo>>,
    pub token: Arc<str>,
    /// The run tokens, one of which a run of the Moebius agent uses to operate Moebius.
    pub router: Arc<crate::router::Router>,
    /// What the server runs with, which the Models and Settings screens show and edit. The rounds a card gets and the
    /// day's token budget, which boards and the usage screen show, are read from it.
    pub config: Arc<Live>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub version: String,
}

/// The body of a person's decision: why they made it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    #[serde(default)]
    pub reason: String,
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
            AppError::Exists(_) | AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Git(GitError::Conflict { .. }) => StatusCode::CONFLICT,
            AppError::Forge(ForgeError::Git(GitError::Conflict { .. })) => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self(status, e.to_string())
    }
}

/// A failure of the server itself: a 500 with the error's message.
fn internal(e: impl ToString) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Runs a blocking operation on the core off the async threads.
async fn blocking<T: Send + 'static>(
    app: &Arc<App>,
    f: impl FnOnce(&Arc<App>) -> crate::app::Result<T> + Send + 'static,
) -> Result<axum::Json<T>, ApiError> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || f(&app))
        .await
        .map_err(internal)?
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
    crate::platform::make_owner_only(path)?;
    Ok(token)
}

/// The API's routes, ready to be served.
pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .merge(cards::routes())
        .merge(repositories::routes())
        .merge(runs::routes())
        .merge(chats::routes())
        .merge(automations::routes())
        .merge(harness::routes())
        .merge(rules::routes())
        .merge(settings::routes())
        .merge(improve::routes())
        .merge(lessons::routes())
        .merge(uploads::routes())
        .merge(terminals::routes())
        .merge(events::routes())
        .merge(webhooks::routes())
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .merge(webhooks::public_routes())
        .with_state(state)
}

/// Serves the API on `listener` until the future is dropped.
pub async fn serve(listener: tokio::net::TcpListener, state: ApiState) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}

/// Lets a request through as the actor its token names: a person with the server's token, or the run of the Moebius agent
/// whose unrevoked run token it is. Any other token is refused.
async fn authenticate(State(state): State<ApiState>, mut request: Request, next: Next) -> Response {
    let given = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let actor = match given {
        Some(token) if same(token.as_bytes(), state.token.as_bytes()) => Some(Actor::Person),
        Some(_) => agent_of(&state, request.headers()),
        None => None,
    };
    match actor {
        Some(actor) => {
            request.extensions_mut().insert(actor);
            next.run(request).await
        }
        None => (StatusCode::UNAUTHORIZED, "a valid token is needed").into_response(),
    }
}

/// The Moebius agent's run that the run token in `headers` belongs to; none for any other token.
fn agent_of(state: &ApiState, headers: &axum::http::HeaderMap) -> Option<Actor> {
    let grant = state.router.grant(headers)?;
    let run = state.app.store.run(grant.run_id).ok()??;
    (run.agent == MOEBIUS_AGENT).then_some(Actor::Agent {
        name: run.agent,
        run: run.id,
    })
}

/// Refuses an agent what only a person may do.
fn person_only(by: &Actor) -> Result<(), ApiError> {
    match by {
        Actor::Person => Ok(()),
        Actor::Agent { .. } => Err(ApiError(
            StatusCode::FORBIDDEN,
            "only a person can do this".into(),
        )),
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

/// The harness configuration repository, or the 422 that says agents are configured in `moebius.toml`.
fn harness_repo(state: &ApiState) -> Result<Arc<HarnessRepo>, AppError> {
    state.harness_repo.clone().ok_or_else(|| {
        AppError::Invalid(
            "agents are configured in moebius.toml, not in a harness repository".into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::testing::start;
    use super::*;

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
        assert_eq!(snap.boards[0].cards[0].card.title, "First");

        assert_eq!(get("/v1/snapshot", "wrong").await.unwrap().status(), 401);
        let bare = client
            .get(format!("http://{addr}/v1/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(bare.status(), 401);
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
