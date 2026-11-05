//! What the poll reads of an issue besides its text, and the rules that keep the tracker sound: which sub-issues count,
//! and which edges would make issues wait for each other for ever.
//!
//! The poll acts on what the forge says has *changed*. For each field (state, parent, labels, blockers, comment count)
//! the store keeps what the forge was last seen to say (`forge_seen`); a snapshot that says the same is not a change,
//! whatever Moebius has written since. Moebius records what it wrote as what the forge says (and when the write ended),
//! so that someone who puts the old value back on the forge before the next poll is noticed, while a snapshot whose
//! read began before that write ended changes nothing: it may not have seen the write, and the issue is read again
//! after it. Both times are Moebius's own clock. What the poll cannot apply is not recorded, so that it is tried again.

use std::collections::{BTreeMap, BTreeSet};

use super::text::part_of;
use super::{App, Result, blockers_in, columns};
use crate::bus::Event;
use crate::repos::forge::{Comment as ForgeComment, Issue};
use crate::store::Card;

/// What each issue of a repository waits for, by issue number: the issues that block it, and its sub-issues, because an
/// issue with sub-issues is not done before they are.
pub(super) type Waits = BTreeMap<i64, Vec<i64>>;

/// Whether the issue `from` waits, directly or through others, for the issue `to` (or is it).
pub(super) fn waits_for(waits: &Waits, from: i64, to: i64) -> bool {
    let mut pending = vec![from];
    let mut seen = BTreeSet::new();
    while let Some(number) = pending.pop() {
        if number == to {
            return true;
        }
        if seen.insert(number) {
            pending.extend(waits.get(&number).into_iter().flatten().copied());
        }
    }
    false
}

/// The field of `forge_seen` that holds when Moebius last finished writing to the issue, in milliseconds.
const WRITTEN: &str = "written_at";

/// Milliseconds since the epoch.
pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn text<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

impl App {
    /// Whether the issue of `card` counts as a sub-issue of its parent: one the board may act on, which is one a person
    /// can have vouched for, not any issue whose description says "Part of #n". The author of the issue is trusted, or a
    /// person approved the issue, or it came from a grilling the person confirmed. Anyone else's issue is shown under its
    /// parent but does not hold the parent back, close it or count in it.
    pub fn counts_as_sub_issue(&self, card: &Card) -> bool {
        self.is_trusted(card)
            || card.confirmed
            || self
                .store
                .judgements_for_card(card.id)
                .is_ok_and(|judgements| {
                    judgements
                        .iter()
                        .any(|j| j.kind == "approve_issue" && j.choice == "approve")
                })
    }

    /// What each issue of the repository waits for (see [`Waits`]). A sub-issue that does not count (an outsider's that
    /// nobody approved) is waited for by nobody, so it takes no part in what would make issues wait for each other.
    pub(super) fn waits(&self, repository_id: i64) -> Result<Waits> {
        let mut waits = Waits::new();
        for (number, by) in self.store.dependency_edges(repository_id)? {
            waits.entry(number).or_default().push(by);
        }
        for card in self.store.cards(repository_id)? {
            if let Some(parent) = card.parent
                && self.counts_as_sub_issue(&card)
            {
                waits.entry(parent).or_default().push(card.number);
            }
        }
        Ok(waits)
    }

    /// Re-arms the parent of the closed sub-issue `card`: a parent a person reopened is left open until one of its
    /// sub-issues closes after that (see [`App::close_for_script`]).
    pub(in crate::app) fn sub_issue_closed(&self, card: &Card) -> Result<()> {
        if let Some(parent) = card.parent
            && self.counts_as_sub_issue(card)
            && let Some(parent) = self.store.card_by_number(card.repository_id, parent)?
        {
            self.store.set_kept_open(parent.id, false)?;
        }
        Ok(())
    }

    /// Records that Moebius wrote to the issue of `card_id` on the forge, if `on_forge`, which it has just done: what the
    /// forge now says is what the card holds. A later snapshot that says something else is then a change of the forge's,
    /// unless it was read before the write ended (see [`App::stale_snapshot`]).
    pub(super) fn wrote_forge(&self, card_id: i64, on_forge: bool) -> Result<()> {
        if !on_forge {
            return Ok(());
        }
        let card = self.card(card_id)?;
        let (id, number) = (card.repository_id, card.number);
        let state = if card.closed { "closed" } else { "open" };
        let parent = card.parent.map(|p| p.to_string()).unwrap_or_default();
        let blockers: BTreeSet<i64> = self.store.dependencies(card.id)?.into_iter().collect();
        for (field, value) in [
            ("state", state.to_string()),
            ("parent", parent),
            ("labels", text(&card.labels.iter().collect::<BTreeSet<_>>())),
            ("blockers", text(&blockers)),
            ("body_blockers", text(&blockers_in(&card.body))),
            (WRITTEN, now_millis().to_string()),
        ] {
            self.store.set_forge_seen(id, number, field, &value)?;
        }
        Ok(())
    }

    /// Whether a snapshot of the forge whose read began at `read_at` (milliseconds since the epoch) is older than the
    /// last write of Moebius's to the issue: the write ended after the read began, so the snapshot may say what the
    /// forge said before it.
    pub(in crate::app) fn stale_snapshot(
        &self,
        repository_id: i64,
        number: i64,
        read_at: i64,
    ) -> Result<bool> {
        let written = self
            .store
            .forge_seen(repository_id, number, WRITTEN)?
            .and_then(|at| at.parse::<i64>().ok())
            .unwrap_or(0);
        Ok(written > read_at)
    }

    /// Whether the forge says something else about `field` of the issue than it said when it was last read. It is
    /// recorded with [`App::forge_saw`] once the change is applied, so that one that could not be is not lost.
    fn forge_differs(&self, card: &Card, field: &str, now: &str) -> Result<bool> {
        Ok(self
            .store
            .forge_seen(card.repository_id, card.number, field)?
            .as_deref()
            != Some(now))
    }

    fn forge_saw(&self, card: &Card, field: &str, now: &str) -> Result<()> {
        Ok(self
            .store
            .set_forge_seen(card.repository_id, card.number, field, now)?)
    }

    /// Tells the person that the forge said something the board does not take, as a forge error.
    fn refuse_edge(&self, card: &Card, message: String) -> Result<()> {
        self.bus.publish(Event::ForgeError {
            repository: self.repository(card.repository_id)?.name,
            message: format!("#{}: {message}", card.number),
        });
        Ok(())
    }

    /// Brings what an issue has besides its text in line with the forge's copy of it (see `sync_issues`): its state,
    /// parent, labels, blockers and comments. Each change is announced as the API's own changes are, and only what the
    /// forge changed is taken (see the module).
    pub(in crate::app) fn sync_issue_details(&self, card: &Card, issue: &Issue) -> Result<()> {
        let id = card.id;

        let state = if issue.open { "open" } else { "closed" };
        if self.forge_differs(card, "state", state)? {
            let raw_closed = self.store.issue_closed(id)?;
            if !issue.open {
                let in_done = card.column == columns::DONE;
                if !raw_closed {
                    self.store.set_issue_closed(id, true)?;
                    self.close_settled(id);
                    self.sub_issue_closed(card)?;
                }
                if !in_done {
                    self.move_card(id, columns::DONE)?;
                }
                // A card in Done already reflects it (a store from before issues were closed has them all): that is
                // not news.
                if !raw_closed && !in_done {
                    self.bus.publish(Event::CardClosed { card: id });
                }
            } else if raw_closed {
                self.reopened(id)?;
            }
            self.forge_saw(card, "state", state)?;
        }

        // A parent that is the issue itself, or that waits for it, would make the issues wait for each other.
        let forge_parent = issue.parent.or_else(|| part_of(&issue.body));
        let said = forge_parent.map(|p| p.to_string()).unwrap_or_default();
        if self.forge_differs(card, "parent", &said)? {
            let refused = match forge_parent {
                Some(parent) => {
                    parent == card.number
                        || waits_for(&self.waits(card.repository_id)?, card.number, parent)
                }
                None => false,
            };
            let parent = if refused { None } else { forge_parent };
            if parent != card.parent {
                self.store.set_parent(id, parent)?;
                self.bus
                    .publish(Event::CardParentChanged { card: id, parent });
            }
            match forge_parent {
                Some(forge_parent) if refused => self.refuse_edge(
                    card,
                    format!(
                        "the forge names #{forge_parent} as its parent, which would make them wait for each other; ignored"
                    ),
                )?,
                _ => self.forge_saw(card, "parent", &said)?,
            }
        }

        let labels: BTreeSet<String> = issue.labels.iter().cloned().collect();
        if self.forge_differs(card, "labels", &text(&labels))? {
            if labels != card.labels.iter().cloned().collect::<BTreeSet<_>>() {
                self.store.set_labels(id, &labels)?;
                self.bus.publish(Event::CardLabelsChanged {
                    card: id,
                    labels: labels.iter().cloned().collect(),
                });
            }
            self.forge_saw(card, "labels", &text(&labels))?;
        }

        self.sync_blockers(card, &issue.body, issue.blocked_by.as_deref())?;
        self.sync_comments(card, issue)
    }

    /// The comments the poll read, and what the forge counted and when it last changed the issue at that time: a comment
    /// deleted there is not read for ever, and a comment deleted and another written between two polls (the count is the
    /// same) is noticed by the issue's time, which a comment moves (see `poll_issues`).
    fn sync_comments(&self, card: &Card, issue: &Issue) -> Result<()> {
        let id = card.id;
        let (repository, number) = (card.repository_id, card.number);
        let Some(comments) = &issue.comments else {
            // Never read, and as far as the count says nothing to read: this is the time it is taken to be right at.
            if self
                .store
                .forge_seen(repository, number, "comments_at")?
                .is_none()
            {
                self.store
                    .set_forge_seen(repository, number, "comments_at", &issue.updated)?;
            }
            return Ok(());
        };
        for ForgeComment {
            id: forge_id,
            author,
            body,
            at,
        } in comments
        {
            let body = crate::blobs::redact(body);
            if let Some(comment) = self.store.add_comment(
                id,
                author,
                &body,
                Some(*at).filter(|at| *at > 0),
                Some(*forge_id),
            )? {
                self.bus.publish(Event::CardCommented { card: id, comment });
            }
        }
        self.store.set_forge_seen(
            repository,
            number,
            "comments",
            &issue.comment_count.to_string(),
        )?;
        self.store
            .set_forge_seen(repository, number, "comments_at", &issue.updated)?;
        Ok(())
    }

    /// The blockers: what the description names ("Blocked by #n") and what the forge keeps. A forge that could not be read
    /// for them (`blocked_by` is none) leaves the blockers it keeps as they are, and only what the description names
    /// changes. An edge that would make issues wait for each other is dropped with a forge error, and not recorded as
    /// seen, so that it is taken once the cycle is broken and the poll reads the issue again.
    pub(in crate::app) fn sync_blockers(
        &self,
        card: &Card,
        body: &str,
        kept: Option<&[i64]>,
    ) -> Result<()> {
        let id = card.id;
        let mut named = blockers_in(body);
        named.remove(&card.number);
        let named_text = text(&named);
        let stored: BTreeSet<i64> = self.store.dependencies(id)?.into_iter().collect();
        let before_named =
            self.store
                .forge_seen(card.repository_id, card.number, "body_blockers")?;

        let (mut next, said) = match kept {
            Some(kept) => {
                let mut all = named;
                all.extend(kept.iter().copied().filter(|n| *n != card.number));
                let said = text(&all);
                if !self.forge_differs(card, "blockers", &said)? {
                    self.forge_saw(card, "body_blockers", &named_text)?;
                    return Ok(());
                }
                (all, Some(said))
            }
            None => {
                if before_named.as_deref() == Some(&*named_text) {
                    return Ok(());
                }
                let before: BTreeSet<i64> = before_named
                    .and_then(|seen| serde_json::from_str(&seen).ok())
                    .unwrap_or_default();
                let mut all: BTreeSet<i64> = stored.difference(&before).copied().collect();
                all.extend(named);
                (all, None)
            }
        };
        // A blocker that waits for this issue (its own parent, say) would hold both for ever.
        let mut refused = Vec::new();
        if !next.is_empty() {
            let waits = self.waits(card.repository_id)?;
            next.retain(|blocker| {
                let cycle = waits_for(&waits, *blocker, card.number);
                if cycle {
                    refused.push(*blocker);
                }
                !cycle
            });
        }
        if next != stored {
            self.store.set_dependencies(id, &next)?;
            self.bus.publish(Event::CardBlockingChanged {
                card: id,
                blocked_by: next.into_iter().collect(),
            });
        }
        if refused.is_empty() {
            if let Some(said) = said {
                self.forge_saw(card, "blockers", &said)?;
            }
            self.forge_saw(card, "body_blockers", &named_text)?;
        } else {
            let numbers: Vec<String> = refused.iter().map(|n| format!("#{n}")).collect();
            self.refuse_edge(
                card,
                format!(
                    "the forge says {} block it, which would make issues wait for each other; ignored",
                    numbers.join(", ")
                ),
            )?;
        }
        Ok(())
    }

    /// Takes what a poll read of the blockers of an issue the forge did not list this time (it answered "not modified"):
    /// the blockers the forge keeps, read from `read_at` (milliseconds). An answer read before a write of Moebius's
    /// ended is left alone.
    pub fn reread_blockers(
        &self,
        repository_id: i64,
        number: i64,
        read_at: i64,
        kept: &[i64],
    ) -> Result<()> {
        let Some(card) = self.store.card_by_number(repository_id, number)? else {
            return Ok(());
        };
        if self.stale_snapshot(repository_id, number, read_at)? {
            return Ok(());
        }
        let card = self.card(card.id)?;
        self.sync_blockers(&card, &card.body.clone(), Some(kept))
    }

    /// The numbers of the issues of the repository that are open and have a card: the ones whose blockers are read.
    pub fn open_issue_numbers(&self, repository_id: i64) -> Result<BTreeSet<i64>> {
        Ok(self
            .store
            .cards(repository_id)?
            .into_iter()
            .filter(|card| !card.closed && card.column != columns::DONE)
            .map(|card| card.number)
            .collect())
    }
}
