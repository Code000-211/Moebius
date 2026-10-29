//! The ports of the server, for the Ports view of the desktop app (see [`crate::server::ports`]).
//!
//! - `GET /v1/ports`: the ports that processes of runs, chats and terminals listen on.
//! - `GET /v1/ports/{port}/tunnel`: a WebSocket that carries one TCP connection to `127.0.0.1:<port>` on the server (or
//!   `[::1]:<port>` when only that answers), as binary messages both ways. Only a person opens it; the server listens on
//!   no new interface, and the app is what listens on its own machine.

use std::time::Duration;

use axum::Extension;
use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::{ApiError, ApiState, internal, person_only};
use crate::decide::Actor;
use crate::server::ports::{self, Port};

/// How long the server waits for the port to take a connection.
const CONNECT: Duration = Duration::from_secs(5);

/// The routes of the ports.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/ports", get(list))
        .route("/v1/ports/{port}/tunnel", get(tunnel))
}

async fn list(State(state): State<ApiState>) -> Result<axum::Json<Vec<Port>>, ApiError> {
    let app = state.app.clone();
    let found = tokio::task::spawn_blocking(move || ports::detect(&app))
        .await
        .map_err(internal)?;
    Ok(axum::Json(found))
}

async fn tunnel(
    Extension(by): Extension<Actor>,
    UrlPath(port): UrlPath<u16>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    person_only(&by)?;
    if port == 0 {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "port 0 is no port".into(),
        ));
    }
    let stream = connect(port).await?;
    Ok(upgrade.on_upgrade(move |socket| carry(socket, stream)))
}

/// The connection to the port on the server's loopback address.
async fn connect(port: u16) -> Result<TcpStream, ApiError> {
    let refused = |e: &dyn std::fmt::Display| {
        ApiError(
            StatusCode::BAD_GATEWAY,
            format!("nothing answers on port {port}: {e}"),
        )
    };
    let attempt = async {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => Ok(stream),
            Err(first) => TcpStream::connect(("::1", port)).await.map_err(|_| first),
        }
    };
    match tokio::time::timeout(CONNECT, attempt).await {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(e)) => Err(refused(&e)),
        Err(_) => Err(refused(&"it did not answer in time")),
    }
}

/// Carries bytes between the socket and the connection until either side ends.
async fn carry(socket: WebSocket, stream: TcpStream) {
    let (mut read, mut write) = stream.into_split();
    let (mut send, mut receive) = socket.split();
    let up = async {
        while let Some(Ok(message)) = receive.next().await {
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
    };
    let down = async {
        let mut buffer = vec![0u8; 16 * 1024];
        loop {
            match read.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let bytes = buffer[..n].to_vec().into();
                    if send.send(Message::Binary(bytes)).await.is_err() {
                        return;
                    }
                }
            }
        }
        let close = CloseFrame {
            code: 1000,
            reason: "".into(),
        };
        let _ = send.send(Message::Close(Some(close))).await;
    };
    tokio::select! {
        () = up => {}
        () = down => {}
    }
}

// Linux only: the ports are read from /proc, and the shell is a Unix program.
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use futures_util::{SinkExt, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    use crate::server::api::testing::{get, start, until};

    type Socket = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    async fn open(
        addr: &str,
        token: &str,
        path: &str,
    ) -> Result<Socket, tokio_tungstenite::tungstenite::Error> {
        let mut request = format!("ws://{addr}{path}").into_client_request().unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        tokio_tungstenite::connect_async(request)
            .await
            .map(|(socket, _)| socket)
    }

    fn status_of(e: tokio_tungstenite::tungstenite::Error) -> u16 {
        match e {
            tokio_tungstenite::tungstenite::Error::Http(response) => response.status().as_u16(),
            other => panic!("{other:?}"),
        }
    }

    async fn read_until(terminal: &mut Socket, pattern: &str) -> String {
        let pattern = regex::Regex::new(pattern).unwrap();
        let mut printed = String::new();
        let read = async {
            while let Some(message) = terminal.next().await {
                if let Message::Binary(bytes) = message.unwrap() {
                    printed.push_str(&String::from_utf8_lossy(&bytes));
                    if let Some(found) = pattern.captures(&printed) {
                        return found[1].to_string();
                    }
                }
            }
            panic!("the socket closed before {pattern} was printed: {printed:?}");
        };
        tokio::time::timeout(std::time::Duration::from_secs(60), read)
            .await
            .expect("nothing was printed in a minute")
    }

    /// A server that answers every line with `echo:` and the line, on a port the system chose.
    async fn echo_server() -> (u16, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buffer = [0u8; 1024];
                    while let Ok(n) = stream.read(&mut buffer).await {
                        if n == 0 {
                            break;
                        }
                        let mut answer = b"echo:".to_vec();
                        answer.extend_from_slice(&buffer[..n]);
                        if stream.write_all(&answer).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        (port, task)
    }

    #[tokio::test]
    async fn a_port_a_shell_opens_is_found_and_reached_through_the_tunnel() {
        let (_dir, state, addr) = start().await;
        // A test server listening in a shell of the server's own terminal: python is what the shell starts, and it
        // prints the port the system gave it.
        let mut shell = open(&addr, &state.token, "/v1/terminal").await.unwrap();
        let program = "import socket,sys;s=socket.socket();s.bind(('127.0.0.1',0));s.listen();\
            print('listening-%d-end'%s.getsockname()[1],flush=True);\
            c,_=s.accept();c.sendall(b'hello from the shell');c.recv(10)";
        shell
            .send(Message::Binary(
                format!("python3 -c \"{program}\"\n").into_bytes().into(),
            ))
            .await
            .unwrap();
        let port: u16 = read_until(&mut shell, "listening-(\\d+)-end")
            .await
            .parse()
            .unwrap();

        let found: Vec<super::Port> = loop {
            let found: Vec<super::Port> = get(&state, &addr, "/v1/ports").await;
            if found.iter().any(|p| p.port == port) {
                break found;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        };
        let ours = found.iter().find(|p| p.port == port).unwrap();
        assert_eq!((ours.kind.as_str(), ours.run), ("terminal", None));
        assert!(ours.process.starts_with("python"), "{}", ours.process);

        // The tunnel carries what the program sends.
        let mut tunnel = open(&addr, &state.token, &format!("/v1/ports/{port}/tunnel"))
            .await
            .unwrap();
        assert_eq!(
            read_until(&mut tunnel, "(hello from the shell)").await,
            "hello from the shell"
        );
        tunnel.close(None).await.unwrap();
        shell.close(None).await.unwrap();
    }

    #[tokio::test]
    async fn the_tunnel_carries_bytes_both_ways() {
        let (_dir, state, addr) = start().await;
        let (port, server) = echo_server().await;
        let mut tunnel = open(&addr, &state.token, &format!("/v1/ports/{port}/tunnel"))
            .await
            .unwrap();
        tunnel
            .send(Message::Binary(b"ping".to_vec().into()))
            .await
            .unwrap();
        assert_eq!(read_until(&mut tunnel, "(echo:ping)").await, "echo:ping");
        tunnel
            .send(Message::Binary(vec![0u8, 255, 7].into()))
            .await
            .unwrap();
        let Some(Ok(Message::Binary(bytes))) = tunnel.next().await else {
            panic!("no answer");
        };
        assert_eq!(&bytes[..], b"echo:\0\xff\x07");
        server.abort();
    }

    #[tokio::test]
    async fn the_tunnel_needs_a_person_and_a_port_that_answers() {
        let (_dir, state, addr) = start().await;
        let (port, server) = echo_server().await;
        let path = format!("/v1/ports/{port}/tunnel");
        assert_eq!(
            status_of(open(&addr, "nope", &path).await.unwrap_err()),
            401
        );

        // The run token of an agent is refused.
        let run = state
            .app
            .store
            .add_run(Some(1), "Moebius", "test", "chat")
            .unwrap();
        let agent_token = state.router.issue_token(crate::router::Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        });
        assert_eq!(
            status_of(open(&addr, &agent_token, &path).await.unwrap_err()),
            403
        );
        server.abort();
        until("the port is closed", || {
            std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        })
        .await;
        assert_eq!(
            status_of(open(&addr, &state.token, &path).await.unwrap_err()),
            502
        );
        assert_eq!(
            status_of(
                open(&addr, &state.token, "/v1/ports/0/tunnel")
                    .await
                    .unwrap_err()
            ),
            422
        );
    }

    #[tokio::test]
    async fn a_child_of_a_run_in_its_worktree_is_found_with_its_run() {
        let (_dir, state, addr) = start().await;
        let work = tempfile::tempdir().unwrap();
        let run = state
            .app
            .store
            .add_run(Some(1), "Coder", "test", "code")
            .unwrap();
        state
            .app
            .store
            .set_run_worktree(run.id, &work.path().to_string_lossy())
            .unwrap();
        state.app.store.set_run_status(run.id, "running").unwrap();
        let mut child = std::process::Command::new("python3")
            .args([
                "-c",
                "import socket,sys;s=socket.socket();s.bind(('127.0.0.1',0));s.listen();\
                 print(s.getsockname()[1],flush=True);sys.stdin.read()",
            ])
            .current_dir(work.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(child.stdout.take().unwrap()),
            &mut line,
        )
        .unwrap();
        let port: u16 = line.trim().parse().unwrap();

        let found: Vec<super::Port> = get(&state, &addr, "/v1/ports").await;
        let ours = found.iter().find(|p| p.port == port).expect("the port");
        assert_eq!(
            (ours.kind.as_str(), ours.run, ours.pid),
            ("run", Some(run.id), child.id())
        );

        // Once the run has ended the port is no run's.
        state.app.store.set_run_status(run.id, "succeeded").unwrap();
        let found: Vec<super::Port> = get(&state, &addr, "/v1/ports").await;
        assert!(found.iter().all(|p| p.port != port));
        child.kill().unwrap();
        child.wait().unwrap();
    }
}
