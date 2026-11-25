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
use super::exec::{Ended, Machine, Ran, contained_script, describe, direct_script, quote};
use super::report::{Measured, Report, Series, StepResult, tail};
use super::rows::{self, Row};
use crate::app::App;
use crate::config::LaneConfig;
use crate::store::LaneJob;

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
        let running = Job {
            app: self,
            job,
            lane: &lane,
            machine: &machine,
            contained,
            dir: format!("{}/jobs/{}", machine.work_dir(), job.id),
            started: Instant::now(),
            commands: std::cell::Cell::new(0),
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

impl Job<'_> {
    fn cancelled(&self) -> bool {
        self.app
            .store
            .lane_job(self.job.id)
            .ok()
            .flatten()
            .is_none_or(|job| job.state != "running")
    }

    fn remaining(&self) -> Option<Duration> {
        Duration::from_secs(self.lane.timeout_seconds).checked_sub(self.started.elapsed())
    }

    fn go(&self, definition: &Definition) -> Result<(), Stop> {
        self.prepare()?;
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

    /// The tools on the machine, the commits there and their checkouts.
    fn prepare(&self) -> Result<(), Stop> {
        let cancelled = || self.cancelled();
        let mut checks = String::from(
            "for tool in git timeout sha256sum; do command -v $tool >/dev/null 2>&1 || { echo \"$tool is missing on the machine\"; exit 3; }; done;",
        );
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
        let mut script = format!("set -e; rm -rf {dir}; mkdir -p {dir}/home;");
        for (side, commit, reference) in [
            ("before", &self.job.base, &before),
            ("after", &self.job.head, &after),
        ] {
            script.push_str(&format!(
                " git init --quiet {dir}/{side}; git -C {dir}/{side} fetch --quiet {bare} {reference}; \
                 git -c advice.detachedHead=false -C {dir}/{side} checkout --quiet --detach {commit}; \
                 [ \"$(git -C {dir}/{side} rev-parse HEAD)\" = {commit} ] || {{ echo \"{side} is not at the pinned commit\"; exit 5; }};",
                bare = quote(&bare),
                reference = quote(reference),
                commit = quote(commit),
            ));
        }
        script.push_str(&format!(
            " git --git-dir={bare} update-ref -d {after}; git --git-dir={bare} update-ref -d {before}; chmod -R a+rwX {dir}",
            bare = quote(&bare),
            after = quote(&after),
            before = quote(&before),
        ));
        let checked_out = self
            .machine
            .run(&script, Duration::from_secs(900), &cancelled, None);
        match checked_out.ended {
            Ended::Exit(0) => Ok(()),
            Ended::Cancelled => Err(Stop::Cancelled),
            _ => Err(Stop::Errored(format!(
                "the commits could not be checked out on the machine: {}",
                describe(&checked_out)
            ))),
        }
    }

    /// Runs `command` of a step in the checkout `side`. `run` is the number of the run of a measurement.
    fn command(
        &self,
        side: &str,
        command: &str,
        seconds: u64,
        run: Option<usize>,
        prefix: Option<&str>,
    ) -> Result<Ran, Stop> {
        let Some(left) = self.remaining() else {
            return Err(Stop::Errored(format!(
                "the job took longer than the lane's limit of {} seconds",
                self.lane.timeout_seconds
            )));
        };
        let seconds = seconds.min(left.as_secs().max(1));
        let n = self.commands.get();
        self.commands.set(n + 1);
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
                &format!("moebius-lane-{}-{n}", self.job.id),
                &self.dir,
                side,
                &environment,
                command,
                seconds,
            )
        } else {
            direct_script(&self.dir, side, &environment, command, seconds)
        };
        let ran = self.machine.run(
            &script,
            Duration::from_secs(seconds),
            &|| self.cancelled(),
            prefix,
        );
        if ran.ended == Ended::Cancelled {
            return Err(Stop::Cancelled);
        }
        Ok(ran)
    }

    /// What a command's end means for the step `step` on `side`.
    fn judge(&self, step: &Step, side: &str, ran: &Ran) -> Result<(), Stop> {
        let why = match &ran.ended {
            Ended::Exit(0) => return Ok(()),
            Ended::Exit(125) if self.contained => {
                return Err(Stop::Errored(format!(
                    "the container runtime could not start the job's container: {}",
                    tail(&ran.output).lines().last().unwrap_or("")
                )));
            }
            Ended::Exit(code) => format!("it exited with code {code}"),
            Ended::TimedOut => format!("it ran past its limit of {} seconds", step.timeout_seconds),
            Ended::Failed(why) => return Err(Stop::Errored(why.clone())),
            Ended::Cancelled => return Err(Stop::Cancelled),
        };
        if side == "before" {
            // The merge base is not the change's doing.
            return Err(Stop::Errored(format!(
                "the merge base {} itself fails at the step {}: {why}",
                self.job.base.get(..7).unwrap_or(&self.job.base),
                step.name
            )));
        }
        let name = if step.on == On::Both || step.measure.is_some() {
            format!("{} ({side})", step.name)
        } else {
            step.name.clone()
        };
        Err(Stop::Failed {
            step: name,
            why,
            output: ran.output.clone(),
        })
    }

    fn at(&self, text: &str) {
        let _ = self.app.store.set_lane_job_step(self.job.id, text);
    }

    /// A step that runs once on each side it names.
    fn plain(&self, step: &Step) -> Result<(), Stop> {
        let sides: &[&str] = match step.on {
            On::Head => &["after"],
            On::Both => &["before", "after"],
        };
        for side in sides {
            self.at(&format!("{} ({side})", step.name));
            let ran = self.command(side, &step.command, step.timeout_seconds, None, None)?;
            self.judge(step, side, &ran)?;
        }
        Ok(())
    }

    /// The text of a file of a checkout on the machine, read from the start of its first megabyte.
    fn read_file(&self, side: &str, path: &str) -> Result<String, String> {
        let ran = self.machine.run(
            &format!(
                "head -c 1048576 -- {}/{side}/{}",
                quote(&self.dir),
                quote(path)
            ),
            Duration::from_secs(60),
            &|| self.cancelled(),
            None,
        );
        match ran.ended {
            Ended::Exit(0) => Ok(ran.output),
            _ => Err(format!("it wrote no {path}")),
        }
    }

    fn remove_file(&self, side: &str, path: &str) {
        let _ = self.machine.run(
            &format!("rm -f -- {}/{side}/{}", quote(&self.dir), quote(path)),
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
                let ran = self.command(
                    side,
                    &step.command,
                    step.timeout_seconds,
                    Some(run),
                    measure.prefix.as_deref(),
                )?;
                self.judge(step, side, &ran)?;
                let mut found: Vec<Row> = measure
                    .prefix
                    .as_deref()
                    .map(|prefix| rows::from_lines(&ran.prefixed, prefix))
                    .unwrap_or_default();
                if let Some(path) = &measure.json {
                    let more = self
                        .read_file(side, path)
                        .and_then(|text| rows::from_json(&text))
                        .map_err(|why| failed(why, &ran.output))?;
                    found.extend(more);
                }
                if found.is_empty() {
                    return Err(failed(
                        format!("run {run} on {side} gave no measurement rows"),
                        &ran.output,
                    ));
                }
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
                    let hashed = self.machine.run(
                        &format!("sha256sum -- {}/{side}/{}", quote(&self.dir), quote(path)),
                        Duration::from_secs(120),
                        &|| self.cancelled(),
                        None,
                    );
                    match (&hashed.ended, hashed.output.split_whitespace().next()) {
                        (Ended::Exit(0), Some(hash)) => hashes.push(hash.to_string()),
                        _ => return Err(failed(format!("it wrote no {path}"), &ran.output)),
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

    /// Takes the job's directory off the machine, whatever came of the job.
    fn clean_up(&self) {
        let dir = quote(&self.dir);
        let mut script = format!("rm -rf {dir} 2>/dev/null;");
        if self.contained {
            // Files a container's user made may not be the machine user's to remove.
            script.push_str(&format!(
                " [ ! -d {dir} ] || {{ {runtime} run --rm --network none --user {user} -v {dir}:/work:rw {image} \
                 sh -c 'rm -rf /work/before /work/after /work/home' >/dev/null 2>&1; rm -rf {dir}; }};",
                runtime = quote(&self.lane.runtime),
                user = quote(&self.lane.user),
                image = quote(self.lane.image.as_deref().unwrap_or_default()),
            ));
        }
        let _ = self
            .machine
            .run(&script, Duration::from_secs(120), &|| false, None);
    }
}
