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

/// Everything the app shows, as `GET /v1/snapshot` answers.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub boards: Vec<Board>,
    #[serde(default)]
    pub inbox: Vec<InboxItem>,
}

/// One run of a card, as far as the shell needs it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Run {
    status: String,
}

/// The snapshot and the number of runs working on each repository's cards. The snapshot has no runs, so they are read
/// from the cards that can have one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overview {
    pub snapshot: Snapshot,
    pub live_runs: HashMap<i64, usize>,
}

/// A card in one of these columns of the default workflow has no run going on.
const COLUMNS_WITHOUT_RUNS: [&str; 2] = ["Backlog", "Done"];

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
        let response = self
            .http
            .get(format!("{}{path}", self.settings.address))
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

    /// The snapshot with the live runs of each repository.
    pub async fn overview(&self) -> Result<Overview, ClientError> {
        let snapshot = self.snapshot().await?;
        let mut live_runs = HashMap::new();
        for board in &snapshot.boards {
            let mut live = 0;
            for card in &board.cards {
                if COLUMNS_WITHOUT_RUNS.contains(&card.column.as_str()) {
                    continue;
                }
                let runs: Vec<Run> = self.get(&format!("/v1/cards/{}/runs", card.id)).await?;
                live += runs
                    .iter()
                    .filter(|run| matches!(run.status.as_str(), "starting" | "running"))
                    .count();
            }
            live_runs.insert(board.repository.id, live);
        }
        Ok(Overview {
            snapshot,
            live_runs,
        })
    }
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
            match connect(&settings).await {
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
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Error,
> {
    let url = settings.address.replacen("http", "ws", 1) + "/v1/events";
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
                    "column": "Backlog", "created_at": 1790000000, "added_later": true}],
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
