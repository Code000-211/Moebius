//! A server reached over SSH from the command line: one `ssh` that forwards a port of this machine to the server's
//! `127.0.0.1:7171` and, on the same login, reads the token the app's setup left (`~/.moebius/server/.moebius/token`).
//! `--ssh <host>` holds the tunnel for one command; `moebius connect <host>` holds it and writes where it is to
//! `connected.toml` next to the desktop app's settings, which the commands that follow read.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::client::Connection;

/// The port the server listens on at `127.0.0.1` on the remote.
const REMOTE_PORT: u16 = 7171;
/// How long the login and the tunnel may take.
const OPEN_LIMIT: Duration = Duration::from_secs(120);
/// What `ssh` is asked to run: write the token, then keep the login open until `ssh` is closed on this side.
const REMOTE_COMMAND: &str =
    "sh -c 'cat \"$HOME/.moebius/server/.moebius/token\" && echo && exec cat'";

/// The `ssh` client: the program the desktop app's settings name (`ssh_program`), else `ssh`.
pub fn program(desktop: Option<&Path>) -> String {
    #[derive(Deserialize, Default)]
    struct Settings {
        ssh_program: Option<String>,
    }
    desktop
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| toml::from_str::<Settings>(&text).ok())
        .and_then(|settings| settings.ssh_program)
        .unwrap_or_else(|| "ssh".to_string())
}

/// An open tunnel to the server of a host; `ssh` ends with it.
pub struct Tunnel {
    child: Child,
    pub host: String,
    pub connection: Connection,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Tunnel {
    /// Logs in to `host` (`ssh` asks for a password on this terminal if it needs one), forwards a free local port and
    /// reads the token.
    pub fn open(program: &str, host: &str) -> Result<Tunnel, String> {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .map_err(|e| format!("cannot find a free port: {e}"))?
            .port();
        let mut child = Command::new(program)
            .args([
                "-T",
                "-L",
                &format!("127.0.0.1:{port}:127.0.0.1:{REMOTE_PORT}"),
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
                "-o",
                "ExitOnForwardFailure=yes",
                // A host is never read as an option, whatever its name.
                "--",
                host,
                REMOTE_COMMAND,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot start {program}: {e}"))?;
        let stdout = child.stdout.take().expect("piped");
        let mut token = String::new();
        let read = BufReader::new(stdout).read_line(&mut token);
        let tunnel_host = host.to_string();
        match read {
            Ok(n) if n > 0 && !token.trim().is_empty() => {}
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "cannot read the token on {tunnel_host}: has the desktop app set the server up there?"
                ));
            }
        }
        let tunnel = Tunnel {
            child,
            host: tunnel_host,
            connection: Connection {
                url: format!("http://127.0.0.1:{port}"),
                token: token.trim().to_string(),
            },
        };
        let started = Instant::now();
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            if started.elapsed() > OPEN_LIMIT {
                return Err(format!("the tunnel to {host} did not open"));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(tunnel)
    }

    /// Waits until `ssh` ends: the connection dropped or the process was told to stop.
    pub fn wait(&mut self) -> Result<(), String> {
        // Holding the pipe open keeps the remote side's `cat` running.
        let _held = self.child.stdin.take();
        let _ = std::io::stdout().flush();
        self.child
            .wait()
            .map(|_| ())
            .map_err(|e| format!("cannot wait for ssh: {e}"))
    }
}

/// Where `connect` leaves the tunnel for the commands that follow: next to the desktop app's settings.
pub fn connected_path(desktop: Option<&Path>) -> Option<PathBuf> {
    desktop.map(|path| path.with_file_name("connected.toml"))
}

#[derive(Debug, Serialize, Deserialize)]
struct Connected {
    host: String,
    url: String,
    token: String,
}

/// Writes where the tunnel is, readable by the person only.
pub fn write_connected(path: &Path, tunnel: &Tunnel) -> Result<(), String> {
    let text = toml::to_string(&Connected {
        host: tunnel.host.clone(),
        url: tunnel.connection.url.clone(),
        token: tunnel.connection.token.clone(),
    })
    .map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// The tunnel `moebius connect` holds, if one is open: its file names a port that answers. A file left by a
/// `connect` that was stopped is removed.
pub fn connected(path: Option<&Path>) -> Option<Connection> {
    let path = path?;
    let saved: Connected = toml::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let port = saved.url.rsplit(':').next()?.parse::<u16>().ok()?;
    if TcpStream::connect(("127.0.0.1", port)).is_err() {
        let _ = std::fs::remove_file(path);
        return None;
    }
    Some(Connection {
        url: saved.url,
        token: saved.token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_comes_from_the_desktop_settings_else_is_ssh() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("desktop.toml");
        assert_eq!(program(Some(&settings)), "ssh");
        std::fs::write(&settings, "ssh_program = \"/opt/ssh\"\naddress = \"x\"\n").unwrap();
        assert_eq!(program(Some(&settings)), "/opt/ssh");
        assert_eq!(program(None), "ssh");
    }

    #[test]
    fn a_connection_whose_port_answers_is_used_and_a_stale_one_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let desktop = dir.path().join("desktop.toml");
        let path = connected_path(Some(&desktop)).unwrap();
        assert_eq!(path.file_name().unwrap(), "connected.toml");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::fs::write(
            &path,
            format!("host = \"build\"\nurl = \"http://127.0.0.1:{port}\"\ntoken = \"tok\"\n"),
        )
        .unwrap();
        let found = connected(Some(&path)).unwrap();
        assert_eq!(
            (found.url.as_str(), found.token.as_str()),
            (format!("http://127.0.0.1:{port}").as_str(), "tok")
        );
        drop(listener);
        assert!(connected(Some(&path)).is_none());
        assert!(!path.exists());
    }
}
