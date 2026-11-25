//! Running a command on a lane's machine (ADR 0035): the server itself (`sh`) or a runner (`ssh`), directly or in a
//! fresh container. Nothing here knows what a job is; [`super::job`] builds the commands.
//!
//! - [`Machine::run`] runs one shell script with a time limit, keeps the end of its output (and the lines that start
//!   with a prefix, for a step's rows), and stops it when told to. The child gets no environment of the server's but
//!   `PATH` and `HOME` (and `SSH_AUTH_SOCK` for `ssh`), so no secret of the server reaches a job.
//! - [`container_args`] is the one place that says how a job's container is isolated.
//! - [`Machine::push`] puts the pinned commits into a bare clone on the machine, over SSH for a runner.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::{LaneConfig, RunnerConfig};
use crate::platform;

/// How much of a command's output is kept: the end of it.
pub const OUTPUT_AT_MOST: usize = 256 * 1024;
/// How many output lines that start with a step's prefix are kept.
const PREFIXED_AT_MOST: usize = 20_000;
/// How long after its own time limit a command is given before the server kills the process it started.
const GRACE: Duration = Duration::from_secs(30);

/// Where a lane's commands run.
pub struct Machine {
    runner: Option<RunnerConfig>,
    work_dir: String,
}

/// How a command ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    Exit(i32),
    /// The time limit passed (the `timeout` of the machine said so, or the server had to stop it).
    TimedOut,
    /// It was stopped because the job was cancelled.
    Cancelled,
    /// It could not be started or reached.
    Failed(String),
}

/// What a command did.
#[derive(Debug)]
pub struct Ran {
    pub ended: Ended,
    /// The end of its output, both streams, as text.
    pub output: String,
    /// The lines of output that start with the prefix that was asked for.
    pub prefixed: Vec<String>,
}

impl Ran {
    fn failed(why: String) -> Self {
        Ran {
            ended: Ended::Failed(why),
            output: String::new(),
            prefixed: Vec::new(),
        }
    }
}

/// `text` as one word of a shell command.
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The arguments the container program is given to run `command` of a job in a fresh, isolated container: no network,
/// no capability, no new privilege, a read-only root, an unprivileged user, the job's directory as the only place to
/// write (and a `/tmp`), and the devices and groups the GPUs need. `side` is `before` or `after`: the checkout the
/// command starts in.
pub fn container_args(
    lane: &LaneConfig,
    name: &str,
    job_dir: &str,
    side: &str,
    environment: &[(String, String)],
    command: &str,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "--name",
        name,
        "--network",
        "none",
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--user",
        &lane.user,
        "--pids-limit",
        "4096",
        "--tmpfs",
        "/tmp:rw,exec,nosuid,size=8g",
        "-e",
        "HOME=/tmp",
    ]
    .map(String::from)
    .to_vec();
    for (name, value) in environment {
        args.extend(["-e".into(), format!("{name}={value}")]);
    }
    for device in &lane.devices {
        args.extend(["--device".into(), device.clone()]);
    }
    for group in &lane.groups {
        args.extend(["--group-add".into(), group.clone()]);
    }
    args.extend([
        "-v".into(),
        format!("{job_dir}:/work:rw"),
        "-w".into(),
        format!("/work/{side}"),
        lane.image.clone().unwrap_or_default(),
        "sh".into(),
        "-c".into(),
        command.to_string(),
    ]);
    args
}

/// The shell script that runs `command` directly on the machine, in the checkout `side` of the job's directory.
pub fn direct_script(
    job_dir: &str,
    side: &str,
    environment: &[(String, String)],
    command: &str,
    seconds: u64,
) -> String {
    let exports: String = environment
        .iter()
        .map(|(name, value)| format!("export {name}={}; ", quote(value)))
        .collect();
    format!(
        "cd {dir}/{side} && export HOME={dir}/home && {exports}exec timeout -k 10 {seconds} sh -c {command} 2>&1",
        dir = quote(job_dir),
        command = quote(command),
    )
}

/// The shell script that runs `command` in a container, and removes the container whatever came of it.
pub fn contained_script(
    lane: &LaneConfig,
    name: &str,
    job_dir: &str,
    side: &str,
    environment: &[(String, String)],
    command: &str,
    seconds: u64,
) -> String {
    let words: String = container_args(lane, name, job_dir, side, environment, command)
        .iter()
        .map(|w| format!(" {}", quote(w)))
        .collect();
    let runtime = quote(&lane.runtime);
    format!(
        "timeout -k 10 {seconds} {runtime}{words} 2>&1; code=$?; {runtime} rm -f {} >/dev/null 2>&1; exit $code",
        quote(name)
    )
}

impl Machine {
    /// The machine of `lane`: its runner, or the server itself with its jobs under `data_dir`.
    pub fn new(lane: &LaneConfig, runner: Option<&RunnerConfig>, data_dir: &Path) -> Self {
        let work_dir = lane.work_dir.clone().unwrap_or_else(|| match runner {
            Some(runner) => format!(
                "{}/lanes/{}",
                runner.working_dir.trim_end_matches('/'),
                lane.name
            ),
            None => format!("{}/lanes/{}", data_dir.display(), lane.name),
        });
        Machine {
            runner: runner.cloned(),
            work_dir,
        }
    }

    /// The lane's directory on the machine.
    pub fn work_dir(&self) -> &str {
        &self.work_dir
    }

    /// What the machine is called to a person.
    pub fn name(&self) -> String {
        match &self.runner {
            Some(runner) => runner.host.clone(),
            None => "the server".into(),
        }
    }

    fn ssh_options(&self, runner: &RunnerConfig) -> Vec<String> {
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
        if let Some(key) = &runner.key_file {
            options.extend(["-i".into(), key.to_string_lossy().into_owned()]);
            options.extend(["-o".into(), "IdentitiesOnly=yes".into()]);
        }
        options
    }

    /// The process that runs `script` on the machine, with the environment of the server cleared.
    fn command(&self, script: &str) -> Command {
        let mut command = match &self.runner {
            None => {
                let mut sh = Command::new("sh");
                sh.arg("-c").arg(script);
                sh
            }
            Some(runner) => {
                let mut ssh = Command::new(&runner.ssh);
                ssh.args(self.ssh_options(runner));
                if let Some(port) = runner.port {
                    ssh.arg("-p").arg(port.to_string());
                }
                ssh.arg(match &runner.user {
                    Some(user) => format!("{user}@{}", runner.host),
                    None => runner.host.clone(),
                });
                ssh.arg(script);
                ssh
            }
        };
        command.env_clear();
        for kept in ["PATH", "HOME", "SSH_AUTH_SOCK"] {
            if let Some(value) = std::env::var_os(kept) {
                command.env(kept, value);
            }
        }
        command
    }

    /// Runs `script` on the machine for at most `limit` (the script's own `timeout` is the one that should end it; the
    /// server stops what it started a little later). `cancelled` is asked every moment. The lines of the output that
    /// start with `prefix` are kept apart.
    pub fn run(
        &self,
        script: &str,
        limit: Duration,
        cancelled: &dyn Fn() -> bool,
        prefix: Option<&str>,
    ) -> Ran {
        let started = Instant::now();
        let mut command = self.command(script);
        platform::own_group(&mut command);
        let mut child = match command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => return Ran::failed(format!("cannot start {:?}: {e}", command.get_program())),
        };
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let prefix = prefix.map(str::to_string);
        let reading = std::thread::spawn(move || capture(stdout, prefix.as_deref()));
        let errors = std::thread::spawn(move || capture(stderr, None));
        let mut ended = None;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {}
                Err(e) => {
                    ended = Some(Ended::Failed(e.to_string()));
                    break None;
                }
            }
            if cancelled() {
                ended = Some(Ended::Cancelled);
                break None;
            }
            if started.elapsed() > limit + GRACE {
                ended = Some(Ended::TimedOut);
                break None;
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        if status.is_none() {
            platform::kill_tree(child.id());
            let _ = child.kill();
            let _ = child.wait();
        }
        let (output, prefixed) = reading.join().unwrap_or_default();
        let (said, _) = errors.join().unwrap_or_default();
        let ended = match (status, ended) {
            (Some(status), _) => match status.code() {
                Some(124) => Ended::TimedOut,
                Some(255) if self.runner.is_some() => {
                    Ended::Failed(format!("cannot reach {}: {}", self.name(), said.trim()))
                }
                Some(code) => Ended::Exit(code),
                None => Ended::Failed("the command was ended by a signal".into()),
            },
            (None, Some(ended)) => ended,
            (None, None) => Ended::Failed("the command was lost".into()),
        };
        Ran {
            ended,
            output,
            prefixed,
        }
    }

    /// Puts the commits `refs` (a commit and the ref it is called by there) from the server's clone `clone` into the
    /// bare clone `bare` on the machine, which is made when it is not there.
    pub fn push(
        &self,
        clone: &Path,
        bare: &str,
        refs: &[(&str, String)],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        let made = self.run(
            &format!("[ -d {b} ] || git init --bare --quiet {b}", b = quote(bare)),
            Duration::from_secs(60),
            cancelled,
            None,
        );
        if made.ended != Ended::Exit(0) {
            return Err(format!(
                "the clone on the machine could not be made: {}",
                describe(&made)
            ));
        }
        let mut git = Command::new("git");
        git.args(["-c", "ssh.variant=ssh", "-C"])
            .arg(clone)
            .arg("push")
            .arg("--quiet");
        match &self.runner {
            None => git.arg(bare),
            Some(runner) => {
                let host = if runner.host.contains(':') {
                    format!("[{}]", runner.host)
                } else {
                    runner.host.clone()
                };
                let user = runner
                    .user
                    .as_deref()
                    .map_or(String::new(), |u| format!("{u}@"));
                let port = runner.port.map_or(String::new(), |p| format!(":{p}"));
                let words: Vec<String> = std::iter::once(runner.ssh.to_string_lossy().into_owned())
                    .chain(self.ssh_options(runner))
                    .map(|w| quote(&w))
                    .collect();
                git.env("GIT_SSH_COMMAND", words.join(" "));
                git.arg(format!("ssh://{user}{host}{port}{bare}"))
            }
        };
        for (commit, name) in refs {
            git.arg(format!("{commit}:{name}"));
        }
        git.env("GIT_TERMINAL_PROMPT", "0").stdin(Stdio::null());
        let output = git.output().map_err(|e| format!("cannot run git: {e}"))?;
        if output.status.success() {
            return Ok(());
        }
        Err(format!(
            "the commits could not be sent to the machine: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// What a command that did not end well said, in a few words.
pub fn describe(ran: &Ran) -> String {
    match &ran.ended {
        Ended::Failed(why) => why.clone(),
        Ended::TimedOut => "it took too long".into(),
        Ended::Cancelled => "it was cancelled".into(),
        Ended::Exit(code) => {
            let last = ran
                .output
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("");
            format!(
                "it exited with code {code}: {}",
                last.chars().take(200).collect::<String>()
            )
        }
    }
}

/// Reads a stream to its end, keeping the end of it up to [`OUTPUT_AT_MOST`] bytes and the lines that start with
/// `prefix`.
fn capture(mut stream: impl Read, prefix: Option<&str>) -> (String, Vec<String>) {
    let mut kept: Vec<u8> = Vec::new();
    let mut prefixed = Vec::new();
    let mut line: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 16 * 1024];
    let note = |line: &mut Vec<u8>, prefixed: &mut Vec<String>| {
        if let Some(prefix) = prefix
            && prefixed.len() < PREFIXED_AT_MOST
            && line.starts_with(prefix.as_bytes())
        {
            prefixed.push(String::from_utf8_lossy(line).trim_end().to_string());
        }
        line.clear();
    };
    while let Ok(n) = stream.read(&mut buffer) {
        if n == 0 {
            break;
        }
        for &byte in &buffer[..n] {
            if byte == b'\n' {
                note(&mut line, &mut prefixed);
            } else if line.len() < 4096 {
                line.push(byte);
            }
        }
        kept.extend_from_slice(&buffer[..n]);
        if kept.len() > 2 * OUTPUT_AT_MOST {
            kept.drain(..kept.len() - OUTPUT_AT_MOST);
        }
    }
    note(&mut line, &mut prefixed);
    if kept.len() > OUTPUT_AT_MOST {
        kept.drain(..kept.len() - OUTPUT_AT_MOST);
    }
    (String::from_utf8_lossy(&kept).into_owned(), prefixed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Isolation;

    fn lane() -> LaneConfig {
        LaneConfig {
            name: "gpu1".into(),
            runner: None,
            work_dir: None,
            isolation: Isolation::Always,
            runtime: "podman".into(),
            image: Some("rocm/dev:6.2".into()),
            devices: vec!["/dev/kfd".into(), "/dev/dri".into()],
            groups: vec!["video".into()],
            user: "65534:65534".into(),
            timeout_seconds: 600,
            gpus: None,
            power_cap: None,
            rocm: None,
            driver: None,
        }
    }

    #[test]
    fn a_container_has_no_network_no_privilege_and_a_read_only_root() {
        let args = container_args(
            &lane(),
            "n1",
            "/jobs/7",
            "after",
            &[("LANE_SIDE".into(), "after".into())],
            "./build.sh",
        );
        let joined = args.join(" ");
        for part in [
            "run --rm --name n1",
            "--network none",
            "--read-only",
            "--cap-drop ALL",
            "--security-opt no-new-privileges",
            "--user 65534:65534",
            "--device /dev/kfd --device /dev/dri",
            "--group-add video",
            "-v /jobs/7:/work:rw -w /work/after",
            "-e LANE_SIDE=after",
            "rocm/dev:6.2 sh -c ./build.sh",
        ] {
            assert!(joined.contains(part), "{part} in {joined}");
        }
        assert!(!joined.contains("--privileged"));
        assert!(!args.iter().any(|a| a.contains("MOEBIUS")));
    }

    #[test]
    fn a_word_is_quoted_for_the_shell() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn the_end_of_the_output_and_the_rows_are_kept() {
        let text = format!("{}ROW a 1\nnoise\nROW b 2", "x\n".repeat(OUTPUT_AT_MOST));
        let (kept, rows) = capture(text.as_bytes(), Some("ROW "));
        assert!(kept.len() <= OUTPUT_AT_MOST + 8);
        assert!(kept.ends_with("ROW b 2"));
        assert_eq!(rows, vec!["ROW a 1".to_string(), "ROW b 2".to_string()]);
    }

    #[cfg(unix)] // runs `sh`
    #[test]
    fn a_command_runs_with_nothing_of_the_servers_environment() {
        let machine = Machine::new(&lane(), None, std::path::Path::new("/tmp"));
        let ran = machine.run("env", Duration::from_secs(10), &|| false, None);
        assert_eq!(ran.ended, Ended::Exit(0));
        // The test process has many variables (cargo sets dozens); only the few the machine keeps are there.
        let names: Vec<&str> = ran
            .output
            .lines()
            .filter_map(|l| l.split_once('=').map(|(name, _)| name))
            .collect();
        for name in &names {
            assert!(
                matches!(
                    *name,
                    "PATH" | "HOME" | "SSH_AUTH_SOCK" | "PWD" | "SHLVL" | "_" | "OLDPWD"
                ),
                "{name} reached the command: {names:?}"
            );
        }
    }

    #[cfg(unix)] // runs `sh`
    #[test]
    fn a_command_that_is_cancelled_is_stopped() {
        let machine = Machine::new(&lane(), None, std::path::Path::new("/tmp"));
        let began = Instant::now();
        let ran = machine.run("sleep 30", Duration::from_secs(60), &|| true, None);
        assert_eq!(ran.ended, Ended::Cancelled);
        assert!(began.elapsed() < Duration::from_secs(10));
    }
}
