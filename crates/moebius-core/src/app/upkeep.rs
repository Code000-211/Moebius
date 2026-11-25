//! Stale handling (ADR 0032): what keeps an owner from tending issues nobody works on and pull requests nobody pushes
//! to. Once a day, before the digest, each repository on a forge is looked at with its own settings (`Upkeep`):
//!
//! - an issue a helper took (or was assigned) and that has no followed pull request after `release_after_days` is
//!   released: the helper is taken off it with a reply on the issue, so that it can be taken again;
//! - a followed pull request whose last review asked for changes and that saw no push for `stale_after_days` gets a
//!   reminder comment, once for each head; after `close_after_days` it is closed with a polite comment and its issue is
//!   released.
//!
//! Nothing is done for a card that is held, nor while **Pause all** is on. Every action is written to the log the
//! digest reads, and one the forge refuses is reported with the forge's other errors and left for the next day.

use serde_json::json;

use super::{App, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::{Card, Change, Repository, Upkeep, now};

const DAY: i64 = 86_400;

/// What a look at the repositories did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Upkept {
    pub released: usize,
    pub reminded: usize,
    pub closed: usize,
}

impl App {
    /// Why the stale handling leaves the card alone, or none when it may act on it.
    pub(crate) fn upkeep_exempt(&self, card: &Card) -> Result<Option<&'static str>> {
        if card.held.is_some() {
            return Ok(Some("held"));
        }
        if self.runs_paused()?.is_some() {
            return Ok(Some("paused"));
        }
        Ok(None)
    }

    /// Looks at every repository on a forge once, with its settings; says what it did.
    pub fn run_upkeep(&self) -> Result<Upkept> {
        let mut done = Upkept::default();
        for repository in self.store.repositories()? {
            if repository.forge == "none" || repository.files_of.is_some() {
                continue;
            }
            let upkeep = self.store.upkeep(repository.id)?;
            let say = |what: &str, e: &dyn std::fmt::Display| {
                self.bus.publish(Event::ForgeError {
                    repository: repository.name.clone(),
                    message: format!("stale handling: {what}: {e}"),
                });
            };
            for card in self.store.cards(repository.id)? {
                if card.closed {
                    continue;
                }
                match self.auto_release(&repository, &card, &upkeep) {
                    Ok(true) => done.released += 1,
                    Ok(false) => {}
                    Err(e) => say(&format!("releasing #{}", card.number), &e),
                }
            }
            for change in self.store.changes(repository.id)? {
                if change.state != "open" || change.outside_author.is_none() {
                    continue;
                }
                match self.stale_request(&repository, &change, &upkeep) {
                    Ok(Some(true)) => done.closed += 1,
                    Ok(Some(false)) => done.reminded += 1,
                    Ok(None) => {}
                    Err(e) => say(
                        &format!("pull request #{}", change.pull_request.unwrap_or_default()),
                        &e,
                    ),
                }
            }
        }
        Ok(done)
    }

    /// Releases the issue of `card` when a helper took it `release_after_days` ago and no pull request of theirs is
    /// followed. The first time an assignee is seen starts their time.
    fn auto_release(&self, repository: &Repository, card: &Card, upkeep: &Upkeep) -> Result<bool> {
        let Some(login) = card.taken_by.clone() else {
            return Ok(false);
        };
        if upkeep.release_after_days <= 0
            || self.upkeep_exempt(card)?.is_some()
            || self.follows_outside_request(card)?
        {
            return Ok(false);
        }
        let Some(since) = self.store.assigned_since(card.id)? else {
            self.store.set_assigned_since(card.id, now())?;
            return Ok(false);
        };
        let days = (now() - since) / DAY;
        if days < upkeep.release_after_days {
            return Ok(false);
        }
        self.unassign_taken(repository, card, &login)?;
        let text = format!(
            "Released: @{login} took this issue {days} days ago and no pull request followed, so it is free again. \
             Anyone may take it."
        );
        self.tell_issue(repository, card, &text);
        self.store.note_activity(
            "auto_release",
            Some(repository.id),
            Some(card.number),
            Some(&login),
            &format!("taken {days} days ago, no pull request followed"),
            &json!({ "card": card.id, "title": card.title }),
        )?;
        Ok(true)
    }

    /// When the latest review that asked for changes was written, when it was written for the head the pull request has
    /// now: a push after it means the author has answered and the reviewer is the one who is waited for. The forge's own
    /// latest review says so where the forge gives the commit of a review (GitHub does); otherwise the Reviewer's latest
    /// round with a verdict does. A time of 0 is a review whose time the forge did not say.
    fn changes_asked_at(
        &self,
        repository: &Repository,
        change: &Change,
        number: i64,
    ) -> Result<Option<i64>> {
        let head = change.head.as_deref();
        if let Ok(Some(mark)) = self.forge(repository)?.latest_review(repository, number) {
            let of_the_head = match (&mark.head, head) {
                (Some(reviewed), Some(now)) => reviewed == now,
                _ => true,
            };
            return Ok((mark.state == "changes_requested" && of_the_head).then_some(mark.at));
        }
        Ok(self
            .store
            .external_reviews(change.id)?
            .into_iter()
            .rev()
            .find(|r| r.verdict.is_some())
            .filter(|r| r.verdict.as_deref() == Some("changes") && Some(r.head.as_str()) == head)
            .map(|r| r.at))
    }

    /// Whether the stale handling already tried `action` on the pull request `number` at this head and the forge
    /// refused: the refusal is told once, in the digest, and the action is not tried again for that head.
    fn refused_before(
        &self,
        repository: &Repository,
        number: i64,
        action: &str,
        head: &Option<String>,
    ) -> Result<bool> {
        Ok(self
            .store
            .activity_about("stale_failed", repository.id, number)?
            .iter()
            .any(|a| a.data["action"] == action && a.data["head"] == json!(head)))
    }

    /// Reminds, or closes, the followed pull request `change` when its last review asked for changes, of the head it has
    /// now, and nobody pushed or reviewed for the days a repository sets. Says `Some(true)` for one it closed,
    /// `Some(false)` for one it reminded, none for one it left.
    fn stale_request(
        &self,
        repository: &Repository,
        change: &Change,
        upkeep: &Upkeep,
    ) -> Result<Option<bool>> {
        let (Some(number), Some(card_id), Some(author)) = (
            change.pull_request,
            change.card_id,
            change.outside_author.as_deref(),
        ) else {
            return Ok(None);
        };
        let card = self.card(card_id)?;
        if self.upkeep_exempt(&card)?.is_some() {
            return Ok(None);
        }
        let Some(asked) = self.changes_asked_at(repository, change, number)? else {
            return Ok(None);
        };
        // The author's turn began with the later of the review and the last push.
        let since = asked.max(change.published_at.unwrap_or(0));
        let idle = (now() - since) / DAY;
        let forge = self.forge(repository)?;
        let data = json!({ "card": card.id, "title": card.title, "change": change.id, "head": change.head });
        let failed = |action: &str, why: &dyn std::fmt::Display| {
            self.store.note_activity(
                "stale_failed",
                Some(repository.id),
                Some(number),
                Some(author),
                &why.to_string(),
                &json!({ "card": card.id, "title": card.title, "head": change.head, "action": action }),
            )
        };
        if upkeep.close_after_days > 0 && idle >= upkeep.close_after_days {
            if self.refused_before(repository, number, "close", &change.head)? {
                return Ok(None);
            }
            // Closed first: a close the forge refuses is told once, not commented on every day.
            if let Err(e) = forge.close_request(repository, number) {
                failed("close", &e)?;
                return Ok(None);
            }
            let text = format!(
                "Closing this pull request politely: a review asked for changes and nothing was pushed for {idle} \
                 days since. Thank you for the work, @{author}. The issue is free again, and you are welcome to \
                 open a new pull request."
            );
            if let Err(e) = forge.comment_request(repository, number, &text) {
                self.bus.publish(Event::ForgeError {
                    repository: repository.name.clone(),
                    message: format!(
                        "stale handling: the comment on closed pull request #{number}: {e}"
                    ),
                });
            }
            self.followed_closed(change)?;
            let card = self.card(card_id)?;
            self.unassign_taken(repository, &card, author)?;
            self.store.note_activity(
                "stale_closed",
                Some(repository.id),
                Some(number),
                Some(author),
                &format!("no push for {idle} days after a review asked for changes"),
                &data,
            )?;
            return Ok(Some(true));
        }
        if upkeep.stale_after_days <= 0 || idle < upkeep.stale_after_days {
            return Ok(None);
        }
        let reminded = self
            .store
            .activity_about("stale_reminder", repository.id, number)?
            .iter()
            .any(|a| a.data["head"] == json!(change.head));
        if reminded || self.refused_before(repository, number, "remind", &change.head)? {
            return Ok(None);
        }
        let close = match upkeep.close_after_days {
            days if days > 0 => format!(" It is closed after {days} days without a push."),
            _ => String::new(),
        };
        let text = format!(
            "A reminder for @{author}: the last review asked for changes and nothing was pushed for {idle} days. \
             Push your changes to have it reviewed again.{close}"
        );
        if let Err(e) = forge.comment_request(repository, number, &text) {
            failed("remind", &e)?;
            return Ok(None);
        }
        self.store.note_activity(
            "stale_reminder",
            Some(repository.id),
            Some(number),
            Some(author),
            &format!("no push for {idle} days after a review asked for changes"),
            &data,
        )?;
        Ok(Some(false))
    }

    /// Writes `text` on the card's issue; one that cannot be written is reported, since what it tells has been done.
    fn tell_issue(&self, repository: &Repository, card: &Card, text: &str) {
        let by = Actor::Script {
            column: "upkeep".into(),
        };
        if let Err(e) = self.comment_issue(&by, card.id, text) {
            self.bus.publish(Event::ForgeError {
                repository: repository.name.clone(),
                message: format!("stale handling: the reply on #{}: {e}", card.number),
            });
        }
    }
}
