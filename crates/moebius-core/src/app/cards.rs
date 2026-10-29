//! Cards and the issues behind them: keeping cards in line with a forge's issues, a person creating, editing, approving
//! and retrying them, moving them between columns, why they wait and what blocks them, and a card's history.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::json;

use super::{App, AppError, PERSON, Result, columns};
use crate::bus::AppEvent;
use crate::decide::{Actor, Question};
use crate::repos::github::Issue;
use crate::store::{Card, Repository};

/// The issue numbers an issue's body says it is blocked by: "blocked by #12", "Blocked by: #3, #4".
fn blocked_by(body: &str) -> BTreeSet<i64> {
    static BLOCKED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)blocked by:?[ \t]*((?:#\d+[ \t,]*)+)").unwrap());
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#(\d+)").unwrap());
    BLOCKED
        .captures_iter(body)
        .flat_map(|blocked| {
            NUMBER
                .captures_iter(&blocked[1])
                .filter_map(|n| n[1].parse().ok())
                .collect::<Vec<i64>>()
        })
        .collect()
}

/// One thing in a card's history: `moved`, `run_started`, `run_ended`, `decided`, `inbox_added` or `inbox_resolved`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    pub at: i64,
    pub kind: String,
    pub data: serde_json::Value,
}

impl HistoryEntry {
    fn new(at: i64, kind: &str, data: serde_json::Value) -> Self {
        Self {
            at,
            kind: kind.into(),
            data,
        }
    }
}

impl App {
    /// Brings the cards of `repository` in line with its GitHub issues: open issues get a card in the backlog,
    /// changed titles and bodies are copied, closed issues move their card to done, and "blocked by #n" in a body
    /// becomes the card's dependencies. A closed issue without a card is not worth one.
    pub fn sync_issues(&self, repository: &Repository, issues: &[Issue]) -> Result<()> {
        for issue in issues {
            let body = issue.body.as_deref().unwrap_or_default();
            let card = match self.store.card_by_number(repository.id, issue.number)? {
                Some(card) => {
                    if card.title != issue.title || card.body != body {
                        self.store.set_card_text(card.id, &issue.title, body)?;
                        self.bus
                            .publish(AppEvent::new("card_updated", json!(self.card(card.id)?)));
                    }
                    card
                }
                None if issue.state == "open" => {
                    let card = self.store.add_numbered_card(
                        repository.id,
                        issue.number,
                        &issue.title,
                        body,
                        &issue.user.login,
                        columns::BACKLOG,
                    )?;
                    self.bus.publish(AppEvent::new("card_added", json!(card)));
                    card
                }
                None => continue,
            };
            let mut blockers = blocked_by(body);
            blockers.remove(&issue.number);
            self.store.set_dependencies(card.id, &blockers)?;
            if issue.state == "closed" && card.column != columns::DONE {
                self.move_card(card.id, columns::DONE)?;
            }
        }
        Ok(())
    }

    /// Creates an issue a person wrote in the app: a new card in the backlog. On a forge the issue is opened there
    /// first, the same way a confirmed grilling opens its issues, and the card is confirmed, since the person
    /// wrote it, so that it moves on without approval.
    pub fn create_issue(
        &self,
        repository_id: i64,
        title: &str,
        body: &str,
        author: &str,
    ) -> Result<Card> {
        let repository = self.repository(repository_id)?;
        if title.trim().is_empty() {
            return Err(AppError::Invalid("an issue needs a title".into()));
        }
        if repository.forge != "none" {
            return self.create_confirmed_issue(&repository, title, body, &BTreeSet::new());
        }
        let card = self
            .store
            .add_card(repository_id, title, body, author, columns::BACKLOG)?;
        self.bus.publish(AppEvent::new("card_added", json!(card)));
        Ok(card)
    }

    /// Creates an issue from a confirmed grilling: a card in the backlog that needs no approval, blocked by the issues
    /// with the numbers `blocked_by`. On a forge the issue is opened there first, with a "Blocked by" line for the
    /// poll to read, and the card gets its number, so that the poll finds it existing.
    pub(crate) fn create_confirmed_issue(
        &self,
        repository: &Repository,
        title: &str,
        body: &str,
        blocked_by: &BTreeSet<i64>,
    ) -> Result<Card> {
        let card = if repository.forge == "none" {
            self.store.add_confirmed_card(
                repository.id,
                None,
                title,
                body,
                PERSON,
                columns::BACKLOG,
                blocked_by,
            )?
        } else {
            let numbers: Vec<String> = blocked_by.iter().map(|n| format!("#{n}")).collect();
            let body = if numbers.is_empty() {
                body.to_string()
            } else {
                format!("{}\n\nBlocked by {}", body.trim(), numbers.join(", "))
                    .trim()
                    .to_string()
            };
            let number = self
                .forge(repository)?
                .create_issue(repository, title, &body)?;
            self.store.add_confirmed_card(
                repository.id,
                Some(number),
                title,
                &body,
                PERSON,
                columns::BACKLOG,
                blocked_by,
            )?
        };
        self.bus.publish(AppEvent::new("card_added", json!(card)));
        Ok(card)
    }

    /// A person, or the Moebius agent, changes the title and body of a card. On a forge the issue is changed there
    /// first, and "blocked by #n" in the new body becomes the card's dependencies, as the poll would read it. The edit
    /// is recorded as an `edit_card` decision and announced as `card_updated`; text that changes nothing does neither.
    pub fn edit_card(
        &self,
        by: &Actor,
        card_id: i64,
        title: &str,
        body: &str,
        reason: &str,
    ) -> Result<Card> {
        let card = self.card(card_id)?;
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::Invalid("an issue needs a title".into()));
        }
        if card.title == title && card.body == body {
            return Ok(card);
        }
        let repository = self.repository(card.repository_id)?;
        if repository.forge != "none" {
            self.forge(&repository)?
                .update_issue(&repository, card.number, title, body)?;
        }
        self.store.set_card_text(card_id, title, body)?;
        if repository.forge != "none" {
            let mut blockers = blocked_by(body);
            blockers.remove(&card.number);
            self.store.set_dependencies(card_id, &blockers)?;
        }
        let context = BTreeMap::from([
            ("title".into(), title.to_string()),
            ("body".into(), body.to_string()),
            ("previous_title".into(), card.title.clone()),
            ("previous_body".into(), card.body.clone()),
        ]);
        self.record_action(by, "edit_card", Some(card_id), None, context, reason)?;
        let card = self.card(card_id)?;
        self.bus
            .publish(AppEvent::new("card_updated", json!(card.clone())));
        Ok(card)
    }

    /// A person approves an outside issue: the decision is recorded and the card moves to ready.
    pub fn approve_issue(&self, by: &Actor, card_id: i64, reason: &str) -> Result<Card> {
        let card = self.card(card_id)?;
        if card.column != columns::BACKLOG {
            return Err(AppError::Invalid(format!(
                "card {card_id} is not in the backlog"
            )));
        }
        self.decisions.record_by(
            by,
            &Question {
                kind: "approve_issue".into(),
                context: BTreeMap::from([
                    ("title".into(), card.title.clone()),
                    ("body".into(), card.body.clone()),
                    ("author".into(), card.author.clone()),
                ]),
                options: vec!["approve".into(), "wait".into()],
                fail_closed: "wait".into(),
                run_id: None,
                card_id: Some(card_id),
            },
            "approve",
            reason,
        )?;
        self.store.resolve_inbox_for_card(card_id)?;
        self.move_card(card_id, columns::READY)
    }

    /// A person retries a card that is stuck or whose pull request was closed without a merge: the decision is
    /// recorded, the card's item is resolved and its limits start afresh, so the board starts work on it again. A card
    /// with a closed change goes back to In progress with a note saying so; its next publish opens a new change.
    pub fn retry_card(&self, by: &Actor, card_id: i64, reason: &str) -> Result<Card> {
        let card = self.card(card_id)?;
        let stuck = self.store.has_open_inbox_item(card_id, "stuck")?;
        let closed = self.store.has_open_inbox_item(card_id, "change_closed")?;
        if !stuck && !closed {
            return Err(AppError::Invalid(format!(
                "card {card_id} is neither stuck nor has a closed pull request"
            )));
        }
        let context = BTreeMap::from([("title".into(), card.title.clone())]);
        self.record_action(by, "retry", Some(card_id), None, context, reason)?;
        self.store.set_card_retried(card_id)?;
        self.store.resolve_inbox_for_card(card_id)?;
        if closed {
            self.store
                .set_card_note(card_id, "The pull request was closed without a merge.")?;
            return self.move_card(card_id, columns::IN_PROGRESS);
        }
        self.set_waiting(card_id, None);
        self.card(card_id)
    }

    /// What happened to a card, oldest first: its moves (since they were recorded), its runs starting and ending,
    /// the decisions made for it and its inbox items.
    pub fn card_history(&self, card_id: i64) -> Result<Vec<HistoryEntry>> {
        self.card(card_id)?;
        let mut history = Vec::new();
        for m in self.store.moves_for_card(card_id)? {
            let data = json!({ "from": m.from, "to": m.to, "note": m.note });
            history.push(HistoryEntry::new(m.at, "moved", data));
        }
        for run in self.store.runs_for_card(card_id)? {
            let data = json!({ "run": run.id, "agent": run.agent, "purpose": run.purpose });
            history.push(HistoryEntry::new(run.started_at, "run_started", data));
            if let Some(ended_at) = run.ended_at {
                let data = json!({ "run": run.id, "status": run.status });
                history.push(HistoryEntry::new(ended_at, "run_ended", data));
            }
        }
        for j in self.store.judgements_for_card(card_id)? {
            let data = json!({
                "judgement": j.id, "kind": j.kind, "choice": j.choice, "decider": j.decider, "reason": j.reason,
            });
            history.push(HistoryEntry::new(j.at, "decided", data));
        }
        for item in self.store.inbox_for_card(card_id)? {
            let data = json!({ "item": item.id, "kind": item.kind, "title": item.title });
            history.push(HistoryEntry::new(item.at, "inbox_added", data.clone()));
            if let Some(resolved_at) = item.resolved_at {
                history.push(HistoryEntry::new(resolved_at, "inbox_resolved", data));
            }
        }
        // Stable, so that what happened in the same second stays in the order of its kind above.
        history.sort_by_key(|entry| entry.at);
        Ok(history)
    }

    pub fn move_card(&self, card_id: i64, column: &str) -> Result<Card> {
        let from = self.card(card_id)?.column;
        self.store.move_card(card_id, column)?;
        self.bus.publish(AppEvent::new(
            "card_moved",
            json!({ "card": card_id, "from": from, "to": column }),
        ));
        self.set_waiting(card_id, None);
        self.card(card_id)
    }

    /// Why the card waits now, or none when it does not. Publishes `card_waiting` when that is not what it was.
    pub fn set_waiting(&self, card_id: i64, reason: Option<String>) {
        let mut waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
        let before = match &reason {
            Some(reason) => waiting.insert(card_id, reason.clone()),
            None => waiting.remove(&card_id),
        };
        if before != reason {
            self.bus.publish(AppEvent::new(
                "card_waiting",
                json!({ "card": card_id, "reason": reason }),
            ));
        }
    }

    /// The numbers of the issues that block the card and are not done yet; an issue with no card counts as not done.
    pub fn blocked_by(&self, card: &Card) -> Result<Vec<i64>> {
        let mut blocked_by = Vec::new();
        for number in self.store.dependencies(card.id)? {
            let blocker = self.store.card_by_number(card.repository_id, number)?;
            if blocker.is_none_or(|b| b.column != columns::DONE) {
                blocked_by.push(number);
            }
        }
        Ok(blocked_by)
    }

    /// Why each waiting card among `cards` waits.
    pub fn waiting(&self, cards: &[Card]) -> BTreeMap<i64, String> {
        let waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
        cards
            .iter()
            .filter_map(|card| Some((card.id, waiting.get(&card.id)?.clone())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blockers_are_read_from_a_body() {
        let numbers = |body| blocked_by(body).into_iter().collect::<Vec<_>>();
        assert_eq!(numbers("Needs the parser.\n\nblocked by #12"), [12]);
        assert_eq!(numbers("Blocked by: #3, #4\nBLOCKED BY #9 #3"), [3, 4, 9]);
        assert_eq!(numbers("Fixes #5, not blocked by anything"), [] as [i64; 0]);
    }
}
