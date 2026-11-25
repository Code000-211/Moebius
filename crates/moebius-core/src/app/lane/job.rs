//! Running one job (ADR 0035): the pinned commits of the change are sent to the lane's machine and checked out there,
//! the project's steps run one after the other (each command in a fresh container, or directly on the machine, as the
//! lane's isolation says for this change), and a measuring step runs on the merge base and on the head by turns.
//!
//! What a step printed is data: it is kept to its last lines, redacted when it is posted, and only its rows (numbers)
//! reach the Reviewer. A job ends `measured` or `passed` when every step passed, `failed` when a step of the head
//! failed (the change is to blame) and `errored` when the lane is (it cannot be reached, the container runtime is
//! missing, the merge base itself does not build, the definition is wrong, the job's time ran out). A job that is
//! cancelled meanwhile (a new push) is left as the cancellation made it.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::definition::{self, Definition, LANE_FILE, On, Step};
use super::exec::{
    Ended, Finish, GIT, GIT_ENVIRONMENT, Isolated, Machine, Watch, contained_script, describe,
    direct_script, harness_path, quote, read_status,
};
use super::report::{Measured, Report, Series, StepResult, tail};
use super::rows::{self, Row};
use crate::app::App;
use crate::config::LaneConfig;
use crate::store::LaneJob;

/// What an `errored` job's `failed_step` says when it ran out of time for a change someone else opened: the job is not
/// counted by [`crate::store::Store::lane_infrastructure_streak`].
pub const TIME_MARK: &str = "total time";

/// What it says when the disk of the lane fell under its reserve while the merge base ran, after the head had run in the
/// same job: not counted either.
pub const DISK_MARK: &str = "disk of the lane";

/// How a job ended, before it is written down.
pub(super) enum Outcome {
    Cancelled,
    Ended {
        state: &'static str,
        failed_step: Option<String>,
        summary: String,
        report: Value,
    },
}

/// Why a job stopped before its last step.
enum Stop {
    Cancelled,
    /// The lane is to blame.
    Errored(String),
    /// The lane's, but not a failure that counts towards pausing it when the change is someone else's: the job's total time
    /// ran out while the merge base had used more of it than the head, or the disk of the lane ran low while the merge
    /// base ran after the head had. The second is what `failed_step` is marked with.
    Exempt(String, &'static str),
    /// A step of the change's head failed: the change is to blame.
    Failed {
        step: String,
        why: String,
        output: String,
    },
}

/// The state of one job while it runs.
struct Job<'a> {
    app: &'a App,
    job: &'a LaneJob,
    lane: &'a LaneConfig,
    machine: &'a Machine,
    contained: bool,
    dir: String,
    started: Instant,
    commands: std::cell::Cell<u32>,
    /// Whether the command that is going was given less time than its step allows, since the job's own time was running out.
    cut: std::cell::Cell<bool>,
    /// How long the commands on each side have taken, so that running out of the total time is blamed on the side that
    /// used it: the merge base's slowness is the lane's, the head's is the change's.
    spent_before: std::cell::Cell<Duration>,
    spent_after: std::cell::Cell<Duration>,
    /// The paths of the harness that the merge base has (see [`Job::prepare`]), as the job's copy and the checkouts have them.
    harness: std::cell::RefCell<Vec<String>>,
    report: std::cell::RefCell<Report>,
}

impl App {
    /// Runs the job that was claimed (it is `running`) to its end and writes down what came of it. Blocks for as long as
    /// the job lasts.
    pub fn run_lane_job(&self, id: i64) {
        let Ok(Some(job)) = self.store.lane_job(id) else {
            return;
        };
        if job.state != "running" {
            return;
        }
        match self.execute_lane_job(&job) {
            Outcome::Cancelled => {}
            Outcome::Ended {
                state,
                failed_step,
                summary,
                report,
            } => self.lane_job_ended(&job, state, failed_step.as_deref(), &summary, &report),
        }
    }

    fn execute_lane_job(&self, job: &LaneJob) -> Outcome {
        let errored = |summary: String| Outcome::Ended {
            state: "errored",
            failed_step: None,
            report: json!({ "error": summary }),
            summary,
        };
        let Some(lane) = self.lanes.lane(&job.lane) else {
            return errored(format!("the lane {} is not configured any more", job.lane));
        };
        // The directory of a lane that is the server itself is looked at again before every job: a link may have been made.
        if let Some(why) = crate::config::work_dir_problem(&lane, &self.data_dir, true) {
            return errored(format!("the lane {}: {why}", lane.name));
        }
        let runner = lane
            .runner
            .as_deref()
            .and_then(|name| self.lanes.runner(name));
        let definition = match self.lane_definition_of(job.card_id) {
            Ok(definition) => definition,
            Err(why) => return errored(why),
        };
        let contained = lane.isolation.contained(job.external);
        let machine = Machine::new(&lane, runner.as_ref(), &self.data_dir);
        #[cfg(test)]
        let machine = machine.with_environment(self.lanes.test_environment());
        let running = Job {
            app: self,
            job,
            lane: &lane,
            machine: &machine,
            contained,
            dir: format!("{}/jobs/{}", machine.work_dir(), job.id),
            started: Instant::now(),
            commands: std::cell::Cell::new(0),
            cut: std::cell::Cell::new(false),
            spent_before: std::cell::Cell::new(Duration::ZERO),
            spent_after: std::cell::Cell::new(Duration::ZERO),
            harness: std::cell::RefCell::new(Vec::new()),
            report: std::cell::RefCell::new(Report {
                lane: lane.clone(),
                contained,
                head: job.head.clone(),
                base: job.base.clone(),
                steps: Vec::new(),
                measured: Vec::new(),
            }),
        };
        let result = running.go(&definition);
        running.clean_up();
        let mut report = running.report.borrow().to_json();
        match result {
            Ok(()) => {
                let measured = !running.report.borrow().measured.is_empty();
                let summary = if measured {
                    let rows: usize = running
                        .report
                        .borrow()
                        .measured
                        .iter()
                        .map(|m| m.series.len())
                        .sum();
                    format!(
                        "every step passed and {rows} measured row{} came of it",
                        if rows == 1 { "" } else { "s" }
                    )
                } else {
                    format!("every one of the {} steps passed", definition.steps.len())
                };
                if measured {
                    report["text"] = json!(running.report.borrow().text());
                }
                Outcome::Ended {
                    state: if measured { "measured" } else { "passed" },
                    failed_step: None,
                    summary,
                    report,
                }
            }
            Err(Stop::Cancelled) => Outcome::Cancelled,
            Err(Stop::Errored(summary)) => {
                report["error"] = json!(summary);
                Outcome::Ended {
                    state: "errored",
                    failed_step: None,
                    summary,
                    report,
                }
            }
            // Not counted towards pausing the lane when it is someone else's change that used up the time.
            Err(Stop::Exempt(summary, mark)) => {
                report["error"] = json!(summary);
                Outcome::Ended {
                    state: "errored",
                    failed_step: job.external.then(|| mark.to_string()),
                    summary,
                    report,
                }
            }
            Err(Stop::Failed { step, why, output }) => {
                report["failure"] = json!({ "step": step, "why": why, "output": tail(&output) });
                Outcome::Ended {
                    state: "failed",
                    summary: format!("the step {step} failed: {why}"),
                    failed_step: Some(step),
                    report,
                }
            }
        }
    }

    /// The definition of the project of `card_id`, or why there is none.
    pub(super) fn lane_definition_of(&self, card_id: i64) -> Result<Definition, String> {
        let card = self.card(card_id).map_err(|e| e.to_string())?;
        let project = self.project(card.project_id).map_err(|e| e.to_string())?;
        let text = self.project_file(&project, LANE_FILE).ok_or_else(|| {
            format!(
                "the project {} has no {LANE_FILE} in the harness configuration repository, so no step is known",
                project.name
            )
        })?;
        definition::parse(&text)
    }
}

/// What a command of a step did: how it ended, and what it printed.
struct Done {
    finish: Finish,
    output: String,
    prefixed: Vec<String>,
    /// Whether more lines had the prefix than are read.
    prefixed_cut: bool,
    /// Whether a line had the prefix in the middle or was too long to be read.
    prefixed_ambiguous: bool,
}

/// A shell prefix that stops unless `$f` is a regular file inside the checkout `side`, not reached through a link: the
/// code of a job plants symbolic links, and the host must not follow one out of the checkout.
fn inside(dir: &str, side: &str, path: &str, must_exist: bool) -> String {
    let base = format!("{dir}/{side}");
    let full = format!("{base}/{path}");
    let flag = if must_exist { "-f" } else { "-m" };
    let mut guard = format!(
        "b=$(readlink -f -- {base}) && f=$(readlink {flag} -- {full}) && case \"$f\" in \"$b\"/*) ;; *) exit 9;; esac",
        base = quote(&base),
        full = quote(&full),
    );
    if must_exist {
        guard.push_str(&format!(" && [ -f \"$f\" ] && [ ! -L {} ]", quote(&full)));
    }
    guard
}

impl Job<'_> {
    fn cancelled(&self) -> bool {
        self.app
            .store
            .lane_job(self.job.id)
            .ok()
            .flatten()
            .is_none_or(|job| job.state != "running")
    }

    /// What every command's script watches.
    fn watch(&self) -> Watch<'_> {
        Watch {
            work_dir: self.machine.work_dir(),
            disk_kib: self.lane.disk_bytes() / 1024,
        }
    }

    fn remaining(&self) -> Option<Duration> {
        Duration::from_secs(self.lane.timeout_seconds).checked_sub(self.started.elapsed())
    }

    fn go(&self, definition: &Definition) -> Result<(), Stop> {
        self.prepare(&definition.harness)?;
        for step in &definition.steps {
            let began = Instant::now();
            let result = match &step.measure {
                Some(measure) => self.measuring(step, measure),
                None => self.plain(step),
            };
            let state = match &result {
                Ok(()) => "passed",
                Err(Stop::Failed { .. }) => "failed",
                Err(_) => "skipped",
            };
            self.report.borrow_mut().steps.push(StepResult {
                name: step.name.clone(),
                state,
                seconds: began.elapsed().as_secs(),
            });
            result?;
        }
        Ok(())
    }

    /// The tools on the machine, the commits there and their checkouts. Every git command runs with nothing of the
    /// account's configuration, no hook and no filter; the checkout of the head has the paths of the measuring harness
    /// from the merge base, never from the head.
    fn prepare(&self, harness: &[String]) -> Result<(), Stop> {
        let cancelled = || self.cancelled();
        let mut checks = String::from(
            "for tool in git timeout sha256sum readlink tar find xargs cp df awk ps; do command -v $tool >/dev/null 2>&1 || { echo \"$tool is missing on the machine\"; exit 3; }; done;",
        );
        // A job fills the disk it is given and no more: it does not start where there is less than that free.
        let work = quote(self.machine.work_dir());
        let disk = self.lane.disk_bytes();
        // and keeps a reserve of 5 % of the disk's blocks and inodes besides, which the watcher of a command holds it to.
        checks.push_str(&format!(
            " mkdir -p {work} && set -- $(df -Pk {work} | awk 'NR==2 {{print $2, $4}}') && total=$1 && free=$2 \
             && [ \"${{free:-0}}\" -ge $(({needed} + ${{total:-0}} / 20)) ] \
             && set -- $(df -Pi {work} | awk 'NR==2 {{print $2, $4}}') && [ \"${{2:-0}}\" -ge $((${{1:-0}} / 20)) ] || \
             {{ echo \"the directory {here} of the lane has too little free: $((${{free:-0}} / 1024)) MiB, and a job needs {disk} bytes and a reserve of 5% of the blocks and inodes of the disk\"; exit 6; }};",
            needed = disk.div_ceil(1024),
            here = self.machine.work_dir(),
        ));
        // Fail closed: a job that has to be contained never runs without the runtime.
        if self.contained {
            let runtime = quote(&self.lane.runtime);
            checks.push_str(&format!(
                " command -v {runtime} >/dev/null 2>&1 || {{ echo \"the container runtime {name} is missing on the machine\"; exit 4; }}; \
                 {runtime} --version >/dev/null 2>&1 || {{ echo \"the container runtime {name} does not run on the machine\"; exit 4; }};",
                name = self.lane.runtime
            ));
        }
        let checked = self
            .machine
            .run(&checks, Duration::from_secs(60), &cancelled, None);
        match &checked.ended {
            Ended::Exit(0) => {}
            Ended::Cancelled => return Err(Stop::Cancelled),
            Ended::Exit(6) => {
                return Err(Stop::Errored(format!(
                    "{}; the job was not run",
                    checked.output.trim()
                )));
            }
            Ended::Exit(4) => {
                return Err(Stop::Errored(format!(
                    "{}; the job was not run, since this lane never runs a job that must be isolated without the container runtime",
                    checked.output.trim()
                )));
            }
            _ => {
                return Err(Stop::Errored(format!(
                    "the machine {} is not ready: {}",
                    self.machine.name(),
                    describe(&checked)
                )));
            }
        }
        let bare = format!(
            "{}/repos/repo-{}.git",
            self.machine.work_dir(),
            self.job.repository_id
        );
        let repository = self
            .app
            .repository(self.job.repository_id)
            .map_err(|e| Stop::Errored(e.to_string()))?;
        let (after, before) = (
            format!("refs/lane/{}/after", self.job.id),
            format!("refs/lane/{}/before", self.job.id),
        );
        self.machine
            .push(
                std::path::Path::new(&repository.path),
                &bare,
                &[
                    (&self.job.head, after.clone()),
                    (&self.job.base, before.clone()),
                ],
                &cancelled,
            )
            .map_err(|why| {
                if self.cancelled() {
                    Stop::Cancelled
                } else {
                    Stop::Errored(why)
                }
            })?;
        let dir = quote(&self.dir);
        let mut script = format!("{GIT_ENVIRONMENT} set -e; rm -rf {dir}; mkdir -p {dir}/home;");
        for (side, commit, reference) in [
            ("before", &self.job.base, &before),
            ("after", &self.job.head, &after),
        ] {
            script.push_str(&format!(
                " {GIT} init --template= --quiet {dir}/{side}; {GIT} -C {dir}/{side} fetch --quiet {bare} {reference}; \
                 {GIT} -c advice.detachedHead=false -C {dir}/{side} checkout --quiet --detach {commit}; \
                 [ \"$({GIT} -C {dir}/{side} rev-parse HEAD)\" = {commit} ] || {{ echo \"{side} is not at the pinned commit\"; exit 5; }};",
                bare = quote(&bare),
                reference = quote(reference),
                commit = quote(commit),
            ));
        }
        // The harness of the measurement is the merge base's: it is taken out of the merge base into a directory of the
        // job's own that no step can write to (and it is never made writable), and what the head has there is replaced
        // by it. A container has that directory laid over the checkout, read-only; a direct command has it put back before
        // every measurement. The hash of every file is kept to be checked then.
        let harness_dir = format!("{dir}/harness");
        // Nothing of the head's is followed to anywhere else on this machine: where a directory above a harness path is a
        // link, or is not a directory, the job is refused before anything is removed or copied.
        for path in harness {
            let path = harness_path(path);
            let mut above = String::new();
            for part in path
                .split('/')
                .rev()
                .skip(1)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                if !above.is_empty() {
                    above.push('/');
                }
                above.push_str(part);
                script.push_str(&format!(
                    " if [ -L {dir}/after/{a} ] || {{ [ -e {dir}/after/{a} ] && [ ! -d {dir}/after/{a} ]; }}; then \
                     echo {said}; exit 10; fi;",
                    a = quote(&above),
                    said = quote(&format!(
                        "the head has a link or a file where the directory {above} of the measuring harness is"
                    )),
                ));
            }
        }
        script.push_str(&format!(" mkdir -p {harness_dir};"));
        for path in harness {
            let path = harness_path(path);
            let quoted = quote(path);
            let parent = path
                .rsplit_once('/')
                .map_or(String::new(), |(p, _)| format!("/{p}"));
            script.push_str(&format!(
                " rm -rf {dir}/after/{quoted}; if [ -n \"$({GIT} --git-dir={bare} ls-tree -r --name-only {base} -- {quoted})\" ]; then \
                 {GIT} --git-dir={bare} archive --format=tar --output={dir}/harness.tar {base} -- {quoted}; \
                 tar -x -C {harness_dir} -f {dir}/harness.tar; rm -f {dir}/harness.tar; \
                 mkdir -p {dir}/after{parent}; cp -a {harness_dir}/{quoted} {dir}/after/{quoted}; \
                 printf 'HARNESS %s\\n' {quoted}; fi;",
                bare = quote(&bare),
                base = quote(&self.job.base),
                parent = quote(&parent),
            ));
        }
        script.push_str(&format!(
            " [ -z \"$(find {harness_dir} -type l)\" ] || {{ echo \"the harness of the merge base holds a link\"; exit 5; }}; \
             (cd {harness_dir} && find . -type f -print0 | xargs -0 -r sha256sum > {dir}/harness.sha256);"
        ));
        script.push_str(&format!(
            " {GIT} --git-dir={bare} update-ref -d {after}; {GIT} --git-dir={bare} update-ref -d {before}; \
             chmod -R a+rwX {dir}/before {dir}/after",
            bare = quote(&bare),
            after = quote(&after),
            before = quote(&before),
        ));
        let checked_out = self.machine.run(
            &script,
            Duration::from_secs(900),
            &cancelled,
            Some("HARNESS "),
        );
        match checked_out.ended {
            Ended::Exit(0) => {
                *self.harness.borrow_mut() = checked_out
                    .prefixed
                    .iter()
                    .filter_map(|line| line.strip_prefix("HARNESS "))
                    .map(str::to_string)
                    .collect();
                self.probe_limits()
            }
            Ended::Cancelled => Err(Stop::Cancelled),
            Ended::Exit(10) => Err(Stop::Failed {
                step: "measuring harness".into(),
                why: checked_out.output.trim().to_string(),
                output: String::new(),
            }),
            _ => Err(Stop::Errored(format!(
                "the commits could not be checked out on the machine: {}",
                describe(&checked_out)
            ))),
        }
    }

    /// Before anything of the change runs in a container: a container with the flags every command is given says what its
    /// cgroup limits are. A runtime that does not enforce them (rootless `podman` without delegated cgroup v2
    /// controllers, cgroup v1) takes the flags and ignores them, and a job must not start then.
    fn probe_limits(&self) -> Result<(), Stop> {
        if !self.contained {
            return Ok(());
        }
        let token = self.token();
        let name = format!("moebius-lane-{}-probe", self.job.id);
        let script = contained_script(
            self.lane,
            &Isolated {
                job: self.job.id,
                name: &name,
                job_dir: &self.dir,
                side: "after",
                environment: &[],
                harness: &[],
            },
            "for f in memory.max memory.swap.max pids.max cpu.max; do echo \"LIMIT $f $(cat /sys/fs/cgroup/$f 2>&1 | head -n 1)\"; done",
            120,
            &token,
            None,
        );
        let ran = self.machine.run(
            &script,
            Duration::from_secs(120),
            &|| self.cancelled(),
            None,
        );
        if ran.ended == Ended::Cancelled {
            return Err(Stop::Cancelled);
        }
        let (output, finish) = read_status(&ran.output, &token, 120);
        if finish == Some(Finish::Gone) {
            return Err(Stop::Errored(format!(
                "the container runtime could not start the job's container: {}",
                tail(&output).lines().last().unwrap_or("")
            )));
        }
        if finish != Some(Finish::Exit(0)) {
            return Err(Stop::Errored(format!(
                "the container that reads the limits the runtime enforces did not end well: {}; the job was not run",
                describe(&ran)
            )));
        }
        let limit = |name: &str| {
            output
                .lines()
                .find_map(|line| line.strip_prefix(&format!("LIMIT {name} ")))
                .unwrap_or("")
                .trim()
                .to_string()
        };
        let mut wrong = Vec::new();
        let at_most =
            |text: String, most: u64| text.parse::<u64>().is_ok_and(|n| n > 0 && n <= most);
        if !at_most(limit("memory.max"), self.lane.memory_bytes()) {
            wrong.push(format!(
                "memory.max is {:?}, not at most {}",
                limit("memory.max"),
                self.lane.memory
            ));
        }
        if limit("memory.swap.max") != "0" {
            wrong.push(format!(
                "memory.swap.max is {:?}, not 0",
                limit("memory.swap.max")
            ));
        }
        if !at_most(limit("pids.max"), 4096) {
            wrong.push(format!(
                "pids.max is {:?}, not at most 4096",
                limit("pids.max")
            ));
        }
        if let Some(cpus) = self
            .lane
            .cpus
            .as_deref()
            .and_then(|c| c.trim().parse::<f64>().ok())
        {
            let cpu = limit("cpu.max");
            let ok = cpu.split_once(' ').is_some_and(|(quota, period)| {
                matches!((quota.parse::<f64>(), period.parse::<f64>()),
                    (Ok(q), Ok(p)) if p > 0.0 && (q / p) <= cpus * 1.01)
            });
            if !ok {
                wrong.push(format!("cpu.max is {cpu:?}, not {cpus} processors"));
            }
        }
        if wrong.is_empty() {
            return Ok(());
        }
        Err(Stop::Errored(format!(
            "the container runtime does not enforce the limits of the lane ({}): rootless podman needs the cgroup v2 controllers memory, pids and cpu delegated to the user, and cgroup v1 is not supported; the job was not run",
            wrong.join("; ")
        )))
    }

    /// After a measuring run: every file of the harness is what it was when the merge base's copy was made, in the job's
    /// own copy and in the checkout of `side`. A step that renamed a directory above the harness away and planted another
    /// harness in its place, or wrote into the harness, fails here.
    fn verify_harness(&self, step: &Step, side: &str) -> Result<(), Stop> {
        if self.harness.borrow().is_empty() {
            return Ok(());
        }
        let dir = quote(&self.dir);
        let ran = self.machine.run(
            &format!(
                "(cd {dir}/harness && sha256sum -c --quiet {dir}/harness.sha256 >/dev/null) && \
                 (cd {dir}/{side} && sha256sum -c --quiet {dir}/harness.sha256 >/dev/null)"
            ),
            Duration::from_secs(300),
            &|| self.cancelled(),
            None,
        );
        match ran.ended {
            Ended::Exit(0) => Ok(()),
            Ended::Cancelled => Err(Stop::Cancelled),
            _ if side == "before" => Err(Stop::Errored(format!(
                "the measuring harness changed while the merge base ran it: {}",
                describe(&ran)
            ))),
            _ => Err(Stop::Failed {
                step: Self::labelled(step, side),
                why: "the measuring harness was changed while the step ran: a file of it is not the merge base's".into(),
                output: String::new(),
            }),
        }
    }

    /// A random word only this server knows, which a script says its command's end with.
    fn token(&self) -> String {
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(self.job.id as u64);
        format!("LANE-{:016x}{:016x}", hasher.finish(), {
            let mut again = std::collections::hash_map::RandomState::new().build_hasher();
            again.write_u8(1);
            again.finish()
        })
    }

    /// Stops what the command `n` started and is not sure to have ended, and makes sure of it: the container is
    /// removed (by name, over SSH for a runner) or the process group is killed. A lane whose workload could not be
    /// stopped pauses itself, so that no next job starts on the same GPUs.
    fn stop_command(&self, n: u32) {
        let script = if self.contained {
            let runtime = quote(&self.lane.runtime);
            let name = quote(&format!("moebius-lane-{}-{n}", self.job.id));
            format!(
                "for i in 1 2 3 4 5; do {runtime} rm -f {name} >/dev/null 2>&1; \
                 {runtime} inspect {name} >/dev/null 2>&1 || exit 0; sleep 1; done; exit 1"
            )
        } else {
            let pid = quote(&format!("{}/pid-{n}", self.dir));
            format!(
                "p=$(cat {pid} 2>/dev/null); [ -z \"$p\" ] && exit 0; for i in 1 2 3 4 5; do kill -KILL -$p 2>/dev/null; \
                 kill -0 -$p 2>/dev/null || exit 0; sleep 1; done; exit 1"
            )
        };
        let stopped = self
            .machine
            .run(&script, Duration::from_secs(60), &|| false, None);
        if stopped.ended != Ended::Exit(0) {
            self.app.pause_lane_for(
                &self.job.lane,
                &format!(
                    "a command of job {} could not be stopped on {}, so no job starts there until a person has looked",
                    self.job.id,
                    self.machine.name()
                ),
                Some(self.job.card_id),
            );
        }
    }

    /// The step as a report names it for `side`.
    fn labelled(step: &Step, side: &str) -> String {
        if step.on == On::Both || step.measure.is_some() {
            format!("{} ({side})", step.name)
        } else {
            step.name.clone()
        }
    }

    /// What the job's own time running out at `step` means: the side that used more of it is to blame, and on the
    /// merge base that is the lane's (no reply, no blame on the change), on the head it is the change's, which names the
    /// limit.
    fn out_of_time(&self, step: &Step, output: &str) -> Stop {
        let limit = self.lane.timeout_seconds;
        if self.spent_before.get() > self.spent_after.get() {
            return Stop::Exempt(
                format!(
                    "the job ran out of its total time of {limit} seconds at the step {}",
                    step.name
                ),
                TIME_MARK,
            );
        }
        Stop::Failed {
            step: Self::labelled(step, "after"),
            why: format!("it ran out of the lane's total time of {limit} seconds for a job"),
            output: output.to_string(),
        }
    }

    /// Runs `command` of `step` in the checkout `side`. `run` is the number of the run of a measurement.
    fn command(
        &self,
        step: &Step,
        side: &str,
        run: Option<usize>,
        prefix: Option<&str>,
    ) -> Result<Done, Stop> {
        let (command, seconds) = (step.command.as_str(), step.timeout_seconds);
        let Some(left) = self.remaining() else {
            return Err(self.out_of_time(step, ""));
        };
        self.cut.set(left.as_secs().max(1) < seconds);
        let seconds = seconds.min(left.as_secs().max(1));
        let n = self.commands.get();
        self.commands.set(n + 1);
        let token = self.token();
        let environment: Vec<(String, String)> = [
            ("LANE_SIDE", side.to_string()),
            ("LANE_JOB", self.job.id.to_string()),
            ("LANE_HEAD", self.job.head.clone()),
            ("LANE_BASE", self.job.base.clone()),
            ("LANE_RUN", run.map_or(String::new(), |r| r.to_string())),
            ("CI", "1".into()),
        ]
        .map(|(name, value)| (name.to_string(), value))
        .to_vec();
        let script = if self.contained {
            contained_script(
                self.lane,
                &Isolated {
                    job: self.job.id,
                    name: &format!("moebius-lane-{}-{n}", self.job.id),
                    job_dir: &self.dir,
                    side,
                    environment: &environment,
                    harness: &self.harness.borrow(),
                },
                command,
                seconds,
                &token,
                Some(&self.watch()),
            )
        } else {
            direct_script(
                &self.dir,
                side,
                &environment,
                command,
                seconds,
                &format!("{}/pid-{n}", self.dir),
                &token,
                Some(&self.watch()),
            )
        };
        let began = Instant::now();
        let ran = self.machine.run(
            &script,
            Duration::from_secs(seconds),
            &|| self.cancelled(),
            prefix,
        );
        let spent = if side == "before" {
            &self.spent_before
        } else {
            &self.spent_after
        };
        spent.set(spent.get() + began.elapsed());
        let (output, finish) = read_status(&ran.output, &token, seconds);
        match (&ran.ended, finish) {
            (Ended::Cancelled, _) => {
                self.stop_command(n);
                Err(Stop::Cancelled)
            }
            (Ended::TimedOut, _) => {
                // The server had to stop the script: the workload may still be there.
                self.stop_command(n);
                Ok(Done {
                    finish: Finish::TimedOut,
                    output,
                    prefixed: ran.prefixed,
                    prefixed_cut: ran.prefixed_cut,
                    prefixed_ambiguous: ran.prefixed_ambiguous,
                })
            }
            (Ended::Failed(why), _) => {
                self.stop_command(n);
                Err(Stop::Errored(why.clone()))
            }
            // The script told how the command ended: whatever the number is, it is the command's own.
            (_, Some(finish)) => Ok(Done {
                finish,
                output,
                prefixed: ran.prefixed,
                prefixed_cut: ran.prefixed_cut,
                prefixed_ambiguous: ran.prefixed_ambiguous,
            }),
            (_, None) => {
                self.stop_command(n);
                // A command that runs on the machine itself can end the script that runs it, so that its last line is
                // never written: on the head that is the change's doing.
                if !self.contained && side == "after" {
                    return Err(Stop::Failed {
                        step: Self::labelled(step, side),
                        why: "its command ended the script that runs it, so how it ended is not known".into(),
                        output,
                    });
                }
                Err(Stop::Errored(format!(
                    "the machine did not say how the command ended: {}",
                    describe(&ran)
                )))
            }
        }
    }

    /// What a command's end means for the step `step` on `side`.
    fn judge(&self, step: &Step, side: &str, done: &Done) -> Result<(), Stop> {
        let why = match done.finish {
            Finish::Exit(0) => return Ok(()),
            Finish::Gone => {
                return Err(Stop::Errored(format!(
                    "the container runtime could not start the job's container: {}",
                    tail(&done.output).lines().last().unwrap_or("")
                )));
            }
            // The watcher measures the whole directory of the job, the head's checkout included: what the head left there
            // makes the next command of the merge base the one that trips it, so that is the change's, not the lane's.
            Finish::DiskFull if side == "before" => {
                return Err(Stop::Failed {
                    step: Self::labelled(step, "after"),
                    why: format!(
                        "the change left the job's disk full: the merge base's command was stopped for passing the {} the lane allows a job",
                        self.lane.disk
                    ),
                    output: done.output.clone(),
                });
            }
            Finish::DiskFull => format!(
                "the job wrote more than the {} the lane allows it, or has a directory the lane cannot read in its own",
                self.lane.disk
            ),
            // The disk of the lane fell under its reserve: on the merge base that may be anyone's doing, so it is the lane's.
            // ... and when a command of the head has run in this job already, whoever's it was is not told by the count of
            // errors either, or a head could make the lane pause by what it left on the disk.
            Finish::DiskLaneFull if side == "before" && self.spent_after.get() > Duration::ZERO => {
                return Err(Stop::Exempt(
                    format!(
                        "the disk of the lane fell under its reserve while the merge base ran the step {}, after the head had run in this job",
                        step.name
                    ),
                    DISK_MARK,
                ));
            }
            Finish::DiskLaneFull => {
                "the disk of the lane fell under its reserve of 5% of the blocks or inodes while it ran".to_string()
            }
            // The job's own time was running out: on the merge base that is the lane's, on the head the change's.
            Finish::TimedOut if self.cut.get() => {
                return Err(self.out_of_time(step, &done.output));
            }
            Finish::Exit(code) => format!("it exited with code {code}"),
            Finish::TimedOut => {
                format!("it ran past its limit of {} seconds", step.timeout_seconds)
            }
        };
        if side == "before" {
            // The merge base is not the change's doing.
            return Err(Stop::Errored(format!(
                "the merge base {} itself fails at the step {}: {why}",
                self.job.base.get(..7).unwrap_or(&self.job.base),
                step.name
            )));
        }
        Err(Stop::Failed {
            step: Self::labelled(step, side),
            why,
            output: done.output.clone(),
        })
    }

    /// Writes the step the job is at and says so, so that the card and the settings show it as it goes.
    fn at(&self, text: &str) {
        let _ = self.app.store.set_lane_job_step(self.job.id, text);
        if let Ok(card) = self.app.card(self.job.card_id) {
            self.app.bus.publish(crate::bus::Event::CardUpdated(card));
        }
    }

    /// A step that runs once on each side it names.
    fn plain(&self, step: &Step) -> Result<(), Stop> {
        let sides: &[&str] = match step.on {
            On::Head => &["after"],
            On::Both => &["before", "after"],
        };
        for side in sides {
            self.at(&format!("{} ({side})", step.name));
            let done = self.command(step, side, None, None)?;
            self.judge(step, side, &done)?;
        }
        Ok(())
    }

    /// The text of a regular file of a checkout on the machine, from the start of its first megabyte. A link, a file
    /// outside the checkout or anything that is not a regular file is no file.
    fn read_file(&self, side: &str, path: &str) -> Result<String, String> {
        let ran = self.machine.run(
            &format!(
                "{} && head -c 1048576 -- \"$f\"",
                inside(&self.dir, side, path, true)
            ),
            Duration::from_secs(60),
            &|| self.cancelled(),
            None,
        );
        match ran.ended {
            Ended::Exit(0) => Ok(ran.output),
            _ => Err(format!(
                "it wrote no regular file {path} inside its checkout"
            )),
        }
    }

    /// Removes a file the last run left, never through a link that leads out of the checkout.
    fn remove_file(&self, side: &str, path: &str) {
        let _ = self.machine.run(
            &format!(
                "{} && rm -f -- \"$f\"",
                inside(&self.dir, side, path, false)
            ),
            Duration::from_secs(60),
            &|| self.cancelled(),
            None,
        );
    }

    /// A measuring step: `runs` runs on the merge base and on the head, one after the other by turns.
    fn measuring(&self, step: &Step, measure: &definition::Measure) -> Result<(), Stop> {
        let failed = |why: String, output: &str| Stop::Failed {
            step: step.name.clone(),
            why,
            output: output.to_string(),
        };
        let mut series: BTreeMap<String, (String, Vec<f64>, Vec<f64>)> = BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        let mut hashes: Vec<String> = Vec::new();
        for run in 1..=measure.runs {
            for side in ["before", "after"] {
                self.at(&format!(
                    "{} ({side}, run {run} of {})",
                    step.name, measure.runs
                ));
                for file in [&measure.json, &measure.parity_file].into_iter().flatten() {
                    self.remove_file(side, file);
                }
                self.restore_harness(step, side)?;
                let done = self.command(step, side, Some(run), measure.prefix.as_deref())?;
                self.judge(step, side, &done)?;
                self.verify_harness(step, side)?;
                // Rows are read only from a run that was read whole: lines past the cap are the code's noise, and
                // which of the lines before it were the harness's is not known.
                if done.prefixed_cut {
                    return Err(failed(
                        "it printed more lines that start with the measurement prefix than a run may; the measurement is not trusted".into(),
                        &done.output,
                    ));
                }
                if done.prefixed_ambiguous {
                    return Err(failed(
                        "a line of its output holds the measurement prefix in its middle or is too long to be read, so which rows it gave is not known".into(),
                        &done.output,
                    ));
                }
                let mut found: Vec<Row> = match measure.prefix.as_deref() {
                    Some(prefix) => rows::from_lines(&done.prefixed, prefix)
                        .map_err(|why| failed(why, &done.output))?,
                    None => Vec::new(),
                };
                if let Some(path) = &measure.json {
                    let more = self
                        .read_file(side, path)
                        .and_then(|text| rows::from_json(&text))
                        .map_err(|why| failed(why, &done.output))?;
                    found.extend(more);
                }
                if found.is_empty() {
                    return Err(failed(
                        format!("run {run} on {side} gave no measurement rows"),
                        &done.output,
                    ));
                }
                if found.len() > rows::ROWS_AT_MOST {
                    return Err(failed(
                        format!(
                            "run {run} on {side} gave more than {} rows",
                            rows::ROWS_AT_MOST
                        ),
                        &done.output,
                    ));
                }
                rows::names_are_unique(&found)
                    .map_err(|why| failed(format!("run {run} on {side}: {why}"), &done.output))?;
                for row in found {
                    let entry = series.entry(row.name.clone()).or_insert_with(|| {
                        order.push(row.name.clone());
                        (row.unit.clone(), Vec::new(), Vec::new())
                    });
                    if side == "before" {
                        &mut entry.1
                    } else {
                        &mut entry.2
                    }
                    .push(row.value);
                }
                if let Some(path) = &measure.parity_file {
                    // Hashed by the host once the command, and its container, have ended.
                    let hashed = self.machine.run(
                        &format!(
                            "{} && sha256sum -- \"$f\"",
                            inside(&self.dir, side, path, true)
                        ),
                        Duration::from_secs(120),
                        &|| self.cancelled(),
                        None,
                    );
                    match (&hashed.ended, hashed.output.split_whitespace().next()) {
                        (Ended::Exit(0), Some(hash)) => hashes.push(hash.to_string()),
                        _ => {
                            return Err(failed(
                                format!("it wrote no regular file {path} inside its checkout"),
                                &done.output,
                            ));
                        }
                    }
                }
            }
        }
        for (name, (_, before, after)) in &series {
            if before.len() != measure.runs || after.len() != measure.runs {
                return Err(failed(
                    format!(
                        "the row {name} was given in {} of {} runs before and {} after",
                        before.len(),
                        measure.runs,
                        after.len()
                    ),
                    "",
                ));
            }
        }
        let parity = match &measure.parity_file {
            None => "not checked".to_string(),
            Some(path) if hashes.windows(2).all(|pair| pair[0] == pair[1]) => {
                format!("identical ({path}, every run)")
            }
            Some(path) => format!("differs ({path}: the bytes are not the same in every run)"),
        };
        self.report.borrow_mut().measured.push(Measured {
            step: step.name.clone(),
            runs: measure.runs,
            series: order
                .into_iter()
                .map(|name| {
                    let (unit, before, after) = series.remove(&name).expect("it was entered");
                    Series {
                        name,
                        unit,
                        before,
                        after,
                    }
                })
                .collect(),
            parity,
        });
        Ok(())
    }

    /// Before a measuring run of a command that runs directly on the machine: the harness is put back from the job's
    /// own copy of the merge base's, through the guard that stops at a link leading out of the checkout, and every file
    /// of it is checked against the hash taken when the copy was made. In a container the copy lies over the checkout,
    /// read-only, and there is nothing to put back.
    fn restore_harness(&self, step: &Step, side: &str) -> Result<(), Stop> {
        let paths = self.harness.borrow().clone();
        if self.contained || paths.is_empty() {
            return Ok(());
        }
        let dir = quote(&self.dir);
        let mut script = String::from("set -e;");
        for path in &paths {
            let path = harness_path(path);
            script.push_str(&format!(
                " {guard}; rm -rf -- \"$f\"; mkdir -p -- \"$(dirname -- \"$f\")\"; cp -a -- {dir}/harness/{path} \"$f\";",
                guard = inside(&self.dir, side, path, false),
                path = quote(path),
            ));
        }
        script.push_str(&format!(
            " (cd {dir}/harness && sha256sum -c --quiet {dir}/harness.sha256 >/dev/null); \
             (cd {dir}/{side} && sha256sum -c --quiet {dir}/harness.sha256 >/dev/null)"
        ));
        let ran = self.machine.run(
            &script,
            Duration::from_secs(300),
            &|| self.cancelled(),
            None,
        );
        match ran.ended {
            Ended::Exit(0) => Ok(()),
            Ended::Cancelled => Err(Stop::Cancelled),
            _ if side == "before" => Err(Stop::Errored(format!(
                "the measuring harness could not be put back on the merge base: {}",
                describe(&ran)
            ))),
            _ => Err(Stop::Failed {
                step: Self::labelled(step, side),
                why: format!(
                    "the measuring harness of the merge base could not be put back in the checkout of the head (a link or a changed file): {}",
                    describe(&ran)
                ),
                output: String::new(),
            }),
        }
    }

    /// Takes the job's directory off the machine, whatever came of the job, and what its commands left running first.
    /// A directory that is still there afterwards is an error of the lane, which pauses until a person has looked.
    fn clean_up(&self) {
        let script = format!(
            "{} {}",
            reap_script(self.lane, self.job.id, &self.dir, self.contained),
            remove_script(self.lane, self.job.id, &self.dir, self.contained)
        );
        let ran = self
            .machine
            .run(&script, remove_limit(self.lane), &|| false, None);
        // The machine that cannot be reached is the lane's trouble that the job already counted.
        if !matches!(ran.ended, Ended::Exit(0) | Ended::Failed(_)) {
            self.app.pause_lane_for(
                &self.job.lane,
                &format!(
                    "what job {} left on {} could not be removed ({}), so no job starts there until a person has looked",
                    self.job.id,
                    self.machine.name(),
                    describe(&ran)
                ),
                Some(self.job.card_id),
            );
        }
    }
}

/// How long the removal of a job's directory may take: the time of a container that does it (a tenth of the lane's time
/// for a job, 2 to 30 minutes) for each of the three directories, and a little more.
fn remove_limit(lane: &LaneConfig) -> Duration {
    Duration::from_secs(removal_seconds(lane) * 3 + 120)
}

fn removal_seconds(lane: &LaneConfig) -> u64 {
    (lane.timeout_seconds / 10).clamp(120, 1800)
}

/// The shell commands that remove the directory `dir` of job `id`, and fail when it is still there. What a container's
/// user made may not be the machine user's to remove, and may have no mode to be read at all: a container of the lane's
/// image makes it its own again (`chmod`) and removes everything, a name of two dots and a directory of mode 000
/// included. It is a container like the job's own (named and labelled with the job so that it is found and removed, no
/// network, no capability, a limit on processes and memory) and has a time limit, after which it is removed by name.
pub(super) fn remove_script(lane: &LaneConfig, id: i64, dir: &str, contained: bool) -> String {
    let d = quote(dir);
    let mut script = format!("rm -rf {d} 2>/dev/null;");
    if contained && lane.image.is_some() {
        let runtime = quote(&lane.runtime);
        let memory = quote(lane.memory.trim());
        script.push_str(&format!(" if [ -e {d} ]; then"));
        for side in ["before", "after", "home"] {
            let name = quote(&format!("moebius-lane-{id}-clean-{side}"));
            // (`--log-driver none`: nothing of a cleanup is kept in a log of the runtime.)
            script.push_str(&format!(
                " [ ! -d {d}/{side} ] || {{ timeout -k 10 {limit} {runtime} run --rm --name {name} --label moebius-lane-job={id} \
                 --network none --cap-drop ALL --security-opt no-new-privileges --pids-limit 256 --log-driver none --memory {memory} --memory-swap {memory} \
                 --user {user} -v {d}/{side}:/work:rw {image} \
                 sh -c 'chmod -R u+rwX /work 2>/dev/null; find /work -mindepth 1 -delete' >/dev/null 2>&1; \
                 {runtime} rm -f {name} >/dev/null 2>&1; }};",
                limit = removal_seconds(lane),
                user = quote(&lane.user),
                image = quote(lane.image.as_deref().unwrap_or_default()),
            ));
        }
        script.push_str(&format!(" rm -rf {d}; fi;"));
    }
    script.push_str(&format!(" [ ! -e {d} ]"));
    script
}

/// The shell commands that end whatever commands of job `id` left on the machine: its containers, found by the label
/// they were made with, and the process groups of its direct commands, whose ids are in the `pid-*` files of its
/// directory `dir`. The script exits with 8 when something is still there, and goes on to its next command when not.
pub(super) fn reap_script(lane: &LaneConfig, id: i64, dir: &str, contained: bool) -> String {
    let runtime = quote(&lane.runtime);
    let label = format!("label=moebius-lane-job={id}");
    let dir = quote(dir);
    let containers = if contained {
        format!(
            "{runtime} ps -aq --filter {label} 2>/dev/null | xargs -r {runtime} rm -f >/dev/null 2>&1; \
             [ -z \"$({runtime} ps -aq --filter {label} 2>/dev/null)\" ] || exit 8; "
        )
    } else {
        String::new()
    };
    format!(
        "{containers}for f in {dir}/pid-*; do [ -f \"$f\" ] || continue; p=$(cat \"$f\" 2>/dev/null); \
           case \"$p\" in ''|*[!0-9]*) continue;; esac; [ \"$p\" -gt 1 ] || continue; i=0; \
           while [ $i -lt 5 ]; do kill -KILL -$p 2>/dev/null; \
             ps -A -o pgid= -o stat= 2>/dev/null | awk -v p=$p '$1 == p && $2 !~ /^Z/ {{ found = 1 }} END {{ exit !found }}' || break; \
             i=$((i + 1)); sleep 1; done; \
           ! ps -A -o pgid= -o stat= 2>/dev/null | awk -v p=$p '$1 == p && $2 !~ /^Z/ {{ found = 1 }} END {{ exit !found }}' || exit 8; done;"
    )
}

impl App {
    /// What a stopped server left of the jobs that were running: the containers and processes of each are ended and its
    /// directory is removed, and then the job waits its turn again. A lane where something cannot be ended pauses itself.
    pub fn recover_lane_jobs(&self) {
        let running: Vec<LaneJob> = self
            .store
            .lane_active()
            .unwrap_or_default()
            .into_iter()
            .filter(|job| job.state == "running")
            .collect();
        for job in &running {
            let Some(lane) = self.lanes.lane(&job.lane) else {
                continue;
            };
            let runner = lane
                .runner
                .as_deref()
                .and_then(|name| self.lanes.runner(name));
            let machine = Machine::new(&lane, runner.as_ref(), &self.data_dir);
            #[cfg(test)]
            let machine = machine.with_environment(self.lanes.test_environment());
            let dir = format!("{}/jobs/{}", machine.work_dir(), job.id);
            let contained = lane.isolation.contained(job.external);
            let script = format!(
                "{} {}",
                reap_script(&lane, job.id, &dir, contained),
                remove_script(&lane, job.id, &dir, contained)
            );
            let ran = machine.run(&script, remove_limit(&lane), &|| false, None);
            if ran.ended != Ended::Exit(0) {
                self.pause_lane_for(
                    &job.lane,
                    &format!(
                        "what job {} left running on {} when the server stopped could not be ended ({}), so no job starts there until a person has looked",
                        job.id,
                        machine.name(),
                        describe(&ran)
                    ),
                    Some(job.card_id),
                );
            }
        }
        let _ = self.store.requeue_interrupted_lane_jobs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Isolation;

    fn lane(timeout_seconds: u64) -> LaneConfig {
        LaneConfig {
            name: "gpu1".into(),
            runner: None,
            work_dir: None,
            isolation: Isolation::Always,
            runtime: "podman".into(),
            image: Some("rocm/dev:6.2".into()),
            devices: vec![],
            groups: vec![],
            user: "65534:65534".into(),
            timeout_seconds,
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

    #[test]
    fn the_time_to_remove_a_directory_grows_with_the_time_of_a_job_and_ends_with_a_removal_by_name()
    {
        assert_eq!(removal_seconds(&lane(600)), 120);
        assert_eq!(removal_seconds(&lane(7200)), 720);
        assert_eq!(removal_seconds(&lane(86_400)), 1800);
        assert_eq!(
            remove_limit(&lane(7200)),
            Duration::from_secs(720 * 3 + 120)
        );
        let script = remove_script(&lane(7200), 5, "/jobs/5", true);
        assert!(script.contains("timeout -k 10 720 "), "{script}");
        assert!(
            script.contains("rm -f 'moebius-lane-5-clean-after'"),
            "{script}"
        );
        assert!(script.ends_with("[ ! -e '/jobs/5' ]"), "{script}");
    }
}
