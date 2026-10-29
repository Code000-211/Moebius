//! The connection to a running server: where it is (the flags, the desktop app's saved settings or `moebius.toml`),
//! the one [`Request`] a command makes, and the [`Client`] that sends it over HTTP with the token.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Method;
use serde::Deserialize;
use serde_json::Value;

/// The server a command talks to and the token that proves who it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub url: String,
    pub token: String,
}

/// What the desktop app saved: `<config dir>/moebius/desktop.toml`.
#[derive(Debug, Deserialize)]
struct Saved {
    address: String,
    token: String,
}

/// The desktop app's settings file of this person, if the platform has a config directory.
pub fn desktop_settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("moebius").join("desktop.toml"))
}

/// The address and token the desktop app saved; none when it saved nothing.
fn desktop_settings(path: Option<&Path>) -> Result<Option<Connection>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let saved: Saved =
        toml::from_str(&text).map_err(|e| format!("{} is not valid: {e}", path.display()))?;
    Ok(Some(Connection {
        url: address_url(&saved.address),
        token: saved.token,
    }))
}

/// The server of a `moebius.toml` that exists: the `listen` address and the content of the token file.
fn config_settings(config: &Path) -> Result<Option<Connection>, String> {
    if !config.is_file() {
        return Ok(None);
    }
    let config = moebius_core::config::Config::load(config).map_err(|e| e.to_string())?;
    let token = std::fs::read_to_string(&config.token_file).map_err(|e| {
        format!(
            "cannot read the token {}: {e} (has the server started once?)",
            config.token_file.display()
        )
    })?;
    Ok(Some(Connection {
        url: listen_url(config.listen),
        token: token.trim().to_string(),
    }))
}

/// `http://` before an address that has no scheme, and no `/` at the end.
fn address_url(address: &str) -> String {
    let address = address.trim().trim_end_matches('/');
    if address.contains("://") {
        address.to_string()
    } else {
        format!("http://{address}")
    }
}

/// The URL for an address a server listens on; a server that listens on all interfaces is reached on the loopback.
fn listen_url(listen: SocketAddr) -> String {
    let ip = match listen.ip() {
        ip if ip.is_unspecified() && ip.is_ipv6() => std::net::Ipv6Addr::LOCALHOST.into(),
        ip if ip.is_unspecified() => std::net::Ipv4Addr::LOCALHOST.into(),
        ip => ip,
    };
    format!("http://{}", SocketAddr::new(ip, listen.port()))
}

/// Where the server is, from the first source that says: `--server` and `--token` (or `MOEBIUS_URL` and
/// `MOEBIUS_TOKEN`), which must come together; the desktop app's saved settings; the `moebius.toml` that exists. A
/// `--token` alone replaces the token of the settings or the file.
pub fn connect(
    server: Option<&str>,
    token: Option<&str>,
    desktop: Option<&Path>,
    config: &Path,
) -> Result<Connection, String> {
    if let Some(server) = server {
        let token = token.ok_or("--server needs --token (or MOEBIUS_TOKEN)")?;
        return Ok(Connection {
            url: address_url(server),
            token: token.trim().to_string(),
        });
    }
    let found = match desktop_settings(desktop)? {
        Some(found) => found,
        None => config_settings(config)?.ok_or_else(|| {
            format!(
                "no server to connect to: give --server and --token, set MOEBIUS_URL and MOEBIUS_TOKEN, \
                 save the address in the desktop app, or run it where {} is",
                config.display()
            )
        })?,
    };
    Ok(Connection {
        token: token.map_or(found.token, |token| token.trim().to_string()),
        ..found
    })
}

/// One request to the desktop API.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub body: Option<Value>,
}

impl Request {
    pub fn get(path: impl Into<String>) -> Self {
        Self {
            method: Method::GET,
            path: path.into(),
            body: None,
        }
    }

    pub fn post(path: impl Into<String>, body: Value) -> Self {
        Self {
            method: Method::POST,
            path: path.into(),
            body: Some(body),
        }
    }

    pub fn put(path: impl Into<String>, body: Value) -> Self {
        Self {
            method: Method::PUT,
            path: path.into(),
            body: Some(body),
        }
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self {
            method: Method::DELETE,
            path: path.into(),
            body: None,
        }
    }
}

/// The desktop API over HTTP.
pub struct Client {
    http: reqwest::blocking::Client,
    connection: Connection,
}

impl Client {
    pub fn new(connection: Connection) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            // A merge or a chat's start does git work before it answers.
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { http, connection })
    }

    /// Sends the request and returns the answer's body; an error status is an error with the server's message.
    pub fn send(&self, request: &Request) -> Result<String, String> {
        let url = format!("{}{}", self.connection.url, request.path);
        let mut builder = self
            .http
            .request(request.method.clone(), &url)
            .bearer_auth(&self.connection.token);
        if let Some(body) = &request.body {
            builder = builder.json(body);
        }
        let response = builder
            .send()
            .map_err(|e| format!("cannot reach {}: {e}", self.connection.url))?;
        let status = response.status();
        let text = response.text().map_err(|e| e.to_string())?;
        if status.is_success() {
            Ok(text)
        } else {
            Err(format!(
                "the server answered {}: {}",
                status.as_u16(),
                text.trim()
            ))
        }
    }

    /// The JSON a GET answers.
    pub fn get(&self, path: &str) -> Result<Value, String> {
        let text = self.send(&Request::get(path))?;
        serde_json::from_str(&text).map_err(|e| format!("the answer to {path} is not JSON: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_comes_from_the_flags_then_the_desktop_app_then_the_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let desktop = dir.path().join("desktop.toml");
        let config = dir.path().join("moebius.toml");
        let nothing = dir.path().join("none.toml");
        let go = |server, token, desktop: &Path, config: &Path| {
            connect(server, token, Some(desktop), config)
        };

        // Nothing at all.
        let error = go(None, None, &nothing, &nothing).unwrap_err();
        assert!(error.contains("no server to connect to"), "{error}");

        // The configuration in the current directory: its listen address and its token file.
        std::fs::write(&config, "listen = \"0.0.0.0:9001\"\n").unwrap();
        let error = go(None, None, &nothing, &config).unwrap_err();
        assert!(error.contains("cannot read the token"), "{error}");
        std::fs::create_dir_all(dir.path().join(".moebius")).unwrap();
        std::fs::write(dir.path().join(".moebius/token"), "from-file\n").unwrap();
        let found = go(None, None, &nothing, &config).unwrap();
        assert_eq!(
            (found.url.as_str(), found.token.as_str()),
            ("http://127.0.0.1:9001", "from-file")
        );

        // The desktop app's settings come before the file; an address without a scheme gets one.
        std::fs::write(
            &desktop,
            "address = \"10.0.0.2:7171/\"\ntoken = \"from-app\"\n",
        )
        .unwrap();
        let found = go(None, None, &desktop, &config).unwrap();
        assert_eq!(
            (found.url.as_str(), found.token.as_str()),
            ("http://10.0.0.2:7171", "from-app")
        );

        // A token alone replaces the token only.
        let found = go(None, Some("mine"), &desktop, &config).unwrap();
        assert_eq!(
            (found.url.as_str(), found.token.as_str()),
            ("http://10.0.0.2:7171", "mine")
        );

        // A server given must come with its token, so that one server's token is never sent to another.
        let found = go(
            Some("https://moebius.example/"),
            Some("t"),
            &desktop,
            &config,
        )
        .unwrap();
        assert_eq!(
            (found.url.as_str(), found.token.as_str()),
            ("https://moebius.example", "t")
        );
        let error = go(Some("http://x:1"), None, &desktop, &config).unwrap_err();
        assert!(error.contains("--server needs --token"), "{error}");

        // Settings that are not valid are an error, not skipped.
        std::fs::write(&desktop, "address = 1\n").unwrap();
        assert!(
            go(None, None, &desktop, &config)
                .unwrap_err()
                .contains("not valid")
        );
    }

    #[test]
    fn the_flags_read_the_environment() {
        use clap::CommandFactory;
        let command = crate::Cli::command();
        let arg = |name: &str| {
            command
                .get_arguments()
                .find(|a| a.get_id() == name)
                .unwrap()
                .get_env()
                .map(|e| e.to_string_lossy().into_owned())
        };
        assert_eq!(arg("server").as_deref(), Some("MOEBIUS_URL"));
        assert_eq!(arg("token").as_deref(), Some("MOEBIUS_TOKEN"));
    }
}
