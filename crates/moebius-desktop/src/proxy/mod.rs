//! The app's own address on the loopback, where the window shows the web app: the bundled build at every path outside
//! `/v1/` (`assets`), and `/v1/` forwarded to the server with the access token (`forward`), HTTP and WebSockets alike.
//! The web app thus talks to its own address, as it does when the server serves it, and never holds the token.
//!
//! Other programs and other people on the machine can reach the loopback too, and a web page in a browser can send
//! requests to it, so the proxy forwards only what the window sends:
//!
//! - the window first opens [`Proxy::entry_url`], which holds a random key and sets it as a cookie (`HttpOnly`,
//!   `SameSite=Strict`); every request to `/v1/` must carry that cookie;
//! - every request must name the proxy's own address in `Host`, which a page that rebinds a name to the loopback does
//!   not;
//! - a request to `/v1/` with an `Origin` must name the proxy's own, which a web page of another origin cannot.
//!
//! The files of the web app are answered without the cookie (they hold no data), since the sandboxed frame of a
//! plugin's page has no origin and sends no cookie for its fonts.

mod assets;
mod forward;

pub use assets::{Files, embedded};

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::server::Server;

/// The path the window opens first, which sets the cookie and leads to the web app.
pub const ENTRY: &str = "/.moebius-desktop/enter";

/// The start of the name of the cookie that proves a request comes from the window; the proxy's port ends it, since
/// a cookie is kept per host and not per port, and every proxy of the app is on `127.0.0.1`.
const COOKIE: &str = "moebius_desktop_";

/// The proxy, running on the loopback until it is dropped. Dropping it stops the listener, closes connections once
/// their request is answered, and ends the WebSockets it relays, so nothing of it goes on with the server's token.
pub struct Proxy {
    addr: SocketAddr,
    key: String,
    stop: tokio::sync::watch::Sender<bool>,
}

impl Proxy {
    /// Where the proxy listens.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The address the window opens first: it sets the cookie and leads to the web app.
    pub fn entry_url(&self) -> String {
        format!("http://{}{ENTRY}?key={}", self.addr, self.key)
    }

    /// The origin of the web app it serves, `http://127.0.0.1:<port>`.
    pub fn origin(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// [`Proxy::entry_url`] that leads to the page `path` of the web app instead of its start (a person's place when
    /// they come back to this server). A `path` that is not a page of the web app leads to the start.
    pub fn entry_url_at(&self, path: &str) -> String {
        let mut url = reqwest::Url::parse(&self.entry_url()).expect("the entry is a URL");
        url.query_pairs_mut().append_pair("to", path);
        url.into()
    }
}

/// `path` when it is a page of the web app (an absolute path outside `/v1/`, with no host, no control character and no
/// backslash in it), else `/`.
fn page_path(path: &str) -> &str {
    let page = path.starts_with('/')
        && !path.starts_with("//")
        && path != "/v1"
        && !path.starts_with("/v1/")
        && !path.starts_with(ENTRY)
        && !path.contains(|c: char| c.is_control() || c == '\\');
    if page { path } else { "/" }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

/// What every request of the proxy needs.
struct Shared {
    server: Server,
    files: Files,
    /// The proxy's own `Host`, `127.0.0.1:<port>`.
    host: String,
    key: String,
    http: reqwest::Client,
    /// Turns true when the proxy is dropped.
    stopped: tokio::sync::watch::Receiver<bool>,
}

impl Shared {
    fn cookie(&self) -> String {
        format!(
            "{COOKIE}{}",
            self.host.rsplit(':').next().unwrap_or_default()
        )
    }
}

/// Waits until `stopped` turns true, or its proxy is gone.
async fn until_stopped(mut stopped: tokio::sync::watch::Receiver<bool>) {
    let _ = stopped.wait_for(|stopped| *stopped).await;
}

/// Starts a proxy on a free port of the loopback that serves the web app from `files` and forwards `/v1/` to `server`.
pub async fn start(server: Server, files: Files) -> std::io::Result<Proxy> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let key = random_key()?;
    let http = reqwest::Client::builder()
        // A redirect is the web app's to follow, so it is answered as it came.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(std::io::Error::other)?;
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let shared = Arc::new(Shared {
        server,
        files,
        host: addr.to_string(),
        key: key.clone(),
        http,
        stopped: stopped.clone(),
    });
    let router = Router::new()
        .route(ENTRY, axum::routing::get(enter))
        .fallback(answer)
        .with_state(shared);
    tokio::spawn(async move {
        // The proxy ends when the app drops it; an error of the listener ends it too.
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(until_stopped(stopped))
            .await;
    });
    Ok(Proxy { addr, key, stop })
}

/// A new key: 32 random bytes as hex.
fn random_key() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(hex::encode(bytes))
}

#[derive(Deserialize)]
struct Entry {
    #[serde(default)]
    key: String,
    /// The page to lead to, as a path with its query.
    #[serde(default)]
    to: String,
}

/// `GET /.moebius-desktop/enter?key=<key>`: with the right key, sets the cookie and leads to `to`, or `/`.
async fn enter(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Query(entry): Query<Entry>,
) -> Response {
    if !own_host(&shared, &headers) || !same(entry.key.as_bytes(), shared.key.as_bytes()) {
        return (
            StatusCode::FORBIDDEN,
            "this address is the desktop app's own",
        )
            .into_response();
    }
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, page_path(&entry.to).to_string()),
            (
                header::SET_COOKIE,
                format!(
                    "{}={}; HttpOnly; SameSite=Strict; Path=/",
                    shared.cookie(),
                    shared.key
                ),
            ),
        ],
    )
        .into_response()
}

/// Every other request: `/v1/` forwarded when it comes from the window, any other path a file of the web app.
async fn answer(State(shared): State<Arc<Shared>>, request: Request) -> Response {
    if !own_host(&shared, request.headers()) {
        return (
            StatusCode::FORBIDDEN,
            "this address is the desktop app's own",
        )
            .into_response();
    }
    let path = request.uri().path();
    if path == "/v1" || path.starts_with("/v1/") {
        if !from_window(&shared, request.headers()) {
            return (
                StatusCode::FORBIDDEN,
                "only the desktop app's window may ask this",
            )
                .into_response();
        }
        return forward::forward(&shared, request).await;
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    assets::serve(shared.files, path, &shared.host)
}

/// Whether `Host` names the proxy itself.
fn own_host(shared: &Shared, headers: &HeaderMap) -> bool {
    headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| host == shared.host)
}

/// Whether a request carries the window's cookie and, when it has an `Origin`, the proxy's own.
fn from_window(shared: &Shared, headers: &HeaderMap) -> bool {
    let origin_ok = match headers.get(header::ORIGIN) {
        None => true,
        Some(origin) => origin.as_bytes() == format!("http://{}", shared.host).as_bytes(),
    };
    let cookie = shared.cookie();
    let cookie_ok = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .any(|(name, value)| name == cookie && same(value.as_bytes(), shared.key.as_bytes()));
    origin_ok && cookie_ok
}

/// Compares two secrets in a time that does not depend on where they differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::page_path;

    #[test]
    fn only_a_page_of_the_web_app_is_led_to() {
        assert_eq!(page_path("/fleet?tab=2"), "/fleet?tab=2");
        for other in [
            "",
            "fleet",
            "//evil.example/x",
            "/v1/snapshot",
            "/v1",
            "/.moebius-desktop/enter?key=x",
            "/a\\b",
            "/a\nb",
        ] {
            assert_eq!(page_path(other), "/", "{other:?}");
        }
    }
}
