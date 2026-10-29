//! Replays: refused tool calls run again in a throwaway copy of the worktree (see `replay`), and the refusals still to
//! replay.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Judgement, Result, Store, now};

/// A refused tool call run again in a throwaway copy of the worktree (see `replay`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replay {
    pub id: i64,
    /// The judgement that refused the call.
    pub judgement: i64,
    pub run: i64,
    /// The exit code; none when the call did not run, or was ended by a signal.
    pub exit: Option<i32>,
    pub timed_out: bool,
    /// What the call printed, or why it did not run; secrets removed and cut to a few KB.
    pub output: String,
    /// The files, relative to the worktree, that the call created, changed and deleted in the copy.
    pub created: Vec<String>,
    pub changed: Vec<String>,
    pub deleted: Vec<String>,
    pub at: i64,
}

impl Store {
    /// The refused tool calls no outcome and no replay is known for, made since `since`, oldest first.
    pub fn replay_candidates(&self, since: i64) -> Result<Vec<Judgement>> {
        self.judgements_query(
            "WHERE kind = 'tool_call' AND choice = 'refuse' AND run_id IS NOT NULL
               AND at >= ?1 AND NOT EXISTS (SELECT 1 FROM replays WHERE judgement_id = judgements.id)
             ORDER BY id",
            [since],
        )
    }

    /// Stores `replay` (its `id` and `at` are set here) and returns it as stored.
    pub fn add_replay(&self, replay: Replay) -> Result<Replay> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO replays (judgement_id, run_id, exit, timed_out, output, created, changed, deleted, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                replay.judgement,
                replay.run,
                replay.exit,
                replay.timed_out,
                replay.output,
                serde_json::to_string(&replay.created)?,
                serde_json::to_string(&replay.changed)?,
                serde_json::to_string(&replay.deleted)?,
                at
            ],
        )?;
        Ok(Replay {
            id: conn.last_insert_rowid(),
            at,
            ..replay
        })
    }

    /// The replays of the judgement and of the run, if given, newest first; at most `limit`.
    pub fn replays(
        &self,
        judgement: Option<i64>,
        run: Option<i64>,
        limit: i64,
    ) -> Result<Vec<Replay>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, judgement_id, run_id, exit, timed_out, output, created, changed, deleted, at
             FROM replays WHERE (?1 IS NULL OR judgement_id = ?1) AND (?2 IS NULL OR run_id = ?2)
             ORDER BY id DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![judgement, run, limit], |r| {
            Ok((
                Replay {
                    id: r.get(0)?,
                    judgement: r.get(1)?,
                    run: r.get(2)?,
                    exit: r.get(3)?,
                    timed_out: r.get(4)?,
                    output: r.get(5)?,
                    created: Vec::new(),
                    changed: Vec::new(),
                    deleted: Vec::new(),
                    at: r.get(9)?,
                },
                [r.get::<_, String>(6)?, r.get(7)?, r.get(8)?],
            ))
        })?;
        rows.map(|row| {
            let (replay, [created, changed, deleted]) = row?;
            Ok(Replay {
                created: serde_json::from_str(&created)?,
                changed: serde_json::from_str(&changed)?,
                deleted: serde_json::from_str(&deleted)?,
                ..replay
            })
        })
        .collect()
    }

    /// How many replays were made since `since`.
    pub fn replays_since(&self, since: i64) -> Result<usize> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM replays WHERE at >= ?1",
            [since],
            |r| r.get::<_, i64>(0),
        )? as usize)
    }
}
