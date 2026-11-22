//! What the schedule of a workflow remembers: the minute each workflow's schedule last fired at.
//!
//! A workflow whose trigger is a `schedule` (see `workflows::trigger::schedule`) wakes when its cron comes due, and
//! one occurrence starts one run. The minute it fired at is what makes that hold across a restart, where the store is
//! all that remembers. A workflow belongs to a project (ADR 0025), so a row is keyed by the project and the name. It is kept as the server's local minute, the same text the clock automations keep (see
//! `automations`), so the minute a fall-back day reads twice stays the minute already fired at and is not fired again.

use rusqlite::{OptionalExtension, params};

use super::{Result, Store};

/// The table of this part, one row per workflow whose schedule has fired.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE workflow_schedules (
    workflow TEXT PRIMARY KEY,
    fired_at TEXT NOT NULL
);
"#;

/// Keys the table by project as well (ADR 0025): `project_id` 0 is a workflow still kept server-wide, from before
/// workflows belonged to projects, and the migration of the files moves its row to the project with the minute kept. A
/// project's id, not its name, so that renaming a project leaves its schedules alone.
pub(super) const BY_PROJECT: &str = r#"
CREATE TABLE workflow_schedules_by_project (
    project_id INTEGER NOT NULL DEFAULT 0,
    workflow TEXT NOT NULL,
    fired_at TEXT NOT NULL,
    PRIMARY KEY (project_id, workflow)
);
INSERT INTO workflow_schedules_by_project (project_id, workflow, fired_at)
    SELECT 0, workflow, fired_at FROM workflow_schedules;
DROP TABLE workflow_schedules;
ALTER TABLE workflow_schedules_by_project RENAME TO workflow_schedules;
"#;

impl Store {
    /// The local minute the schedule of the workflow `workflow` of the project `project_id` last fired at.
    pub fn project_schedule_firing(
        &self,
        project_id: i64,
        workflow: &str,
    ) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT fired_at FROM workflow_schedules WHERE project_id = ?1 AND workflow = ?2",
                params![project_id, workflow],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    /// Records the minute the schedule of the workflow `workflow` of the project `project_id` fired at.
    pub fn record_project_schedule_firing(
        &self,
        project_id: i64,
        workflow: &str,
        fired_at: &str,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO workflow_schedules (project_id, workflow, fired_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (project_id, workflow) DO UPDATE SET fired_at = excluded.fired_at",
            params![project_id, workflow, fired_at],
        )?;
        Ok(())
    }

    /// The names of the workflows whose schedule row is still server-wide (project 0), in name order.
    pub fn server_wide_schedule_names(&self) -> Result<Vec<String>> {
        let conn = self.read();
        let mut statement = conn.prepare(
            "SELECT workflow FROM workflow_schedules WHERE project_id = 0 ORDER BY workflow",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Gives the schedule of the server-wide workflow `workflow` to the project `project_id`, with the minute it last
    /// fired at kept, so that the move fires nothing twice. A project that has a row for it already keeps that one,
    /// and the server-wide row goes either way. True when there was a server-wide row.
    pub fn move_schedule_to_project(&self, workflow: &str, project_id: i64) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO workflow_schedules (project_id, workflow, fired_at)
             SELECT ?2, workflow, fired_at FROM workflow_schedules WHERE project_id = 0 AND workflow = ?1",
            params![workflow, project_id],
        )?;
        let moved = tx.execute(
            "DELETE FROM workflow_schedules WHERE project_id = 0 AND workflow = ?1",
            params![workflow],
        )?;
        tx.commit()?;
        Ok(moved > 0)
    }

    /// The local minute the schedule of `workflow` last fired at, or `None` when it has not fired yet.
    pub fn schedule_firing(&self, workflow: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT fired_at FROM workflow_schedules WHERE project_id = 0 AND workflow = ?1",
                params![workflow],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    /// Records that the schedule of `workflow` fired at the local minute `fired_at`, replacing when it last did.
    pub fn record_schedule_firing(&self, workflow: &str, fired_at: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO workflow_schedules (project_id, workflow, fired_at) VALUES (0, ?1, ?2)
             ON CONFLICT (project_id, workflow) DO UPDATE SET fired_at = excluded.fired_at",
            params![workflow, fired_at],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_same_name_in_two_projects_keeps_two_minutes() {
        let (_dir, store) = crate::store::test_store();
        store
            .record_project_schedule_firing(1, "nightly", "2026-11-09 03:30")
            .unwrap();
        store
            .record_project_schedule_firing(2, "nightly", "2026-11-10 04:00")
            .unwrap();
        assert_eq!(
            store
                .project_schedule_firing(1, "nightly")
                .unwrap()
                .as_deref(),
            Some("2026-11-09 03:30")
        );
        assert_eq!(
            store
                .project_schedule_firing(2, "nightly")
                .unwrap()
                .as_deref(),
            Some("2026-11-10 04:00")
        );
        assert_eq!(store.schedule_firing("nightly").unwrap(), None);
    }

    #[test]
    fn moving_a_schedule_to_a_project_keeps_the_minute_it_fired_at() {
        let (_dir, store) = crate::store::test_store();
        store
            .record_schedule_firing("nightly", "2026-11-09 03:30")
            .unwrap();
        assert!(store.move_schedule_to_project("nightly", 7).unwrap());
        assert_eq!(
            store
                .project_schedule_firing(7, "nightly")
                .unwrap()
                .as_deref(),
            Some("2026-11-09 03:30")
        );
        assert_eq!(store.schedule_firing("nightly").unwrap(), None);
        // Run again after a crash, it finds nothing to move and changes nothing.
        assert!(!store.move_schedule_to_project("nightly", 7).unwrap());
        assert_eq!(
            store
                .project_schedule_firing(7, "nightly")
                .unwrap()
                .as_deref(),
            Some("2026-11-09 03:30")
        );
    }

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
