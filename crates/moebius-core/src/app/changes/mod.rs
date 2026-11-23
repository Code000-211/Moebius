//! Changes: offering a card's work (`publish`, one change per repository it works in) and a chat's work in the inbox,
//! rejecting and merging them, and what the forge says became of them.

mod merge;
mod publish;

pub use merge::CardMerge;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::json;

use super::{App, AppError, Result, columns};
use crate::bus::Event;
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

/// How much of what the code run said a pull request carries, in characters.
const SAID_AT_MOST: usize = 8_000;

/// How many files `git diff --stat` lists in a pull request before the rest is left to its line of totals.
const FILES_AT_MOST: usize = 30;

/// `text` cut to `at_most` characters at a character boundary, saying where it was cut.
fn cut(text: &str, at_most: usize) -> String {
    let mut kept = text.chars().take(at_most).collect::<String>();
    if kept.len() < text.len() {
        kept.push('\n');
        kept.push('…');
    }
    kept
}

/// `stat` cut to its first `at_most` lines, keeping the line of totals that `git diff --stat` ends with.
fn cut_lines(stat: &str, at_most: usize) -> String {
    let lines = stat.lines().collect::<Vec<_>>();
    if lines.len() <= at_most {
        return stat.to_string();
    }
    let totals = lines.last().copied().unwrap_or_default();
    format!("{}\n…\n{totals}", lines[..at_most - 1].join("\n"))
}

/// The subject of the last commit on `branch`; None when git says nothing, so a merge keeps its plain line.
fn branch_tip_subject(clone: &Path, branch: &str) -> Option<String> {
    let subject = git(clone, &["log", "--format=%s", "-1", branch])
        .ok()?
        .trim()
        .lines()
        .next()?
        .trim()
        .to_owned();
    (!subject.is_empty()).then_some(subject)
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
    /// where the next round is told the reason. The change stays open and is updated by that round. A workflow run's
    /// change has no card: its rejection is recorded, it is closed, and none is returned.
    pub fn reject_change(&self, by: &Actor, change_id: i64, reason: &str) -> Result<Option<Card>> {
        let change = self.change(change_id)?;
        if change.card_id.is_none() {
            if change.run_id.is_none() || change.state != "open" {
                return Err(AppError::Invalid(format!(
                    "change {change_id} is not waiting to be merged"
                )));
            }
            let repository = change
                .repository_id
                .map(|id| self.repository(id))
                .transpose()?;
            let name = repository.as_ref().map_or("harness", |r| r.name.as_str());
            let context = BTreeMap::from([
                ("repository".into(), name.to_string()),
                ("branch".into(), change.branch.clone()),
            ]);
            self.record_action(by, "reject", None, None, context, reason)?;
            self.change_closed(&change)?;
            return Ok(None);
        }
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
        self.move_card(card.id, columns::IN_PROGRESS, Some(by))
            .map(Some)
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
            let message = self.chat_commit_message(&run, "when the chat's work was offered");
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

    /// Offers `branch` of the harness configuration repository, which holds work that is not a chat's (a package to
    /// install, say), as a change without a card: it must pass the check of a proposal (`HarnessRepo::check_chat_branch`)
    /// and is then pushed and put in the inbox, ready to merge by a person.
    pub fn offer_harness_branch(&self, branch: &str, title: &str) -> Result<Change> {
        let harness_repo = self.harness_repo()?;
        match harness_repo.check_chat_branch(self, branch) {
            Ok(Some(_)) => {}
            Ok(None) => return Err(AppError::Invalid(format!("{branch} changes nothing"))),
            Err(why) => {
                return Err(AppError::Invalid(format!(
                    "the change is not fit to be offered: {why}"
                )));
            }
        }
        let head = git(harness_repo.path(), &["rev-parse", branch])?;
        let change = self.store.add_harness_change(branch, &head)?;
        self.put_in_inbox(change, title, None)
    }

    /// Offers the branch of the chat `run`, as `offer_chat_change` says. The inner error says why the branch of a chat
    /// in the harness configuration repository is not fit to offer.
    fn offer_chat_branch(&self, run: &Run) -> Result<std::result::Result<Option<Change>, String>> {
        let branch = run.branch();
        let title = self.chat_title(run)?;
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
                    Some(self.put_in_inbox(change, &title, None)?)
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
        let body = self.chat_pull_request_body(run, &repository, &branch);
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

    /// What the pull request of a chat's change says: what the chat's agent said it did in its last answer, the
    /// stat of the files changed, which chat it was and what it was asked. Never just the chat's first message; a
    /// section Moebius has nothing for is left out.
    fn chat_pull_request_body(&self, run: &Run, repository: &Repository, branch: &str) -> String {
        let mut body = String::new();
        if let Some(said) = run
            .summary
            .as_deref()
            .map(str::trim)
            .filter(|said| !said.is_empty())
        {
            body.push_str("## What the chat did\n\n");
            body.push_str(&cut(said, SAID_AT_MOST));
            body.push('\n');
        }
        if let Some(stat) = self.files_changed(repository, branch) {
            body.push_str(&format!("\n## Files changed\n\n```\n{stat}\n```\n"));
        }
        let asked = self.chat_title(run).unwrap_or_default();
        let asked = if asked.is_empty() {
            String::new()
        } else {
            format!(", asked to: {}.", asked.trim_end_matches('.').trim_end())
        };
        body.push_str(&format!(
            "\nThis change came from chat {} with the agent {}{asked}\n\nMade by Moebius.",
            run.id, run.agent
        ));
        body
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
        self.put_in_inbox(self.change(change_id)?, title, None)
    }

    /// The stat of what the change does to the repository, `git diff --stat` from where the branch left the
    /// repository's branch, cut to its first files when the list is long.
    fn files_changed(&self, repository: &Repository, branch: &str) -> Option<String> {
        let from = format!("{}...{branch}", repository.base_ref());
        let stat = git(Path::new(&repository.path), &["diff", "--stat", &from]).ok()?;
        let stat = stat.trim();
        (!stat.is_empty()).then(|| cut_lines(stat, FILES_AT_MOST))
    }

    /// Puts a change in the inbox under its card, if it has one, ready to merge, and says it was offered.
    fn put_in_inbox(
        &self,
        change: Change,
        title: &str,
        judgements: Option<&str>,
    ) -> Result<Change> {
        let mut data = json!({ "change": change.id, "branch": change.branch });
        if let Some(run) = change.run_id {
            data["run"] = json!(run);
        }
        if let Some(judgements) = judgements {
            data["judgements"] = json!(judgements);
        }
        self.store
            .add_inbox_item("ready_to_merge", change.card_id, title, &data)?;
        self.bus.publish(Event::ChangeOffered(change.clone()));
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
        self.offer_published_change(repository, None, branch, title, body)
    }

    /// Offers the branch of the workflow run `run_id` as the run's change: published through the repository's forge
    /// like any other, with no card, and recorded with the run so its merge or rejection is the run's outcome.
    pub fn offer_run_branch_change(
        &self,
        repository: &Repository,
        run_id: i64,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<Change> {
        self.offer_published_change(repository, Some(run_id), branch, title, body)
    }

    fn offer_published_change(
        &self,
        repository: &Repository,
        run_id: Option<i64>,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<Change> {
        let head = git(Path::new(&repository.path), &["rev-parse", branch])?;
        let forge = self.forge(repository)?;
        let pull_request = forge.publish(repository, title, body, branch, None)?;
        let change = match run_id {
            Some(run_id) => {
                self.store
                    .add_run_change(repository.id, run_id, branch, pull_request, &head)?
            }
            None => self
                .store
                .add_change(repository.id, None, branch, pull_request, &head)?,
        };
        self.put_in_inbox(change, title, None)
    }

    /// The files that keep the change from merging cleanly into the repository's default branch, one per line; none
    /// when it merges cleanly.
    pub fn conflict_of(&self, change: &Change) -> Result<Option<String>> {
        let repository = self.repository_of(change)?;
        Ok(self.repos.conflicts_kept(
            Path::new(&repository.path),
            &repository.base_ref(),
            &change.branch,
        )?)
    }

    /// A person merges a change: the decision is recorded, the forge merges, and the card moves to done once no other
    /// change of it is open. A change of a card over several repositories is refused while a change before it, in the
    /// order the card names its repositories, is open (see `App::merge_card`). A change of the
    /// harness configuration repository is merged into its default branch and pushed (see `HarnessRepo::merge_branch`).
    /// When the merge fails while another change of the card is merged, the card is marked as `App::merge_card`
    /// marks it.
    pub fn merge_change(&self, by: &Actor, change_id: i64, reason: &str) -> Result<String> {
        self.merge_one(by, change_id, reason).inspect_err(|e| {
            let Ok(change) = self.change(change_id) else {
                return;
            };
            if let Some(Ok(card)) = change.card_id.map(|id| self.card(id))
                && change.state == "open"
                && self.partly_merged(&card, &change).unwrap_or(false)
            {
                let _ = self.merge_stopped(&card, &change, &e.to_string());
            }
        })
    }

    /// Merges one change, as `merge_change` does, without marking its card when the merge fails.
    pub(in crate::app) fn merge_one(
        &self,
        by: &Actor,
        change_id: i64,
        reason: &str,
    ) -> Result<String> {
        let change = self.change(change_id)?;
        if change.state != "open" {
            return Err(AppError::Invalid(format!(
                "change {change_id} is {}",
                change.state
            )));
        }
        self.check_merge_order(&change)?;
        // A pull request someone else opened is merged only once its checks have passed: it is theirs, not reviewed here.
        // Its checks must be known for its current head: after a push they are unknown until the poll has read them.
        if change.outside_author.is_some()
            && (matches!(change.checks.as_deref(), Some("pending" | "failure"))
                || change.checks_head != change.head)
        {
            return Err(AppError::Invalid(format!(
                "the checks of pull request #{} have not passed",
                change.pull_request.unwrap_or_default()
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
            // The issue's number is one of the card's own repository; another repository's merge names the card.
            Some(card) if change.repository_id == Some(card.repository_id) => {
                format!("Merge #{}: {}", card.number, card.title)
            }
            Some(card) => format!(
                "Merge {}-{}: {}",
                card.project_key, card.project_number, card.title
            ),
            None => {
                // The merge says what it carries: the subject of the change's last commit, in the line the forge
                // or the harness clone writes under it.
                let subject = match &repository {
                    Some(repository) => {
                        branch_tip_subject(Path::new(&repository.path), &change.branch)
                    }
                    None => self
                        .harness_repo()
                        .ok()
                        .and_then(|harness| branch_tip_subject(harness.path(), &change.branch)),
                };
                match subject {
                    Some(subject) => format!("Merge {}: {subject}", change.branch),
                    None => format!("Merge {}", change.branch),
                }
            }
        };
        let commit = match &repository {
            Some(repository) => self
                .forge(repository)?
                .merge(repository, &change, &message)?,
            None => self
                .harness_repo()?
                .merge_branch(self, by, &change.branch, &message)?,
        };
        self.change_merged(&change, Some(&commit), Some(by))?;
        Ok(commit)
    }

    /// A change is merged, here or on the forge: its inbox items are resolved and its card, if it has one, is done, once
    /// no other change of the card is open; until then only the change's own inbox item is resolved.
    /// `by` is the person who merged it, who is not told that the card moved (none for the poll).
    pub fn change_merged(
        &self,
        change: &Change,
        commit: Option<&str>,
        by: Option<&Actor>,
    ) -> Result<()> {
        self.store.set_change_state(change.id, "merged")?;
        self.bus.publish(Event::ChangeMerged {
            change: change.id,
            commit: commit.map(str::to_string),
        });
        self.localize_change_merged(change, commit);
        match change.card_id {
            // A card over several repositories is done once its last open change is merged (ADR 0019).
            Some(card_id) if !self.open_changes(&self.card(card_id)?)?.is_empty() => {
                self.store.resolve_inbox_for_change(change.id)?;
            }
            Some(card_id) => {
                let card = self.card(card_id)?;
                // A change closed on the forge without being merged is not merged: the card is not done.
                if let Some(closed) = self.closed_change(&card)? {
                    self.store.resolve_inbox_for_change(change.id)?;
                    let name = self.repository_of(&closed)?.name;
                    self.merge_stopped(
                        &card,
                        &closed,
                        &format!("the change in {name} was closed without being merged"),
                    )?;
                } else {
                    self.store.resolve_inbox_for_card(card_id)?;
                    self.move_card(card_id, columns::DONE, by)?;
                }
            }
            None => self.store.resolve_inbox_for_change(change.id)?,
        }
        Ok(())
    }

    /// A change's pull request was closed on the forge without being merged: the change is closed and, when it has a
    /// card, the person is told.
    pub fn change_closed(&self, change: &Change) -> Result<()> {
        // A pull request someone else opened: the card goes back where it was (see `App::followed_closed`).
        if change.outside_author.is_some() {
            return self.followed_closed(change);
        }
        self.store.set_change_state(change.id, "closed")?;
        self.bus.publish(Event::ChangeClosed { change: change.id });
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
        self.bus
            .publish(Event::ChangeUpdated(self.change(change.id)?));
        Ok(())
    }
}
