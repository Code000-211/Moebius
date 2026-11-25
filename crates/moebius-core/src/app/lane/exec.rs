//! Running a command on a lane's machine (ADR 0035): the server itself (`sh`) or a runner (`ssh`), directly or in a
//! fresh container. Nothing here knows what a job is; [`super::job`] builds the commands.
//!
//! - [`Machine::run`] runs one shell script with a time limit, keeps the end of its output (and the lines that start
//!   with a prefix, for a step's rows), and stops it when told to. The child gets no environment of the server's but
//!   `PATH` and `HOME`; `SSH_AUTH_SOCK` goes to the `ssh` client alone, which forwards nothing. What a command left
//!   running in its process group is killed when it ends, and the pipes are read for a few seconds at most, so a
//!   background process cannot hold the lane.
//! - [`container_args`], [`contained_script`] and [`direct_script`] are the one place that says how a job's command is
//!   isolated and how its own end is told from the runtime's or the shell's: every script ends with a line that starts
//!   with a token only the server knows, which says how the *command* ended, so that an exit code of 124, 125 or 255 of the
//!   change's own command is the change's result and not an error of the lane ([`read_status`]).
//! - [`GIT`] and [`GIT_ENVIRONMENT`] are what every git command on the machine is run with: no configuration of the
//!   account, no hooks, no attributes file, no filter (so no `git-lfs`) and no prompt.
//! - [`Machine::push`] puts the pinned commits into a bare clone on the machine, over SSH for a runner.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::config::{LaneConfig, RunnerConfig};
use crate::platform;

/// How much of a command's output is kept: the end of it.
pub const OUTPUT_AT_MOST: usize = 256 * 1024;
/// How many output lines that start with a step's prefix are kept.
const PREFIXED_AT_MOST: usize = 20_000;
/// How long after its own time limit a command is given before the server kills the process it started.
const GRACE: Duration = Duration::from_secs(30);
/// How long the pipes of a command that ended are read before the process group is killed and they are let go.
const PIPES: Duration = Duration::from_secs(5);

/// What is set before any git command of a job: no configuration of the account or the system, no prompt, no `git-lfs`
/// download, no agent of SSH.
pub const GIT_ENVIRONMENT: &str = "export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 \
     GIT_TERMINAL_PROMPT=0 GIT_LFS_SKIP_SMUDGE=1; unset SSH_AUTH_SOCK SSH_AGENT_PID;";

/// A git command of a job: it reads no hook, no attributes file and no filter, and `git-lfs` is switched off, so the
/// attributes of a pull request cannot make anything run on the machine.
pub const GIT: &str = "git -c core.hooksPath=/dev/null -c core.attributesFile=/dev/null -c core.fsmonitor=false \
     -c filter.lfs.process= -c filter.lfs.smudge= -c filter.lfs.clean= -c filter.lfs.required=false \
     -c protocol.ext.allow=never";

/// Where a lane's commands run.
pub struct Machine {
    runner: Option<RunnerConfig>,
    work_dir: String,
    /// Variables a test gives every process of the machine, so that what a hostile account would have is there.
    #[cfg(test)]
    extra_environment: Vec<(String, String)>,
}

/// How a command ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    Exit(i32),
    /// The time limit passed (the server had to stop it).
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

/// How the *command* of a step ended, as its script's last line says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    /// It exited with this code, whatever the number is: 124, 125 and 255 included, they are its own.
    Exit(i32),
    /// It ran past its limit.
    TimedOut,
    /// There is no container: the runtime could not start it.
    Gone,
}

/// `text` as one word of a shell command.
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The arguments the container program is given to run `command` of a job in a fresh, isolated container: no network,
/// no capability, no new privilege, a read-only root, an unprivileged user, a `/tmp`, and the checkout of one side as
/// the only place to write and the only part of the job's directory it sees (the other side is not mounted), and the
/// devices and groups the GPUs need. The container is named and labelled so that it can be found and removed.
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
        "--name",
        name,
        "--label",
        &format!("moebius-lane={name}"),
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
        format!("{job_dir}/{side}:/work:rw"),
        "-w".into(),
        "/work".into(),
        lane.image.clone().unwrap_or_default(),
        "sh".into(),
        "-c".into(),
        command.to_string(),
    ]);
    args
}

/// The shell script that runs `command` directly on the machine, in the checkout `side` of the job's directory, in a
/// process group of its own (`timeout` makes one) whose id is written to `pid_file`, and kills what is left of that
/// group when the command has ended. Its last line is `<token> direct <exit code> <seconds it took>`.
pub fn direct_script(
    job_dir: &str,
    side: &str,
    environment: &[(String, String)],
    command: &str,
    seconds: u64,
    pid_file: &str,
    token: &str,
) -> String {
    let exports: String = environment
        .iter()
        .map(|(name, value)| format!("export {name}={}; ", quote(value)))
        .collect();
    let inner = format!(
        "echo $$ > {}; exec timeout -k 10 {seconds} sh -c {}",
        quote(pid_file),
        quote(command)
    );
    format!(
        "unset SSH_AUTH_SOCK SSH_AGENT_PID; cd {dir}/{side} && export HOME={dir}/home && {exports}s=$(date +%s); \
         sh -c {inner} 2>&1; c=$?; kill -KILL -$(cat {pid} 2>/dev/null) 2>/dev/null; e=$(date +%s); \
         echo \"{token} direct $c $((e-s))\"",
        dir = quote(job_dir),
        inner = quote(&inner),
        pid = quote(pid_file),
    )
}

/// The shell script that runs `command` in a container, then asks the runtime how the container ended (so that the
/// command's own exit code is told from the runtime's), and removes the container whatever came of it. Its last line is
/// `<token> <running> <exit code>`, or `<token> gone` when there was no container to ask.
pub fn contained_script(
    lane: &LaneConfig,
    name: &str,
    job_dir: &str,
    side: &str,
    environment: &[(String, String)],
    command: &str,
    seconds: u64,
    token: &str,
) -> String {
    let mut args = container_args(lane, name, job_dir, side, environment, command);
    // The runtime's own bound, besides the `timeout` of the client.
    if lane.runtime.rsplit('/').next() == Some("podman") {
        args.splice(1..1, ["--timeout".to_string(), seconds.to_string()]);
    }
    let words: String = args.iter().map(|w| format!(" {}", quote(w))).collect();
    let runtime = quote(&lane.runtime);
    let name = quote(name);
    format!(
        "unset SSH_AUTH_SOCK SSH_AGENT_PID; timeout -k 10 {seconds} {runtime}{words} 2>&1; \
         st=$({runtime} inspect -f '{{{{.State.Running}}}} {{{{.State.ExitCode}}}}' {name} 2>/dev/null) || st=gone; \
         {runtime} rm -f {name} >/dev/null 2>&1; echo \"{token} $st\""
    )
}

/// The output of a script without its last line, and how its command ended, when that line is there. `seconds` is the
/// command's limit: an exit code of 124 after that long is the `timeout` of the machine, and before it is the command's.
pub fn read_status(output: &str, token: &str, seconds: u64) -> (String, Option<Finish>) {
    let Some(at) = output.rfind(token) else {
        return (output.to_string(), None);
    };
    let (before, line) = output.split_at(at);
    let words: Vec<&str> = line.trim().split_whitespace().skip(1).collect();
    let finish = match words.as_slice() {
        ["direct", code, took] => match (code.parse::<i32>(), took.parse::<u64>()) {
            (Ok(124), Ok(took)) if took >= seconds => Some(Finish::TimedOut),
            (Ok(code), Ok(_)) => Some(Finish::Exit(code)),
            _ => None,
        },
        ["gone"] => Some(Finish::Gone),
        ["true", _] => Some(Finish::TimedOut),
        ["false", code] => code.parse().ok().map(Finish::Exit),
        _ => None,
    };
    (before.to_string(), finish)
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
            #[cfg(test)]
            extra_environment: Vec::new(),
        }
    }

    /// The same machine, with `variables` in the environment of everything that runs on it (for the tests).
    #[cfg(test)]
    pub fn with_environment(mut self, variables: Vec<(String, String)>) -> Self {
        self.extra_environment = variables;
        self
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
            "-o",
            "ForwardAgent=no",
            "-o",
            "ClearAllForwardings=yes",
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
        // The agent of SSH is for the `ssh` client alone; a command on the server itself never has it.
        let kept: &[&str] = if self.runner.is_some() {
            &["PATH", "HOME", "SSH_AUTH_SOCK"]
        } else {
            &["PATH", "HOME"]
        };
        for name in kept {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        #[cfg(test)]
        for (name, value) in &self.extra_environment {
            command.env(name, value);
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
        let (out_sender, out_receiver) = mpsc::channel();
        let (err_sender, err_receiver) = mpsc::channel();
        std::thread::spawn(move || drop(out_sender.send(capture(stdout, prefix.as_deref()))));
        std::thread::spawn(move || drop(err_sender.send(capture(stderr, None))));
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
        // Whatever is left in the command's process group goes with it, whether it ended or was stopped.
        platform::kill_tree(child.id());
        if status.is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
        // A process that got out of the group may still hold the pipes: they are read for a few seconds, then let go.
        let (output, prefixed) = out_receiver.recv_timeout(PIPES).unwrap_or_default();
        let (said, _) = err_receiver.recv_timeout(PIPES).unwrap_or_default();
        let ended = match (status, ended) {
            (Some(status), _) => match status.code() {
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
            &format!(
                "{GIT_ENVIRONMENT} [ -d {b} ] || {GIT} init --template= --bare --quiet {b}",
                b = quote(bare)
            ),
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
            "run --name n1 --label moebius-lane=n1",
            "--network none",
            "--read-only",
            "--cap-drop ALL",
            "--security-opt no-new-privileges",
            "--user 65534:65534",
            "--device /dev/kfd --device /dev/dri",
            "--group-add video",
            "-e LANE_SIDE=after",
            "rocm/dev:6.2 sh -c ./build.sh",
        ] {
            assert!(joined.contains(part), "{part} in {joined}");
        }
        assert!(!joined.contains("--privileged"));
        assert!(!args.iter().any(|a| a.contains("MOEBIUS")));
    }

    #[test]
    fn a_container_sees_the_side_it_runs_and_not_the_other() {
        for (side, other) in [("after", "before"), ("before", "after")] {
            let args = container_args(&lane(), "n", "/jobs/7", side, &[], "true");
            let mounts: Vec<&String> = args
                .iter()
                .enumerate()
                .filter(|(at, _)| *at > 0 && args[*at - 1] == "-v")
                .map(|(_, a)| a)
                .collect();
            assert_eq!(mounts, [&format!("/jobs/7/{side}:/work:rw")]);
            assert!(!args.iter().any(|a| a.contains(other)), "{args:?}");
            assert!(!args.iter().any(|a| a == "/jobs/7:/work:rw"));
        }
    }

    #[test]
    fn a_podman_container_is_given_the_runtimes_own_time_limit() {
        let script = contained_script(&lane(), "n", "/j", "after", &[], "true", 90, "T");
        assert!(script.contains("'--timeout' '90'"), "{script}");
        let mut docker = lane();
        docker.runtime = "docker".into();
        let script = contained_script(&docker, "n", "/j", "after", &[], "true", 90, "T");
        assert!(!script.contains("--timeout"), "{script}");
        assert!(
            script.contains("rm -f"),
            "the container is removed whatever came of it"
        );
    }

    #[test]
    fn a_word_is_quoted_for_the_shell() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn the_end_of_a_script_says_how_the_command_ended_and_the_commands_own_codes_are_its_own() {
        let said = |line: &str| read_status(&format!("out\n{line}\n"), "TOK", 60);
        assert_eq!(said("TOK direct 0 3").1, Some(Finish::Exit(0)));
        // 124, 125 and 255 of the command itself are its exit codes, as 2 is.
        for code in [124, 125, 255, 2] {
            assert_eq!(
                said(&format!("TOK direct {code} 3")).1,
                Some(Finish::Exit(code))
            );
        }
        // 124 after the whole limit is the `timeout` of the machine.
        assert_eq!(said("TOK direct 124 60").1, Some(Finish::TimedOut));
        assert_eq!(said("TOK false 125").1, Some(Finish::Exit(125)));
        assert_eq!(said("TOK true 0").1, Some(Finish::TimedOut));
        assert_eq!(said("TOK gone").1, Some(Finish::Gone));
        // No last line, or one of another token: nothing is known, and the output is whole.
        assert_eq!(read_status("out\nOTHER direct 0 1\n", "TOK", 60).1, None);
        assert_eq!(said("TOK direct 0 3").0, "out\n");
    }

    #[test]
    fn the_end_of_the_output_and_the_rows_are_kept() {
        let text = format!("{}ROW a 1\nnoise\nROW b 2", "x\n".repeat(OUTPUT_AT_MOST));
        let (kept, rows) = capture(text.as_bytes(), Some("ROW "));
        assert!(kept.len() <= OUTPUT_AT_MOST + 8);
        assert!(kept.ends_with("ROW b 2"));
        assert_eq!(rows, vec!["ROW a 1".to_string(), "ROW b 2".to_string()]);
    }

    #[test]
    fn git_runs_with_no_configuration_hook_or_filter_of_the_account() {
        for part in [
            "GIT_CONFIG_GLOBAL=/dev/null",
            "GIT_CONFIG_SYSTEM=/dev/null",
            "unset SSH_AUTH_SOCK",
        ] {
            assert!(GIT_ENVIRONMENT.contains(part), "{part}");
        }
        for part in [
            "core.hooksPath=/dev/null",
            "core.attributesFile=/dev/null",
            "filter.lfs.smudge=",
            "filter.lfs.required=false",
        ] {
            assert!(GIT.contains(part), "{part}");
        }
    }

    #[test]
    fn ssh_forwards_nothing() {
        let runner = RunnerConfig {
            name: "b".into(),
            host: "h".into(),
            user: None,
            port: None,
            key_file: None,
            working_dir: "/w".into(),
            max_runs: 1,
            bin_dir: None,
            ssh: "ssh".into(),
        };
        let machine = Machine::new(&lane(), Some(&runner), Path::new("/tmp"));
        let options = machine.ssh_options(&runner).join(" ");
        assert!(options.contains("ForwardAgent=no"), "{options}");
        assert!(options.contains("ClearAllForwardings=yes"), "{options}");
    }

    #[cfg(unix)] // runs `sh`
    #[test]
    fn a_command_runs_with_nothing_of_the_servers_environment() {
        let machine = Machine::new(&lane(), None, std::path::Path::new("/tmp"));
        let ran = machine.run("env", Duration::from_secs(10), &|| false, None);
        assert_eq!(ran.ended, Ended::Exit(0));
        // The test process has many variables (cargo sets dozens); only the few the machine keeps are there, and
        // the agent of SSH is not one of them on the server itself.
        let names: Vec<&str> = ran
            .output
            .lines()
            .filter_map(|l| l.split_once('=').map(|(name, _)| name))
            .collect();
        for name in &names {
            assert!(
                matches!(*name, "PATH" | "HOME" | "PWD" | "SHLVL" | "_" | "OLDPWD"),
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

    #[cfg(unix)] // runs `sh`
    #[test]
    fn a_background_process_that_holds_the_pipe_cannot_hold_the_job() {
        let machine = Machine::new(&lane(), None, std::path::Path::new("/tmp"));
        let began = Instant::now();
        // The shell ends at once; its child keeps standard output open for a minute.
        let ran = machine.run(
            "sleep 60 & echo done",
            Duration::from_secs(120),
            &|| false,
            None,
        );
        assert_eq!(ran.ended, Ended::Exit(0));
        assert!(ran.output.contains("done"));
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "took {:?}",
            began.elapsed()
        );
    }
}
