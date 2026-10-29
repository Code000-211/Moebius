//! A shell on the server over a WebSocket (`GET /v1/runs/{id}/terminal`, `GET /v1/terminal`, `GET /v1/chats/{id}/tui`):
//! the bytes it prints, and the keys and sizes the app sends it.

use futures_util::{SinkExt, StreamExt};
use iced::futures::channel::mpsc;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::settings::Settings;

use super::events::connect;

/// What a shell sends the app.
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

/// A shell on the WebSocket `path` (`/v1/runs/{id}/terminal` for a run's worktree, `/v1/terminal` for the server) for as
/// long as the stream is held: dropping it closes the socket, which kills the shell.
pub fn terminal(
    settings: Settings,
    path: String,
) -> impl iced::futures::Stream<Item = TerminalEvent> {
    iced::stream::channel(64, async move |mut out: mpsc::Sender<TerminalEvent>| {
        let mut socket = match connect(&settings, &path).await {
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
