//! What the Inbox tells the person (ADR 0021), as the app needs it for native notifications and the count on its icon:
//! [`watch`] follows one server's event stream with the access token and says, as [`Effect`]s, when to show a
//! notification and what the count of unread notices is.
//!
//! A notice is what the server keeps for one person (`GET /v1/notices`: a card given to them, a mention, a change to a
//! card they follow, which includes a change that is ready to merge), and the stream sends `notice_added` and
//! `notice_read` only to that person. The first read of the notices is the baseline and notifies nothing, so starting
//! the app does not announce everything unread; a notice that arrives while the stream is down is notified once the
//! stream is back. A notice is notified at most once, whatever sends it twice.

use std::collections::HashSet;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{self, http::header};

use crate::server::Server;

/// One notice of the person, as `GET /v1/notices` and the events give it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Notice {
    pub id: u64,
    /// `assigned`, `mentioned` or `changed`.
    pub kind: String,
    pub by: Option<String>,
    pub title: String,
    pub body: String,
    pub read: bool,
}

impl Notice {
    /// The path of the web app that opens this notice: the Inbox's Mentions tab, which reads it and goes to its card or
    /// item.
    pub fn path(&self) -> String {
        format!("/inbox?kind=mentions&notice={}", self.id)
    }

    /// The title and the text of its notification.
    pub fn text(&self) -> (String, String) {
        let by = self.by.as_deref().map(|by| format!("@{by}"));
        let body = match (self.kind.as_str(), by) {
            ("assigned", Some(by)) => format!("{by} gave it to you"),
            ("assigned", None) => "Given to you".to_string(),
            ("mentioned", Some(by)) => format!("{by} mentioned you: {}", self.body),
            ("mentioned", None) => format!("You were mentioned: {}", self.body),
            _ => self.body.clone(),
        };
        (self.title.clone(), body)
    }
}

/// What the app does about the notices of a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Show a native notification of this notice.
    Notify(Notice),
    /// The count of unread notices is now this, for the icon.
    Count(usize),
}

/// What the app knows of the notices: which it has told the person of, and which are unread.
#[derive(Default)]
struct Tally {
    loaded: bool,
    seen: HashSet<u64>,
    unread: HashSet<u64>,
}

impl Tally {
    /// The notices read from the server, which are all there are: the first time is the baseline; later (after the
    /// stream was down) an unread notice not seen before is notified.
    fn load(&mut self, notices: &[Notice]) -> Vec<Effect> {
        let mut effects = Vec::new();
        for notice in notices.iter().rev().filter(|notice| !notice.read) {
            if self.seen.insert(notice.id) && self.loaded {
                effects.push(Effect::Notify(notice.clone()));
            }
        }
        self.seen.extend(notices.iter().map(|notice| notice.id));
        self.unread = notices
            .iter()
            .filter(|notice| !notice.read)
            .map(|notice| notice.id)
            .collect();
        self.loaded = true;
        effects.push(Effect::Count(self.unread.len()));
        effects
    }

    /// An event of the stream: `notice_added` notifies a notice not seen before, `notice_read` lowers the count.
    fn event(&mut self, event: &Value) -> Vec<Effect> {
        let Some(notice) = event["data"]
            .get("notice")
            .and_then(|notice| serde_json::from_value::<Notice>(notice.clone()).ok())
        else {
            return Vec::new();
        };
        let before = self.unread.len();
        let mut effects = Vec::new();
        match event["kind"].as_str() {
            Some("notice_added") => {
                let new = self.seen.insert(notice.id);
                if notice.read {
                    self.unread.remove(&notice.id);
                } else {
                    self.unread.insert(notice.id);
                    if new {
                        effects.push(Effect::Notify(notice));
                    }
                }
            }
            Some("notice_read") => {
                self.seen.insert(notice.id);
                self.unread.remove(&notice.id);
            }
            _ => return Vec::new(),
        }
        if self.unread.len() != before {
            effects.push(Effect::Count(self.unread.len()));
        }
        effects
    }
}

/// The watch of one server's notices, which stops when it is dropped.
pub struct Watcher {
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// How long to wait before connecting again after the stream ended or could not be reached, at most.
const RETRY_MAX: Duration = Duration::from_secs(30);

/// Follows the notices of `server` until the returned [`Watcher`] is dropped, and sends what to do about them.
pub fn watch(server: Server) -> (Watcher, mpsc::UnboundedReceiver<Effect>) {
    let (send, effects) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut tally = Tally::default();
        let mut wait = Duration::from_millis(500);
        loop {
            if follow(&server, &mut tally, &send, &mut wait).await.is_err() {
                // Nobody listens any more.
                return;
            }
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(RETRY_MAX);
        }
    });
    (Watcher { task }, effects)
}

/// One connection: opens the stream, reads the notices (after, so that none falls between), then passes events until
/// the stream ends. Errors only when `send` has no receiver; any other trouble ends the connection quietly.
async fn follow(
    server: &Server,
    tally: &mut Tally,
    send: &mpsc::UnboundedSender<Effect>,
    wait: &mut Duration,
) -> Result<(), ()> {
    let emit = |effects: Vec<Effect>| {
        effects
            .into_iter()
            .try_for_each(|effect| send.send(effect).map_err(|_| ()))
    };
    let Some(mut stream) = connect(server).await else {
        return Ok(());
    };
    let Some(notices) = read(server).await else {
        return Ok(());
    };
    *wait = Duration::from_millis(500);
    emit(tally.load(&notices))?;
    while let Some(Ok(message)) = stream.next().await {
        if let tungstenite::Message::Text(text) = message
            && let Ok(event) = serde_json::from_str::<Value>(text.as_str())
        {
            emit(tally.event(&event))?;
        }
    }
    Ok(())
}

type Stream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The server's event stream, signed in with the token.
async fn connect(server: &Server) -> Option<Stream> {
    let url = format!("{}/v1/events", server.url)
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let mut request = url.as_str().into_client_request().ok()?;
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {}", server.token).parse().ok()?,
    );
    tokio_tungstenite::connect_async(request)
        .await
        .ok()
        .map(|(stream, _)| stream)
}

/// The person's notices, newest first.
async fn read(server: &Server) -> Option<Vec<Notice>> {
    crate::server::http()
        .get(format!("{}/v1/notices", server.url))
        .bearer_auth(&server.token)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn notice(id: u64, read: bool) -> Notice {
        Notice {
            id,
            kind: "mentioned".into(),
            by: Some("tom".into()),
            title: "Fix it".into(),
            body: "Hi @lea".into(),
            read,
        }
    }

    fn event(kind: &str, notice: &Notice) -> Value {
        json!({"kind": kind, "data": {"person": "lea", "notice": {
            "id": notice.id, "kind": notice.kind, "by": notice.by, "card_id": 7, "inbox_item_id": null,
            "title": notice.title, "body": notice.body, "at": 5, "read": notice.read}}})
    }

    #[test]
    fn the_first_read_is_the_baseline_and_notifies_nothing() {
        let mut tally = Tally::default();
        assert_eq!(
            tally.load(&[notice(2, false), notice(1, true)]),
            [Effect::Count(1)]
        );
    }

    #[test]
    fn an_added_notice_is_notified_once() {
        let mut tally = Tally::default();
        tally.load(&[]);
        let added = notice(3, false);
        assert_eq!(
            tally.event(&event("notice_added", &added)),
            [Effect::Notify(added.clone()), Effect::Count(1)]
        );
        assert_eq!(tally.event(&event("notice_added", &added)), []);
    }

    #[test]
    fn reading_a_notice_lowers_the_count_once() {
        let mut tally = Tally::default();
        tally.load(&[notice(3, false), notice(4, false)]);
        let read = notice(3, true);
        assert_eq!(
            tally.event(&event("notice_read", &read)),
            [Effect::Count(1)]
        );
        assert_eq!(tally.event(&event("notice_read", &read)), []);
    }

    #[test]
    fn a_notice_missed_while_the_stream_was_down_is_notified_after() {
        let mut tally = Tally::default();
        tally.load(&[notice(1, false)]);
        assert_eq!(
            tally.load(&[notice(2, false), notice(1, false)]),
            [Effect::Notify(notice(2, false)), Effect::Count(2)]
        );
        // The same read again tells nothing new.
        assert_eq!(
            tally.load(&[notice(2, false), notice(1, false)]),
            [Effect::Count(2)]
        );
    }

    #[test]
    fn other_events_and_a_notice_added_already_read_notify_nothing() {
        let mut tally = Tally::default();
        tally.load(&[]);
        assert_eq!(
            tally.event(&json!({"kind": "card_moved", "data": {"card": 1}})),
            []
        );
        assert_eq!(tally.event(&event("notice_added", &notice(5, true))), []);
    }

    #[test]
    fn a_notice_opens_the_mentions_tab_at_itself_and_says_what_happened() {
        let notice = notice(9, false);
        assert_eq!(notice.path(), "/inbox?kind=mentions&notice=9");
        assert_eq!(
            notice.text(),
            (
                "Fix it".to_string(),
                "@tom mentioned you: Hi @lea".to_string()
            )
        );
        let assigned = Notice {
            kind: "assigned".into(),
            by: None,
            ..notice
        };
        assert_eq!(assigned.text().1, "Given to you");
    }
}
