//! Reviews and measurements that flow without the owner's clicks (ADR 0033). Four parts, each a module:
//!
//! - [`measure`]: a comment of a registered measurer that starts with `## Measurement` is read at the poll, checked
//!   (who wrote it, which commits it names, which fields it has) and, when it holds, starts the next round of the
//!   Reviewer with the measurement in its context;
//! - [`committed`]: in a repository that trusts it, the measurement the author committed with the change is accepted
//!   for the head the Reviewer approved;
//! - [`bind`] and [`unbound`]: a pull request that names no issue with a closing keyword is bound to the issue its title
//!   or branch names, or asked once to name one and closed politely when it does not;
//! - this file: the settings of a repository (`GET` and `PUT /v1/repositories/{id}/flow`) and `review_on_green`, which
//!   has a followed pull request reviewed once the checks of its head have been green, or absent, for a few minutes.
//!
//! Nothing here acts for a card that is held, in a frozen repository, or while all agents are paused.

mod bind;
mod committed;
pub mod measure;
mod status;
mod unbound;

use std::collections::BTreeMap;

use super::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::{Card, Change, Flow, now};

/// The most minutes the checks may be asked to stay green before a review starts.
pub const GREEN_WAIT_AT_MOST: i64 = 120;
/// The most days a pull request that names no issue may stay open.
pub const UNBOUND_DAYS_AT_MOST: i64 = 365;
/// The most rounds of the Reviewer a repository may give a followed pull request, short of no limit (0).
pub const ROUND_LIMIT_AT_MOST: i64 = 100;
/// The label a pull request that names no issue gets.
pub const UNBOUND_LABEL: &str = "unbound";

impl App {
    /// A person changes the flow settings of a repository on a forge. Recorded as an `edit_flow` decision. An agent does
    /// not: what starts a review without a person is the owner's to turn on.
    pub fn edit_flow(
        &self,
        by: &Actor,
        repository_id: i64,
        edit: &FlowEdit,
        reason: &str,
    ) -> Result<Flow> {
        if !by.is_person() {
            return Err(AppError::Forbidden(
                "only a person changes the flow settings of a repository".into(),
            ));
        }
        let repository = self.repository(repository_id)?;
        if repository.forge == "none" {
            return Err(AppError::Invalid(format!(
                "{} is on no forge, so there is nothing to review there",
                repository.name
            )));
        }
        let before = self.store.flow(repository_id)?;
        let after = Flow {
            review_on_green: edit.review_on_green.unwrap_or(before.review_on_green),
            green_wait_minutes: edit.green_wait_minutes.unwrap_or(before.green_wait_minutes),
            unbound_close_days: edit.unbound_close_days.unwrap_or(before.unbound_close_days),
            status_labels: edit.status_labels.unwrap_or(before.status_labels),
            status_prefix: edit
                .status_prefix
                .as_deref()
                .map_or(before.status_prefix.clone(), |p| p.trim().to_string()),
            work_on_forge: edit.work_on_forge.unwrap_or(before.work_on_forge),
            review_round_limit: edit.review_round_limit.unwrap_or(before.review_round_limit),
        };
        let prefix = &after.status_prefix;
        if prefix.is_empty() || prefix.len() > 30 || prefix.contains([',', '\n', '\r']) {
            return Err(AppError::Invalid(format!(
                "status_prefix is 1 to 30 characters with no comma or new line, not {prefix:?}"
            )));
        }
        if !(0..=GREEN_WAIT_AT_MOST).contains(&after.green_wait_minutes) {
            return Err(AppError::Invalid(format!(
                "green_wait_minutes is a number from 0 to {GREEN_WAIT_AT_MOST}, not {}",
                after.green_wait_minutes
            )));
        }
        if !(0..=UNBOUND_DAYS_AT_MOST).contains(&after.unbound_close_days) {
            return Err(AppError::Invalid(format!(
                "unbound_close_days is a number from 0 to {UNBOUND_DAYS_AT_MOST}, not {}",
                after.unbound_close_days
            )));
        }
        if !(0..=ROUND_LIMIT_AT_MOST).contains(&after.review_round_limit) {
            return Err(AppError::Invalid(format!(
                "review_round_limit is a number from 0 (no limit) to {ROUND_LIMIT_AT_MOST}, not {}",
                after.review_round_limit
            )));
        }
        if after == before {
            return Ok(after);
        }
        let context = BTreeMap::from([
            ("repository".into(), repository.name.clone()),
            ("review_on_green".into(), after.review_on_green.to_string()),
            (
                "green_wait_minutes".into(),
                after.green_wait_minutes.to_string(),
            ),
            (
                "unbound_close_days".into(),
                after.unbound_close_days.to_string(),
            ),
            ("status_labels".into(), after.status_labels.to_string()),
            ("status_prefix".into(), after.status_prefix.clone()),
            ("work_on_forge".into(), after.work_on_forge.to_string()),
            (
                "review_round_limit".into(),
                after.review_round_limit.to_string(),
            ),
        ]);
        self.record_action(by, "edit_flow", None, None, context, reason)?;
        self.store.set_flow(repository_id, &after)?;
        self.bus
            .publish(Event::RepositoryUpdated(self.repository(repository_id)?));
        Ok(after)
    }

    /// Records that the checks of the followed pull request's head are green, or that the forge reports none at all for
    /// it, the first time they are seen so: the few minutes `review_on_green` waits count from then, and a push, which is
    /// a new head, begins them again.
    pub(in crate::app) fn note_green_head(&self, change: &Change) -> Result<()> {
        if let Some(head) = change.head.as_deref()
            && green(change, head)
        {
            self.store
                .note_green(change.id, head, change.checks.is_none())?;
        }
        Ok(())
    }

    /// `review_on_green`: asks for a round of the Reviewer on the head of the followed pull request once its checks have
    /// been green for the repository's few minutes with no push, as the author's `/done` does; whichever comes first
    /// asks, the other finds the round asked for. A head is asked for once, whatever became of the round; the round limit
    /// holds, and so do a freeze, a hold and a pause (the caller looks at those first).
    pub(in crate::app) fn review_on_green(&self, card: &Card, change: &Change) -> Result<()> {
        let repository = self.repository(card.repository_id)?;
        let flow = self.store.flow(repository.id)?;
        if !flow.review_on_green || repository.forge == "none" {
            return Ok(());
        }
        let Some(head) = change.head.as_deref() else {
            return Ok(());
        };
        if !green(change, head) {
            return Ok(());
        }
        let Some(since) = self.store.green_since(change.id, head)? else {
            return Ok(());
        };
        if now() - since < flow.green_wait_minutes * 60 {
            return Ok(());
        }
        if self.store.external_review_of_head(change.id, head)?
            || self.store.external_reviews(change.id)?.iter().any(|r| {
                matches!(
                    r.state.as_str(),
                    "wanted" | "starting" | "running" | "approved" | "posting"
                )
            })
        {
            return Ok(());
        }
        // The head is asked for once, and so is the owner for it: an Inbox item the owner resolved is not made again
        // at every tick.
        if !self.store.ask_green_review(change.id, head)? {
            return Ok(());
        }
        if flow.rounds_spent(self.store.external_review_rounds(change.id)?) {
            return self.ask_owner_to_look(card, change);
        }
        self.store.add_external_review(change.id, head)?;
        self.answer(
            &repository,
            card.id,
            &if change.checks.is_none() {
                format!(
                    "This version of the pull request has no checks and no push for {} minutes, so the Reviewer reads it now. \
                     Nothing needs to be written for that.",
                    flow.green_wait_minutes
                )
            } else {
                format!(
                    "The checks of this version of the pull request have been green for {} minutes, so the Reviewer reads it now. \
                     Nothing needs to be written for that.",
                    flow.green_wait_minutes
                )
            },
        );
        Ok(())
    }

    /// The followed pull request has a new head: the measurements of the old one that were not judged no longer count.
    /// Says so on the issue when one was waiting for, or being read.
    pub(in crate::app) fn measurement_head_moved(
        &self,
        change: &Change,
        old: &str,
        head: &str,
    ) -> Result<()> {
        let invalid = self.store.invalidate_measurement_reports(change.id, head)?;
        let waiting = self
            .store
            .external_reviews(change.id)?
            .iter()
            .rev()
            .find(|r| r.kind != "person")
            .is_some_and(|r| r.state == "awaiting_measurement" && r.head == old);
        if invalid == 0 && !waiting {
            return Ok(());
        }
        let Some(card) = change.card_id.map(|id| self.card(id)).transpose()? else {
            return Ok(());
        };
        let repository = self.repository(card.repository_id)?;
        self.answer(
            &repository,
            card.id,
            &format!(
                "The pull request has a new commit, so a measurement of {} no longer counts. \
                 Once the code of this version is approved, post the measurement again.",
                short(old)
            ),
        );
        Ok(())
    }
}

/// Whether the checks of `head` count as green for `review_on_green`: the forge read them for this head and found them
/// `success`, or found no check runs or statuses at all. A head that gets checks later is pending or red until they pass.
fn green(change: &Change, head: &str) -> bool {
    change.checks_head.as_deref() == Some(head)
        && matches!(change.checks.as_deref(), Some("success") | None)
}

/// The first seven characters of a commit.
pub(super) fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

/// An edit of a repository's flow settings; what is none stays as it is.
#[derive(Debug, Clone, Default)]
pub struct FlowEdit {
    pub review_on_green: Option<bool>,
    pub green_wait_minutes: Option<i64>,
    pub unbound_close_days: Option<i64>,
    pub status_labels: Option<bool>,
    pub status_prefix: Option<String>,
    pub work_on_forge: Option<bool>,
    pub review_round_limit: Option<i64>,
}
