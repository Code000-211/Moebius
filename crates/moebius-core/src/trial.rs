//! Trials (spec 7, ADR 0001, ADR 0008): a proposal of the Retro (see `retro`) is tried on the tasks of the errors it
//! addresses before a person is asked to merge it. For each card its `Cards:` line names, the card's agent runs the
//! card's task again (a run with the purpose `trial`) in a fresh worktree of the card's repository, on a branch of its
//! own, with the agent's definition read from the proposal's branch instead of the default branch. Afterwards the
//! repository's own checks run on what the agent made. A trial is positive when the errors the proposal addresses are
//! gone and nothing got worse; the trial's branches and worktrees are removed and only the stored result stays.
//!
//! A proposal is `open` until it is tried, `trying` while it is, and then `positive` (every trial was), `negative`
//! (some trial was) or `untried` (the rest: some trial was skipped or failed, so nobody knows). A positive or untried
//! proposal waits in the inbox for a person to merge or dismiss it; a negative one stays on the Retro page. The
//! schedule asks `on_schedule` once a minute, and trials only start inside the trial window; `POST
//! /v1/proposals/{id}/trial` tries a proposal at any hour. Nothing here waits for a person.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::Serialize;
use serde_json::json;

use crate::app::{App, AppError, Result};
use crate::board::Runs;
use crate::bus::AppEvent;
use crate::config::TimeOfDay;
use crate::harness_repo::HarnessRepo;
use crate::repos::trial_name;
use crate::retro::{Report, wait_for_end};
use crate::store::{Found, Proposal, Store};

/// How many times the tokens of the card's first code run a trial run may use and still be no worse.
const TOKENS_FACTOR: f64 = 1.5;

/// The agent that tried a card's task when the report does not say who made its error.
const DEFAULT_AGENT: &str = "Coder";

/// What the trial run of one card measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Measured {
    /// Whether the run succeeded: it ended with a commit of what the agent made.
    pub succeeded: bool,
    /// The kinds of the addressed errors that happened in the run, as the Retro's report has them.
    pub errors: Vec<String>,
    /// Whether the repository's own checks passed on what the run made; none when the repository has none, or when
    /// the run made nothing to check.
    pub checks: Option<bool>,
    /// The tokens the run used.
    pub tokens: i64,
    /// A trial is one round.
    pub rounds: usize,
}

/// What the card's first code run, the one the errors happened in, measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Original {
    /// The kinds of the errors the card had in the Retro's report.
    pub errors: Vec<String>,
    /// Whether the card was once sent back because the repository's own checks failed.
    pub checks_failed: bool,
    /// The code runs the card needed. A trial is one round, which is never more than that, so rounds never make a
    /// trial negative.
    pub rounds: usize,
    /// The tokens of the first code run.
    pub tokens: i64,
}

/// Whether a trial is positive, and why or why not: positive when the run succeeded, none of the addressed errors
/// happened, the checks passed or failed before as well, and the tokens are no more than `TOKENS_FACTOR` times those of
/// the first code run.
pub fn compare(trial: &Measured, original: &Original) -> (bool, String) {
    let mut worse = Vec::new();
    if !trial.succeeded {
        worse.push("the run did not succeed".to_string());
    }
    if !trial.errors.is_empty() {
        worse.push(format!("it still has {}", trial.errors.join(", ")));
    }
    if trial.checks == Some(false) && !original.checks_failed {
        worse.push("the repository's checks fail, which they did not before".to_string());
    }
    if trial.tokens as f64 > original.tokens as f64 * TOKENS_FACTOR {
        worse.push(format!(
            "it used {} tokens, more than {TOKENS_FACTOR} times the first run's {}",
            trial.tokens, original.tokens
        ));
    }
    if !worse.is_empty() {
        return (false, worse.join("; "));
    }
    let checks = match trial.checks {
        Some(true) => "the checks pass",
        Some(false) => "the checks fail as they did before",
        None => "there were no checks",
    };
    (
        true,
        format!(
            "the run succeeded, {} did not happen again, {checks}, and it used {} tokens against {}",
            original.errors.join(", "),
            trial.tokens,
            original.tokens
        ),
    )
}

/// Whether `now` is inside the trial window: from its start up to its end, which may be on the next day. A window that
/// ends when it starts is empty.
pub fn in_window(now: TimeOfDay, (start, end): (TimeOfDay, TimeOfDay)) -> bool {
    if start.0 <= end.0 {
        start.0 <= now.0 && now.0 < end.0
    } else {
        now.0 >= start.0 || now.0 < end.0
    }
}

/// One card's trial, before it is stored.
struct Tried {
    run: Option<i64>,
    outcome: &'static str,
    reason: String,
    measurements: serde_json::Value,
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
    fn skipped(reason: String) -> Self {
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
fn try_card(
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
    let report: Report = serde_json::from_value(retro.report)
        .map_err(|e| broken(crate::store::StoreError::from(e).into()))?;
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
    let _ =
        app.repos
            .remove_trial_branch(std::path::Path::new(&repository.path), proposal.id, card_id);
    tried
}

/// The measurements of the ended trial run `run_id`, compared with the original.
fn measure(
    app: &App,
    repository: &crate::store::Repository,
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

/// The status a proposal gets from its trials.
fn status_of(outcomes: &[&str]) -> &'static str {
    if outcomes.contains(&"negative") {
        "negative"
    } else if !outcomes.is_empty() && outcomes.iter().all(|o| *o == "positive") {
        "positive"
    } else {
        "untried"
    }
}

fn publish_status(app: &App, proposal: i64, status: &str) {
    app.bus.publish(AppEvent::new(
        "proposal_status",
        json!({ "proposal": proposal, "status": status }),
    ));
}

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
    let trials = app
        .store
        .proposal(proposal.id)?
        .ok_or_else(|| AppError::NotFound(format!("proposal {}", proposal.id)))?
        .trials;
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
    let proposal = app
        .store
        .proposal(id)?
        .ok_or_else(|| AppError::NotFound(format!("proposal {id}")))?;
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
    app.store
        .proposal(id)?
        .ok_or_else(|| AppError::NotFound(format!("proposal {id}")))
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

/// The proposal `id`, if a person can still decide about it: it was tried, and it was not negative.
fn to_decide(app: &App, id: i64) -> Result<Proposal> {
    let proposal = app
        .store
        .proposal(id)?
        .ok_or_else(|| AppError::NotFound(format!("proposal {id}")))?;
    if !matches!(proposal.status.as_str(), "positive" | "untried") {
        return Err(AppError::Conflict(format!(
            "proposal {id} is {}, not positive or untried",
            proposal.status
        )));
    }
    Ok(proposal)
}

/// Closes the matter of a proposal: its inbox item is resolved and it has `status`.
fn decided(app: &App, id: i64, status: &str) -> Result<Proposal> {
    app.store.resolve_inbox_for_proposal(id)?;
    app.store.set_proposal_status(id, status)?;
    publish_status(app, id, status);
    app.store
        .proposal(id)?
        .ok_or_else(|| AppError::NotFound(format!("proposal {id}")))
}

/// A person merges a positive or untried proposal into the harness configuration. A conflict changes nothing.
pub fn merge(app: &App, repo: &HarnessRepo, id: i64, reason: &str) -> Result<Proposal> {
    let proposal = to_decide(app, id)?;
    repo.merge_proposal(app, &proposal.branch, &proposal.title, reason)?;
    decided(app, id, "merged")
}

/// A person dismisses a positive or untried proposal.
pub fn dismiss(app: &App, repo: &HarnessRepo, id: i64, reason: &str) -> Result<Proposal> {
    let proposal = to_decide(app, id)?;
    repo.dismiss_proposal(app, &proposal.branch, &proposal.title, reason)?;
    decided(app, id, "dismissed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u16, minute: u16) -> TimeOfDay {
        TimeOfDay(hour * 60 + minute)
    }

    fn measured() -> Measured {
        Measured {
            succeeded: true,
            errors: vec![],
            checks: Some(true),
            tokens: 150,
            rounds: 1,
        }
    }

    fn original() -> Original {
        Original {
            errors: vec!["run_failed".into()],
            checks_failed: false,
            rounds: 2,
            tokens: 100,
        }
    }

    #[test]
    fn a_trial_is_positive_when_the_error_is_gone_and_nothing_got_worse() {
        let (positive, reason) = compare(&measured(), &original());
        assert!(positive, "{reason}");
        assert!(
            reason.contains("run_failed did not happen again"),
            "{reason}"
        );
        // No checks, or checks that failed before as well, are no worse.
        let none = Measured {
            checks: None,
            ..measured()
        };
        assert!(compare(&none, &original()).0);
        let failing = Measured {
            checks: Some(false),
            ..measured()
        };
        let failed_before = Original {
            checks_failed: true,
            ..original()
        };
        assert!(compare(&failing, &failed_before).0);
    }

    #[test]
    fn a_trial_is_negative_for_each_thing_that_is_not_as_good() {
        let negative = |trial: Measured, original: Original| {
            let (positive, reason) = compare(&trial, &original);
            assert!(!positive);
            reason
        };
        let reason = negative(
            Measured {
                succeeded: false,
                ..measured()
            },
            original(),
        );
        assert_eq!(reason, "the run did not succeed");
        let reason = negative(
            Measured {
                errors: vec!["run_failed".into(), "tool_failed".into()],
                ..measured()
            },
            original(),
        );
        assert_eq!(reason, "it still has run_failed, tool_failed");
        let reason = negative(
            Measured {
                checks: Some(false),
                ..measured()
            },
            original(),
        );
        assert_eq!(
            reason,
            "the repository's checks fail, which they did not before"
        );
        // Everything that is wrong is said.
        let reason = negative(
            Measured {
                succeeded: false,
                checks: Some(false),
                ..measured()
            },
            original(),
        );
        assert!(reason.starts_with("the run did not succeed; the repository's checks"));
    }

    #[test]
    fn a_trial_may_use_one_and_a_half_times_the_tokens_of_the_first_run() {
        let with = |tokens| Measured {
            tokens,
            ..measured()
        };
        assert!(compare(&with(150), &original()).0);
        let (positive, reason) = compare(&with(151), &original());
        assert!(!positive);
        assert_eq!(
            reason,
            "it used 151 tokens, more than 1.5 times the first run's 100"
        );
        // A first run that used nothing allows nothing.
        let free = Original {
            tokens: 0,
            ..original()
        };
        assert!(compare(&with(0), &free).0);
        assert!(!compare(&with(1), &free).0);
    }

    #[test]
    fn the_trial_window_may_cross_midnight() {
        let day = (at(0, 0), at(6, 0));
        assert!(in_window(at(0, 0), day));
        assert!(in_window(at(5, 59), day));
        assert!(!in_window(at(6, 0), day));
        assert!(!in_window(at(23, 59), day));
        let night = (at(22, 0), at(4, 30));
        assert!(in_window(at(22, 0), night));
        assert!(in_window(at(23, 59), night));
        assert!(in_window(at(0, 0), night));
        assert!(in_window(at(4, 29), night));
        assert!(!in_window(at(4, 30), night));
        assert!(!in_window(at(12, 0), night));
        assert!(!in_window(at(21, 59), night));
        // A window that ends when it starts is empty.
        assert!(!in_window(at(3, 0), (at(3, 0), at(3, 0))));
    }

    #[test]
    fn the_trials_give_a_proposal_its_status() {
        assert_eq!(status_of(&["positive"]), "positive");
        assert_eq!(status_of(&["positive", "positive"]), "positive");
        assert_eq!(status_of(&["positive", "negative"]), "negative");
        assert_eq!(status_of(&["skipped", "negative"]), "negative");
        assert_eq!(status_of(&["positive", "skipped"]), "untried");
        assert_eq!(status_of(&["failed"]), "untried");
        assert_eq!(status_of(&[]), "untried");
    }
}
