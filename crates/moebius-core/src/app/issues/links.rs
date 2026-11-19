//! Duplicates and related issues. An issue is marked a duplicate of another, the original: it is closed with a comment
//! that points to the original, and the link is kept, so that the original lists its duplicates. Issues are related
//! in both directions, so that each shows the other. Moebius keeps these links (a forge has no link that all of them
//! share); the comment and the closing reach the forge as for any issue. When an issue is made, the issues alike in
//! words (see `similar`) are offered in a comment of the Moebius agent, which closes nothing.

use super::similar::{Similar, likely};
use super::{App, AppError, Child, Result, state_of};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::{Card, DUPLICATE, RELATED};

/// What an issue is linked to, besides its parent and blockers: where it is a duplicate, the issues that are duplicates
/// of it and the issues related to it.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Links {
    /// The original this issue was closed as a duplicate of.
    pub duplicate_of: Option<Child>,
    /// The issues closed as duplicates of this one.
    pub duplicates: Vec<Child>,
    pub related: Vec<Child>,
}

/// An issue as another's links show it.
fn linked(card: &Card) -> Child {
    Child {
        id: card.id,
        number: card.number,
        title: card.title.clone(),
        state: state_of(card).into(),
        column: card.column.clone(),
    }
}

impl App {
    /// The links of the issue `card`.
    pub fn links_of(&self, card: &Card) -> Result<Links> {
        let cards = |ids: Vec<i64>| -> Result<Vec<Child>> {
            let mut found = Vec::new();
            for id in ids {
                if let Some(card) = self.store.card(id)? {
                    found.push(linked(&card));
                }
            }
            Ok(found)
        };
        let original = self.store.links_from(card.id, DUPLICATE)?;
        let related = self.store.links_from(card.id, RELATED)?;
        Ok(Links {
            duplicate_of: cards(original.into_iter().map(|l| l.other_id).collect())?
                .into_iter()
                .next(),
            duplicates: cards(self.store.links_to(card.id, DUPLICATE)?)?,
            related: cards(related.into_iter().map(|l| l.other_id).collect())?,
        })
    }

    /// Announces the links of the issue `card_id` as they are now.
    fn links_changed(&self, card_id: i64) -> Result<()> {
        let links = self.links_of(&self.card(card_id)?)?;
        let numbers = |cards: &[Child]| cards.iter().map(|c| c.number).collect();
        self.bus.publish(Event::CardLinksChanged {
            card: card_id,
            duplicate_of: links.duplicate_of.map(|c| c.number),
            duplicates: numbers(&links.duplicates),
            related: numbers(&links.related),
        });
        Ok(())
    }

    /// Marks the card's issue a duplicate of the issue `original` (a number): a comment says "Duplicate of #n", the issue
    /// is closed with that reason, and the link is kept. Marking it as the duplicate of another issue moves the link.
    /// Refused for the issue itself, an original that is a duplicate itself (point to its original), and an issue that
    /// has duplicates of its own (move them first). Announces `card_links_changed` for both issues.
    pub fn mark_duplicate(&self, by: &Actor, card_id: i64, original: i64) -> Result<Card> {
        let card = self.card(card_id)?;
        let original = self.related(&card, original, "original")?;
        if let Some(first) = self.store.links_from(original.id, DUPLICATE)?.first() {
            let first = self.card(first.other_id)?;
            return Err(AppError::Invalid(format!(
                "#{} is a duplicate of #{} itself: mark #{} as a duplicate of that",
                original.number, first.number, card.number
            )));
        }
        if !self.store.links_to(card.id, DUPLICATE)?.is_empty() {
            return Err(AppError::Invalid(format!(
                "other issues are duplicates of #{}: mark them as duplicates of #{} first",
                card.number, original.number
            )));
        }
        let before = self.store.links_from(card.id, DUPLICATE)?;
        if before.iter().any(|l| l.other_id == original.id) {
            return Ok(card);
        }
        // Closing is what can be refused (a run is going), so it comes first and nothing is left half made.
        let reason = format!("Duplicate of #{}: {}", original.number, original.title);
        self.close_issue(by, card_id, &reason)?;
        self.comment_issue(by, card_id, &format!("Duplicate of #{}.", original.number))?;
        for old in &before {
            self.store.remove_link(card_id, old.other_id, DUPLICATE)?;
        }
        self.store.add_link(card_id, original.id, DUPLICATE)?;
        self.links_changed(card_id)?;
        self.links_changed(original.id)?;
        for old in before {
            self.links_changed(old.other_id)?;
        }
        self.card(card_id)
    }

    /// Takes the duplicate link off the card's issue. The issue stays closed: reopening it is a step of its own.
    /// Announces `card_links_changed` for both issues.
    pub fn unmark_duplicate(&self, card_id: i64) -> Result<Card> {
        let card = self.card(card_id)?;
        let Some(link) = self.store.links_from(card_id, DUPLICATE)?.pop() else {
            return Err(AppError::NotFound(format!(
                "a duplicate link of #{}",
                card.number
            )));
        };
        self.store.remove_link(card_id, link.other_id, DUPLICATE)?;
        self.links_changed(card_id)?;
        self.links_changed(link.other_id)?;
        Ok(card)
    }

    /// Relates the card's issue to the issue `number`, in both directions. Relating what is related changes nothing.
    /// Announces `card_links_changed` for both issues.
    pub fn add_related(&self, card_id: i64, number: i64) -> Result<Links> {
        let card = self.card(card_id)?;
        let other = self.related(&card, number, "related issue")?;
        if self
            .store
            .links_from(card_id, RELATED)?
            .iter()
            .all(|l| l.other_id != other.id)
        {
            self.store.add_link(card_id, other.id, RELATED)?;
            self.store.add_link(other.id, card_id, RELATED)?;
            self.links_changed(card_id)?;
            self.links_changed(other.id)?;
        }
        self.links_of(&card)
    }

    /// Stops the issue `number` from being related to the card's issue, in both directions. Announces
    /// `card_links_changed` for both issues.
    pub fn remove_related(&self, card_id: i64, number: i64) -> Result<Links> {
        let card = self.card(card_id)?;
        let other = self
            .store
            .card_by_number(card.repository_id, number)?
            .ok_or_else(|| AppError::NotFound(format!("issue #{number}")))?;
        if !self.store.remove_link(card_id, other.id, RELATED)? {
            return Err(AppError::NotFound(format!(
                "#{number} among the issues related to #{}",
                card.number
            )));
        }
        self.store.remove_link(other.id, card_id, RELATED)?;
        self.links_changed(card_id)?;
        self.links_changed(other.id)?;
        self.links_of(&card)
    }

    /// The open issues of the card's repository that are likely the same as the card's issue, the most alike first,
    /// leaving out those of another project that is private.
    pub fn similar_issues(&self, card_id: i64) -> Result<Vec<Similar>> {
        let card = self.card(card_id)?;
        // A card of another project that is private is never offered: its title would show on this card's board.
        let private: std::collections::HashSet<i64> = self.store.hidden_projects(None)?;
        let mut cards = self.store.cards(card.repository_id)?;
        cards.retain(|other| {
            other.project_id == card.project_id || !private.contains(&other.project_id)
        });
        Ok(likely(&card, cards))
    }

    /// Offers the likely duplicates of the new issue `card` in a comment of the Moebius agent, which closes nothing and
    /// says what to do if one is the same. Says nothing when there are none.
    pub(super) fn offer_duplicates(&self, card: &Card) -> Result<()> {
        let found = self.similar_issues(card.id)?;
        if found.is_empty() {
            return Ok(());
        }
        let list: Vec<String> = found
            .iter()
            .map(|s| format!("- #{}: {}", s.number, s.title))
            .collect();
        let body = format!(
            "These open issues look like this one:\n{}\n\nIf this is the same issue, mark it as a duplicate of that one; if they only belong together, relate them. Nothing was closed.",
            list.join("\n")
        );
        let moebius = Actor::Agent {
            name: "Moebius".into(),
            run: 0,
        };
        self.comment_issue(&moebius, card.id, &body)?;
        Ok(())
    }
}
