//! `moebius login`: saves a person's access token for the command line (ADR 0018).
//!
//! The token comes from `--token` (or `MOEBIUS_TOKEN`), else from the first line of standard input, so that it is not
//! left in the shell's history. The server is `--server`, else the address saved before, else the `listen` of the
//! `moebius.toml` that exists. The token is checked with `GET /v1/session` and saved only when it is a person's: the
//! `address` and `token` of `cli.toml` are written, its other keys kept, and the file is readable by its owner only.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use super::client::{Client, Connection, address_url, config_url};

/// Who the saved token is, as the server said.
#[derive(Debug, PartialEq, Eq)]
pub struct LoggedIn {
    pub url: String,
    pub name: String,
    pub handle: String,
}

/// The server to log in to: `server`, else the `address` of `settings`, else the `listen` of `config`.
fn server_url(server: Option<&str>, settings: &Path, config: &Path) -> Result<String, String> {
    if let Some(server) = server {
        return Ok(address_url(server));
    }
    if let Some(address) = saved(settings)?.get("address").and_then(|a| a.as_str()) {
        return Ok(address_url(address));
    }
    config_url(config)?.ok_or_else(|| {
        format!(
            "no server to log in to: give --server, or run it where {} is",
            config.display()
        )
    })
}

/// What `settings` holds now, or nothing when it is not there.
fn saved(settings: &Path) -> Result<toml::Table, String> {
    match std::fs::read_to_string(settings) {
        Ok(text) => text
            .parse::<toml::Table>()
            .map_err(|e| format!("{} is not valid: {e}", settings.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(e) => Err(format!("cannot read {}: {e}", settings.display())),
    }
}

/// The token from `--token`, else the first line of `input`, asked for on standard error when it is a terminal.
fn read_token(token: Option<&str>, input: &mut dyn BufRead) -> Result<String, String> {
    if let Some(token) = token {
        return Ok(token.trim().to_string());
    }
    if std::io::stdin().is_terminal() {
        eprint!("Access token: ");
        let _ = std::io::stderr().flush();
    }
    let mut line = String::new();
    input
        .read_line(&mut line)
        .map_err(|e| format!("cannot read the token: {e}"))?;
    let token = line.trim().to_string();
    if token.is_empty() {
        return Err("no token was given".into());
    }
    Ok(token)
}

/// Checks `token` against the server and saves it with the server's address in `settings`.
pub fn login(
    server: Option<&str>,
    token: Option<&str>,
    settings: &Path,
    config: &Path,
    input: &mut dyn BufRead,
) -> Result<LoggedIn, String> {
    let url = server_url(server, settings, config)?;
    let token = read_token(token, input)?;
    let client = Client::new(Connection {
        url: url.clone(),
        token: token.clone(),
    })?;
    let session = client.get("/v1/session")?;
    let person = &session["person"];
    let field = |name: &str| person[name].as_str().map(str::to_string);
    let (Some(name), Some(handle)) = (field("name"), field("handle")) else {
        return Err("that token is not a person's".into());
    };
    let mut table = saved(settings)?;
    table.insert("address".into(), url.clone().into());
    table.insert("token".into(), token.into());
    let text = toml::to_string(&table).map_err(|e| e.to_string())?;
    if let Some(dir) = settings.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    }
    moebius_core::server::settings::write_atomically(settings, text.as_bytes(), true)
        .map_err(|e| format!("cannot write {}: {e}", settings.display()))?;
    Ok(LoggedIn { url, name, handle })
}

/// The sentence `login` prints.
pub fn describe(logged_in: &LoggedIn, settings: &Path) -> String {
    format!(
        "Logged in to {} as {} (@{}); the token is saved in {}.",
        logged_in.url,
        logged_in.name,
        logged_in.handle,
        settings.display()
    )
}
