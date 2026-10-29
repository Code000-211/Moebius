//! The port forward to the server: `ssh -N -L 127.0.0.1:<port>:127.0.0.1:<remote port>` with keep-alives, as a stream
//! that says when the tunnel is up and when it is down, and starts `ssh` again with a growing wait when it drops. The
//! local port stays the same across restarts, so the address the app uses never changes.

use std::hash::{Hash, Hasher};
use std::process::Stdio;
use std::time::Duration;

use iced::futures::SinkExt;
use iced::futures::channel::mpsc;
use tokio::io::AsyncReadExt;

use super::{Ssh, next_prompt};

/// How often `ssh` is to be answered, and how many missed answers end the connection: a dead link is found in about
/// `INTERVAL * COUNT` seconds.
const KEEP_ALIVE_INTERVAL: u32 = 15;
const KEEP_ALIVE_COUNT: u32 = 3;
/// How often the local end is tried while `ssh` logs in, and how long the login may take.
const PROBE_EVERY: Duration = Duration::from_millis(100);
const LOGIN_LIMIT: Duration = Duration::from_secs(120);

/// A tunnel to a host.
#[derive(Debug, Clone)]
pub struct Tunnel {
    pub ssh: Ssh,
    pub host: String,
    pub local_port: u16,
    pub remote_port: u16,
}

/// Two tunnels are the same subscription when they connect the same ports of the same host.
impl Hash for Tunnel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (&self.host, self.local_port, self.remote_port).hash(state);
    }
}

/// What the tunnel tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelEvent {
    /// The local port forwards to the server.
    Up,
    /// The tunnel ended or could not be made; it is tried again.
    Down(String),
    /// `ssh` asks for a password, a passphrase or a yes.
    Prompt(String),
}

/// The wait before the tunnel is made again, after the `failures`-th failure in a row.
pub fn backoff(failures: u32) -> Duration {
    Duration::from_secs(1 << failures.min(5)).min(Duration::from_secs(30))
}

impl Tunnel {
    /// The arguments of `ssh` that make the tunnel.
    pub fn arguments(&self) -> Vec<String> {
        let forward = format!(
            "127.0.0.1:{}:127.0.0.1:{}",
            self.local_port, self.remote_port
        );
        [
            "-N",
            "-L",
            &forward,
            "-o",
            &format!("ServerAliveInterval={KEEP_ALIVE_INTERVAL}"),
            "-o",
            &format!("ServerAliveCountMax={KEEP_ALIVE_COUNT}"),
            "-o",
            "ExitOnForwardFailure=yes",
            // A host is never read as an option, whatever its name.
            "--",
            &self.host,
        ]
        .map(str::to_string)
        .to_vec()
    }
}

/// Keeps the tunnel up for as long as the stream is held: `Up` once the local port is open, `Down` when `ssh` ends,
/// then a new `ssh` after the wait of [`backoff`].
pub fn run(tunnel: &Tunnel) -> impl iced::futures::Stream<Item = TunnelEvent> + use<> {
    let tunnel = tunnel.clone();
    iced::stream::channel(32, async move |mut out: mpsc::Sender<TunnelEvent>| {
        let mut failures = 0;
        loop {
            let mut command = tunnel.ssh.command(&tunnel.arguments());
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(e) => {
                    let message = format!("cannot start {}: {e}", tunnel.ssh.program());
                    let _ = out.send(TunnelEvent::Down(message)).await;
                    tokio::time::sleep(backoff(failures)).await;
                    failures += 1;
                    continue;
                }
            };
            let started = tokio::time::Instant::now();
            let mut up = false;
            let status = loop {
                tokio::select! {
                    status = child.wait() => break status,
                    Some(prompt) = next_prompt(tunnel.ssh.askpass()) => {
                        let _ = out.send(TunnelEvent::Prompt(prompt)).await;
                    }
                    () = tokio::time::sleep(PROBE_EVERY), if !up && started.elapsed() < LOGIN_LIMIT => {
                        if tokio::net::TcpStream::connect(("127.0.0.1", tunnel.local_port)).await.is_ok() {
                            up = true;
                            failures = 0;
                            let _ = out.send(TunnelEvent::Up).await;
                        }
                    }
                }
            };
            let mut said = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut said).await;
            }
            let reason = said
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| match status {
                    Ok(status) => format!("ssh ended ({status})"),
                    Err(e) => e.to_string(),
                });
            let _ = out.send(TunnelEvent::Down(reason)).await;
            tokio::time::sleep(backoff(failures)).await;
            failures += 1;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tunnel_forwards_the_loopback_with_keep_alives() {
        let tunnel = Tunnel {
            ssh: Ssh::new("ssh"),
            host: "-oProxyCommand=x".into(),
            local_port: 50123,
            remote_port: 7171,
        };
        assert_eq!(
            tunnel.arguments().join(" "),
            "-N -L 127.0.0.1:50123:127.0.0.1:7171 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 \
             -o ExitOnForwardFailure=yes -- -oProxyCommand=x"
        );
    }

    #[test]
    fn the_wait_before_the_tunnel_is_made_again_grows_to_a_limit() {
        let waits: Vec<u64> = (0..8).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30, 30]);
    }
}
