//! Forwarding a request of the window to the server: HTTP with its body and answer streamed through, and a WebSocket
//! (the event stream, a terminal, a port) as a second WebSocket to the server whose messages are passed both ways.
//!
//! The access token is added as `Authorization: Bearer`; what only concerns the window and the proxy (its cookie, its
//! `Host` and `Origin`, hop-by-hop headers) is not passed on, nor is a cookie the server sets, since the token signs
//! in every request.

use axum::body::{Body, HttpBody};
use axum::extract::FromRequestParts;
use axum::extract::Request;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt, TryStreamExt};
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use super::{Shared, until_stopped};
use crate::server::Server;

/// Headers that are the window's or the proxy's own and are never passed to the server.
const WINDOW_HEADERS: &[HeaderName] = &[
    header::HOST,
    header::AUTHORIZATION,
    header::COOKIE,
    header::ORIGIN,
    header::REFERER,
    header::CONNECTION,
    header::UPGRADE,
    header::TE,
    header::TRAILER,
    header::TRANSFER_ENCODING,
    header::PROXY_AUTHORIZATION,
];

/// Headers of the server's answer that are not passed to the window.
const ANSWER_HEADERS: &[HeaderName] = &[
    header::SET_COOKIE,
    header::CONNECTION,
    header::TRANSFER_ENCODING,
    header::CONTENT_LENGTH,
    header::TRAILER,
];

/// Forwards `request` (a path under `/v1/`) to the server of `shared`.
pub(super) async fn forward(shared: &Shared, request: Request) -> Response {
    let server = &shared.server;
    let upgrade = request
        .headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    if upgrade {
        return socket(server, shared.stopped.clone(), request).await;
    }
    let path = request
        .uri()
        .path_and_query()
        .map_or("/", |path| path.as_str())
        .to_string();
    let (parts, body) = request.into_parts();
    let mut headers = parts.headers;
    strip(&mut headers, WINDOW_HEADERS);
    let mut outgoing = shared
        .http
        .request(parts.method, format!("{}{path}", server.url))
        .bearer_auth(&server.token);
    // A request without a body goes without one, and one of a known length keeps its `Content-Length`, so that a
    // reverse proxy in front of the server never sees a `GET` with a chunked body.
    if body.size_hint().exact() == Some(0) {
        headers.remove(header::CONTENT_LENGTH);
    } else {
        outgoing = outgoing.body(reqwest::Body::wrap_stream(body.into_data_stream()));
    }
    let sent = outgoing.headers(headers).send().await;
    let answer = match sent {
        Ok(answer) => answer,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("cannot reach {}: {}", server.url, e.without_url()),
            )
                .into_response();
        }
    };
    let status = answer.status();
    let mut headers = answer.headers().clone();
    strip(&mut headers, ANSWER_HEADERS);
    let mut response =
        Body::from_stream(answer.bytes_stream().map_err(std::io::Error::other)).into_response();
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

fn strip(headers: &mut HeaderMap, names: &[HeaderName]) {
    for name in names {
        headers.remove(name);
    }
}

/// Answers the upgrade of the window and connects a WebSocket to the same path of the server, then passes messages
/// both ways until either side closes. A server that refuses the socket answers the window with its status.
async fn socket(
    server: &Server,
    stopped: tokio::sync::watch::Receiver<bool>,
    request: Request,
) -> Response {
    let path = request
        .uri()
        .path_and_query()
        .map_or("/", |path| path.as_str())
        .to_string();
    let (mut parts, _) = request.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(upgrade) => upgrade,
        Err(rejection) => return rejection.into_response(),
    };
    let url = format!("{}{path}", server.url)
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let mut outgoing = match url.as_str().into_client_request() {
        Ok(outgoing) => outgoing,
        Err(e) => return (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    };
    match format!("Bearer {}", server.token).parse() {
        Ok(value) => outgoing.headers_mut().insert(header::AUTHORIZATION, value),
        Err(_) => return (StatusCode::BAD_GATEWAY, "the token is not a header").into_response(),
    };
    let upstream = match tokio_tungstenite::connect_async(outgoing).await {
        Ok((upstream, _)) => upstream,
        Err(tungstenite::Error::Http(refused)) => {
            return StatusCode::from_u16(refused.status().as_u16())
                .unwrap_or(StatusCode::BAD_GATEWAY)
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("cannot reach {}: {e}", server.url),
            )
                .into_response();
        }
    };
    upgrade.on_upgrade(move |window| relay(window, upstream, stopped))
}

/// Passes messages between the window's socket and the server's until one of them ends or the proxy stops.
async fn relay(
    window: WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    stopped: tokio::sync::watch::Receiver<bool>,
) {
    let (mut to_window, mut from_window) = window.split();
    let (mut to_server, mut from_server) = upstream.split();
    let up = async {
        while let Some(Ok(message)) = from_window.next().await {
            let message = match message {
                Message::Text(text) => tungstenite::Message::text(text.as_str()),
                Message::Binary(bytes) => tungstenite::Message::binary(bytes),
                Message::Ping(bytes) => tungstenite::Message::Ping(bytes),
                Message::Pong(bytes) => tungstenite::Message::Pong(bytes),
                Message::Close(_) => break,
            };
            if to_server.send(message).await.is_err() {
                break;
            }
        }
        let _ = to_server.close().await;
    };
    let down = async {
        while let Some(Ok(message)) = from_server.next().await {
            let message = match message {
                tungstenite::Message::Text(text) => Message::text(text.as_str()),
                tungstenite::Message::Binary(bytes) => Message::binary(bytes),
                tungstenite::Message::Ping(bytes) => Message::Ping(bytes),
                tungstenite::Message::Pong(bytes) => Message::Pong(bytes),
                tungstenite::Message::Close(_) => break,
                tungstenite::Message::Frame(_) => continue,
            };
            if to_window.send(message).await.is_err() {
                break;
            }
        }
        let _ = to_window.close().await;
    };
    // When one side ends, or the proxy stops, both are dropped, which closes them.
    tokio::select! {
        () = up => {}
        () = down => {}
        () = until_stopped(stopped) => {}
    }
}
