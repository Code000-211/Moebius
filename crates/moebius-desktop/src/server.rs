//! A server the app connects to: its address, the access token it is signed in to with, and the check that the server
//! is of this app's release line before the bundled web app runs against it.
//!
//! The check is one `GET /v1/health` with the token: it proves the token and answers the server's version. The web app
//! in the app was built for the API of this app's version, so a server of another minor version (another major one,
//! or for `0.x` another `x`) is refused with the release whose app matches it.

use std::time::Duration;

use serde::Deserialize;

/// This app's version, which is the version of the web app it bundles.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the releases are published.
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// How long the check waits for the server.
const TIMEOUT: Duration = Duration::from_secs(10);

/// A server and the token the app signs in to it with.
#[derive(Clone)]
pub struct Server {
    /// Its address as a URL, such as `http://127.0.0.1:7171`, without a `/` at the end.
    pub url: String,
    pub token: String,
}

impl Server {
    /// The server at `address` (with or without `http://`), signed in to with `token`.
    pub fn new(address: &str, token: &str) -> Self {
        Self {
            url: address_url(address),
            token: token.trim().to_string(),
        }
    }
}

/// Why the app does not open the web app against a server.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConnectError {
    #[error("give the server's address and an access token")]
    Missing,
    #[error("cannot reach {url}: {reason}")]
    Unreachable { url: String, reason: String },
    #[error("the server does not accept this access token")]
    Token,
    #[error("{url} answered {status} where a Moebius server answers its version")]
    NotMoebius { url: String, status: u16 },
    #[error(
        "the server runs Moebius {server}, and this app is Moebius {app}, which works with servers of {line} only; \
         install the app of release v{server}, which matches the server ({release}), or upgrade the server to {line}",
        line = release_line(.app).unwrap_or_default(),
        release = release_url(.server)
    )]
    OtherRelease { server: String, app: String },
}

#[derive(Deserialize)]
struct Health {
    version: String,
}

/// The one HTTP client of the app: its connections to a server are kept and used again, so that a read of the tray or
/// the rail is not a new connection (and, over https, a new handshake) each time.
pub fn http() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// Checks that `server` accepts its token and runs this app's release line, and returns the server's version.
pub async fn check(server: &Server) -> Result<String, ConnectError> {
    check_against(server, VERSION).await
}

/// [`check`] for an app of version `app`.
pub async fn check_against(server: &Server, app: &str) -> Result<String, ConnectError> {
    if server.token.is_empty() || server.url == "http://" {
        return Err(ConnectError::Missing);
    }
    let unreachable = |e: reqwest::Error| ConnectError::Unreachable {
        url: server.url.clone(),
        reason: e.without_url().to_string(),
    };
    let response = http()
        .get(format!("{}/v1/health", server.url))
        .bearer_auth(&server.token)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(unreachable)?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ConnectError::Token);
    }
    let not_moebius = || ConnectError::NotMoebius {
        url: server.url.clone(),
        status: status.as_u16(),
    };
    if !status.is_success() {
        return Err(not_moebius());
    }
    let health: Health = response.json().await.map_err(|_| not_moebius())?;
    if release_line(&health.version).is_none() || release_line(&health.version) != release_line(app)
    {
        return Err(ConnectError::OtherRelease {
            server: health.version,
            app: app.to_string(),
        });
    }
    Ok(health.version)
}

/// The name of the computer a server runs on, as it reports it (`GET /v1/server`), or none: a server that does not say
/// is not an error, the rail names it by its address. It waits three seconds at most.
pub async fn host_of(server: &Server) -> Option<String> {
    #[derive(Deserialize)]
    struct Where {
        host: String,
    }
    let response = http()
        .get(format!("{}/v1/server", server.url))
        .bearer_auth(&server.token)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    Some(response.json::<Where>().await.ok()?.host)
}

/// The major and minor version of `version` (`0.13` of `0.13.2`), which a server and the app must share.
pub fn release_line(version: &str) -> Option<String> {
    let mut parts = version.trim().split('.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next()?.parse().ok()?;
    Some(format!("{major}.{minor}"))
}

/// The page of the release `v<version>`, which holds the app of that version.
fn release_url(version: &str) -> String {
    format!("{REPOSITORY}/releases/tag/v{}", version.trim())
}

/// `http://` before an address that has no scheme, and no `/` at the end, as the command line reads an address.
fn address_url(address: &str) -> String {
    let address = address.trim().trim_end_matches('/');
    if address.contains("://") {
        address.to_string()
    } else {
        format!("http://{address}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_line_is_the_major_and_minor_version() {
        assert_eq!(release_line("0.13.2").as_deref(), Some("0.13"));
        assert_eq!(release_line("1.0.0-dev").as_deref(), Some("1.0"));
        assert_eq!(release_line("0.13").as_deref(), Some("0.13"));
        assert_eq!(release_line("thirteen"), None);
        assert_eq!(release_line(""), None);
    }

    #[test]
    fn an_address_without_a_scheme_is_http() {
        assert_eq!(address_url("127.0.0.1:7171/"), "http://127.0.0.1:7171");
        assert_eq!(
            address_url(" https://moebius.example "),
            "https://moebius.example"
        );
    }

    #[test]
    fn the_refusal_names_the_release_that_matches() {
        let refusal = ConnectError::OtherRelease {
            server: "0.12.4".into(),
            app: "0.13.0".into(),
        }
        .to_string();
        assert!(refusal.contains("Moebius 0.12.4"), "{refusal}");
        assert!(refusal.contains("servers of 0.13 only"), "{refusal}");
        assert!(
            refusal.contains(&format!("{REPOSITORY}/releases/tag/v0.12.4")),
            "{refusal}"
        );
    }

    #[tokio::test]
    async fn nothing_is_asked_without_an_address_or_a_token() {
        for (address, token) in [("", "t"), ("127.0.0.1:1", " ")] {
            assert_eq!(
                check(&Server::new(address, token)).await,
                Err(ConnectError::Missing)
            );
        }
    }

    #[tokio::test]
    async fn a_server_that_reports_its_host_names_itself_and_one_that_does_not_is_nameless() {
        use axum::{Json, Router, routing::get};
        let app = Router::new().route(
            "/v1/server",
            get(|| async { Json(serde_json::json!({"host": "build-box", "os": "linux"})) }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let named = Server::new(&address.to_string(), "t");
        assert_eq!(host_of(&named).await.as_deref(), Some("build-box"));
        // A port nobody listens on answers nothing.
        let nameless = Server::new("127.0.0.1:1", "t");
        assert_eq!(host_of(&nameless).await, None);
    }
}
