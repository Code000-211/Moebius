//! Autoresearch (spec 7, ADR 0001): from a brief a person confirms, a baseline is measured, then the Autoresearcher
//! proposes levers and an experiment runs for each; every lever kept becomes its own change.
//!
//! A brief is a repository, a goal, a `measure` command and which way is `better` (`lower` or `higher`), and a token
//! budget. The measure is a shell command that runs in a fresh worktree with the environment cleared (`PATH` only,
//! `HOME` the worktree) and a time limit; its result is the last number it prints on standard output, and an exit
//! status other than 0, a time out or no number means the measurement failed.
//!
//! A research is `draft` until a person confirms it, then `confirmed`. The schedule asks [`on_schedule`] once a minute,
//! and inside the trial window, when no proposal is being tried and no other research is running, the oldest confirmed
//! research starts; `POST /v1/research/{id}/start` starts one at any hour. It is `running` while a thread drives it:
//!
//! 1. The baseline: the measure on the repository's base ref. If it fails the research is `failed`.
//! 2. A round: the built-in agent Autoresearcher (a run with the purpose `research` and no card, in a detached worktree
//!    of the repository) is told the brief, the baseline and every experiment so far, and leaves one branch
//!    `research/<id>-<name>` with one commit for each lever. A round that leaves no new branch ends the research as
//!    `done`.
//! 3. An experiment for each new branch: the measure on the branch, then the repository's own checks. The lever is
//!    kept when the measurement is better than the baseline by more than [`MARGIN`] and the checks pass or there are
//!    none. A kept lever is offered like a chat's change: a change without a card, and a `ready_to_merge` inbox item.
//! 4. Before each round the tokens of the research's runs are compared with the budget; a research that has used it up
//!    is `spent`. A round may run over.
//!
//! A person stops a research with `POST /v1/research/{id}/stop`: it is `stopped` and its running run is stopped. The
//! branches of the levers stay in the repository's clone. A server that starts again sends the `running` research back
//! to `confirmed`; started again, it goes on with the baseline and experiments it has.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::LazyLock;

use crate::app::{App, AppError, Result, tail};
use crate::board::Runs;
use crate::bus::AppEvent;
use crate::config::TimeOfDay;
use crate::decide::{Actor, Question};
use crate::replay::run_apart;
use crate::repos::git;
use crate::retro::wait_for_end;
use crate::store::{Experiment, Repository, Research};
use crate::trial::in_window;

/// How long a measure may run before it is killed.
pub const MEASURE_LIMIT: Duration = Duration::from_secs(10 * 60);

/// How much better than the baseline a measurement must be, as a share of the baseline, for the lever to be kept.
const MARGIN: f64 = 0.01;

/// The most of the end of a measure's output that is looked at for its number.
const OUTPUT_LOOKED_AT: usize = 64 * 1024;

/// A research with what the API shows of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchView {
    #[serde(flatten)]
    pub research: Research,
    /// The tokens its research runs have used so far.
    pub tokens: i64,
    /// The experiments, the first first: one for each lever.
    pub experiments: Vec<Experiment>,
}

/// The body of `POST /v1/research`.
#[derive(Debug, Clone, Deserialize)]
pub struct NewResearch {
    pub repository: i64,
    pub goal: String,
    pub measure: String,
    pub better: String,
    pub token_budget: i64,
}

/// The last number in what a measure printed.
pub fn parse_measurement(output: &str) -> Option<f64> {
    static NUMBER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?").unwrap());
    NUMBER
        .find_iter(output)
        .last()
        .and_then(|found| found.as_str().parse::<f64>().ok())
        .filter(|number| number.is_finite())
}

/// How much `value` differs from `baseline`, in percent of the baseline; none when the baseline is 0.
pub fn percent(baseline: f64, value: f64) -> Option<f64> {
    (baseline != 0.0).then(|| (value - baseline) / baseline.abs() * 100.0)
}

/// Whether a lever is kept, and why or why not: its measurement is better than the baseline in the brief's direction
/// by more than [`MARGIN`] of the baseline, and the checks passed or there are none.
pub fn judge(baseline: f64, value: f64, better: &str, checks: Option<bool>) -> (bool, String) {
    let gain = if better == "lower" {
        baseline - value
    } else {
        value - baseline
    };
    if gain <= MARGIN * baseline.abs() {
        let why = format!(
            "{value} against the baseline {baseline}: not better by more than {} %",
            MARGIN * 100.0
        );
        return (false, why);
    }
    let change = percent(baseline, value).map_or(String::new(), |p| format!(" ({p:+.1} %)"));
    if checks == Some(false) {
        let why = format!(
            "{value} against the baseline {baseline}{change} is better, but the repository's checks fail"
        );
        return (false, why);
    }
    (
        true,
        format!("{baseline} to {value}{change}, better than the baseline"),
    )
}

/// Whether the tokens used have reached the budget.
pub fn spent(tokens: i64, budget: i64) -> bool {
    tokens >= budget
}

/// The research `id` with its experiments and the tokens it has used.
pub fn view(app: &App, id: i64) -> Result<ResearchView> {
    let research = app
        .store
        .research(id)?
        .ok_or_else(|| AppError::NotFound(format!("research {id}")))?;
    Ok(ResearchView {
        tokens: app.store.research_tokens(id)?,
        experiments: app.store.experiments(id)?,
        research,
    })
}

/// All research, the newest first.
pub fn views(app: &App) -> Result<Vec<ResearchView>> {
    app.store
        .researches()?
        .iter()
        .map(|research| view(app, research.id))
        .collect()
}

fn publish_status(app: &App, research: i64, status: &str) {
    app.bus.publish(AppEvent::new(
        "research_status",
        json!({ "research": research, "status": status }),
    ));
}

/// The body of `PUT /v1/research/{id}`: the brief of a draft, written anew.
#[derive(Debug, Clone, Deserialize)]
pub struct Brief {
    pub goal: String,
    pub measure: String,
    pub better: String,
    pub token_budget: i64,
    /// Why the brief changes, kept with the decision.
    #[serde(default)]
    pub reason: String,
}

/// Refuses a brief without a goal or a measure, with a direction other than `lower` or `higher`, or with a budget
/// that is not more than 0.
fn check_brief(goal: &str, measure: &str, better: &str, token_budget: i64) -> Result<()> {
    let invalid = |why: &str| Err(AppError::Invalid(why.into()));
    if goal.trim().is_empty() {
        return invalid("a research needs a goal");
    }
    if measure.trim().is_empty() {
        return invalid("a research needs a measure command");
    }
    if !matches!(better, "lower" | "higher") {
        return invalid("better is lower or higher");
    }
    if token_budget <= 0 {
        return invalid("the token budget must be more than 0");
    }
    Ok(())
}

/// Stores a brief as a draft.
pub fn create(app: &App, new: NewResearch) -> Result<ResearchView> {
    check_brief(&new.goal, &new.measure, &new.better, new.token_budget)?;
    app.store
        .repository(new.repository)?
        .ok_or_else(|| AppError::NotFound(format!("repository {}", new.repository)))?;
    let research = app.store.add_research(
        new.repository,
        &new.goal,
        &new.measure,
        &new.better,
        new.token_budget,
    )?;
    publish_status(app, research.id, &research.status);
    view(app, research.id)
}

/// A person, or the Moebius agent, writes the brief of a draft anew: checked as a new brief is, recorded as an
/// `edit_research` decision and announced as `research_status`. A brief that changes nothing does neither.
pub fn edit(app: &App, by: &Actor, id: i64, brief: &Brief) -> Result<ResearchView> {
    let research = app
        .store
        .research(id)?
        .ok_or_else(|| AppError::NotFound(format!("research {id}")))?;
    if research.status != "draft" {
        return Err(AppError::Conflict(format!(
            "research {id} is {}, not a draft",
            research.status
        )));
    }
    let (goal, measure) = (brief.goal.trim(), brief.measure.trim());
    check_brief(goal, measure, &brief.better, brief.token_budget)?;
    let unchanged = (goal, measure, brief.better.as_str(), brief.token_budget)
        == (
            research.goal.as_str(),
            research.measure.as_str(),
            research.better.as_str(),
            research.token_budget,
        );
    if unchanged {
        return view(app, id);
    }
    if !app
        .store
        .set_research_brief(id, goal, measure, &brief.better, brief.token_budget)?
    {
        return Err(AppError::Conflict(format!(
            "research {id} is not a draft any more"
        )));
    }
    let repository = app
        .store
        .repository(research.repository_id)?
        .map(|r| r.name)
        .unwrap_or_default();
    app.decisions.record_by(
        by,
        &Question {
            kind: "edit_research".into(),
            context: [
                ("repository", repository),
                ("goal", goal.to_string()),
                ("measure", measure.to_string()),
                ("better", brief.better.clone()),
                ("token_budget", brief.token_budget.to_string()),
                ("previous_goal", research.goal),
                ("previous_measure", research.measure),
                ("previous_better", research.better),
                ("previous_token_budget", research.token_budget.to_string()),
            ]
            .map(|(key, value)| (key.to_string(), value))
            .into(),
            options: vec!["edit_research".into(), "leave".into()],
            fail_closed: "leave".into(),
            run_id: None,
            card_id: None,
        },
        "edit_research",
        &brief.reason,
    )?;
    publish_status(app, id, "draft");
    view(app, id)
}

/// A person confirms the brief of a draft: the decision is recorded and the research waits for its time.
pub fn confirm(app: &App, by: &Actor, id: i64) -> Result<ResearchView> {
    let research = app
        .store
        .research(id)?
        .ok_or_else(|| AppError::NotFound(format!("research {id}")))?;
    let repository = app
        .store
        .repository(research.repository_id)?
        .ok_or_else(|| AppError::NotFound(format!("repository {}", research.repository_id)))?;
    if research.status != "draft" {
        return Err(AppError::Conflict(format!(
            "research {id} is {}, not a draft",
            research.status
        )));
    }
    app.decisions.record_by(
        by,
        &Question {
            kind: "confirm_brief".into(),
            context: [
                ("repository", repository.name),
                ("goal", research.goal),
                ("measure", research.measure),
                ("better", research.better),
                ("token_budget", research.token_budget.to_string()),
            ]
            .map(|(key, value)| (key.to_string(), value))
            .into(),
            options: vec!["confirm".into(), "reject".into()],
            fail_closed: "reject".into(),
            run_id: None,
            card_id: None,
        },
        "confirm",
        "",
    )?;
    if !app.store.move_research(id, &["draft"], "confirmed", "")? {
        return Err(AppError::Conflict(format!(
            "research {id} is not a draft any more"
        )));
    }
    publish_status(app, id, "confirmed");
    view(app, id)
}

/// A person stops a research that has not ended; a research run that is going is stopped like a run is.
pub fn stop(app: &App, runs: &dyn Runs, id: i64) -> Result<ResearchView> {
    let research = app
        .store
        .research(id)?
        .ok_or_else(|| AppError::NotFound(format!("research {id}")))?;
    let moved = app.store.move_research(
        id,
        &["draft", "confirmed", "running"],
        "stopped",
        "a person stopped it",
    )?;
    if !moved {
        return Err(AppError::Conflict(format!(
            "research {id} is {}, which has ended",
            research.status
        )));
    }
    publish_status(app, id, "stopped");
    for run in app.store.runs_of_research(id)? {
        if run.ended_at.is_none() {
            runs.stop(run.id);
        }
    }
    view(app, id)
}

fn is_running(app: &App, id: i64) -> bool {
    app.store
        .research(id)
        .ok()
        .flatten()
        .is_some_and(|research| research.status == "running")
}

/// Runs the measure `command` in a fresh worktree of `repository` at `rev`, named `name`, and returns the number it
/// printed or why the measurement failed.
fn measure(
    app: &App,
    repository: &Repository,
    name: &str,
    rev: &str,
    command: &str,
    limit: Duration,
) -> std::result::Result<f64, String> {
    let clone = Path::new(&repository.path);
    let worktree = app
        .repos
        .add_detached_worktree(&repository.name, clone, name, rev)
        .map_err(|e| format!("cannot make a worktree of {rev}: {e}"))?;
    let (out, err) = (
        app.data_dir.join(format!("{name}.out")),
        app.data_dir.join(format!("{name}.err")),
    );
    let ended = std::fs::File::create(&out)
        .and_then(|stdout| Ok((stdout, std::fs::File::create(&err)?)))
        .and_then(|files| run_apart("sh", &["-c", command], &worktree, files, limit));
    let printed = std::fs::read(&out).unwrap_or_default();
    let printed = &printed[printed.len().saturating_sub(OUTPUT_LOOKED_AT)..];
    let printed = String::from_utf8_lossy(printed).into_owned();
    let complaint = tail(&err);
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&err);
    let _ = app.repos.remove_worktree(clone, &worktree);
    match ended {
        Err(e) => Err(format!("cannot run the measure: {e}")),
        Ok((_, true)) => Err(format!(
            "the measure was stopped after {} seconds",
            limit.as_secs()
        )),
        Ok((Some(0), false)) => {
            parse_measurement(&printed).ok_or_else(|| "the measure printed no number".to_string())
        }
        Ok((code, false)) => {
            let code = code.map_or("a signal".to_string(), |code| format!("exit status {code}"));
            Err(format!(
                "the measure ended with {code}: {}",
                complaint.trim()
            ))
        }
    }
}

/// The lever branches of research `id` in the clone, by name.
fn lever_branches(repository: &Repository, id: i64) -> Result<Vec<String>> {
    let pattern = format!("refs/heads/research/{id}-*");
    let listed = git(
        Path::new(&repository.path),
        &["for-each-ref", "--format=%(refname:short)", &pattern],
    )?;
    Ok(listed.lines().map(str::to_string).collect())
}

/// What the Autoresearcher is told in a round.
fn prompt(research: &Research, baseline: f64, experiments: &[Experiment]) -> String {
    let done = if experiments.is_empty() {
        "none yet".to_string()
    } else {
        experiments
            .iter()
            .map(|e| {
                let value = e
                    .value
                    .map_or("no measurement".to_string(), |v| v.to_string());
                let kept = if e.kept { "kept" } else { "not kept" };
                format!(
                    "- {} \"{}\": {value}, {kept}: {}",
                    e.branch, e.title, e.reason
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "Research {id}. The goal: {goal}\n\n\
         Moebius measures it with `{measure}` in a worktree, and takes the last number it prints; a {better} number is \
         better. The repository as it is measures {baseline}.\n\n\
         Propose levers that move the measurement the right way. Make each lever its own branch named \
         research/{id}-<short-name> from the current commit, with one commit whose message says what the lever is. \
         Commit with `git -c user.name=Autoresearcher -c user.email=autoresearcher@localhost commit`. Moebius \
         measures every branch and keeps those that are better than the baseline by more than {margin} %.\n\n\
         The experiments so far:\n{done}\n",
        id = research.id,
        goal = research.goal,
        measure = research.measure,
        better = research.better,
        margin = MARGIN * 100.0,
    )
}

/// Measures the lever on `branch` and keeps it, offering it as a change, or not; stores and publishes the experiment.
fn try_lever(
    app: &App,
    repository: &Repository,
    research: &Research,
    baseline: f64,
    branch: &str,
    limit: Duration,
) -> Result<()> {
    let clone = Path::new(&repository.path);
    let title = git(clone, &["log", "-1", "--format=%s", branch])?;
    let short = branch.rsplit('/').next().unwrap_or(branch);
    let name = format!("research-{}-{short}", research.id);
    let measured = measure(app, repository, &name, branch, &research.measure, limit);
    let (value, checks, kept, mut reason) = match measured {
        Err(why) => (None, None, false, format!("the measurement failed: {why}")),
        Ok(value) => {
            let checks = app.local_checks(repository, &format!("{name}-checks"), branch);
            let (kept, reason) = judge(baseline, value, &research.better, checks);
            (Some(value), checks, kept, reason)
        }
    };
    let mut change_id = None;
    if kept {
        let body = format!(
            "This change came from research {}: {}\n\nThe measurement went from {baseline} to {}.\n\nMade by Moebius.",
            research.id,
            research.goal,
            value.unwrap_or_default()
        );
        match app.offer_branch_change(repository, branch, &title, &body) {
            Ok(change) => change_id = Some(change.id),
            Err(e) => reason.push_str(&format!("; it could not be offered as a change: {e}")),
        }
    }
    app.store.add_experiment(&Experiment {
        id: 0,
        research_id: research.id,
        branch: branch.to_string(),
        title,
        value,
        checks,
        kept,
        reason,
        change_id,
        at: 0,
    })?;
    app.bus.publish(AppEvent::new(
        "experiment_done",
        json!({ "research": research.id, "lever": branch, "value": value, "kept": kept }),
    ));
    Ok(())
}

/// How a driven research ended: its status and why. None when a person stopped it.
type Ended = Option<(&'static str, String)>;

/// Measures the baseline if need be, then runs rounds until the research ends.
fn drive(app: &Arc<App>, runs: &Arc<dyn Runs>, id: i64, limit: Duration) -> Result<Ended> {
    let research = app
        .store
        .research(id)?
        .ok_or_else(|| AppError::NotFound(format!("research {id}")))?;
    let repository = app
        .store
        .repository(research.repository_id)?
        .ok_or_else(|| AppError::NotFound(format!("repository {}", research.repository_id)))?;
    let base = repository.base_ref();
    let baseline = match research.baseline {
        Some(baseline) => baseline,
        None => {
            let name = format!("research-{id}-baseline");
            match measure(app, &repository, &name, &base, &research.measure, limit) {
                Ok(baseline) => {
                    app.store.set_baseline(id, baseline)?;
                    baseline
                }
                Err(why) => {
                    return Ok(Some(("failed", format!("the baseline failed: {why}"))));
                }
            }
        }
    };
    loop {
        if !is_running(app, id) {
            return Ok(None);
        }
        let tokens = app.store.research_tokens(id)?;
        if spent(tokens, research.token_budget) {
            let reason = format!(
                "its runs used {tokens} tokens of the budget of {}",
                research.token_budget
            );
            return Ok(Some(("spent", reason)));
        }
        let experiments = app.store.experiments(id)?;
        let mut ended = app.bus.subscribe();
        let run = match runs.start_research(
            app,
            &repository,
            &base,
            &prompt(&research, baseline, &experiments),
        ) {
            Ok(run) => run,
            Err(why) => {
                let why = format!("the Autoresearcher cannot start: {why}");
                return Ok(Some(("failed", why)));
            }
        };
        app.store.add_research_run(id, run.id)?;
        if !is_running(app, id) {
            // Stopped while the run started, before it could be found to stop.
            runs.stop(run.id);
        }
        wait_for_end(app, &mut ended, run.id);
        if !is_running(app, id) {
            return Ok(None);
        }
        let run = app.store.run(run.id)?;
        if let Some(run) = run.filter(|run| run.status != "succeeded") {
            let why = format!("the Autoresearcher's run {} {}", run.id, run.status);
            return Ok(Some(("failed", why)));
        }
        let tried: Vec<String> = experiments.into_iter().map(|e| e.branch).collect();
        let mut levers = Vec::new();
        for branch in lever_branches(&repository, id)? {
            let beyond = format!("{base}..{branch}");
            let commits = git(
                Path::new(&repository.path),
                &["rev-list", "--count", &beyond],
            )?;
            if !tried.contains(&branch) && commits != "0" {
                levers.push(branch);
            }
        }
        if levers.is_empty() {
            return Ok(Some(("done", "a round proposed no more levers".into())));
        }
        for branch in levers {
            if !is_running(app, id) {
                return Ok(None);
            }
            try_lever(app, &repository, &research, baseline, &branch, limit)?;
        }
    }
}

/// Drives the research to its end and gives it its status, unless a person stopped it meanwhile.
fn run(app: &Arc<App>, runs: &Arc<dyn Runs>, id: i64, limit: Duration) {
    let ended = drive(app, runs, id, limit)
        .unwrap_or_else(|e| Some(("failed", format!("the research could not go on: {e}"))));
    if let Some((status, reason)) = ended
        && app
            .store
            .move_research(id, &["running"], status, &reason)
            .unwrap_or(false)
    {
        publish_status(app, id, status);
    }
}

/// Starts the confirmed research `id` on a thread of its own and returns it, `running`. Only one runs at a time.
pub fn start(
    app: &Arc<App>,
    runs: &Arc<dyn Runs>,
    id: i64,
    limit: Duration,
) -> Result<ResearchView> {
    let research = app
        .store
        .research(id)?
        .ok_or_else(|| AppError::NotFound(format!("research {id}")))?;
    if !app.store.start_research(id)? {
        return Err(AppError::Conflict(if research.status == "confirmed" {
            "another research is running".to_string()
        } else {
            format!("research {id} is {}, not confirmed", research.status)
        }));
    }
    publish_status(app, id, "running");
    let (thread_app, thread_runs) = (app.clone(), runs.clone());
    let started = std::thread::Builder::new()
        .name(format!("research {id}"))
        .spawn(move || run(&thread_app, &thread_runs, id, limit));
    if let Err(e) = started {
        app.store.move_research(id, &["running"], "confirmed", "")?;
        publish_status(app, id, "confirmed");
        return Err(AppError::Invalid(format!("cannot start the research: {e}")));
    }
    view(app, id)
}

/// The schedule's check, once a minute: inside the trial window, when no proposal is being tried and no research is
/// running, starts the oldest confirmed research.
pub fn on_schedule(
    app: &Arc<App>,
    runs: &Arc<dyn Runs>,
    window: (TimeOfDay, TimeOfDay),
    limit: Duration,
) -> Result<()> {
    if !in_window(TimeOfDay(app.store.minutes_into_the_day()?), window) {
        return Ok(());
    }
    if app
        .store
        .proposals(None)?
        .iter()
        .any(|p| p.status == "trying")
    {
        return Ok(());
    }
    let all = app.store.researches()?;
    if all.iter().any(|r| r.status == "running") {
        return Ok(());
    }
    // The newest come first.
    match all.iter().rev().find(|r| r.status == "confirmed") {
        Some(oldest) => start(app, runs, oldest.id, limit).map(drop),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use moebius_plugin_api::Host;

    /// An app with a repository `demo` whose `score` file holds 100.
    fn app_with_repository() -> (tempfile::TempDir, App, Repository) {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("score"), "100\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        let repository = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        (dir, app, repository)
    }

    #[test]
    fn a_measure_runs_in_a_worktree_with_a_clean_environment_and_a_time_limit() {
        let (_dir, app, repository) = app_with_repository();
        let run = |command: &str, limit: Duration| {
            measure(&app, &repository, "m", "main", command, limit)
        };
        let limit = Duration::from_secs(30);
        assert_eq!(run("cat score", limit), Ok(100.0));
        // Only what was printed on standard output counts, and only the last number.
        assert_eq!(run("echo 7 >&2; echo 1; echo 2.5", limit), Ok(2.5));
        // The environment holds PATH and HOME, and the command runs where the files of the branch are.
        let variables = "env | cut -d= -f1 | grep -vx -e PWD -e SHLVL -e _ | wc -l";
        assert_eq!(run(variables, limit), Ok(2.0));
        let failed = run("echo 1; echo broke >&2; exit 4", limit).unwrap_err();
        assert!(
            failed.contains("exit status 4") && failed.contains("broke"),
            "{failed}"
        );
        assert_eq!(
            run("echo nothing", limit).unwrap_err(),
            "the measure printed no number"
        );
        let slow = run("echo 1; sleep 30", Duration::from_secs(1)).unwrap_err();
        assert!(slow.contains("stopped after 1 seconds"), "{slow}");
        // The worktrees are gone.
        let worktrees = git(Path::new(&repository.path), &["worktree", "list"]).unwrap();
        assert_eq!(worktrees.lines().count(), 1, "{worktrees}");
    }

    #[test]
    fn a_measurement_is_the_last_number_printed() {
        assert_eq!(parse_measurement("80\n"), Some(80.0));
        assert_eq!(parse_measurement("took 12 ms\nscore: 80.5\n"), Some(80.5));
        assert_eq!(parse_measurement("delta -3.25"), Some(-3.25));
        assert_eq!(parse_measurement("1 2 3"), Some(3.0));
        assert_eq!(parse_measurement("x = .5"), Some(0.5));
        assert_eq!(parse_measurement("2.5e-3 s"), Some(0.0025));
        assert_eq!(parse_measurement("+7"), Some(7.0));
        assert_eq!(parse_measurement("12 then done"), Some(12.0));
    }

    #[test]
    fn output_without_a_number_has_no_measurement() {
        assert_eq!(parse_measurement(""), None);
        assert_eq!(parse_measurement("all good\n"), None);
        assert_eq!(parse_measurement("- . e"), None);
    }

    #[test]
    fn a_lower_measurement_is_kept_when_it_beats_the_baseline_by_more_than_the_margin() {
        assert!(judge(100.0, 80.0, "lower", None).0);
        assert!(judge(100.0, 98.9, "lower", None).0);
        // Exactly the margin is not more than it; worse is not better.
        assert!(!judge(100.0, 99.0, "lower", None).0);
        assert!(!judge(100.0, 100.0, "lower", None).0);
        assert!(!judge(100.0, 120.0, "lower", None).0);
    }

    #[test]
    fn a_higher_measurement_is_kept_when_it_beats_the_baseline_by_more_than_the_margin() {
        assert!(judge(100.0, 120.0, "higher", None).0);
        assert!(judge(100.0, 101.1, "higher", None).0);
        assert!(!judge(100.0, 101.0, "higher", None).0);
        assert!(!judge(100.0, 80.0, "higher", None).0);
        // The margin is of the size of the baseline, whatever its sign; from 0 any gain counts.
        assert!(judge(-100.0, -80.0, "higher", None).0);
        assert!(!judge(-100.0, -99.5, "higher", None).0);
        assert!(judge(0.0, 0.1, "higher", None).0);
        assert!(!judge(0.0, 0.0, "higher", None).0);
    }

    #[test]
    fn a_lever_whose_checks_fail_is_not_kept_however_good_it_measures() {
        assert!(judge(100.0, 80.0, "lower", Some(true)).0);
        let (kept, why) = judge(100.0, 80.0, "lower", Some(false));
        assert!(!kept);
        assert!(why.contains("checks fail"), "{why}");
    }

    #[test]
    fn the_budget_is_spent_when_the_tokens_reach_it() {
        assert!(!spent(0, 1000));
        assert!(!spent(999, 1000));
        assert!(spent(1000, 1000));
        assert!(spent(5000, 1000));
    }

    #[test]
    fn the_change_is_shown_in_percent_of_the_baseline() {
        assert_eq!(percent(100.0, 80.0), Some(-20.0));
        assert_eq!(percent(-50.0, -25.0), Some(50.0));
        assert_eq!(percent(0.0, 1.0), None);
    }
}
