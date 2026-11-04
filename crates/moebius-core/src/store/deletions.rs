//! Deleting what a person made and no longer wants: a chat with its grillings, a research with its experiments, and a
//! replay. Each is one transaction that takes out what belongs to the thing and nothing else; the files, worktrees
//! and branches belong to the caller (see `app::deletions`).
//!
//! A run that is deleted leaves its history of spend and its judgements behind: the `usage` rows and the judgements
//! made in the run stay, no longer pointing at a run that is gone, so that the totals of the Usage page, the shadowing
//! of the decision model and the training export do not change when a person tidies up.

use rusqlite::{Transaction, params};

use super::{Result, Run, Store};

/// What deleting a chat took out of the store.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChatRemoved {
    /// How many events of the chat were deleted.
    pub events: usize,
    /// The changes the chat offered, deleted with their inbox items.
    pub changes: Vec<i64>,
    /// How many inbox items were deleted: those of the changes and those of the chat's grillings.
    pub inbox_items: usize,
    /// The grillings the chat ended in, deleted with it.
    pub grillings: Vec<i64>,
    /// How many `usage` rows of the chat stay, from now on without a run.
    pub usage: usize,
}

/// What deleting a research took out of the store.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ResearchRemoved {
    /// How many experiments were deleted.
    pub experiments: usize,
    /// The runs of the research, deleted with their events.
    pub runs: Vec<i64>,
    /// The changes offered for kept levers, deleted with their inbox items.
    pub changes: Vec<i64>,
    /// How many inbox items were deleted.
    pub inbox_items: usize,
    /// How many `usage` rows of the runs stay, from now on without a run.
    pub usage: usize,
}

/// The ids `sql` selects, with `params`.
fn ids(tx: &Transaction, sql: &str, params: impl rusqlite::Params) -> rusqlite::Result<Vec<i64>> {
    tx.prepare(sql)?.query_map(params, |r| r.get(0))?.collect()
}

/// A comma-separated list of ids for an `IN (...)` clause; `NULL` for none, which matches nothing.
fn list(ids: &[i64]) -> String {
    if ids.is_empty() {
        return "NULL".into();
    }
    ids.iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Deletes the inbox items about the changes `changes`, the changes, and what points at them. Returns how many items
/// were deleted.
fn delete_changes(tx: &Transaction, changes: &[i64]) -> rusqlite::Result<usize> {
    let list = list(changes);
    let items = tx.execute(
        &format!("DELETE FROM inbox_items WHERE json_extract(data, '$.change') IN ({list})"),
        [],
    )?;
    tx.execute(
        &format!("UPDATE proposals SET change_id = NULL WHERE change_id IN ({list})"),
        [],
    )?;
    tx.execute(
        &format!("DELETE FROM experiments WHERE change_id IN ({list})"),
        [],
    )?;
    tx.execute(&format!("DELETE FROM changes WHERE id IN ({list})"), [])?;
    Ok(items)
}

/// Deletes the run `id` with its events and replays; its usage, judgements and automation firings stay without it.
/// Returns how many events were deleted and how many usage rows stay.
fn delete_run(tx: &Transaction, id: i64) -> rusqlite::Result<(usize, usize)> {
    tx.execute("DELETE FROM replays WHERE run_id = ?1", [id])?;
    tx.execute(
        "UPDATE judgements SET run_id = NULL WHERE run_id = ?1",
        [id],
    )?;
    tx.execute("UPDATE firings SET run_id = NULL WHERE run_id = ?1", [id])?;
    let usage = tx.execute("UPDATE usage SET run_id = NULL WHERE run_id = ?1", [id])?;
    let events = tx.execute("DELETE FROM events WHERE run_id = ?1", [id])?;
    tx.execute("DELETE FROM research_runs WHERE run_id = ?1", [id])?;
    tx.execute("DELETE FROM runs WHERE id = ?1", [id])?;
    Ok((events, usage))
}

impl Store {
    /// Deletes the chat `run`: its events, its read marker (a column of the run), the changes it offered with their
    /// inbox items, and the grillings it ended in with theirs. Its usage stays, as the module says. The files, the
    /// worktree and the branch belong to the caller.
    pub fn delete_chat(&self, run: &Run) -> Result<ChatRemoved> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let changes = ids(
            &tx,
            "SELECT id FROM changes WHERE branch = ?1 AND repository_id IS ?2 AND harness_repository = ?3",
            params![run.branch(), run.repository_id, run.harness_repository],
        )?;
        let grillings = ids(&tx, "SELECT id FROM grillings WHERE run_id = ?1", [run.id])?;
        let mut inbox_items = delete_changes(&tx, &changes)?;
        inbox_items += tx.execute(
            &format!(
                "DELETE FROM inbox_items WHERE kind IN ('grilling_issues', 'grilling_report')
                 AND json_extract(data, '$.grilling') IN ({})",
                list(&grillings)
            ),
            [],
        )?;
        tx.execute("DELETE FROM grillings WHERE run_id = ?1", [run.id])?;
        let (events, usage) = delete_run(&tx, run.id)?;
        tx.commit()?;
        Ok(ChatRemoved {
            events,
            changes,
            inbox_items,
            grillings,
            usage,
        })
    }

    /// Deletes the research `id`: its experiments, its runs with their events, the changes offered for its kept
    /// levers with their inbox items. Says what it took out, or none when there is no such research. The usage of its
    /// runs stays. The files, worktrees and branches belong to the caller.
    pub fn delete_research(&self, id: i64) -> Result<Option<ResearchRemoved>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let stored = ids(&tx, "SELECT id FROM research WHERE id = ?1", [id])?;
        if stored.is_empty() {
            return Ok(None);
        }
        let runs = ids(
            &tx,
            "SELECT run_id FROM research_runs WHERE research_id = ?1 ORDER BY run_id",
            [id],
        )?;
        let changes = ids(
            &tx,
            "SELECT change_id FROM experiments WHERE research_id = ?1 AND change_id IS NOT NULL",
            [id],
        )?;
        let experiments = tx.execute("DELETE FROM experiments WHERE research_id = ?1", [id])?;
        let inbox_items = delete_changes(&tx, &changes)?;
        let mut usage = 0;
        for run in &runs {
            usage += delete_run(&tx, *run)?.1;
        }
        tx.execute("DELETE FROM research WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(Some(ResearchRemoved {
            experiments,
            runs,
            changes,
            inbox_items,
            usage,
        }))
    }

    /// Deletes the replay `id`; false when there is none. The judgement it replayed and its outcome stay.
    pub fn delete_replay(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM replays WHERE id = ?1", [id])?
            == 1)
    }
}
