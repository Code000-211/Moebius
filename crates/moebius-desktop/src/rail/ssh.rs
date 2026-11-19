//! A server reached over SSH, as `moebius connect <host>` reaches one: one `ssh` that forwards a free port of this
//! machine to the server's `127.0.0.1:7171` and, on the same login, prints the token the setup left on the host. The
//! system's `ssh` does the login (keys, the agent, `~/.ssh/config`, jump hosts); the app has no terminal to answer a
//! question on, so `ssh` runs in batch mode and fails where it would ask one.
//!
//! When no server is set up on the host, [`Tunnel::open_setting_up`] runs `moebius connect <host> --yes` (the program
//! that sets a host up, which the person has said yes to) until the token is there, then holds the tunnel itself.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::server::Server;

/// The port a server listens on at `127.0.0.1` on its host.
const REMOTE_PORT: u16 = 7171;
/// How long the login, the tunnel and a setup may take.
const LIMIT: Duration = Duration::from_secs(120);
/// What `ssh` runs on the host: print the token in a line `MOEBIUS-TOKEN <token>` (a banner before it is skipped), then
/// keep the login open until `ssh` is closed here. It ends with 1 when there is no token file, as `moebius connect`'s.
const REMOTE_COMMAND: &str = "sh -c 'for f in \"$HOME/.moebius/server/.moebius/token\" \
    \"$HOME/.config/moebius/.moebius/token\"; do if [ -s \"$f\" ]; then echo \"MOEBIUS-TOKEN $(cat \"$f\")\"; exec cat; fi; done; exit 1'";
/// How many lines the host prints before the login is given up.
const TOKEN_LINES: usize = 200;

/// Why a tunnel did not open.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SshError {
    /// The login worked and there is no token to read: no server is set up on the host.
    #[error("no server is set up on {0}")]
    NoServer(String),
    #[error("{0}")]
    Failed(String),
}

/// The arguments of the `ssh` that forwards `127.0.0.1:<local>` to the server of `host` and prints the token.
fn tunnel_args(local: u16, host: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "-T",
        "-L",
        &format!("127.0.0.1:{local}:127.0.0.1:{REMOTE_PORT}"),
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=30",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
        "-o",
        "ExitOnForwardFailure=yes",
        // A host is never read as an option, whatever its name.
        "--",
    ]
    .map(String::from)
    .into();
    args.extend([host.to_string(), REMOTE_COMMAND.to_string()]);
    args
}

/// An open tunnel to the server of a host; `ssh` ends with it.
pub struct Tunnel {
    child: Child,
    /// The server behind the tunnel: its forwarded address and the token read on the host.
    pub server: Server,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Tunnel {
    /// Logs in to `host` with `program` (an `ssh`), forwards a free port and reads the token. Blocks.
    pub fn open(program: &str, host: &str) -> Result<Tunnel, SshError> {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .map_err(|e| SshError::Failed(format!("cannot find a free port: {e}")))?
            .port();
        let mut child = Command::new(program)
            .args(tunnel_args(port, host))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| SshError::Failed(format!("cannot start {program}: {e}")))?;
        let stdout = child.stdout.take().expect("piped");
        let token = BufReader::new(stdout)
            .lines()
            .take(TOKEN_LINES)
            .map_while(Result::ok)
            .find_map(|line| {
                line.strip_prefix("MOEBIUS-TOKEN ")
                    .map(|token| token.trim().to_string())
                    .filter(|token| !token.is_empty())
            });
        let Some(token) = token else {
            let _ = child.kill();
            let status = child.wait();
            // `ssh` ends with 255 when it could not log in; any other end is the remote command's: no token.
            return Err(match status {
                Ok(status) if status.code() != Some(255) => SshError::NoServer(host.to_string()),
                _ => SshError::Failed(format!(
                    "cannot log in to {host} with {program} without a question (keys and the agent only)"
                )),
            });
        };
        let tunnel = Tunnel {
            child,
            server: Server {
                url: format!("http://127.0.0.1:{port}"),
                token,
            },
        };
        let started = Instant::now();
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            if started.elapsed() > LIMIT {
                return Err(SshError::Failed(format!(
                    "the tunnel to {host} did not open"
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(tunnel)
    }

    /// [`Tunnel::open`] for a host that has no server yet: `moebius connect <host> --yes` (the `moebius` program) sets
    /// it up there, and the tunnel opens once the token is. The program's own tunnel ends with it. Blocks.
    pub fn open_setting_up(program: &str, moebius: &str, host: &str) -> Result<Tunnel, SshError> {
        let mut setup = Command::new(moebius)
            .args(["connect", "--no-window", "--yes", "--", host])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                SshError::Failed(format!(
                    "cannot start {moebius} to set the server up (install it, or run `moebius connect {host}` in a \
                     terminal): {e}"
                ))
            })?;
        let started = Instant::now();
        let result = loop {
            match Tunnel::open(program, host) {
                Err(SshError::NoServer(_)) => {}
                other => break other,
            }
            if let Ok(Some(status)) = setup.try_wait()
                && !status.success()
            {
                break Err(SshError::Failed(format!(
                    "setting the server up on {host} failed: run `moebius connect {host}` in a terminal to see why"
                )));
            }
            if started.elapsed() > LIMIT {
                break Err(SshError::Failed(format!(
                    "the server on {host} did not come up in time"
                )));
            }
            std::thread::sleep(Duration::from_secs(1));
        };
        let _ = setup.kill();
        let _ = setup.wait();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_never_read_as_an_option() {
        let args = tunnel_args(5000, "-oProxyCommand=evil");
        let at = args.iter().position(|a| a == "--").expect("a separator");
        assert_eq!(args[at + 1], "-oProxyCommand=evil");
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert!(args.contains(&"127.0.0.1:5000:127.0.0.1:7171".to_string()));
    }
}
