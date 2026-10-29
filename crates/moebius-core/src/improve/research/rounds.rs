//! Driving a research: the thread that measures its baseline and runs its rounds, the Autoresearcher told the brief
//! and the experiments so far, each new lever branch measured and kept or not, and the schedule's start of the oldest
//! confirmed research inside the trial window.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::measure::{judge, measure, spent};
use super::{MARGIN, ResearchView, find, publish_status, repository_of, view};
use crate::app::{App, AppError, Result};
use crate::board::Runs;
use crate::bus::AppEvent;
use crate::config::TimeOfDay;
use crate::improve::retro::wait_for_end;
use crate::improve::trial::in_window;
use crate::repos::git;
use crate::store::{Experiment, Repository, Research};

/// Whether research `id` is still running: a person may have stopped it.
fn is_running(app: &App, id: i64) -> bool {
    app.store
        .research(id)
        .ok()
        .flatten()
        .is_some_and(|research| research.status == "running")
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
    let research = find(app, id)?;
    let repository = repository_of(app, &research)?;
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
    let research = find(app, id)?;
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
