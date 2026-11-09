//! The part of a workflow run that waits for a person: the approval node takes the run and its node `waiting`, and
//! an inbox item carries the person's decision. A decision comes back through `App::resume_workflow_run`, which
//! records it in the node's row, resolves the inbox item and wakes the thread that waits. A run a stopped server
//! left waiting is driven again by `App::resume_waiting_workflow_runs`: a decision that was made is replayed from
//! the node's row, and a wait that is still open is waited for again, to the deadline its inbox item fixed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, MutexGuard};
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use super::run::{Resume, WorkflowSource};
use crate::app::{App, AppError, Result};
use crate::bus::Event;
use crate::store::{InboxItem, Store, WorkflowNodeRun, WorkflowRun};
use crate::workflows::definition::NodeDefinition;
use crate::workflows::nodes::NodeError;
use crate::workflows::nodes::approval::{self, Decision, KIND};

/// The seconds a decision is waited for at a stretch, so the deadline is honoured to a few seconds.
const WAIT_CHUNK: i64 = 5;
/// How often a wait looks at the run's cancellation flag, so a cancel leaves its inbox item withdrawn and its node
/// cancelled without waiting for the deadline.
const CANCEL_POLL: Duration = Duration::from_millis(20);
/// The seconds the run gives the node over its deadline, so the deadline is what names the run's failure, not the
/// run's own node timeout.
const WAIT_MARGIN: i64 = 2;

/// What the resume route takes: the decision and, with it, the person's value or reason.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRequest {
    /// `approve`, `edit` or `reject`.
    pub decision: String,
    /// The person's value, for an `edit`.
    #[serde(default)]
    pub value: Option<Value>,
    /// The reason the value is rejected, for a `reject`.
    #[serde(default)]
    pub reason: Option<String>,
}

impl App {
    /// Records a person's decision on a workflow run that waits for one: the decision goes into the waiting node's
    /// row, where a server that restarts replays it, and the inbox item is resolved. The thread that waits is woken
    /// when this server drives the run; when none does, the run is driven again so the decision is never left
    /// against a run nobody runs. A second decision on a finished wait is refused.
    pub fn resume_workflow_run(
        self: &Arc<Self>,
        run_id: i64,
        request: &DecisionRequest,
    ) -> Result<WorkflowRun> {
        let run = self
            .store
            .workflow_run(run_id)?
            .ok_or_else(|| AppError::NotFound(format!("workflow run {run_id}")))?;
        if run.status != "waiting" {
            return Err(AppError::Conflict(
                "a second decision on a finished wait is refused".into(),
            ));
        }
        let decision =
            match request.decision.as_str() {
                "approve" => Decision::Approve,
                "edit" => Decision::Edit(request.value.clone().ok_or_else(|| {
                    AppError::Invalid("an `edit` decision needs a `value`".into())
                })?),
                "reject" => Decision::Reject(
                    request
                        .reason
                        .clone()
                        .filter(|reason| !reason.trim().is_empty())
                        .unwrap_or_else(|| "rejected".into()),
                ),
                other => {
                    return Err(AppError::Invalid(format!(
                        "`decision` is `{other}`, not approve, edit or reject"
                    )));
                }
            };

        // The open inbox item that carries the decision, and the waiting row of the node it waits on.
        let item = self
            .store
            .inbox_for_workflow_run(run_id)?
            .ok_or_else(|| AppError::Conflict("the run is not waiting for a decision".into()))?;
        let node_id = item
            .data
            .get("node")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Invalid("the inbox item carries no node".into()))?;
        let row = self
            .store
            .workflow_node_runs(run_id)?
            .into_iter()
            .rev()
            .find(|row| row.node_id == node_id)
            .ok_or_else(|| AppError::Invalid("the waiting node left no row".into()))?;
        if row.status != "waiting" {
            return Err(AppError::Conflict(
                "a second decision on a finished wait is refused".into(),
            ));
        }

        // The decision is recorded in the node's row, where a server that restarts replays it.
        let (status, error, output) = match &decision {
            Decision::Approve => ("succeeded", None, Some(row.input.clone())),
            Decision::Edit(value) => ("succeeded", None, Some(value.clone())),
            Decision::Reject(reason) => ("failed", Some(reason.clone()), None),
        };
        let ended = self.store.finish_waiting_workflow_node_run(
            row.id,
            status,
            output.as_ref(),
            error.as_deref(),
        )?;
        if !ended {
            return Err(AppError::Conflict(
                "a second decision on a finished wait is refused".into(),
            ));
        }
        self.store.resolve_inbox_for_workflow_run(run_id)?;
        self.bus.publish(Event::WorkflowNodeEnded {
            run: run_id,
            node: node_id.to_string(),
            attempt: row.attempt,
            status: status.into(),
            error: error.clone(),
        });
        self.bus.publish(Event::WorkflowNodeStatus {
            run: run_id,
            node: node_id.to_string(),
            attempt: row.attempt,
            status: status.into(),
        });
        match self.decision_waiters().remove(&run_id) {
            // The thread that waits takes the decision, and the run runs on from the node.
            Some(send) => {
                self.store.set_workflow_run_status(run_id, "running")?;
                self.bus.publish(Event::WorkflowRunStatus {
                    run: run_id,
                    status: "running".into(),
                });
                let _ = send.send(decision);
            }
            // Nobody runs this run in this process: a server that stopped left it behind, or the decision came in
            // while this one was driving it. The run waits on until a drive replays its rows and takes the
            // decision from the node's row.
            None => self.replay_waiting_run(run_id)?,
        }

        Ok(self
            .store
            .workflow_run(run_id)?
            .expect("the run was just read"))
    }

    /// Drives the runs a stopped server left waiting again: a decision that was made is replayed from the node's
    /// row, and a wait that is still open is waited for again.
    pub fn resume_waiting_workflow_runs(self: &Arc<Self>) -> Result<()> {
        for run in self.store.waiting_workflow_runs()? {
            self.replay_waiting_run(run.id)?;
        }
        Ok(())
    }

    /// Drives the waiting run `run` again on a thread of its own, from the workflow its row names. The wait takes
    /// what its rows left behind: a decision a person made, or the deadline their inbox item fixed. A run whose
    /// definition can no longer be read fails closed, saying so.
    fn replay_waiting_run(self: &Arc<Self>, run: i64) -> Result<()> {
        let Some(driving) = self.begin_driving(run) else {
            return Ok(());
        };
        let stored = self
            .store
            .workflow_run(run)?
            .ok_or_else(|| AppError::NotFound(format!("workflow run {run}")))?;
        let name = stored.workflow;
        let trigger = stored.value;
        let app = Arc::clone(self);
        let spawned = thread::Builder::new()
            .name(format!("workflow run {run}"))
            .spawn(move || {
                let definition = match name.as_str().definition(&app) {
                    Ok(definition) => definition,
                    Err(error) => {
                        let message = format!("the workflow could no longer be read: {error}");
                        let _ = app.store.finish_workflow_run(run, "failed", Some(&message));
                        let _ = app.store.resolve_inbox_for_workflow_run(run);
                        return;
                    }
                };
                let resume = match app.store.workflow_node_runs(run) {
                    Ok(rows) => rows.last().map(|last| Resume {
                        node: last.node_id.clone(),
                        trigger,
                        input: last.input.clone(),
                        earlier: rows
                            .iter()
                            .filter(|row| row.node_id != last.node_id && row.status == "succeeded")
                            .filter_map(|row| {
                                row.output
                                    .clone()
                                    .map(|output| (row.node_id.clone(), output))
                            })
                            .collect(),
                    }),
                    Err(error) => {
                        let message = format!("the run's nodes could no longer be read: {error}");
                        let _ = app.store.finish_workflow_run(run, "failed", Some(&message));
                        let _ = app.store.resolve_inbox_for_workflow_run(run);
                        return;
                    }
                };
                let _ = app.drive_claimed_workflow_run(run, definition, resume, driving);
            });
        if let Err(error) = spawned {
            self.store
                .finish_workflow_run(run, "failed", Some(&error.to_string()))?;
            self.store.resolve_inbox_for_workflow_run(run)?;
        }
        Ok(())
    }
}

/// The decisions that wait for a workflow run, keyed by run.
impl App {
    fn decision_waiters(
        &self,
    ) -> MutexGuard<'_, std::collections::HashMap<i64, mpsc::Sender<Decision>>> {
        self.workflow_decisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// How long the executor may give the node `node` of run `run`: for an approval node, its deadline over the margin,
/// for the rest the run's own node timeout.
pub(super) fn wait_timeout(app: &App, run: i64, node: &NodeDefinition) -> Option<Duration> {
    if node.kind != KIND {
        return None;
    }
    let deadline = wait_deadline(&app.store, run, node)?;
    let remaining = (deadline - crate::store::now()).max(0);
    Some(Duration::from_secs(remaining as u64 + WAIT_MARGIN as u64))
}

/// The deadline the wait of `node` in run `run` runs to: the one its open inbox item fixed, or the node's own from
/// now.
fn wait_deadline(store: &Store, run: i64, node: &NodeDefinition) -> Option<i64> {
    open_decision(store, run, node)
        .ok()
        .flatten()
        .and_then(|item| item.data.get("deadline").and_then(Value::as_i64))
        .or_else(|| {
            approval::Approval::from_params(&node.params)
                .ok()
                .map(|approval| approval.deadline(crate::store::now()))
        })
}

/// The open inbox item of run `run` that carries the decision of node `node`, if one is open.
fn open_decision(
    store: &Store,
    run: i64,
    node: &NodeDefinition,
) -> crate::store::Result<Option<InboxItem>> {
    Ok(store
        .inbox_for_workflow_run(run)?
        .filter(|item| item.data.get("node") == Some(&Value::String(node.id.clone()))))
}

/// Waits for a person's decision on the value and gives the value the run continues with: the value for an
/// approval, the person's for an edit, or the failure the run ends with.
pub(super) fn wait(
    app: &App,
    run: i64,
    node: &NodeDefinition,
    input: &Value,
    cancellation: &AtomicBool,
) -> std::result::Result<Value, NodeError> {
    let store = &app.store;
    if cancellation.load(Ordering::Relaxed) {
        return Err(NodeError::Cancelled);
    }
    let approval = approval::Approval::from_params(&node.params)?;
    let failed = |error: &crate::store::StoreError| NodeError::Failed(error.to_string());

    // A decision that was made already, for a server that stopped mid-wait: the run continues with it.
    if let Some(decided) = decided_row(store, run, node)? {
        return continue_with(&decided);
    }

    // The inbox item that carries the decision: the one a stopped server left open keeps the deadline it fixed,
    // and a fresh one fixes the node's own from now.
    let open = open_decision(store, run, node).map_err(|error| failed(&error))?;
    let deadline = open
        .as_ref()
        .and_then(|item| item.data.get("deadline").and_then(Value::as_i64))
        .unwrap_or_else(|| approval.deadline(crate::store::now()));
    if open.is_none() {
        store
            .add_inbox_item(
                crate::store::WORKFLOW_APPROVAL,
                None,
                approval.prompt(),
                &json!({
                    "run": run,
                    "node": node.id,
                    "prompt": approval.prompt(),
                    "value": input,
                    "deadline": deadline,
                }),
            )
            .map_err(|error| failed(&error))?;
        if cancellation.load(Ordering::Relaxed) {
            let _ = store.resolve_inbox_for_workflow_run(run);
            return Err(NodeError::Cancelled);
        }
    }

    let rows = store
        .workflow_node_runs(run)
        .map_err(|error| failed(&error))?;
    let row = rows
        .iter()
        .rev()
        .find(|row| row.node_id == node.id)
        .ok_or_else(|| NodeError::Failed("the waiting node left no row".to_string()))?;

    // A wait a stopped server left behind belongs to an attempt no thread runs any more: this attempt takes the
    // wait over, and that row is closed as this one opens.
    for earlier in rows.iter().filter(|earlier| {
        earlier.node_id == node.id && earlier.status == "waiting" && earlier.id != row.id
    }) {
        store
            .finish_workflow_node_run(
                earlier.id,
                "cancelled",
                None,
                Some("the server stopped while this node waited"),
            )
            .map_err(|error| failed(&error))?;
        app.bus.publish(Event::WorkflowNodeStatus {
            run,
            node: node.id.clone(),
            attempt: earlier.attempt,
            status: "cancelled".into(),
        });
    }

    // The channel a decision comes back on, registered before anything says the run waits, so a fast decision is
    // not lost.
    let (send, receive) = mpsc::channel();
    app.decision_waiters().insert(run, send);
    // A decision may have come in between: the node's row says so.
    if let Some(decided) = decided_row(store, run, node)? {
        app.decision_waiters().remove(&run);
        return continue_with(&decided);
    }

    // The run and its node wait; nothing else of the run runs. A cancel that came in before this point leaves no
    // waiting row or run behind, and a cancel after it is seen before the first stretch of waiting.
    if cancellation.load(Ordering::Relaxed) {
        app.decision_waiters().remove(&run);
        let _ = store.resolve_inbox_for_workflow_run(run);
        return Err(NodeError::Cancelled);
    }
    let node_waiting = store
        .set_workflow_node_run_status(row.id, "waiting")
        .map_err(|error| failed(&error))?;
    let run_waiting = node_waiting
        && store
            .set_workflow_run_status(run, "waiting")
            .map_err(|error| failed(&error))?;
    if !run_waiting {
        app.decision_waiters().remove(&run);
        let _ = store.finish_waiting_workflow_node_run(
            row.id,
            "cancelled",
            None,
            Some("the workflow run was cancelled"),
        );
        let _ = store.resolve_inbox_for_workflow_run(run);
        return Err(NodeError::Cancelled);
    }
    app.bus.publish(Event::WorkflowNodeStatus {
        run,
        node: node.id.clone(),
        attempt: row.attempt,
        status: "waiting".into(),
    });
    app.bus.publish(Event::WorkflowRunWaiting { run });
    app.bus.publish(Event::WorkflowRunStatus {
        run,
        status: "waiting".into(),
    });

    loop {
        if cancellation.load(Ordering::Relaxed) {
            app.decision_waiters().remove(&run);
            let _ = store.finish_waiting_workflow_node_run(
                row.id,
                "cancelled",
                None,
                Some("the workflow run was cancelled"),
            );
            let _ = store.resolve_inbox_for_workflow_run(run);
            return Err(NodeError::Cancelled);
        }
        let remaining = deadline - crate::store::now();
        if remaining <= 0 {
            break;
        }
        let look = Duration::from_millis(remaining as u64)
            .min(Duration::from_secs(WAIT_CHUNK as u64))
            .min(CANCEL_POLL);
        match receive.recv_timeout(look) {
            Ok(decision) => {
                app.decision_waiters().remove(&run);
                return approval.continues_with(input, &decision);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    // The wait reached its deadline: a decision that was made meanwhile is replayed, and the run otherwise fails
    // closed, naming the deadline.
    app.decision_waiters().remove(&run);
    let deadline_error = approval::Approval::deadline_error(deadline);
    let ended = store
        .finish_waiting_workflow_node_run(row.id, "failed", None, Some(&deadline_error.to_string()))
        .map_err(|error| NodeError::Failed(error.to_string()))?;
    if !ended {
        // A decision took the wait as the deadline came due, and is what the run continues with.
        if let Some(decided) = decided_row(store, run, node)? {
            return continue_with(&decided);
        }
    }
    let _ = store.resolve_inbox_for_workflow_run(run);
    Err(deadline_error)
}

/// The node's row of a decision that was made already: its last attempt, ended with a value or a reason.
fn decided_row(
    store: &Store,
    run: i64,
    node: &NodeDefinition,
) -> std::result::Result<Option<WorkflowNodeRun>, NodeError> {
    Ok(store
        .workflow_node_runs(run)
        .map_err(|error| NodeError::Failed(error.to_string()))?
        .into_iter()
        .filter(|row| row.node_id == node.id)
        .find(|row| matches!(row.status.as_str(), "succeeded" | "failed")))
}

/// The value the run continues with, from a decision that was recorded in the node's row already.
fn continue_with(decided: &WorkflowNodeRun) -> std::result::Result<Value, NodeError> {
    if decided.status == "succeeded" {
        Ok(decided.output.clone().unwrap_or(Value::Null))
    } else {
        Err(NodeError::Failed(
            decided
                .error
                .clone()
                .unwrap_or_else(|| "the approval was rejected".to_string()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use moebius_plugin_api::Host;

    use super::*;
    use crate::harness::repo::testing;
    use crate::repos::git;
    use crate::store::{Repository, Store};

    /// A workflow that asks a person between two nodes, and asks them with `prompt`.
    fn asking(timeout_seconds: Option<u64>) -> String {
        let deadline = match timeout_seconds {
            Some(seconds) => format!("prompt = \"Ship the change?\", timeout_seconds = {seconds}"),
            None => "prompt = \"Ship the change?\"".to_string(),
        };
        format!(
            r#"
name = "asking"
description = "Ask a person before the change ships."

[[trigger]]
kind = "test"

[[nodes]]
id = "ask"
kind = "approval"
params = {{ {deadline} }}

[[nodes]]
id = "after"
kind = "test"

[[edges]]
from = "trigger"
port = "out"
to = "ask"
[[edges]]
from = "ask"
port = "out"
to = "after"
"#
        )
    }

    /// A workflow like `asking` with a node before the wait, so a test can see whether a restart runs that node
    /// again.
    fn building(timeout_seconds: Option<u64>) -> String {
        let deadline = match timeout_seconds {
            Some(seconds) => format!("prompt = \"Ship the change?\", timeout_seconds = {seconds}"),
            None => "prompt = \"Ship the change?\"".to_string(),
        };
        format!(
            r#"
name = "building"
description = "Build the change, then ask a person before it ships."

[[trigger]]
kind = "test"

[[nodes]]
id = "build"
kind = "test"

[[nodes]]
id = "ask"
kind = "approval"
params = {{ {deadline} }}

[[nodes]]
id = "after"
kind = "test"

[[edges]]
from = "trigger"
port = "out"
to = "build"
[[edges]]
from = "build"
port = "out"
to = "ask"
[[edges]]
from = "ask"
port = "out"
to = "after"
"#
        )
    }

    /// A workflow like `building` whose node after the wait checks both the run's trigger and the node before it.
    fn checking() -> String {
        r#"
name = "checking"
description = "Build the change, ask a person, then check that the trigger and the build reached the check."

[[trigger]]
kind = "test"

[[nodes]]
id = "build"
kind = "test"

[[nodes]]
id = "ask"
kind = "approval"
params = { prompt = "Ship the change?" }

[[nodes]]
id = "check"
kind = "condition"
params = { ports = [{ name = "pass", when = "trigger.source == 'trigger' and node('build').source == 'build'" }] }

[[nodes]]
id = "after"
kind = "test"

[[edges]]
from = "trigger"
port = "out"
to = "build"
[[edges]]
from = "build"
port = "out"
to = "ask"
[[edges]]
from = "ask"
port = "out"
to = "check"
[[edges]]
from = "check"
port = "pass"
to = "after"
"#
        .to_string()
    }

    /// A server with a harness configuration repository and one repository a workflow run can belong to.
    fn setup(text: &str) -> (tempfile::TempDir, Arc<App>, Repository) {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = Arc::new(App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(5))),
            &dir.path().join("data"),
        ));
        let harness = testing::use_harness(&app, dir.path());
        let folder = harness.path().join("workflows");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("asking.toml"), text).unwrap();
        let repository = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        (dir, app, repository)
    }

    /// What the run carries into the wait, so a test can see which value the decision leaves on the run.
    fn value() -> Value {
        json!({"change": 7})
    }

    fn request(decision: &str, value: Option<Value>, reason: Option<&str>) -> DecisionRequest {
        DecisionRequest {
            decision: decision.to_string(),
            value,
            reason: reason.map(str::to_string),
        }
    }

    /// Waits for the bus to say run `id` took `status`, then returns it as stored.
    fn wait_for_status(
        app: &App,
        events: &mut tokio::sync::broadcast::Receiver<crate::bus::AppEvent>,
        id: i64,
        status: &str,
    ) -> WorkflowRun {
        loop {
            match events.blocking_recv() {
                Ok(event)
                    if event.kind == "workflow_run_status"
                        && event.data["run"] == id
                        && event.data["status"] == status =>
                {
                    return app.store.workflow_run(id).unwrap().unwrap();
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(error) => panic!("the bus closed before the run was {status}: {error}"),
            }
        }
    }

    /// Waits for the bus to say run `id` ended, then returns it as stored.
    fn wait_for_end(
        app: &App,
        events: &mut tokio::sync::broadcast::Receiver<crate::bus::AppEvent>,
        id: i64,
    ) -> WorkflowRun {
        loop {
            match events.blocking_recv() {
                Ok(event) if event.kind == "workflow_run_ended" && event.data["run"] == id => {
                    return app.store.workflow_run(id).unwrap().unwrap();
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(error) => panic!("the bus closed before the run ended: {error}"),
            }
        }
    }

    /// The nodes of run `id` that have a row, each as `id:status`.
    fn nodes(app: &App, id: i64) -> Vec<String> {
        app.store
            .workflow_node_runs(id)
            .unwrap()
            .iter()
            .map(|row| format!("{}:{}", row.node_id, row.status))
            .collect()
    }

    /// The run and its wait as a server that stopped mid-wait leaves them: the run and its node `waiting`, and the
    /// inbox item still open with the deadline it fixed.
    fn left_waiting(app: &App, repository_id: i64) -> WorkflowRun {
        left_waiting_with(app, repository_id, &value())
    }

    /// The run and its wait as a server that stopped mid-wait leaves them, waiting on `value`.
    fn left_waiting_with(app: &App, repository_id: i64, value: &Value) -> WorkflowRun {
        let run = app
            .store
            .add_workflow_run("asking", repository_id, "test", value)
            .unwrap();
        let row = app
            .store
            .add_workflow_node_run(run.id, "ask", 1, value)
            .unwrap();
        app.store
            .set_workflow_node_run_status(row.id, "waiting")
            .unwrap();
        app.store
            .add_inbox_item(
                crate::store::WORKFLOW_APPROVAL,
                None,
                "Ship the change?",
                &json!({
                    "run": run.id,
                    "node": "ask",
                    "prompt": "Ship the change?",
                    "value": value,
                    "deadline": crate::store::now() + 600,
                }),
            )
            .unwrap();
        app.store
            .set_workflow_run_status(run.id, "waiting")
            .unwrap();
        app.store.workflow_run(run.id).unwrap().unwrap()
    }

    /// The run and its wait as a server that stopped mid-wait leaves them, after `build` has run: the run and its
    /// wait node `waiting`, the node before it finished, and the inbox item still open.
    fn left_waiting_after_a_build(app: &App, repository_id: i64) -> WorkflowRun {
        let run = app
            .store
            .add_workflow_run("building", repository_id, "test", &value())
            .unwrap();
        let build = app
            .store
            .add_workflow_node_run(run.id, "build", 1, &value())
            .unwrap();
        app.store
            .finish_workflow_node_run(build.id, "succeeded", Some(&value()), None)
            .unwrap();
        let row = app
            .store
            .add_workflow_node_run(run.id, "ask", 1, &value())
            .unwrap();
        app.store
            .set_workflow_node_run_status(row.id, "waiting")
            .unwrap();
        app.store
            .add_inbox_item(
                crate::store::WORKFLOW_APPROVAL,
                None,
                "Ship the change?",
                &json!({
                    "run": run.id,
                    "node": "ask",
                    "prompt": "Ship the change?",
                    "value": value(),
                    "deadline": crate::store::now() + 600,
                }),
            )
            .unwrap();
        app.store
            .set_workflow_run_status(run.id, "waiting")
            .unwrap();
        app.store.workflow_run(run.id).unwrap().unwrap()
    }

    #[test]
    fn a_run_waits_for_the_decision_and_runs_nothing_else_while_it_does() {
        let (_dir, app, repository) = setup(&asking(None));
        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run("asking", repository.id, value())
            .unwrap();

        let waiting = wait_for_status(&app, &mut events, run.id, "waiting");
        assert_eq!(waiting.status, "waiting");
        assert_eq!(
            nodes(&app, run.id),
            vec!["ask:waiting"],
            "the wait holds the run; nothing after it has started"
        );

        // A person is asked, with the prompt and the value the decision is about.
        let item = app
            .store
            .inbox_for_workflow_run(run.id)
            .unwrap()
            .expect("the wait asks a person");
        assert_eq!(item.kind, crate::store::WORKFLOW_APPROVAL);
        assert_eq!(item.title, "Ship the change?");
        assert_eq!(item.data["node"], json!("ask"));
        assert_eq!(item.data["value"], value());
        assert_eq!(item.resolved_at, None);

        // The decision is what ends the wait.
        app.resume_workflow_run(run.id, &request("approve", None, None))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);
        assert_eq!(ended.status, "succeeded");
        assert_eq!(
            nodes(&app, run.id),
            vec!["ask:succeeded", "after:succeeded"]
        );
        assert_eq!(
            app.store.inbox_for_workflow_run(run.id).unwrap(),
            None,
            "the decision resolves the inbox item"
        );
    }

    #[test]
    fn a_cancel_before_the_wait_starts_leaves_no_inbox_item_or_waiting_run() {
        let (_dir, app, repository) = setup(&asking(None));
        let definition = crate::workflows::definition::WorkflowDefinition::parse(
            &asking(None),
            "workflows/asking.toml",
        )
        .unwrap();
        let node = definition
            .nodes
            .iter()
            .find(|node| node.id == "ask")
            .unwrap()
            .clone();
        let run = app
            .store
            .add_workflow_run("asking", repository.id, "test", &value())
            .unwrap();
        let row = app
            .store
            .add_workflow_node_run(run.id, "ask", 1, &value())
            .unwrap();
        let cancellation = app.workflow_run_cancellation(run.id);
        cancellation.store(true, Ordering::Relaxed);

        let error = wait(&app, run.id, &node, &value(), &cancellation).unwrap_err();

        assert!(matches!(error, NodeError::Cancelled), "{error:?}");
        assert_eq!(
            app.store.inbox_for_workflow_run(run.id).unwrap(),
            None,
            "the cancelled run must not ask a person"
        );
        assert_eq!(
            app.store.workflow_run(run.id).unwrap().unwrap().status,
            "queued",
            "a cancel that came before the wait must leave no waiting run behind"
        );
        assert_eq!(
            app.store.workflow_node_runs(run.id).unwrap()[0].status,
            "running",
            "and no waiting node behind"
        );
        assert_eq!(row.node_id, "ask");
    }

    #[test]
    fn an_approval_shows_the_prompt_built_from_the_parameters_and_the_value() {
        let workflow = asking(None).replace(
            "Ship the change?",
            "Ship {{ input.change }} ({{ trigger.change }})?",
        );
        let (_dir, app, repository) = setup(&workflow);
        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run("asking", repository.id, value())
            .unwrap();
        wait_for_status(&app, &mut events, run.id, "waiting");

        let item = app
            .store
            .inbox_for_workflow_run(run.id)
            .unwrap()
            .expect("the wait asks a person");
        assert_eq!(item.title, "Ship 7 (7)?");
        assert_eq!(item.data["prompt"], json!("Ship 7 (7)?"));

        app.resume_workflow_run(run.id, &request("reject", None, Some("done")))
            .unwrap();
        wait_for_end(&app, &mut events, run.id);
    }

    #[test]
    fn an_approval_carries_the_value_of_the_run_on() {
        let (_dir, app, repository) = setup(&asking(None));
        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run("asking", repository.id, value())
            .unwrap();
        wait_for_status(&app, &mut events, run.id, "waiting");

        app.resume_workflow_run(run.id, &request("approve", None, None))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);

        assert_eq!(ended.status, "succeeded");
        assert_eq!(left_the_wait(&app, run.id), value());
    }

    #[test]
    fn an_edit_carries_the_persons_value_on() {
        let (_dir, app, repository) = setup(&asking(None));
        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run("asking", repository.id, value())
            .unwrap();
        wait_for_status(&app, &mut events, run.id, "waiting");

        let shipped = json!({"change": 8});
        app.resume_workflow_run(run.id, &request("edit", Some(shipped.clone()), None))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);

        assert_eq!(ended.status, "succeeded");
        assert_eq!(left_the_wait(&app, run.id), shipped);
    }

    #[test]
    fn a_rejection_fails_the_run_with_the_reason() {
        let (_dir, app, repository) = setup(&asking(None));
        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run("asking", repository.id, value())
            .unwrap();
        wait_for_status(&app, &mut events, run.id, "waiting");

        app.resume_workflow_run(run.id, &request("reject", None, Some("not this week")))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);

        assert_eq!(ended.status, "failed");
        assert_eq!(ended.error.as_deref(), Some("not this week"));
        assert_eq!(nodes(&app, run.id), vec!["ask:failed"]);
    }

    #[test]
    fn a_second_decision_on_a_finished_wait_is_refused() {
        let (_dir, app, repository) = setup(&asking(None));
        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run("asking", repository.id, value())
            .unwrap();
        wait_for_status(&app, &mut events, run.id, "waiting");
        app.resume_workflow_run(run.id, &request("approve", None, None))
            .unwrap();
        wait_for_end(&app, &mut events, run.id);

        let error = app
            .resume_workflow_run(run.id, &request("reject", None, Some("too late")))
            .unwrap_err();
        assert!(
            error.to_string().contains("second decision"),
            "the second decision is refused: {error}"
        );
        // The decision the run took stands.
        assert_eq!(
            app.store.workflow_run(run.id).unwrap().unwrap().status,
            "succeeded"
        );
    }

    #[test]
    fn a_wait_nobody_decides_by_its_deadline_fails_the_run_closed() {
        let (_dir, app, repository) = setup(&asking(Some(1)));
        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run("asking", repository.id, value())
            .unwrap();

        let ended = wait_for_end(&app, &mut events, run.id);
        assert_eq!(ended.status, "failed");
        let error = ended.error.expect("the run says why it failed");
        assert!(error.contains("deadline"), "{error}");
        assert_eq!(nodes(&app, run.id), vec!["ask:failed"]);
        assert_eq!(
            app.store.inbox_for_workflow_run(run.id).unwrap(),
            None,
            "the finished wait leaves nobody asked"
        );
    }

    #[test]
    fn a_run_a_stopped_server_left_waiting_waits_again() {
        let (_dir, app, repository) = setup(&asking(None));
        let run = left_waiting(&app, repository.id);
        let mut events = app.bus.subscribe();

        app.resume_waiting_workflow_runs().unwrap();

        // The run waits again, with the same question still open for the person, and nothing after it has run.
        let waiting = wait_for_status(&app, &mut events, run.id, "waiting");
        assert_eq!(waiting.status, "waiting");
        assert_eq!(
            nodes(&app, run.id),
            vec!["ask:cancelled", "ask:waiting"],
            "the wait the server left is closed and this attempt waits in its place, with nothing after it started"
        );
        assert_eq!(
            app.store
                .inbox_for_workflow_run(run.id)
                .unwrap()
                .map(|item| item.title),
            Some("Ship the change?".to_string()),
            "the person is still asked"
        );

        app.resume_workflow_run(run.id, &request("approve", None, None))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);
        assert_eq!(ended.status, "succeeded");
        assert_eq!(left_the_wait(&app, run.id), value());
    }

    #[test]
    fn a_replay_that_cannot_read_the_workflow_ends_the_run_and_leaves_nobody_asked() {
        let (_dir, app, repository) = setup(&asking(None));
        let run = left_waiting(&app, repository.id);
        std::fs::remove_file(
            app.harness_repo()
                .unwrap()
                .path()
                .join("workflows")
                .join("asking.toml"),
        )
        .unwrap();

        app.resume_waiting_workflow_runs().unwrap();
        app.wait_until("the run to fail", || {
            app.store.workflow_run(run.id).unwrap().unwrap().status == "failed"
        })
        .unwrap();

        let ended = app.store.workflow_run(run.id).unwrap().unwrap();
        assert!(
            ended
                .error
                .as_deref()
                .is_some_and(|error| error.contains("could no longer be read")),
            "the run fails closed saying why: {ended:?}"
        );
        assert_eq!(
            app.store.inbox_for_workflow_run(run.id).unwrap(),
            None,
            "a run that is over leaves nobody asked"
        );
    }

    #[test]
    fn a_restart_resumes_at_the_wait_without_running_an_earlier_node_again() {
        let (_dir, app, repository) = setup(&asking(None));
        std::fs::write(
            app.harness_repo()
                .unwrap()
                .path()
                .join("workflows")
                .join("building.toml"),
            building(None),
        )
        .unwrap();
        let run = left_waiting_after_a_build(&app, repository.id);
        let mut events = app.bus.subscribe();

        app.resume_waiting_workflow_runs().unwrap();

        // The run resumes at the wait itself: the node before it does not run a second time.
        let waiting = wait_for_status(&app, &mut events, run.id, "waiting");
        assert_eq!(waiting.status, "waiting");
        assert_eq!(
            nodes(&app, run.id),
            vec!["build:succeeded", "ask:cancelled", "ask:waiting"],
            "the node before the wait keeps its one row; the wait is attempted again"
        );

        app.resume_workflow_run(run.id, &request("approve", None, None))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);
        assert_eq!(ended.status, "succeeded");
        assert_eq!(left_the_wait(&app, run.id), value());
    }

    #[test]
    fn a_restart_gives_the_nodes_after_the_wait_the_trigger_and_the_earlier_values() {
        let (_dir, app, repository) = setup(&asking(None));
        std::fs::write(
            app.harness_repo()
                .unwrap()
                .path()
                .join("workflows")
                .join("checking.toml"),
            checking(),
        )
        .unwrap();
        let trigger = json!({"change": 7, "source": "trigger"});
        let built = json!({"change": 7, "source": "build"});
        let run = app
            .store
            .add_workflow_run("checking", repository.id, "test", &trigger)
            .unwrap();
        let build = app
            .store
            .add_workflow_node_run(run.id, "build", 1, &trigger)
            .unwrap();
        app.store
            .finish_workflow_node_run(build.id, "succeeded", Some(&built), None)
            .unwrap();
        let row = app
            .store
            .add_workflow_node_run(run.id, "ask", 1, &built)
            .unwrap();
        app.store
            .set_workflow_node_run_status(row.id, "waiting")
            .unwrap();
        app.store
            .add_inbox_item(
                crate::store::WORKFLOW_APPROVAL,
                None,
                "Ship the change?",
                &json!({
                    "run": run.id,
                    "node": "ask",
                    "prompt": "Ship the change?",
                    "value": built,
                    "deadline": crate::store::now() + 600,
                }),
            )
            .unwrap();
        app.store
            .set_workflow_run_status(run.id, "waiting")
            .unwrap();
        let mut events = app.bus.subscribe();

        app.resume_waiting_workflow_runs().unwrap();
        wait_for_status(&app, &mut events, run.id, "waiting");
        app.resume_workflow_run(run.id, &request("approve", None, None))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);

        assert_eq!(
            ended.status, "succeeded",
            "the check saw the run's trigger and the node before the wait: {:?}",
            ended.error
        );
        assert_eq!(
            nodes(&app, run.id),
            vec![
                "build:succeeded",
                "ask:cancelled",
                "ask:succeeded",
                "check:succeeded",
                "after:succeeded"
            ]
        );
    }

    #[test]
    fn a_decision_made_while_no_server_ran_the_run_takes_it_on() {
        let (_dir, app, repository) = setup(&asking(None));
        let run = left_waiting(&app, repository.id);
        let mut events = app.bus.subscribe();

        // The person decides while nothing drives the run: the decision is recorded on the waiting
        // node, and the run is driven to take it. The drive runs on its own thread, so the run's own
        // status may already be `running`; the node's row is what the decision fixed.
        app.resume_workflow_run(run.id, &request("approve", None, None))
            .unwrap();
        let decided = app
            .store
            .workflow_node_runs(run.id)
            .unwrap()
            .into_iter()
            .find(|row| row.node_id == "ask")
            .expect("the waiting node has a row");
        assert_eq!(
            decided.status, "succeeded",
            "the decision is recorded for the drive that takes it"
        );

        let ended = wait_for_end(&app, &mut events, run.id);
        assert_eq!(ended.status, "succeeded");
        assert_eq!(left_the_wait(&app, run.id), value());

        // A second decision on the same wait is refused.
        let error = app
            .resume_workflow_run(run.id, &request("approve", None, None))
            .unwrap_err();
        assert!(error.to_string().contains("second decision"), "{error}");
    }

    #[test]
    fn an_approval_taken_up_after_a_restart_continues_with_the_value_as_stored() {
        let (_dir, app, repository) = setup(&asking(None));
        let secret = "workflow-approval-secret-227-a1b2c3";
        crate::blobs::remember_secret(secret);
        let value = json!({"change": 7, "token": secret});
        let run = left_waiting_with(&app, repository.id, &value);
        let mut events = app.bus.subscribe();

        app.resume_waiting_workflow_runs().unwrap();
        wait_for_status(&app, &mut events, run.id, "waiting");
        app.resume_workflow_run(run.id, &request("approve", None, None))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);

        // The wait the server left carries the value only as the store kept it: the part that is not a secret
        // survives, and the secret is redacted rather than kept or lost altogether.
        assert_eq!(ended.status, "succeeded");
        let after = left_the_wait(&app, run.id);
        assert_eq!(after["change"], json!(7));
        assert!(after.to_string().contains("[redacted:secret]"), "{after}");
        assert!(!after.to_string().contains(secret), "{after}");
    }

    /// The value the node after the wait was given: what the decision left on the run.
    fn left_the_wait(app: &App, id: i64) -> Value {
        app.store
            .workflow_node_runs(id)
            .unwrap()
            .into_iter()
            .find(|row| row.node_id == "after")
            .expect("the run ran on past the wait")
            .input
    }
}
