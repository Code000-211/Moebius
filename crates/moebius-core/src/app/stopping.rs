//! What a person, or the Moebius agent, does to keep work on a card from going on: holding (blocking) it, releasing it,
//! and deleting it. A held card stays in its column but the board runs no script for it and starts no run; a deleted
//! card is gone with everything stored for it, and on a forge its number is remembered so that the poll never makes a
//! card for its issue again.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{App, AppError, Result};
use crate::board::CardRuns;
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::Card;

/// How long blocking and deleting a card wait for its runs.
#[derive(Debug, Clone, Copy)]
pub struct CardStopWaits {
    /// How long blocking a card keeps trying to stop a run that is not live yet (its harness has not registered it),
    /// before the block is given up and the card released again.
    pub starting: Duration,
    /// How long deleting a card waits for the runs it stopped to end: a little more than a harness gives an agent to
    /// settle before it is killed.
    pub ending: Duration,
}

impl Default for CardStopWaits {
    fn default() -> Self {
        Self {
            starting: Duration::from_secs(10),
            ending: Duration::from_secs(30),
        }
    }
}

/// How often a run that could not be stopped yet is tried again.
const POLL: Duration = Duration::from_millis(20);

impl App {
    /// Holds a card: its running runs are stopped, and the board runs no script for it and starts no run until it is
    /// released. It stays in its column. The decision is recorded (kind `block_card`, with the reason in its context)
    /// and `card_updated` is sent. A card that is held already is a conflict.
    pub fn block_card(
        &self,
        by: &Actor,
        runs: &dyn CardRuns,
        card_id: i64,
        reason: Option<&str>,
    ) -> Result<Card> {
        let card = self.card(card_id)?;
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        // Held first, so that the board starts nothing while the runs are stopped.
        if !self.store.hold_card(card_id, &by.decider(), reason)? {
            return Err(AppError::Conflict(format!(
                "card {card_id} is held already"
            )));
        }
        if let Err(e) = self.stop_starting_runs(by, runs, card_id) {
            // Not blocked after all: given back, as a delete does.
            let _ = self.store.release_card(card_id);
            return Err(e);
        }
        let context = BTreeMap::from([
            ("title".into(), card.title.clone()),
            ("reason".into(), reason.unwrap_or_default().to_string()),
        ]);
        self.record_action(
            by,
            "block_card",
            Some(card_id),
            None,
            context,
            reason.unwrap_or_default(),
        )?;
        // What the card waited for is not what it waits for now.
        self.set_waiting(card_id, None);
        let card = self.card(card_id)?;
        self.bus.publish(Event::CardUpdated(card.clone()));
        Ok(card)
    }

    /// Releases a held card: the next tick treats it like any other card. The decision is recorded (kind
    /// `unblock_card`) and `card_updated` is sent. A card that is not held is a conflict.
    pub fn unblock_card(&self, by: &Actor, card_id: i64, reason: &str) -> Result<Card> {
        let card = self.card(card_id)?;
        let Some(held) = card.held.clone() else {
            return Err(AppError::Conflict(format!("card {card_id} is not held")));
        };
        if !self.store.release_card(card_id)? {
            return Err(AppError::Conflict(format!("card {card_id} is not held")));
        }
        // A run the block stopped is not a person's wish to stop the card: it starts afresh, as after a retry, instead
        // of waiting in the inbox as stuck. A run stopped before the block still waits for a retry.
        let blocked_run = self
            .store
            .runs_for_card(card_id)?
            .into_iter()
            .next_back()
            .is_some_and(|run| {
                run.status == "stopped" && run.ended_at.is_some_and(|at| at >= held.at)
            });
        if blocked_run {
            self.store.set_card_retried(card_id)?;
        }
        let context = BTreeMap::from([
            ("title".into(), card.title.clone()),
            ("reason".into(), reason.to_string()),
            (
                "held_reason".into(),
                held.reason.unwrap_or_default().to_string(),
            ),
            ("held_by".into(), held.by),
        ]);
        self.record_action(by, "unblock_card", Some(card_id), None, context, reason)?;
        let card = self.card(card_id)?;
        self.bus.publish(Event::CardUpdated(card.clone()));
        Ok(card)
    }

    /// Deletes a card with everything stored for it (see `Store::delete_card`) and its worktrees and `moebius/`
    /// branches in the clone. Its running runs are stopped first, and the deletion waits for them to end. The issue on
    /// a forge is not touched; its number is remembered, so the poll does not make a card for it again. The decision is
    /// recorded (kind `delete_card`, with the title and number; it has no card to belong to) and `card_deleted` is
    /// sent. Returns the card as it was.
    pub fn delete_card(
        &self,
        by: &Actor,
        runs: &dyn CardRuns,
        card_id: i64,
        reason: &str,
    ) -> Result<Card> {
        let card = self.card(card_id)?;
        let repository = self.repository(card.repository_id)?;
        // Held, so that the board starts nothing meanwhile; given back if the card cannot be deleted.
        let held_here = self.store.hold_card(card_id, &by.decider(), None)?;
        let removed = self
            .end_runs_of(by, runs, card_id)
            .and_then(|()| self.remove_card_work(&repository.path, &card))
            .and_then(|()| {
                Ok(self
                    .store
                    .delete_card(card_id, repository.forge != "none")?)
            });
        match removed {
            Ok(true) => {}
            Ok(false) => return Err(AppError::NotFound(format!("card {card_id}"))),
            Err(e) => {
                if held_here {
                    let _ = self.store.release_card(card_id);
                }
                return Err(e);
            }
        }
        let context = BTreeMap::from([
            ("title".into(), card.title.clone()),
            ("number".into(), card.number.to_string()),
            ("repository".into(), repository.name.clone()),
        ]);
        self.record_action(by, "delete_card", None, None, context, reason)?;
        self.waiting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&card_id);
        self.close_settled(card_id);
        self.bus.publish(Event::CardDeleted { card: card_id });
        Ok(card)
    }

    /// Stops every run of the card that has not ended. A run the board is starting cannot be stopped until its harness
    /// has registered it, so what could not be stopped is tried again for a while; a run that has been stopped is not
    /// stopped twice, and it need not have ended. A conflict when a run still cannot be stopped in time.
    fn stop_starting_runs(&self, by: &Actor, runs: &dyn CardRuns, card_id: i64) -> Result<()> {
        let deadline = Instant::now() + self.card_stop_waits.starting;
        let mut stopped = BTreeSet::new();
        loop {
            let active = self.stop_pass(by, runs, card_id, "the card was blocked", &mut stopped)?;
            let Some(id) = active.iter().find(|id| !stopped.contains(*id)) else {
                return Ok(());
            };
            if Instant::now() >= deadline {
                return Err(AppError::Conflict(format!(
                    "run {id} cannot be stopped yet; try blocking the card again in a moment"
                )));
            }
            std::thread::sleep(POLL);
        }
    }

    /// Looks at the runs of the card that have not ended and stops those not in `stopped` yet, which are then put in
    /// it. A run that cannot be stopped (it is not live yet, or a chat is waking) is left for the next look. Returns
    /// the runs that have not ended.
    fn stop_pass(
        &self,
        by: &Actor,
        runs: &dyn CardRuns,
        card_id: i64,
        reason: &str,
        stopped: &mut BTreeSet<i64>,
    ) -> Result<Vec<i64>> {
        let mut active = Vec::new();
        for run in self.store.runs_for_card(card_id)? {
            if run.ended_at.is_some() {
                continue;
            }
            active.push(run.id);
            if !stopped.contains(&run.id) {
                match self.stop_run(by, runs, run.id, reason) {
                    Ok(_) => {
                        stopped.insert(run.id);
                    }
                    Err(AppError::Invalid(_) | AppError::Conflict(_)) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(active)
    }

    /// Stops the runs of the card that have not ended and waits until they have, since what an agent leaves is
    /// committed and its worktree removed only as its run ends.
    fn end_runs_of(&self, by: &Actor, runs: &dyn CardRuns, card_id: i64) -> Result<()> {
        let deadline = Instant::now() + self.card_stop_waits.ending;
        let mut stopped = BTreeSet::new();
        loop {
            let active = self.stop_pass(by, runs, card_id, "the card was deleted", &mut stopped)?;
            let Some(run) = active.first() else {
                return Ok(());
            };
            if Instant::now() >= deadline {
                return Err(AppError::Conflict(format!(
                    "run {run} has not stopped yet; try deleting the card again in a moment"
                )));
            }
            std::thread::sleep(POLL);
        }
    }

    /// Removes the worktrees and `moebius/` branches the card's runs and changes left in the clone at `path`.
    fn remove_card_work(&self, path: &str, card: &Card) -> Result<()> {
        let mut worktrees: Vec<PathBuf> = Vec::new();
        let mut branches: BTreeSet<String> = BTreeSet::new();
        for run in self.store.runs_for_card(card.id)? {
            worktrees.extend(run.worktree.as_ref().map(PathBuf::from));
            branches.insert(run.branch());
        }
        for change in self.store.changes(card.repository_id)? {
            if change.card_id == Some(card.id) && change.branch.starts_with("moebius/") {
                branches.insert(change.branch);
            }
        }
        // The branch of the card's change when it was published, before a change was stored for it.
        branches.insert(format!("moebius/card-{}", card.number));
        self.repos
            .remove_card_work(std::path::Path::new(path), card.id, &worktrees, &branches)?;
        Ok(())
    }
}
