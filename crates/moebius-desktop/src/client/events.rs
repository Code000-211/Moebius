//! The event stream of the server (`GET /v1/events`) as an iced subscription that connects again by itself, and the
//! WebSocket connection with the token that the terminal shares.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use iced::futures::channel::mpsc;
use serde::Deserialize;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::settings::Settings;

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

pub(super) async fn connect(
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
