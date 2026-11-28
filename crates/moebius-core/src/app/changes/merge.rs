//! Merging a card's changes together (ADR 0019): every open change of the card, one after another in the order the
//! card names its repositories. There is no atomic merge across forges, so a merge that fails stops there: what is
//! merged stays merged, the card stays where it is with an inbox item `merge_stopped` that says what is merged and what
//! is not, and the card is done once its last change is merged, by a later merge of the card or of that change.

use serde_json::json;

use super::super::{App, AppError, Result};
use crate::decide::Actor;
use crate::store::{Card, Change, InboxItem, now};

/// How long, in seconds, a column script's merge of a change waits before it is tried again after `failures` failures
/// in a row at the same head: 30 seconds, twice as long after each failure, at most ten minutes. A forge that refuses
/// a merge (a draft someone made, a branch protection) answers the same until a person acts, so the merge is not
/// decided again at every tick.
pub(crate) fn merge_retry_wait(failures: i64) -> i64 {
    (30_i64 << (failures - 1).clamp(0, 5)).min(600)
}

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
    /// and those whose changes are still open. The same change at the same head is not tried again until the wait the
    /// item gives is over (see [`merge_retry_wait`]): it fails as it did, and nothing is decided.
    pub fn merge_card(&self, by: &Actor, card_id: i64, reason: &str) -> Result<CardMerge> {
        let card = self.card(card_id)?;
        let changes = self.open_changes(&card)?;
        if changes.is_empty() {
            return Err(AppError::Invalid(
                "the card has no open change to merge".into(),
            ));
        }
        if let Some(why) = self.merge_waiting(&changes[0])? {
            return Ok(CardMerge {
                merged: Vec::new(),
                stopped: Some((changes[0].clone(), why)),
            });
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

    /// Why the merge of `change` failed last, while the card's open `merge_stopped` item is about it at its head and
    /// the wait before it is tried again is not over; none when it may be tried now.
    fn merge_waiting(&self, change: &Change) -> Result<Option<String>> {
        let Some(card_id) = change.card_id else {
            return Ok(None);
        };
        let waiting = self.open_merge_stopped(card_id)?.filter(|item| {
            item.data["change"] == change.id
                && item.data["head"].as_str() == change.head.as_deref()
                && item.data["retry_at"].as_i64().is_some_and(|at| at > now())
        });
        Ok(waiting.map(|item| item.data["reason"].as_str().unwrap_or_default().to_string()))
    }

    /// The card's open `merge_stopped` item, if it has one.
    fn open_merge_stopped(&self, card_id: i64) -> Result<Option<InboxItem>> {
        Ok(self
            .store
            .inbox_for_card(card_id)?
            .into_iter()
            .find(|item| item.kind == "merge_stopped" && item.resolved_at.is_none()))
    }

    /// Marks the card whose merge stopped at `change` with one open `merge_stopped` item: the change, why it failed,
    /// the repositories of the card whose changes are merged (`merged`) and those whose are still open (`open`), the
    /// failed one first, how many times in a row it failed (`failures`), at which head (`head`), and when it is tried
    /// again (`retry_at`). The same change failing again keeps the item, with its reason, head and wait brought up to
    /// date; another one replaces it.
    pub(super) fn merge_stopped(&self, card: &Card, change: &Change, why: &str) -> Result<()> {
        if let Some(item) = self
            .open_merge_stopped(card.id)?
            .filter(|item| item.data["change"] == change.id)
        {
            let failures = item.data["failures"].as_i64().unwrap_or(1) + 1;
            let mut data = item.data;
            data["failures"] = json!(failures);
            data["head"] = json!(change.head);
            data["reason"] = json!(why);
            data["retry_at"] = json!(now() + merge_retry_wait(failures));
            self.store.set_inbox_item_data(item.id, &data)?;
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
            "failures": 1,
            "head": change.head,
            "retry_at": now() + merge_retry_wait(1),
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

#[cfg(test)]
mod tests {
    use super::merge_retry_wait;

    #[test]
    fn the_wait_before_a_failed_merge_is_tried_again_doubles_up_to_ten_minutes() {
        let waits: Vec<i64> = (1..=8).map(merge_retry_wait).collect();
        assert_eq!(waits, [30, 60, 120, 240, 480, 600, 600, 600]);
    }
}
