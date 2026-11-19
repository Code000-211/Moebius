//! What the tray icon shows of a server and what it asks of it (ADR 0021): the agents working, the Inbox, today's
//! tokens against the day's budget, and whether all agents are paused, with the routes that pause and resume them. The
//! figures are read from the snapshot and the usage of the server, and [`follow`] reads them again whenever the event
//! stream says something happened. The tray itself, a menu the system draws, is the program's `tray_icon` module.

use std::time::Duration;

use futures_util::StreamExt;
use serde_json::Value;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::server::Server;

/// How long [`follow`] waits for more events before it reads the figures: a busy run says something at every step.
const DEBOUNCE: Duration = Duration::from_millis(750);

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
    /// The figures of the answers of `GET /v1/snapshot` and `GET /v1/usage`.
    pub fn read(snapshot: &Value, usage: &Value) -> Option<Self> {
        let boards = snapshot.get("boards")?.as_array()?;
        let working = boards
            .iter()
            .flat_map(|board| board["cards"].as_array().into_iter().flatten())
            .filter(|card| matches!(card["run"]["status"].as_str(), Some("starting" | "running")))
            .count();
        Some(Self {
            working,
            inbox: snapshot.get("inbox")?.as_array()?.len(),
            tokens: usage["totals"]["tokens"].as_i64()?,
            budget: usage["totals"]["day_budget"].as_i64(),
            paused: snapshot.get("paused").is_some_and(|p| !p.is_null()),
        })
    }

    /// The lines of the tray's menu and its tooltip.
    pub fn lines(&self) -> Lines {
        let working = match self.working {
            0 => "No agents working".to_string(),
            1 => "1 agent working".to_string(),
            n => format!("{n} agents working"),
        };
        let inbox = format!("Inbox: {}", self.inbox);
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

async fn get(server: &Server, path: &str) -> Result<Value, Error> {
    let response = reqwest::Client::new()
        .get(format!("{}{path}", server.url))
        .bearer_auth(&server.token)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| Error::Unreachable(e.without_url().to_string()))?;
    if !response.status().is_success() {
        return Err(Error::Refused(response.status().as_u16()));
    }
    response.json().await.map_err(|_| Error::Unreadable)
}

/// Reads the figures of `server`.
pub async fn read(server: &Server) -> Result<Figures, Error> {
    let snapshot = get(server, "/v1/snapshot").await?;
    let usage = get(server, "/v1/usage?days=1").await?;
    Figures::read(&snapshot, &usage).ok_or(Error::Unreadable)
}

/// Pauses all agents of `server`, or resumes them.
pub async fn set_paused(server: &Server, paused: bool) -> Result<(), Error> {
    let route = if paused { "pause" } else { "resume" };
    let response = reqwest::Client::new()
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
        loop {
            show(read(&server).await.ok());
            // Wait for something to happen, then for the burst it belongs to to end.
            if events.next().await.is_none_or(|m| m.is_err()) {
                break;
            }
            let burst = tokio::time::sleep(DEBOUNCE);
            tokio::pin!(burst);
            loop {
                tokio::select! {
                    () = &mut burst => break,
                    next = events.next() => if next.is_none_or(|m| m.is_err()) {
                        break;
                    },
                }
            }
        }
        show(None);
        tokio::time::sleep(RETRY).await;
    }
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

    fn snapshot(statuses: &[&str], inbox: usize, paused: bool) -> Value {
        let cards: Vec<Value> = statuses
            .iter()
            .map(|s| json!({"run": if s.is_empty() { Value::Null } else { json!({"status": s}) }}))
            .collect();
        let mut snapshot = json!({"boards": [{"cards": cards}], "inbox": vec![json!({}); inbox]});
        if paused {
            snapshot["paused"] = json!({"by": "person:ada", "at": 1});
        }
        snapshot
    }

    fn usage(tokens: i64, budget: Option<i64>) -> Value {
        json!({"totals": {"tokens": tokens, "day_budget": budget}})
    }

    #[test]
    fn the_figures_count_the_cards_with_a_run_going_the_inbox_and_the_days_tokens() {
        let figures = Figures::read(
            &snapshot(&["running", "starting", "succeeded", ""], 2, false),
            &usage(1_250_000, Some(8_000_000)),
        )
        .unwrap();
        assert_eq!(
            figures,
            Figures {
                working: 2,
                inbox: 2,
                tokens: 1_250_000,
                budget: Some(8_000_000),
                paused: false
            }
        );
        assert_eq!(Figures::read(&json!({}), &usage(1, None)), None);
    }

    #[test]
    fn the_lines_say_what_is_going_on_and_what_the_pause_item_does() {
        let lines = Figures {
            working: 3,
            inbox: 1,
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
