//! Publishing a card's work through the forges of its repositories (ADR 0019): one change per repository the card
//! works in, each on its own branch, and offering them in the inbox, ready to merge.

use std::path::Path;

use super::super::{App, AppError, Result, columns};
use super::{SAID_AT_MOST, cut};
use crate::bus::Event;
use crate::repos::git;
use crate::store::{Card, Change, Repository, now};

/// What is published of a card's work in one of its repositories: `head` on `branch`.
struct Publish<'a> {
    card: &'a Card,
    repository: &'a Repository,
    branch: &'a str,
    head: &'a str,
}

/// How much of the review's judgements a pull request carries, in characters.
const JUDGEMENTS_AT_MOST: usize = 4_000;

impl App {
    /// Publishes the work of the card's latest successful `code` run on the branch of its change, `moebius/card-<number>`:
    /// the branch is pushed to the forge and the change is recorded, without a pull request. The pull request opens
    /// only when the change is offered (see `offer_change`), once the review approved it. The branch belongs to
    /// Moebius, so a round that rewrote its commit (an amended message, say) replaces what was published; the push is
    /// refused if the forge's branch has moved on since. Does nothing when this work is published. A card over several
    /// repositories has each of them published (see `publish_card_changes`); this answers the change of its own.
    pub fn publish_change(&self, card_id: i64) -> Result<Change> {
        self.publish_card_change(card_id, false, None)
    }

    /// Publishes the card's work as `publish_change` does and, if the change has no pull request yet, opens one
    /// through the repository's forge; a forge without them has none to open. Without the review's `judgements`,
    /// which only an offer has. A review script that calls this when the `code` run ended lets the forge run its
    /// checks while the review of the card still reads the change.
    pub fn open_pull_request(&self, card_id: i64) -> Result<Change> {
        self.publish_card_change(card_id, true, None)
    }

    /// Publishes the card's work as `publish_card_changes` does and answers the change of the card's own repository.
    pub(crate) fn publish_card_change(
        &self,
        card_id: i64,
        open_pull_request: bool,
        judgements: Option<&str>,
    ) -> Result<Change> {
        let changes = self.publish_card_changes(card_id, open_pull_request, judgements)?;
        self.own_change(card_id, changes)
    }

    /// The change of the card's own repository among `changes`, or an error when it has none there: its change was
    /// merged and a later round left nothing new in that repository.
    fn own_change(&self, card_id: i64, changes: Vec<Change>) -> Result<Change> {
        let own = self.card(card_id)?.repository_id;
        changes
            .into_iter()
            .find(|change| change.repository_id == Some(own))
            .ok_or_else(|| {
                AppError::Invalid(
                    "the card has nothing new to publish in its own repository, whose change is merged".into(),
                )
            })
    }

    /// Publishes the card's work in each of its repositories, in the order it names them (ADR 0019): one change per
    /// repository, each on its own branch of that repository, through that repository's forge, and, if the change has
    /// no pull request yet, opening one when `open_pull_request`; a forge without them has none to open. The pull
    /// request says what the change does (see `pull_request_body`), with the review's `judgements` in it when the offer
    /// has them. The card's own repository always has its change, on `moebius/card-<number>`; another repository has
    /// one, on `moebius/card-<KEY>-<number in the project>`, once a `code` run left commits in it beyond its default
    /// branch. The own repository has none either when its change was merged and nothing new was committed in it since.
    /// Refused when the card has no successful `code` run.
    pub(crate) fn publish_card_changes(
        &self,
        card_id: i64,
        open_pull_request: bool,
        judgements: Option<&str>,
    ) -> Result<Vec<Change>> {
        let card = self.card(card_id)?;
        let repositories = self.card_repositories(&card)?;
        // Every repository can be published through its forge, or none is.
        for repository in &repositories {
            self.forge(repository)?;
        }
        let mut changes = Vec::new();
        for repository in repositories {
            let own = repository.id == card.repository_id;
            let code_branch = match self.store.code_branch_in(card_id, repository.id)? {
                Some(branch) => branch,
                None if own => {
                    return Err(AppError::Invalid(
                        "the card has no code run to publish".into(),
                    ));
                }
                None => continue,
            };
            let clone = Path::new(&repository.path);
            let head = git(clone, &["rev-parse", &code_branch])?;
            let existing = self.store.open_change_in(card_id, repository.id)?;
            // A repository where the card has no open change gets one only for commits it has not merged: a change
            // with none is a pull request the forge refuses (GitHub answers 422). The card's own repository has its
            // change always, unless that was merged and a later round, which reworks another repository, left nothing
            // new in it.
            if existing.is_none()
                && (!own || self.store.has_merged_change_in(card_id, repository.id)?)
            {
                let beyond_base = format!("{}..{head}", repository.base_ref());
                if git(clone, &["rev-list", "--count", &beyond_base])? == "0" {
                    continue;
                }
            }
            // An open change keeps its branch, which its pull request is on, though the card has since moved to
            // another project and so has another key.
            let branch = match &existing {
                Some(change) => change.branch.clone(),
                None if own => format!("moebius/card-{}", card.number),
                None => format!("moebius/card-{}-{}", card.project_key, card.project_number),
            };
            let publish = Publish {
                card: &card,
                repository: &repository,
                branch: &branch,
                head: &head,
            };
            changes.push(self.publish_in(publish, existing, open_pull_request, judgements)?);
        }
        Ok(changes)
    }

    /// Publishes `head` of the card's work in one repository on `branch` as its change, updating `existing`, the
    /// card's open change there. Does nothing when this work is published.
    fn publish_in(
        &self,
        Publish {
            card,
            repository,
            branch,
            head,
        }: Publish,
        existing: Option<Change>,
        open_pull_request: bool,
        judgements: Option<&str>,
    ) -> Result<Change> {
        // Where the agents work on the forge as people do (ADR 0034) the pull request opens with the first round, as a
        // draft, and is kept current after each round.
        let draft_mode = self.works_on_forge(repository);
        let open = open_pull_request || draft_mode;
        let opened = |c: &Change| c.pull_request.is_some() || !open;
        if let Some(change) = existing
            .as_ref()
            .filter(|c| c.head.as_deref() == Some(head) && opened(c))
        {
            return Ok(change.clone());
        }
        let forge = self.forge(repository)?;
        let clone = Path::new(&repository.path);
        self.repos.point_branch(clone, branch, head)?;
        let pull_request = existing.as_ref().and_then(|c| c.pull_request);
        let as_draft =
            draft_mode && (pull_request.is_none() || existing.as_ref().is_some_and(|c| c.draft));
        let mut written = None;
        let pull_request = if open {
            let body = if draft_mode {
                let (body, template) = self.own_pull_body(card, repository);
                written = Some((body.clone(), template));
                body
            } else {
                self.pull_request_body(card, repository, branch, judgements)
            };
            if as_draft {
                forge.publish_draft(repository, &card.title, &body, branch, pull_request)?
            } else {
                forge.publish(repository, &card.title, &body, branch, pull_request)?
            }
        } else {
            forge.push(repository, branch)?;
            pull_request
        };
        let change = match existing {
            Some(change) => {
                self.store.set_change_head(change.id, head, now())?;
                if pull_request != change.pull_request {
                    self.store
                        .set_change_pull_request(change.id, pull_request)?;
                }
                self.change(change.id)?
            }
            None => {
                self.store
                    .add_change(repository.id, Some(card.id), branch, pull_request, head)?
            }
        };
        let change = match (as_draft, pull_request) {
            (true, Some(_)) if !change.draft => {
                self.store.set_change_draft(change.id, true)?;
                self.change(change.id)?
            }
            _ => change,
        };
        if let Some((body, template)) = &written {
            // What the round's publish writes on the forge must not fail the round: it is reported.
            if let Err(e) = self
                .after_round_published(card, repository, &change, (body, template))
                .and_then(|()| self.link_pull_request(card, repository, &change))
            {
                self.bus.publish(Event::ForgeError {
                    repository: repository.name.clone(),
                    message: format!("#{}: the pull request: {e}", card.number),
                });
            }
        }
        let change = self.change(change.id)?;
        self.bus.publish(Event::ChangeUpdated(change.clone()));
        Ok(change)
    }

    /// The card's open changes, one per repository it works in that has one, in the order the card names them.
    pub fn open_changes(&self, card: &Card) -> Result<Vec<Change>> {
        let mut changes = Vec::new();
        for repository in self.card_repositories(card)? {
            if let Some(change) = self.store.open_change_in(card.id, repository.id)? {
                changes.push(change);
            }
        }
        Ok(changes)
    }

    /// Publishes the card's change, opening its pull request, and puts it in the inbox, ready to merge. A card over
    /// several repositories has each of its changes offered; this answers the change of its own repository.
    pub fn offer_change(&self, card_id: i64) -> Result<Change> {
        let (changes, _) = self.offer_change_noted(card_id, None)?;
        self.own_change(card_id, changes)
    }

    /// Like `offer_change`, with the judgements of the review attached to the inbox items as `judgements`, and
    /// answering every change of the card, in the order it names its repositories. Also says whether the card moved
    /// to Ready to merge: it does not when a person or the forge poll moved it while the changes were published, and
    /// then nothing is put in the inbox.
    pub fn offer_change_noted(
        &self,
        card_id: i64,
        judgements: Option<&str>,
    ) -> Result<(Vec<Change>, bool)> {
        let seen = self.card(card_id)?.column;
        let changes = self.publish_card_changes(card_id, true, judgements)?;
        let changes = self.make_ready(changes)?;
        if changes.is_empty() {
            return Err(AppError::Invalid(
                "the card has nothing new to offer: its changes are merged".into(),
            ));
        }
        self.finish_offer(card_id, &seen, changes, judgements)
    }

    /// Moves the card from the column it was `seen` in to Ready to merge and puts each of `changes` in the inbox, only
    /// if the card is still there; otherwise the card is left where it is and the inbox is left alone.
    pub(crate) fn finish_offer(
        &self,
        card_id: i64,
        seen: &str,
        changes: Vec<Change>,
        judgements: Option<&str>,
    ) -> Result<(Vec<Change>, bool)> {
        if self
            .move_card_from(card_id, seen, columns::READY_TO_MERGE, None, None)?
            .is_none()
        {
            return Ok((changes, false));
        }
        let card = self.card(card_id)?;
        let changes = changes
            .into_iter()
            .map(|change| self.put_in_inbox(change, &card.title, judgements))
            .collect::<Result<Vec<_>>>()?;
        Ok((changes, true))
    }

    /// What the pull request of a card's change says, in place of the `Closes #<number>` line alone: the line that
    /// closes the issue when the change is merged (for a change in another repository than the card's own, a line
    /// that names the card instead), for a card over several repositories the line that names them in the order they
    /// are merged, what the code run said it did, the stat of the files it changed,
    /// what the review found, and who made it. A section Moebius has nothing for is left out.
    pub(crate) fn pull_request_body(
        &self,
        card: &Card,
        repository: &Repository,
        branch: &str,
        judgements: Option<&str>,
    ) -> String {
        // The issue's number is one of the card's own repository; another repository's pull request names the card.
        let mut body = if repository.id == card.repository_id {
            format!("Closes #{}\n", card.number)
        } else {
            let own = self
                .repository(card.repository_id)
                .map_or_else(|_| String::new(), |r| format!(" in {}", r.name));
            format!(
                "Part of {}-{} (issue #{}{own})\n",
                card.project_key, card.project_number, card.number
            )
        };
        // A card over several repositories is merged together (ADR 0019): each pull request says with which.
        if card.repositories.len() > 1 {
            body.push_str(&format!(
                "\nOne of the changes of {}-{} in {}, which Moebius merges together, in that order.\n",
                card.project_key,
                card.project_number,
                card.repositories.join(", ")
            ));
        }
        if let Some(said) = self.what_the_code_run_said(card.id) {
            body.push_str(&format!(
                "\n## What changed\n\n{}\n",
                cut(&said, SAID_AT_MOST)
            ));
        }
        if let Some(stat) = self.files_changed(repository, branch) {
            body.push_str(&format!("\n## Files changed\n\n```\n{stat}\n```\n"));
        }
        if let Some(judgements) = judgements.map(str::trim).filter(|text| !text.is_empty()) {
            body.push_str(&format!(
                "\n## Review\n\n{}\n",
                cut(judgements, JUDGEMENTS_AT_MOST)
            ));
        }
        body.push_str("\nMade by Moebius.\n");
        body
    }

    /// What the card's latest successful `code` run answered last, without the lines that name its reproduction test
    /// or ask for a repository: those are for the board, not for a reader of the pull request.
    fn what_the_code_run_said(&self, card_id: i64) -> Option<String> {
        let said = self
            .store
            .runs_for_card(card_id)
            .ok()?
            .into_iter()
            .rev()
            .find(|run| run.purpose == "code" && run.status == "succeeded")?
            .summary?;
        let kept = said
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                !line.starts_with("REPRODUCE:")
                    && !line.starts_with("REPRODUCE-FILE:")
                    && !line.starts_with(crate::app::card_repositories::ASKS_FOR)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let kept = kept.trim();
        (!kept.is_empty()).then(|| kept.to_string())
    }
}
