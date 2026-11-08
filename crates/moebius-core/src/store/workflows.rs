//! Workflow runs and what each node of a run did. A run belongs to a workflow name and a repository, carries the value
//! it was started with and moves through its statuses; each node of the run is a row of its own, with the attempt, the
//! input it was given and the output it produced. The JSON values are stored as text, redacted first.

use std::path::Path;

use rusqlite::{OptionalExtension, Row, params};
use serde_json::Value;

use super::{Result, Store, now};
use crate::blobs::redact_json;

/// A status after which a run or a node is done and gets its finish time.
fn is_finished(status: &str) -> bool {
    matches!(status, "succeeded" | "failed" | "cancelled")
}

/// One run of a workflow: what started it and what became of it.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowRun {
    pub id: i64,
    pub workflow: String,
    pub repository_id: i64,
    pub trigger: String,
    pub status: String,
    pub value: Value,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    /// The worktree in which the run's agents work on the branch named for the run; none before its first agent node.
    pub worktree: Option<String>,
}

/// One node of a workflow run at one attempt: the input it was given and the output it left.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowNodeRun {
    pub id: i64,
    pub run_id: i64,
    pub node_id: String,
    pub attempt: i64,
    pub status: String,
    pub input: Value,
    pub output: Option<Value>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
}

pub(super) const SCHEMA: &str = r#"
CREATE TABLE workflow_runs (
    id INTEGER PRIMARY KEY,
    workflow TEXT NOT NULL,
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    trigger TEXT NOT NULL,
    status TEXT NOT NULL,
    value TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER,
    error TEXT
);
CREATE INDEX workflow_runs_by_repository ON workflow_runs (repository_id, created_at);
CREATE INDEX workflow_runs_by_workflow ON workflow_runs (workflow, created_at);
CREATE TABLE workflow_node_runs (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
    node_id TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    status TEXT NOT NULL,
    input TEXT NOT NULL,
    output TEXT,
    started_at INTEGER,
    finished_at INTEGER,
    error TEXT,
    UNIQUE (run_id, node_id, attempt)
);
CREATE INDEX workflow_node_runs_by_run ON workflow_node_runs (run_id, id);
"#;

impl Store {
    /// Stores a queued run of `workflow` for `repository_id`, started by `trigger` with `value`.
    pub fn add_workflow_run(
        &self,
        workflow: &str,
        repository_id: i64,
        trigger: &str,
        value: &Value,
    ) -> Result<WorkflowRun> {
        let value = json_text(value);
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO workflow_runs (workflow, repository_id, trigger, status, value, created_at)
                 VALUES (?1, ?2, ?3, 'queued', ?4, ?5)",
                params![workflow, repository_id, trigger, value, now()],
            )?;
            conn.last_insert_rowid()
        };
        Ok(self.workflow_run(id)?.expect("the run was just stored"))
    }

    /// Records the worktree of the run `id`, in which the run's agents work on the branch named for the run, so its
    /// later agents reuse it.
    pub fn set_workflow_run_worktree(&self, id: i64, worktree: &Path) -> Result<()> {
        self.conn().execute(
            "UPDATE workflow_runs SET worktree = ?1 WHERE id = ?2",
            params![worktree.to_string_lossy().to_string(), id],
        )?;
        Ok(())
    }

    /// The run with `id`, if it exists.
    pub fn workflow_run(&self, id: i64) -> Result<Option<WorkflowRun>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT * FROM workflow_runs WHERE id = ?1",
                [id],
                run_from_row,
            )
            .optional()?)
    }

    /// How many runs of `workflow` are queued or running, that is, not yet finished.
    pub fn active_workflow_runs(&self, workflow: &str) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM workflow_runs
             WHERE workflow = ?1 AND status NOT IN ('succeeded', 'failed', 'cancelled')",
            [workflow],
            |row| row.get(0),
        )?)
    }

    /// Moves a run to `status`. Its start is set the first time it runs, its finish the first time it ends.
    pub fn set_workflow_run_status(&self, id: i64, status: &str) -> Result<()> {
        let at = now();
        self.conn().execute(
            "UPDATE workflow_runs SET status = ?1,
                 started_at = CASE WHEN ?2 = 'running' AND started_at IS NULL THEN ?3 ELSE started_at END,
                 finished_at = CASE WHEN ?4 AND finished_at IS NULL THEN ?3 ELSE finished_at END
             WHERE id = ?5",
            params![status, status, at, is_finished(status), id],
        )?;
        Ok(())
    }

    /// Ends a run with `status` and `error`, redacted before it is stored.
    pub fn finish_workflow_run(&self, id: i64, status: &str, error: Option<&str>) -> Result<()> {
        let at = now();
        self.conn().execute(
            "UPDATE workflow_runs SET status = ?1,
                 started_at = COALESCE(started_at, ?2),
                 finished_at = COALESCE(finished_at, ?2),
                 error = ?3
             WHERE id = ?4",
            params![status, at, error.map(crate::blobs::redact), id],
        )?;
        Ok(())
    }

    /// Stores one attempt of a node of `run_id` as it starts, with its input.
    pub fn add_workflow_node_run(
        &self,
        run_id: i64,
        node_id: &str,
        attempt: i64,
        input: &Value,
    ) -> Result<WorkflowNodeRun> {
        let input = json_text(input);
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO workflow_node_runs (run_id, node_id, attempt, status, input, started_at)
                 VALUES (?1, ?2, ?3, 'running', ?4, ?5)",
                params![run_id, node_id, attempt, input, now()],
            )?;
            conn.last_insert_rowid()
        };
        Ok(self
            .workflow_node_run(id)?
            .expect("the node run was just stored"))
    }

    /// The node run with `id`, if it exists.
    pub fn workflow_node_run(&self, id: i64) -> Result<Option<WorkflowNodeRun>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT * FROM workflow_node_runs WHERE id = ?1",
                [id],
                node_from_row,
            )
            .optional()?)
    }

    /// The nodes of a run, in the order they were stored.
    pub fn workflow_node_runs(&self, run_id: i64) -> Result<Vec<WorkflowNodeRun>> {
        let conn = self.conn();
        let mut statement =
            conn.prepare("SELECT * FROM workflow_node_runs WHERE run_id = ?1 ORDER BY id")?;
        let rows = statement.query_map([run_id], node_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Moves a node run to `status`, setting its finish the first time it ends.
    pub fn set_workflow_node_run_status(&self, id: i64, status: &str) -> Result<()> {
        let at = now();
        self.conn().execute(
            "UPDATE workflow_node_runs SET status = ?1,
                 started_at = CASE WHEN ?2 = 'running' AND started_at IS NULL THEN ?3 ELSE started_at END,
                 finished_at = CASE WHEN ?4 AND finished_at IS NULL THEN ?3 ELSE finished_at END
             WHERE id = ?5",
            params![status, status, at, is_finished(status), id],
        )?;
        Ok(())
    }

    /// Ends a node run with `status`, its `output` and `error`; both are redacted before they are stored.
    pub fn finish_workflow_node_run(
        &self,
        id: i64,
        status: &str,
        output: Option<&Value>,
        error: Option<&str>,
    ) -> Result<()> {
        let at = now();
        let output = output.map(json_text);
        self.conn().execute(
            "UPDATE workflow_node_runs SET status = ?1,
                 started_at = COALESCE(started_at, ?2),
                 finished_at = COALESCE(finished_at, ?2),
                 output = ?3,
                 error = ?4
             WHERE id = ?5",
            params![status, at, output, error.map(crate::blobs::redact), id],
        )?;
        Ok(())
    }
}

/// A JSON value as the text a row holds, with every known secret already redacted.
fn json_text(value: &Value) -> String {
    redact_json(value).to_string()
}

fn json_column(row: &Row<'_>, index: usize) -> rusqlite::Result<Value> {
    let text: String = row.get(index)?;
    serde_json::from_str(&text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn run_from_row(row: &Row<'_>) -> rusqlite::Result<WorkflowRun> {
    Ok(WorkflowRun {
        id: row.get(0)?,
        workflow: row.get(1)?,
        repository_id: row.get(2)?,
        trigger: row.get(3)?,
        status: row.get(4)?,
        value: json_column(row, 5)?,
        created_at: row.get(6)?,
        started_at: row.get(7)?,
        finished_at: row.get(8)?,
        error: row.get(9)?,
        worktree: row.get(10)?,
    })
}

fn node_from_row(row: &Row<'_>) -> rusqlite::Result<WorkflowNodeRun> {
    Ok(WorkflowNodeRun {
        id: row.get(0)?,
        run_id: row.get(1)?,
        node_id: row.get(2)?,
        attempt: row.get(3)?,
        status: row.get(4)?,
        input: json_column(row, 5)?,
        output: match row.get::<_, Option<String>>(6)? {
            Some(text) => Some(serde_json::from_str(&text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?),
            None => None,
        },
        started_at: row.get(7)?,
        finished_at: row.get(8)?,
        error: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{Bus, Event};
    use crate::store::test_store;
    use serde_json::json;

    fn store_with_repository() -> (tempfile::TempDir, Store, i64) {
        let (dir, store) = test_store();
        let repository = store
            .add_repository("moebius", "/tmp/moebius", "none", "main")
            .unwrap();
        (dir, store, repository.id)
    }

    #[test]
    fn a_run_and_its_nodes_round_trip() {
        let (_dir, store, repository) = store_with_repository();
        let run = store
            .add_workflow_run(
                "release",
                repository,
                "manual",
                &json!({"version": "1.2.3"}),
            )
            .unwrap();
        assert_eq!(run.status, "queued");
        assert_eq!(run.value, json!({"version": "1.2.3"}));

        let build = store
            .add_workflow_node_run(run.id, "build", 1, &json!({"target": "linux"}))
            .unwrap();
        let test = store
            .add_workflow_node_run(run.id, "test", 1, &json!({"suite": "all"}))
            .unwrap();
        store
            .finish_workflow_node_run(
                build.id,
                "succeeded",
                Some(&json!({"artifact": "app"})),
                None,
            )
            .unwrap();
        store
            .finish_workflow_node_run(test.id, "succeeded", Some(&json!({"passed": 12})), None)
            .unwrap();

        let stored = store.workflow_run(run.id).unwrap().unwrap();
        assert_eq!(stored.workflow, "release");
        assert_eq!(stored.repository_id, repository);
        assert_eq!(stored.trigger, "manual");
        assert_eq!(stored.value, json!({"version": "1.2.3"}));

        let nodes = store.workflow_node_runs(run.id).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].node_id, "build");
        assert_eq!(nodes[0].attempt, 1);
        assert_eq!(nodes[0].input, json!({"target": "linux"}));
        assert_eq!(nodes[0].output, Some(json!({"artifact": "app"})));
        assert_eq!(nodes[1].node_id, "test");
        assert_eq!(nodes[1].output, Some(json!({"passed": 12})));
    }

    #[test]
    fn a_run_moves_through_its_statuses_and_sets_its_times() {
        let (_dir, store, repository) = store_with_repository();
        let run = store
            .add_workflow_run("release", repository, "schedule", &json!({}))
            .unwrap();
        assert!(run.started_at.is_none());
        assert!(run.finished_at.is_none());

        store.set_workflow_run_status(run.id, "running").unwrap();
        let running = store.workflow_run(run.id).unwrap().unwrap();
        assert_eq!(running.status, "running");
        assert!(running.started_at.is_some());
        assert!(running.finished_at.is_none());

        store.set_workflow_run_status(run.id, "waiting").unwrap();
        let waiting = store.workflow_run(run.id).unwrap().unwrap();
        assert_eq!(waiting.status, "waiting");
        assert_eq!(waiting.started_at, running.started_at);
        assert!(waiting.finished_at.is_none());

        store.set_workflow_run_status(run.id, "succeeded").unwrap();
        let done = store.workflow_run(run.id).unwrap().unwrap();
        assert_eq!(done.status, "succeeded");
        assert_eq!(done.started_at, running.started_at);
        assert!(done.finished_at.is_some());
    }

    #[test]
    fn a_node_moves_through_its_statuses_and_sets_its_times() {
        let (_dir, store, repository) = store_with_repository();
        let run = store
            .add_workflow_run("release", repository, "manual", &json!({}))
            .unwrap();
        let node = store
            .add_workflow_node_run(run.id, "build", 2, &json!({}))
            .unwrap();
        assert_eq!(node.status, "running");
        assert!(node.started_at.is_some());

        store
            .set_workflow_node_run_status(node.id, "waiting")
            .unwrap();
        let waiting = store.workflow_node_run(node.id).unwrap().unwrap();
        assert_eq!(waiting.status, "waiting");
        assert!(waiting.finished_at.is_none());

        store
            .finish_workflow_node_run(node.id, "cancelled", None, Some("stopped by a person"))
            .unwrap();
        let done = store.workflow_node_run(node.id).unwrap().unwrap();
        assert_eq!(done.status, "cancelled");
        assert_eq!(done.error.as_deref(), Some("stopped by a person"));
        assert!(done.finished_at.is_some());
    }

    #[test]
    fn a_secret_in_a_value_input_or_output_is_redacted_before_it_is_stored() {
        let (_dir, store, repository) = store_with_repository();
        let secret = "workflow-secret-216-a1b2c3";
        crate::blobs::remember_secret(secret);

        let run = store
            .add_workflow_run("release", repository, "manual", &json!({"token": secret}))
            .unwrap();
        let node = store
            .add_workflow_node_run(run.id, "build", 1, &json!({"token": secret}))
            .unwrap();
        store
            .finish_workflow_node_run(
                node.id,
                "failed",
                Some(&json!({"token": secret})),
                Some(secret),
            )
            .unwrap();

        let run = store.workflow_run(run.id).unwrap().unwrap();
        assert!(!run.value.to_string().contains(secret));
        let node = store.workflow_node_run(node.id).unwrap().unwrap();
        assert!(!node.input.to_string().contains(secret));
        assert!(!node.output.unwrap().to_string().contains(secret));
        assert!(!node.error.unwrap().contains(secret));
    }

    #[test]
    fn each_workflow_event_is_published_once() {
        let bus = Bus::new();
        let mut receiver = bus.subscribe();
        let events = [
            Event::WorkflowRunQueued {
                run: 7,
                workflow: "release".into(),
                repository: 1,
            },
            Event::WorkflowRunStarted { run: 7 },
            Event::WorkflowRunWaiting { run: 7 },
            Event::WorkflowRunEnded {
                run: 7,
                status: "succeeded".into(),
                error: None,
            },
            Event::WorkflowRunStatus {
                run: 7,
                status: "running".into(),
            },
            Event::WorkflowNodeStarted {
                run: 7,
                node: "build".into(),
                attempt: 1,
            },
            Event::WorkflowNodeEnded {
                run: 7,
                node: "build".into(),
                attempt: 1,
                status: "succeeded".into(),
                error: None,
            },
            Event::WorkflowNodeStatus {
                run: 7,
                node: "build".into(),
                attempt: 1,
                status: "running".into(),
            },
        ];
        for event in &events {
            bus.publish(event.clone());
        }
        let mut published = Vec::new();
        for _ in 0..events.len() {
            published.push(receiver.try_recv().unwrap().kind);
        }
        assert_eq!(
            published,
            [
                "workflow_run_queued",
                "workflow_run_started",
                "workflow_run_waiting",
                "workflow_run_ended",
                "workflow_run_status",
                "workflow_node_started",
                "workflow_node_ended",
                "workflow_node_status",
            ]
        );
    }
}
