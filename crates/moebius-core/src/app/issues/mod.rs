//! An issue as a tracker has it: a parent and sub-issues, blocking edges that are written as well as read, labels,
//! claims, comments, closing and reopening, and the queries over all of it. Each change is made on the forge first
//! where the forge keeps that thing (see `repos::forge`), then stored, then announced on the bus; the poll reads the
//! forge's side back through [`App::sync_issues`], which uses the helpers here.
//!
//! What a forge cannot hold, Moebius keeps and the reference says so: claims are Moebius's only (and advisory), GitLab and
//! Forgejo have no parent, so it is a line "Part of #n" in the body, and a blocker is a native link or dependency where
//! the forge has them and a "Blocked by #n" line of the body where it does not (`text`). `sync` is the poll's side: what
//! it reads back, which sub-issues count and which edges are refused.

mod sync;
mod text;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{App, AppError, Result, columns};
use crate::bus::Event;
use crate::decide::Actor;
use crate::repos::forge::Forge;
use crate::store::{Card, Comment, Repository};
pub(crate) use sync::now_millis;
use sync::waits_for;
pub(super) use text::part_of;
use text::{with_blocker_line, with_parent_line, without_blocker};

/// What `GET /v1/repositories/{id}/issues` filters by; every part that is set must match.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueQuery {
    /// The number of the parent issue.
    pub parent: Option<i64>,
    pub label: Option<String>,
    /// `open` or `closed`.
    pub state: Option<String>,
    pub claimed: Option<bool>,
    /// Whether an issue that is not done blocks it.
    pub blocked: Option<bool>,
}

/// A sub-issue as its parent's view shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Child {
    pub id: i64,
    pub number: i64,
    pub title: String,
    /// `open` or `closed`.
    pub state: String,
    pub column: String,
}

/// An issue with what a tracker shows of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueView {
    #[serde(flatten)]
    pub card: Card,
    /// `open` or `closed`.
    pub state: String,
    /// The numbers of the issues that block it and are not done.
    pub blocked_by: Vec<i64>,
    /// Its sub-issues.
    pub children: Vec<Child>,
}

/// Whether a card's issue is closed, as the word.
pub fn state_of(card: &Card) -> &'static str {
    if card.closed { "closed" } else { "open" }
}

impl App {
    /// The sub-issues of `card`, oldest first: the issues that name it as their parent and count as sub-issues (see
    /// [`App::counts_as_sub_issue`]); an outside issue nobody approved is not one.
    pub fn children_of(&self, card: &Card) -> Result<Vec<Child>> {
        Ok(self
            .store
            .children(card.repository_id, card.number)?
            .into_iter()
            .filter(|child| self.counts_as_sub_issue(child))
            .map(|child| Child {
                id: child.id,
                number: child.number,
                title: child.title.clone(),
                state: state_of(&child).into(),
                column: child.column,
            })
            .collect())
    }

    /// The issue `card_id` with its state, open blockers and sub-issues.
    pub fn issue(&self, card_id: i64) -> Result<IssueView> {
        let card = self.card(card_id)?;
        self.issue_view(card)
    }

    fn issue_view(&self, card: Card) -> Result<IssueView> {
        Ok(IssueView {
            state: state_of(&card).into(),
            blocked_by: self.blocked_by(&card)?,
            children: self.children_of(&card)?,
            card,
        })
    }

    /// The issues of a repository that match `query`, oldest first.
    pub fn issues(&self, repository_id: i64, query: &IssueQuery) -> Result<Vec<IssueView>> {
        self.repository(repository_id)?;
        let mut found = Vec::new();
        for card in self.store.cards(repository_id)? {
            if query.parent.is_some_and(|parent| {
                card.parent != Some(parent) || !self.counts_as_sub_issue(&card)
            }) || query
                .label
                .as_ref()
                .is_some_and(|l| !card.labels.contains(l))
                || query.state.as_deref().is_some_and(|s| s != state_of(&card))
                || query
                    .claimed
                    .is_some_and(|c| c != card.claimed_by.is_some())
            {
                continue;
            }
            let view = self.issue_view(card)?;
            if query
                .blocked
                .is_some_and(|b| b != !view.blocked_by.is_empty())
            {
                continue;
            }
            found.push(view);
        }
        Ok(found)
    }

    /// The card of the issue `number` of the card's repository, which a relation names.
    fn related(&self, card: &Card, number: i64, what: &str) -> Result<Card> {
        if number == card.number {
            return Err(AppError::Invalid(format!(
                "an issue cannot be its own {what}"
            )));
        }
        self.store
            .card_by_number(card.repository_id, number)?
            .ok_or_else(|| AppError::NotFound(format!("issue #{number} (the {what})")))
    }

    /// Makes `parent` (an issue number) the parent of the card's issue, or gives it none. Refused for an issue that is
    /// not there, the issue itself, any issue below it, and a parent that waits for the issue (it is blocked by it, or
    /// by an issue that is): the two would wait for each other. On a forge with sub-issues the forge is told; on another
    /// it is the line "Part of #n" of the description that is written. Announces `card_parent_changed`.
    pub fn set_parent(&self, card_id: i64, parent: Option<i64>) -> Result<Card> {
        let card = self.card(card_id)?;
        if card.parent == parent {
            return Ok(card);
        }
        if let Some(parent) = parent {
            self.related(&card, parent, "parent")?;
            // The parent waits for its sub-issues, so a new sub-issue that waits for the parent, or for anything below
            // it, is a cycle.
            if waits_for(&self.waits(card.repository_id)?, card.number, parent) {
                return Err(AppError::Invalid(format!(
                    "#{} waits for #{parent} (it is below it, or blocked by it or something that is), so #{parent} cannot be its parent",
                    card.number
                )));
            }
        }
        let repository = self.repository(card.repository_id)?;
        if repository.forge != "none" {
            let forge = self.forge(&repository)?;
            if forge.has_sub_issues() {
                forge.set_issue_parent(&repository, card.number, parent, card.parent)?;
            } else {
                self.rewrite_body(&card, &repository, forge, |body| {
                    with_parent_line(body, parent)
                })?;
            }
        }
        self.store.set_parent(card_id, parent)?;
        self.wrote_forge(card_id, repository.forge != "none")?;
        self.bus.publish(Event::CardParentChanged {
            card: card_id,
            parent,
        });
        self.card(card_id)
    }

    /// Writes the description of the card's issue as `edit` makes it from the description the forge has now (what the
    /// poll read last when the forge cannot be asked), so that an edit made on the forge in between is not lost. Returns
    /// the description written.
    pub(in crate::app) fn rewrite_body(
        &self,
        card: &Card,
        repository: &Repository,
        forge: &dyn Forge,
        edit: impl Fn(&str) -> String,
    ) -> Result<String> {
        let current = forge
            .issue_body(repository, card.number)
            .unwrap_or_else(|_| card.body.clone());
        let body = edit(&current);
        if body != current {
            forge.update_issue(repository, card.number, &card.title, &body)?;
        }
        self.store.set_card_text(card.id, &card.title, &body)?;
        Ok(body)
    }

    /// Creates an issue with what a tracker gives one at birth: a parent, labels and blockers. What they name is checked
    /// before anything is created on the forge: the parent and the blockers have to be issues of the repository, and
    /// must not make the new issue wait for itself. If a step after the creation still fails (the forge refuses a
    /// label, say), the issue exists and is returned with a warning for each step that failed, not an error.
    pub fn create_tracked_issue(
        &self,
        repository_id: i64,
        title: &str,
        body: &str,
        parent: Option<i64>,
        labels: &[String],
        blocked_by: &[i64],
    ) -> Result<(Card, Vec<String>)> {
        let repository = self.repository(repository_id)?;
        let waits = self.waits(repository_id)?;
        let exists = |number: i64, what: &str| -> Result<()> {
            self.store
                .card_by_number(repository_id, number)?
                .map(|_| ())
                .ok_or_else(|| AppError::NotFound(format!("issue #{number} (the {what})")))
        };
        if let Some(parent) = parent {
            exists(parent, "parent")?;
        }
        for blocker in blocked_by {
            exists(*blocker, "blocker")?;
            // The new issue would wait for the blocker, and its parent would wait for it.
            if parent.is_some_and(|parent| waits_for(&waits, *blocker, parent)) {
                return Err(AppError::Invalid(format!(
                    "#{blocker} cannot block the new issue: its parent waits for it, so they would wait for each other"
                )));
            }
        }
        let card = self.create_held_issue(&repository, title, body, true)?;
        let mut warnings = Vec::new();
        let mut note = |what: String, result: Result<()>| {
            if let Err(e) = result {
                warnings.push(format!("{what}: {e}"));
            }
        };
        if let Some(parent) = parent {
            note(
                format!("could not make #{parent} its parent"),
                self.set_parent(card.id, Some(parent)).map(|_| ()),
            );
        }
        if !labels.is_empty() {
            note(
                "could not set its labels".into(),
                self.set_labels(card.id, labels).map(|_| ()),
            );
        }
        for blocker in blocked_by {
            note(
                format!("could not make #{blocker} block it"),
                self.add_blocker(card.id, *blocker).map(|_| ()),
            );
        }
        // Released only now: a script that saw the card before its blockers were stored could start it unblocked.
        self.store.release_card(card.id)?;
        let card = self.card(card.id)?;
        self.bus.publish(Event::CardAdded(card.clone()));
        Ok((card, warnings))
    }

    /// Replaces the labels of the card's issue (blank ones and repeats are dropped), on the forge too. Announces
    /// `card_labels_changed`.
    pub fn set_labels(&self, card_id: i64, labels: &[String]) -> Result<Card> {
        let card = self.card(card_id)?;
        let labels: BTreeSet<String> = labels
            .iter()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if card.labels.iter().cloned().collect::<BTreeSet<_>>() == labels {
            return Ok(card);
        }
        let repository = self.repository(card.repository_id)?;
        let sorted: Vec<String> = labels.iter().cloned().collect();
        if repository.forge != "none" {
            self.forge(&repository)?
                .set_issue_labels(&repository, card.number, &sorted)?;
        }
        self.store.set_labels(card_id, &labels)?;
        self.wrote_forge(card_id, repository.forge != "none")?;
        self.bus.publish(Event::CardLabelsChanged {
            card: card_id,
            labels: sorted,
        });
        self.card(card_id)
    }

    /// Makes the issue `blocker` (a number) block the card's issue, on the forge too (where the forge keeps no blocking,
    /// as a line "Blocked by #n" of the description). Refused when that makes issues wait for each other, directly or
    /// through others: a blocker that waits for the issue, which includes its parent. Announces `card_blocking_changed`.
    pub fn add_blocker(&self, card_id: i64, blocker: i64) -> Result<Vec<i64>> {
        let card = self.card(card_id)?;
        self.related(&card, blocker, "blocker")?;
        let mut blocked_by: BTreeSet<i64> = self.store.dependencies(card_id)?.into_iter().collect();
        if blocked_by.contains(&blocker) {
            return Ok(blocked_by.into_iter().collect());
        }
        // The new edge closes a cycle when the blocker already waits, through others, for this issue.
        if waits_for(&self.waits(card.repository_id)?, blocker, card.number) {
            return Err(AppError::Invalid(format!(
                "#{blocker} already waits for #{} (it is blocked by it, or is below it), so it cannot block it",
                card.number
            )));
        }
        let repository = self.repository(card.repository_id)?;
        if repository.forge != "none" {
            let forge = self.forge(&repository)?;
            if !forge.set_issue_blocker(&repository, card.number, blocker, true)? {
                self.rewrite_body(&card, &repository, forge, |body| {
                    with_blocker_line(body, blocker)
                })?;
            }
        }
        self.store.add_dependency(card_id, blocker)?;
        self.wrote_forge(card_id, repository.forge != "none")?;
        blocked_by.insert(blocker);
        let blocked_by: Vec<i64> = blocked_by.into_iter().collect();
        self.bus.publish(Event::CardBlockingChanged {
            card: card_id,
            blocked_by: blocked_by.clone(),
        });
        Ok(blocked_by)
    }

    /// Stops the issue `blocker` (a number) from blocking the card's issue, on the forge too. A blocker the description
    /// names ("Blocked by #n") is taken out of it. Announces `card_blocking_changed`.
    pub fn remove_blocker(&self, card_id: i64, blocker: i64) -> Result<Vec<i64>> {
        let card = self.card(card_id)?;
        let mut blocked_by: BTreeSet<i64> = self.store.dependencies(card_id)?.into_iter().collect();
        if !blocked_by.contains(&blocker) {
            return Err(AppError::NotFound(format!(
                "#{blocker} among the blockers of #{}",
                card.number
            )));
        }
        let repository = self.repository(card.repository_id)?;
        if repository.forge != "none" {
            let forge = self.forge(&repository)?;
            let named = blockers_in(&card.body).contains(&blocker);
            let removed = forge.set_issue_blocker(&repository, card.number, blocker, false);
            // The forge may hold nothing for a blocker that only the description names.
            if !named {
                removed?;
            }
            if named {
                self.rewrite_body(&card, &repository, forge, |body| {
                    without_blocker(body, blocker)
                })?;
            }
        }
        self.store.remove_dependency(card_id, blocker)?;
        self.wrote_forge(card_id, repository.forge != "none")?;
        blocked_by.remove(&blocker);
        let blocked_by: Vec<i64> = blocked_by.into_iter().collect();
        self.bus.publish(Event::CardBlockingChanged {
            card: card_id,
            blocked_by: blocked_by.clone(),
        });
        Ok(blocked_by)
    }

    /// Claims the card's issue for `claimer`, or for the actor when none is named, so that two sessions do not take
    /// the same issue. Claiming again as the same claimer changes nothing; an issue someone else claimed is a
    /// conflict. Claims are Moebius's own, not the forge's assignees. Announces `card_claimed`.
    pub fn claim_issue(&self, by: &Actor, card_id: i64, claimer: Option<&str>) -> Result<Card> {
        let card = self.card(card_id)?;
        let claimer = claimer
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map_or_else(|| by.decider(), str::to_string);
        if !self.store.claim_card(card_id, &claimer)? {
            return Err(AppError::Conflict(format!(
                "#{} is claimed by {} already",
                card.number,
                card.claimed_by.unwrap_or_default()
            )));
        }
        if card.claimed_by.is_none() {
            self.bus.publish(Event::CardClaimed {
                card: card_id,
                by: claimer,
            });
        }
        self.card(card_id)
    }

    /// Releases the claim on the card's issue. A person may release any claim; the Moebius agent only the one it made
    /// (as itself, or under the name `claimer` that it claimed by), which is a conflict with a forbidden for any other.
    /// An issue nobody claimed is a conflict. Claims are advisory, so this keeps honest sessions from releasing each
    /// other's, nothing more. Announces `card_released`.
    pub fn release_issue(&self, by: &Actor, card_id: i64, claimer: Option<&str>) -> Result<Card> {
        let card = self.card(card_id)?;
        let Some(holder) = card.claimed_by.as_deref() else {
            return Err(AppError::Conflict(format!(
                "#{} is not claimed",
                card.number
            )));
        };
        let own = by.decider();
        if !matches!(by, Actor::Person) && holder != own && claimer.map(str::trim) != Some(holder) {
            return Err(AppError::Forbidden(format!(
                "#{} is claimed by {holder}: only they or a person can release it",
                card.number
            )));
        }
        self.store.release_claim(card_id)?;
        self.bus.publish(Event::CardReleased { card: card_id });
        self.card(card_id)
    }

    /// Writes a comment on the card's issue as the actor, on the forge too (where it is the forge's own identity that
    /// writes it). Secrets are left out of it. Announces `card_commented`. A comment the poll stored first (it read it
    /// from the forge between the two) is returned as it is stored. Comments edited or deleted on the forge are not
    /// changed or removed here.
    pub fn comment_issue(&self, by: &Actor, card_id: i64, body: &str) -> Result<Comment> {
        let card = self.card(card_id)?;
        let body = crate::blobs::redact(body.trim());
        if body.is_empty() {
            return Err(AppError::Invalid("a comment needs text".into()));
        }
        let repository = self.repository(card.repository_id)?;
        let forge_id = if repository.forge != "none" {
            Some(
                self.forge(&repository)?
                    .comment_issue(&repository, card.number, &body)?,
            )
        } else {
            None
        };
        match self
            .store
            .add_local_comment(card_id, &by.decider(), &body, forge_id)?
        {
            Some(comment) => {
                self.bus.publish(Event::CardCommented {
                    card: card_id,
                    comment: comment.clone(),
                });
                Ok(comment)
            }
            // The poll read it from the forge before this call stored it: it is that one, and it was announced.
            None => forge_id
                .and_then(|forge_id| {
                    self.store
                        .comment_by_forge_id(card_id, forge_id)
                        .transpose()
                })
                .transpose()?
                .ok_or_else(|| AppError::Conflict("the comment is stored already".into())),
        }
    }

    /// The comments of the card's issue, oldest first.
    pub fn comments(&self, card_id: i64) -> Result<Vec<Comment>> {
        self.card(card_id)?;
        Ok(self.store.comments(card_id)?)
    }

    /// Closes the card's issue on a person's or the agent's word: it is closed on the forge, its card goes to Done and
    /// its blockers count as resolved. Refused while a run of the card is going: stop it first (block the card).
    /// Recorded as a `close_issue` decision; announces `card_closed`.
    pub fn close_issue(&self, by: &Actor, card_id: i64, reason: &str) -> Result<Card> {
        let card = self.card(card_id)?;
        if self
            .store
            .runs_for_card(card_id)?
            .iter()
            .any(|run| run.ended_at.is_none())
        {
            return Err(AppError::Conflict(format!(
                "a run of #{} is going; block the card to stop it first",
                card.number
            )));
        }
        let closed = self.close_card(card_id)?;
        let context = BTreeMap::from([("title".into(), card.title.clone())]);
        self.record_action(by, "close_issue", Some(card_id), None, context, reason)?;
        Ok(closed)
    }

    /// Closes the card's issue and moves its card to Done, without a decision (a column script does it too). Says
    /// nothing and does nothing for an issue that is closed already.
    pub(crate) fn close_card(&self, card_id: i64) -> Result<Card> {
        let card = self.card(card_id)?;
        if self.store.issue_closed(card_id)? {
            return Ok(card);
        }
        let repository = self.repository(card.repository_id)?;
        if repository.forge != "none" {
            self.forge(&repository)?
                .set_issue_open(&repository, card.number, false)?;
        }
        self.store.set_issue_closed(card_id, true)?;
        self.sub_issue_closed(&card)?;
        if card.column != columns::DONE {
            self.move_card(card_id, columns::DONE)?;
        }
        self.wrote_forge(card_id, repository.forge != "none")?;
        self.close_settled(card_id);
        self.bus.publish(Event::CardClosed { card: card_id });
        self.card(card_id)
    }

    /// Gives the card's issue the labels a column script asked for (`moebius.set_label`) and takes away those it asked
    /// to remove, on the forge too; the others stay as they are. Does nothing when the issue is as asked. A forge that
    /// refuses is told once as a forge error and not asked again for a while, and the card goes on: a label is a mark,
    /// not a step of the work.
    pub(crate) fn set_labels_for_script(
        &self,
        card_id: i64,
        changes: &[(String, bool)],
    ) -> Result<()> {
        let mut labels: BTreeSet<String> = self.card(card_id)?.labels.into_iter().collect();
        let before = labels.clone();
        for (label, present) in changes {
            if *present {
                labels.insert(label.clone());
            } else {
                labels.retain(|l| !l.eq_ignore_ascii_case(label));
            }
        }
        if labels == before {
            return Ok(());
        }
        let retry = self.close_retry;
        if self
            .label_refused
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&card_id)
            .is_some_and(|at| at.elapsed() < retry)
        {
            return Ok(());
        }
        match self.set_labels(card_id, &labels.into_iter().collect::<Vec<_>>()) {
            Ok(_) => {
                self.label_refused
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&card_id);
                Ok(())
            }
            Err(AppError::Forge(e)) => {
                let card = self.card(card_id)?;
                let first = self
                    .label_refused
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(card_id, std::time::Instant::now())
                    .is_none();
                if first {
                    self.bus.publish(Event::ForgeError {
                        repository: self.repository(card.repository_id)?.name,
                        message: format!("the forge would not label #{}: {e}", card.number),
                    });
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Closes the card's issue because its column script said so (a parent whose sub-issues are all done). A parent a
    /// person reopened is not closed again for the sub-issues that were done already: it stays open, and waits, until
    /// one of its sub-issues closes after that. A forge that refuses is not asked again for a while, so that the board
    /// does not ask at every tick: the reason is told once as a forge error, however often the board asks again later,
    /// and then given back, to be shown as what the card waits for. A person closing the issue, the poll seeing it
    /// closed or the card deleted ends the wait. Says why it could not, or none when it did.
    pub(crate) fn close_for_script(&self, card_id: i64) -> Result<Option<String>> {
        if self.store.kept_open(card_id)? {
            return Ok(Some(
                "reopened: it is closed again when a sub-issue closes, or by hand".into(),
            ));
        }
        let retry = self.close_retry;
        if let Some((_, reason)) = self
            .close_refused
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&card_id)
            .filter(|(at, _)| at.elapsed() < retry)
        {
            return Ok(Some(reason.clone()));
        }
        match self.close_card(card_id) {
            Ok(_) => Ok(None),
            Err(AppError::Forge(e)) => {
                let card = self.card(card_id)?;
                let reason = format!("the forge would not close #{}: {e}", card.number);
                let first = self
                    .close_refused
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(card_id, (std::time::Instant::now(), reason.clone()))
                    .is_none();
                if first {
                    self.bus.publish(Event::ForgeError {
                        repository: self.repository(card.repository_id)?.name,
                        message: reason.clone(),
                    });
                }
                Ok(Some(reason))
            }
            Err(e) => Err(e),
        }
    }

    /// Forgets that the forge refused to close the issue of the card: it is closed, or the card is gone.
    pub(in crate::app) fn close_settled(&self, card_id: i64) {
        self.close_refused
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&card_id);
    }

    /// Reopens a closed issue, on the forge too: its card goes back to the backlog, where the workflow takes it up as
    /// it does a new issue. An issue that is open is a conflict. Recorded as a `reopen_issue` decision; announces
    /// `card_reopened`.
    pub fn reopen_issue(&self, by: &Actor, card_id: i64, reason: &str) -> Result<Card> {
        let card = self.card(card_id)?;
        if !card.closed {
            return Err(AppError::Conflict(format!("#{} is open", card.number)));
        }
        let repository = self.repository(card.repository_id)?;
        if repository.forge != "none" {
            self.forge(&repository)?
                .set_issue_open(&repository, card.number, true)?;
        }
        self.reopened(card_id)?;
        self.wrote_forge(card_id, repository.forge != "none")?;
        let context = BTreeMap::from([("title".into(), card.title.clone())]);
        self.record_action(by, "reopen_issue", Some(card_id), None, context, reason)?;
        self.card(card_id)
    }

    /// What reopening does here: the issue is no longer closed and a card in Done goes back to the backlog.
    pub(in crate::app) fn reopened(&self, card_id: i64) -> Result<()> {
        self.store.set_issue_closed(card_id, false)?;
        // Whoever reopened it wants it open: it is not closed again for the sub-issues that are done already.
        self.store.set_kept_open(card_id, true)?;
        if self.card(card_id)?.column == columns::DONE {
            self.move_card(card_id, columns::BACKLOG)?;
        }
        self.bus.publish(Event::CardReopened { card: card_id });
        Ok(())
    }
}

/// The issue numbers a body says block its issue ("Blocked by #12").
pub(super) fn blockers_in(body: &str) -> BTreeSet<i64> {
    super::cards::blocked_by(body)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use moebius_plugin_api::Host;
    use tokio::sync::broadcast::Receiver;

    use super::*;
    use crate::bus::AppEvent;
    use crate::repos::forge::{Forge, ForgeError, Issue};
    use crate::store::{Change, Repository, Store};

    /// A forge that records what it is asked to do, and fails when `failing` is set.
    struct Recording {
        calls: Arc<Mutex<Vec<String>>>,
        sub_issues: bool,
        failing: Arc<Mutex<bool>>,
        /// Whether the forge keeps blocking itself (GitLab without Premium does not).
        blocking: Arc<Mutex<bool>>,
        /// The description the forge has now, when it is asked for it.
        body: Arc<Mutex<Option<String>>>,
        /// Whether it refuses labels.
        labels_refused: Arc<Mutex<bool>>,
        /// Called when the forge is asked to set a blocker, so that a test can look at the store while a new issue
        /// is still being set up.
        on_blocker: BlockerHook,
    }

    /// What a test does when the forge is asked to set a blocker.
    type BlockerHook = Arc<Mutex<Option<Arc<dyn Fn(i64) + Send + Sync>>>>;

    impl Recording {
        fn note(&self, call: String) -> std::result::Result<(), ForgeError> {
            if *self.failing.lock().unwrap() {
                return Err(ForgeError::Invalid("the forge said no".into()));
            }
            self.calls.lock().unwrap().push(call);
            Ok(())
        }
    }

    impl Forge for Recording {
        fn name(&self) -> &'static str {
            "github"
        }
        fn publish(
            &self,
            _: &Repository,
            _: &str,
            _: &str,
            _: &str,
            _: Option<i64>,
        ) -> std::result::Result<Option<i64>, ForgeError> {
            Ok(None)
        }
        fn merge(
            &self,
            _: &Repository,
            _: &Change,
            _: &str,
        ) -> std::result::Result<String, ForgeError> {
            Ok(String::new())
        }
        fn refresh(&self, _: &Repository) -> std::result::Result<(), ForgeError> {
            Ok(())
        }
        fn update_issue(
            &self,
            _: &Repository,
            number: i64,
            title: &str,
            body: &str,
        ) -> std::result::Result<(), ForgeError> {
            self.note(format!("update #{number} {title:?} {body:?}"))
        }
        fn set_issue_open(
            &self,
            _: &Repository,
            number: i64,
            open: bool,
        ) -> std::result::Result<(), ForgeError> {
            self.note(format!("#{number} open={open}"))
        }
        fn create_issue(
            &self,
            _: &Repository,
            title: &str,
            _: &str,
        ) -> std::result::Result<i64, ForgeError> {
            let number = 10 + self.calls.lock().unwrap().len() as i64;
            self.note(format!("create {title:?}"))?;
            Ok(number)
        }
        fn set_issue_labels(
            &self,
            _: &Repository,
            number: i64,
            labels: &[String],
        ) -> std::result::Result<(), ForgeError> {
            if *self.labels_refused.lock().unwrap() {
                return Err(ForgeError::Invalid("no such label".into()));
            }
            self.note(format!("#{number} labels={}", labels.join(",")))
        }
        fn comment_issue(
            &self,
            _: &Repository,
            number: i64,
            body: &str,
        ) -> std::result::Result<i64, ForgeError> {
            self.note(format!("#{number} comment {body:?}"))?;
            Ok(500)
        }
        fn has_sub_issues(&self) -> bool {
            self.sub_issues
        }
        fn set_issue_parent(
            &self,
            _: &Repository,
            number: i64,
            parent: Option<i64>,
            previous: Option<i64>,
        ) -> std::result::Result<(), ForgeError> {
            self.note(format!("#{number} parent={parent:?} previous={previous:?}"))
        }
        fn set_issue_blocker(
            &self,
            _: &Repository,
            number: i64,
            blocker: i64,
            blocks: bool,
        ) -> std::result::Result<bool, ForgeError> {
            if let Some(hook) = self.on_blocker.lock().unwrap().clone() {
                hook(number);
            }
            self.note(format!("#{number} blocker #{blocker} {blocks}"))?;
            Ok(*self.blocking.lock().unwrap())
        }
        fn issue_body(&self, _: &Repository, _: i64) -> std::result::Result<String, ForgeError> {
            self.body
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| ForgeError::Invalid("not asked".into()))
        }
    }

    struct Setup {
        _dir: tempfile::TempDir,
        app: App,
        repository: Repository,
        calls: Arc<Mutex<Vec<String>>>,
        failing: Arc<Mutex<bool>>,
        blocking: Arc<Mutex<bool>>,
        body: Arc<Mutex<Option<String>>>,
        labels_refused: Arc<Mutex<bool>>,
        on_blocker: BlockerHook,
        events: Receiver<AppEvent>,
    }

    impl Setup {
        /// A repository without a forge, or on a forge that records its calls (with sub-issues when `sub_issues`).
        fn new(forge: Option<bool>) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
            let mut app = App::new(
                store,
                Arc::new(Host::new(Duration::from_secs(1))),
                &dir.path().join("data"),
            );
            let calls = Arc::new(Mutex::new(Vec::new()));
            let failing = Arc::new(Mutex::new(false));
            let blocking = Arc::new(Mutex::new(true));
            let body = Arc::new(Mutex::new(None));
            let labels_refused = Arc::new(Mutex::new(false));
            let on_blocker: BlockerHook = Arc::new(Mutex::new(None));
            let repository = match forge {
                None => app.store.add_repository("w", "/w", "none", "main").unwrap(),
                Some(sub_issues) => {
                    app.add_forge(Box::new(Recording {
                        calls: calls.clone(),
                        sub_issues,
                        failing: failing.clone(),
                        blocking: blocking.clone(),
                        body: body.clone(),
                        labels_refused: labels_refused.clone(),
                        on_blocker: on_blocker.clone(),
                    }));
                    app.store
                        .add_github_repository("w", "/w", "main", "acme/w", &["person".into()])
                        .unwrap()
                }
            };
            let events = app.bus.subscribe();
            Self {
                _dir: dir,
                app,
                repository,
                calls,
                failing,
                blocking,
                body,
                labels_refused,
                on_blocker,
                events,
            }
        }

        fn card(&self, title: &str) -> Card {
            self.card_by(title, "person")
        }

        /// A card in the backlog written by `author`.
        fn card_by(&self, title: &str, author: &str) -> Card {
            self.app
                .store
                .add_numbered_card(
                    self.repository.id,
                    self.app.store.cards(self.repository.id).unwrap().len() as i64 + 1,
                    title,
                    "",
                    author,
                    columns::BACKLOG,
                )
                .unwrap()
        }

        /// The kinds of the events since the last call.
        fn kinds(&mut self) -> Vec<String> {
            let mut kinds = Vec::new();
            while let Ok(event) = self.events.try_recv() {
                kinds.push(event.kind);
            }
            kinds
        }

        fn calls(&self) -> Vec<String> {
            std::mem::take(&mut self.calls.lock().unwrap())
        }
    }

    #[test]
    fn sub_issues_nest_without_cycles_and_a_parent_lists_them_with_their_state() {
        let mut s = Setup::new(None);
        let (map, a, b) = (s.card("Map"), s.card("A"), s.card("B"));
        let a = s.app.set_parent(a.id, Some(map.number)).unwrap();
        s.app.set_parent(b.id, Some(a.number)).unwrap();
        assert_eq!(s.kinds(), ["card_parent_changed", "card_parent_changed"]);

        // Setting what is set changes nothing and says nothing.
        s.app.set_parent(a.id, Some(map.number)).unwrap();
        assert!(s.kinds().is_empty());

        for (card, parent) in [(map.id, map.number), (map.id, b.number)] {
            let refused = s.app.set_parent(card, Some(parent)).unwrap_err();
            assert!(matches!(refused, AppError::Invalid(_)), "{refused}");
        }
        assert!(matches!(
            s.app.set_parent(a.id, Some(99)),
            Err(AppError::NotFound(_))
        ));

        s.app.close_card(b.id).unwrap();
        let children = s.app.issue(a.id).unwrap().children;
        assert_eq!(
            children
                .iter()
                .map(|c| (c.number, c.state.as_str(), c.column.as_str()))
                .collect::<Vec<_>>(),
            [(b.number, "closed", "Done")]
        );
        s.app.set_parent(a.id, None).unwrap();
        assert_eq!(s.app.card(a.id).unwrap().parent, None);
    }

    #[test]
    fn blocking_is_written_cycles_are_refused_and_a_closed_blocker_resolves() {
        let mut s = Setup::new(None);
        let (a, b, c) = (s.card("A"), s.card("B"), s.card("C"));
        assert_eq!(s.app.add_blocker(b.id, a.number).unwrap(), [a.number]);
        s.app.add_blocker(c.id, b.number).unwrap();
        // Adding what is there changes nothing.
        s.app.add_blocker(c.id, b.number).unwrap();
        assert_eq!(
            s.kinds(),
            ["card_blocking_changed", "card_blocking_changed"]
        );

        for (card, blocker) in [(a.id, a.number), (a.id, b.number), (a.id, c.number)] {
            let refused = s.app.add_blocker(card, blocker).unwrap_err();
            assert!(matches!(refused, AppError::Invalid(_)), "{refused}");
        }
        assert_eq!(s.app.issue(c.id).unwrap().blocked_by, [b.number]);

        s.app.close_card(b.id).unwrap();
        assert!(s.app.issue(c.id).unwrap().blocked_by.is_empty());
        assert_eq!(s.app.issue(b.id).unwrap().blocked_by, [a.number]);
        assert_eq!(
            s.app.remove_blocker(b.id, a.number).unwrap(),
            [] as [i64; 0]
        );
        assert!(matches!(
            s.app.remove_blocker(b.id, a.number),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn labels_claims_and_comments_are_kept_and_announced() {
        let mut s = Setup::new(None);
        let a = s.card("A");
        let labels = ["wayfinder:spike", " bug ", "bug", ""].map(String::from);
        let card = s.app.set_labels(a.id, &labels).unwrap();
        assert_eq!(card.labels, ["bug", "wayfinder:spike"]);
        s.app
            .set_labels(a.id, &["bug".into(), "wayfinder:spike".into()])
            .unwrap();

        let agent = Actor::Agent {
            name: "Moebius".into(),
            run: 4,
        };
        assert_eq!(
            s.app
                .claim_issue(&agent, a.id, None)
                .unwrap()
                .claimed_by
                .as_deref(),
            Some("agent:Moebius")
        );
        s.app.claim_issue(&agent, a.id, None).unwrap();
        let taken = s.app.claim_issue(&Actor::Person, a.id, Some("session 2"));
        assert!(matches!(taken, Err(AppError::Conflict(_))), "{taken:?}");
        s.app.release_issue(&Actor::Person, a.id, None).unwrap();
        assert!(matches!(
            s.app.release_issue(&Actor::Person, a.id, None),
            Err(AppError::Conflict(_))
        ));
        let card = s
            .app
            .claim_issue(&Actor::Person, a.id, Some(" session 2 "))
            .unwrap();
        assert_eq!(card.claimed_by.as_deref(), Some("session 2"));

        let comment = s
            .app
            .comment_issue(
                &agent,
                a.id,
                "  Decided: use a queue. sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789  ",
            )
            .unwrap();
        assert_eq!(comment.author, "agent:Moebius");
        assert!(comment.body.starts_with("Decided: use a queue."));
        assert!(
            !comment
                .body
                .contains("abcdefghijklmnopqrstuvwxyz0123456789")
        );
        assert!(matches!(
            s.app.comment_issue(&agent, a.id, "  "),
            Err(AppError::Invalid(_))
        ));
        assert_eq!(s.app.comments(a.id).unwrap(), [comment]);
        assert_eq!(
            s.kinds(),
            [
                "card_labels_changed",
                "card_claimed",
                "card_released",
                "card_claimed",
                "card_commented"
            ]
        );
    }

    #[test]
    fn closing_moves_the_card_to_done_and_reopening_takes_it_back_to_the_backlog() {
        let mut s = Setup::new(None);
        let a = s.card("A");
        s.app.move_card(a.id, columns::IN_PROGRESS).unwrap();
        s.kinds();

        // A run that is going has to be stopped first.
        let run = s
            .app
            .store
            .add_run(Some(a.id), "Coder", "test", "code")
            .unwrap();
        assert!(matches!(
            s.app.close_issue(&Actor::Person, a.id, ""),
            Err(AppError::Conflict(_))
        ));
        s.app.store.set_run_status(run.id, "succeeded").unwrap();

        let closed = s
            .app
            .close_issue(&Actor::Person, a.id, "not needed")
            .unwrap();
        assert_eq!((closed.column.as_str(), closed.closed), ("Done", true));
        s.app.close_issue(&Actor::Person, a.id, "again").unwrap();
        assert_eq!(s.kinds(), ["card_moved", "card_closed"]);
        assert_eq!(s.app.store.judgements("close_issue").unwrap().len(), 2);

        let reopened = s
            .app
            .reopen_issue(&Actor::Person, a.id, "it is needed")
            .unwrap();
        assert_eq!(
            (reopened.column.as_str(), reopened.closed),
            ("Backlog", false)
        );
        assert!(matches!(
            s.app.reopen_issue(&Actor::Person, a.id, ""),
            Err(AppError::Conflict(_))
        ));
        assert_eq!(s.kinds(), ["card_moved", "card_reopened"]);
    }

    #[test]
    fn issues_are_queried_by_parent_label_state_claim_and_blocking() {
        let s = Setup::new(None);
        let (map, a, b, c) = (s.card("Map"), s.card("A"), s.card("B"), s.card("C"));
        for card in [&a, &b, &c] {
            s.app.set_parent(card.id, Some(map.number)).unwrap();
        }
        s.app.set_labels(a.id, &["wayfinder:spike".into()]).unwrap();
        s.app.add_blocker(b.id, a.number).unwrap();
        s.app.claim_issue(&Actor::Person, c.id, None).unwrap();
        let numbers = |query: IssueQuery| -> Vec<i64> {
            s.app
                .issues(s.repository.id, &query)
                .unwrap()
                .iter()
                .map(|i| i.card.number)
                .collect()
        };
        let under = |query: IssueQuery| {
            numbers(IssueQuery {
                parent: Some(map.number),
                ..query
            })
        };
        assert_eq!(under(IssueQuery::default()), [a.number, b.number, c.number]);
        assert_eq!(
            under(IssueQuery {
                label: Some("wayfinder:spike".into()),
                ..Default::default()
            }),
            [a.number]
        );
        // The frontier: open, unblocked and unclaimed.
        assert_eq!(
            under(IssueQuery {
                state: Some("open".into()),
                blocked: Some(false),
                claimed: Some(false),
                ..Default::default()
            }),
            [a.number]
        );
        s.app.close_card(a.id).unwrap();
        assert_eq!(
            under(IssueQuery {
                state: Some("open".into()),
                blocked: Some(false),
                claimed: Some(false),
                ..Default::default()
            }),
            [b.number]
        );
        assert_eq!(
            numbers(IssueQuery {
                state: Some("closed".into()),
                ..Default::default()
            }),
            [a.number]
        );
    }

    #[test]
    fn a_forge_is_told_first_and_a_forge_that_refuses_leaves_the_issue_as_it_was() {
        let mut s = Setup::new(Some(false));
        let (a, b, p) = (s.card("A"), s.card("B"), s.card("P"));
        s.app.set_labels(a.id, &["bug".into()]).unwrap();
        s.app.add_blocker(b.id, a.number).unwrap();
        s.app.close_card(a.id).unwrap();
        s.app.comment_issue(&Actor::Person, a.id, "Done.").unwrap();
        // GitLab and Forgejo have no parent: the body says it.
        s.app.set_parent(b.id, Some(p.number)).unwrap();
        s.app.reopen_issue(&Actor::Person, a.id, "").unwrap();
        assert_eq!(
            s.calls(),
            [
                "#1 labels=bug",
                "#2 blocker #1 true",
                "#1 open=false",
                "#1 comment \"Done.\"",
                "update #2 \"B\" \"Part of #3\"",
                "#1 open=true",
            ]
        );
        assert_eq!(s.app.card(b.id).unwrap().body, "Part of #3");
        s.app.set_parent(b.id, None).unwrap();
        assert_eq!(s.app.card(b.id).unwrap().body, "");
        s.calls();

        let c = s.card("C");
        *s.failing.lock().unwrap() = true;
        s.kinds();
        assert!(matches!(
            s.app.set_labels(a.id, &[]),
            Err(AppError::Forge(_))
        ));
        assert!(matches!(
            s.app.add_blocker(c.id, a.number),
            Err(AppError::Forge(_))
        ));
        assert!(matches!(s.app.close_card(b.id), Err(AppError::Forge(_))));
        assert!(matches!(
            s.app.comment_issue(&Actor::Person, a.id, "x"),
            Err(AppError::Forge(_))
        ));
        assert_eq!(s.app.card(a.id).unwrap().labels, ["bug"]);
        assert!(!s.app.card(b.id).unwrap().closed);
        assert_eq!(s.app.comments(a.id).unwrap().len(), 1);
        assert!(s.kinds().is_empty());
    }

    #[test]
    fn a_forge_with_sub_issues_keeps_the_parent_itself_and_a_blocker_the_body_names_is_edited_out_of_it()
     {
        let s = Setup::new(Some(true));
        let (a, b) = (s.card("A"), s.card("B"));
        s.app.set_parent(b.id, Some(a.number)).unwrap();
        s.app.set_parent(b.id, None).unwrap();
        assert_eq!(
            s.calls(),
            [
                "#2 parent=Some(1) previous=None",
                "#2 parent=None previous=Some(1)"
            ]
        );
        assert_eq!(s.app.card(b.id).unwrap().body, "");

        s.app
            .sync_issues(
                &s.repository,
                &[Issue {
                    number: 2,
                    title: "B".into(),
                    body: "Text.\n\nBlocked by #1".into(),
                    open: true,
                    ..Issue::default()
                }],
            )
            .unwrap();
        assert_eq!(s.app.issue(b.id).unwrap().blocked_by, [a.number]);
        // The description is what holds it, so it is taken out of the description, as well as the forge's own link.
        s.app.remove_blocker(b.id, a.number).unwrap();
        assert_eq!(
            s.calls(),
            ["#2 blocker #1 false", "update #2 \"B\" \"Text.\""]
        );
        assert_eq!(s.app.card(b.id).unwrap().body, "Text.");
        assert!(s.app.store.dependencies(b.id).unwrap().is_empty());
    }

    #[test]
    fn the_poll_copies_state_parent_labels_blockers_and_comments_and_announces_each_change() {
        let mut s = Setup::new(Some(true));
        let issue = |number: i64, open: bool| Issue {
            number,
            title: format!("Issue {number}"),
            open,
            author: "leon".into(),
            ..Issue::default()
        };
        let mut two = issue(2, true);
        two.parent = Some(1);
        two.labels = vec!["wayfinder:decision".into(), "bug".into()];
        two.blocked_by = Some(vec![3]);
        two.comment_count = 1;
        two.comments = Some(vec![crate::repos::forge::Comment {
            id: 71,
            author: "leon".into(),
            body: "Why?".into(),
            at: 1_700_000_000,
        }]);
        s.app
            .sync_issues(
                &s.repository,
                &[issue(1, true), two.clone(), issue(3, true)],
            )
            .unwrap();
        let card = s
            .app
            .store
            .card_by_number(s.repository.id, 2)
            .unwrap()
            .unwrap();
        assert_eq!(
            (card.parent, &card.labels, card.closed),
            (
                Some(1),
                &vec!["bug".to_string(), "wayfinder:decision".into()],
                false
            )
        );
        assert_eq!(s.app.issue(card.id).unwrap().blocked_by, [3]);
        assert_eq!(s.app.comments(card.id).unwrap()[0].at, 1_700_000_000);
        let kinds = s.kinds();
        for expected in [
            "card_added",
            "card_parent_changed",
            "card_labels_changed",
            "card_blocking_changed",
            "card_commented",
        ] {
            assert!(
                kinds.iter().any(|k| k == expected),
                "{expected} in {kinds:?}"
            );
        }

        // Read again: nothing changed, so nothing is said and the comment is not stored twice.
        s.app.sync_issues(&s.repository, &[two.clone()]).unwrap();
        assert!(s.kinds().is_empty());
        assert_eq!(s.app.comments(card.id).unwrap().len(), 1);
        assert_eq!(s.app.store.forge_comment_count(card.id).unwrap(), 1);

        // Closed on the forge: Done; reopened there: back in the backlog. A card in Done because its change was merged
        // is not reopened by an issue the forge left open.
        let mut closed = two.clone();
        closed.open = false;
        s.app.sync_issues(&s.repository, &[closed.clone()]).unwrap();
        assert_eq!(s.app.card(card.id).unwrap().column, "Done");
        assert_eq!(s.kinds(), ["card_moved", "card_closed"]);
        s.app.sync_issues(&s.repository, &[two.clone()]).unwrap();
        assert_eq!(s.app.card(card.id).unwrap().column, "Backlog");
        assert_eq!(s.kinds(), ["card_moved", "card_reopened"]);
        s.app.move_card(card.id, columns::DONE).unwrap();
        s.app.sync_issues(&s.repository, &[two]).unwrap();
        assert_eq!(s.app.card(card.id).unwrap().column, "Done");
    }

    #[test]
    fn editing_an_issue_keeps_the_blockers_and_the_parent_the_forge_holds() {
        let s = Setup::new(Some(false));
        let (a, b, c) = (s.card("A"), s.card("B"), s.card("C"));
        s.app.set_parent(b.id, Some(a.number)).unwrap();
        s.app.add_blocker(b.id, c.number).unwrap();
        s.app
            .edit_card(&Actor::Person, b.id, "B", "New text.\n\nPart of #1", "")
            .unwrap();
        let card = s.app.card(b.id).unwrap();
        assert_eq!(card.parent, Some(1));
        assert_eq!(s.app.store.dependencies(b.id).unwrap(), [3]);
        s.app
            .edit_card(&Actor::Person, b.id, "B", "Plain.", "")
            .unwrap();
        assert_eq!(s.app.card(b.id).unwrap().parent, None);
        assert_eq!(s.app.store.dependencies(b.id).unwrap(), [3]);
    }

    /// The forge's copy of issue `number` as the poll reads it: open, with nothing besides a title.
    fn open_issue(number: i64, title: &str) -> Issue {
        Issue {
            number,
            title: title.into(),
            open: true,
            author: "person".into(),
            blocked_by: Some(Vec::new()),
            ..Issue::default()
        }
    }

    #[test]
    fn a_card_in_done_whose_issue_the_forge_left_open_stays_in_done_through_a_poll() {
        // What a migrated store has: a merged card in Done whose issue was never closed on its forge.
        let mut s = Setup::new(Some(true));
        let a = s.card("A");
        s.app.move_card(a.id, columns::DONE).unwrap();
        assert!(!s.app.store.issue_closed(a.id).unwrap());
        s.kinds();
        s.app
            .sync_issues(&s.repository, &[open_issue(1, "A")])
            .unwrap();
        assert_eq!(s.app.card(a.id).unwrap().column, "Done");
        assert!(s.kinds().is_empty());
        assert!(s.calls().is_empty());
    }

    #[test]
    fn an_outside_issue_nobody_approved_is_no_sub_issue_whatever_its_description_says() {
        let s = Setup::new(Some(true));
        let parent = s.card("Parent");
        let mut outside = open_issue(2, "Outside");
        outside.author = "mallory".into();
        outside.body = "Part of #1".into();
        s.app
            .sync_issues(&s.repository, &[open_issue(1, "Parent"), outside.clone()])
            .unwrap();
        let child = s
            .app
            .store
            .card_by_number(s.repository.id, 2)
            .unwrap()
            .unwrap();
        // It names its parent, which the board shows, but it does not count for it.
        assert_eq!(child.parent, Some(1));
        assert!(s.app.issue(parent.id).unwrap().children.is_empty());
        let under = IssueQuery {
            parent: Some(1),
            ..Default::default()
        };
        assert!(s.app.issues(s.repository.id, &under).unwrap().is_empty());

        // Closing it does not leave the parent without open sub-issues either.
        outside.open = false;
        s.app.sync_issues(&s.repository, &[outside]).unwrap();
        assert!(s.app.issue(parent.id).unwrap().children.is_empty());

        // A person approving it makes it one; so does the issue of a trusted author.
        s.app.reopen_issue(&Actor::Person, child.id, "").unwrap();
        s.app
            .approve_issue(&Actor::Person, child.id, "fine")
            .unwrap();
        assert_eq!(s.app.issue(parent.id).unwrap().children.len(), 1);
        let mut trusted = open_issue(3, "Trusted");
        trusted.body = "Part of #1".into();
        s.app.sync_issues(&s.repository, &[trusted]).unwrap();
        assert_eq!(s.app.issue(parent.id).unwrap().children.len(), 2);
    }

    #[test]
    fn a_forge_that_could_not_be_read_for_blockers_leaves_the_ones_it_keeps() {
        let s = Setup::new(Some(true));
        let mut two = open_issue(2, "Two");
        two.blocked_by = Some(vec![3]);
        s.app.sync_issues(&s.repository, &[two.clone()]).unwrap();
        let card = s
            .app
            .store
            .card_by_number(s.repository.id, 2)
            .unwrap()
            .unwrap();
        assert_eq!(s.app.store.dependencies(card.id).unwrap(), [3]);

        // The read failed: what the forge keeps stays, what the description names changes.
        two.blocked_by = None;
        s.app.sync_issues(&s.repository, &[two.clone()]).unwrap();
        assert_eq!(s.app.store.dependencies(card.id).unwrap(), [3]);
        two.body = "Blocked by #4".into();
        s.app.sync_issues(&s.repository, &[two.clone()]).unwrap();
        assert_eq!(s.app.store.dependencies(card.id).unwrap(), [3, 4]);
        two.body = String::new();
        s.app.sync_issues(&s.repository, &[two.clone()]).unwrap();
        assert_eq!(s.app.store.dependencies(card.id).unwrap(), [3]);

        // Read again, and the forge no longer keeps it.
        two.blocked_by = Some(Vec::new());
        s.app.sync_issues(&s.repository, &[two]).unwrap();
        assert!(s.app.store.dependencies(card.id).unwrap().is_empty());
    }

    #[test]
    fn a_snapshot_of_the_forge_older_than_a_write_of_moebius_changes_nothing() {
        let mut s = Setup::new(Some(true));
        let (a, b, c) = (s.card("A"), s.card("B"), s.card("C"));
        // What the forge said before each write below, which a slow poll hands over after it.
        let stale = |number: i64, title: &str| open_issue(number, title);
        s.kinds();

        s.app.set_labels(a.id, &["bug".into()]).unwrap();
        s.app.set_parent(b.id, Some(a.number)).unwrap();
        s.app.add_blocker(b.id, c.number).unwrap();
        s.app.close_card(c.id).unwrap();
        s.app.close_card(a.id).unwrap();
        s.app.reopen_issue(&Actor::Person, a.id, "").unwrap();
        s.kinds();
        // The poll began before the writes ended (at the epoch, say).
        s.app
            .sync_issues_read_at(
                &s.repository,
                &[stale(1, "A"), stale(2, "B"), stale(3, "C")],
                0,
            )
            .unwrap();
        let (a2, b2) = (s.app.card(a.id).unwrap(), s.app.card(b.id).unwrap());
        assert_eq!(a2.labels, ["bug"]);
        assert_eq!(
            (b2.parent, s.app.store.dependencies(b.id).unwrap()),
            (Some(1), vec![3])
        );
        assert_eq!((a2.closed, a2.column.as_str()), (false, "Backlog"));
        let c2 = s.app.card(c.id).unwrap();
        assert_eq!((c2.closed, c2.column.as_str()), (true, "Done"));
        assert!(s.kinds().is_empty(), "no change was announced");

        // The forge then catches up with Moebius, and then somebody changes it: that is a change to take.
        let mut fresh = stale(1, "A");
        fresh.labels = vec!["bug".into()];
        let mut closed_c = stale(3, "C");
        closed_c.open = false;
        s.app
            .sync_issues(&s.repository, &[fresh.clone(), closed_c])
            .unwrap();
        assert!(s.kinds().is_empty());
        fresh.labels = vec!["wayfinder:spike".into()];
        s.app.sync_issues(&s.repository, &[fresh]).unwrap();
        assert_eq!(s.app.card(a.id).unwrap().labels, ["wayfinder:spike"]);
        assert_eq!(s.kinds(), ["card_labels_changed"]);
    }

    #[test]
    fn someone_putting_back_what_moebius_wrote_before_the_next_poll_is_noticed() {
        let mut s = Setup::new(Some(true));
        let (a, b) = (s.card("A"), s.card("B"));
        s.app.set_labels(a.id, &["bug".into()]).unwrap();
        s.app.add_blocker(b.id, a.number).unwrap();
        s.kinds();
        // A poll that began after the writes shows the forge without them: somebody took them off.
        s.app
            .sync_issues(&s.repository, &[open_issue(1, "A")])
            .unwrap();
        let mut two = open_issue(2, "B");
        two.blocked_by = Some(Vec::new());
        s.app.sync_issues(&s.repository, &[two]).unwrap();
        assert!(s.app.card(a.id).unwrap().labels.is_empty());
        assert!(s.app.store.dependencies(b.id).unwrap().is_empty());
        assert_eq!(s.kinds(), ["card_labels_changed", "card_blocking_changed"]);
    }

    #[test]
    fn a_closed_blocker_whose_card_was_deleted_still_resolves() {
        let s = Setup::new(Some(true));
        let cards: Vec<Card> = (1..=5).map(|n| s.card(&format!("Card {n}"))).collect();
        let (five, nine) = (&cards[4], &cards[0]);
        // #5 is blocked by #1 (standing in for #9), whose card is then deleted.
        s.app.store.add_dependency(five.id, nine.number).unwrap();
        s.app
            .sync_issues(&s.repository, &[open_issue(1, "Card 1")])
            .unwrap();
        assert!(s.app.store.delete_card(nine.id, true).unwrap());
        assert_eq!(s.app.issue(five.id).unwrap().blocked_by, [1]);

        // The poll does not make the card again, but it learns that the issue was closed.
        let mut closed = open_issue(1, "Card 1");
        closed.open = false;
        s.app.sync_issues(&s.repository, &[closed]).unwrap();
        assert!(
            s.app
                .store
                .card_by_number(s.repository.id, 1)
                .unwrap()
                .is_none()
        );
        assert!(s.app.issue(five.id).unwrap().blocked_by.is_empty());
        // What else was seen of the deleted card goes with it.
        assert_eq!(
            s.app
                .store
                .forge_seen(s.repository.id, 1, "labels")
                .unwrap(),
            None
        );
    }

    #[test]
    fn a_forge_that_keeps_no_blocking_gets_a_line_in_the_description_and_the_edge_persists() {
        let s = Setup::new(Some(false));
        let (a, b) = (s.card("A"), s.card("B"));
        *s.blocking.lock().unwrap() = false;
        // Somebody edited the description on the forge since the last poll: the line is added to that text.
        *s.body.lock().unwrap() = Some("Edited on the forge.".into());
        s.app.add_blocker(b.id, a.number).unwrap();
        assert_eq!(
            s.calls(),
            [
                "#2 blocker #1 true",
                "update #2 \"B\" \"Edited on the forge.\\n\\nBlocked by #1\""
            ]
        );
        assert_eq!(
            s.app.card(b.id).unwrap().body,
            "Edited on the forge.\n\nBlocked by #1"
        );

        // The poll reads the description back, and the forge links nothing as blocking: the edge stays.
        let mut two = open_issue(2, "B");
        two.body = "Edited on the forge.\n\nBlocked by #1".into();
        s.app.sync_issues(&s.repository, &[two]).unwrap();
        assert_eq!(s.app.store.dependencies(b.id).unwrap(), [1]);

        *s.body.lock().unwrap() = Some("Edited on the forge.\n\nBlocked by #1".into());
        s.app.remove_blocker(b.id, a.number).unwrap();
        assert_eq!(
            s.calls(),
            [
                "#2 blocker #1 false",
                "update #2 \"B\" \"Edited on the forge.\""
            ]
        );
        assert!(s.app.store.dependencies(b.id).unwrap().is_empty());
    }

    #[test]
    fn issues_that_would_wait_for_each_other_are_refused_and_ignored_when_the_forge_says_them() {
        let s = Setup::new(None);
        let (map, child, c) = (s.card("Map"), s.card("Child"), s.card("C"));
        s.app.set_parent(child.id, Some(map.number)).unwrap();

        // A sub-issue blocked by an ancestor would wait for the parent that waits for it.
        let refused = s.app.add_blocker(child.id, map.number).unwrap_err();
        assert!(matches!(refused, AppError::Invalid(_)), "{refused}");
        // A parent blocked by its own descendant is not a cycle by itself: it waits for it either way.
        s.app.add_blocker(map.id, child.number).unwrap();
        // But a parent that waits for the issue that is to be its child is.
        s.app.add_blocker(c.id, map.number).unwrap();
        let refused = s.app.set_parent(c.id, Some(map.number)).unwrap_err();
        assert!(matches!(refused, AppError::Invalid(_)), "{refused}");

        // What a forge says is held to the same: a description that makes an issue its own parent, or one of its
        // sub-issues its parent, or an ancestor its blocker, is ignored.
        let mut s = Setup::new(Some(true));
        let (a, b) = (s.card("A"), s.card("B"));
        s.app.set_parent(b.id, Some(a.number)).unwrap();
        let mut own = open_issue(1, "A");
        own.body = "Part of #1".into();
        s.app.sync_issues(&s.repository, &[own]).unwrap();
        assert_eq!(s.app.card(a.id).unwrap().parent, None);
        let mut above = open_issue(1, "A");
        above.body = "Part of #2".into();
        s.app.sync_issues(&s.repository, &[above.clone()]).unwrap();
        assert_eq!(s.app.card(a.id).unwrap().parent, None);
        let mut blocked = open_issue(2, "B");
        blocked.parent = Some(1);
        blocked.blocked_by = Some(vec![1]);
        s.kinds();
        s.app
            .sync_issues(&s.repository, &[blocked.clone()])
            .unwrap();
        assert!(s.app.store.dependencies(b.id).unwrap().is_empty());
        // The board says so, as a forge error, and takes it again once the cycle is gone.
        assert_eq!(s.kinds(), ["forge_error"]);
        s.app.set_parent(b.id, None).unwrap();
        blocked.parent = None;
        s.app.sync_issues(&s.repository, &[blocked]).unwrap();
        assert_eq!(s.app.store.dependencies(b.id).unwrap(), [1]);
        // The parent that was ignored is taken too, now that #2 is not below #1 any more.
        s.app.sync_issues(&s.repository, &[above]).unwrap();
        assert_eq!(s.app.card(a.id).unwrap().parent, Some(2));
    }

    #[test]
    fn an_outside_issue_does_not_make_issues_wait_for_each_other() {
        let s = Setup::new(Some(true));
        let parent = s.card("Parent");
        let mut outside = open_issue(2, "Outside");
        outside.author = "mallory".into();
        outside.body = "Part of #1".into();
        s.app.sync_issues(&s.repository, &[outside]).unwrap();
        let outside = s
            .app
            .store
            .card_by_number(s.repository.id, 2)
            .unwrap()
            .unwrap();
        assert_eq!(outside.parent, Some(1));
        // It does not hold #1 back, so blocking it with #1 is no cycle; a person approving it makes it one.
        s.app.add_blocker(outside.id, parent.number).unwrap();
        s.app
            .approve_issue(&Actor::Person, outside.id, "fine")
            .unwrap();
        let refused = s.app.add_blocker(parent.id, outside.number).unwrap_err();
        assert!(matches!(refused, AppError::Invalid(_)), "{refused}");
    }

    #[test]
    fn a_blocker_the_forge_says_is_closed_resolves_though_it_has_no_card() {
        let s = Setup::new(Some(true));
        let b = s.card("B");
        s.app.store.add_dependency(b.id, 7).unwrap();
        s.app.store.add_dependency(b.id, 8).unwrap();
        let mut closed = open_issue(7, "Long gone");
        closed.open = false;
        s.app.sync_issues(&s.repository, &[closed]).unwrap();
        // No card is made for it, but what the forge said of it is remembered.
        assert!(
            s.app
                .store
                .card_by_number(s.repository.id, 7)
                .unwrap()
                .is_none()
        );
        assert_eq!(s.app.issue(b.id).unwrap().blocked_by, [8]);
    }

    #[test]
    fn a_script_gives_and_takes_a_label_keeping_the_others_and_asks_the_forge_only_for_a_change() {
        let s = Setup::new(Some(true));
        let a = s.card("A");
        s.app.set_labels(a.id, &["bug".to_string()]).unwrap();
        s.calls.lock().unwrap().clear();
        let give = [("in-progress".to_string(), true)];
        s.app.set_labels_for_script(a.id, &give).unwrap();
        assert_eq!(s.calls(), ["#1 labels=bug,in-progress"]);
        assert_eq!(s.app.card(a.id).unwrap().labels, ["bug", "in-progress"]);

        // As asked already: nothing is sent.
        s.app.set_labels_for_script(a.id, &give).unwrap();
        assert!(s.calls().is_empty());

        // Taking it away matches the name in any case and leaves the other label.
        let take = [("In-Progress".to_string(), false)];
        s.app.set_labels_for_script(a.id, &take).unwrap();
        assert_eq!(s.app.card(a.id).unwrap().labels, ["bug"]);
    }

    #[test]
    fn a_forge_that_refuses_a_label_for_a_script_is_told_once_and_does_not_stop_the_card() {
        let mut s = Setup::new(Some(true));
        let a = s.card("A");
        s.kinds();
        *s.labels_refused.lock().unwrap() = true;
        let give = [("in-progress".to_string(), true)];
        s.app.set_labels_for_script(a.id, &give).unwrap();
        s.app.set_labels_for_script(a.id, &give).unwrap();
        assert_eq!(s.kinds(), ["forge_error"]);
        assert!(s.app.card(a.id).unwrap().labels.is_empty());
    }

    #[test]
    fn a_forge_that_refuses_to_close_an_issue_for_a_script_is_not_asked_again_at_every_tick() {
        let mut s = Setup::new(Some(true));
        let a = s.card("A");
        s.kinds();
        *s.failing.lock().unwrap() = true;
        let reason = s.app.close_for_script(a.id).unwrap().unwrap();
        assert!(reason.contains("the forge said no"), "{reason}");
        assert_eq!(s.kinds(), ["forge_error"]);

        // The forge is back, but the board holds off and says why; nothing was asked.
        *s.failing.lock().unwrap() = false;
        assert_eq!(s.app.close_for_script(a.id).unwrap(), Some(reason));
        assert!(s.calls().is_empty());
        assert!(s.kinds().is_empty());
        assert!(!s.app.card(a.id).unwrap().closed);

        // A person closing it is no business of the wait.
        s.app.close_issue(&Actor::Person, a.id, "by hand").unwrap();
        assert_eq!(s.app.close_for_script(a.id).unwrap(), None);
    }

    #[test]
    fn a_forge_that_keeps_refusing_is_told_once_however_often_it_is_asked_again() {
        let mut s = Setup::new(Some(true));
        s.app.close_retry = Duration::ZERO;
        let a = s.card("A");
        s.kinds();
        *s.failing.lock().unwrap() = true;
        for _ in 0..3 {
            assert!(s.app.close_for_script(a.id).unwrap().is_some());
        }
        assert_eq!(s.kinds(), ["forge_error"]);

        // Closed, the episode is over: a later refusal (the issue reopened) is told again.
        *s.failing.lock().unwrap() = false;
        assert_eq!(s.app.close_for_script(a.id).unwrap(), None);
        s.app.store.set_kept_open(a.id, false).unwrap();
        s.app.store.set_issue_closed(a.id, false).unwrap();
        *s.failing.lock().unwrap() = true;
        assert!(s.app.close_for_script(a.id).unwrap().is_some());
        assert_eq!(s.kinds(), ["card_moved", "card_closed", "forge_error"]);
    }

    #[test]
    fn a_parent_a_person_reopened_is_not_closed_again_until_a_sub_issue_closes_after_that() {
        let mut s = Setup::new(Some(true));
        let (parent, child) = (s.card("Parent"), s.card("Child"));
        s.app.set_parent(child.id, Some(parent.number)).unwrap();
        s.app.close_card(child.id).unwrap();
        // The board closes the parent with its last sub-issue.
        assert_eq!(s.app.close_for_script(parent.id).unwrap(), None);
        assert!(s.app.card(parent.id).unwrap().closed);

        s.app
            .reopen_issue(&Actor::Person, parent.id, "not done yet")
            .unwrap();
        s.kinds();
        s.calls();
        let waiting = s.app.close_for_script(parent.id).unwrap().unwrap();
        assert!(waiting.starts_with("reopened"), "{waiting}");
        assert!(!s.app.card(parent.id).unwrap().closed);
        assert!(s.calls().is_empty(), "the forge was not asked");
        assert!(s.kinds().is_empty());

        // A new sub-issue, done: that is the last sub-issue closing, and the parent closes with it.
        let next = s.card("Next");
        s.app.set_parent(next.id, Some(parent.number)).unwrap();
        s.app.close_card(next.id).unwrap();
        assert_eq!(s.app.close_for_script(parent.id).unwrap(), None);
        assert!(s.app.card(parent.id).unwrap().closed);

        // So does a reopening seen on the forge.
        let mut two = open_issue(1, "Parent");
        two.open = true;
        s.app.sync_issues(&s.repository, &[two]).unwrap();
        assert!(!s.app.card(parent.id).unwrap().closed);
        assert!(s.app.close_for_script(parent.id).unwrap().is_some());
    }

    #[test]
    fn the_first_poll_after_the_migration_says_nothing_of_closed_issues_that_are_in_done() {
        let mut s = Setup::new(Some(true));
        let a = s.card("A");
        // A store from before issues were closed: the card is in Done, and its issue is not marked closed.
        s.app.move_card(a.id, columns::DONE).unwrap();
        s.kinds();
        let mut closed = open_issue(1, "A");
        closed.open = false;
        s.app.sync_issues(&s.repository, &[closed.clone()]).unwrap();
        assert!(s.kinds().is_empty());
        assert!(s.app.card(a.id).unwrap().closed);
        assert_eq!(s.app.card(a.id).unwrap().column, "Done");

        // Closing a card that is not in Done is news.
        let b = s.card("B");
        s.kinds();
        closed.number = 2;
        closed.title = "B".into();
        s.app.sync_issues(&s.repository, &[closed]).unwrap();
        assert_eq!(s.app.card(b.id).unwrap().column, "Done");
        assert_eq!(s.kinds(), ["card_moved", "card_closed"]);
    }

    #[test]
    fn an_issue_is_created_with_a_parent_labels_and_blockers_or_with_warnings() {
        let s = Setup::new(Some(true));
        let (parent, blocker) = (s.card("Parent"), s.card("Blocker"));
        let create = |parent: Option<i64>, labels: &[&str], blocked_by: &[i64]| {
            let labels: Vec<String> = labels.iter().map(|l| l.to_string()).collect();
            s.app
                .create_tracked_issue(s.repository.id, "New", "", parent, &labels, blocked_by)
        };

        // What is named has to be there, and not make the issue wait for itself, before anything is created.
        for refused in [
            create(Some(99), &[], &[]),
            create(None, &[], &[99]),
            create(Some(parent.number), &[], &[parent.number]),
        ] {
            assert!(refused.is_err());
        }
        assert!(s.calls().is_empty());
        assert_eq!(s.app.store.cards(s.repository.id).unwrap().len(), 2);

        let (card, warnings) = create(Some(parent.number), &["bug"], &[blocker.number]).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(card.parent, Some(parent.number));
        assert_eq!(s.app.issue(card.id).unwrap().blocked_by, [blocker.number]);

        // A forge that refuses a step afterwards: the issue exists, and says what failed.
        *s.labels_refused.lock().unwrap() = true;
        let (card, warnings) = create(Some(parent.number), &["nope"], &[]).unwrap();
        assert_eq!(card.parent, Some(parent.number));
        assert!(card.labels.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].starts_with("could not set its labels"),
            "{warnings:?}"
        );
    }

    #[test]
    fn a_tracked_issue_is_held_until_its_blockers_are_stored_so_no_script_starts_it_unblocked() {
        let s = Setup::new(Some(true));
        let (parent, blocker) = (s.card("Parent"), s.card("Blocker"));
        let store = s.app.store.clone();
        *s.on_blocker.lock().unwrap() = Some(Arc::new(move |number: i64| {
            let card = store
                .card_by_number(1, number)
                .unwrap()
                .expect("the card exists while its blockers are set");
            assert!(
                card.held.is_some(),
                "#{} could be started before its blocker was stored",
                card.number
            );
        }));
        let (card, warnings) = s
            .app
            .create_tracked_issue(
                s.repository.id,
                "New",
                "",
                Some(parent.number),
                &[],
                &[blocker.number],
            )
            .unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(card.held.is_none());
        assert_eq!(s.app.issue(card.id).unwrap().blocked_by, [blocker.number]);
    }

    #[test]
    fn a_comment_the_poll_stored_first_is_returned_and_a_claim_is_released_only_by_its_holder() {
        let mut s = Setup::new(Some(true));
        let a = s.card("A");
        // The poll read the comment from the forge between the forge's answer and the store's.
        s.app
            .store
            .add_comment(a.id, "moebius[bot]", "Decided.", None, Some(500))
            .unwrap();
        s.kinds();
        let comment = s
            .app
            .comment_issue(&Actor::Person, a.id, "Decided.")
            .unwrap();
        assert_eq!(comment.author, "person");
        assert!(s.kinds().is_empty());
        assert_eq!(s.app.comments(a.id).unwrap().len(), 1);

        let agent = |name: &str| Actor::Agent {
            name: name.into(),
            run: 1,
        };
        s.app
            .claim_issue(&agent("Moebius"), a.id, Some("session 1"))
            .unwrap();
        for (by, claimer) in [
            (agent("Other"), None),
            (agent("Moebius"), None),
            (agent("Moebius"), Some("session 2")),
        ] {
            let refused = s.app.release_issue(&by, a.id, claimer).unwrap_err();
            assert!(matches!(refused, AppError::Forbidden(_)), "{refused}");
        }
        s.app
            .release_issue(&agent("Moebius"), a.id, Some("session 1"))
            .unwrap();
        s.app.claim_issue(&agent("Moebius"), a.id, None).unwrap();
        s.app.release_issue(&agent("Moebius"), a.id, None).unwrap();
        s.app.claim_issue(&agent("Moebius"), a.id, None).unwrap();
        s.app.release_issue(&Actor::Person, a.id, None).unwrap();
    }
}
