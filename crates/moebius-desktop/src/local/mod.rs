//! A server of this machine that the app sets up and starts (ADR 0021, D3): on Linux, and on Windows inside WSL 2, the
//! first start offers to run one. The app brings `moebius` (the release of its own version, checked against that
//! release's `SHA256SUMS`), runs `init` and `serve` in the background, and connects to it with the token the server
//! keeps in its data directory.
//!
//! It is the server `moebius connect` makes on a host, in the same places under the home directory of the machine's
//! system: the program `~/.moebius/bin/moebius` and the server's directory `~/.moebius/server`, so the two never make
//! two servers. [`Local::state`] only looks and changes nothing, so a person who declines leaves nothing behind;
//! [`Local::start`] is what the person agreed to. A server that runs already (or is found again after the app was
//! closed) is used as it is; one that was set up and has stopped is started again without a new `init`.
//!
//! Every step is a POSIX shell script that the [`host`] runs: `sh` on Linux, `wsl.exe` on Windows. The scripts print
//! the facts they report in lines that start with `MOEBIUS-<NAME> `, and only those lines are read.

mod host;
mod release;

use std::time::Duration;

use crate::server::{self, Server};
pub use host::Host;

/// The program, under the home directory of the machine's system.
const BINARY: &str = ".moebius/bin/moebius";
/// The server's directory, under the home directory: the configuration, the data and `serve.log`.
const SERVER_DIR: &str = ".moebius/server";
/// Where the server listens when its configuration does not say.
const DEFAULT_LISTEN: &str = "127.0.0.1:7171";
/// How long a started server has to write its token and answer.
const START_TIME: Duration = Duration::from_secs(90);
/// How often the server is looked for while waiting.
const POLL: Duration = Duration::from_millis(250);

/// What this machine has of a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// A server cannot run here, or the app cannot tell: the reason is shown to the person.
    Unavailable(String),
    /// Nothing is set up: the first start's offer.
    NotSetUp,
    /// A server is set up and does not answer.
    Stopped,
    /// A server answers with its token.
    Running,
}

/// Why a local server was not started.
#[derive(Debug, thiserror::Error)]
pub enum LocalError {
    #[error("{0}")]
    Unavailable(String),
    #[error("cannot download {url}: {reason}")]
    Download { url: String, reason: String },
    #[error("the download of {url} does not match the SHA256SUMS of its release")]
    Checksum { url: String },
    #[error("{0}")]
    Step(String),
    #[error("the server did not answer within {} seconds; its log is ~/{SERVER_DIR}/serve.log", START_TIME.as_secs())]
    Timeout,
}

/// What the machine has, as the status script reports it.
#[derive(Debug, Default, PartialEq, Eq)]
struct Found {
    system: String,
    binary: Option<String>,
    version: Option<String>,
    configured: bool,
    token: Option<String>,
    listen: Option<String>,
}

impl Found {
    fn parse(stdout: &str) -> Found {
        let mut found = Found::default();
        for line in stdout.lines() {
            let Some(rest) = line.strip_prefix("MOEBIUS-") else {
                continue;
            };
            let (name, value) = rest.split_once(' ').unwrap_or((rest, ""));
            let value = value.trim();
            match name {
                "SYSTEM" => found.system = value.to_string(),
                "BINARY" => found.binary = Some(value.to_string()).filter(|v| !v.is_empty()),
                "VERSION" => found.version = value.split_whitespace().last().map(str::to_string),
                "CONFIG" => found.configured = true,
                "TOKEN" => found.token = Some(value.to_string()).filter(|v| !v.is_empty()),
                "LISTEN" => found.listen = Some(value.to_string()).filter(|v| !v.is_empty()),
                _ => {}
            }
        }
        found
    }

    /// The server's address: its configuration's `listen`, else the default.
    fn url(&self) -> String {
        // A wildcard address is reached on the loopback.
        let listen = self.listen.as_deref().unwrap_or(DEFAULT_LISTEN);
        let listen = match listen.rsplit_once(':') {
            Some(("0.0.0.0", port)) => format!("127.0.0.1:{port}"),
            Some(("[::]", port)) => format!("[::1]:{port}"),
            _ => listen.to_string(),
        };
        format!("http://{listen}")
    }

    /// Whether `moebius` is there and of this app's release line.
    fn has_matching_binary(&self) -> bool {
        self.binary.is_some()
            && self
                .version
                .as_deref()
                .and_then(server::release_line)
                .is_some_and(|line| Some(line) == server::release_line(server::VERSION))
    }
}

/// The status script: reads, changes nothing. The program is the one of a previous install if there is one, else the
/// first `moebius` on the PATH.
fn status_script() -> String {
    format!(
        r#"echo "MOEBIUS-SYSTEM $(uname -sm)"
bin="$HOME/{BINARY}"
if [ ! -x "$bin" ]; then bin=$(command -v moebius 2>/dev/null || true); fi
if [ -n "$bin" ]; then echo "MOEBIUS-BINARY $bin"; echo "MOEBIUS-VERSION $("$bin" --version 2>/dev/null | head -n 1)"; fi
d="$HOME/{SERVER_DIR}"
if [ -f "$d/moebius.toml" ]; then
  echo "MOEBIUS-CONFIG yes"
  sed -n 's/^listen *= *"\(.*\)" *$/MOEBIUS-LISTEN \1/p' "$d/moebius.toml" | head -n 1
fi
if [ -s "$d/.moebius/token" ]; then echo "MOEBIUS-TOKEN $(cat "$d/.moebius/token")"; fi
exit 0"#
    )
}

/// The local server of one machine.
#[derive(Debug, Clone)]
pub struct Local {
    host: Host,
    /// The directory the release's files are under: `<repository>/releases/download/v<version>`.
    release_base: String,
}

impl Local {
    /// The local server of this machine's system: Linux, or WSL 2 on Windows; none elsewhere.
    pub fn of_this_machine() -> Result<Local, String> {
        Ok(Local::new(Host::this_machine()?, release::base()))
    }

    pub fn new(host: Host, release_base: String) -> Local {
        Local { host, release_base }
    }

    async fn find(&self) -> Result<Found, LocalError> {
        let out = self.host.run(&status_script(), None).await?;
        Ok(Found::parse(&out))
    }

    /// Looks at the machine and changes nothing.
    pub async fn state(&self) -> State {
        let found = match self.find().await {
            Ok(found) => found,
            Err(e) => return State::Unavailable(e.to_string()),
        };
        if let Err(reason) = release::target(&found.system) {
            return State::Unavailable(reason);
        }
        if !found.configured {
            return State::NotSetUp;
        }
        match self.answering(&found).await {
            Some(_) => State::Running,
            None => State::Stopped,
        }
    }

    /// The server of `found` when it answers with its token and is of this app's release line.
    async fn answering(&self, found: &Found) -> Option<Server> {
        let server = Server::new(&found.url(), found.token.as_deref()?);
        server::check(&server).await.ok().map(|_| server)
    }

    /// Sets the server up where it is not (the program, `init`), starts it where it does not run, and returns it with
    /// its token once it answers.
    pub async fn start(&self) -> Result<Server, LocalError> {
        let mut found = self.find().await?;
        let target = release::target(&found.system).map_err(LocalError::Unavailable)?;
        if let Some(server) = self.answering(&found).await {
            return Ok(server);
        }
        let binary = if found.has_matching_binary() {
            found.binary.clone().unwrap_or_default()
        } else {
            release::install(&self.host, &self.release_base, target).await?
        };
        if !found.configured {
            let init = format!(
                "umask 077; d=\"$HOME/{SERVER_DIR}\"; mkdir -p \"$d\" && chmod 700 \"$d\" && cd \"$d\" && {} init",
                host::quote(&binary)
            );
            self.host.run(&init, None).await?;
        }
        let serve = format!(
            "cd \"$HOME/{SERVER_DIR}\" && exec {} serve >> serve.log 2>&1",
            host::quote(&binary)
        );
        self.host.spawn(&serve)?;
        let deadline = tokio::time::Instant::now() + START_TIME;
        loop {
            found = self.find().await?;
            if let Some(server) = self.answering(&found).await {
                return Ok(server);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(LocalError::Timeout);
            }
            tokio::time::sleep(POLL).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_script_reports_is_read_by_its_marked_lines_only() {
        let found = Found::parse(
            "welcome to the shell\nMOEBIUS-SYSTEM Linux x86_64\nMOEBIUS-BINARY /home/a/.moebius/bin/moebius\n\
             MOEBIUS-VERSION moebius 0.13.4\nMOEBIUS-CONFIG yes\nMOEBIUS-LISTEN 127.0.0.1:9000\nMOEBIUS-TOKEN abc\n",
        );
        assert_eq!(found.system, "Linux x86_64");
        assert_eq!(
            found.binary.as_deref(),
            Some("/home/a/.moebius/bin/moebius")
        );
        assert_eq!(found.version.as_deref(), Some("0.13.4"));
        assert!(found.configured);
        assert_eq!(found.url(), "http://127.0.0.1:9000");
        assert_eq!(found.token.as_deref(), Some("abc"));
        assert_eq!(Found::parse("").url(), "http://127.0.0.1:7171");
        let wildcard = Found::parse("MOEBIUS-LISTEN 0.0.0.0:7000");
        assert_eq!(wildcard.url(), "http://127.0.0.1:7000");
    }

    #[test]
    fn a_program_of_another_release_line_is_not_used() {
        let ours = server::VERSION;
        let mut found = Found {
            binary: Some("/bin/moebius".into()),
            version: Some(ours.to_string()),
            ..Found::default()
        };
        assert!(found.has_matching_binary());
        found.version = Some("0.1.0".to_string());
        assert!(!found.has_matching_binary());
        found.version = None;
        assert!(!found.has_matching_binary());
    }
}
