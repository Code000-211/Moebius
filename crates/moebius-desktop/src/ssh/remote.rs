//! Setting the server up on the other side of an SSH connection, step by step, as a stream of [`Event`]s: find out the
//! system, find `moebius` (or offer to install it), make the configuration when there is none, start the server unless
//! it runs, and read its token. The server listens on `127.0.0.1` only; the tunnel (`tunnel`) is the way to it.

use iced::futures::SinkExt;
use iced::futures::channel::mpsc;

use super::{Output, Ssh, SshError, quote};

/// Where a release of `moebius` is: `{version}` is this app's version and `{target}` the Rust target triple of the
/// remote system. The one place that names it.
pub const RELEASE_URL: &str =
    "https://github.com/Code000-211/Moebius/releases/download/v{version}/moebius-{target}";
/// The port the server listens on at `127.0.0.1` when its configuration is the one `moebius init` writes.
pub const REMOTE_PORT: u16 = 7171;
/// Where the app puts `moebius` on the remote, and the directory of the configuration and data it sets up, under the
/// remote home directory.
const BINARY: &str = ".moebius/bin/moebius";
const SERVER_DIR: &str = ".moebius/server";
/// How often the token file is looked for, and how long between two looks.
const TOKEN_TRIES: u32 = 40;
const TOKEN_WAIT: std::time::Duration = std::time::Duration::from_millis(500);

/// The steps of a connection, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Connect,
    Find,
    Install,
    Configure,
    Start,
    Token,
    Tunnel,
}

impl Step {
    pub const ALL: [Step; 7] = [
        Step::Connect,
        Step::Find,
        Step::Install,
        Step::Configure,
        Step::Start,
        Step::Token,
        Step::Tunnel,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Step::Connect => "Connect and detect the system",
            Step::Find => "Find Moebius",
            Step::Install => "Install Moebius",
            Step::Configure => "Set up the configuration",
            Step::Start => "Start the server",
            Step::Token => "Read the token",
            Step::Tunnel => "Open the tunnel",
        }
    }
}

/// What the setup tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The step that runs now; those before it are done.
    Step(Step),
    /// A line for the log the person can open.
    Log(String),
    /// `ssh` asks for a password, a passphrase or a yes.
    Prompt(String),
    /// `moebius` is not on the remote: the system and the release that matches it.
    Missing { system: String, url: String },
    /// The server runs and this is its token; the tunnel is to use the local `port`.
    Ready { token: String, port: u16 },
    /// The setup stopped, with the reason.
    Failed(String),
}

/// How the binary gets to the remote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Install {
    /// The remote downloads it.
    Download,
    /// This machine downloads it and sends it, for a remote without internet.
    Upload,
}

/// Where the setup starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// Look for `moebius`; if it is not there, ask.
    Check,
    /// Install it as chosen, then go on.
    Install(Install),
}

/// What a setup needs to know.
#[derive(Debug, Clone)]
pub struct Plan {
    pub ssh: Ssh,
    pub host: String,
    /// The URL pattern of a release; [`RELEASE_URL`] unless a test says another.
    pub release: String,
}

impl Plan {
    pub fn new(ssh: Ssh, host: &str) -> Plan {
        Plan {
            ssh,
            host: host.to_string(),
            release: RELEASE_URL.to_string(),
        }
    }
}

/// The system of the remote, as `uname -sm` says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    pub system: String,
    pub arch: String,
}

impl Platform {
    pub fn parse(uname: &str) -> Result<Platform, String> {
        let mut words = uname.split_whitespace();
        match (words.next(), words.next()) {
            (Some(system), Some(arch)) => Ok(Platform {
                system: system.to_string(),
                arch: arch.to_string(),
            }),
            _ => Err(format!("cannot tell the system from {:?}", uname.trim())),
        }
    }

    /// The Rust target triple of a release for this system.
    pub fn target(&self) -> Result<&'static str, String> {
        match (self.system.as_str(), self.arch.as_str()) {
            ("Linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
            ("Linux", "aarch64" | "arm64") => Ok("aarch64-unknown-linux-gnu"),
            ("Darwin", "x86_64") => Ok("x86_64-apple-darwin"),
            ("Darwin", "arm64" | "aarch64") => Ok("aarch64-apple-darwin"),
            (system, arch) => Err(format!("no release of Moebius for {system} {arch}")),
        }
    }
}

/// The URL of the release for `platform` out of the pattern.
pub fn release_url(pattern: &str, platform: &Platform) -> Result<String, String> {
    Ok(pattern
        .replace("{version}", env!("CARGO_PKG_VERSION"))
        .replace("{target}", platform.target()?))
}

/// The setup as a stream of events; it ends with `Ready`, `Failed`, or `Missing` (which waits for the person's choice
/// and is followed by a new setup started with `Start::Install`).
pub fn connect(plan: Plan, start: Start) -> impl iced::futures::Stream<Item = Event> {
    iced::stream::channel(64, async move |out: mpsc::Sender<Event>| {
        let mut run = Run { plan, out };
        if let Err(message) = run.go(start).await {
            run.send(Event::Failed(message)).await;
        }
    })
}

struct Run {
    plan: Plan,
    out: mpsc::Sender<Event>,
}

impl Run {
    async fn send(&mut self, event: Event) {
        let _ = self.out.send(event).await;
    }

    async fn log(&mut self, line: impl Into<String>) {
        self.send(Event::Log(line.into())).await;
    }

    /// Runs `script` on the remote; the log gets the line that says what is run (`shown`) and what comes back, unless
    /// `secret` says the output is not to be logged.
    async fn run(
        &mut self,
        shown: &str,
        script: &str,
        stdin: Option<Vec<u8>>,
        secret: bool,
    ) -> Result<Output, String> {
        self.log(format!("$ {shown}")).await;
        let mut prompts = self.out.clone();
        let output = self
            .plan
            .ssh
            .run(&self.plan.host, script, stdin, |prompt| {
                let _ = prompts.try_send(Event::Prompt(prompt));
            })
            .await
            .map_err(|e: SshError| e.to_string())?;
        if !secret {
            for line in output.stdout.lines() {
                self.log(line).await;
            }
        }
        for line in output.stderr.lines() {
            self.log(line).await;
        }
        Ok(output)
    }

    async fn step(&mut self, step: Step) {
        self.send(Event::Step(step)).await;
    }

    async fn go(&mut self, start: Start) -> Result<(), String> {
        self.step(Step::Connect).await;
        let uname = self.run("uname -sm", "uname -sm", None, false).await?;
        if !uname.ok {
            return Err(failure("cannot connect", &uname));
        }
        let platform = Platform::parse(&uname.stdout)?;
        self.step(Step::Find).await;
        let mut binary = self.find().await?;
        if let Start::Install(how) = start {
            self.step(Step::Install).await;
            self.install(&platform, how).await?;
            self.step(Step::Find).await;
            binary = Some(
                self.find()
                    .await?
                    .ok_or("Moebius is not on the remote after the install")?,
            );
        }
        let Some(binary) = binary else {
            let url = release_url(&self.plan.release, &platform)?;
            let system = format!("{} {}", platform.system, platform.arch);
            self.send(Event::Missing { system, url }).await;
            return Ok(());
        };
        self.configure(&binary).await?;
        self.start_server(&binary).await?;
        let token = self.token().await?;
        let port = free_port().map_err(|e| format!("cannot find a free port: {e}"))?;
        self.send(Event::Ready { token, port }).await;
        Ok(())
    }

    /// The `moebius` on the remote's `PATH` or in `~/.moebius/bin`.
    async fn find(&mut self) -> Result<Option<String>, String> {
        let script = format!(
            "p=$(command -v moebius 2>/dev/null); \
             if [ -z \"$p\" ] && [ -x \"$HOME/{BINARY}\" ]; then p=\"$HOME/{BINARY}\"; fi; \
             if [ -n \"$p\" ]; then echo \"$p\"; fi"
        );
        let found = self.run("command -v moebius", &script, None, false).await?;
        let path = found.stdout.lines().next().unwrap_or("").trim().to_string();
        Ok((!path.is_empty()).then_some(path))
    }

    async fn install(&mut self, platform: &Platform, how: Install) -> Result<(), String> {
        let url = release_url(&self.plan.release, platform)?;
        let target = format!("$HOME/{BINARY}");
        let place = format!("chmod +x \"{target}.part\" && mv \"{target}.part\" \"{target}\"");
        match how {
            Install::Download => {
                let script = format!(
                    "set -e; mkdir -p \"$HOME/.moebius/bin\"; \
                     if command -v curl >/dev/null 2>&1; then curl -fsSL {url} -o \"{target}.part\"; \
                     elif command -v wget >/dev/null 2>&1; then wget -qO \"{target}.part\" {url}; \
                     else echo 'neither curl nor wget is installed' >&2; exit 1; fi; {place}",
                    url = quote(&url),
                );
                let shown = format!("download {url}");
                let done = self.run(&shown, &script, None, false).await?;
                if !done.ok {
                    return Err(failure("the remote cannot download Moebius", &done));
                }
            }
            Install::Upload => {
                self.log(format!("Downloading {url} here")).await;
                let bytes = download_here(&url).await?;
                self.log(format!("Uploading {} bytes", bytes.len())).await;
                let script = format!(
                    "set -e; mkdir -p \"$HOME/.moebius/bin\"; cat > \"{target}.part\"; {place}"
                );
                let done = self
                    .run("upload moebius", &script, Some(bytes), false)
                    .await?;
                if !done.ok {
                    return Err(failure("cannot upload Moebius", &done));
                }
            }
        }
        Ok(())
    }

    /// `moebius init` in the server's directory, unless it has a configuration.
    async fn configure(&mut self, binary: &str) -> Result<(), String> {
        self.step(Step::Configure).await;
        let script = format!(
            "mkdir -p \"$HOME/{SERVER_DIR}\" && cd \"$HOME/{SERVER_DIR}\" && \
             if [ -f moebius.toml ]; then echo 'The configuration exists.'; else {} init; fi",
            quote(binary)
        );
        let done = self.run("moebius init", &script, None, false).await?;
        if done.ok {
            Ok(())
        } else {
            Err(failure("moebius init failed", &done))
        }
    }

    /// Starts `moebius serve` as a systemd user service where there is one, else detached; nothing if it runs.
    async fn start_server(&mut self, binary: &str) -> Result<(), String> {
        self.step(Step::Start).await;
        let script = format!(
            r#"cd "$HOME/{SERVER_DIR}" || exit 1
if systemctl --user is-active --quiet moebius.service 2>/dev/null; then echo 'The server runs (systemd).'; exit 0; fi
if [ -f serve.pid ] && kill -0 "$(cat serve.pid)" 2>/dev/null; then echo 'The server runs.'; exit 0; fi
if systemctl --user show-environment >/dev/null 2>&1; then
  mkdir -p "$HOME/.config/systemd/user"
  cat > "$HOME/.config/systemd/user/moebius.service" <<UNIT
[Unit]
Description=Moebius server
[Service]
WorkingDirectory=$HOME/{SERVER_DIR}
Environment="PATH=$PATH"
ExecStart={binary} serve
Restart=on-failure
[Install]
WantedBy=default.target
UNIT
  systemctl --user daemon-reload && systemctl --user enable --now moebius.service && echo 'Started as a systemd user service.' && exit 0
fi
nohup {binary} serve </dev/null >serve.log 2>&1 &
echo $! > serve.pid
echo 'Started detached.'"#,
            binary = quote(binary)
        );
        let done = self.run("moebius serve", &script, None, false).await?;
        if done.ok {
            Ok(())
        } else {
            Err(failure("cannot start the server", &done))
        }
    }

    /// The token the server writes when it starts; waits for the file.
    async fn token(&mut self) -> Result<String, String> {
        self.step(Step::Token).await;
        let script = format!("cat \"$HOME/{SERVER_DIR}/.moebius/token\"");
        for attempt in 0..TOKEN_TRIES {
            let read = self.run("cat .moebius/token", &script, None, true).await?;
            let token = read.stdout.trim();
            if read.ok && !token.is_empty() {
                self.log("Read the token.").await;
                return Ok(token.to_string());
            }
            if attempt + 1 < TOKEN_TRIES {
                tokio::time::sleep(TOKEN_WAIT).await;
            }
        }
        let script = format!("tail -n 20 \"$HOME/{SERVER_DIR}/serve.log\" 2>/dev/null");
        let _ = self.run("tail serve.log", &script, None, false).await;
        Err("the server wrote no token; see the log".to_string())
    }
}

/// A free port of the loopback, for the local end of the tunnel.
pub fn free_port() -> std::io::Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

/// The reason a command failed: what it said last.
fn failure(what: &str, output: &Output) -> String {
    let said = output.stderr.trim();
    let said = if said.is_empty() {
        output.stdout.trim()
    } else {
        said
    };
    match said.lines().last() {
        Some(line) => format!("{what}: {line}"),
        None => what.to_string(),
    }
}

/// The bytes at `url`, fetched here with `curl`, which Linux, macOS and Windows 10 and later all have.
async fn download_here(url: &str) -> Result<Vec<u8>, String> {
    let output = tokio::process::Command::new("curl")
        .args(["-fsSL", url])
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("cannot run curl: {e}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "cannot download {url}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_picks_the_release() {
        let linux = Platform::parse("Linux x86_64\n").unwrap();
        assert_eq!(
            release_url("https://x/v{version}/moebius-{target}", &linux).unwrap(),
            format!(
                "https://x/v{}/moebius-x86_64-unknown-linux-gnu",
                env!("CARGO_PKG_VERSION")
            )
        );
        let mac = Platform::parse("Darwin arm64").unwrap();
        assert_eq!(mac.target().unwrap(), "aarch64-apple-darwin");
        let other = Platform::parse("FreeBSD amd64").unwrap();
        assert!(other.target().unwrap_err().contains("FreeBSD amd64"));
        assert!(Platform::parse("").is_err());
    }

    #[test]
    fn a_failure_names_the_last_line_it_said() {
        let output = Output {
            ok: false,
            stdout: String::new(),
            stderr: "warning\nPermission denied\n".into(),
        };
        assert_eq!(
            failure("cannot connect", &output),
            "cannot connect: Permission denied"
        );
    }
}
