//! One run on a runner. The run's branch is pushed over SSH to a bare clone kept in the runner's working directory
//! (`<working_dir>/repos/`) and cloned into the run's own directory (`<working_dir>/runs/<id>/work`); what the harness
//! is given from the server (its agent directory, bridge, skills and extensions) is copied to `<working_dir>/runs/<id>/files`.
//! The harness is started by one more `ssh`, whose standard input and output are the harness's own, so steering and
//! stopping work as they do locally. The router, the tool-call check and the MCP gateway are reached through an SSH
//! reverse tunnel (`-R`) to the router's listener, held by an `ssh` of its own while the run goes on, so the runner
//! needs no access to the server's network. When the run ends, what the agent left is committed there and fetched
//! into the run's worktree, which is committed as for a local run.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Duration;

use super::Slot;
use crate::config::RunnerConfig;

/// How long the tunnel may take to come up.
const TUNNEL_TIMEOUT: Duration = Duration::from_secs(30);

/// The environment variables of a run that name the server's own services, which the runner cannot reach: only the
/// router is tunnelled.
const NOT_TUNNELLED: [&str; 3] = ["MOEBIUS", "MOEBIUS_URL", "MOEBIUS_TOKEN"];

/// What is local about a run: its worktree, its directory under the data directory, and the router's base URL.
pub(in crate::harness) struct Local<'a> {
    pub worktree: &'a Path,
    pub run_dir: &'a Path,
    pub router_url: &'a str,
}

/// A run on a runner, from the slot it holds to the tunnel it keeps open.
pub(in crate::harness) struct RemoteRun {
    config: RunnerConfig,
    run_id: i64,
    /// The run's directory on the runner.
    dir: String,
    tunnel: Mutex<Option<Child>>,
    /// The router's base URL as the runner sees it, once the tunnel is up.
    tunnel_url: OnceLock<String>,
    /// The bare clone the run's branch was pushed to, once it was.
    repo: OnceLock<String>,
    _slot: Slot,
}

impl RemoteRun {
    /// A run `run_id` on the runner `slot` is for; nothing is done on the runner yet.
    pub fn new(slot: Slot, run_id: i64) -> Self {
        let config = slot
            .runners
            .config(&slot.name)
            .expect("a slot is for a configured runner")
            .clone();
        Self {
            dir: format!("{}/runs/{run_id}", config.working_dir.trim_end_matches('/')),
            config,
            run_id,
            tunnel: Mutex::new(None),
            tunnel_url: OnceLock::new(),
            repo: OnceLock::new(),
            _slot: slot,
        }
    }

    /// The runner's name.
    pub fn runner(&self) -> &str {
        &self.config.name
    }

    /// The router's base URL as the run sees it: the end of the tunnel on the runner.
    pub fn router_url(&self) -> Option<&str> {
        self.tunnel_url.get().map(String::as_str)
    }

    /// Opens the reverse tunnel from a port the runner chooses to the router at `router_url`, and waits until it is up.
    pub fn connect(&self, router_url: &str) -> Result<(), String> {
        let (scheme, rest) = router_url
            .split_once("://")
            .ok_or_else(|| format!("{router_url} is not a URL"))?;
        let local = rest.split('/').next().unwrap_or(rest);
        let mut child = self
            .ssh(&["-N", "-o", "ExitOnForwardFailure=yes", "-R"])
            .arg(format!("0:{local}"))
            .arg(self.target())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", self.config.ssh.display()))?;
        let stderr = child.stderr.take().expect("stderr is piped");
        let (sender, receiver) = mpsc::channel();
        // The tunnel says which port it was given; the thread then keeps reading, so that the tunnel never blocks on a
        // full pipe, and tells how it ended when it does.
        std::thread::spawn(move || {
            let mut said = Vec::new();
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                match allocated_port(&line) {
                    Some(port) => drop(sender.send(Ok(port))),
                    None => said.push(line),
                }
            }
            let _ = sender.send(Err(said.join("\n")));
        });
        let port = match receiver.recv_timeout(TUNNEL_TIMEOUT) {
            Ok(Ok(port)) => port,
            Ok(Err(said)) => {
                let _ = child.wait();
                return Err(format!(
                    "the tunnel to {} did not come up: {}",
                    self.config.host,
                    said.trim()
                ));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "the tunnel to {} did not come up in {} seconds",
                    self.config.host,
                    TUNNEL_TIMEOUT.as_secs()
                ));
            }
        };
        *self.tunnel.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
        let _ = self.tunnel_url.set(format!("{scheme}://127.0.0.1:{port}"));
        Ok(())
    }

    /// Pushes the worktree's commit as the run's branch to the bare clone `repository` names, and clones it to the
    /// run's directory, ready for the harness.
    pub fn prepare(&self, worktree: &Path, repository: &str) -> Result<(), String> {
        let root = self.config.working_dir.trim_end_matches('/');
        let repo = format!("{root}/repos/{repository}.git");
        self.script(&format!(
            "mkdir -p {} && git init --bare --quiet {}",
            quote(&self.dir),
            quote(&repo)
        ))?;
        let branch = format!("moebius/run-{}", self.run_id);
        self.git(
            worktree,
            &[
                "push",
                "--quiet",
                "--force",
                &self.url(&repo),
                &format!("HEAD:refs/heads/{branch}"),
            ],
        )?;
        let _ = self.repo.set(repo.clone());
        self.script(&format!(
            "cd {dir} && rm -rf work && git clone --quiet --branch {branch} {repo} work \
             && git -C work config user.name Moebius && git -C work config user.email moebius@localhost",
            dir = quote(&self.dir),
            branch = quote(&branch),
            repo = quote(&repo),
        ))
    }

    /// The command that starts `command` on the runner, in the run's clone: the paths of the server that it names are
    /// copied to the runner and made the runner's, the router's URL becomes the tunnel's, and the environment is the
    /// one `command` sets. Its standard input and output are the harness's.
    pub fn command(&self, command: &Command, local: &Local) -> Result<Command, String> {
        let router_url = self
            .router_url()
            .ok_or("the tunnel to the runner is not open")?;
        let files = format!("{}/files", self.dir);
        let mut shipped = BTreeSet::new();
        let mut translate = |value: &std::ffi::OsStr| -> String {
            let text = value.to_string_lossy().into_owned();
            if let Some(rest) = text.strip_prefix(local.router_url) {
                return format!("{router_url}{rest}");
            }
            let path = Path::new(&text);
            if let Ok(inside) = path.strip_prefix(local.worktree) {
                return format!("{}/work/{}", self.dir, inside.display());
            }
            let is_path = text.starts_with('/') && !text.contains('\n');
            if is_path && (path.exists() || path.starts_with(local.run_dir)) {
                if path.exists() {
                    shipped.insert(path.to_path_buf());
                }
                return format!("{files}{text}");
            }
            text
        };
        let mut script = format!("cd {}/work && exec env", quote(&self.dir));
        for (name, value) in command.get_envs() {
            let name = name.to_string_lossy();
            if NOT_TUNNELLED.contains(&name.as_ref()) || !valid_variable(&name) {
                continue;
            }
            match value {
                Some(value) => script.push_str(&format!(" {name}={}", quote(&translate(value)))),
                None => script.push_str(&format!(" -u {name}")),
            }
        }
        let program = Path::new(command.get_program())
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let program = match &self.config.bin_dir {
            Some(dir) => format!("{}/{program}", dir.trim_end_matches('/')),
            None => program,
        };
        script.push(' ');
        script.push_str(&quote(&program));
        for arg in command.get_args() {
            script.push(' ');
            script.push_str(&quote(&translate(arg)));
        }
        self.ship(&shipped, &files)?;
        let mut ssh = self.ssh(&[]);
        ssh.arg(self.target()).arg(script);
        Ok(ssh)
    }

    /// What the agent left on the runner is committed with `message` and fetched into `worktree`, which is then at the
    /// run's last commit, as if the run had worked there.
    pub fn bring_back(&self, worktree: &Path, message: &str) -> Result<(), String> {
        let work = format!("{}/work", self.dir);
        self.script(&format!(
            "cd {} && git add --all && (git diff --cached --quiet || git -c user.name=Moebius \
             -c user.email=moebius@localhost commit --quiet -m {})",
            quote(&work),
            quote(message)
        ))?;
        self.git(worktree, &["fetch", "--quiet", &self.url(&work), "HEAD"])?;
        self.git(worktree, &["reset", "--hard", "--quiet", "FETCH_HEAD"])
    }

    /// Closes the tunnel and removes the run's directory and branch on the runner, as far as the runner answers.
    pub fn cleanup(&self) {
        if let Some(mut tunnel) = self.tunnel.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = tunnel.kill();
            let _ = tunnel.wait();
        }
        if self.router_url().is_none() {
            return;
        }
        let mut script = format!("rm -rf {}", quote(&self.dir));
        if let Some(repo) = self.repo.get() {
            script.push_str(&format!(
                "; git --git-dir={} update-ref -d refs/heads/moebius/run-{}",
                quote(repo),
                self.run_id
            ));
        }
        let _ = self.script(&script);
    }

    /// Copies `paths` (and what is under them, with links followed) of the server to `files` on the runner, at the same
    /// paths under it.
    fn ship(&self, paths: &BTreeSet<PathBuf>, files: &str) -> Result<(), String> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut tar = Command::new("tar");
        tar.args(["-chf", "-", "-C", "/"]);
        for path in paths {
            tar.arg(path.strip_prefix("/").unwrap_or(path));
        }
        let mut tar = tar
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run tar: {e}"))?;
        let unpack = self
            .ssh(&[])
            .arg(self.target())
            .arg(format!("mkdir -p {0} && tar -xf - -C {0}", quote(files)))
            .stdin(tar.stdout.take().expect("stdout is piped"))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("cannot start {}: {e}", self.config.ssh.display()))?;
        let packed = tar.wait_with_output().map_err(|e| e.to_string())?;
        if !packed.status.success() {
            return Err(format!(
                "cannot pack the run's files: {}",
                String::from_utf8_lossy(&packed.stderr).trim()
            ));
        }
        if !unpack.status.success() {
            return Err(format!(
                "cannot copy the run's files to {}: {}",
                self.config.host,
                String::from_utf8_lossy(&unpack.stderr).trim()
            ));
        }
        Ok(())
    }

    /// Runs `script` in the runner's shell and fails with what it said when it fails.
    fn script(&self, script: &str) -> Result<(), String> {
        let output = self
            .ssh(&[])
            .arg(self.target())
            .arg(script)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot start {}: {e}", self.config.ssh.display()))?;
        if output.status.success() {
            return Ok(());
        }
        Err(format!(
            "{} failed: {}",
            self.config.host,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }

    /// Runs git in `dir` on the server, with `ssh` as the runner's.
    fn git(&self, dir: &Path, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .args(["-c", "ssh.variant=ssh", "-C"])
            .arg(dir)
            .args(args)
            .env("GIT_SSH_COMMAND", self.git_ssh_command())
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run git: {e}"))?;
        if output.status.success() {
            return Ok(());
        }
        Err(format!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }

    /// The `ssh` command for the runner up to the target: its options, the port, and `extra`. The target and the
    /// remote command, if any, come after it.
    fn ssh(&self, extra: &[&str]) -> Command {
        let mut command = Command::new(&self.config.ssh);
        command.args(self.options());
        if let Some(port) = self.config.port {
            command.arg("-p").arg(port.to_string());
        }
        command.args(extra);
        command
    }

    /// `user@host`, or the host alone.
    fn target(&self) -> String {
        match &self.config.user {
            Some(user) => format!("{user}@{}", self.config.host),
            None => self.config.host.clone(),
        }
    }

    /// The options every `ssh` to the runner gets: no questions asked, the key if there is one, and a connection that
    /// notices when the runner is gone.
    fn options(&self) -> Vec<String> {
        let mut options: Vec<String> = [
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "ServerAliveInterval=30",
            "-o",
            "LogLevel=ERROR",
        ]
        .map(String::from)
        .to_vec();
        if let Some(key) = &self.config.key_file {
            options.extend(["-i".into(), key.to_string_lossy().into_owned()]);
            options.extend(["-o".into(), "IdentitiesOnly=yes".into()]);
        }
        options
    }

    /// What git is given as `GIT_SSH_COMMAND`: the program and the options, but not the port, which the URL carries.
    fn git_ssh_command(&self) -> String {
        std::iter::once(self.config.ssh.to_string_lossy().into_owned())
            .chain(self.options())
            .map(|word| quote(&word))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The URL of `path` (absolute) on the runner.
    fn url(&self, path: &str) -> String {
        let host = if self.config.host.contains(':') {
            format!("[{}]", self.config.host)
        } else {
            self.config.host.clone()
        };
        let port = self.config.port.map_or(String::new(), |p| format!(":{p}"));
        let user = self
            .config
            .user
            .as_deref()
            .map_or(String::new(), |u| format!("{u}@"));
        format!("ssh://{user}{host}{port}{path}")
    }
}

impl Drop for RemoteRun {
    fn drop(&mut self) {
        if let Some(mut tunnel) = self.tunnel.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = tunnel.kill();
            let _ = tunnel.wait();
        }
    }
}

/// `text` as one word of a POSIX shell.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Whether `name` can name an environment variable in a shell command.
fn valid_variable(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The port in the line `ssh` prints when it was asked for port 0: `Allocated port 40211 for remote forward to ...`.
fn allocated_port(line: &str) -> Option<u16> {
    line.strip_prefix("Allocated port ")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_quoted_for_the_shell() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert!(
            valid_variable("MOEBIUS_RUN_TOKEN") && !valid_variable("1A") && !valid_variable("A-B")
        );
    }

    #[test]
    fn the_tunnels_port_is_read_from_what_ssh_says() {
        let line = "Allocated port 40211 for remote forward to 127.0.0.1:7172";
        assert_eq!(allocated_port(line), Some(40211));
        assert_eq!(allocated_port("Permission denied"), None);
    }
}
