//! What the test lane keeps (ADR 0035): which lane a repository uses (`lane_settings`), the jobs of every lane
//! (`lane_jobs`: one change at one pinned head, queued, running and what came of it) and the lanes that stopped
//! themselves (`lane_state`).
//!
//! A lane runs one job at a time: [`Store::claim_lane_job`] moves the oldest queued job of a lane to `running` in one
//! statement that holds only while no job of that lane is running, so two threads cannot both start one.

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

use super::{Result, Store, now};

/// The tables of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE lane_settings (
    repository_id INTEGER PRIMARY KEY,
    lane TEXT
);
CREATE TABLE lane_jobs (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL,
    card_id INTEGER NOT NULL,
    change_id INTEGER NOT NULL,
    lane TEXT NOT NULL,
    head TEXT NOT NULL,
    base TEXT NOT NULL,
    external INTEGER NOT NULL,
    state TEXT NOT NULL,
    failed_step TEXT,
    summary TEXT NOT NULL DEFAULT '',
    report TEXT NOT NULL DEFAULT '{}',
    step TEXT,
    delivered INTEGER NOT NULL DEFAULT 0,
    queued_at INTEGER NOT NULL,
    started_at INTEGER,
    ended_at INTEGER
);
CREATE INDEX lane_jobs_of_a_change ON lane_jobs (change_id, id);
CREATE INDEX lane_jobs_of_a_lane ON lane_jobs (lane, state, id);
CREATE TABLE lane_state (
    lane TEXT PRIMARY KEY,
    paused INTEGER NOT NULL DEFAULT 0,
    since INTEGER NOT NULL DEFAULT 0,
    reason TEXT NOT NULL DEFAULT '',
    card_id INTEGER,
    floor_job INTEGER NOT NULL DEFAULT 0
);
"#;

/// How many jobs in a row may end with an infrastructure error before the lane pauses itself.
pub const INFRASTRUCTURE_FAILURES: i64 = 3;

/// One job of a lane. Its `state` is `queued`, `running`, `measured` (every step passed and a measurement came of it),
/// `passed` (every step passed, and no step measures), `failed` (a step failed: the change is to blame),
/// `errored` (the lane is to blame: it could not be reached, the runtime is missing, the definition is wrong) or
/// `cancelled` (a new push, a closed change or a person ended it first).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaneJob {
    pub id: i64,
    pub repository_id: i64,
    pub card_id: i64,
    pub change_id: i64,
    pub lane: String,
    /// The commit of the change the job builds and measures (`after`).
    pub head: String,
    /// The merge base of the change with the default branch (`before`).
    pub base: String,
    /// Whether the change is a pull request someone else opened.
    pub external: bool,
    pub state: String,
    /// The step a `failed` job failed at.
    pub failed_step: Option<String>,
    /// A line that says what came of it, redacted.
    pub summary: String,
    /// The report: machine, steps and measurement rows (see `app::lane::report`).
    pub report: Value,
    /// The step a running job is at.
    pub step: Option<String>,
    /// Whether what came of the job was handed on: the failure answered, the measurement given to the Reviewer.
    pub delivered: bool,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
}

const COLUMNS: &str = "id, repository_id, card_id, change_id, lane, head, base, external, state, failed_step, summary, report, \
                       step, delivered, queued_at, started_at, ended_at";

fn read(r: &rusqlite::Row) -> rusqlite::Result<LaneJob> {
    let report: String = r.get(11)?;
    Ok(LaneJob {
        id: r.get(0)?,
        repository_id: r.get(1)?,
        card_id: r.get(2)?,
        change_id: r.get(3)?,
        lane: r.get(4)?,
        head: r.get(5)?,
        base: r.get(6)?,
        external: r.get(7)?,
        state: r.get(8)?,
        failed_step: r.get(9)?,
        summary: r.get(10)?,
        report: serde_json::from_str(&report).unwrap_or(Value::Null),
        step: r.get(12)?,
        delivered: r.get(13)?,
        queued_at: r.get(14)?,
        started_at: r.get(15)?,
        ended_at: r.get(16)?,
    })
}

/// What a lane's state says: paused by itself since when, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LanePause {
    pub since: i64,
    pub reason: String,
    /// The card whose job made the lane pause, when one did: the reason may quote what that card's job printed, so it is
    /// shown only to those who see the card. It is never sent itself.
    #[serde(skip_serializing)]
    pub card_id: Option<i64>,
}

impl Store {
    /// The lane the repository uses, if one is assigned.
    pub fn repository_lane(&self, repository_id: i64) -> Result<Option<String>> {
        Ok(self
            .read()
            .query_row(
                "SELECT lane FROM lane_settings WHERE repository_id = ?1",
                params![repository_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Assigns a lane to the repository, or none.
    pub fn set_repository_lane(&self, repository_id: i64, lane: Option<&str>) -> Result<()> {
        self.conn().execute(
            "INSERT INTO lane_settings (repository_id, lane) VALUES (?1, ?2) \
             ON CONFLICT(repository_id) DO UPDATE SET lane = ?2",
            params![repository_id, lane],
        )?;
        Ok(())
    }

    /// The repositories that use the lane `lane`.
    pub fn lane_repositories(&self, lane: &str) -> Result<Vec<i64>> {
        let conn = self.read();
        let mut stmt = conn.prepare("SELECT repository_id FROM lane_settings WHERE lane = ?1")?;
        Ok(stmt
            .query_map(params![lane], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Queues a job for the change at `head`, unless one that counts is there already: a job that is queued, running or
    /// ended with a result for this head, or one that errored less than `retry_after` seconds ago. The job's id, or none.
    #[allow(clippy::too_many_arguments)]
    pub fn add_lane_job(
        &self,
        repository_id: i64,
        card_id: i64,
        change_id: i64,
        lane: &str,
        head: &str,
        base: &str,
        external: bool,
        retry_after: i64,
    ) -> Result<Option<i64>> {
        let conn = self.conn();
        let counts: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM lane_jobs WHERE change_id = ?1 AND head = ?2 AND \
             (state IN ('queued', 'running', 'measured', 'passed', 'failed') \
              OR (state = 'errored' AND ended_at > ?3)))",
            params![change_id, head, now() - retry_after],
            |r| r.get(0),
        )?;
        if counts {
            return Ok(None);
        }
        conn.execute(
            "INSERT INTO lane_jobs (repository_id, card_id, change_id, lane, head, base, external, state, queued_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'queued', ?8)",
            params![repository_id, card_id, change_id, lane, head, base, external, now()],
        )?;
        Ok(Some(conn.last_insert_rowid()))
    }

    /// The job `id`.
    pub fn lane_job(&self, id: i64) -> Result<Option<LaneJob>> {
        Ok(self
            .read()
            .query_row(
                &format!("SELECT {COLUMNS} FROM lane_jobs WHERE id = ?1"),
                params![id],
                read,
            )
            .optional()?)
    }

    fn lane_jobs_where(&self, clause: &str, args: impl rusqlite::Params) -> Result<Vec<LaneJob>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM lane_jobs WHERE {clause}"))?;
        Ok(stmt
            .query_map(args, read)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// The jobs of the change, oldest first.
    pub fn lane_jobs_of_change(&self, change_id: i64) -> Result<Vec<LaneJob>> {
        self.lane_jobs_where("change_id = ?1 ORDER BY id", params![change_id])
    }

    /// The jobs of the card (of any of its changes), oldest first.
    pub fn lane_jobs_of_card(&self, card_id: i64) -> Result<Vec<LaneJob>> {
        self.lane_jobs_where("card_id = ?1 ORDER BY id", params![card_id])
    }

    /// The latest `limit` jobs of the repository, newest first.
    pub fn lane_jobs_of_repository(&self, repository_id: i64, limit: i64) -> Result<Vec<LaneJob>> {
        self.lane_jobs_where(
            "repository_id = ?1 ORDER BY id DESC LIMIT ?2",
            params![repository_id, limit],
        )
    }

    /// The jobs of the lane that wait or run, oldest first.
    pub fn lane_queue(&self, lane: &str) -> Result<Vec<LaneJob>> {
        self.lane_jobs_where(
            "lane = ?1 AND state IN ('queued', 'running') ORDER BY id",
            params![lane],
        )
    }

    /// Every job that is queued or running, oldest first.
    pub fn lane_active(&self) -> Result<Vec<LaneJob>> {
        self.lane_jobs_where("state IN ('queued', 'running') ORDER BY id", [])
    }

    /// Moves the queued job `id` to running, unless a job of its lane is running: the lane's lock. Says whether it did.
    pub fn claim_lane_job(&self, id: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE lane_jobs SET state = 'running', started_at = ?2 WHERE id = ?1 AND state = 'queued' \
             AND NOT EXISTS (SELECT 1 FROM lane_jobs o WHERE o.lane = lane_jobs.lane AND o.state = 'running')",
            params![id, now()],
        )? > 0)
    }

    /// Writes the step a running job is at.
    pub fn set_lane_job_step(&self, id: i64, step: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE lane_jobs SET step = ?2 WHERE id = ?1 AND state = 'running'",
            params![id, step],
        )?;
        Ok(())
    }

    /// Ends a running job with `state`; a job that was cancelled meanwhile stays cancelled. Says whether it was ended.
    pub fn finish_lane_job(
        &self,
        id: i64,
        state: &str,
        failed_step: Option<&str>,
        summary: &str,
        report: &Value,
    ) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE lane_jobs SET state = ?2, failed_step = ?3, summary = ?4, report = ?5, step = NULL, ended_at = ?6, \
             delivered = CASE WHEN ?2 = 'errored' THEN 1 ELSE 0 END \
             WHERE id = ?1 AND state = 'running'",
            params![id, state, failed_step, summary, report.to_string(), now()],
        )? > 0)
    }

    /// Cancels the queued and running jobs of the change, except those for `keep_head`. The jobs cancelled.
    pub fn cancel_lane_jobs(
        &self,
        change_id: i64,
        keep_head: Option<&str>,
        why: &str,
    ) -> Result<Vec<LaneJob>> {
        let jobs: Vec<LaneJob> = self
            .lane_jobs_of_change(change_id)?
            .into_iter()
            .filter(|j| matches!(j.state.as_str(), "queued" | "running"))
            .filter(|j| keep_head != Some(j.head.as_str()))
            .collect();
        let mut cancelled = Vec::new();
        for job in jobs {
            let done = self.conn().execute(
                "UPDATE lane_jobs SET state = 'cancelled', summary = ?2, step = NULL, ended_at = ?3, delivered = 1 \
                 WHERE id = ?1 AND state IN ('queued', 'running')",
                params![job.id, why, now()],
            )?;
            if done > 0 {
                cancelled.push(job);
            }
        }
        Ok(cancelled)
    }

    /// Marks the results of the change's jobs for a head other than `head` as handed on, since they count for nothing.
    pub fn set_lane_results_invalid(&self, change_id: i64, head: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE lane_jobs SET delivered = 1 WHERE change_id = ?1 AND head != ?2 AND delivered = 0",
            params![change_id, head],
        )?;
        Ok(())
    }

    /// The jobs that ended with a result nobody was told yet, oldest first.
    pub fn undelivered_lane_jobs(&self) -> Result<Vec<LaneJob>> {
        self.lane_jobs_where(
            "delivered = 0 AND state IN ('measured', 'passed', 'failed') ORDER BY id",
            [],
        )
    }

    /// Records that what came of the job was handed on.
    pub fn set_lane_job_delivered(&self, id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE lane_jobs SET delivered = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Puts the jobs a stopped server left running back in the queue.
    pub fn requeue_interrupted_lane_jobs(&self) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE lane_jobs SET state = 'queued', started_at = NULL, step = NULL WHERE state = 'running'",
            [],
        )?)
    }

    /// How many jobs of the lane in a row ended with an infrastructure error, counting back from the newest that ended
    /// and not past the lane's last resume; a job that was cancelled is no job of the count.
    pub fn lane_infrastructure_streak(&self, lane: &str) -> Result<i64> {
        let floor: i64 = self
            .read()
            .query_row(
                "SELECT floor_job FROM lane_state WHERE lane = ?1",
                params![lane],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let conn = self.read();
        let mut stmt = conn.prepare(
            "SELECT state FROM lane_jobs WHERE lane = ?1 AND id > ?2 \
             AND state IN ('measured', 'passed', 'failed', 'errored') ORDER BY ended_at DESC, id DESC",
        )?;
        let states = stmt.query_map(params![lane, floor], |r| r.get::<_, String>(0))?;
        let mut streak = 0;
        for state in states {
            if state? != "errored" {
                break;
            }
            streak += 1;
        }
        Ok(streak)
    }

    /// The pause the lane put itself in, if it is in one.
    pub fn lane_pause(&self, lane: &str) -> Result<Option<LanePause>> {
        Ok(self
            .read()
            .query_row(
                "SELECT since, reason, card_id FROM lane_state WHERE lane = ?1 AND paused = 1",
                params![lane],
                |r| {
                    Ok(LanePause {
                        since: r.get(0)?,
                        reason: r.get(1)?,
                        card_id: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// Pauses the lane, for the job of `card_id` when one made it. False when it was paused already, which keeps its first
    /// reason.
    pub fn pause_lane(&self, lane: &str, reason: &str, card_id: Option<i64>) -> Result<bool> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR IGNORE INTO lane_state (lane) VALUES (?1)",
            params![lane],
        )?;
        Ok(conn.execute(
            "UPDATE lane_state SET paused = 1, since = ?2, reason = ?3, card_id = ?4 WHERE lane = ?1 AND paused = 0",
            params![lane, now(), reason, card_id],
        )? > 0)
    }

    /// Lifts the lane's pause, and starts its count of failures over. False when it was not paused.
    pub fn resume_lane(&self, lane: &str) -> Result<bool> {
        let conn = self.conn();
        let newest: i64 = conn.query_row(
            "SELECT COALESCE(MAX(id), 0) FROM lane_jobs WHERE lane = ?1",
            params![lane],
            |r| r.get(0),
        )?;
        Ok(conn.execute(
            "UPDATE lane_state SET paused = 0, floor_job = ?2 WHERE lane = ?1 AND paused = 1",
            params![lane, newest],
        )? > 0)
    }
}

#[cfg(test)]
mod tests {
    use crate::store::test_store as store;

    fn queue(s: &crate::store::Store, lane: &str, head: &str) -> Option<i64> {
        s.add_lane_job(1, 1, 1, lane, head, "base", true, 600)
            .unwrap()
    }

    #[test]
    fn a_lane_runs_one_job_at_a_time() {
        let (_dir, s) = store();
        let first = queue(&s, "gpu1", "h1").unwrap();
        let other_head = s
            .add_lane_job(1, 1, 2, "gpu1", "h2", "b", true, 600)
            .unwrap()
            .unwrap();
        let elsewhere = s
            .add_lane_job(1, 1, 3, "gpu2", "h3", "b", true, 600)
            .unwrap()
            .unwrap();
        assert!(s.claim_lane_job(first).unwrap());
        assert!(
            !s.claim_lane_job(other_head).unwrap(),
            "the lane is locked while a job runs"
        );
        assert!(s.claim_lane_job(elsewhere).unwrap(), "another lane is free");
        assert!(!s.claim_lane_job(first).unwrap(), "a job starts once");
        s.finish_lane_job(first, "passed", None, "ok", &serde_json::json!({}))
            .unwrap();
        assert!(s.claim_lane_job(other_head).unwrap());
    }

    #[test]
    fn a_head_has_one_job_unless_it_errored_a_while_ago() {
        let (_dir, s) = store();
        let id = queue(&s, "gpu1", "h1").unwrap();
        assert_eq!(queue(&s, "gpu1", "h1"), None, "queued already");
        assert!(queue(&s, "gpu1", "h2").is_some(), "another head");
        s.claim_lane_job(id).unwrap();
        s.finish_lane_job(id, "errored", None, "unreachable", &serde_json::json!({}))
            .unwrap();
        assert_eq!(
            queue(&s, "gpu1", "h1"),
            None,
            "an error just now is waited out"
        );
        assert!(
            s.add_lane_job(1, 1, 1, "gpu1", "h1", "base", true, 0)
                .unwrap()
                .is_some(),
            "and tried again later"
        );
    }

    #[test]
    fn a_job_that_was_cancelled_stays_cancelled() {
        let (_dir, s) = store();
        let id = queue(&s, "gpu1", "h1").unwrap();
        s.claim_lane_job(id).unwrap();
        let cancelled = s.cancel_lane_jobs(1, Some("h2"), "a new push").unwrap();
        assert_eq!(cancelled.len(), 1);
        assert!(
            !s.finish_lane_job(id, "measured", None, "late", &serde_json::json!({}))
                .unwrap()
        );
        assert_eq!(s.lane_job(id).unwrap().unwrap().state, "cancelled");
        assert!(s.cancel_lane_jobs(1, None, "again").unwrap().is_empty());
    }

    #[test]
    fn three_errors_in_a_row_are_counted_until_the_lane_is_resumed() {
        let (_dir, s) = store();
        let end = |head: &str, state: &str| {
            let id = s
                .add_lane_job(1, 1, 1, "gpu1", head, "b", true, 0)
                .unwrap()
                .unwrap();
            s.claim_lane_job(id).unwrap();
            s.finish_lane_job(id, state, None, "", &serde_json::json!({}))
                .unwrap();
        };
        end("a", "errored");
        end("b", "errored");
        assert_eq!(s.lane_infrastructure_streak("gpu1").unwrap(), 2);
        end("c", "failed");
        assert_eq!(s.lane_infrastructure_streak("gpu1").unwrap(), 0);
        end("d", "errored");
        end("e", "errored");
        end("f", "errored");
        assert_eq!(s.lane_infrastructure_streak("gpu1").unwrap(), 3);
        assert!(s.pause_lane("gpu1", "three in a row", None).unwrap());
        assert!(!s.pause_lane("gpu1", "again", None).unwrap());
        assert_eq!(
            s.lane_pause("gpu1").unwrap().unwrap().reason,
            "three in a row"
        );
        assert!(s.resume_lane("gpu1").unwrap());
        assert_eq!(s.lane_infrastructure_streak("gpu1").unwrap(), 0);
        assert!(s.lane_pause("gpu1").unwrap().is_none());
    }

    #[test]
    fn the_repository_keeps_the_lane_it_was_given() {
        let (_dir, s) = store();
        assert_eq!(s.repository_lane(1).unwrap(), None);
        s.set_repository_lane(1, Some("gpu1")).unwrap();
        assert_eq!(s.repository_lane(1).unwrap().as_deref(), Some("gpu1"));
        assert_eq!(s.lane_repositories("gpu1").unwrap(), vec![1]);
        s.set_repository_lane(1, None).unwrap();
        assert_eq!(s.repository_lane(1).unwrap(), None);
    }
}
