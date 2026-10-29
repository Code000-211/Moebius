//! Trying a proposal: its trials started on a thread of their own, one card after the other, each stored and
//! published, the proposal's status from them and its inbox item; and the schedule's start of the oldest open proposal
//! inside the trial window.

use std::sync::Arc;

use serde_json::json;

use super::card::{Tried, try_card};
use super::verdict::status_of;
use super::{find, in_window, publish_status};
use crate::app::{App, AppError, Result};
use crate::board::Runs;
use crate::bus::AppEvent;
use crate::config::TimeOfDay;
use crate::harness::repo::HarnessRepo;
use crate::store::Proposal;

/// Stores a trial and publishes it.
fn record(app: &App, proposal: i64, card: Option<i64>, tried: Tried) -> Result<()> {
    app.store.add_trial(
        proposal,
        card,
        tried.run,
        tried.outcome,
        &tried.reason,
        &tried.measurements,
    )?;
    app.bus.publish(AppEvent::new(
        "trial_done",
        json!({ "proposal": proposal, "card": card, "outcome": tried.outcome }),
    ));
    Ok(())
}

/// Tries the proposal on each of its cards, one after the other, then gives it its status; a positive or untried
/// proposal goes to the inbox.
fn try_proposal(
    app: &Arc<App>,
    runs: &Arc<dyn Runs>,
    repo: &HarnessRepo,
    proposal: &Proposal,
) -> Result<()> {
    if proposal.cards.is_empty() {
        let reason = "the proposal names no cards, so there is no task to try it on".to_string();
        record(app, proposal.id, None, Tried::skipped(reason))?;
    }
    for &card in &proposal.cards {
        let tried = try_card(app, runs, repo, proposal, card).unwrap_or_else(|tried| tried);
        record(app, proposal.id, Some(card), tried)?;
    }
    let trials = find(app, proposal.id)?.trials;
    let outcomes: Vec<&str> = trials.iter().map(|t| t.outcome.as_str()).collect();
    let status = status_of(&outcomes);
    if status != "negative" {
        let reasons: Vec<_> = trials
            .iter()
            .map(|t| json!({ "card": t.card_id, "outcome": t.outcome, "reason": t.reason }))
            .collect();
        app.store.add_inbox_item(
            "proposal",
            None,
            &proposal.title,
            &json!({ "proposal": proposal.id, "branch": proposal.branch, "trials": reasons }),
        )?;
    }
    app.store.set_proposal_status(proposal.id, status)?;
    publish_status(app, proposal.id, status);
    Ok(())
}

/// Starts the trials of the open proposal `id` on a thread of its own and returns the proposal, which is `trying`.
/// Only one proposal is tried at a time.
pub fn start(
    app: &Arc<App>,
    runs: &Arc<dyn Runs>,
    repo: &Arc<HarnessRepo>,
    id: i64,
) -> Result<Proposal> {
    let proposal = find(app, id)?;
    if proposal.kind != "harness" {
        return Err(AppError::Conflict(format!(
            "proposal {id} changes Moebius's own configuration, which is not tried: a person decides it in the inbox"
        )));
    }
    if !app.store.start_trying(id)? {
        return Err(AppError::Conflict(if proposal.status == "open" {
            "another proposal is being tried".to_string()
        } else {
            format!("proposal {id} is {}, not open", proposal.status)
        }));
    }
    publish_status(app, id, "trying");
    let (app, runs, repo) = (app.clone(), runs.clone(), repo.clone());
    let started = std::thread::Builder::new()
        .name(format!("trial of proposal {id}"))
        .spawn({
            let app = app.clone();
            move || {
                if let Err(e) = try_proposal(&app, &runs, &repo, &proposal) {
                    // Whatever went wrong, the proposal is not left trying.
                    let _ = app.store.set_proposal_status(proposal.id, "untried");
                    publish_status(&app, proposal.id, "untried");
                    let message = format!("the trial of proposal {} failed: {e}", proposal.id);
                    app.bus
                        .publish(AppEvent::new("retro_error", json!({ "message": message })));
                }
            }
        });
    if let Err(e) = started {
        app.store.set_proposal_status(id, "open")?;
        publish_status(&app, id, "open");
        return Err(AppError::Invalid(format!("cannot start the trial: {e}")));
    }
    find(&app, id)
}

/// The schedule's check, once a minute: inside the trial window, and when no other proposal is being tried and the
/// board has a run to spare (`run_limit`, counting the runs of cards), starts the trials of the oldest open proposal.
pub fn on_schedule(
    app: &Arc<App>,
    runs: &Arc<dyn Runs>,
    repo: Option<&Arc<HarnessRepo>>,
    window: (TimeOfDay, TimeOfDay),
    run_limit: u32,
) -> Result<()> {
    let Some(repo) = repo else {
        return Ok(());
    };
    if !in_window(TimeOfDay(app.store.minutes_into_the_day()?), window) {
        return Ok(());
    }
    let active = app
        .store
        .active_runs()?
        .iter()
        .filter(|run| run.card_id.is_some())
        .count();
    if active >= run_limit as usize {
        return Ok(());
    }
    if app
        .store
        .researches()?
        .iter()
        .any(|r| r.status == "running")
    {
        return Ok(());
    }
    let proposals = app.store.proposals(None)?;
    if proposals.iter().any(|p| p.status == "trying") {
        return Ok(());
    }
    // The newest come first.
    match proposals.iter().rev().find(|p| p.status == "open") {
        Some(oldest) => start(app, runs, repo, oldest.id).map(drop),
        None => Ok(()),
    }
}
