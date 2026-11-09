//! Starting and driving a workflow run. A trigger names a workflow, gives it a value and a repository, and gets back
//! the stored run at once; the executor follows the definition off the request path, and this module records every node
//! and the run's own end in the store and on the bus.

use std::sync::Arc;
use std::thread;

use serde_json::Value;

use crate::app::{App, AppError, Result};
use crate::bus::Event;
use crate::store::WorkflowRun;
use crate::workflows::decorate::Scope;
use crate::workflows::definition::{self, NodeDefinition, WorkflowDefinition};
use crate::workflows::nodes::{NodeError, Routed};
use crate::workflows::run::{Error as RunError, Limits, Observer, Runner, StepResult, execute};

/// A definition a trigger asks a run for: either the definition itself or the name of its file in the harness
/// configuration repository.
pub trait WorkflowSource {
    /// The definition this source names, read and validated.
    fn definition(self, app: &App) -> Result<WorkflowDefinition>;
}

impl WorkflowSource for &str {
    fn definition(self, app: &App) -> Result<WorkflowDefinition> {
        definition::read(app.harness_repo()?.path(), self)
            .map_err(|error| AppError::Invalid(error.to_string()))
    }
}

impl WorkflowSource for WorkflowDefinition {
    fn definition(self, _app: &App) -> Result<WorkflowDefinition> {
        Ok(self)
    }
}

impl App {
    /// Stores a run of `source` for `repository_id`, started with `value`, and drives it on its own thread. This is the
    /// one door a trigger has into a run. The run is in the store before this returns, and an unreadable definition is
    /// refused before anything is stored.
    pub fn start_workflow_run<S: WorkflowSource>(
        self: &Arc<Self>,
        source: S,
        repository_id: i64,
        value: Value,
    ) -> Result<WorkflowRun> {
        let definition = source.definition(self)?;
        definition
            .validate(&format!("workflows/{}.toml", definition.name))
            .map_err(|error| AppError::Invalid(error.to_string()))?;
        let trigger = definition
            .trigger
            .first()
            .map(|trigger| trigger.kind.clone())
            .unwrap_or_default();
        let run = self
            .store
            .add_workflow_run(&definition.name, repository_id, &trigger, &value)?;

        self.bus.publish(Event::WorkflowRunQueued {
            run: run.id,
            workflow: run.workflow.clone(),
            repository: run.repository_id,
        });

        let app = Arc::clone(self);
        let stored = run.id;
        let spawned = thread::Builder::new()
            .name(format!("workflow run {stored}"))
            .spawn(move || {
                let _ = app.drive_workflow_run(stored, definition);
            });
        if let Err(error) = spawned {
            self.store
                .finish_workflow_run(run.id, "failed", Some(&error.to_string()))?;
            return Err(AppError::Invalid(format!(
                "cannot start workflow run: {error}"
            )));
        }

        Ok(run)
    }

    /// Runs one stored run to its end, recording each node before it starts and after it finishes.
    fn drive_workflow_run(&self, run_id: i64, definition: WorkflowDefinition) -> Result<()> {
        let run = self
            .store
            .workflow_run(run_id)?
            .ok_or_else(|| AppError::NotFound(format!("workflow run {run_id}")))?;
        self.store.set_workflow_run_status(run_id, "running")?;
        self.bus.publish(Event::WorkflowRunStarted { run: run_id });
        self.bus.publish(Event::WorkflowRunStatus {
            run: run_id,
            status: "running".into(),
        });

        let runner: Arc<dyn Runner> = Arc::new(NodeRunner);
        let mut observer = StoreObserver {
            app: self,
            run: run_id,
            current: None,
        };
        let result = execute(
            &definition,
            run.value,
            Limits::from_definition(&definition),
            &runner,
            &mut observer,
        );

        match result {
            Ok(_) => {
                self.store.finish_workflow_run(run_id, "succeeded", None)?;
                self.bus.publish(Event::WorkflowRunEnded {
                    run: run_id,
                    status: "succeeded".into(),
                    error: None,
                });
                self.bus.publish(Event::WorkflowRunStatus {
                    run: run_id,
                    status: "succeeded".into(),
                });
            }
            Err(error) => {
                let error = error.to_string();
                self.store
                    .finish_workflow_run(run_id, "failed", Some(&error))?;
                self.bus.publish(Event::WorkflowRunEnded {
                    run: run_id,
                    status: "failed".into(),
                    error: Some(error.clone()),
                });
                self.bus.publish(Event::WorkflowRunStatus {
                    run: run_id,
                    status: "failed".into(),
                });
            }
        }

        Ok(())
    }
}

/// Runs a node through the same registry a definition is validated against.
struct NodeRunner;

impl Runner for NodeRunner {
    fn run(&self, node: &NodeDefinition, input: &Value) -> std::result::Result<Value, NodeError> {
        crate::workflows::nodes::node(&node.kind, &node.params)?.run(input.clone())
    }

    fn run_routed(
        &self,
        node: &NodeDefinition,
        scope: &Scope,
    ) -> std::result::Result<Routed, NodeError> {
        crate::workflows::nodes::node(&node.kind, &node.params)?.run_routed(scope)
    }
}

/// Records a run's nodes in the store and publishes them as they start and finish.
struct StoreObserver<'a> {
    app: &'a App,
    run: i64,
    current: Option<i64>,
}

impl Observer for StoreObserver<'_> {
    fn started(
        &mut self,
        node: &NodeDefinition,
        input: &Value,
    ) -> std::result::Result<(), RunError> {
        let recorded = self
            .app
            .store
            .add_workflow_node_run(self.run, &node.id, 1, input)
            .map_err(|error| record_error(node, &error))?;
        self.current = Some(recorded.id);
        self.app.bus.publish(Event::WorkflowNodeStarted {
            run: self.run,
            node: node.id.clone(),
            attempt: 1,
        });
        self.app.bus.publish(Event::WorkflowNodeStatus {
            run: self.run,
            node: node.id.clone(),
            attempt: 1,
            status: "running".into(),
        });
        Ok(())
    }

    fn finished(
        &mut self,
        node: &NodeDefinition,
        result: &StepResult,
    ) -> std::result::Result<(), RunError> {
        let id = self.current.take().ok_or_else(|| RunError::Record {
            node: node.id.clone(),
            reason: "it was not recorded as started".into(),
        })?;
        match result {
            StepResult::Succeeded(output) => self
                .app
                .store
                .finish_workflow_node_run(id, "succeeded", Some(output), None)
                .map_err(|error| record_error(node, &error))?,
            StepResult::Failed(error) => self
                .app
                .store
                .finish_workflow_node_run(id, "failed", None, Some(&error.to_string()))
                .map_err(|error| record_error(node, &error))?,
        }
        let status = match result {
            StepResult::Succeeded(_) => "succeeded",
            StepResult::Failed(_) => "failed",
        };
        let error = match result {
            StepResult::Succeeded(_) => None,
            StepResult::Failed(error) => Some(error.to_string()),
        };
        self.app.bus.publish(Event::WorkflowNodeEnded {
            run: self.run,
            node: node.id.clone(),
            attempt: 1,
            status: status.into(),
            error,
        });
        self.app.bus.publish(Event::WorkflowNodeStatus {
            run: self.run,
            node: node.id.clone(),
            attempt: 1,
            status: status.into(),
        });
        Ok(())
    }
}

/// A store error met while recording `node`.
fn record_error(node: &NodeDefinition, error: &impl std::fmt::Display) -> RunError {
    RunError::Record {
        node: node.id.clone(),
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;
    use serde_json::json;

    use super::*;
    use crate::harness::repo::testing;
    use crate::repos::git;
    use crate::store::{Repository, Store};

    /// A server with a harness configuration repository and one repository a workflow run can belong to.
    fn setup() -> (tempfile::TempDir, Arc<App>, Repository) {
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
            Arc::new(Host::new(Duration::from_secs(1))),
            &dir.path().join("data"),
        ));
        testing::use_harness(&app, dir.path());
        let repository = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        (dir, app, repository)
    }

    /// Writes `text` as the workflow called `name` in the harness configuration repository.
    fn write_workflow(app: &App, name: &str, text: &str) {
        let folder = app.harness_repo().unwrap().path().join("workflows");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(format!("{name}.toml")), text).unwrap();
    }

    /// Waits for the bus to say that run `id` ended, then returns it as stored.
    fn wait_for_end(
        app: &App,
        events: &mut tokio::sync::broadcast::Receiver<crate::bus::AppEvent>,
        id: i64,
    ) -> crate::store::WorkflowRun {
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

    fn definition(value: &str) -> WorkflowDefinition {
        WorkflowDefinition::parse(value, "workflows/test.toml").unwrap()
    }

    fn chain() -> WorkflowDefinition {
        definition(
            r#"
name = "recorded"
description = "Run two nodes in a row."

[[trigger]]
kind = "test"

[[nodes]]
id = "one"
kind = "test"
params = { wait_for_gate = "recorded-run" }

[[nodes]]
id = "two"
kind = "test"

[[edges]]
from = "trigger"
port = "out"
to = "one"

[[edges]]
from = "one"
port = "out"
to = "two"
"#,
        )
    }

    #[test]
    fn a_started_run_is_stored_before_it_finishes_and_records_its_nodes_in_order() {
        let (_dir, app, repository) = setup();

        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run(chain(), repository.id, json!({"seed": 1}))
            .unwrap();

        let stored = app.store.workflow_run(run.id).unwrap().unwrap();
        assert!(
            stored.status != "succeeded" && stored.status != "failed",
            "the run had already finished: {stored:?}"
        );

        crate::workflows::nodes::test_gate::release("recorded-run");
        let ended = wait_for_end(&app, &mut events, run.id);
        assert_eq!(ended.status, "succeeded");
        assert_eq!(ended.error, None);

        let nodes = app.store.workflow_node_runs(run.id).unwrap();
        assert_eq!(
            nodes
                .iter()
                .map(|node| node.node_id.as_str())
                .collect::<Vec<_>>(),
            vec!["one", "two"]
        );
        assert!(nodes.iter().all(|node| node.status == "succeeded"));
        assert_eq!(nodes[0].input, json!({"seed": 1}));
        assert_eq!(nodes[0].output, Some(json!({"seed": 1})));
        assert_eq!(nodes[1].input, json!({"seed": 1}));
        assert_eq!(nodes[1].output, Some(json!({"seed": 1})));
    }

    /// A code node runs against the run itself: what the trigger carried and what the node before it output are in
    /// scope for its Luau, its source is filled from expressions like any parameter, and the output the run stores for
    /// it is the value the next node was handed.
    #[test]
    fn a_code_node_reads_the_run_and_its_output_is_what_the_next_node_is_handed() {
        let (_dir, app, repository) = setup();
        let workflow = definition(
            r#"
name = "reshape"
description = "Reshape the value in Luau."

[[trigger]]
kind = "test"

[[nodes]]
id = "first"
kind = "test"

[[nodes]]
id = "shape"
kind = "code"
params = { source = "return { repository = '{{ trigger.repository }}', twice = {{ node('first').n * 2 }} }" }

[[nodes]]
id = "second"
kind = "code"
params = { source = "return { seen = input.twice, here = node('shape').repository }" }

[[edges]]
from = "trigger"
to = "first"
port = "out"

[[edges]]
from = "first"
to = "shape"
port = "out"

[[edges]]
from = "shape"
to = "second"
port = "out"
"#,
        );

        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run(
                workflow,
                repository.id,
                json!({"repository": "moebius", "n": 1}),
            )
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);
        assert_eq!(ended.status, "succeeded", "{:?}", ended.error);

        let nodes = app.store.workflow_node_runs(run.id).unwrap();
        let reshaped = json!({"repository": "moebius", "twice": 2});
        // The code read the trigger and the node before it.
        assert_eq!(nodes[1].output, Some(reshaped.clone()));
        // What the run stores as the code node's output is the value the edge carried to the next node.
        assert_eq!(nodes[2].input, reshaped);
        assert_eq!(nodes[2].output, Some(json!({"seen": 2, "here": "moebius"})));
    }

    #[test]
    fn a_failing_node_ends_the_run_failed_and_records_its_error() {
        let (_dir, app, repository) = setup();
        write_workflow(
            &app,
            "broken",
            r#"
name = "broken"
description = "A node that fails."

[[trigger]]
kind = "test"

[[nodes]]
id = "one"
kind = "test"
params = { fail = "one is broken" }

[[nodes]]
id = "two"
kind = "test"

[[edges]]
from = "trigger"
port = "out"
to = "one"

[[edges]]
from = "one"
port = "out"
to = "two"
"#,
        );

        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run("broken", repository.id, json!(1))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);

        assert_eq!(ended.status, "failed");
        assert_eq!(ended.error.as_deref(), Some("one is broken"));
        let nodes = app.store.workflow_node_runs(run.id).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_id, "one");
        assert_eq!(nodes[0].status, "failed");
        assert_eq!(nodes[0].error.as_deref(), Some("one is broken"));
    }

    #[test]
    fn a_node_that_times_out_ends_the_run_failed() {
        let (_dir, app, repository) = setup();
        write_workflow(
            &app,
            "slow",
            r#"
name = "slow"
description = "A node that is given no time at all."
node_timeout_seconds = 0

[[trigger]]
kind = "test"

[[nodes]]
id = "one"
kind = "test"
params = { delay_ms = 100 }

[[edges]]
from = "trigger"
port = "out"
to = "one"
"#,
        );

        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run("slow", repository.id, json!(1))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);

        assert_eq!(ended.status, "failed");
        assert!(ended.error.unwrap().contains("timed out"));
        let nodes = app.store.workflow_node_runs(run.id).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].status, "failed");
    }

    #[test]
    fn a_definition_with_an_unresolved_edge_cannot_start() {
        let (_dir, app, repository) = setup();
        let broken = definition(
            r#"
name = "broken"
description = "An edge that goes nowhere."

[[trigger]]
kind = "test"

[[nodes]]
id = "one"
kind = "test"

[[edges]]
from = "trigger"
port = "out"
to = "one"

[[edges]]
from = "one"
port = "out"
to = "missing"
"#,
        );

        let error = app
            .start_workflow_run(broken, repository.id, json!(1))
            .unwrap_err();

        assert!(error.to_string().contains("missing"), "{error}");
    }

    #[test]
    fn a_name_reads_the_stored_definition() {
        let (_dir, app, repository) = setup();
        write_workflow(
            &app,
            "stored",
            r#"
name = "stored"
description = "Read from the harness configuration repository."

[[trigger]]
kind = "test"

[[nodes]]
id = "one"
kind = "test"

[[edges]]
from = "trigger"
port = "out"
to = "one"
"#,
        );

        let mut events = app.bus.subscribe();
        let run = app
            .start_workflow_run("stored", repository.id, json!({"a": true}))
            .unwrap();
        let ended = wait_for_end(&app, &mut events, run.id);

        assert_eq!(ended.status, "succeeded");
        assert_eq!(ended.workflow, "stored");
        assert_eq!(ended.trigger, "test");
    }
}
