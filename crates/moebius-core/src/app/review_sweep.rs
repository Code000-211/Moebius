//! Ending the rounds of the Reviewer that cannot end by themselves (ADR 0029). A pull request has one round going at a
//! time, so a round left going by a crash, a failure between its claim and its run, a review that was never written, or
//! a pull request that was closed would hold the pull request for ever. The sweep ends those, a person ends one by
//! hand, and a closed pull request ends its rounds.

use super::reviews::REVIEW_NOT_POSSIBLE_REASON_POSTING;
use super::{App, AppError, Result};
use crate::decide::Actor;
use crate::store::{Change, ExternalReview, now};

/// How long a round may be starting, in seconds: its run is made in moments.
pub const STARTING_SECONDS: i64 = 300;
/// How long a round may be writing its review on the forge, in seconds.
pub const POSTING_SECONDS: i64 = 600;

/// The states in which a round holds its pull request or waits for its turn: a person's end of the round, and a
/// pull request that closes, end all of them.
const ACTIVE: [&str; 5] = ["wanted", "starting", "running", "posting", "approved"];

impl App {
    /// Ends the rounds that cannot end by themselves, as of `now`, in seconds. At startup (`restart`) no thread of
    /// this server is claiming or writing anything, so a round that is starting or posting is dead at once.
    ///
    /// - A round `starting` for over [`STARTING_SECONDS`] ends `failed`: its claim was never followed by a run.
    /// - A round `posting` for over [`POSTING_SECONDS`] ends `failed` and the owner is asked to look: the review may
    ///   be on the forge in part, so it is never written again by itself.
    /// - A round `running` for a pull request that was closed or merged ends `failed`, and its run is stopped.
    pub fn sweep_external_reviews(&self, now: i64, restart: bool) -> Result<()> {
        for round in self
            .store
            .external_reviews_in(&["starting", "running", "posting"])?
        {
            let change = self.change(round.change_id)?;
            let age = now - round.moved_at;
            match round.state.as_str() {
                "starting" if restart || age > STARTING_SECONDS => {
                    self.end_round(&round, "the review did not start")?;
                }
                "posting" if restart || age > POSTING_SECONDS => {
                    if self.end_round(&round, "writing the review did not finish")? {
                        self.ask_owner_about_a_round(&change, REVIEW_NOT_POSSIBLE_REASON_POSTING)?;
                    }
                }
                "running" if change.state != "open" => {
                    self.end_round(&round, "the pull request was closed")?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Whether the run of the round has ended.
    fn run_ended(&self, round: &ExternalReview) -> Result<bool> {
        Ok(match round.run_id.map(|id| self.run(id)) {
            Some(Ok(run)) => matches!(run.status.as_str(), "succeeded" | "failed" | "stopped"),
            // A run that is not there is not going.
            Some(Err(_)) | None => true,
        })
    }

    /// Ends the round as `failed` with the reason, from the state it is in, and stops its run if one is going. False
    /// when another thread moved the round first.
    pub(crate) fn end_round(&self, round: &ExternalReview, why: &str) -> Result<bool> {
        if !self
            .store
            .move_external_review(round.id, &round.state, "failed")?
        {
            return Ok(false);
        }
        self.store
            .set_external_review_verdict(round.id, "failed", "", why, &[])?;
        if let Some(run_id) = round.run_id
            && !self.run_ended(round)?
            && let Ok(runs) = self.review_runs()
        {
            runs.stop(run_id);
        }
        Ok(true)
    }

    /// Ends every round of the pull request the change follows that is going or waiting, because it closed.
    pub(crate) fn end_rounds_of(&self, change: &Change, why: &str) -> Result<()> {
        for round in self.store.external_reviews(change.id)? {
            if ACTIVE.contains(&round.state.as_str()) {
                self.end_round(&round, why)?;
            }
        }
        Ok(())
    }

    /// A person ends the round of the pull request the change follows that is going or waiting, and stops its run.
    /// The author may write `/done` again afterwards; the round has counted if its run had started.
    pub fn end_review_round(&self, by: &Actor, change_id: i64, reason: &str) -> Result<usize> {
        let change = self.change(change_id)?;
        if change.outside_author.is_none() {
            return Err(AppError::Invalid(format!(
                "change {change_id} is not a pull request of someone else"
            )));
        }
        let mut ended = 0;
        let mut rounds = Vec::new();
        for id in self.store.same_pull_request_changes(change_id)? {
            rounds.extend(self.store.external_reviews(id)?);
        }
        for round in rounds {
            if ACTIVE.contains(&round.state.as_str())
                && self.end_round(&round, "a person ended the round")?
            {
                ended += 1;
            }
        }
        if ended == 0 {
            return Err(AppError::Invalid(
                "no review round of this pull request is going".into(),
            ));
        }
        let context = std::collections::BTreeMap::from([(
            "pull_request".to_string(),
            change.pull_request.unwrap_or_default().to_string(),
        )]);
        self.record_action(
            by,
            "end_review_round",
            change.card_id,
            None,
            context,
            reason,
        )?;
        Ok(ended)
    }
}

/// Stamps `now` for the callers that sweep on a clock.
pub(crate) fn clock() -> i64 {
    now()
}
