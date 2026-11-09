//! What the schedule of a workflow remembers: the minute each workflow's schedule last fired at.
//!
//! A workflow whose trigger is a `schedule` (see `workflows::trigger::schedule`) wakes when its cron comes due, and
//! one occurrence starts one run. The minute it fired at is what makes that hold across a restart, where the store is
//! all that remembers. It is kept as the server's local minute, the same text the clock automations keep (see
//! `automations`), so the minute a fall-back day reads twice stays the minute already fired at and is not fired again.

use rusqlite::{OptionalExtension, params};

use super::{Result, Store};

/// The table of this part, one row per workflow whose schedule has fired.
pub const SCHEMA: &str = r#"
CREATE TABLE workflow_schedules (
    workflow TEXT PRIMARY KEY,
    fired_at TEXT NOT NULL
);
"#;

impl Store {
    /// The local minute the schedule of `workflow` last fired at, or `None` when it has not fired yet.
    pub fn schedule_firing(&self, workflow: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT fired_at FROM workflow_schedules WHERE workflow = ?1",
                params![workflow],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    /// Records that the schedule of `workflow` fired at the local minute `fired_at`, replacing when it last did.
    pub fn record_schedule_firing(&self, workflow: &str, fired_at: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO workflow_schedules (workflow, fired_at) VALUES (?1, ?2)
             ON CONFLICT (workflow) DO UPDATE SET fired_at = excluded.fired_at",
            params![workflow, fired_at],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_workflow_that_never_fired_has_no_firing() {
        let (_dir, store) = crate::store::test_store();
        assert_eq!(store.schedule_firing("nightly").unwrap(), None);
    }

    #[test]
    fn the_minute_a_schedule_fired_at_is_kept_and_replaced() {
        let (_dir, store) = crate::store::test_store();
        store
            .record_schedule_firing("nightly", "2026-11-09 03:30")
            .unwrap();
        assert_eq!(
            store.schedule_firing("nightly").unwrap().as_deref(),
            Some("2026-11-09 03:30")
        );
        store
            .record_schedule_firing("nightly", "2026-11-10 03:30")
            .unwrap();
        assert_eq!(
            store.schedule_firing("nightly").unwrap().as_deref(),
            Some("2026-11-10 03:30")
        );
        // Another workflow keeps its own minute.
        store
            .record_schedule_firing("weekly", "2026-11-01 04:00")
            .unwrap();
        assert_eq!(
            store.schedule_firing("nightly").unwrap().as_deref(),
            Some("2026-11-10 03:30")
        );
    }
}
