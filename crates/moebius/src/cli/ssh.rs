//! A server reached over SSH from the command line: one `ssh` that forwards a port of this machine to the server's
//! `127.0.0.1:7171` and, on the same login, reads the token the app's setup left (`~/.moebius/server/.moebius/token`).
//! `--ssh <host>` holds the tunnel for one command; `moebius connect <host>` holds it and writes where it is to
//! `connected.toml` next to the desktop app's settings, which the commands that follow read. [`wait_until_healthy`]
//! waits until the forwarded port answers `GET /v1/health`, so that a window is never opened on a tunnel that is not
//! there yet.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::client::Connection;

/// The port the server listens on at `127.0.0.1` on the remote unless `--port` says another.
pub const REMOTE_PORT: u16 = 7171;
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

/// The arguments of the `ssh` that forwards `127.0.0.1:<local>` to `127.0.0.1:<remote>` of `host` and prints the token.
fn tunnel_args(local: u16, remote: u16, host: &str) -> Vec<String> {
    [
        "-T",
        "-L",
        &format!("127.0.0.1:{local}:127.0.0.1:{remote}"),
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
    ]
    .map(String::from)
    .into()
}

/// Waits until the server at `url` (a forwarded port, say) answers `GET /v1/health`: any answer but a server error
/// says it is there. Fails after `limit`.
pub fn wait_until_healthy(url: &str, limit: Duration) -> Result<(), String> {
    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    loop {
        if let Ok(response) = http.get(format!("{url}/v1/health")).send()
            && !response.status().is_server_error()
        {
            return Ok(());
        }
        if started.elapsed() > limit {
            return Err(format!("the server behind {url} did not answer"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
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
    pub fn open(program: &str, host: &str, remote_port: u16) -> Result<Tunnel, String> {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .map_err(|e| format!("cannot find a free port: {e}"))?
            .port();
        let mut child = Command::new(program)
            .args(tunnel_args(port, remote_port, host))
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
    fn the_tunnel_forwards_a_local_port_to_the_remote_one_and_the_host_is_never_an_option() {
        let args = tunnel_args(4000, 9000, "-oProxyCommand=x");
        let at = |flag: &str| args.iter().position(|a| a == flag).unwrap();
        assert_eq!(args[at("-L") + 1], "127.0.0.1:4000:127.0.0.1:9000");
        assert!(args.contains(&"ExitOnForwardFailure=yes".to_string()));
        let end = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[end + 1], "-oProxyCommand=x");
        assert_eq!(args[end + 2], REMOTE_COMMAND);
    }

    #[test]
    fn waiting_for_health_retries_until_the_server_answers_and_gives_up() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let served = std::thread::spawn(move || {
            // Not ready twice (a server error), then ready.
            for status in [
                "503 Service Unavailable",
                "503 Service Unavailable",
                "200 OK",
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 1024];
                let n = stream.read(&mut buffer).unwrap();
                assert!(String::from_utf8_lossy(&buffer[..n]).starts_with("GET /v1/health "));
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                )
                .unwrap();
            }
        });
        wait_until_healthy(&url, Duration::from_secs(60)).unwrap();
        served.join().unwrap();
        // Nothing listens there any more.
        let error = wait_until_healthy(&url, Duration::ZERO).unwrap_err();
        assert!(error.contains("did not answer"), "{error}");
    }

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
