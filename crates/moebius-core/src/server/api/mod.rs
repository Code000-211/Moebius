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
//! - `ports`: the ports that runs and terminals listen on, and a WebSocket that carries a connection to one.
//! - `whereabouts`: where the server runs and what the desktop app reports about itself.
//! - `events`: the WebSocket on which every event of the core arrives.
//! - `session`: signing in from a browser with a cookie, and the one-time codes that open the web app signed in.
//! - `web`: the web app itself, served at `/` without a token.
//! - `webhooks`: the deliveries of the forges, which need no token but a signature, and the secrets to set on the forges.
//!
//! Every request to `/v1/` but a forge's webhook delivery and the sign-in of a browser carries
//! `Authorization: Bearer <token>`: the server's token, or the run token of a run of the Moebius agent, which acts as
//! that agent (`PUT /v1/secrets/{name}` and the terminal stay with a person); or the `moebius_session` cookie of a
//! browser that signed in, which acts as a person. Anything else is answered with 401.

mod automations;
mod cards;
mod chats;
mod events;
mod harness;
mod improve;
mod lessons;
mod ports;
mod repositories;
mod rules;
mod runs;
mod session;
mod settings;
mod terminals;
mod uploads;
mod web;
mod webhooks;
mod whereabouts;

#[cfg(test)]
mod testing;

pub use cards::{
    AttemptView, Board, CardView, ChangeView, EditedCard, Merged, NewIssue, RunView, Snapshot,
};
pub use chats::{ChatMessage, ConfirmedIssues, NewChat, NewChatSettings};
pub use improve::{DayUsage, UsageAnswer, UsageTotals};
pub use repositories::{EditedRepository, NewRepository};
pub use runs::Steer;
pub use session::{CODE_LIFETIME, LoginCode, Sessions, login_url};
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
    /// The browsers that signed in, and the one-time codes that sign one in.
    pub sessions: Sessions,
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
    // A new file with its own mode replaces whatever was there, so no one who held the old one open can read the token.
    crate::server::settings::write_atomically(path, token.as_bytes(), true)?;
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
        .merge(ports::routes())
        .merge(events::routes())
        .merge(webhooks::routes())
        .merge(whereabouts::routes())
        .merge(session::routes())
        .fallback(web::fallback)
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .merge(webhooks::public_routes())
        .merge(session::public_routes())
        .with_state(state)
}

/// Serves the API on `listener` until the future is dropped.
pub async fn serve(listener: tokio::net::TcpListener, state: ApiState) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}

/// How a request proved who it is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Via {
    Bearer,
    Cookie,
}

/// Who the request is and how it proved it: a person with the server's token or with the cookie of a session, or the
/// run of the Moebius agent whose unrevoked run token it is. A bearer token that is not valid is refused even with a
/// valid cookie.
fn identify(state: &ApiState, headers: &axum::http::HeaderMap) -> Option<(Actor, Via)> {
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match given {
        Some(token) if same(token.as_bytes(), state.token.as_bytes()) => {
            Some((Actor::Person, Via::Bearer))
        }
        Some(_) => agent_of(state, headers).map(|actor| (actor, Via::Bearer)),
        None => session::cookie_of(headers)
            .filter(|id| state.sessions.has(id))
            .map(|_| (Actor::Person, Via::Cookie)),
    }
}

/// Lets a request through as the actor it names (see [`identify`]). Anything else under `/v1/` is refused; the web app
/// outside it needs nothing. A request that only a cookie vouches for has to be one a foreign page cannot make: a
/// change carries `X-Moebius: 1`, and an `Origin` must be this server's own.
async fn authenticate(State(state): State<ApiState>, mut request: Request, next: Next) -> Response {
    let path = request.uri().path();
    if path != "/v1" && !path.starts_with("/v1/") {
        return next.run(request).await;
    }
    match identify(&state, request.headers()) {
        Some((actor, via)) => {
            if via == Via::Cookie && !same_site(&request) {
                return (StatusCode::FORBIDDEN, "cross-site request refused").into_response();
            }
            request.extensions_mut().insert(actor);
            next.run(request).await
        }
        None => (StatusCode::UNAUTHORIZED, "a valid token is needed").into_response(),
    }
}

/// Whether a request that a cookie vouches for came from the web app: a change has the header `X-Moebius: 1`, and an
/// upgrade to a WebSocket or a change that has an `Origin` has the server's own host in it.
fn same_site(request: &Request) -> bool {
    let headers = request.headers();
    let upgrade = headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let safe = matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    );
    if !safe && headers.get("x-moebius").and_then(|v| v.to_str().ok()) != Some("1") {
        return false;
    }
    if !(upgrade || !safe) {
        return true;
    }
    match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(origin) => {
            let origin = origin.split_once("://").map_or(origin, |(_, rest)| rest);
            session::host_of(headers).is_some_and(|host| host.eq_ignore_ascii_case(origin))
        }
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

    // A Unix mode check; Windows has no modes (see `platform`).
    #[cfg(unix)]
    #[test]
    fn a_token_file_is_private_also_when_an_empty_one_was_there() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let held = std::fs::File::open(&path).unwrap();
        load_or_create_token(&path).unwrap();
        // A descriptor opened before the server started does not see the token.
        let mut seen = String::new();
        std::io::Read::read_to_string(&mut &held, &mut seen).unwrap();
        assert_eq!(seen, "");
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        let fresh = dir.path().join("fresh");
        load_or_create_token(&fresh).unwrap();
        assert_eq!(mode(&fresh), 0o600);
    }
}
