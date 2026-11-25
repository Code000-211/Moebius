//! Pull requests that name no issue (ADR 0033). One that has no closing keyword and that no title or branch binds to an
//! issue (see `bind`) is asked once to name one, with a comment and the label `unbound`; after the repository's
//! `unbound_close_days` without one it is closed politely. Never done in a frozen repository, while agents are paused,
//! for a pull request of one of this server's people or of Moebius's own, for one against another branch, or in a
//! repository where commands or the server's answers on the forge are off (the owner has not asked for helpers there).

use std::collections::BTreeSet;

use super::UNBOUND_LABEL;
use crate::app::{App, OpenRequest, Result, closing_references};
use crate::bus::Event;
use crate::store::{Repository, now};

const DAY: i64 = 86_400;

impl App {
    /// A pull request that names an issue but is not followed: the label that said it names none goes, and when the
    /// issue cannot take it up for a lasting reason (it is done or closed, somebody else took it, or it has a pull
    /// request already) the pull request is told once. A pull request that cannot be followed for another reason (its
    /// author may not write and is no assignee, a run is going) is left as it is.
    fn settle_named(
        &self,
        repository: &Repository,
        request: &OpenRequest,
        named: &BTreeSet<i64>,
    ) -> Result<()> {
        let seen = self.store.unbound(repository.id, request.number)?;
        let forge = self.forge(repository)?;
        let report = |what: &str, e: &dyn std::fmt::Display| {
            self.bus.publish(Event::ForgeError {
                repository: repository.name.clone(),
                message: format!("pull request #{}: {what}: {e}", request.number),
            });
        };
        if seen.is_some_and(|s| s.labelled) {
            match forge.remove_request_label(repository, request.number, UNBOUND_LABEL) {
                Ok(()) => {
                    self.store
                        .set_unbound_done(repository.id, request.number, "unlabelled")?
                }
                Err(e) => report("the label unbound could not be taken off", &e),
            }
        }
        if seen.is_some_and(|s| s.explained) {
            return Ok(());
        }
        let Some(number) = named.iter().next().copied() else {
            return Ok(());
        };
        let Some(card) = self.store.card_by_number(repository.id, number)? else {
            return Ok(());
        };
        let why = if card.closed
            || self.column_class(card.project_id, &card.column) == crate::app::columns::DONE
        {
            format!("#{number} is done or closed")
        } else if let Some(taker) = card
            .taken_by
            .as_deref()
            .filter(|t| !t.eq_ignore_ascii_case(&request.author))
        {
            format!("#{number} is taken by @{taker}")
        } else if !self.open_changes(&card)?.is_empty() {
            format!("#{number} already has a pull request")
        } else {
            return Ok(());
        };
        self.store.note_unbound(repository.id, request.number)?;
        let text = format!(
            "Thanks @{}! This pull request names an issue, but it is not taken up: {why}. The owner can look at it.",
            request.author
        );
        match forge.comment_request(repository, request.number, &text) {
            Ok(_) => self
                .store
                .set_unbound_done(repository.id, request.number, "explained")?,
            Err(e) => report("the comment that says why it is not taken up", &e),
        }
        Ok(())
    }

    /// Looks at the open pull requests of `repository` after the poll followed those that name an issue: asks the others
    /// to name one, takes the label off one that was bound since, and closes one that waited long enough.
    pub fn nudge_unbound_requests(
        &self,
        repository: &Repository,
        requests: &[OpenRequest],
    ) -> Result<()> {
        let Some(name) = repository.forge_name() else {
            return Ok(());
        };
        let people: BTreeSet<String> = self
            .store
            .logins_on(&repository.forge)?
            .into_iter()
            .map(|(_, login)| login.to_lowercase())
            .collect();
        let known: BTreeSet<i64> = self
            .store
            .changes(repository.id)?
            .iter()
            .filter(|c| c.state != "closed" || c.outside_author.is_none())
            .filter_map(|c| c.pull_request)
            .collect();
        let active = repository.commands && repository.answer_on_forge;
        let flow = self.store.flow(repository.id)?;
        for request in requests {
            let report = |what: &str, e: &dyn std::fmt::Display| {
                self.bus.publish(Event::ForgeError {
                    repository: repository.name.clone(),
                    message: format!("pull request #{}: {what}: {e}", request.number),
                });
            };
            // Followed since: the label that said it named no issue goes.
            if known.contains(&request.number) {
                if let Some(seen) = self.store.unbound(repository.id, request.number)?
                    && seen.labelled
                {
                    match self.forge(repository)?.remove_request_label(
                        repository,
                        request.number,
                        UNBOUND_LABEL,
                    ) {
                        Ok(()) => self.store.set_unbound_done(
                            repository.id,
                            request.number,
                            "unlabelled",
                        )?,
                        Err(e) => report("the label unbound could not be taken off", &e),
                    }
                }
                continue;
            }
            let text = format!("{}\n{}", request.title, request.body);
            if !active
                || people.contains(&request.author.to_lowercase())
                || request.base != repository.default_branch
                || self.frozen(repository.id)?.is_some()
                || self.runs_paused()?.is_some()
            {
                continue;
            }
            // A pull request that names an issue, by a closing keyword, a backlog item's ID or `#N`, is not unbound even
            // when it cannot be followed now.
            let mut named = closing_references(&text, name);
            if named.is_empty() {
                named = self.named_issues(repository, request)?;
            }
            if !named.is_empty() {
                self.settle_named(repository, request, &named)?;
                continue;
            }
            if self.store.unbound(repository.id, request.number)?.is_none() {
                self.store.note_unbound(repository.id, request.number)?;
            }
            let Some(seen) = self.store.unbound(repository.id, request.number)? else {
                continue;
            };
            if seen.closed_at.is_some() {
                continue;
            }
            let forge = self.forge(repository)?;
            let days = flow.unbound_close_days;
            if !seen.commented {
                let rest = if days > 0 {
                    format!(
                        " A pull request that names no issue after {days} days is closed; you can reopen it once it does."
                    )
                } else {
                    String::new()
                };
                let comment = format!(
                    "Thanks @{}! This repository links every pull request to an issue, and this one names none. Please add \
                     `Fixes #<the issue's number>` to the description.{rest}\n\n\
                     (The server also binds a pull request by itself when its title or branch holds the ID of a backlog \
                     item, or `#<number>` of an issue you are assigned to.)",
                    request.author
                );
                match forge.comment_request(repository, request.number, &comment) {
                    Ok(_) => {
                        self.store
                            .set_unbound_done(repository.id, request.number, "commented")?
                    }
                    Err(e) => report("the comment asking for an issue", &e),
                }
            }
            if !seen.labelled {
                match forge.add_request_label(repository, request.number, UNBOUND_LABEL) {
                    Ok(()) => {
                        self.store
                            .set_unbound_done(repository.id, request.number, "labelled")?
                    }
                    Err(e) => report("the label unbound", &e),
                }
            }
            if days > 0 && now() - seen.first_seen >= days * DAY {
                if let Err(e) = forge.close_request(repository, request.number) {
                    report("it could not be closed", &e);
                    continue;
                }
                self.store
                    .set_unbound_done(repository.id, request.number, "closed")?;
                let text = format!(
                    "Closing this pull request: it named no issue for {days} days. Add `Fixes #<the issue's number>` to the \
                     description and reopen it, or open a new one. Thank you @{}!",
                    request.author
                );
                if let Err(e) = forge.comment_request(repository, request.number, &text) {
                    report("the comment on the closed pull request", &e);
                }
                self.store.note_activity(
                    "unbound_closed",
                    Some(repository.id),
                    Some(request.number),
                    Some(&request.author),
                    &format!("named no issue for {days} days"),
                    &serde_json::json!({ "title": request.title }),
                )?;
            }
        }
        Ok(())
    }
}
