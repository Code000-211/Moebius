//! One card's trial: the card's task run again by its agent with the agent's definition from the proposal's branch,
//! in a worktree of its own, then measured (its errors, the repository's checks, its tokens) and compared with the
//! card's first code run. The trial's worktree and branch are removed afterwards.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use serde_json::json;

use super::verdict::{Measured, Original, compare};
use crate::app::{App, AppError, Result};
use crate::board::Runs;
use crate::harness::repo::HarnessRepo;
use crate::improve::retro::{Report, wait_for_end};
use crate::repos::trial_name;
use crate::store::{Found, Proposal, Repository, Store, StoreError};

/// The agent that tried a card's task when the report does not say who made its error.
const DEFAULT_AGENT: &str = "Coder";

/// One card's trial, before it is stored.
pub(super) struct Tried {
    pub run: Option<i64>,
    pub outcome: &'static str,
    pub reason: String,
    pub measurements: serde_json::Value,
}

impl Tried {
    fn new(run: Option<i64>, outcome: &'static str, reason: String) -> Self {
        Self {
            run,
            outcome,
            reason,
            measurements: json!({}),
        }
    }

    /// Nothing was tried: there was nothing to try it on.
    pub fn skipped(reason: String) -> Self {
        Self::new(None, "skipped", reason)
    }

    /// Nothing could be tried: something went wrong.
    fn failed(run: Option<i64>, reason: impl ToString) -> Self {
        Self::new(run, "failed", reason.to_string())
    }
}

/// The errors of one run, from the sources the Retro's report reads.
fn run_errors(store: &Store, run: i64) -> Result<Vec<Found>> {
    let mut found = store.failed_tool_calls(0, i64::MAX, Some(run))?;
    found.extend(store.refused_tool_calls_between(0, i64::MAX, Some(run))?);
    found.extend(store.failed_runs(0, i64::MAX, Some(run))?);
    found.extend(store.stopped_runs(0, i64::MAX, Some(run))?);
    found.extend(store.steering(0, i64::MAX, Some(run))?);
    Ok(found)
}

/// The tokens a run used.
fn run_tokens(store: &Store, run: i64) -> Result<i64> {
    Ok(store.usage_for_run(run)?.iter().map(|u| u.total()).sum())
}

/// Tries the proposal on one card.
pub(super) fn try_card(
    app: &Arc<App>,
    runs: &Arc<dyn Runs>,
    repo: &HarnessRepo,
    proposal: &Proposal,
    card_id: i64,
) -> std::result::Result<Tried, Tried> {
    let store = &app.store;
    let broken = |e: AppError| Tried::failed(None, e);
    let card = store
        .card(card_id)
        .map_err(|e| broken(e.into()))?
        .ok_or_else(|| Tried::skipped(format!("card {card_id} does not exist")))?;
    let retro = store
        .retro(&proposal.retro_date)
        .map_err(|e| broken(e.into()))?
        .ok_or_else(|| Tried::skipped(format!("the Retro of {} is gone", proposal.retro_date)))?;
    let report: Report =
        serde_json::from_value(retro.report).map_err(|e| broken(StoreError::from(e).into()))?;
    let errors: Vec<_> = report
        .errors
        .iter()
        .filter(|e| e.card == Some(card_id))
        .collect();
    if errors.is_empty() {
        return Err(Tried::skipped(format!(
            "the report of {} has no error of card {card_id}",
            report.date
        )));
    }
    let kinds: BTreeSet<String> = errors.iter().map(|e| e.kind.clone()).collect();
    let agent = errors
        .iter()
        .find_map(|e| e.agent.clone())
        .unwrap_or_else(|| DEFAULT_AGENT.to_string());

    let code_runs: Vec<_> = store
        .runs_for_card(card_id)
        .map_err(|e| broken(e.into()))?
        .into_iter()
        .filter(|r| r.purpose == "code")
        .collect();
    let first = code_runs.first().ok_or_else(|| {
        Tried::skipped(format!(
            "card {card_id} has no code run to compare a trial with"
        ))
    })?;
    let original = Original {
        errors: kinds.iter().cloned().collect(),
        checks_failed: store
            .moves_for_card(card_id)
            .map_err(|e| broken(e.into()))?
            .iter()
            .any(|m| {
                m.note
                    .as_deref()
                    .is_some_and(|n| n.starts_with("The checks failed"))
            }),
        rounds: code_runs.len(),
        tokens: run_tokens(store, first.id).map_err(broken)?,
    };
    let repository = store
        .repository(card.repository_id)
        .map_err(|e| broken(e.into()))?
        .ok_or_else(|| Tried::skipped("the card's repository is gone".into()))?;

    let name = trial_name(proposal.id, card_id);
    let (definition, root) = repo
        .agent_on_branch(app, &proposal.branch, &name, &agent)
        .map_err(|e| {
            Tried::failed(
                None,
                format!(
                    "the agent {agent} cannot be used on {}: {e}",
                    proposal.branch
                ),
            )
        })?;
    let mut ended = app.bus.subscribe();
    let started = runs.start_trial(
        app,
        &card,
        &agent,
        definition,
        proposal.id,
        &repository.base_ref(),
    );
    let tried = match started {
        Err(e) => Err(Tried::failed(
            None,
            format!("cannot start the trial run: {e}"),
        )),
        Ok(run) => {
            wait_for_end(app, &mut ended, run.id);
            measure(app, &repository, run.id, &name, &kinds, &original)
        }
    };
    repo.remove_worktree(app, &root);
    let _ = app
        .repos
        .remove_trial_branch(Path::new(&repository.path), proposal.id, card_id);
    tried
}

/// The measurements of the ended trial run `run_id`, compared with the original.
fn measure(
    app: &App,
    repository: &Repository,
    run_id: i64,
    name: &str,
    kinds: &BTreeSet<String>,
    original: &Original,
) -> std::result::Result<Tried, Tried> {
    let store = &app.store;
    let failed = |e: AppError| Tried::failed(Some(run_id), e);
    let run = store
        .run(run_id)
        .map_err(|e| failed(e.into()))?
        .ok_or_else(|| Tried::failed(Some(run_id), "the trial run is gone"))?;
    let succeeded = run.status == "succeeded";
    // What a run that did not succeed made is not worth checking.
    let checks = if succeeded {
        app.local_checks(
            repository,
            &format!("{name}-checks"),
            &format!("moebius/{name}"),
        )
    } else {
        None
    };
    let mut errors: BTreeSet<String> = run_errors(store, run_id)
        .map_err(failed)?
        .into_iter()
        .map(|f| f.kind.to_string())
        .filter(|kind| kinds.contains(kind))
        .collect();
    if checks == Some(false) && kinds.contains("checks_failed") {
        errors.insert("checks_failed".into());
    }
    let measured = Measured {
        succeeded,
        errors: errors.into_iter().collect(),
        checks,
        tokens: run_tokens(store, run_id).map_err(failed)?,
        rounds: 1,
    };
    let (positive, reason) = compare(&measured, original);
    Ok(Tried {
        run: Some(run_id),
        outcome: if positive { "positive" } else { "negative" },
        reason,
        measurements: json!({ "trial": measured, "original": original }),
    })
}
