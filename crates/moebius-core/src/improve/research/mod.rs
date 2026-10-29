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
//!
//! This module holds what the API shows of a research; `brief` has the brief from draft to confirmed and stopping it,
//! `measure` running a measure and judging what it printed, and `rounds` the thread that drives a research, its rounds
//! and experiments, and the schedule's start.

mod brief;
mod measure;
mod rounds;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{App, AppError, Result};
use crate::bus::AppEvent;
use crate::store::{Experiment, Repository, Research};

pub use brief::{Brief, NewResearch, confirm, create, edit, stop};
pub use measure::{MEASURE_LIMIT, judge, parse_measurement, percent, spent};
pub use rounds::{on_schedule, start};

/// How much better than the baseline a measurement must be, as a share of the baseline, for the lever to be kept.
const MARGIN: f64 = 0.01;

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

/// The research `id`, or `NotFound`.
fn find(app: &App, id: i64) -> Result<Research> {
    app.store
        .research(id)?
        .ok_or_else(|| AppError::NotFound(format!("research {id}")))
}

/// The repository a research is about, or `NotFound`.
fn repository_of(app: &App, research: &Research) -> Result<Repository> {
    app.store
        .repository(research.repository_id)?
        .ok_or_else(|| AppError::NotFound(format!("repository {}", research.repository_id)))
}

/// The research `id` with its experiments and the tokens it has used.
pub fn view(app: &App, id: i64) -> Result<ResearchView> {
    let research = find(app, id)?;
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
