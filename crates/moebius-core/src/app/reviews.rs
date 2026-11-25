//! Reviews of the pull requests someone else opened (ADR 0029). When the author of a followed pull request writes `/done`
//! on the issue, the server has the Reviewer read it, once the checks of its head are known and green, and writes what
//! the Reviewer found on the forge as a review with comments on lines. An approved pull request is merged, pinned to the
//! head that was read, unless the repository leaves the merge to the owner.
//!
//! One round is one row of `external_reviews` (see `store::reviews`) and moves through its states, driven by
//! [`App::advance_external_reviews`], which the poll calls after it read the checks of the request and the engine calls
//! for the card at each tick:
//!
//! - `/done` of the author adds a `wanted` round, unless the pull request was reviewed three times already (the owner is
//!   asked instead) or its head was reviewed;
//! - a `wanted` round starts once the checks of the head are green: pending checks wait, red ones are answered on the
//!   pull request and nothing is reviewed;
//! - a `running` round ends with the Reviewer's structured answer (see `board::structured`): a review of the head it read
//!   only, dropped when the head moved meanwhile;
//! - changes requested are written as a review that asks for them, with the comments on the lines the diff has; the rest
//!   goes into the body;
//! - an approval is written as an approving review, and the pull request is merged when the repository says so and its
//!   checks are green, else the card goes to Ready to merge and the owner is asked;
//! - a review a person asked for from the card (`kind` `person`) is written the same way, but is never merged and never
//!   moves the card: the person merges the pull request where they asked for the review.
//!
//! A review run of such a pull request is restricted: it reads files and nothing else (see `decide::guard`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use super::review_context::ContextError;
use super::{App, AppError, Result, columns};
use crate::board::{CardRuns, Variant, structured};
use crate::decide::Actor;
use crate::repos::forge::ReviewPost;
use crate::repos::git;
use crate::store::{Card, Change, ExternalReview, Repository, ReviewComment, Run};

/// How many rounds of the Reviewer a followed pull request gets; the owner is asked after that.
pub const ROUND_LIMIT: i64 = 3;

/// The column a project may have, for a card whose pull request waits for a measurement.
pub const READY_FOR_TESTING: &str = "Ready for testing";

/// The kind of the inbox item that asks the owner to look at a pull request the Reviewer read [`ROUND_LIMIT`] times.
pub const ROUNDS_SPENT: &str = "review_rounds_spent";
/// What the owner is told when a review was not written to the end.
pub(super) const REVIEW_NOT_POSSIBLE_REASON_POSTING: &str =
    "the Reviewer's review was not written to the end, so it may be on the forge in part";
/// The kind of the inbox item that says a followed pull request could not be given to the Reviewer.
pub const REVIEW_NOT_POSSIBLE: &str = "review_not_possible";

fn reviewer() -> Actor {
    Actor::Script {
        column: "external-review".into(),
    }
}

/// The lines each file of a diff shows on its new side, with the three lines of context a forge shows: where a comment
/// of a review may be written.
fn commentable_lines(diff: &str) -> BTreeMap<String, BTreeSet<i64>> {
    let mut lines: BTreeMap<String, BTreeSet<i64>> = BTreeMap::new();
    let mut file: Option<String> = None;
    let mut next = 0i64;
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ ") {
            file = path.strip_prefix("b/").map(str::to_string);
        } else if line.starts_with("--- ") {
            file = None;
        } else if let Some(header) = line.strip_prefix("@@ ") {
            next = header
                .split_whitespace()
                .find_map(|part| part.strip_prefix('+'))
                .and_then(|part| part.split(',').next()?.parse().ok())
                .unwrap_or(0);
        } else if let Some(file) = &file {
            match line.chars().next() {
                Some('+') | Some(' ') => {
                    lines.entry(file.clone()).or_default().insert(next);
                    next += 1;
                }
                _ => {}
            }
        }
    }
    lines
}

impl App {
    /// Makes `runs` start the review runs in place of the server's harness.
    #[cfg(test)]
    pub(crate) fn set_review_runs(&self, runs: Arc<dyn CardRuns>) {
        *self.review_runs.lock().unwrap_or_else(|e| e.into_inner()) = Some(runs);
    }

    /// What starts the review runs.
    pub(super) fn review_runs(&self) -> Result<Arc<dyn CardRuns>> {
        #[cfg(test)]
        if let Some(runs) = self
            .review_runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Ok(runs);
        }
        let runs: Arc<dyn CardRuns> = self.runs()?.clone();
        Ok(runs)
    }

    /// Whether a round of the card may act now: the card is not held, the agents are not paused and the repository is not
    /// frozen. A round that posts a review or merges is the server acting for the owner, and the owner's hold, pause and
    /// freeze stop it as they stop a run.
    fn may_act(&self, card_id: i64) -> Result<bool> {
        let card = self.card(card_id)?;
        Ok(card.held.is_none()
            && self.runs_paused()?.is_none()
            && self.frozen(card.repository_id)?.is_none())
    }

    /// Whether the pull request is against the repository's default branch, as the poll last read it. Unknown, which a
    /// forge that does not say leaves it, is as the follow found it.
    fn against_default_branch(&self, repository: &Repository, change: &Change) -> Result<bool> {
        Ok(self
            .store
            .change_base_branch(change.id)?
            .is_none_or(|branch| branch == repository.default_branch))
    }

    /// The open change of the card that is a pull request someone else opened.
    pub(crate) fn followed_change(&self, card: &Card) -> Result<Option<Change>> {
        Ok(self
            .open_changes(card)?
            .into_iter()
            .find(|change| change.outside_author.is_some()))
    }

    /// Whether a run of `purpose` for `card` is restricted: a review of a card that follows a pull request someone else
    /// opened. Fails closed: when the store cannot say, the run is restricted.
    pub(crate) fn restricts_review(&self, card: &Card, purpose: &str) -> bool {
        purpose.starts_with("review") && !matches!(self.followed_change(card), Ok(None))
    }

    /// Whether `run` was started restricted: it reads a pull request someone else opened and has no tools. Decided once,
    /// when the run starts, and kept on the run's row, so that nothing that happens to the pull request later (it is
    /// closed, the card moves) lifts it. Fails closed: a run the store cannot answer for is restricted.
    pub fn is_restricted_review(&self, run: &Run) -> bool {
        self.store.run_restricted(run.id).unwrap_or(true)
    }

    /// The comments of a review that fit lines of the diff of `change`, and the text of those that do not.
    fn split_comments(
        &self,
        repository: &Repository,
        head: &str,
        comments: &[ReviewComment],
    ) -> (Vec<ReviewComment>, String) {
        // Of the commit that was read, as a commit: never of a branch name, which may have moved.
        let diff = super::review_context::diff_of(repository, head, 3).unwrap_or_default();
        let lines = commentable_lines(&diff);
        let (mut fit, mut rest) = (Vec::new(), String::new());
        for comment in comments {
            if lines
                .get(&comment.path)
                .is_some_and(|set| set.contains(&comment.line))
            {
                fit.push(comment.clone());
            } else {
                rest.push_str(&format!(
                    "\n\n`{}` line {}: {}",
                    comment.path, comment.line, comment.body
                ));
            }
        }
        (fit, rest)
    }

    /// Writes `text` as a comment on the pull request of `change`, when the repository answers on the forge. A comment
    /// that cannot be written is reported with the forge's other errors.
    fn answer_on_request(&self, repository: &Repository, change: &Change, text: &str) {
        let (true, Some(number)) = (repository.answer_on_forge, change.pull_request) else {
            return;
        };
        let written = self.forge(repository).and_then(|forge| {
            Ok(forge.comment_issue(repository, number, &crate::blobs::redact(text))?)
        });
        if let Err(e) = written {
            self.bus.publish(crate::bus::Event::ForgeError {
                repository: repository.name.clone(),
                message: format!("pull request #{number}: {e}"),
            });
        }
    }

    /// `/done` of the author of the pull request the card follows: asks for a round of the Reviewer on its head. The
    /// answer says what comes of it.
    pub(in crate::app) fn review_on_done(
        &self,
        repository: &Repository,
        card: &Card,
        change: &Change,
    ) -> Result<String> {
        if self.store.external_review_rounds(change.id)? >= ROUND_LIMIT {
            self.ask_owner_to_look(card, change)?;
            return Ok(
                "The pull request was reviewed three times already; a person will look.".into(),
            );
        }
        // A frozen repository starts no round: the author is told, and writes `/done` again once it is lifted.
        if self.frozen(repository.id)?.is_some() {
            return Ok(
                "This repository is frozen for now, so no review starts. Write /done again once the owner has lifted it."
                    .into(),
            );
        }
        let wanted = self
            .store
            .external_reviews(change.id)?
            .iter()
            .any(|r| r.state == "wanted");
        if wanted {
            return Ok("The review starts once the checks have passed.".into());
        }
        let Some(head) = change.head.as_deref() else {
            return Ok(
                "I do not know the head of the pull request yet; write /done again in a moment."
                    .into(),
            );
        };
        if self.store.external_review_of_head(change.id, head)? {
            // An approval that could not be merged is merged again when its author asks, not read again.
            let unmerged = self
                .store
                .external_reviews(change.id)?
                .into_iter()
                .find(|r| {
                    r.head == head && r.state == "posted" && r.verdict.as_deref() == Some("approve")
                });
            if let (Some(round), true) = (unmerged, repository.merge_reviewed) {
                if !self.may_act(card.id)? {
                    return Ok(
                        "Moebius is paused, or the card is held, so it is not merged now.".into(),
                    );
                }
                if !matches!(change.checks.as_deref(), Some("success") | None)
                    || change.checks_head.as_deref() != Some(head)
                {
                    return Ok("The checks have not passed, so it is not merged.".into());
                }
                let author = change.outside_author.clone().unwrap_or_default();
                return Ok(
                    match self.merge_reviewed(repository, card, change, &round)? {
                        Ok(()) => format!("Merged, thank you @{author}!"),
                        Err(why) => {
                            format!("It could not be merged: {why}. The owner has been asked.")
                        }
                    },
                );
            }
            return Ok(
                "This version of the pull request was reviewed already; push your changes, then write /done again."
                    .into(),
            );
        }
        self.store.add_external_review(change.id, head)?;
        Ok("Thanks, the pull request will be reviewed once its checks have passed.".into())
    }

    /// Asks the owner to look at a pull request one of whose rounds had to be ended or could not start (see
    /// `review_sweep`).
    pub(super) fn ask_owner_about_a_round(&self, change: &Change, reason: &str) -> Result<()> {
        let Some(card) = change.card_id.map(|id| self.card(id)).transpose()? else {
            return Ok(());
        };
        self.ask_owner_unreviewed(&card, change, reason)
    }

    /// Puts an item in the owner's inbox that says the pull request could not be given to the Reviewer, and why: it is
    /// too large, changes a binary file, git could not read it, or the run of a review failed to start. The owner reads
    /// it and merges it, or does not.
    fn ask_owner_unreviewed(&self, card: &Card, change: &Change, reason: &str) -> Result<()> {
        if self
            .store
            .has_open_inbox_item(card.id, REVIEW_NOT_POSSIBLE)?
        {
            return Ok(());
        }
        self.store.add_inbox_item(
            REVIEW_NOT_POSSIBLE,
            Some(card.id),
            &format!(
                "Pull request #{} could not be reviewed: {}",
                change.pull_request.unwrap_or_default(),
                card.title
            ),
            &serde_json::json!({
                "change": change.id,
                "pull_request": change.pull_request,
                "author": change.outside_author,
                "repository": self.repository(card.repository_id)?.name,
                "reason": reason,
            }),
        )?;
        self.bus
            .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
        Ok(())
    }

    /// Puts an item in the owner's inbox that says the Reviewer read the pull request often enough.
    fn ask_owner_to_look(&self, card: &Card, change: &Change) -> Result<()> {
        if self.store.has_open_inbox_item(card.id, ROUNDS_SPENT)? {
            return Ok(());
        }
        self.store.add_inbox_item(
            ROUNDS_SPENT,
            Some(card.id),
            &format!(
                "Pull request #{} was reviewed {ROUND_LIMIT} times: {}",
                change.pull_request.unwrap_or_default(),
                card.title
            ),
            &serde_json::json!({
                "change": change.id,
                "pull_request": change.pull_request,
                "author": change.outside_author,
                "repository": self.repository(card.repository_id)?.name,
            }),
        )?;
        self.bus
            .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
        Ok(())
    }

    /// Moves on the rounds of the Reviewer on the pull request the card follows, as the module says. Called after the
    /// poll read the request and at each tick of the card; does nothing for a card that follows no request or has no
    /// round going. A failure is reported with the forge's errors, and the round it came from is retried at the next
    /// call unless it says otherwise.
    pub fn advance_external_reviews(self: &Arc<Self>, card_id: i64) {
        // Rounds that cannot end by themselves are ended first, whatever this card follows (`review_sweep`).
        if let Err(e) = self.sweep_external_reviews(super::review_sweep::clock(), false) {
            self.bus.publish(crate::bus::Event::ForgeError {
                repository: String::new(),
                message: format!("the sweep of the reviews of pull requests: {e}"),
            });
        }
        let result = self.card(card_id).and_then(|card| {
            let Some(change) = self.followed_change(&card)? else {
                return Ok(());
            };
            if self
                .store
                .external_reviews(change.id)?
                .iter()
                .all(|r| !matches!(r.state.as_str(), "wanted" | "running" | "approved"))
            {
                return Ok(());
            }
            // The owner's hold and pause stop what a round does on the forge, whichever thread comes here.
            if !self.may_act(card_id)? {
                return Ok(());
            }
            self.advance(&card, &change)
        });
        if let Err(e) = result {
            let name = self
                .card(card_id)
                .ok()
                .and_then(|card| self.repository(card.repository_id).ok())
                .map(|r| r.name)
                .unwrap_or_default();
            self.bus.publish(crate::bus::Event::ForgeError {
                repository: name,
                message: format!("the review of a pull request: {e}"),
            });
        }
    }

    fn advance(self: &Arc<Self>, card: &Card, change: &Change) -> Result<()> {
        let repository = self.repository_of(change)?;
        for round in self.store.external_reviews(change.id)? {
            match round.state.as_str() {
                "wanted" => self.advance_wanted(&repository, card, change, &round)?,
                "running" => self.advance_running(&repository, card, change, &round)?,
                "approved" => {
                    // The checks of the head may be green now; one thread takes the round to write it.
                    if !self.may_act(card.id)? {
                        continue;
                    }
                    if !self
                        .store
                        .move_external_review(round.id, "approved", "posting")?
                    {
                        continue;
                    }
                    let said = structured::Structured {
                        verdict: "approve",
                        summary: round.summary.clone(),
                        comments: round.comments.clone(),
                    };
                    self.act_on_verdict(&repository, card, change, &round, &said)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// A round the author asked for: waits for the checks of the current head, answers red ones, starts the Reviewer
    /// on green ones.
    fn advance_wanted(
        self: &Arc<Self>,
        repository: &Repository,
        card: &Card,
        change: &Change,
        round: &ExternalReview,
    ) -> Result<()> {
        let end = |why: &str, text: &str| -> Result<()> {
            // Only the thread that ends the round answers for it.
            if self
                .store
                .move_external_review(round.id, "wanted", "failed")?
            {
                self.store
                    .set_external_review_verdict(round.id, "failed", "", why, &[])?;
                self.answer_on_request(repository, change, text);
            }
            Ok(())
        };
        let Some(head) = change.head.as_deref() else {
            return Ok(());
        };
        if self.store.external_review_rounds(change.id)? >= ROUND_LIMIT {
            self.store
                .set_external_review_verdict(round.id, "failed", "", "rounds spent", &[])?;
            return self.ask_owner_to_look(card, change);
        }
        if self.store.external_review_of_head(change.id, head)? {
            return end(
                "head reviewed",
                "This version of the pull request was reviewed already; push your changes, then write /done again.",
            );
        }
        // The checks are those of the head the poll read last; after a push they are unknown until it has read them.
        if change.checks_head.as_deref() != Some(head) {
            return Ok(());
        }
        let no_checks = change.checks.is_none();
        match change.checks.as_deref() {
            // A repository without checks has none to wait for.
            Some("success") | None => {}
            Some("failure") => {
                return end(
                    "checks failed",
                    "The checks of this pull request fail, so nothing was reviewed. Fix them, push, then write /done again.",
                );
            }
            // Pending: the review waits for them.
            _ => return Ok(()),
        }
        let runs = self.review_runs()?;
        if !runs.has_agent("Reviewer") {
            return end(
                "no Reviewer",
                "There is no Reviewer agent on this server, so nothing was reviewed.",
            );
        }
        // The Reviewer reads what is fetched now; when the request moved on since the poll, the poll reads it first.
        if let Some(number) = change.pull_request {
            let forge = self.forge(repository)?;
            forge.fetch_request(repository, number, &change.branch)?;
            let fetched = git(Path::new(&repository.path), &["rev-parse", &change.branch])?;
            if fetched != head {
                return Ok(());
            }
        }
        if !self.against_default_branch(repository, change)? {
            return end(
                "another branch",
                "This pull request is against a branch other than the default branch, so nothing was reviewed.",
            );
        }
        // The commits the review is of, recorded before anything is read: the head, and the default branch's tip it is
        // merged into. Everything the Reviewer is given is read from these two, never from a name.
        let base = git(
            Path::new(&repository.path),
            &["rev-parse", &repository.base_ref()],
        )?;
        // What the Reviewer is given is made now, once, so that a pull request that is too big or does not merge is
        // answered and not started.
        let not_possible = |reason: &str| -> Result<()> {
            end(
                "not reviewed automatically",
                &format!(
                    "This pull request could not be reviewed automatically, so nothing was reviewed: {reason}. The owner has been asked to look at it."
                ),
            )?;
            self.ask_owner_unreviewed(card, change, reason)
        };
        match self.review_context_at(change, head, &base) {
            Ok(_) => {}
            Err(ContextError::Conflicts) => {
                return end(
                    "conflicts",
                    "This pull request does not merge cleanly into the default branch, so nothing was reviewed. Resolve the conflicts, push, then write /done again.",
                );
            }
            Err(
                ContextError::TooLarge(why)
                | ContextError::Unreadable(why)
                | ContextError::Other(why),
            ) => return not_possible(&why),
        }
        // The repository's rules for the Reviewer are read from the default branch's commit the review is of, never from
        // the pull request; a brief that is too large, or cannot be read, is a review that does not start.
        if let Err(why) = super::review_context::review_brief_at(repository, &base) {
            return not_possible(&why);
        }
        // Only one thread starts the run, and only one round of the pull request runs at a time: the poll and the engine
        // both come here. The claim writes the two commits with it, and the run reads them from the claimed round, so
        // nothing a thread that lost wrote can reach the run.
        if !self.store.claim_external_review(round.id, head, &base)? {
            return Ok(());
        }
        // For the owner's merge of a request that is not merged by the server: the commit that was reviewed.
        self.store.set_change_reviewed_head(change.id, head)?;
        self.store.set_change_reviewed_base(change.id, &base)?;
        // Whatever goes wrong from here until the run is recorded, the claim does not stay: the round goes back to
        // `wanted` when no run was made, and ends `failed` when one was.
        let started = (|| -> Result<()> {
            let context = BTreeMap::from([
                ("repository".into(), repository.name.clone()),
                (
                    "pull_request".into(),
                    change.pull_request.unwrap_or_default().to_string(),
                ),
            ]);
            self.record_action(
                &reviewer(),
                "review_change",
                Some(card.id),
                None,
                context,
                "the author said the pull request is done",
            )?;
            let rounds = self.store.external_review_rounds(change.id)?;
            let run = match runs.start(self, card, "Reviewer", ("review", None), Variant::default())
            {
                Ok(run) => run,
                Err(e) => {
                    // It is asked for again at the next call.
                    return Err(AppError::Conflict(e.to_string()));
                }
            };
            // The round is of the head the run reads, which is the one that was fetched just now.
            self.store
                .start_external_review(round.id, run.id, rounds, (head, &base))?;
            Ok(())
        })();
        if let Err(e) = started {
            let _ = self
                .store
                .move_external_review(round.id, "starting", "wanted");
            return Err(e);
        }
        if no_checks {
            self.answer_on_request(
                repository,
                change,
                "No checks ran for this pull request, so the review did not wait for any.",
            );
        }
        self.bus
            .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
        Ok(())
    }

    /// A round whose run is going: nothing until it ends; then its answer is read.
    fn advance_running(
        &self,
        repository: &Repository,
        card: &Card,
        change: &Change,
        round: &ExternalReview,
    ) -> Result<()> {
        let Some(run) = round.run_id.map(|id| self.run(id)).transpose()? else {
            return Ok(());
        };
        if !matches!(run.status.as_str(), "succeeded" | "failed" | "stopped") {
            return Ok(());
        }
        if !self.may_act(card.id)? {
            return Ok(());
        }
        // One thread reads the answer of the run; the other finds the round moved.
        if !self
            .store
            .move_external_review(round.id, "running", "posting")?
        {
            return Ok(());
        }
        match run.status.as_str() {
            "succeeded" => {}
            _ => {
                self.store.set_external_review_verdict(
                    round.id,
                    "failed",
                    "",
                    "the run did not end",
                    &[],
                )?;
                self.answer_on_request(
                    repository,
                    change,
                    "The review could not be done. Write /done to try again.",
                );
                return Ok(());
            }
        }
        let Some(said) = structured::read(run.summary.as_deref()) else {
            self.store.set_external_review_verdict(
                round.id,
                "failed",
                "",
                "the review gave no verdict",
                &[],
            )?;
            self.answer_on_request(
                repository,
                change,
                "The review gave no verdict, so nothing was written. Write /done to try again.",
            );
            return Ok(());
        };
        // A verdict counts for the head it read only.
        if change.head.as_deref() != Some(round.head.as_str()) {
            self.store.set_external_review_verdict(
                round.id,
                "stale",
                said.verdict,
                &said.summary,
                &said.comments,
            )?;
            self.answer_on_request(
                repository,
                change,
                "The pull request changed while it was reviewed, so the review was dropped. Write /done to have the new version reviewed.",
            );
            return Ok(());
        }
        self.act_on_verdict(repository, card, change, round, &said)
    }

    /// Writes the verdict of `round`, which this thread holds in the state `posting`, as `write_verdict` does. A failure
    /// ends the round as `failed`, and it is not tried again by itself: a review written twice would be two reviews, and
    /// a forge that failed half way (GitLab writes a note, discussions and an approval) has written part of it. The
    /// author may write `/done` for another.
    fn act_on_verdict(
        &self,
        repository: &Repository,
        card: &Card,
        change: &Change,
        round: &ExternalReview,
        said: &structured::Structured,
    ) -> Result<()> {
        // Hold and pause come first: the round is put back as it was found and nothing is written.
        if !self.may_act(card.id)? {
            self.store
                .move_external_review(round.id, "posting", "running")?;
            return Ok(());
        }
        // A pull request that was retargeted to another branch is no pull request that was reviewed.
        if !self.against_default_branch(repository, change)? {
            self.store.set_external_review_verdict(
                round.id,
                "stale",
                said.verdict,
                &said.summary,
                &said.comments,
            )?;
            self.answer_on_request(
                repository,
                change,
                "This pull request is against another branch now, so the review was dropped and nothing was merged.",
            );
            return Ok(());
        }
        let outcome = self.write_verdict(repository, card, change, round, said);
        if let Err(e) = &outcome {
            let reason = e.to_string();
            let reason = reason.lines().next().unwrap_or_default();
            self.store.set_external_review_verdict(
                round.id,
                "failed",
                "",
                &format!("the review could not be written: {reason}"),
                &[],
            )?;
            self.answer_on_request(
                repository,
                change,
                &format!("The review could not be written on the forge: {reason}. Write /done to try again."),
            );
        }
        outcome
    }

    /// Writes the verdict of `round` on the forge, and merges or hands on an approved pull request.
    fn write_verdict(
        &self,
        repository: &Repository,
        card: &Card,
        change: &Change,
        round: &ExternalReview,
        said: &structured::Structured,
    ) -> Result<()> {
        let number = change.pull_request.unwrap_or_default();
        // `approve_code` is an approval of the code that never merges: the measurement the repository's rules ask for is
        // missing (see `awaiting_measurement`).
        let code_only = said.verdict == "approve_code";
        let approve = said.verdict != "changes";
        // A review a person asked for is never merged: the person merges the pull request where they asked for it.
        let merges = repository.merge_reviewed && round.kind == "author" && !code_only;
        if approve && merges {
            // Merged only when the checks of the head that was read are green; pending ones are waited for.
            if change.head.as_deref() != Some(round.head.as_str()) {
                self.store
                    .set_external_review_state(round.id, "stale", None)?;
                return Ok(());
            }
            match (change.checks_head.as_deref() == Some(&round.head))
                .then_some(change.checks.as_deref())
            {
                // No checks at all is nothing to wait for, as for a merge by a person.
                Some(Some("success") | None) => {}
                Some(Some("failure")) => {
                    self.store.set_external_review_verdict(
                        round.id,
                        "posted",
                        said.verdict,
                        &said.summary,
                        &said.comments,
                    )?;
                    self.answer_on_request(
                        repository,
                        change,
                        "The reviewer approved this version, but its checks fail now, so it is not merged.",
                    );
                    return Ok(());
                }
                _ => {
                    self.store.set_external_review_verdict(
                        round.id,
                        "approved",
                        said.verdict,
                        &said.summary,
                        &said.comments,
                    )?;
                    return Ok(());
                }
            }
        }
        let (comments, rest) = self.split_comments(repository, &round.head, &said.comments);
        // Whatever the Reviewer wrote is redacted before it is posted: it was told by a stranger's pull request.
        let lead = if code_only {
            "**The code is approved; the measurement this repository asks for is still missing, so this is not merged yet.**\n\n"
        } else {
            ""
        };
        let body = crate::blobs::redact(&format!("{lead}{}{rest}", said.summary.trim()));
        let comments: Vec<ReviewComment> = comments
            .into_iter()
            .map(|c| ReviewComment {
                body: crate::blobs::redact(&c.body),
                ..c
            })
            .collect();
        let post = ReviewPost {
            approve,
            head: &round.head,
            body: body.trim(),
            comments: &comments,
        };
        let forge = self.forge(repository)?;
        let url = match forge.review_request(repository, number, &post) {
            Ok(url) => url,
            Err(first) => {
                // A forge that refuses the lines still takes the review without them.
                let plain = ReviewPost {
                    comments: &[],
                    body: &crate::blobs::redact(&format!("{}{}", said.summary.trim(), {
                        let all: String = comments
                            .iter()
                            .map(|c| format!("\n\n`{}` line {}: {}", c.path, c.line, c.body))
                            .collect();
                        format!("{all}{rest}")
                    })),
                    ..post
                };
                forge
                    .review_request(repository, number, &plain)
                    .map_err(|e| AppError::Conflict(format!("{first}; {e}")))?
            }
        };
        self.store.set_external_review_verdict(
            round.id,
            "posted",
            said.verdict,
            &said.summary,
            &said.comments,
        )?;
        if code_only {
            self.store
                .set_external_review_state(round.id, "awaiting_measurement", Some(&url))?;
            return self.awaiting_measurement(repository, card, change, round);
        }
        self.store
            .set_external_review_state(round.id, "posted", Some(&url))?;
        if !approve {
            self.answer(
                repository,
                card.id,
                &format!(
                    "Reviewed pull request #{number}: changes are requested, see the review there. Push your changes, then write /done."
                ),
            );
            self.bus
                .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
            return Ok(());
        }
        if round.kind == "person" {
            self.answer(
                repository,
                card.id,
                &format!("Reviewed pull request #{number}: approved. The owner will merge it."),
            );
            self.bus
                .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
            return Ok(());
        }
        if merges {
            // Whether it was merged or is left to the owner is told by `merge_reviewed` itself.
            self.merge_reviewed(repository, card, change, round)?.ok();
            return Ok(());
        }
        // The owner merges: the card waits in Ready to merge.
        self.hand_to_owner(card, change, "")?;
        self.answer(
            repository,
            card.id,
            &format!("Reviewed pull request #{number}: approved. The owner will merge it."),
        );
        Ok(())
    }

    /// Merges the approved pull request, pinned to the head that was read, and thanks its author. The inner error says why
    /// it was not merged: then the card waits in Ready to merge with an Inbox item that gives the reason, and the pull
    /// request says so; the author's next `/done` for the same head tries the merge again.
    fn merge_reviewed(
        &self,
        repository: &Repository,
        card: &Card,
        change: &Change,
        round: &ExternalReview,
    ) -> Result<std::result::Result<(), String>> {
        let by = reviewer();
        let author = change.outside_author.clone().unwrap_or_default();
        // Right before it merges: the owner's hold and pause, and the branch the pull request is against now.
        if !self.may_act(card.id)? {
            return Ok(Err("agents are paused, or the card is held".into()));
        }
        if !self.against_default_branch(repository, &self.change(change.id)?)? {
            return Ok(Err("the pull request is against another branch".into()));
        }
        // The forge merges into whatever the branch is now: what the review was of is the merge into the tip it read, so
        // the base is read from the forge again, the clone brought up to date, and what the merge does now compared with
        // what it did then.
        if let Some(number) = change.pull_request {
            let forge = self.forge(repository)?;
            match forge.request_base(repository, number) {
                Ok(branch) if branch == repository.default_branch => {}
                Ok(_) => return Ok(Err("the pull request is against another branch".into())),
                Err(e) => {
                    return Ok(Err(format!(
                        "the pull request's branch could not be read: {e}"
                    )));
                }
            }
            if let Err(e) = forge.refresh(repository) {
                return Ok(Err(format!("the default branch could not be read: {e}")));
            }
            let tip = git(
                Path::new(&repository.path),
                &["rev-parse", &repository.base_ref()],
            )?;
            let reviewed = round.base.clone();
            if reviewed.is_empty() {
                return Ok(Err("no base commit is recorded for the round".into()));
            }
            match super::review_context::effect_unchanged(repository, &reviewed, &tip, &round.head)
            {
                Ok(true) => {}
                Ok(false) => {
                    if self
                        .store
                        .move_external_review(round.id, "approved", "stale")?
                        || self
                            .store
                            .move_external_review(round.id, "posted", "stale")?
                    {
                        self.answer_on_request(
                            repository,
                            change,
                            "The default branch changed what merging this pull request does after it was reviewed, so it is not merged. Write /done again and it will be read again.",
                        );
                    }
                    return Ok(Err("the default branch changed after the review".into()));
                }
                Err(e) => return Ok(Err(format!("the merge could not be checked: {e}"))),
            }
        }
        // The checks above took time: the owner's hold, pause and setting are read once more, right before the forge is
        // asked to merge.
        if !self.may_act(card.id)? {
            return Ok(Err("agents are paused, or the card is held".into()));
        }
        if !self.repository(repository.id)?.merge_reviewed {
            self.hand_to_owner(card, change, "")?;
            return Ok(Err("merging approved pull requests was turned off".into()));
        }
        let merged = self.merge_one(
            &by,
            change.id,
            "the Reviewer approved the pull request",
            Some(&round.head),
        );
        match merged {
            Ok(_) => {
                self.store
                    .set_external_review_state(round.id, "merged", None)?;
                self.answer(
                    repository,
                    card.id,
                    &format!("Merged, thank you @{author}!"),
                );
                Ok(Ok(()))
            }
            Err(e) => {
                let reason = e.to_string();
                let reason = reason.lines().next().unwrap_or_default().to_string();
                self.answer_on_request(
                    repository,
                    change,
                    &format!(
                        "The reviewer approved this version, but it could not be merged: {reason}"
                    ),
                );
                // A refusal of the merge gate has put the card in Ready to merge with its reason already.
                if !matches!(e, AppError::Refused(_)) {
                    self.hand_to_owner(card, change, &reason)?;
                }
                Ok(Err(reason))
            }
        }
    }

    /// The Reviewer approved the code of `round` and its review is on the forge, but the measurement the repository's
    /// brief asks for is missing. Nothing is merged. The issue gets the repository's ready-for-testing label, the card
    /// goes to the project's column "Ready for testing" when the project has one (else it stays in In review), and the
    /// issue says that a measurement is needed and how to post one. The round counts; a later measurement starts the
    /// next. A review a person asked for is only written, as for any verdict: it moves nothing.
    fn awaiting_measurement(
        &self,
        repository: &Repository,
        card: &Card,
        change: &Change,
        round: &ExternalReview,
    ) -> Result<()> {
        let number = change.pull_request.unwrap_or_default();
        if round.kind == "person" {
            self.answer(
                repository,
                card.id,
                &format!(
                    "Reviewed pull request #{number}: the code is approved, but the measurement the repository asks for is missing."
                ),
            );
            self.bus
                .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
            return Ok(());
        }
        let report = |what: &str, e: &dyn std::fmt::Display| {
            self.bus.publish(crate::bus::Event::ForgeError {
                repository: repository.name.clone(),
                message: format!("pull request #{number}: {what}: {e}"),
            });
        };
        if let Err(e) =
            self.set_labels_for_script(card.id, &[(repository.testing_label.clone(), true)])
        {
            report("the label could not be written", &e);
        }
        let now = self.card(card.id)?;
        let has_column = self
            .project_columns_of(now.project_id)?
            .views()
            .into_iter()
            .find(|c| c.name.eq_ignore_ascii_case(READY_FOR_TESTING))
            .map(|c| c.name);
        if let Some(column) = has_column
            && now.column != column
            && self.column_class(now.project_id, &now.column) != columns::DONE
        {
            self.move_card(card.id, &column, Some(&reviewer()))?;
        }
        let measurers = match repository.measurers.as_slice() {
            [] => "a registered measurer".to_string(),
            logins => format!(
                "a registered measurer ({})",
                logins
                    .iter()
                    .map(|login| format!("@{login}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        let text = format!(
            "Reviewed pull request #{number}: the code looks right, but the measurement this repository asks for is missing, so it is not merged. \
             To go on, {measurers} posts a comment on this issue that starts with `## Measurement` and gives the measured numbers: \
             before and after on one machine, and that the output is the same. The pull request is reviewed again once that comment is here."
        );
        let by = reviewer();
        if let Err(e) = self.comment_issue(&by, card.id, &text) {
            report("the reply could not be written", &e);
        }
        self.bus
            .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
        Ok(())
    }

    /// The approved pull request waits for the owner: the card goes to Ready to merge, with an Inbox item that says why
    /// when it could not be merged.
    fn hand_to_owner(&self, card: &Card, change: &Change, why: &str) -> Result<()> {
        self.store.resolve_inbox_for_card(card.id)?;
        let now = self.card(card.id)?;
        if self.column_class(now.project_id, &now.column) != columns::READY_TO_MERGE {
            self.move_card(card.id, columns::READY_TO_MERGE, Some(&reviewer()))?;
        }
        let mut data = serde_json::json!({ "change": change.id, "branch": change.branch });
        if !why.is_empty() {
            data["reason"] = serde_json::json!(why);
        }
        self.store
            .add_inbox_item("ready_to_merge", Some(card.id), &card.title, &data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_of_a_diff_are_the_new_side_of_each_hunk() {
        let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +10,4 @@ fn x\n ctx\n-old\n+new\n+newer\n ctx2\n\
                    diff --git a/b.rs b/b.rs\n--- /dev/null\n+++ b/b.rs\n@@ -0,0 +1,2 @@\n+one\n+two";
        let lines = commentable_lines(diff);
        assert_eq!(lines["a.rs"], BTreeSet::from([10, 11, 12, 13]));
        assert_eq!(lines["b.rs"], BTreeSet::from([1, 2]));
        assert!(!lines.contains_key("c.rs"));
    }

    #[test]
    fn a_merge_of_a_verdict_is_refused_unless_the_head_is_the_one_that_was_read() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::store::Store::in_memory(dir.path()).unwrap());
        let host = Arc::new(moebius_plugin_api::Host::new(
            std::time::Duration::from_secs(1),
        ));
        let app = App::new(store.clone(), host, dir.path());
        let repo = store.add_repository("r", "/r", "none", "main").unwrap();
        let change = store
            .add_change(repo.id, None, "moebius/pr-7", Some(7), "bbb")
            .unwrap();
        store
            .set_change_outside(change.id, "alice", "In progress")
            .unwrap();
        let error = app
            .merge_one(&reviewer(), change.id, "approved", Some("aaa"))
            .unwrap_err();
        assert!(error.to_string().contains("moved on from aaa"), "{error}");
        // The state is untouched: nothing was merged.
        assert_eq!(store.change(change.id).unwrap().unwrap().state, "open");
    }
}
