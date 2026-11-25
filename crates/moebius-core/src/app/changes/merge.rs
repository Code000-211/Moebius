//! Merging a card's changes together (ADR 0019): every open change of the card, one after another in the order the
//! card names its repositories. There is no atomic merge across forges, so a merge that fails stops there: what is
//! merged stays merged, the card stays where it is with an inbox item `merge_stopped` that says what is merged and what
//! is not, and the card is done once its last change is merged, by a later merge of the card or of that change.

use serde_json::json;

use super::super::{App, AppError, Result};
use crate::decide::Actor;
use crate::store::{Card, Change};

/// What a merge of a card's changes did (see [`App::merge_card`]).
#[derive(Debug)]
pub struct CardMerge {
    /// The changes this merge merged, in order, each with its merge commit.
    pub merged: Vec<(Change, String)>,
    /// The change whose merge failed, and why; none when every open change merged and the card is done.
    pub stopped: Option<(Change, String)>,
}

impl App {
    /// Merges every open change of the card in the order the card names its repositories, each as
    /// [`App::merge_change`] does; the card moves to Done with its last. The first merge that fails stops the rest:
    /// the card stays where it is, and an inbox item `merge_stopped` names the repositories whose changes are merged
    /// and those whose changes are still open.
    pub fn merge_card(&self, by: &Actor, card_id: i64, reason: &str) -> Result<CardMerge> {
        let card = self.card(card_id)?;
        let changes = self.open_changes(&card)?;
        if changes.is_empty() {
            return Err(AppError::Invalid(
                "the card has no open change to merge".into(),
            ));
        }
        let mut merged = Vec::new();
        for change in changes {
            match self.merge_one(by, change.id, reason, None) {
                Ok(commit) => merged.push((change, commit)),
                Err(e) => {
                    let why = e.to_string();
                    // The gate has written its own inbox item, which names the reason.
                    if !matches!(e, AppError::Refused(_)) {
                        self.merge_stopped(&card, &change, &why)?;
                    }
                    return Ok(CardMerge {
                        merged,
                        stopped: Some((change, why)),
                    });
                }
            }
        }
        Ok(CardMerge {
            merged,
            stopped: None,
        })
    }

    /// Refuses to merge a change of a card while a change before it, in the order the card names its repositories,
    /// is open: a card's changes are merged in that order.
    pub(super) fn check_merge_order(&self, change: &Change) -> Result<()> {
        let Some(card_id) = change.card_id else {
            return Ok(());
        };
        let card = self.card(card_id)?;
        match self.open_changes(&card)?.first() {
            Some(first) if first.id != change.id => {
                let name = self.repository_of(first)?.name;
                Err(AppError::Invalid(format!(
                    "merge the change in {name} first: a card's changes are merged in the order the card names its \
                     repositories"
                )))
            }
            _ => Ok(()),
        }
    }

    /// Marks the card whose merge stopped at `change` with one open `merge_stopped` item: the change, why it failed,
    /// the repositories of the card whose changes are merged (`merged`) and those whose are still open (`open`), the
    /// failed one first. The same change failing again keeps the item; another one replaces it.
    pub(super) fn merge_stopped(&self, card: &Card, change: &Change, why: &str) -> Result<()> {
        let open_item = self
            .store
            .inbox_for_card(card.id)?
            .into_iter()
            .find(|item| item.kind == "merge_stopped" && item.resolved_at.is_none());
        if open_item.is_some_and(|item| item.data["change"] == change.id) {
            return Ok(());
        }
        let mut merged = Vec::new();
        let mut open = Vec::new();
        for repository in self.card_repositories(card)? {
            let latest = self.latest_change_in(card, repository.id)?;
            match latest.map(|c| c.state) {
                Some(state) if state == "merged" => merged.push(repository.name),
                Some(state) if state == "open" => open.push(repository.name),
                _ => {}
            }
        }
        let data = json!({
            "change": change.id,
            "repository": self.repository_of(change)?.name,
            "reason": why,
            "merged": merged,
            "open": open,
        });
        self.store
            .resolve_inbox_kind_for_card(card.id, "merge_stopped")?;
        self.store
            .add_inbox_item("merge_stopped", Some(card.id), &card.title, &data)?;
        // The daily digest tells why the merge stopped.
        let _ = self.store.note_activity(
            "merge_failed",
            change.repository_id,
            Some(card.number),
            None,
            why,
            &json!({ "change": change.id, "card": card.id, "title": card.title }),
        );
        Ok(())
    }

    /// Whether another change of the card than `change` is merged: a failed merge of `change` then leaves the card
    /// half merged.
    pub(super) fn partly_merged(&self, card: &Card, change: &Change) -> Result<bool> {
        for repository in self.card_repositories(card)? {
            if repository.id == change.repository_id.unwrap_or_default() {
                continue;
            }
            let latest = self.latest_change_in(card, repository.id)?;
            if latest.is_some_and(|c| c.state == "merged") {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The first change, in the order the card names its repositories, that is the card's latest in its repository
    /// and was closed without being merged: the card is not done while there is one.
    pub(super) fn closed_change(&self, card: &Card) -> Result<Option<Change>> {
        for repository in self.card_repositories(card)? {
            if let Some(change) = self.latest_change_in(card, repository.id)?
                && change.state == "closed"
            {
                return Ok(Some(change));
            }
        }
        Ok(None)
    }

    /// The card's latest change in the repository, which says how its work there stands; none before the first.
    fn latest_change_in(&self, card: &Card, repository_id: i64) -> Result<Option<Change>> {
        Ok(self
            .store
            .changes(repository_id)?
            .into_iter()
            .rfind(|change| change.card_id == Some(card.id)))
    }
}
