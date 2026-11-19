//! The app against servers on port 0: it signs in to a real server with a person's access token and shows the web app
//! through its proxy, with the API and the event stream behind it, and it refuses a fake server of another minor
//! version, naming the release that matches.

use std::borrow::Cow;
use std::path::Path;

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use moebius_core::config::Config;
use moebius_core::server::Server as MoebiusServer;
use moebius_core::server::api::{self, ApiState};
use moebius_desktop::proxy::{self, ENTRY, Proxy};
use moebius_desktop::server::{self, ConnectError, Server, VERSION};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

/// The web app as the tests build it: an index and one script.
fn files(name: &str) -> Option<Cow<'static, [u8]>> {
    match name {
        "index.html" => Some(Cow::Borrowed(
            b"<!doctype html><title>Moebius web app</title>",
        )),
        "assets/app.js" => Some(Cow::Borrowed(b"export {}")),
        _ => None,
    }
}

/// A real server whose data is in `dir`, on port 0, and its address and the server's token.
async fn serve(dir: &Path) -> (MoebiusServer, String, String) {
    let toml = "";
    let file = dir.join("moebius.toml");
    std::fs::write(&file, toml).unwrap();
    let config = Config::parse(toml, dir, &file).unwrap();
    let server = MoebiusServer::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
    let state = ApiState {
        codes: Default::default(),
        app: server.app.clone(),
        runs: server.harness.clone(),
        harness_repo: None,
        token: server.token.clone(),
        router: server.router.clone(),
        config: server.live.clone(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(api::serve(listener, state));
    let token = server.token.to_string();
    (server, base, token)
}

/// A new access token of the first owner, issued with the server's token.
async fn access_token(base: &str, server_token: &str) -> String {
    let issued: Value = reqwest::Client::new()
        .post(format!("{base}/v1/tokens"))
        .bearer_auth(server_token)
        .json(&json!({ "name": "desktop" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    issued["token"].as_str().unwrap().to_string()
}

/// Opens the proxy's entry as the window does and returns the cookie it sets.
async fn enter(proxy: &Proxy) -> String {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = client.get(proxy.entry_url()).send().await.unwrap();
    assert_eq!(response.status(), 303);
    assert_eq!(response.headers()["location"], "/");
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    // Cookies are kept per host and not per port, so each proxy's cookie has a name of its own.
    let name = format!("moebius_desktop_{}=", proxy.addr().port());
    assert!(cookie.starts_with(&name), "{cookie}");
    assert!(
        cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"),
        "{cookie}"
    );
    cookie.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn it_signs_in_to_a_server_on_port_0_with_an_access_token_and_shows_the_web_app() {
    let dir = tempfile::tempdir().unwrap();
    let (_server, base, server_token) = serve(dir.path()).await;
    let token = access_token(&base, &server_token).await;
    let connected = Server::new(&base, &token);
    assert_eq!(server::check(&connected).await, Ok(VERSION.to_string()));

    let proxy = proxy::start(connected, files).await.unwrap();
    let cookie = enter(&proxy).await;
    let at = |path: &str| format!("http://{}{path}", proxy.addr());
    let client = reqwest::Client::new();

    // The web app, at any path it routes, with its policy naming the proxy's own WebSocket.
    let page = client.get(at("/board/demo")).send().await.unwrap();
    assert_eq!(page.status(), 200);
    let policy = page.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        policy.contains(&format!("ws://{}", proxy.addr())),
        "{policy}"
    );
    assert!(page.text().await.unwrap().contains("Moebius web app"));

    // The API, signed in as the person whose token it is, without the web app ever holding the token.
    let session: Value = client
        .get(at("/v1/session"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(session["signed_in"], true);
    assert_eq!(session["person"]["handle"], "owner");
    let health = client
        .get(at("/v1/health"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), 200);

    // A change goes through with its body and the header the web app sends.
    let issued = client
        .post(at("/v1/tokens"))
        .header("cookie", &cookie)
        .header("x-moebius", "1")
        .header("origin", format!("http://{}", proxy.addr()))
        .json(&json!({ "name": "second" }))
        .send()
        .await
        .unwrap();
    assert_eq!(issued.status(), 200);
    assert!(issued.headers().get("set-cookie").is_none());

    // The event stream is a WebSocket through the proxy too.
    let mut request = format!("ws://{}/v1/events", proxy.addr())
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let (_socket, answer) = tokio_tungstenite::connect_async(request).await.unwrap();
    assert_eq!(answer.status(), 101);
}

#[tokio::test]
async fn a_wrong_token_is_refused_before_anything_opens() {
    let dir = tempfile::tempdir().unwrap();
    let (_server, base, _) = serve(dir.path()).await;
    assert_eq!(
        server::check(&Server::new(&base, "mbs_nope")).await,
        Err(ConnectError::Token)
    );
}

/// A fake server that answers `/v1/health` with `version` to the token `t`, echoes a WebSocket at `/v1/echo`
/// after saying which `Authorization` it was opened with, and answers `/v1/headers` with how a body was sent.
async fn fake(version: &'static str) -> String {
    let app = axum::Router::new()
        .route(
            "/v1/health",
            axum::routing::get(move |headers: HeaderMap| async move {
                if headers.get("authorization").is_none_or(|v| v != "Bearer t") {
                    return axum::http::StatusCode::UNAUTHORIZED.into_response();
                }
                axum::Json(json!({ "version": version })).into_response()
            }),
        )
        .route(
            "/v1/headers",
            axum::routing::any(|headers: HeaderMap| async move {
                let value = |name: &str| headers.get(name).map(|v| v.to_str().unwrap().to_string());
                axum::Json(json!({
                    "content_length": value("content-length"),
                    "transfer_encoding": value("transfer-encoding"),
                }))
            }),
        )
        .route(
            "/v1/echo",
            axum::routing::get(|headers: HeaderMap, upgrade: WebSocketUpgrade| async move {
                let authorization = headers
                    .get("authorization")
                    .map(|v| v.to_str().unwrap().to_string())
                    .unwrap_or_default();
                let cookie = headers.contains_key("cookie");
                upgrade.on_upgrade(move |mut socket| async move {
                    let said = format!("{authorization}; cookie {cookie}");
                    socket.send(Message::text(said)).await.unwrap();
                    while let Some(Ok(message)) = socket.recv().await {
                        if socket.send(message).await.is_err() {
                            break;
                        }
                    }
                })
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

#[tokio::test]
async fn a_server_of_another_minor_version_is_refused_with_the_release_that_matches() {
    let base = fake("0.12.4").await;
    let refused = server::check_against(&Server::new(&base, "t"), "0.13.0")
        .await
        .unwrap_err();
    assert_eq!(
        refused,
        ConnectError::OtherRelease {
            server: "0.12.4".into(),
            app: "0.13.0".into()
        }
    );
    let message = refused.to_string();
    assert!(
        message.contains("https://github.com/Code000-211/Moebius/releases/tag/v0.12.4"),
        "{message}"
    );

    // Another patch version of the same minor one is the same release line.
    let base = fake("0.13.7").await;
    assert_eq!(
        server::check_against(&Server::new(&base, "t"), "0.13.0").await,
        Ok("0.13.7".to_string())
    );
    // So is the server's own version for this app.
    let base = fake(VERSION).await;
    assert_eq!(
        server::check(&Server::new(&base, "t")).await,
        Ok(VERSION.to_string())
    );
}

#[tokio::test]
async fn an_address_that_answers_no_version_is_not_a_moebius_server() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, axum::Router::new()).await.unwrap() });
    assert!(matches!(
        server::check(&Server::new(&base, "t")).await,
        Err(ConnectError::NotMoebius { status: 404, .. })
    ));
    drop(base);
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gone = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    assert!(matches!(
        server::check(&Server::new(&gone, "t")).await,
        Err(ConnectError::Unreachable { .. })
    ));
}

#[tokio::test]
async fn a_socket_is_passed_both_ways_with_the_token_and_without_the_cookie() {
    let base = fake(VERSION).await;
    let proxy = proxy::start(Server::new(&base, "t"), files).await.unwrap();
    let cookie = enter(&proxy).await;
    let mut request = format!("ws://{}/v1/echo", proxy.addr())
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let first = socket.next().await.unwrap().unwrap();
    assert_eq!(first, tungstenite::Message::text("Bearer t; cookie false"));
    socket
        .send(tungstenite::Message::text("hello"))
        .await
        .unwrap();
    assert_eq!(
        socket.next().await.unwrap().unwrap(),
        tungstenite::Message::text("hello")
    );
}

#[tokio::test]
async fn only_the_window_may_use_the_proxy() {
    let base = fake(VERSION).await;
    let proxy = proxy::start(Server::new(&base, "t"), files).await.unwrap();
    let at = |path: &str| format!("http://{}{path}", proxy.addr());
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    // Without the key, the entry sets nothing.
    let wrong = client
        .get(at(&format!("{ENTRY}?key=nope")))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 403);
    assert!(wrong.headers().get("set-cookie").is_none());

    // Without the cookie, the API is not forwarded; the files of the web app are answered.
    let no_cookie = client.get(at("/v1/health")).send().await.unwrap();
    assert_eq!(no_cookie.status(), 403);
    assert_eq!(client.get(at("/")).send().await.unwrap().status(), 200);

    let cookie = enter(&proxy).await;
    let with = |request: reqwest::RequestBuilder| request.header("cookie", &cookie);
    assert_eq!(
        with(client.get(at("/v1/health")))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    // A page of another origin is refused even with the cookie, and so is a name rebound to the loopback.
    let foreign = with(client.post(at("/v1/health")))
        .header("origin", "https://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(foreign.status(), 403);
    let rebound = with(client.get(at("/v1/health")))
        .header("host", "evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(rebound.status(), 403);
    // Neither may open a socket.
    let mut request = format!("ws://{}/v1/echo", proxy.addr())
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("origin", "https://evil.example".parse().unwrap());
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    assert!(tokio_tungstenite::connect_async(request).await.is_err());
}

#[tokio::test]
async fn a_body_keeps_its_length_and_a_request_without_one_is_sent_without_one() {
    let base = fake(VERSION).await;
    let proxy = proxy::start(Server::new(&base, "t"), files).await.unwrap();
    let cookie = enter(&proxy).await;
    let url = format!("http://{}/v1/headers", proxy.addr());
    let client = reqwest::Client::new();
    let sent: Value = client
        .get(&url)
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        sent,
        json!({ "content_length": null, "transfer_encoding": null })
    );
    let sent: Value = client
        .post(&url)
        .header("cookie", &cookie)
        .body("{\"name\":\"x\"}")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        sent,
        json!({ "content_length": "12", "transfer_encoding": null })
    );
}

#[tokio::test]
async fn a_dropped_proxy_ends_the_sockets_it_relays() {
    let base = fake(VERSION).await;
    let proxy = proxy::start(Server::new(&base, "t"), files).await.unwrap();
    let cookie = enter(&proxy).await;
    let mut request = format!("ws://{}/v1/echo", proxy.addr())
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket.next().await.unwrap().unwrap();
    drop(proxy);
    // The socket ends: closed, or cut off, but no longer carried to the server with the token.
    match socket.next().await {
        None | Some(Err(_)) | Some(Ok(tungstenite::Message::Close(_))) => {}
        Some(Ok(other)) => panic!("the socket goes on after the proxy stopped: {other:?}"),
    }
}
