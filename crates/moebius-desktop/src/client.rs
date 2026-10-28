//! The client of the desktop API (`docs/reference/desktop-api.md`), the only way the app talks to the server: typed
//! answers for what the shell shows, and the event stream as an iced subscription that reconnects by itself.
//!
//! The types keep to what the reference says and are tolerant: a field that may be missing has a default and a field
//! the app does not know is ignored, so a newer server does not break an older app.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use iced::futures::channel::mpsc;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::settings::Settings;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("the token is not accepted")]
    Unauthorized,
    #[error("cannot reach {0}")]
    Unreachable(String),
    #[error("the server answered {status}: {message}")]
    Server { status: u16, message: String },
    #[error("the server's answer is not understood: {0}")]
    Decode(String),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Health {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Repository {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub path: String,
    /// `none` or `github`.
    #[serde(default)]
    pub forge: String,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub github: Option<String>,
    #[serde(default)]
    pub trusted: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Card {
    pub id: i64,
    pub repository_id: i64,
    pub number: i64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub author: String,
    pub column: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub retried_after: Option<i64>,
    /// The `code` rounds and the tokens since the card was last retried.
    #[serde(default)]
    pub rounds: i64,
    #[serde(default)]
    pub tokens: i64,
    /// The card's latest run.
    #[serde(default)]
    pub run: Option<Run>,
    /// The numbers of the issues that block the card.
    #[serde(default)]
    pub blocked_by: Vec<i64>,
    /// How many tool calls of the latest run were refused.
    #[serde(default)]
    pub refusals: i64,
    /// The card's open change.
    #[serde(default)]
    pub change: Option<CardChange>,
}

impl Card {
    /// Whether the card's latest run is working.
    pub fn is_live(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|run| matches!(run.status.as_str(), "starting" | "running"))
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Run {
    pub id: i64,
    pub agent: String,
    pub purpose: String,
    /// `starting`, `running`, `waiting`, `succeeded`, `failed` or `stopped`.
    pub status: String,
    /// The harness that runs the agent, as `GET /v1/runs/{id}` says; the snapshot's short form has none.
    #[serde(default)]
    pub harness: String,
    #[serde(default)]
    pub started_at: i64,
}

/// One stored event of a run, as `GET /v1/runs/{id}/events` lists it and the stream's `run_event` carries it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RunEvent {
    pub seq: i64,
    pub kind: String,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub at: i64,
}

/// A recorded decision, as far as the live view matches it to a tool call.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Judgement {
    pub kind: String,
    pub choice: String,
    #[serde(default)]
    pub reason: String,
    pub decider: String,
    #[serde(default)]
    pub run: Option<i64>,
}

/// One entry of a card's timeline.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HistoryEntry {
    pub at: i64,
    pub kind: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

/// The open change of a card, as far as the board shows it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CardChange {
    pub id: i64,
    /// `pending`, `success`, `failure` or `none`.
    pub checks: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Change {
    pub id: i64,
    pub repository_id: i64,
    #[serde(default)]
    pub card_id: Option<i64>,
    pub branch: String,
    /// `open`, `merged` or `closed`.
    pub state: String,
    #[serde(default)]
    pub pull_request: Option<i64>,
    #[serde(default)]
    pub checks: Option<String>,
    #[serde(default)]
    pub review: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Board {
    pub repository: Repository,
    #[serde(default)]
    pub cards: Vec<Card>,
    /// The most `code` rounds a card gets; 0 when the server does not say.
    #[serde(default)]
    pub round_limit: i64,
    /// The reason each waiting card waits, by card id.
    #[serde(default)]
    pub waiting: HashMap<i64, String>,
    #[serde(default)]
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InboxItem {
    pub id: i64,
    pub kind: String,
    #[serde(default)]
    pub card_id: Option<i64>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub at: i64,
}

impl InboxItem {
    /// The id of the change a `ready_to_merge` or `change_closed` item is about.
    pub fn change(&self) -> Option<i64> {
        self.data["change"].as_i64()
    }
}

impl Board {
    /// How many runs are working on the board's cards.
    pub fn live_runs(&self) -> usize {
        self.cards.iter().filter(|card| card.is_live()).count()
    }
}

/// Everything the app shows, as `GET /v1/snapshot` answers.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub boards: Vec<Board>,
    #[serde(default)]
    pub inbox: Vec<InboxItem>,
}

/// How many events one request for a run's events asks for.
const EVENT_PAGE: usize = 500;

/// An event of `GET /v1/events`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ServerEvent {
    pub kind: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

/// What the event stream tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Connected,
    /// The connection failed or ended; the stream tries again by itself.
    Disconnected(String),
    /// The server does not accept the token; the stream stops.
    Unauthorized,
    Event(ServerEvent),
}

pub struct Client {
    http: reqwest::Client,
    settings: Settings,
}

impl Client {
    pub fn new(settings: Settings) -> Self {
        Client {
            http: reqwest::Client::new(),
            settings,
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let request = self.http.get(format!("{}{path}", self.settings.address));
        self.send(request).await
    }

    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, ClientError> {
        let request = self
            .http
            .post(format!("{}{path}", self.settings.address))
            .json(&body);
        self.send(request).await
    }

    async fn send<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, ClientError> {
        let response = request
            .bearer_auth(&self.settings.token)
            .send()
            .await
            .map_err(|_| ClientError::Unreachable(self.settings.address.clone()))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(ClientError::Server {
                status: status.as_u16(),
                message,
            });
        }
        response
            .json()
            .await
            .map_err(|e| ClientError::Decode(e.to_string()))
    }

    pub async fn health(&self) -> Result<Health, ClientError> {
        self.get("/v1/health").await
    }

    pub async fn snapshot(&self) -> Result<Snapshot, ClientError> {
        self.get("/v1/snapshot").await
    }

    pub async fn run(&self, id: i64) -> Result<Run, ClientError> {
        self.get(&format!("/v1/runs/{id}")).await
    }

    /// Every event of a run with a `seq` above `after`, read in pages.
    pub async fn run_events(&self, id: i64, after: i64) -> Result<Vec<RunEvent>, ClientError> {
        let mut events: Vec<RunEvent> = Vec::new();
        loop {
            let from = events.last().map_or(after, |event| event.seq);
            let page: Vec<RunEvent> = self
                .get(&format!(
                    "/v1/runs/{id}/events?after={from}&limit={EVENT_PAGE}"
                ))
                .await?;
            let last = page.len() < EVENT_PAGE;
            events.extend(page);
            if last {
                return Ok(events);
            }
        }
    }

    /// The decisions recorded for a card, oldest first.
    pub async fn judgements(&self, card: i64) -> Result<Vec<Judgement>, ClientError> {
        let mut judgements: Vec<Judgement> = self
            .get(&format!("/v1/judgements?card={card}&limit=1000"))
            .await?;
        judgements.reverse();
        Ok(judgements)
    }

    pub async fn history(&self, card: i64) -> Result<Vec<HistoryEntry>, ClientError> {
        self.get(&format!("/v1/cards/{card}/history")).await
    }

    pub async fn steer(&self, run: i64, message: &str) -> Result<Run, ClientError> {
        self.post(
            &format!("/v1/runs/{run}/steer"),
            serde_json::json!({ "message": message }),
        )
        .await
    }

    /// What a person does with an inbox item. The answers are not read: the snapshot shows what changed.
    async fn decide(&self, path: String, body: serde_json::Value) -> Result<(), ClientError> {
        self.post::<serde_json::Value>(&path, body).await.map(drop)
    }

    /// Merges an open change.
    pub async fn merge(&self, change: i64) -> Result<(), ClientError> {
        self.decide(format!("/v1/changes/{change}/merge"), serde_json::json!({}))
            .await
    }

    /// Sends a change back to coding with the reason as the card's note.
    pub async fn reject(&self, change: i64, reason: &str) -> Result<(), ClientError> {
        let body = serde_json::json!({ "reason": reason });
        self.decide(format!("/v1/changes/{change}/reject"), body)
            .await
    }

    /// Approves an outside issue that waits in the backlog.
    pub async fn approve(&self, card: i64) -> Result<(), ClientError> {
        self.decide(format!("/v1/cards/{card}/approve"), serde_json::json!({}))
            .await
    }

    /// Gives a stuck card, or one whose pull request was closed, a fresh start.
    pub async fn retry(&self, card: i64) -> Result<(), ClientError> {
        self.decide(format!("/v1/cards/{card}/retry"), serde_json::json!({}))
            .await
    }

    pub async fn stop(&self, run: i64) -> Result<Run, ClientError> {
        self.post(&format!("/v1/runs/{run}/stop"), serde_json::json!({}))
            .await
    }
}

/// What a shell in a run's worktree sends the app.
#[derive(Debug, Clone)]
pub enum TerminalEvent {
    /// The socket is open; what is sent to the channel goes to the shell.
    Connected(mpsc::Sender<TerminalInput>),
    /// What the shell printed.
    Output(Vec<u8>),
    /// The shell exited or the socket could not be opened or ended; nothing follows.
    Closed(String),
}

/// What the app sends a shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalInput {
    Bytes(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

/// A shell in the worktree of `run` (`GET /v1/runs/{id}/terminal`) for as long as the stream is held: dropping it
/// closes the socket, which kills the shell.
pub fn terminal(settings: Settings, run: i64) -> impl iced::futures::Stream<Item = TerminalEvent> {
    iced::stream::channel(64, async move |mut out: mpsc::Sender<TerminalEvent>| {
        let mut socket = match connect(&settings, &format!("/v1/runs/{run}/terminal")).await {
            Ok(socket) => socket,
            Err(e) => {
                let reason = match e {
                    tungstenite::Error::Http(response) => format!(
                        "the server answered {}: {}",
                        response.status().as_u16(),
                        String::from_utf8_lossy(response.body().as_deref().unwrap_or_default())
                    ),
                    other => other.to_string(),
                };
                let _ = out.send(TerminalEvent::Closed(reason)).await;
                return;
            }
        };
        let (input, mut typed) = mpsc::channel(64);
        if out.send(TerminalEvent::Connected(input)).await.is_err() {
            return;
        }
        let ended = loop {
            tokio::select! {
                message = socket.next() => match message {
                    Some(Ok(Message::Binary(bytes))) => {
                        if out.send(TerminalEvent::Output(bytes.to_vec())).await.is_err() {
                            return;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break "the shell ended".to_string(),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => break e.to_string(),
                },
                sent = typed.next() => {
                    let Some(sent) = sent else { return };
                    let message = match sent {
                        TerminalInput::Bytes(bytes) => Message::Binary(bytes.into()),
                        TerminalInput::Resize { cols, rows } => Message::Text(
                            serde_json::json!({"type": "resize", "cols": cols, "rows": rows})
                                .to_string()
                                .into(),
                        ),
                    };
                    if let Err(e) = socket.send(message).await {
                        break e.to_string();
                    }
                }
            }
        };
        let _ = out.send(TerminalEvent::Closed(ended)).await;
    })
}

/// How long to wait before trying to connect again, after the `failures`-th failure in a row.
fn backoff(failures: u32) -> Duration {
    Duration::from_secs(1 << failures.min(5)).min(Duration::from_secs(30))
}

/// The event stream of the server: `Connected` once the socket is open, then every event, and `Disconnected` when it
/// ends or cannot be opened, after which it connects again with a growing wait. Events are not replayed, so the app
/// reads the snapshot on every `Connected`.
pub fn events(settings: Settings) -> impl iced::futures::Stream<Item = StreamEvent> {
    iced::stream::channel(64, async move |mut out: mpsc::Sender<StreamEvent>| {
        let mut failures = 0;
        loop {
            match connect(&settings, "/v1/events").await {
                Ok(mut socket) => {
                    failures = 0;
                    let _ = out.send(StreamEvent::Connected).await;
                    let ended = loop {
                        match socket.next().await {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(event) = serde_json::from_str(text.as_str()) {
                                    let _ = out.send(StreamEvent::Event(event)).await;
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                break "the server closed the connection".to_string();
                            }
                            Some(Ok(_)) => {}
                            Some(Err(e)) => break e.to_string(),
                        }
                    };
                    let _ = out.send(StreamEvent::Disconnected(ended)).await;
                }
                Err(tungstenite::Error::Http(response)) if response.status() == 401 => {
                    let _ = out.send(StreamEvent::Unauthorized).await;
                    return;
                }
                Err(e) => {
                    let _ = out.send(StreamEvent::Disconnected(e.to_string())).await;
                }
            }
            tokio::time::sleep(backoff(failures)).await;
            failures += 1;
        }
    })
}

async fn connect(
    settings: &Settings,
    path: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Error,
> {
    let url = settings.address.replacen("http", "ws", 1) + path;
    let mut request = url.into_client_request()?;
    let bearer = format!("Bearer {}", settings.token)
        .parse()
        .map_err(|_| tungstenite::Error::Utf8("the token is not a header value".into()))?;
    request.headers_mut().insert("authorization", bearer);
    tokio_tungstenite::connect_async(request)
        .await
        .map(|(socket, _)| socket)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The snapshot of the reference, with a field the app does not know and a card without the optional fields.
    const SNAPSHOT: &str = r#"{
      "boards": [
        {"repository": {"id": 1, "name": "demo", "path": "/srv/.moebius/repos/demo", "forge": "none",
                        "default_branch": "main", "github": null, "trusted": []},
         "cards": [{"id": 1, "repository_id": 1, "number": 1, "title": "Add a feature", "body": "", "author": "person",
                    "column": "Backlog", "created_at": 1790000000, "added_later": true},
                   {"id": 2, "repository_id": 1, "number": 2, "title": "Work", "column": "In progress",
                    "rounds": 2, "tokens": 900, "blocked_by": [1], "refusals": 1,
                    "run": {"id": 4, "agent": "Coder", "purpose": "code", "status": "running"},
                    "change": {"id": 3, "state": "open", "checks": "pending", "review_verdict": null}}],
         "round_limit": 5,
         "waiting": {"1": "needs a person"},
         "changes": []}
      ],
      "inbox": [{"id": 7, "kind": "approve_issue", "card_id": 1, "title": "Add a feature",
                 "data": {"reason": "new author"}, "resolved_at": null, "at": 1790000001}]
    }"#;

    #[test]
    fn the_snapshot_of_the_reference_is_read() {
        let snapshot: Snapshot = serde_json::from_str(SNAPSHOT).unwrap();
        let board = &snapshot.boards[0];
        assert_eq!(board.repository.name, "demo");
        assert_eq!(board.repository.github, None);
        assert_eq!(board.cards[0].column, "Backlog");
        assert_eq!(board.cards[0].note, None);
        assert_eq!(board.waiting[&1], "needs a person");
        assert_eq!(
            (board.cards[0].rounds, board.cards[0].run.as_ref()),
            (0, None)
        );
        let work = &board.cards[1];
        assert_eq!((work.rounds, work.tokens, work.refusals), (2, 900, 1));
        assert_eq!(work.blocked_by, [1]);
        assert_eq!(work.change.as_ref().unwrap().checks, "pending");
        assert_eq!((board.round_limit, board.live_runs()), (5, 1));
        assert_eq!(snapshot.inbox[0].kind, "approve_issue");
    }

    #[test]
    fn missing_parts_of_the_snapshot_are_empty() {
        let snapshot: Snapshot = serde_json::from_str("{}").unwrap();
        assert_eq!(snapshot, Snapshot::default());
    }

    #[test]
    fn an_event_keeps_its_kind_and_data() {
        let event: ServerEvent = serde_json::from_str(
            r#"{"kind":"card_moved","data":{"card":1,"from":"Ready","to":"In progress"}}"#,
        )
        .unwrap();
        assert_eq!(event.kind, "card_moved");
        assert_eq!(event.data["to"], "In progress");
    }

    #[test]
    fn the_wait_before_connecting_again_grows_to_a_limit() {
        let waits: Vec<u64> = (0..8).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30, 30]);
    }
}
