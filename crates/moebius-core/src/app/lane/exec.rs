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
/// How many output lines that start with a step's prefix are read. More is [`Ran::prefixed_cut`]: the lines that are
/// past it are not rows, and the step fails, since which of them were the real ones is not known.
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
    /// Whether there were more such lines than [`PREFIXED_AT_MOST`]: the rest was dropped, so the lines are not to be
    /// trusted (the code under test can print as many as it likes before the real ones).
    pub prefixed_cut: bool,
    /// Whether a line had the prefix in the middle, or was too long to be read: see [`Captured`].
    pub prefixed_ambiguous: bool,
}

impl Ran {
    fn failed(why: String) -> Self {
        Ran {
            ended: Ended::Failed(why),
            output: String::new(),
            prefixed: Vec::new(),
            prefixed_cut: false,
            prefixed_ambiguous: false,
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
    /// The watcher of the script stopped it: the job wrote more than the lane's `disk`, or left the disk less free than
    /// its reserve.
    DiskFull,
    /// The same for the disk of the lane under its reserve, which the job's own directory being small does not make its doing.
    DiskLaneFull,
}

/// `text` as one word of a shell command.
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// What a container of one command of a job is made of.
pub struct Isolated<'a> {
    /// The job it is for: the container is labelled with it, so that the job's containers can be found and removed.
    pub job: i64,
    pub name: &'a str,
    pub job_dir: &'a str,
    pub side: &'a str,
    pub environment: &'a [(String, String)],
    /// The paths of the measuring harness that are there: each is mounted over the checkout from the job's own copy
    /// of the merge base's, read-only, so that nothing a step runs can change what measures it.
    pub harness: &'a [String],
}

/// A path of the harness as the checkout and the job's copy have it: no slash at either end.
pub fn harness_path(path: &str) -> &str {
    path.trim_matches('/')
}

/// Whether `path` is `parent` or lies under it.
fn is_or_under(path: &str, parent: &str) -> bool {
    path == parent || path.starts_with(&format!("{parent}/"))
}

/// The directories above the harness paths, each once, the shallowest first: `tools` for `tools/kbench` and
/// `tools/sub/bench`, and `tools/sub` after it.
pub fn harness_ancestors(harness: &[String]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for path in harness {
        let path = harness_path(path);
        let mut at = 0;
        while let Some(next) = path[at..].find('/') {
            let ancestor = &path[..at + next];
            // A harness path, or one under a harness path, is mounted read-only itself and is not mounted twice.
            if !found.iter().any(|f| f == ancestor)
                && !harness
                    .iter()
                    .any(|other| is_or_under(ancestor, harness_path(other)))
            {
                found.push(ancestor.to_string());
            }
            at += next + 1;
        }
    }
    found.sort_by_key(|a| a.matches('/').count());
    found
}

/// The arguments the container program is given to run `command` of a job in a fresh, isolated container: no network,
/// no capability, no new privilege, a read-only root, an unprivileged user, a `/tmp` no larger than the memory, the
/// memory (swap included), the processors and the file size the lane allows, and the checkout of one side as the only
/// place to write and the only part of the job's directory it sees (the other side is not mounted) with the harness over
/// it, read-only, and the devices and groups the GPUs need. The container is named and labelled so that it can be found
/// and removed.
pub fn container_args(lane: &LaneConfig, container: &Isolated, command: &str) -> Vec<String> {
    let Isolated {
        job,
        name,
        job_dir,
        side,
        environment,
        harness,
    } = container;
    let memory = lane.memory.trim().to_string();
    let tmp = lane.memory_bytes().min(8 << 30);
    let disk = lane.disk_bytes();
    let mut args: Vec<String> = [
        "run",
        "--name",
        name,
        "--label",
        &format!("moebius-lane={name}"),
        "--label",
        &format!("moebius-lane-job={job}"),
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
        // The output reaches the server through the attached streams; the runtime's own log would be written to its disk, out of
        // the sight of the watcher of the job's directory.
        "--log-driver",
        "none",
        "--memory",
        &memory,
        "--memory-swap",
        &memory,
        "--oom-score-adj",
        "1000",
        "--ulimit",
        &format!("fsize={disk}:{disk}"),
        "--tmpfs",
        &format!("/tmp:rw,exec,nosuid,size={tmp}"),
        "-e",
        "HOME=/tmp",
    ]
    .map(String::from)
    .to_vec();
    if let Some(cpus) = &lane.cpus {
        args.extend(["--cpus".into(), cpus.trim().to_string()]);
    }
    for (name, value) in *environment {
        args.extend(["-e".into(), format!("{name}={value}")]);
    }
    for device in &lane.devices {
        args.extend(["--device".into(), device.clone()]);
    }
    for group in &lane.groups {
        args.extend(["--group-add".into(), group.clone()]);
    }
    args.extend(["-v".into(), format!("{job_dir}/{side}:/work:rw")]);
    // Every directory above a harness path is mounted over itself, shallowest first, so that none can be renamed or
    // removed (`EBUSY`): renaming `tools` away and making a new `tools/kbench` would plant a harness otherwise, since the
    // read-only mount of the leaf goes along with the directory it is in.
    for ancestor in harness_ancestors(harness) {
        args.extend([
            "-v".into(),
            format!("{job_dir}/{side}/{ancestor}:/work/{ancestor}:rw"),
        ]);
    }
    // After the mount of the checkout, so that they lie over it.
    let mut mounted: Vec<&str> = Vec::new();
    for path in *harness {
        let path = harness_path(path);
        // One that lies under another harness path (or is the same) is in that mount already.
        if mounted.iter().any(|done| is_or_under(path, done)) {
            continue;
        }
        mounted.push(path);
        args.extend([
            "-v".into(),
            format!("{job_dir}/harness/{path}:/work/{path}:ro"),
        ]);
    }
    args.extend([
        "-w".into(),
        "/work".into(),
        lane.image.clone().unwrap_or_default(),
        "sh".into(),
        "-c".into(),
        command.to_string(),
    ]);
    args
}

/// What a command's script watches while it runs: the disk (ADR 0035). Every few seconds the size of the job's directory and
/// what the disk of the lane has left (blocks and inodes) are read; past `disk_kib` of the first or under a 5 % reserve of
/// the others, the command is killed and its script says `disk`.
pub struct Watch<'a> {
    pub work_dir: &'a str,
    pub disk_kib: u64,
}

/// The marker a watcher leaves when it stopped a command, in the job's directory: `job` when the job's own directory passed
/// its limit, `lane` when the disk of the lane fell under its reserve. They are told apart because the first is what the
/// change did and the second may be anyone's.
fn marker(job_dir: &str, which: &str) -> String {
    quote(&format!("{job_dir}/disk-{which}"))
}

/// The script that ends with the token line `disk` or `disklane` when a watcher stopped the command, else goes on.
fn disk_verdict(job_dir: &str, token: &str, remove: &str) -> String {
    format!(
        "kill $w $w2 2>/dev/null; \
         if [ -e {job} ]; then {remove}echo \"{token} disk\"; exit 0; fi; \
         if [ -e {lane} ]; then {remove}echo \"{token} disklane\"; exit 0; fi; ",
        job = marker(job_dir, "job"),
        lane = marker(job_dir, "lane"),
    )
}

/// The commands of a script that start the watcher of `watch` in the background, with `kill` to stop the command.
fn watcher(watch: &Watch, job_dir: &str, kill: &str) -> String {
    format!(
        "rm -f {marker_job} {marker_lane}; \
         ( while :; do sleep 2; \
           out=$(timeout 20 du -skx {job} 2>{errors}); rc=$?; used=${{out%%[!0-9]*}}; over=0; \
           if [ $rc -eq 124 ] || [ $rc -eq 137 ] || [ -z \"$used\" ]; then over=1; \
           elif grep -q -e 'Permission denied' -e 'cannot read directory' {errors}; then over=1; \
           elif [ \"$used\" -gt {limit} ]; then over=1; fi; \
           if [ $over -eq 1 ]; then : > {marker_job}; {kill}; exit 0; fi; done ) >/dev/null 2>&1 & w=$!; \
         ( while :; do sleep 2; \
           set -- $(df -Pk {work} 2>/dev/null | awk 'NR==2 {{print $2, $4}}'); tk=$1; ak=$2; \
           set -- $(df -Pi {work} 2>/dev/null | awk 'NR==2 {{print $2, $4}}'); ti=$1; ai=$2; \
           if [ \"${{ak:-0}}\" -lt $((${{tk:-0}} / 20)) ] || [ \"${{ai:-0}}\" -lt $((${{ti:-0}} / 20)) ]; then \
             : > {marker_lane}; {kill}; exit 0; fi; done ) >/dev/null 2>&1 & w2=$!; ",
        marker_job = marker(job_dir, "job"),
        marker_lane = marker(job_dir, "lane"),
        errors = quote(&format!("{job_dir}/du-errors")),
        job = quote(job_dir),
        work = quote(watch.work_dir),
        limit = watch.disk_kib,
    )
}

/// The shell script that runs `command` directly on the machine, in the checkout `side` of the job's directory, in a
/// process group of its own (`timeout` makes one) whose id is written to `pid_file`, and kills what is left of that
/// group when the command has ended. Its last line is `<token> direct <exit code> <seconds it took>`.
#[allow(clippy::too_many_arguments)]
pub fn direct_script(
    job_dir: &str,
    side: &str,
    environment: &[(String, String)],
    command: &str,
    seconds: u64,
    pid_file: &str,
    token: &str,
    watch: Option<&Watch>,
) -> String {
    let (start, stop) = match watch {
        Some(watch) => (
            watcher(
                watch,
                job_dir,
                &format!(
                    "kill -KILL -$(cat {} 2>/dev/null) 2>/dev/null",
                    quote(pid_file)
                ),
            ),
            disk_verdict(job_dir, token, ""),
        ),
        None => (String::new(), String::new()),
    };
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
        "unset SSH_AUTH_SOCK SSH_AGENT_PID; cd {dir}/{side} && export HOME={dir}/home && {exports}{start}s=$(date +%s); \
         sh -c {inner} 2>&1; c=$?; kill -KILL -$(cat {pid} 2>/dev/null) 2>/dev/null; e=$(date +%s); {stop}\
         echo \"{token} direct $c $((e-s))\"",
        dir = quote(job_dir),
        inner = quote(&inner),
        pid = quote(pid_file),
    )
}

/// The shell script that removes a container of the same name a server that stopped may have left, runs `command` in a
/// container, then asks the runtime how the container ended (so that the command's own exit code is told from the
/// runtime's), and removes the container whatever came of it. Its last line is `<token> <running> <exit code>`, or
/// `<token> gone` when there was no container to ask.
pub fn contained_script(
    lane: &LaneConfig,
    container: &Isolated,
    command: &str,
    seconds: u64,
    token: &str,
    watch: Option<&Watch>,
) -> String {
    let name = container.name;
    let mut args = container_args(lane, container, command);
    // The runtime's own bound, besides the `timeout` of the client.
    if lane.runtime.rsplit('/').next() == Some("podman") {
        args.splice(1..1, ["--timeout".to_string(), seconds.to_string()]);
    }
    let words: String = args.iter().map(|w| format!(" {}", quote(w))).collect();
    let runtime = quote(&lane.runtime);
    let name = quote(name);
    let (start, stop) = match watch {
        Some(watch) => (
            watcher(
                watch,
                container.job_dir,
                &format!("{runtime} rm -f {name} >/dev/null 2>&1"),
            ),
            disk_verdict(
                container.job_dir,
                token,
                &format!("{runtime} rm -f {name} >/dev/null 2>&1; "),
            ),
        ),
        None => (String::new(), String::new()),
    };
    format!(
        "unset SSH_AUTH_SOCK SSH_AGENT_PID; {runtime} rm -f {name} >/dev/null 2>&1; {start}\
         timeout -k 10 {seconds} {runtime}{words} 2>&1; \
         st=$({runtime} inspect -f '{{{{.State.Running}}}} {{{{.State.ExitCode}}}}' {name} 2>/dev/null) || st=gone; \
         {stop}{runtime} rm -f {name} >/dev/null 2>&1; echo \"{token} $st\""
    )
}

/// The output of a script without its last line, and how its command ended, when that line is there. `seconds` is the
/// command's limit: an exit code of 124 after that long is the `timeout` of the machine, and before it is the command's.
pub fn read_status(output: &str, token: &str, seconds: u64) -> (String, Option<Finish>) {
    let Some(at) = output.rfind(token) else {
        return (output.to_string(), None);
    };
    let (before, line) = output.split_at(at);
    let words: Vec<&str> = line.split_whitespace().skip(1).collect();
    let finish = match words.as_slice() {
        ["direct", code, took] => match (code.parse::<i32>(), took.parse::<u64>()) {
            (Ok(124), Ok(took)) if took >= seconds => Some(Finish::TimedOut),
            (Ok(code), Ok(_)) => Some(Finish::Exit(code)),
            _ => None,
        },
        ["gone"] => Some(Finish::Gone),
        ["disk"] => Some(Finish::DiskFull),
        ["disklane"] => Some(Finish::DiskLaneFull),
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
        let out = out_receiver.recv_timeout(PIPES).unwrap_or_default();
        let said = err_receiver.recv_timeout(PIPES).unwrap_or_default().text;
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
            output: out.text,
            prefixed: out.prefixed,
            prefixed_cut: out.cut,
            prefixed_ambiguous: out.ambiguous,
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

/// What [`capture`] read of a stream.
#[derive(Debug, Default)]
struct Captured {
    text: String,
    prefixed: Vec<String>,
    /// More lines had the prefix than are read.
    cut: bool,
    /// A line had the prefix somewhere but at its start (code under test can print a part of a line and leave the rest
    /// to the harness, so that the harness's row is joined to its own and lost), or was longer than a line is read.
    ambiguous: bool,
}

/// The longest a line is read to be a row.
const LINE_AT_MOST: usize = 4096;

/// Reads a stream to its end, keeping the end of it up to [`OUTPUT_AT_MOST`] bytes and the lines that start with
/// `prefix`, up to [`PREFIXED_AT_MOST`] of them. A stream that has the prefix in the middle of a line, or a line too long
/// to be read, is [`Captured::ambiguous`] when a prefix is asked for: which rows it held is not known.
fn capture(mut stream: impl Read, prefix: Option<&str>) -> Captured {
    let mut kept: Vec<u8> = Vec::new();
    let mut found = Captured::default();
    let mut line: Vec<u8> = Vec::new();
    let mut overflow = false;
    let mut buffer = [0u8; 16 * 1024];
    let note = |line: &mut Vec<u8>, overflow: &mut bool, found: &mut Captured| {
        if let Some(prefix) = prefix {
            let prefix = prefix.as_bytes();
            if line.starts_with(prefix) {
                if found.prefixed.len() < PREFIXED_AT_MOST {
                    found
                        .prefixed
                        .push(String::from_utf8_lossy(line).trim_end().to_string());
                } else {
                    found.cut = true;
                }
            }
            if *overflow
                || (!prefix.is_empty() && line.windows(prefix.len()).skip(1).any(|w| w == prefix))
            {
                found.ambiguous = true;
            }
        }
        line.clear();
        *overflow = false;
    };
    while let Ok(n) = stream.read(&mut buffer) {
        if n == 0 {
            break;
        }
        for &byte in &buffer[..n] {
            if byte == b'\n' {
                note(&mut line, &mut overflow, &mut found);
            } else if line.len() < LINE_AT_MOST {
                line.push(byte);
            } else {
                overflow = true;
            }
        }
        kept.extend_from_slice(&buffer[..n]);
        if kept.len() > 2 * OUTPUT_AT_MOST {
            kept.drain(..kept.len() - OUTPUT_AT_MOST);
        }
    }
    note(&mut line, &mut overflow, &mut found);
    if kept.len() > OUTPUT_AT_MOST {
        kept.drain(..kept.len() - OUTPUT_AT_MOST);
    }
    found.text = String::from_utf8_lossy(&kept).into_owned();
    found
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
            memory: "16g".into(),
            cpus: None,
            shared_disk: false,
            disk: "20g".into(),
            gpus: None,
            power_cap: None,
            rocm: None,
            driver: None,
        }
    }

    fn isolated<'a>(side: &'a str, harness: &'a [String]) -> Isolated<'a> {
        Isolated {
            job: 7,
            name: "n",
            job_dir: "/jobs/7",
            side,
            environment: &[],
            harness,
        }
    }

    #[test]
    fn harness_paths_that_lie_under_one_another_are_mounted_once() {
        let paths = [
            "bench".to_string(),
            "bench/data".to_string(),
            "tools/a/b".to_string(),
        ];
        let args = container_args(&lane(), &isolated("after", &paths), "true");
        let mounts: Vec<&str> = args
            .iter()
            .enumerate()
            .filter(|(at, _)| *at > 0 && args[*at - 1] == "-v")
            .map(|(_, a)| a.as_str())
            .collect();
        let destinations: Vec<&str> = mounts
            .iter()
            .map(|m| m.split(':').nth(1).unwrap())
            .collect();
        let mut unique = destinations.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), destinations.len(), "{mounts:?}");
        // `bench` is read-only and holds `bench/data`; only the directories above `tools/a/b` are over themselves.
        assert!(
            mounts
                .iter()
                .any(|m| m.ends_with("/harness/bench:/work/bench:ro")),
            "{mounts:?}"
        );
        assert!(
            !mounts.iter().any(|m| m.contains("bench/data")),
            "{mounts:?}"
        );
        assert_eq!(harness_ancestors(&paths), ["tools", "tools/a"]);
    }

    #[test]
    fn a_container_does_not_log_its_output_to_the_disk_of_the_runtime() {
        let args = container_args(&lane(), &isolated("after", &[]), "true");
        let at = args
            .iter()
            .position(|a| a == "--log-driver")
            .expect("a log driver");
        assert_eq!(args[at + 1], "none");
    }

    #[test]
    fn the_watcher_counts_what_it_cannot_read_as_over_and_checks_the_disk_on_its_own() {
        let watch = Watch {
            work_dir: "/w",
            disk_kib: 100,
        };
        let script = watcher(&watch, "/w/jobs/1", "kill-it");
        assert!(script.contains("timeout 20 du -skx"), "{script}");
        assert!(script.contains("Permission denied"), "{script}");
        assert!(
            script.contains("disk-job") && script.contains("disk-lane"),
            "{script}"
        );
        assert!(script.contains("w2=$!"), "the disk's own loop: {script}");
    }

    #[test]
    fn a_container_is_limited_in_memory_processors_and_file_size() {
        let mut limited = lane();
        limited.memory = "4g".into();
        limited.cpus = Some("2.5".into());
        limited.disk = "10g".into();
        let args = container_args(&limited, &isolated("after", &[]), "true");
        let joined = args.join(" ");
        let gib = 1u64 << 30;
        for part in [
            "--memory 4g --memory-swap 4g".to_string(),
            "--oom-score-adj 1000".to_string(),
            format!("--ulimit fsize={}:{}", 10 * gib, 10 * gib),
            "--cpus 2.5".to_string(),
            // The /tmp is in memory: no larger than the memory.
            format!("--tmpfs /tmp:rw,exec,nosuid,size={}", 4 * gib),
        ] {
            assert!(joined.contains(&part), "{part} in {joined}");
        }
        // A large limit does not make a larger /tmp than the 8 GiB there always was, and no limit on processors is none.
        let args = container_args(&lane(), &isolated("after", &[]), "true").join(" ");
        assert!(args.contains(&format!("size={}", 8 * gib)), "{args}");
        assert!(args.contains("--memory 16g --memory-swap 16g"), "{args}");
        assert!(!args.contains("--cpus"), "{args}");
    }

    #[test]
    fn the_harness_lies_over_the_checkout_read_only_and_after_it() {
        let paths = ["bench/".to_string(), "/tools/kbench".to_string()];
        let args = container_args(&lane(), &isolated("after", &paths), "true");
        let mounts: Vec<&str> = args
            .iter()
            .enumerate()
            .filter(|(at, _)| *at > 0 && args[*at - 1] == "-v")
            .map(|(_, a)| a.as_str())
            .collect();
        assert_eq!(
            mounts,
            [
                "/jobs/7/after:/work:rw",
                // The directory above a harness path, over itself, so that it cannot be renamed away.
                "/jobs/7/after/tools:/work/tools:rw",
                "/jobs/7/harness/bench:/work/bench:ro",
                "/jobs/7/harness/tools/kbench:/work/tools/kbench:ro",
            ]
        );
    }

    #[test]
    fn a_container_of_the_same_name_is_removed_before_a_command_starts() {
        let script = contained_script(&lane(), &isolated("after", &[]), "true", 90, "T", None);
        let removal = script
            .find("'rm' '-f' 'n'")
            .or_else(|| script.find("rm -f 'n'"));
        let run = script.find("'run'").unwrap();
        assert!(removal.is_some_and(|at| at < run), "{script}");
    }

    #[test]
    fn more_prefixed_lines_than_are_read_say_so() {
        let mut text = String::from(
            "ROW forged 1
",
        );
        text.push_str(
            &"ROW junk 1
"
            .repeat(PREFIXED_AT_MOST),
        );
        text.push_str(
            "ROW real 2
",
        );
        let read = capture(text.as_bytes(), Some("ROW "));
        assert!(read.cut);
        assert_eq!(read.prefixed.len(), PREFIXED_AT_MOST);
        let read = capture("ROW a 1\nROW b 2\n".as_bytes(), Some("ROW "));
        assert!(!read.cut && !read.ambiguous);
        assert_eq!(read.prefixed.len(), 2);
    }

    #[test]
    fn a_line_with_the_prefix_in_the_middle_or_too_long_to_read_is_ambiguous() {
        // The code printed `X` and no newline, so the harness's row is joined to it and does not start a line.
        let joined = capture("ROW a 1 x\nXROW a 2 x\n".as_bytes(), Some("ROW "));
        assert!(joined.ambiguous);
        let long = format!("{}\nROW a 2 x\n", "y".repeat(5000));
        assert!(capture(long.as_bytes(), Some("ROW ")).ambiguous);
        // Nothing is ambiguous without a prefix, nor with the prefix only at the start of a line.
        assert!(!capture(long.as_bytes(), None).ambiguous);
        // "plain ROW" has the prefix's word at its end only, without the space: not the prefix.
        assert!(!capture("ROW a 1\nplain ROW\n".as_bytes(), Some("ROW ")).ambiguous);
        assert!(!capture("ROW a 1\nROW b 2\nnothing\n".as_bytes(), Some("ROW ")).ambiguous);
    }

    #[test]
    fn a_container_has_no_network_no_privilege_and_a_read_only_root() {
        let args = container_args(
            &lane(),
            &Isolated {
                job: 7,
                name: "n1",
                job_dir: "/jobs/7",
                side: "after",
                environment: &[("LANE_SIDE".into(), "after".into())],
                harness: &[],
            },
            "./build.sh",
        );
        let joined = args.join(" ");
        for part in [
            "run --name n1 --label moebius-lane=n1 --label moebius-lane-job=7",
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
            let args = container_args(&lane(), &isolated(side, &[]), "true");
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
        let script = contained_script(&lane(), &isolated("after", &[]), "true", 90, "T", None);
        assert!(script.contains("'--timeout' '90'"), "{script}");
        let mut docker = lane();
        docker.runtime = "docker".into();
        let script = contained_script(&docker, &isolated("after", &[]), "true", 90, "T", None);
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
        let read = capture(text.as_bytes(), Some("ROW "));
        assert!(read.text.len() <= OUTPUT_AT_MOST + 8);
        assert!(read.text.ends_with("ROW b 2"));
        assert_eq!(
            read.prefixed,
            vec!["ROW a 1".to_string(), "ROW b 2".to_string()]
        );
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
