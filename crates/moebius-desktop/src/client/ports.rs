//! The ports of the server (`GET /v1/ports`) and the forwarder that makes one of them reachable on this computer: it
//! listens on a port of `127.0.0.1` and carries every connection made to it over the tunnel WebSocket
//! (`GET /v1/ports/{port}/tunnel`). It uses only the API's address and token, so it works the same when the address is
//! the local end of an SSH tunnel.

use futures_util::{SinkExt, StreamExt};
use iced::futures::channel::mpsc;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::settings::Settings;

use super::events::connect;

/// A port something on the server listens on, and what it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Port {
    pub port: u16,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub process: String,
    /// `run`, `chat` or `terminal`.
    pub kind: String,
    /// The run or chat; for a terminal, the run or chat it was opened on, none for the server's own terminal.
    #[serde(default)]
    pub run: Option<i64>,
}

/// What a forwarder tells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardEvent {
    /// The port on the server that the forwarder is for.
    pub remote: u16,
    pub what: Forwarding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forwarding {
    /// The forwarder listens on this port of `127.0.0.1`.
    Listening(u16),
    /// It cannot listen; nothing follows.
    Failed(String),
    /// A connection to the local port could not be carried to the server.
    Refused(String),
}

/// The forwarder of `remote` listening on `127.0.0.1:local` (any free port when `local` is 0), for as long as the stream
/// is held: dropping it closes the listener and the connections it carries.
pub fn forward(
    settings: Settings,
    remote: u16,
    local: u16,
) -> impl iced::futures::Stream<Item = ForwardEvent> {
    iced::stream::channel(16, async move |mut out: mpsc::Sender<ForwardEvent>| {
        match TcpListener::bind(("127.0.0.1", local)).await {
            Ok(listener) => serve(listener, settings, remote, out).await,
            Err(e) => {
                let what = Forwarding::Failed(format!("cannot listen on 127.0.0.1:{local}: {e}"));
                let _ = out.send(ForwardEvent { remote, what }).await;
            }
        }
    })
}

/// Accepts connections on `listener` and carries each to `remote` on the server, until the future is dropped.
pub async fn serve(
    listener: TcpListener,
    settings: Settings,
    remote: u16,
    mut out: mpsc::Sender<ForwardEvent>,
) {
    if let Ok(address) = listener.local_addr() {
        let what = Forwarding::Listening(address.port());
        let _ = out.send(ForwardEvent { remote, what }).await;
    }
    // Dropping the set ends the connections that are being carried.
    let mut carried: JoinSet<()> = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((connection, _)) = accepted else { continue };
                let (settings, mut out) = (settings.clone(), out.clone());
                carried.spawn(async move {
                    if let Err(reason) = carry(&settings, remote, connection).await {
                        let what = Forwarding::Refused(reason);
                        let _ = out.send(ForwardEvent { remote, what }).await;
                    }
                });
            }
            Some(_) = carried.join_next(), if !carried.is_empty() => {}
        }
    }
}

/// Carries one connection to `remote` over the tunnel until either end closes.
async fn carry(settings: &Settings, remote: u16, connection: TcpStream) -> Result<(), String> {
    let socket = connect(settings, &format!("/v1/ports/{remote}/tunnel"))
        .await
        .map_err(describe)?;
    let (mut to_server, mut from_server) = socket.split();
    let (mut read, mut write) = connection.into_split();
    let up = async {
        let mut buffer = vec![0u8; 16 * 1024];
        loop {
            match read.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if to_server
                        .send(Message::Binary(buffer[..n].to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        let _ = to_server.send(Message::Close(None)).await;
    };
    let down = async {
        while let Some(Ok(message)) = from_server.next().await {
            match message {
                Message::Binary(bytes) => {
                    if write.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        let _ = write.shutdown().await;
    };
    tokio::select! {
        () = up => {}
        () = down => {}
    }
    Ok(())
}

fn describe(e: tungstenite::Error) -> String {
    match e {
        tungstenite::Error::Http(response) => format!(
            "the server answered {}: {}",
            response.status().as_u16(),
            String::from_utf8_lossy(response.body().as_deref().unwrap_or_default())
        ),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_is_read_with_what_the_server_says_of_it() {
        let port: Port = serde_json::from_str(
            r#"{"port":3000,"pid":42,"process":"node","kind":"terminal","run":null,"more":1}"#,
        )
        .unwrap();
        assert_eq!(
            (port.port, port.kind.as_str(), port.run),
            (3000, "terminal", None)
        );
        let port: Port = serde_json::from_str(r#"{"port":8080,"kind":"run","run":7}"#).unwrap();
        assert_eq!((port.process.as_str(), port.run), ("", Some(7)));
    }
}
