//! Changes: publishing a card's work through its repository's forge, offering it and a chat's work in the inbox,
//! rejecting and merging them, and what the forge says became of them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::json;

use super::{App, AppError, Result, columns};
use crate::bus::AppEvent;
use crate::decide::{Actor, Question};
use crate::repos::git;
use crate::store::{Card, Change, Repository, Run, now};

/// What became of the last offer of a chat's branch (see `App::earlier_offer`).
enum Earlier {
    /// The branch has not moved since: the change, while it is open; none once it was merged or closed.
    Same(Option<Change>),
    /// The branch moved on from the open change.
    Open(Change),
    /// Its work was never offered, or the last offer was merged or closed before the branch moved on.
    None,
}

impl App {
    /// The repository a change belongs to; refused for a change of the harness configuration repository.
    pub(super) fn repository_of(&self, change: &Change) -> Result<Repository> {
        match change.repository_id {
            Some(id) => self.repository(id),
            None => Err(AppError::Invalid(format!(
                "change {} belongs to the harness configuration repository",
                change.id
            ))),
        }
    }

    /// A person rejects the change waiting to be merged: the decision is recorded and the card goes back to coding,
    /// where the next round is told the reason. The change stays open and is updated by that round.
    pub fn reject_change(&self, by: &Actor, change_id: i64, reason: &str) -> Result<Card> {
        let change = self.change(change_id)?;
        let card = match change.card_id.map(|id| self.card(id)).transpose()? {
            Some(card) if change.state == "open" && card.column == columns::READY_TO_MERGE => card,
            _ => {
                return Err(AppError::Invalid(format!(
                    "change {change_id} is not waiting to be merged"
                )));
            }
        };
        let context = BTreeMap::from([
            ("title".into(), card.title.clone()),
            ("branch".into(), change.branch.clone()),
        ]);
        self.record_action(by, "reject", Some(card.id), None, context, reason)?;
        let note = match reason.trim() {
            "" => "A person rejected the change.",
            reason => reason,
        };
        self.store.set_card_note(card.id, note)?;
        self.store.resolve_inbox_for_card(card.id)?;
        self.move_card(card.id, columns::IN_PROGRESS)
    }

    /// Publishes the work of the card's latest successful `code` run on the branch of its change, `moebius/card-<number>`,
    /// through the repository's forge: the first time that creates the change, later it updates it. Each round's branch
    /// descends from the last, so the branch only ever moves forward. Does nothing when this work is published.
    pub fn publish_change(&self, card_id: i64) -> Result<Change> {
        let card = self.card(card_id)?;
        let repository = self.repository(card.repository_id)?;
        let forge = self.forge(&repository)?;
        let code_branch = self
            .store
            .code_branch(card_id)?
            .ok_or_else(|| AppError::Invalid("the card has no code run to publish".into()))?;
        let clone = Path::new(&repository.path);
        let head = git(clone, &["rev-parse", &code_branch])?;
        let existing = self.store.open_change(card_id)?;
        if let Some(change) = existing.as_ref().filter(|c| c.head.as_ref() == Some(&head)) {
            return Ok(change.clone());
        }
        let branch = format!("moebius/card-{}", card.number);
        self.repos.point_branch(clone, &branch, &head)?;
        let pull_request = existing.as_ref().and_then(|c| c.pull_request);
        let body = format!("Closes #{}\n\nMade by Moebius.", card.number);
        let pull_request = forge.publish(&repository, &card.title, &body, &branch, pull_request)?;
        let change = match existing {
            Some(change) => {
                self.store.set_change_head(change.id, &head, now())?;
                self.change(change.id)?
            }
            None => self.store.add_change(
                card.repository_id,
                Some(card_id),
                &branch,
                pull_request,
                &head,
            )?,
        };
        self.bus
            .publish(AppEvent::new("change_updated", json!(change)));
        Ok(change)
    }

    /// Publishes the card's change and puts it in the inbox, ready to merge.
    pub fn offer_change(&self, card_id: i64) -> Result<Change> {
        let change = self.publish_change(card_id)?;
        let card = self.card(card_id)?;
        let change = self.put_in_inbox(change, &card.title)?;
        self.move_card(card_id, columns::READY_TO_MERGE)?;
        Ok(change)
    }

    /// Offers what a chat in a repository left on its branch as a change without a card: published through the
    /// repository's forge and put in the inbox, ready to merge. A chat that left nothing beyond the repository's
    /// default branch, or has no repository, offers nothing. What a chat in the harness configuration repository
    /// left is offered the same way once it passes the check of a proposal (see `HarnessRepo::check_chat_branch`);
    /// a branch that fails it is not offered, the chat says why, and the branch stays. A chat whose work was offered
    /// before updates that change while it is open, and offers nothing new when the branch has not moved since.
    pub fn offer_chat_change(&self, run_id: i64) -> Result<Option<Change>> {
        let run = self.run(run_id)?;
        match self.offer_chat_branch(&run)? {
            Ok(offered) => Ok(offered),
            Err(why) => {
                let message = format!("the work is not offered as a change: {why}");
                self.record_run_event(run.id, "moebius_error", &json!({ "message": message }))?;
                Ok(None)
            }
        }
    }

    /// The chat `run_id` offers its worktree as it is now as a change, without ending: what the agent left is committed
    /// on the chat's branch and offered as the chat's end would (see `offer_chat_change`), and the chat goes on in the
    /// same worktree. Offered again later, the chat's open change is updated. Refused while the agent works, whose
    /// worktree may be half edited, and when there is nothing to offer. Recorded as a decision of kind
    /// `offer_chat_changes`.
    pub fn offer_chat_changes(&self, by: &Actor, run_id: i64, reason: &str) -> Result<Change> {
        let run = self.running_chat(run_id)?;
        if run.agent == crate::harness::pibolt::GRILLING_AGENT {
            return Err(AppError::Invalid(format!(
                "chat {run_id} is a grilling, which ends in issues, not a change"
            )));
        }
        let worktree = match (
            &run.worktree,
            run.repository_id.is_some() || run.harness_repository,
        ) {
            (Some(worktree), true) => PathBuf::from(worktree),
            _ => {
                return Err(AppError::Invalid(format!(
                    "chat {run_id} works in no repository: it has nothing to offer"
                )));
            }
        };
        {
            let _turn = self.chat_turns.lock().unwrap_or_else(|e| e.into_inner());
            let status = self.run(run_id)?.status;
            if matches!(status.as_str(), "starting" | "running") {
                return Err(AppError::Conflict(format!(
                    "chat {run_id} is working: its changes can be offered once the agent has answered"
                )));
            }
            let message =
                format!("Chat {run_id}\n\nCommitted by Moebius when the chat's work was offered.");
            crate::repos::commit_all(&worktree, &message)?;
        }
        let change = match self.offer_chat_branch(&run)? {
            Ok(Some(change)) => change,
            Ok(None) => {
                return Err(AppError::Invalid(format!(
                    "chat {run_id} has nothing to offer: its branch changes nothing that was not offered"
                )));
            }
            Err(why) => {
                return Err(AppError::Invalid(format!(
                    "the work is not offered as a change: {why}"
                )));
            }
        };
        let context = BTreeMap::from([
            ("change".to_string(), change.id.to_string()),
            ("branch".to_string(), change.branch.clone()),
        ]);
        self.record_action(
            by,
            "offer_chat_changes",
            None,
            Some(run_id),
            context,
            reason,
        )?;
        Ok(change)
    }

    /// Offers the branch of the chat `run`, as `offer_chat_change` says. The inner error says why the branch of a chat
    /// in the harness configuration repository is not fit to offer.
    fn offer_chat_branch(&self, run: &Run) -> Result<std::result::Result<Option<Change>, String>> {
        let branch = run.branch();
        let title = self.chat_title(run.id)?;
        if run.harness_repository {
            let harness_repo = self.harness_repo()?;
            match harness_repo.check_chat_branch(self, &branch) {
                Ok(None) => return Ok(Ok(None)),
                Err(why) => return Ok(Err(why)),
                Ok(Some(_)) => {}
            }
            let head = git(harness_repo.path(), &["rev-parse", &branch])?;
            return Ok(Ok(match self.earlier_offer(&branch, &head)? {
                Earlier::Same(change) => change,
                Earlier::Open(change) => Some(self.offer_again(change.id, &head, &title)?),
                Earlier::None => {
                    let change = self.store.add_harness_change(&branch, &head)?;
                    Some(self.put_in_inbox(change, &title)?)
                }
            }));
        }
        let Some(repository_id) = run.repository_id else {
            return Ok(Ok(None));
        };
        let repository = self.repository(repository_id)?;
        let clone = Path::new(&repository.path);
        let beyond_base = format!("{}..{branch}", repository.base_ref());
        if git(clone, &["rev-list", "--count", &beyond_base])? == "0" {
            return Ok(Ok(None));
        }
        let head = git(clone, &["rev-parse", &branch])?;
        let body = format!(
            "This change came from chat {} with the agent {}.\n\nMade by Moebius.",
            run.id, run.agent
        );
        Ok(Ok(match self.earlier_offer(&branch, &head)? {
            Earlier::Same(change) => change,
            Earlier::Open(change) => {
                let forge = self.forge(&repository)?;
                forge.publish(&repository, &title, &body, &branch, change.pull_request)?;
                Some(self.offer_again(change.id, &head, &title)?)
            }
            Earlier::None => Some(self.offer_branch_change(&repository, &branch, &title, &body)?),
        }))
    }

    /// What became of the last offer of a chat's `branch`, whose commit is now `head`.
    fn earlier_offer(&self, branch: &str, head: &str) -> Result<Earlier> {
        Ok(match self.store.latest_change_on_branch(branch)? {
            // Nothing moved since: the open change still holds it, and a merged or closed one took it.
            Some(change) if change.head.as_deref() == Some(head) => {
                Earlier::Same((change.state == "open").then_some(change))
            }
            Some(change) if change.state == "open" => Earlier::Open(change),
            _ => Earlier::None,
        })
    }

    /// Records the newer commit `head` of an open change and puts it in the inbox again, ready to merge.
    fn offer_again(&self, change_id: i64, head: &str, title: &str) -> Result<Change> {
        self.store.set_change_head(change_id, head, now())?;
        self.store.resolve_inbox_for_change(change_id)?;
        self.put_in_inbox(self.change(change_id)?, title)
    }

    /// Puts a change in the inbox under its card, if it has one, ready to merge, and says it was offered.
    fn put_in_inbox(&self, change: Change, title: &str) -> Result<Change> {
        self.store.add_inbox_item(
            "ready_to_merge",
            change.card_id,
            title,
            &json!({ "change": change.id, "branch": change.branch }),
        )?;
        self.bus
            .publish(AppEvent::new("change_offered", json!(change)));
        Ok(change)
    }

    /// Offers `branch` of `repository`, which has commits beyond the default branch, as a change without a card:
    /// published through the repository's forge and put in the inbox, ready to merge.
    pub fn offer_branch_change(
        &self,
        repository: &Repository,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<Change> {
        let head = git(Path::new(&repository.path), &["rev-parse", branch])?;
        let forge = self.forge(repository)?;
        let pull_request = forge.publish(repository, title, body, branch, None)?;
        let change = self
            .store
            .add_change(repository.id, None, branch, pull_request, &head)?;
        self.put_in_inbox(change, title)
    }

    /// The files that keep the change from merging cleanly into the repository's default branch, one per line; none
    /// when it merges cleanly.
    pub fn conflict_of(&self, change: &Change) -> Result<Option<String>> {
        let repository = self.repository_of(change)?;
        Ok(self.repos.conflicts(
            Path::new(&repository.path),
            &repository.base_ref(),
            &change.branch,
        )?)
    }

    /// A person merges a change: the decision is recorded, the forge merges, and the card moves to done. A change of the
    /// harness configuration repository is merged into its default branch and pushed (see `HarnessRepo::merge_branch`).
    pub fn merge_change(&self, by: &Actor, change_id: i64, reason: &str) -> Result<String> {
        let change = self.change(change_id)?;
        if change.state != "open" {
            return Err(AppError::Invalid(format!(
                "change {change_id} is {}",
                change.state
            )));
        }
        let repository = change
            .repository_id
            .map(|id| self.repository(id))
            .transpose()?;
        let name = repository.as_ref().map_or("harness", |r| r.name.as_str());
        self.decisions.record_by(
            by,
            &Question {
                kind: "merge".into(),
                context: BTreeMap::from([
                    ("repository".into(), name.to_string()),
                    ("branch".into(), change.branch.clone()),
                ]),
                options: vec!["merge".into(), "reject".into()],
                fail_closed: "reject".into(),
                run_id: None,
                card_id: change.card_id,
            },
            "merge",
            reason,
        )?;
        let message = match change.card_id.map(|id| self.card(id)).transpose()? {
            Some(card) => format!("Merge #{}: {}", card.number, card.title),
            None => format!("Merge {}", change.branch),
        };
        let commit = match &repository {
            Some(repository) => self
                .forge(repository)?
                .merge(repository, &change, &message)?,
            None => self
                .harness_repo()?
                .merge_branch(self, &change.branch, &message)?,
        };
        self.change_merged(&change, Some(&commit))?;
        Ok(commit)
    }

    /// A change is merged, here or on the forge: its inbox items are resolved and its card, if it has one, is done.
    pub fn change_merged(&self, change: &Change, commit: Option<&str>) -> Result<()> {
        self.store.set_change_state(change.id, "merged")?;
        self.bus.publish(AppEvent::new(
            "change_merged",
            json!({ "change": change.id, "commit": commit }),
        ));
        match change.card_id {
            Some(card_id) => {
                self.store.resolve_inbox_for_card(card_id)?;
                self.move_card(card_id, columns::DONE)?;
            }
            None => self.store.resolve_inbox_for_change(change.id)?,
        }
        Ok(())
    }

    /// A change's pull request was closed on the forge without being merged: the change is closed and, when it has a
    /// card, the person is told.
    pub fn change_closed(&self, change: &Change) -> Result<()> {
        self.store.set_change_state(change.id, "closed")?;
        self.bus.publish(AppEvent::new(
            "change_closed",
            json!({ "change": change.id }),
        ));
        match change.card_id {
            Some(card_id) => {
                let card = self.card(card_id)?;
                self.store.resolve_inbox_for_card(card_id)?;
                self.store.add_inbox_item(
                    "change_closed",
                    Some(card_id),
                    &card.title,
                    &json!({ "change": change.id, "pull_request": change.pull_request }),
                )?;
            }
            None => self.store.resolve_inbox_for_change(change.id)?,
        }
        Ok(())
    }

    /// Records the checks of a change's pull request for its head commit `checks_head`, the failing ones, and the
    /// latest review, and publishes the change if they differ from what was known.
    pub fn record_change_status(
        &self,
        change: &Change,
        checks: Option<&str>,
        checks_head: &str,
        failed_checks: Option<&str>,
        review: Option<&str>,
    ) -> Result<()> {
        if change.checks.as_deref() == checks
            && change.checks_head.as_deref() == Some(checks_head)
            && change.failed_checks.as_deref() == failed_checks
            && change.review.as_deref() == review
        {
            return Ok(());
        }
        self.store
            .set_change_status(change.id, checks, checks_head, failed_checks, review)?;
        self.bus.publish(AppEvent::new(
            "change_updated",
            json!(self.change(change.id)?),
        ));
        Ok(())
    }
}
