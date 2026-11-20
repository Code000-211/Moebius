//! What the tray icon shows of a server and what it asks of it (ADR 0021): the agents working, the Inbox, today's
//! tokens against the day's budget, and whether all agents are paused, with the routes that pause and resume them. The
//! figures are the answer of one small route, `GET /v1/status` (never the snapshot), and [`follow`] reads them again
//! when the event stream says something that can change them happened. The tray itself, a menu the system draws, is the program's `tray_icon` module.

use std::time::Duration;

use tokio::time::Instant;

use futures_util::StreamExt;
use serde::Deserialize;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::server::Server;

/// How long [`follow`] waits after the first event of a burst before it reads the figures.
const DEBOUNCE: Duration = Duration::from_millis(750);

/// The least time between two readings that only the steps of runs ask for: a busy run says something at every step,
/// which changes the figures of the tokens and nothing else.
const STEPS: Duration = Duration::from_secs(10);

/// The kind of the events that say a run took a step.
const STEP_EVENT: &str = "run_event";

/// How long [`follow`] waits before it tries a server that cannot be reached again.
const RETRY: Duration = Duration::from_secs(5);

/// How long one request of the tray waits for the server.
const TIMEOUT: Duration = Duration::from_secs(10);

/// What the tray shows of a server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Figures {
    /// The cards whose latest run is starting or running.
    pub working: usize,
    /// The inbox items that are not resolved yet.
    pub inbox: usize,
    /// The notices of the person that are not read yet.
    pub unread: usize,
    /// The tokens the model calls used since local midnight of the server.
    pub tokens: i64,
    /// `day_tokens` of the server's `moebius.toml`, if it has one.
    pub budget: Option<i64>,
    /// Whether all agents are paused.
    pub paused: bool,
}

/// Why the tray cannot read a server or pause it.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot reach the server: {0}")]
    Unreachable(String),
    #[error("the server answered {0}")]
    Refused(u16),
    #[error("the server's answer is not what this app reads")]
    Unreadable,
}

impl Figures {
    /// The figures of the answer of `GET /v1/status`.
    pub fn read(status: &serde_json::Value) -> Option<Self> {
        let status = Status::deserialize(status).ok()?;
        Some(Self {
            working: status.working,
            inbox: status.inbox,
            unread: status.unread_notices,
            tokens: status.tokens,
            budget: status.day_budget,
            paused: status.paused,
        })
    }

    /// The count the rail shows of the server: what the sidebar of the web app counts, the open items of the inbox
    /// and the notices not read yet.
    pub fn rail_count(&self) -> u32 {
        u32::try_from(self.inbox + self.unread).unwrap_or(u32::MAX)
    }

    /// The lines of the tray's menu and its tooltip.
    pub fn lines(&self) -> Lines {
        let working = match self.working {
            0 => "No agents working".to_string(),
            1 => "1 agent working".to_string(),
            n => format!("{n} agents working"),
        };
        // The sidebar and the rail count the items and the unread notices together; the tray says which is which.
        let inbox = match self.unread {
            0 => format!("Inbox: {}", self.inbox),
            1 => format!("Inbox: {}, 1 unread notice", self.inbox),
            n => format!("Inbox: {}, {n} unread notices", self.inbox),
        };
        let tokens = match self.budget {
            Some(budget) => format!("Tokens today: {} of {}", short(self.tokens), short(budget)),
            None => format!("Tokens today: {}", short(self.tokens)),
        };
        let paused = if self.paused { " (paused)" } else { "" };
        Lines {
            tooltip: format!("Moebius{paused}\n{working}\n{inbox}\n{tokens}"),
            working: format!("{working}{paused}"),
            inbox,
            tokens,
            pause: if self.paused {
                "Resume all agents"
            } else {
                "Pause all agents"
            }
            .to_string(),
        }
    }
}

/// The text of the tray's menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lines {
    pub working: String,
    pub inbox: String,
    pub tokens: String,
    /// What the pause item says: it pauses, or resumes while all agents are paused.
    pub pause: String,
    pub tooltip: String,
}

/// The answer of `GET /v1/status`.
#[derive(Deserialize)]
struct Status {
    working: usize,
    inbox: usize,
    unread_notices: usize,
    tokens: i64,
    day_budget: Option<i64>,
    paused: bool,
}

/// The lines of a server that cannot be reached.
pub fn offline() -> Lines {
    Lines {
        working: "Server not reachable".into(),
        inbox: "Inbox: -".into(),
        tokens: "Tokens today: -".into(),
        pause: "Pause all agents".into(),
        tooltip: "Moebius\nServer not reachable".into(),
    }
}

/// A count of tokens as people say it: `950`, `12.5k`, `1.2M`.
fn short(tokens: i64) -> String {
    let (value, unit) = match tokens {
        t if t >= 999_950 => (t as f64 / 1_000_000.0, "M"),
        t if t >= 1_000 => (t as f64 / 1_000.0, "k"),
        t => return t.to_string(),
    };
    let text = format!("{value:.1}");
    format!("{}{unit}", text.strip_suffix(".0").unwrap_or(&text))
}

/// Reads the figures of `server`, waiting for it at most `timeout`.
pub async fn read_within(server: &Server, timeout: Duration) -> Result<Figures, Error> {
    let response = crate::server::http()
        .get(format!("{}/v1/status", server.url))
        .bearer_auth(&server.token)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| Error::Unreachable(e.without_url().to_string()))?;
    if !response.status().is_success() {
        return Err(Error::Refused(response.status().as_u16()));
    }
    let status = response.json().await.map_err(|_| Error::Unreadable)?;
    Figures::read(&status).ok_or(Error::Unreadable)
}

/// Reads the figures of `server`.
pub async fn read(server: &Server) -> Result<Figures, Error> {
    read_within(server, TIMEOUT).await
}

/// Pauses all agents of `server`, or resumes them.
pub async fn set_paused(server: &Server, paused: bool) -> Result<(), Error> {
    let route = if paused { "pause" } else { "resume" };
    let response = crate::server::http()
        .post(format!("{}/v1/runs/{route}", server.url))
        .bearer_auth(&server.token)
        .json(&serde_json::json!({ "reason": "from the tray" }))
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| Error::Unreachable(e.without_url().to_string()))?;
    match response.status() {
        s if s.is_success() => Ok(()),
        s => Err(Error::Refused(s.as_u16())),
    }
}

/// Tells `show` the figures of `server` now and again after every burst of events, for as long as it is polled: `None`
/// while the server cannot be reached, which it keeps trying. Dropping it, or the task that runs it, ends it.
pub async fn follow(server: Server, mut show: impl FnMut(Option<Figures>) + Send) {
    loop {
        let Some(mut events) = events(&server).await else {
            show(None);
            tokio::time::sleep(RETRY).await;
            continue;
        };
        // Read once connected, so that nothing between the last reading and the connection is missed.
        'connected: loop {
            show(read(&server).await.ok());
            let read_at = Instant::now();
            // Wait for something to happen, then for the burst it belongs to to end.
            let mut due: Option<Instant> = None;
            loop {
                let wait = async {
                    match due {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                };
                tokio::select! {
                    () = wait => break,
                    next = events.next() => {
                        let Some(Ok(message)) = next else { break 'connected };
                        let now = Instant::now();
                        due = Some(match (due, is_step(&message)) {
                            (None, true) => (now + DEBOUNCE).max(read_at + STEPS),
                            (None, false) => now + DEBOUNCE,
                            (Some(at), true) => at,
                            (Some(at), false) => at.min(now + DEBOUNCE),
                        });
                    }
                }
            }
        }
        show(None);
        tokio::time::sleep(RETRY).await;
    }
}

/// Whether `message` of the event stream says a run took a step, which changes nothing the tray shows but the tokens.
fn is_step(message: &tokio_tungstenite::tungstenite::Message) -> bool {
    #[derive(Deserialize)]
    struct Kind {
        kind: String,
    }
    message
        .to_text()
        .ok()
        .and_then(|text| serde_json::from_str::<Kind>(text).ok())
        .is_some_and(|event| event.kind == STEP_EVENT)
}

type Events =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The event stream of `server`, if it can be opened.
async fn events(server: &Server) -> Option<Events> {
    let address = server.url.replacen("http", "ws", 1);
    let mut request = format!("{address}/v1/events").into_client_request().ok()?;
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {}", server.token).parse().ok()?,
    );
    tokio_tungstenite::connect_async(request)
        .await
        .ok()
        .map(|(ws, _)| ws)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn status(working: usize, inbox: usize, tokens: i64, budget: Option<i64>) -> serde_json::Value {
        json!({"working": working, "inbox": inbox, "unread_notices": 1, "tokens": tokens,
               "day_budget": budget, "paused": false})
    }

    #[test]
    fn the_figures_are_the_answer_of_the_status_route() {
        let figures = Figures::read(&status(2, 2, 1_250_000, Some(8_000_000))).unwrap();
        assert_eq!(
            figures,
            Figures {
                working: 2,
                inbox: 2,
                unread: 1,
                tokens: 1_250_000,
                budget: Some(8_000_000),
                paused: false
            }
        );
        // The rail counts what the sidebar of the web app counts.
        assert_eq!(figures.rail_count(), 3);
        assert_eq!(Figures::read(&json!({})), None);
    }

    #[test]
    fn the_tray_says_what_its_inbox_line_counts() {
        let lines = |inbox, unread| {
            Figures {
                inbox,
                unread,
                ..Figures::default()
            }
            .lines()
            .inbox
        };
        assert_eq!(lines(61, 0), "Inbox: 61");
        assert_eq!(lines(61, 1), "Inbox: 61, 1 unread notice");
        assert_eq!(lines(61, 200), "Inbox: 61, 200 unread notices");
    }

    #[test]
    fn only_the_steps_of_runs_are_slow_to_be_read() {
        use tokio_tungstenite::tungstenite::Message;
        let message = |kind: &str| Message::text(json!({"kind": kind, "data": {}}).to_string());
        assert!(is_step(&message("run_event")));
        assert!(!is_step(&message("run_ended")));
        assert!(!is_step(&message("notice_added")));
        assert!(!is_step(&Message::text("not json")));
        assert!(!is_step(&Message::Binary(vec![1].into())));
    }

    #[test]
    fn the_lines_say_what_is_going_on_and_what_the_pause_item_does() {
        let lines = Figures {
            working: 3,
            inbox: 1,
            unread: 0,
            tokens: 12_500,
            budget: Some(8_000_000),
            paused: false,
        }
        .lines();
        assert_eq!(
            (
                lines.working.as_str(),
                lines.inbox.as_str(),
                lines.tokens.as_str(),
                lines.pause.as_str()
            ),
            (
                "3 agents working",
                "Inbox: 1",
                "Tokens today: 12.5k of 8M",
                "Pause all agents"
            )
        );
        let paused = Figures {
            working: 0,
            paused: true,
            ..Figures::default()
        }
        .lines();
        assert_eq!(paused.working, "No agents working (paused)");
        assert_eq!(paused.tokens, "Tokens today: 0");
        assert_eq!(paused.pause, "Resume all agents");
        assert!(paused.tooltip.starts_with("Moebius (paused)"));
        assert_eq!(
            Figures {
                working: 1,
                ..Figures::default()
            }
            .lines()
            .working,
            "1 agent working"
        );
    }

    #[test]
    fn counts_of_tokens_are_short() {
        let said: Vec<String> = [0, 950, 1_000, 12_500, 999_999, 1_200_000, 8_000_000]
            .into_iter()
            .map(short)
            .collect();
        assert_eq!(said, ["0", "950", "1k", "12.5k", "1M", "1.2M", "8M"]);
    }
}
