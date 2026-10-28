//! The desktop API (spec 2 and 3): HTTP and a WebSocket event stream on the private address, behind a token.
//!
//! - `GET /v1/health`: the server's version.
//! - `GET /v1/snapshot`: every repository with its cards (and what the board shows of each), and the open inbox.
//! - `GET /v1/events`: a WebSocket on which every [`AppEvent`] arrives as one JSON text message.
//! - `POST /v1/repositories` `{name, url}`: clones a repository without a forge. With `forge: "github"`, `github:
//!   "owner/repo"` and optionally `trusted: [logins]` it is a repository whose issues are polled from GitHub.
//! - `POST /v1/repositories/{id}/issues` `{title, body}`: creates an issue, which becomes a card in the backlog.
//! - `POST /v1/cards/{id}/approve` `{reason}`: a person approves an outside issue; its card moves to ready.
//! - `POST /v1/changes/{id}/merge` `{reason}`: a person merges a change; its card moves to done.
//! - `POST /v1/changes/{id}/reject` `{reason}`: a person sends a change waiting to be merged back to coding.
//! - `POST /v1/cards/{id}/retry` `{reason}`: a person retries a stuck card or one whose pull request was closed, which gets a fresh start.
//! - `GET /v1/runs/{id}`: a run. `GET /v1/runs/{id}/events?after=&limit=`: its stored events, in pages.
//! - `GET /v1/cards/{id}/runs`: the runs of a card. `GET /v1/cards/{id}/history`: its timeline.
//! - `GET /v1/judgements?card=&kind=&limit=`: the recorded decisions, newest first.
//! - `POST /v1/runs/{id}/steer` `{message}`: a person sends a message to a running run.
//! - `POST /v1/runs/{id}/stop` `{reason}`: a person stops a running run; a chat ends as stopped.
//! - `GET /v1/runs/{id}/terminal`: a WebSocket on a shell in the run's worktree (see [`crate::terminal`]).
//! - `POST /v1/chats` `{agent?, message, repository?}`: starts a chat, with the Moebius agent unless `agent` names
//!   another. `GET /v1/chats`: the chats, newest first.
//!   `POST /v1/chats/{id}/messages` `{message}`: writes to a chat. `POST /v1/chats/{id}/end`: ends a chat.
//!
//! Every request carries `Authorization: Bearer <token>`; anything else is answered with 401.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as UrlPath, Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;

use crate::app::{App, AppError, Chat, HistoryEntry, PERSON, columns, runs_since_retry};
use crate::board::{Runs, latest_review};
use crate::bus::AppEvent;
use crate::forge::ForgeError;
use crate::pibolt::MOEBIUS_AGENT;
use crate::repos::GitError;
use crate::store::{Card, Change, Event, InboxItem, Repository, Run};

/// What the API needs from the core.
#[derive(Clone)]
pub struct ApiState {
    pub app: Arc<App>,
    /// The harness, for steering and stopping runs.
    pub runs: Arc<dyn Runs>,
    pub token: Arc<str>,
    /// The most `code` rounds a card gets, which boards show next to a card's rounds.
    pub round_limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub repository: Repository,
    pub cards: Vec<CardView>,
    /// The most `code` rounds a card gets.
    pub round_limit: u32,
    /// Why each waiting card waits, by card id.
    pub waiting: BTreeMap<i64, String>,
    pub changes: Vec<Change>,
}

/// A card with what a board shows of it, so that a client needs one read for a board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardView {
    #[serde(flatten)]
    pub card: Card,
    /// The `code` rounds and the tokens since the card was last retried, as column scripts see them.
    pub rounds: i64,
    pub tokens: i64,
    /// The card's latest run.
    pub run: Option<RunView>,
    /// The numbers of the issues that block the card and are not done.
    pub blocked_by: Vec<i64>,
    /// How many tool calls of the latest run were refused.
    pub refusals: i64,
    /// The card's open change.
    pub change: Option<ChangeView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunView {
    pub id: i64,
    pub agent: String,
    pub purpose: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeView {
    pub id: i64,
    pub state: String,
    /// `pending`, `success`, `failure` or `none`, as column scripts see it.
    pub checks: String,
    /// `approve` or `changes` once a review finished since the latest `code` run.
    pub review_verdict: Option<String>,
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

/// The body of a person's message to a running run or a chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Steer {
    pub message: String,
}

/// The body that starts a chat: the agent to talk to (the Moebius agent without one), the first message and optionally
/// the repository to work in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewChat {
    #[serde(default)]
    pub agent: Option<String>,
    pub message: String,
    #[serde(default)]
    pub repository: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Merged {
    pub commit: String,
}

/// How many events of a run one request returns unless it asks for fewer or more.
const DEFAULT_EVENTS: i64 = 500;
/// The most events of a run one request returns.
const MAX_EVENTS: i64 = 5000;

/// Which of a run's events to return: those after `after`, at most `limit`.
#[derive(Debug, Deserialize)]
struct EventsQuery {
    #[serde(default)]
    after: i64,
    limit: Option<i64>,
}

/// How many judgements one request returns unless it asks for fewer or more.
const DEFAULT_JUDGEMENTS: i64 = 100;
/// The most judgements one request returns.
const MAX_JUDGEMENTS: i64 = 1000;

/// Which judgements to return: those of the card and of the kind, if given, at most `limit`.
#[derive(Debug, Deserialize)]
struct JudgementsQuery {
    card: Option<i64>,
    kind: Option<String>,
    limit: Option<i64>,
}

/// A judgement as the API shows it: without the blobs of its context.
#[derive(Debug, Serialize, Deserialize)]
struct JudgementEntry {
    id: i64,
    kind: String,
    choice: String,
    reason: String,
    decider: String,
    outcome: Option<String>,
    card: Option<i64>,
    run: Option<i64>,
    at: i64,
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
    f: impl FnOnce(&Arc<App>) -> crate::app::Result<T> + Send + 'static,
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
        .route("/v1/cards/{id}/runs", get(card_runs))
        .route("/v1/cards/{id}/history", get(card_history))
        .route("/v1/judgements", get(judgements))
        .route("/v1/runs/{id}", get(get_run))
        .route("/v1/runs/{id}/events", get(run_events))
        .route("/v1/runs/{id}/steer", post(steer_run))
        .route("/v1/runs/{id}/stop", post(stop_run))
        .route("/v1/runs/{id}/terminal", get(run_terminal))
        .route("/v1/chats", get(chats).post(start_chat))
        .route("/v1/chats/{id}/messages", post(say_in_chat))
        .route("/v1/chats/{id}/end", post(end_chat))
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

/// What a board shows of a card. A card in Done carries none of it, which keeps the snapshot cheap to read on every
/// event.
fn card_view(app: &App, card: Card) -> crate::app::Result<CardView> {
    let mut view = CardView {
        card,
        rounds: 0,
        tokens: 0,
        run: None,
        blocked_by: Vec::new(),
        refusals: 0,
        change: None,
    };
    if view.card.column == columns::DONE {
        return Ok(view);
    }
    let runs = app.store.runs_for_card(view.card.id)?;
    view.rounds = runs_since_retry(&runs, &view.card)
        .filter(|r| r.purpose == "code")
        .count() as i64;
    view.tokens = app.store.tokens_for_card(view.card.id)?;
    view.blocked_by = app.blocked_by(&view.card)?;
    if let Some(run) = runs.last() {
        view.refusals = app.store.refused_tool_calls(run.id)?;
        view.run = Some(RunView {
            id: run.id,
            agent: run.agent.clone(),
            purpose: run.purpose.clone(),
            status: run.status.clone(),
        });
    }
    if let Some(change) = app.store.open_change(view.card.id)? {
        view.change = Some(ChangeView {
            id: change.id,
            state: change.state.clone(),
            checks: app.checks_of(&change)?.to_string(),
            review_verdict: latest_review(&runs).map(|r| r.verdict.to_string()),
        });
    }
    Ok(view)
}

async fn snapshot(State(state): State<ApiState>) -> Result<axum::Json<Snapshot>, ApiError> {
    let round_limit = state.round_limit;
    blocking(&state.app, move |app| {
        let mut boards = Vec::new();
        for repository in app.store.repositories()? {
            let cards = app.store.cards(repository.id)?;
            let waiting = app.waiting(&cards);
            boards.push(Board {
                waiting,
                cards: cards
                    .into_iter()
                    .map(|card| card_view(app, card))
                    .collect::<Result<_, _>>()?,
                round_limit,
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

async fn run_terminal(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let axum::Json((worktree, slot)) = blocking(&state.app, move |app| app.open_shell(id)).await?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        crate::terminal::serve(socket, &worktree, slot).await
    }))
}

async fn start_chat(
    State(state): State<ApiState>,
    axum::Json(chat): axum::Json<NewChat>,
) -> Result<axum::Json<Run>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        let agent = chat.agent.as_deref().unwrap_or(MOEBIUS_AGENT);
        app.start_chat(&*runs, agent, chat.repository, &chat.message)
    })
    .await
}

async fn chats(State(state): State<ApiState>) -> Result<axum::Json<Vec<Chat>>, ApiError> {
    blocking(&state.app, |app| app.chats()).await
}

async fn say_in_chat(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(said): axum::Json<Steer>,
) -> Result<axum::Json<Run>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.say_in_chat(&*runs, id, &said.message)
    })
    .await
}

async fn end_chat(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Run>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| app.end_chat(&*runs, id)).await
}

async fn get_run(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Run>, ApiError> {
    blocking(&state.app, move |app| app.run(id)).await
}

async fn run_events(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Query(query): Query<EventsQuery>,
) -> Result<axum::Json<Vec<Event>>, ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_EVENTS).clamp(1, MAX_EVENTS);
    blocking(&state.app, move |app| {
        app.run(id)?;
        Ok(app.store.events_after(id, query.after, limit)?)
    })
    .await
}

async fn judgements(
    State(state): State<ApiState>,
    Query(query): Query<JudgementsQuery>,
) -> Result<axum::Json<Vec<JudgementEntry>>, ApiError> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_JUDGEMENTS)
        .clamp(1, MAX_JUDGEMENTS);
    blocking(&state.app, move |app| {
        let found = app
            .store
            .judgements_page(query.card, query.kind.as_deref(), limit)?;
        Ok(found
            .into_iter()
            .map(|j| JudgementEntry {
                id: j.id,
                kind: j.kind,
                choice: j.choice,
                reason: j.reason,
                decider: j.decider,
                outcome: j.outcome,
                card: j.card_id,
                run: j.run_id,
                at: j.at,
            })
            .collect())
    })
    .await
}

async fn card_runs(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Vec<Run>>, ApiError> {
    blocking(&state.app, move |app| {
        app.card(id)?;
        Ok(app.store.runs_for_card(id)?)
    })
    .await
}

async fn card_history(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Vec<HistoryEntry>>, ApiError> {
    blocking(&state.app, move |app| app.card_history(id)).await
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
    use crate::store::{Store, Usage};
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    async fn start() -> (tempfile::TempDir, ApiState, String) {
        start_with(BTreeMap::from([("Coder".to_string(), agent("fake"))])).await
    }

    fn agent(model: &str) -> AgentConfig {
        AgentConfig {
            model: model.into(),
            thinking: None,
            instructions: None,
            skills: vec![],
            extensions: vec![],
            protocol: Protocol::OpenAi,
            token_budget: None,
        }
    }

    /// A server whose harness has the agents `agents`.
    async fn start_with(
        agents: BTreeMap<String, AgentConfig>,
    ) -> (tempfile::TempDir, ApiState, String) {
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
        // The harness is the real one over a Pi that is a script.
        let pi = PiBolt::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi"),
            "/usr/local/bin/moebius".into(),
            "http://127.0.0.1:9".into(),
            Arc::new(Router::new(store.clone(), vec![])),
            agents,
        );
        let state = ApiState {
            app: Arc::new(App::new(store, host, &dir.path().join("data"))),
            runs: Arc::new(pi),
            token: token.into(),
            round_limit: 5,
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
        assert_eq!(snap.boards[0].cards[0].card.title, "First");

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
    async fn the_snapshot_holds_what_a_board_shows_of_each_card() {
        let (dir, state, addr) = start().await;
        let app = &state.app;
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let store = &app.store;
        let blocker = store
            .add_card(repo.id, "Blocker", "", "person", "Ready")
            .unwrap();
        let card = store
            .add_card(repo.id, "Work", "", "person", "In progress")
            .unwrap();
        let done = store
            .add_card(repo.id, "Done", "", "person", "Done")
            .unwrap();
        // Blocked by an open card and by an issue that has no card; the card of a done issue does not block.
        store
            .set_dependencies(card.id, &[blocker.number, 99, done.number].into())
            .unwrap();
        store
            .set_dependencies(done.id, &[blocker.number].into())
            .unwrap();

        // An earlier round, a retry after it, then a round, its review, and a second review that is working.
        let before = store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        store.set_run_status(before.id, "failed").unwrap();
        store.set_card_retried(card.id).unwrap();
        let code = store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        store.set_run_status(code.id, "succeeded").unwrap();
        let review = store
            .add_run(Some(card.id), "Reviewer", "test", "review")
            .unwrap();
        store
            .set_run_summary(review.id, "Looks right.\nVERDICT: approve")
            .unwrap();
        store.set_run_status(review.id, "succeeded").unwrap();
        let live = store
            .add_run(Some(card.id), "Reviewer", "test", "review")
            .unwrap();
        store.set_run_status(live.id, "running").unwrap();
        for (tool, decision) in [("bash", "refuse"), ("read", "allow")] {
            let event = serde_json::json!({"tool": tool, "decision": decision});
            app.record_run_event(live.id, "tool_check", &event).unwrap();
        }
        for (run, input, output) in [(before.id, 1000, 1), (code.id, 100, 50), (live.id, 7, 3)] {
            let usage = Usage {
                run_id: Some(run),
                model: "fake".into(),
                input_tokens: input,
                output_tokens: output,
                cached_tokens: 0,
            };
            store.add_usage(&usage).unwrap();
        }
        let change = store
            .add_change(repo.id, Some(card.id), "moebius/card-2", None, "abc")
            .unwrap();
        store
            .set_local_checks(change.id, "failure", "abc", None)
            .unwrap();
        store
            .add_inbox_item("stuck", Some(card.id), "Work", &serde_json::json!({}))
            .unwrap();

        let snap: Snapshot = reqwest::Client::new()
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
        assert_eq!(board.round_limit, 5);
        let view = |number| {
            board
                .cards
                .iter()
                .find(|c| c.card.number == number)
                .unwrap()
        };

        let shown = view(card.number);
        assert_eq!((shown.rounds, shown.tokens), (1, 160));
        assert_eq!(shown.blocked_by, [blocker.number, 99]);
        // The latest run is the one working, and one of its two tool calls was refused.
        let run = shown.run.as_ref().unwrap();
        assert_eq!(
            (run.id, run.agent.as_str(), run.status.as_str()),
            (live.id, "Reviewer", "running")
        );
        assert_eq!(shown.refusals, 1);
        let change = shown.change.as_ref().unwrap();
        assert_eq!(
            (change.state.as_str(), change.checks.as_str()),
            ("open", "failure")
        );
        assert_eq!(change.review_verdict.as_deref(), Some("approve"));
        assert_eq!(snap.inbox[0].card_id, Some(card.id));

        let blocked = view(blocker.number);
        assert_eq!(
            (
                blocked.rounds,
                blocked.run.as_ref(),
                blocked.change.as_ref()
            ),
            (0, None, None)
        );
        // A card in Done carries no details.
        let finished = view(done.number);
        assert_eq!(
            (finished.rounds, finished.tokens, finished.blocked_by.len()),
            (0, 0, 0)
        );
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
        assert_eq!(board.cards[0].card.column, "Done");
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

    async fn get<T: serde::de::DeserializeOwned>(state: &ApiState, addr: &str, path: &str) -> T {
        let response = reqwest::Client::new()
            .get(format!("http://{addr}{path}"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{path}");
        response.json().await.unwrap()
    }

    async fn connect(state: &ApiState, addr: &str) -> impl StreamExt<Item = AppEvent> + Unpin {
        let mut request = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", state.token).parse().unwrap(),
        );
        let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        ws.map(|message| serde_json::from_str(message.unwrap().to_text().unwrap()).unwrap())
    }

    #[tokio::test]
    async fn a_finished_runs_events_are_read_in_pages() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let run = app.store.add_run(Some(1), "Coder", "test", "code").unwrap();
        for n in 1..=7 {
            app.record_run_event(run.id, "step", &serde_json::json!({ "n": n }))
                .unwrap();
        }
        app.store.set_run_status(run.id, "succeeded").unwrap();

        let stored: Run = get(&state, &addr, &format!("/v1/runs/{}", run.id)).await;
        assert_eq!(stored.status, "succeeded");
        assert!(stored.ended_at.is_some());
        let all: Vec<Event> = get(&state, &addr, &format!("/v1/runs/{}/events", run.id)).await;
        assert_eq!(
            all.iter().map(|e| e.seq).collect::<Vec<_>>(),
            (1..=7).collect::<Vec<_>>()
        );
        assert_eq!(
            (all[2].kind.as_str(), &all[2].data),
            ("step", &serde_json::json!({ "n": 3 }))
        );

        // Pages of three follow on from the last seq without gaps or repeats.
        let mut paged = Vec::new();
        loop {
            let after = paged.last().map_or(0, |e: &Event| e.seq);
            let page: Vec<Event> = get(
                &state,
                &addr,
                &format!("/v1/runs/{}/events?after={after}&limit=3", run.id),
            )
            .await;
            assert!(page.len() <= 3);
            if page.is_empty() {
                break;
            }
            paged.extend(page);
        }
        assert_eq!(paged, all);
    }

    #[tokio::test]
    async fn the_events_of_a_running_run_arrive_on_the_stream_with_their_seq() {
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
        let id = card.id;
        tokio::task::spawn_blocking(move || engine.settle(id).unwrap())
            .await
            .unwrap();
        let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        until("Pi takes the prompt", || {
            app.store.events(run.id).unwrap().len() >= 2
        })
        .await;

        // A client follows the stream, reads what is stored so far, and drops what it has already read.
        let mut stream = connect(&state, &addr).await;
        let path = format!("/v1/runs/{}/events", run.id);
        let mut seen: Vec<Event> = get(&state, &addr, &path).await;
        let read = seen.last().unwrap().seq;
        let steered = post(
            &state,
            &addr,
            format!("/v1/runs/{}/steer", run.id),
            serde_json::json!({ "message": "Use tabs." }),
        )
        .await;
        assert_eq!(steered.status(), 200);
        // Pi's answer may be stored just before or just after the steer itself.
        let steer_arrived = |seen: &[Event]| {
            let after_read = || seen.iter().filter(|e| e.seq > read);
            after_read().any(|e| e.kind == "steered")
                && after_read().any(|e| e.data["echo"] == "Use tabs.")
        };
        while !steer_arrived(&seen) {
            let event = tokio::time::timeout(std::time::Duration::from_secs(20), stream.next())
                .await
                .expect("a run event arrives")
                .unwrap();
            if event.kind == "run_event" && event.data["run"] == run.id {
                let seq = event.data["seq"].as_i64().unwrap();
                if seq > read {
                    seen.push(Event {
                        run_id: run.id,
                        seq,
                        kind: event.data["kind"].as_str().unwrap().into(),
                        data: event.data["data"].clone(),
                        at: 0,
                    });
                }
            }
        }

        // Each event is there exactly once, and what was stored is what the client saw.
        let stored: Vec<Event> = get(&state, &addr, &path).await;
        assert_eq!(
            stored
                .iter()
                .map(|e| (e.seq, &e.kind, &e.data))
                .collect::<Vec<_>>(),
            seen.iter()
                .map(|e| (e.seq, &e.kind, &e.data))
                .collect::<Vec<_>>()
        );
        assert!(stored.windows(2).all(|w| w[0].seq + 1 == w[1].seq));

        let stopped = post(
            &state,
            &addr,
            format!("/v1/runs/{}/stop", run.id),
            serde_json::json!({}),
        )
        .await;
        assert_eq!(stopped.status(), 200);
        until("the run ends", || {
            app.store.run(run.id).unwrap().unwrap().status == "stopped"
        })
        .await;
    }

    #[tokio::test]
    async fn a_card_has_its_runs_and_its_history_in_time_order() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        // An inbox item, then a wait until the clock has moved on, so that time order differs from the order
        // in which the history gathers its parts.
        let item = app
            .store
            .add_inbox_item("stuck", Some(1), "Stuck", &serde_json::json!({}))
            .unwrap();
        let first = item.at;
        until("the next second", || crate::store::now() > first).await;
        app.store.set_card_note(1, "Add a test.").unwrap();
        app.move_card(1, "Ready").unwrap();
        app.move_card(1, "In progress").unwrap();
        let run = app.store.add_run(Some(1), "Coder", "test", "code").unwrap();
        app.store.set_run_status(run.id, "failed").unwrap();
        app.store
            .add_judgement(&crate::store::NewJudgement {
                kind: "retry".into(),
                choice: "retry".into(),
                reason: "again".into(),
                decider: "person".into(),
                card_id: Some(1),
                ..Default::default()
            })
            .unwrap();
        app.store.resolve_inbox_item(item.id).unwrap();

        let runs: Vec<Run> = get(&state, &addr, "/v1/cards/1/runs").await;
        assert_eq!(runs.iter().map(|r| r.id).collect::<Vec<_>>(), [run.id]);
        let history: Vec<HistoryEntry> = get(&state, &addr, "/v1/cards/1/history").await;
        let kinds: Vec<_> = history.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "inbox_added",
                "moved",
                "moved",
                "run_started",
                "run_ended",
                "decided",
                "inbox_resolved"
            ]
        );
        assert!(history.windows(2).all(|w| w[0].at <= w[1].at));
        assert_eq!(
            history[2].data,
            serde_json::json!({ "from": "Ready", "to": "In progress", "note": "Add a test." })
        );
        assert_eq!(
            history[4].data,
            serde_json::json!({ "run": run.id, "status": "failed" })
        );
        assert_eq!(history[5].data["decider"], "person");
        assert_eq!(history[6].data["item"], item.id);
    }

    #[tokio::test]
    async fn judgements_are_read_newest_first_by_card_and_kind() {
        let (_dir, state, addr) = start().await;
        let store = &state.app.store;
        for (n, kind, card) in [
            (1, "retry", Some(1)),
            (2, "tool_call", None),
            (3, "retry", None),
        ] {
            store
                .add_judgement(&crate::store::NewJudgement {
                    kind: kind.into(),
                    choice: format!("choice {n}"),
                    reason: format!("reason {n}"),
                    decider: "person".into(),
                    card_id: card,
                    ..Default::default()
                })
                .unwrap();
        }
        let ids = |path: &str| {
            let path = path.to_string();
            let (state, addr) = (state.clone(), addr.clone());
            async move {
                let found: Vec<serde_json::Value> = get(&state, &addr, &path).await;
                found
                    .iter()
                    .map(|j| j["id"].as_i64().unwrap())
                    .collect::<Vec<_>>()
            }
        };
        assert_eq!(ids("/v1/judgements").await, [3, 2, 1]);
        assert_eq!(ids("/v1/judgements?kind=retry").await, [3, 1]);
        assert_eq!(ids("/v1/judgements?card=1").await, [1]);
        assert_eq!(ids("/v1/judgements?limit=2").await, [3, 2]);
        assert_eq!(ids("/v1/judgements?limit=0").await, [3]);
        let found: Vec<serde_json::Value> = get(&state, &addr, "/v1/judgements?card=1").await;
        assert_eq!(
            found[0],
            serde_json::json!({ "id": 1, "kind": "retry", "choice": "choice 1", "reason": "reason 1",
                "decider": "person", "outcome": null, "card": 1, "run": null, "at": found[0]["at"] })
        );
    }

    #[tokio::test]
    async fn an_unknown_run_or_card_is_not_found() {
        let (_dir, state, addr) = start().await;
        for path in [
            "/v1/runs/99",
            "/v1/runs/99/events",
            "/v1/cards/99/runs",
            "/v1/cards/99/history",
        ] {
            let response = reqwest::Client::new()
                .get(format!("http://{addr}{path}"))
                .bearer_auth(&*state.token)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 404, "{path}");
        }
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

    /// The kinds of the events of a run, in order.
    fn kinds(app: &App, run_id: i64) -> Vec<String> {
        let events = app.store.events(run_id).unwrap();
        events.into_iter().map(|e| e.kind).collect()
    }

    fn status(app: &App, run_id: i64) -> String {
        app.store.run(run_id).unwrap().unwrap().status
    }

    /// Starts a chat with the Coder; returns its run once the agent has answered the first message.
    async fn chat(state: &ApiState, addr: &str, message: &str, repository: Option<i64>) -> Run {
        let started = post(
            state,
            addr,
            "/v1/chats".into(),
            serde_json::json!({ "agent": "Coder", "message": message, "repository": repository }),
        )
        .await;
        assert_eq!(started.status(), 200);
        let run: Run = started.json().await.unwrap();
        assert_eq!((run.purpose.as_str(), run.card_id), ("chat", None));
        until("the agent has answered", || {
            status(&state.app, run.id) == "waiting"
        })
        .await;
        run
    }

    async fn say(state: &ApiState, addr: &str, run_id: i64, message: &str) -> reqwest::Response {
        post(
            state,
            addr,
            format!("/v1/chats/{run_id}/messages"),
            serde_json::json!({ "message": message }),
        )
        .await
    }

    async fn end(state: &ApiState, addr: &str, run_id: i64) -> reqwest::Response {
        post(
            state,
            addr,
            format!("/v1/chats/{run_id}/end"),
            serde_json::json!({}),
        )
        .await
    }

    #[tokio::test]
    async fn a_chat_answers_two_messages_in_a_row_and_keeps_its_history() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let run = chat(&state, &addr, "Hello", None).await;
        assert_eq!(
            kinds(app, run.id),
            [
                "said",
                "response",
                "agent_start",
                "message_update",
                "agent_end",
                "agent_settled"
            ]
        );
        // Without a repository the agent works in an empty directory of its own.
        let empty = app.data_dir.join(format!("chats/{}", run.id));
        assert!(empty.is_dir());

        let said = say(&state, &addr, run.id, "Again").await;
        assert_eq!(said.status(), 200);
        let settled = |n: usize| {
            kinds(app, run.id)
                .iter()
                .filter(|k| *k == "agent_settled")
                .count()
                == n
                && status(app, run.id) == "waiting"
        };
        until("the second answer", || settled(2)).await;
        let events: Vec<Event> = get(&state, &addr, &format!("/v1/runs/{}/events", run.id)).await;
        let words: Vec<(&str, &str)> = events
            .iter()
            .filter_map(|e| match e.kind.as_str() {
                "said" => Some(("said", e.data["message"].as_str()?)),
                "message_update" => Some((
                    "answered",
                    e.data["assistantMessageEvent"]["delta"].as_str()?,
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            words,
            [
                ("said", "Hello"),
                ("answered", "You said: Hello"),
                ("said", "Again"),
                ("answered", "You said: Again"),
            ]
        );

        // What a person says is stored without secrets.
        say(
            &state,
            &addr,
            run.id,
            "My key is sk-ant-abcdefghijklmnop1234",
        )
        .await;
        until("the third answer", || settled(3)).await;
        let third = app.store.events(run.id).unwrap();
        let said = third.iter().rfind(|e| e.kind == "said").unwrap();
        assert_eq!(said.data["message"], "My key is [redacted:anthropic-key]");

        let other = chat(&state, &addr, "Another chat", None).await;
        let listed: Vec<Chat> = get(&state, &addr, "/v1/chats").await;
        assert_eq!(
            listed.iter().map(|c| c.id).collect::<Vec<_>>(),
            [other.id, run.id]
        );
        assert_eq!(
            (
                listed[1].agent.as_str(),
                listed[1].status.as_str(),
                listed[1].title.as_str()
            ),
            ("Coder", "waiting", "Hello")
        );
        assert_eq!(listed[1].repository_id, None);

        // A person ends the chat; its empty directory goes with it.
        assert_eq!(end(&state, &addr, run.id).await.status(), 200);
        until("the chat ends", || status(app, run.id) == "succeeded").await;
        assert!(!empty.exists());
        assert_eq!(end(&state, &addr, other.id).await.status(), 200);
    }

    #[tokio::test]
    async fn a_chat_in_a_repository_commits_its_work_on_its_branch_when_it_ends() {
        let (dir, state, addr) = start().await;
        let app = &state.app;
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let clone = std::path::PathBuf::from(&repo.path);
        let run = chat(&state, &addr, "Make notes [write notes.txt]", Some(repo.id)).await;
        let worktree = app.store.run(run.id).unwrap().unwrap().worktree.unwrap();
        let worktree = std::path::PathBuf::from(worktree);
        assert!(worktree.join("notes.txt").is_file());
        assert_eq!(run.branch(), format!("moebius/chat-{}", run.id));

        assert_eq!(end(&state, &addr, run.id).await.status(), 200);
        until("the chat ends", || status(app, run.id) == "succeeded").await;
        assert!(!worktree.exists());
        assert_eq!(
            git(&clone, &["show", &format!("{}:notes.txt", run.branch())]).unwrap(),
            "written"
        );
        let listed: Vec<Chat> = get(&state, &addr, "/v1/chats").await;
        assert_eq!(listed[0].repository_id, Some(repo.id));

        // A chat that changed nothing has still succeeded; its branch stays where it started.
        let quiet = chat(&state, &addr, "Hello", Some(repo.id)).await;
        assert_eq!(end(&state, &addr, quiet.id).await.status(), 200);
        until("the chat ends", || status(app, quiet.id) == "succeeded").await;
        assert_eq!(
            git(&clone, &["rev-parse", &quiet.branch()]).unwrap(),
            git(&clone, &["rev-parse", "main"]).unwrap()
        );

        // Nothing can be said to a chat that has ended, and it cannot be ended again.
        for id in [run.id, quiet.id] {
            assert_eq!(say(&state, &addr, id, "Hello?").await.status(), 422);
            assert_eq!(end(&state, &addr, id).await.status(), 422);
        }
        assert_eq!(
            kinds(app, run.id).iter().filter(|k| *k == "said").count(),
            1
        );
    }

    #[tokio::test]
    async fn a_chat_that_is_stopped_ends_and_its_agent_is_gone() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let waiting = chat(&state, &addr, "Hello", None).await;
        let pid = std::fs::read_to_string(
            app.data_dir
                .join(format!("runs/{}/agent/pid.txt", waiting.id)),
        )
        .unwrap();
        let stop = |id: i64| {
            post(
                &state,
                &addr,
                format!("/v1/runs/{id}/stop"),
                serde_json::json!({}),
            )
        };
        assert_eq!(stop(waiting.id).await.status(), 200);
        until("the chat is stopped", || {
            status(app, waiting.id) == "stopped"
        })
        .await;
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        assert!(!app.data_dir.join(format!("chats/{}", waiting.id)).exists());
        assert_eq!(stop(waiting.id).await.status(), 422);
        assert_eq!(say(&state, &addr, waiting.id, "Hello?").await.status(), 422);

        // A chat whose agent is working takes a message as a follow-up, and a stop aborts the turn.
        let started = post(
            &state,
            &addr,
            "/v1/chats".into(),
            serde_json::json!({ "agent": "Coder", "message": "Work for ever [hang]" }),
        )
        .await;
        let working: Run = started.json().await.unwrap();
        until("the agent works", || {
            kinds(app, working.id).contains(&"agent_start".to_string())
        })
        .await;
        assert_eq!(status(app, working.id), "running");
        assert_eq!(
            say(&state, &addr, working.id, "And then this.")
                .await
                .status(),
            200
        );
        let queued = app
            .data_dir
            .join(format!("runs/{}/agent/queued.txt", working.id));
        until("Pi queues the message", || {
            app.store
                .events(working.id)
                .unwrap()
                .iter()
                .any(|e| e.data["data"]["disposition"] == "queued")
        })
        .await;
        assert_eq!(std::fs::read_to_string(queued).unwrap(), "And then this.");
        assert_eq!(stop(working.id).await.status(), 200);
        until("the chat is stopped", || {
            status(app, working.id) == "stopped"
        })
        .await;
    }

    #[tokio::test]
    async fn a_chat_needs_a_known_agent_a_message_and_a_chat_to_write_to() {
        let (_dir, state, addr) = start().await;
        for body in [
            serde_json::json!({ "agent": "Nobody", "message": "Hello" }),
            serde_json::json!({ "agent": "Coder", "message": "  " }),
        ] {
            assert_eq!(
                post(&state, &addr, "/v1/chats".into(), body).await.status(),
                422
            );
        }
        let unknown = serde_json::json!({ "agent": "Coder", "message": "Hello", "repository": 99 });
        assert_eq!(
            post(&state, &addr, "/v1/chats".into(), unknown)
                .await
                .status(),
            404
        );
        assert_eq!(say(&state, &addr, 99, "Hello").await.status(), 404);
        assert!(state.app.store.chats().unwrap().is_empty());

        // A run of a card is no chat.
        let run = state
            .app
            .store
            .add_run(Some(1), "Coder", "test", "code")
            .unwrap();
        assert_eq!(say(&state, &addr, run.id, "Hello").await.status(), 422);
        assert_eq!(end(&state, &addr, run.id).await.status(), 422);
    }

    /// The arguments Pi was started with for a run.
    fn argv(app: &App, run_id: i64) -> Vec<String> {
        let file = app.data_dir.join(format!("runs/{run_id}/agent/argv.json"));
        serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
    }

    /// Starts a chat without an agent and returns its run once the agent has answered.
    async fn chat_without_agent(state: &ApiState, addr: &str, repository: Option<i64>) -> Run {
        let body = serde_json::json!({ "message": "Hello", "repository": repository });
        let started = post(state, addr, "/v1/chats".into(), body).await;
        assert_eq!(started.status(), 200);
        let run: Run = started.json().await.unwrap();
        until("the agent has answered", || {
            status(&state.app, run.id) == "waiting"
        })
        .await;
        run
    }

    #[tokio::test]
    async fn a_chat_without_an_agent_is_with_the_moebius_agent_on_the_coders_model() {
        let (_dir, state, addr) = start().await;
        let run = chat_without_agent(&state, &addr, None).await;
        assert_eq!(run.agent, "Moebius");
        let argv = argv(&state.app, run.id);
        assert!(argv.windows(2).any(|w| w == ["--model", "fake"]));
        // Only the Coder's model and protocol carry over.
        assert!(!argv.iter().any(|a| a == "--thinking"));
        let skill = crate::skill::dir(&state.app.data_dir);
        let skills: Vec<_> = argv.windows(2).filter(|w| w[0] == "--skill").collect();
        assert_eq!(skills, [["--skill", skill.to_str().unwrap()]]);
        let instructions = argv.windows(2).find(|w| w[0] == "--append-system-prompt");
        assert!(instructions.unwrap()[1].starts_with("You are the Moebius agent."));

        // Asking for the agent by name is the same.
        let named = serde_json::json!({ "agent": "Moebius", "message": "Hello" });
        let named: Run = post(&state, &addr, "/v1/chats".into(), named)
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(named.agent, "Moebius");
    }

    #[tokio::test]
    async fn the_moebius_agent_configured_has_its_own_model_and_adds_to_what_it_is_told() {
        let own = AgentConfig {
            thinking: Some("high".into()),
            instructions: Some("Prefer small scripts.".into()),
            skills: vec!["/skills/house-style".into()],
            ..agent("own-model")
        };
        // There is no Coder: the Moebius agent needs none.
        let (_dir, state, addr) = start_with(BTreeMap::from([("Moebius".to_string(), own)])).await;
        let run = chat_without_agent(&state, &addr, None).await;
        let argv = argv(&state.app, run.id);
        assert!(argv.windows(2).any(|w| w == ["--model", "own-model"]));
        assert!(argv.windows(2).any(|w| w == ["--thinking", "high"]));
        // The moebius skill always comes first.
        let skill = crate::skill::dir(&state.app.data_dir);
        let skills: Vec<_> = argv.windows(2).filter(|w| w[0] == "--skill").collect();
        assert_eq!(
            skills,
            [
                ["--skill", skill.to_str().unwrap()],
                ["--skill", "/skills/house-style"]
            ]
        );
        let instructions = argv.windows(2).find(|w| w[0] == "--append-system-prompt");
        let instructions = &instructions.unwrap()[1];
        assert!(instructions.starts_with("You are the Moebius agent."));
        assert!(instructions.ends_with("\n\nPrefer small scripts."));
    }

    #[tokio::test]
    async fn the_moebius_agent_without_a_model_to_use_cannot_start_a_chat() {
        let (_dir, state, addr) = start_with(BTreeMap::new()).await;
        let body = serde_json::json!({ "message": "Hello" });
        let refused = post(&state, &addr, "/v1/chats".into(), body).await;
        assert_eq!(refused.status(), 422);
        let message = refused.text().await.unwrap();
        assert!(message.contains("[agents.Moebius]"), "{message}");
        assert!(state.app.store.chats().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_chat_in_a_repository_offers_what_it_made_as_a_change_the_person_merges() {
        let (dir, state, addr) = start().await;
        let app = &state.app;
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let clone = std::path::PathBuf::from(&repo.path);
        let mut events = app.bus.subscribe();
        let write = "Write a script [write .moebius/ready.luau]";
        let started = serde_json::json!({ "message": write, "repository": repo.id });
        let run: Run = post(&state, &addr, "/v1/chats".into(), started)
            .await
            .json()
            .await
            .unwrap();
        until("the agent has answered", || {
            status(app, run.id) == "waiting"
        })
        .await;
        assert!(app.store.changes(repo.id).unwrap().is_empty());
        assert_eq!(end(&state, &addr, run.id).await.status(), 200);
        until("the chat ends", || status(app, run.id) == "succeeded").await;

        // The change has no card, is on the chat's branch and waits in the inbox.
        let changes = app.store.changes(repo.id).unwrap();
        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        assert_eq!(
            (
                change.card_id,
                change.branch.as_str(),
                change.state.as_str()
            ),
            (None, run.branch().as_str(), "open")
        );
        assert_eq!(change.pull_request, None);
        let inbox = app.store.inbox().unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(
            (inbox[0].kind.as_str(), inbox[0].card_id),
            ("ready_to_merge", None)
        );
        assert_eq!(inbox[0].title, write);
        assert_eq!(inbox[0].data["change"], change.id);
        let mut offered = None;
        while let Ok(event) = events.try_recv() {
            if event.kind == "change_offered" {
                offered = Some(event.data["id"].clone());
            }
        }
        assert_eq!(offered, Some(serde_json::json!(change.id)));

        // Merging it puts the script on the default branch and takes the item out of the inbox.
        let merged = post(
            &state,
            &addr,
            format!("/v1/changes/{}/merge", change.id),
            serde_json::json!({}),
        )
        .await;
        assert_eq!(merged.status(), 200);
        assert_eq!(
            git(&clone, &["show", "main:.moebius/ready.luau"]).unwrap(),
            "written"
        );
        assert_eq!(
            git(&clone, &["log", "-1", "--format=%s", "main"]).unwrap(),
            format!("Merge {}", run.branch())
        );
        assert!(app.store.inbox().unwrap().is_empty());
        assert_eq!(
            app.store.change(change.id).unwrap().unwrap().state,
            "merged"
        );

        // A chat that changed nothing offers nothing; a stopped chat offers what it made.
        let quiet = chat(&state, &addr, "Hello", Some(repo.id)).await;
        assert_eq!(end(&state, &addr, quiet.id).await.status(), 200);
        until("the chat ends", || status(app, quiet.id) == "succeeded").await;
        let writing = chat(&state, &addr, "Again [write notes.txt]", Some(repo.id)).await;
        let stop = format!("/v1/runs/{}/stop", writing.id);
        let stopped = post(&state, &addr, stop, serde_json::json!({})).await;
        assert_eq!(stopped.status(), 200);
        until("the chat is stopped", || {
            status(app, writing.id) == "stopped"
        })
        .await;
        let branches: Vec<_> = app
            .store
            .changes(repo.id)
            .unwrap()
            .into_iter()
            .map(|c| c.branch)
            .collect();
        assert_eq!(branches, [run.branch(), writing.branch()]);
    }

    #[test]
    fn a_token_is_created_once_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/token");
        let a = load_or_create_token(&path).unwrap();
        assert_eq!(a.len(), 64);
        assert_eq!(load_or_create_token(&path).unwrap(), a);
    }

    type Terminal = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    async fn open_terminal(
        addr: &str,
        token: &str,
        run: i64,
    ) -> Result<Terminal, tokio_tungstenite::tungstenite::Error> {
        let mut request = format!("ws://{addr}/v1/runs/{run}/terminal")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        tokio_tungstenite::connect_async(request)
            .await
            .map(|(socket, _)| socket)
    }

    /// Reads what the shell prints until `pattern` matches it, and returns the match's first group.
    async fn read_until(terminal: &mut Terminal, pattern: &str) -> String {
        let pattern = regex::Regex::new(pattern).unwrap();
        let mut printed = String::new();
        let read = async {
            while let Some(message) = terminal.next().await {
                if let tokio_tungstenite::tungstenite::Message::Binary(bytes) = message.unwrap() {
                    printed.push_str(&String::from_utf8_lossy(&bytes));
                    if let Some(found) = pattern.captures(&printed) {
                        return found[1].to_string();
                    }
                }
            }
            panic!("the terminal closed before {pattern} was printed: {printed:?}");
        };
        tokio::time::timeout(std::time::Duration::from_secs(60), read)
            .await
            .expect("the shell printed nothing in a minute")
    }

    async fn type_in(terminal: &mut Terminal, text: &str) {
        use futures_util::SinkExt;
        let bytes = text.as_bytes().to_vec().into();
        terminal
            .send(tokio_tungstenite::tungstenite::Message::Binary(bytes))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_person_opens_a_shell_in_the_worktree_of_a_running_chat() {
        use futures_util::SinkExt;
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let started = post(
            &state,
            &addr,
            "/v1/chats".into(),
            serde_json::json!({ "agent": "Coder", "message": "Work for ever [hang]" }),
        )
        .await;
        let chat: Run = started.json().await.unwrap();
        until("the agent works", || {
            app.store.run(chat.id).unwrap().unwrap().status == "running"
        })
        .await;
        let worktree = app.store.run(chat.id).unwrap().unwrap().worktree.unwrap();

        let mut terminal = open_terminal(&addr, &state.token, chat.id).await.unwrap();
        type_in(&mut terminal, "echo moebius-$((40+2))\n").await;
        assert_eq!(
            read_until(&mut terminal, "(moebius-42)").await,
            "moebius-42"
        );

        // The shell starts in the worktree and takes the size it is sent.
        type_in(&mut terminal, "echo where-$(pwd -P)-end\n").await;
        let printed = read_until(&mut terminal, "where-(/[^\\s]*)-end").await;
        assert_eq!(
            std::path::Path::new(&printed),
            std::fs::canonicalize(&worktree).unwrap()
        );
        let resize = r#"{"type":"resize","cols":100,"rows":30}"#;
        terminal.send(resize.into()).await.unwrap();
        type_in(&mut terminal, "stty size\n").await;
        assert_eq!(read_until(&mut terminal, "(30 100)").await, "30 100");

        // What a person typed is not stored; that a shell was opened is.
        let events = app.store.events(chat.id).unwrap();
        let opened: Vec<_> = events.iter().filter(|e| e.kind == "shell_opened").collect();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].data, serde_json::json!({ "who": "person" }));
        assert!(opened[0].at > 0);
        assert!(!format!("{events:?}").contains("moebius-42"));

        // Closing the socket kills the shell.
        type_in(&mut terminal, "echo pid-$$-end\n").await;
        let pid = read_until(&mut terminal, "pid-(\\d+)-end").await;
        terminal.close(None).await.unwrap();
        until("the shell is gone", || {
            !std::path::Path::new(&format!("/proc/{pid}")).exists()
        })
        .await;

        // A shell that exits closes the socket.
        let mut terminal = open_terminal(&addr, &state.token, chat.id).await.unwrap();
        type_in(&mut terminal, "exit\n").await;
        let closed = async {
            while let Some(message) = terminal.next().await {
                if message.is_err() || message.unwrap().is_close() {
                    break;
                }
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(60), closed)
            .await
            .expect("the socket stays open after the shell exits");
    }

    #[tokio::test]
    async fn a_shell_needs_the_token_a_worktree_and_a_free_place() {
        use tokio_tungstenite::tungstenite::Error;
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let status = |e: Error| match e {
            Error::Http(response) => response.status().as_u16(),
            other => panic!("{other:?}"),
        };
        let run = app.store.add_run(Some(1), "Coder", "test", "code").unwrap();
        let wd = tempfile::tempdir().unwrap();
        app.store
            .set_run_worktree(run.id, &wd.path().to_string_lossy())
            .unwrap();

        // A run that is not over has a worktree; with the wrong token nobody gets in.
        app.store.set_run_status(run.id, "running").unwrap();
        assert_eq!(
            status(open_terminal(&addr, "nope", run.id).await.unwrap_err()),
            401
        );
        assert_eq!(
            status(
                open_terminal(&addr, &state.token, run.id + 1)
                    .await
                    .unwrap_err()
            ),
            404
        );

        // At most a few at once, and a closed one makes room.
        let mut open = Vec::new();
        for _ in 0..crate::terminal::MAX_PER_RUN {
            open.push(open_terminal(&addr, &state.token, run.id).await.unwrap());
        }
        let refused = open_terminal(&addr, &state.token, run.id).await;
        assert_eq!(status(refused.unwrap_err()), 422);
        let events = app.store.events(run.id).unwrap();
        assert_eq!(
            events.iter().filter(|e| e.kind == "shell_opened").count(),
            4
        );
        open.pop().unwrap().close(None).await.unwrap();
        until("a place is free", || app.terminals.take(run.id).is_some()).await;
        drop(open);

        // Once the run is over its worktree is gone.
        app.store.set_run_status(run.id, "succeeded").unwrap();
        let finished = open_terminal(&addr, &state.token, run.id).await;
        assert_eq!(status(finished.unwrap_err()), 422);
    }
}
