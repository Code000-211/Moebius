//! Stopping a workflow run: a person's cancel of one. The cancel sets the run's flag, so that a run going stops at
//! its current node, and ends the run `cancelled`, with its log saying why.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::app::{App, AppError};
use crate::board::CardRuns;
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::{StoreError, WorkflowRun};

/// What could stand in the way of cancelling a workflow run.
#[derive(Debug, thiserror::Error)]
pub enum CancelWorkflowRunError {
    /// The run does not exist.
    #[error("the workflow run does not exist")]
    NotFound,
    /// The run has ended already, so there is nothing to stop.
    #[error("the run has already ended as {status}")]
    AlreadyFinished { status: String },
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    App(#[from] crate::app::AppError),
}

impl App {
    /// The cancellation flag of the run, registered on first use: the thread of the run looks at it between nodes and
    /// while its node goes (see `workflows::run::execute`), and a cancel sets it.
    pub(crate) fn workflow_run_cancellation(&self, run_id: i64) -> Arc<AtomicBool> {
        self.workflow_cancels
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(run_id)
            .or_default()
            .clone()
    }

    /// Leaves the flag of a run behind, once the thread of the run has taken it to its end: the run has ended, so
    /// nobody reads the flag again.
    pub(crate) fn forget_workflow_run_cancellation(&self, run_id: i64) {
        self.workflow_cancels
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&run_id);
    }

    /// Whether workflow run `run_id` has stopped being one a node's agent may start in: its cancel has been asked
    /// for, or the run has already ended. A cancel sets the flag before it looks for the node's agent run, and the
    /// run ends `cancelled` before the flag is forgotten, so a node that looks after its agent registered sees one of
    /// the two even when the cancel found no process to stop.
    pub(crate) fn workflow_run_has_stopped(&self, run_id: i64) -> bool {
        if self
            .workflow_run_cancellation(run_id)
            .load(Ordering::Relaxed)
        {
            return true;
        }
        matches!(
            self.store.workflow_run(run_id),
            Ok(Some(run)) if matches!(run.status.as_str(), "succeeded" | "failed" | "cancelled")
        )
    }

    /// Stops the agent run a node has just started, when its workflow run stopped while the agent run was being
    /// recorded: the run was in the store before the harness knew its process, so a cancel looked and found none.
    pub(crate) fn stop_late_workflow_node_run(
        &self,
        runs: &dyn CardRuns,
        workflow_run_id: i64,
        agent_run_id: i64,
    ) {
        if self.workflow_run_has_stopped(workflow_run_id) {
            runs.stop(agent_run_id);
        }
    }

    /// Cancels the workflow run `run_id` and ends it `cancelled`, with its log saying why it stopped. A queued run
    /// never starts; a run going stops at its current node, whose agent run is stopped through the same stopping
    /// machinery a card's run uses, and no further node starts; a run waiting on a person leaves its inbox item
    /// withdrawn. The run's worktree and branch, if a node opened them, are left as they are. A finished run refuses
    /// the cancel.
    pub fn cancel_workflow_run(
        &self,
        by: &Actor,
        runs: &dyn CardRuns,
        run_id: i64,
    ) -> std::result::Result<WorkflowRun, CancelWorkflowRunError> {
        let run = self
            .store
            .workflow_run(run_id)?
            .ok_or(CancelWorkflowRunError::NotFound)?;
        if matches!(run.status.as_str(), "succeeded" | "failed" | "cancelled") {
            return Err(CancelWorkflowRunError::AlreadyFinished { status: run.status });
        }

        self.workflow_run_cancellation(run_id)
            .store(true, Ordering::Relaxed);
        // Look at the status again now that the flag is set: a run that moved from queued or waiting to running in
        // between sees the flag before it starts a node, and the status here is the one it moved to, so a node going
        // is stopped.
        let run = self
            .store
            .workflow_run(run_id)?
            .ok_or(CancelWorkflowRunError::NotFound)?;
        // A node going is an agent run of its own, stopped as a card's run is stopped: the flag above is seen when it
        // ends, and no further node starts.
        if run.status == "running" {
            self.stop_workflow_node_runs(by, runs, run.id)?;
        }
        // A run waiting on a person leaves its inbox item withdrawn.
        if run.status == "waiting" {
            self.store.resolve_inbox_for_workflow_run(run_id)?;
        }
        self.end_workflow_run_as_cancelled(run_id)?;
        // The run may have ended on its own between the look above and this one; the end that stands is the one the
        // store took first, and a cancel that came after it has nothing left to stop.
        let done = self
            .store
            .workflow_run(run_id)?
            .ok_or(CancelWorkflowRunError::NotFound)?;
        if done.status != "cancelled" {
            self.forget_workflow_run_cancellation(run_id);
            return Err(CancelWorkflowRunError::AlreadyFinished {
                status: done.status,
            });
        }
        Ok(done)
    }

    /// Stops the agent run of the node a workflow run is on, through the runs' own stopping machinery. A node kind
    /// that runs no agent has none. A run that has just ended, or whose agent has not registered with the harness yet,
    /// refuses the stop; the workflow run still ends cancelled, and the node sees the flag when its agent ends.
    fn stop_workflow_node_runs(
        &self,
        by: &Actor,
        runs: &dyn CardRuns,
        workflow_run_id: i64,
    ) -> crate::app::Result<()> {
        let node_runs = self
            .store
            .active_runs()?
            .into_iter()
            .filter(|run| run.workflow_run_id == Some(workflow_run_id));
        for run in node_runs {
            if let Err(error) = self.stop_run(by, runs, run.id, "the workflow run was cancelled") {
                if matches!(error, AppError::Invalid(_) | AppError::Conflict(_)) {
                    continue;
                }
                return Err(error);
            }
        }
        Ok(())
    }

    /// Ends the run `cancelled`, publishing its events when it is the one that ended it: a cancel and the thread of
    /// the run may both reach this, and only the one the store answers for publishes.
    pub(crate) fn end_workflow_run_as_cancelled(
        &self,
        run_id: i64,
    ) -> std::result::Result<(), crate::app::AppError> {
        let reason = "cancelled by a person";
        let ended = self
            .store
            .finish_workflow_run_if_active(run_id, "cancelled", Some(reason))?;
        if ended {
            self.bus.publish(Event::WorkflowRunEnded {
                run: run_id,
                status: "cancelled".into(),
                error: Some(reason.to_string()),
            });
            self.bus.publish(Event::WorkflowRunStatus {
                run: run_id,
                status: "cancelled".into(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use crate::workflows::definition::WorkflowDefinition;
    use moebius_plugin_api::Host;
    use std::sync::Mutex;
    use std::thread;
    use std::time::{Duration, Instant};

    /// A harness whose `stop` records the run it was asked to stop.
    #[derive(Default)]
    struct Harness {
        stopped: Mutex<Vec<i64>>,
    }

    impl CardRuns for Harness {
        fn start(
            &self,
            _: &Arc<App>,
            _: &crate::store::Card,
            _: &str,
            _: (&str, Option<&str>),
            _: crate::board::Variant,
        ) -> std::result::Result<crate::store::Run, crate::board::StartError> {
            unreachable!("a cancel never starts a run")
        }

        fn has_agent(&self, _: &str) -> bool {
            false
        }

        fn runners(&self) -> Vec<crate::harness::remote::RunnerInfo> {
            Vec::new()
        }

        fn agents(&self) -> Vec<crate::board::AgentInfo> {
            Vec::new()
        }

        fn token_budget(&self, _: &str) -> Option<i64> {
            None
        }

        fn steer(&self, _: i64, _: &str) -> bool {
            false
        }

        fn stop(&self, run_id: i64) -> bool {
            self.stopped.lock().unwrap().push(run_id);
            true
        }
    }

    /// One node behind one that waits for a gate a person would never open.
    fn definition() -> WorkflowDefinition {
        WorkflowDefinition::parse(
            r#"
name = "cancel"
description = "One node behind one that waits for a gate a person would never open."

[[trigger]]
kind = "test"

[[nodes]]
id = "one"
kind = "test"
params = { wait_for_gate = "cancel-run-one" }

[[nodes]]
id = "two"
kind = "test"

[[edges]]
from = "trigger"
to = "one"
port = "out"

[[edges]]
from = "one"
to = "two"
port = "out"
"#,
            "workflows/cancel.toml",
        )
        .unwrap()
    }

    fn approval_definition() -> WorkflowDefinition {
        WorkflowDefinition::parse(
            r#"
name = "asking"
description = "Wait for a person, then run a node that a cancel must never reach."

[[trigger]]
kind = "test"

[[nodes]]
id = "ask"
kind = "approval"
params = { prompt = "Ship the change?", timeout_seconds = 600 }

[[nodes]]
id = "after"
kind = "test"

[[edges]]
from = "trigger"
to = "ask"
port = "out"

[[edges]]
from = "ask"
to = "after"
port = "out"
"#,
            "workflows/asking.toml",
        )
        .unwrap()
    }

    fn app() -> (Arc<App>, Arc<Store>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        (
            Arc::new(App::new(
                store.clone(),
                Arc::new(Host::new(Duration::from_secs(1))),
                &dir.path().join("data"),
            )),
            store,
            dir,
        )
    }

    fn repository(store: &Store) -> crate::store::Repository {
        store
            .add_repository(
                "cancel",
                "https://forge.example/repositories/cancel",
                "github",
                "main",
            )
            .unwrap()
    }

    /// Waits, with a deadline, for `check` to hold.
    fn wait_for(check: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !check() {
            if Instant::now() >= deadline {
                panic!("the condition did not come in ten seconds");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_queued_run_is_cancelled_and_never_starts() {
        let (app, store, _dir) = app();
        let repository = repository(&store);
        let run = store
            .add_workflow_run("cancel", repository.id, "manual", &serde_json::json!({}))
            .unwrap();
        assert_eq!(run.status, "queued");

        let done = app
            .cancel_workflow_run(&Actor::Person, &Harness::default(), run.id)
            .unwrap();

        assert_eq!(done.status, "cancelled");
        assert_eq!(done.error.as_deref(), Some("cancelled by a person"));
        assert!(done.started_at.is_none());
        assert!(done.finished_at.is_some());
        assert!(store.workflow_node_runs(run.id).unwrap().is_empty());
    }

    #[test]
    fn a_running_run_stops_at_its_current_node_and_nothing_after_runs() {
        let (app, store, _dir) = app();
        let repository = repository(&store);
        let run = app
            .start_workflow_run(definition(), repository.id, serde_json::json!({}))
            .unwrap();
        // Wait for the first node to be going.
        wait_for(|| {
            store
                .workflow_node_runs(run.id)
                .unwrap()
                .iter()
                .any(|node| node.node_id == "one" && node.status == "running")
        });

        let done = app
            .cancel_workflow_run(&Actor::Person, &Harness::default(), run.id)
            .unwrap();

        assert_eq!(done.status, "cancelled");
        assert_eq!(done.error.as_deref(), Some("cancelled by a person"));
        // The node going is marked cancelled, as the thread of the run gives it up; no node after it ran.
        wait_for(|| {
            store
                .workflow_node_runs(run.id)
                .unwrap()
                .iter()
                .any(|node| node.node_id == "one" && node.status == "cancelled")
        });
        let nodes = store.workflow_node_runs(run.id).unwrap();
        assert_eq!(
            nodes.len(),
            1,
            "the run must stop at its current node: {nodes:?}"
        );
        assert_eq!(nodes[0].node_id, "one");
    }

    #[test]
    fn a_waiting_run_is_cancelled_and_leaves_its_inbox_item_withdrawn() {
        let (app, store, _dir) = app();
        let repository = repository(&store);
        let run = store
            .add_workflow_run("cancel", repository.id, "manual", &serde_json::json!({}))
            .unwrap();
        store.set_workflow_run_status(run.id, "running").unwrap();
        store.set_workflow_run_status(run.id, "waiting").unwrap();
        store
            .add_inbox_item(
                "workflow_approval",
                None,
                "The run waits for a person",
                &serde_json::json!({ "run": run.id }),
            )
            .unwrap();

        let done = app
            .cancel_workflow_run(&Actor::Person, &Harness::default(), run.id)
            .unwrap();

        assert_eq!(done.status, "cancelled");
        assert!(
            store.inbox().unwrap().is_empty(),
            "the run's inbox item should be withdrawn"
        );
    }

    #[test]
    fn a_run_waiting_on_an_approval_is_cancelled_and_its_wait_stops() {
        let (app, store, _dir) = app();
        let repository = repository(&store);
        let run = app
            .start_workflow_run(approval_definition(), repository.id, serde_json::json!({}))
            .unwrap();
        wait_for(|| store.inbox_for_workflow_run(run.id).unwrap().is_some());

        let done = app
            .cancel_workflow_run(&Actor::Person, &Harness::default(), run.id)
            .unwrap();

        assert_eq!(done.status, "cancelled");
        // The waiting thread sees the cancel, withdraws its item and marks its node cancelled; it does not write the
        // run or the node back to waiting after the run has ended.
        wait_for(|| {
            store
                .workflow_node_runs(run.id)
                .unwrap()
                .iter()
                .any(|node| node.node_id == "ask" && node.status == "cancelled")
        });
        assert_eq!(
            store.workflow_run(run.id).unwrap().unwrap().status,
            "cancelled"
        );
        assert_eq!(store.inbox_for_workflow_run(run.id).unwrap(), None);
        let nodes = store.workflow_node_runs(run.id).unwrap();
        assert_eq!(nodes.len(), 1, "no node after the wait ran: {nodes:?}");
    }

    #[test]
    fn a_finished_run_refuses_the_cancel() {
        let (app, store, _dir) = app();
        let repository = repository(&store);
        let run = store
            .add_workflow_run("cancel", repository.id, "manual", &serde_json::json!({}))
            .unwrap();
        store
            .finish_workflow_run(run.id, "succeeded", None)
            .unwrap();

        let error = app
            .cancel_workflow_run(&Actor::Person, &Harness::default(), run.id)
            .unwrap_err();
        assert!(
            matches!(
                error,
                CancelWorkflowRunError::AlreadyFinished {
                    ref status
                } if status == "succeeded"
            ),
            "{error:?}"
        );
        // And an unknown run is not found.
        assert!(matches!(
            app.cancel_workflow_run(&Actor::Person, &Harness::default(), run.id + 100)
                .unwrap_err(),
            CancelWorkflowRunError::NotFound
        ));
    }

    #[test]
    fn an_agent_run_of_the_node_going_is_stopped_through_the_runs_own_machinery() {
        let (app, store, _dir) = app();
        let repository = repository(&store);
        let run = store
            .add_workflow_run("cancel", repository.id, "manual", &serde_json::json!({}))
            .unwrap();
        store.set_workflow_run_status(run.id, "running").unwrap();
        // The agent run of the node the run is on, as an agent node records it.
        let node_run = store.add_run(None, "Coder", "pi-bolt", "workflow").unwrap();
        store
            .set_run_repository(node_run.id, repository.id)
            .unwrap();
        store.set_run_workflow_run(node_run.id, run.id).unwrap();
        // A run of another workflow run in the same repository is not stopped by this cancel.
        let other_workflow = store
            .add_workflow_run("cancel", repository.id, "manual", &serde_json::json!({}))
            .unwrap();
        let other_run = store.add_run(None, "Coder", "pi-bolt", "workflow").unwrap();
        store
            .set_run_repository(other_run.id, repository.id)
            .unwrap();
        store
            .set_run_workflow_run(other_run.id, other_workflow.id)
            .unwrap();
        let harness = Harness::default();

        let done = app
            .cancel_workflow_run(&Actor::Person, &harness, run.id)
            .unwrap();

        assert_eq!(done.status, "cancelled");
        assert_eq!(
            *harness.stopped.lock().unwrap(),
            vec![node_run.id],
            "the node's agent run is stopped, and only it"
        );
    }

    #[test]
    fn a_cancelled_run_keeps_its_worktree_and_branch() {
        let (app, store, dir) = app();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        crate::repos::git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        crate::repos::git(&origin, &["add", "."]).unwrap();
        crate::repos::git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let repository = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        let run = store
            .add_workflow_run("cancel", repository.id, "manual", &serde_json::json!({}))
            .unwrap();
        store.set_workflow_run_status(run.id, "running").unwrap();
        // The worktree a node of the run opened, on the branch named for the run.
        let worktree = app
            .repos
            .add_workflow_worktree(
                &repository.name,
                std::path::Path::new(&repository.path),
                run.id,
                "main",
            )
            .unwrap();
        store
            .set_workflow_run_worktree(run.id, &worktree.path)
            .unwrap();

        let done = app
            .cancel_workflow_run(&Actor::Person, &Harness::default(), run.id)
            .unwrap();

        assert_eq!(done.status, "cancelled");
        assert!(
            worktree.path.is_dir(),
            "the cancelled run's worktree is left as it is"
        );
        let branch = format!("refs/heads/moebius/workflow-run-{}", run.id);
        assert!(
            crate::repos::git(
                std::path::Path::new(&repository.path),
                &["rev-parse", "--verify", &branch]
            )
            .is_ok(),
            "the cancelled run's branch is left as it is"
        );
    }

    #[test]
    fn an_agent_run_that_registers_after_the_cancel_is_stopped() {
        let (app, store, _dir) = app();
        let repository = repository(&store);
        let run = store
            .add_workflow_run("cancel", repository.id, "manual", &serde_json::json!({}))
            .unwrap();
        store.set_workflow_run_status(run.id, "running").unwrap();
        let harness = Harness::default();
        app.cancel_workflow_run(&Actor::Person, &harness, run.id)
            .unwrap();
        assert!(
            harness.stopped.lock().unwrap().is_empty(),
            "the cancel found no process to stop"
        );
        // The run's thread takes the flag to its end, and only then does the node's agent run register.
        app.forget_workflow_run_cancellation(run.id);
        let node_run = store.add_run(None, "Coder", "pi-bolt", "workflow").unwrap();
        store
            .set_run_repository(node_run.id, repository.id)
            .unwrap();
        store.set_run_workflow_run(node_run.id, run.id).unwrap();

        app.stop_late_workflow_node_run(&harness, run.id, node_run.id);

        assert_eq!(
            *harness.stopped.lock().unwrap(),
            vec![node_run.id],
            "the agent run that registered late is stopped"
        );
    }

    #[test]
    fn the_driver_and_the_cancel_register_the_same_flag() {
        let (app, _store, _dir) = app();
        let first = app.workflow_run_cancellation(7);
        let second = app.workflow_run_cancellation(7);
        first.store(true, Ordering::Relaxed);
        assert!(second.load(Ordering::Relaxed));
        // A different run has its own flag.
        assert!(!app.workflow_run_cancellation(8).load(Ordering::Relaxed));
    }
}
