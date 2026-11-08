//! The executor of a workflow run. It follows the edges from the trigger, giving each node the value its predecessor
//! left and taking the one value the node returns. A node that fails, a node that takes longer than its timeout, or a
//! run that reaches its node ceiling fails the whole run closed; the edge from the node that just ran decides whether
//! there is more work or the run is done.

use std::sync::Arc;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use thiserror::Error;

use super::definition::{NodeDefinition, TRIGGER_ID, WorkflowDefinition};
use super::nodes::NodeError;

/// Why a run ended `failed`, or could not start.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum Error {
    /// The definition cannot be followed; no node ran.
    #[error("{0}")]
    Definition(String),
    /// An edge leads to a node the definition does not have.
    #[error("the edge from \"{from}\" names unknown node \"{to}\"")]
    UnknownNode { from: String, to: String },
    /// The run was about to execute more nodes than its ceiling allows.
    #[error("the run reached its ceiling of {0} nodes")]
    Ceiling(u64),
    /// A node ran longer than its timeout.
    #[error("node \"{node}\" timed out after {seconds} seconds")]
    TimedOut { node: String, seconds: f64 },
    /// A node's thread ended without handing back a value.
    #[error("node \"{0}\" ended without a value")]
    NoValue(String),
    /// The thread for a node could not be started.
    #[error("cannot start node \"{node}\": {reason}")]
    Spawn { node: String, reason: String },
    /// The node itself failed.
    #[error("{0}")]
    Node(#[from] NodeError),
    /// An observer could not record a step of the run.
    #[error("cannot record node \"{node}\": {reason}")]
    Record { node: String, reason: String },
}

/// What one node did with the value it was given.
#[derive(Debug, Clone, PartialEq)]
pub enum StepResult {
    /// The node returned this value for the next edge to carry.
    Succeeded(Value),
    /// The node failed with this error, which ends the run.
    Failed(Error),
}

/// The limits one run obeys, taken from its definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How long one node may run before the run fails.
    pub node_timeout: Duration,
    /// How many nodes the run may execute before it fails.
    pub max_nodes: u64,
}

impl Limits {
    /// The limits written in `definition`, or their defaults.
    pub fn from_definition(definition: &WorkflowDefinition) -> Self {
        Self {
            node_timeout: Duration::from_secs(definition.node_timeout_seconds),
            max_nodes: definition.max_nodes,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            node_timeout: Duration::from_secs(super::definition::DEFAULT_NODE_TIMEOUT_SECONDS),
            max_nodes: super::definition::DEFAULT_MAX_NODES,
        }
    }
}

/// Runs one node, given the node as its file wrote it and the value its predecessor left.
pub trait Runner: Send + Sync {
    fn run(&self, node: &NodeDefinition, input: &Value) -> Result<Value, NodeError>;
}

impl<F> Runner for F
where
    F: Fn(&NodeDefinition, &Value) -> Result<Value, NodeError> + Send + Sync,
{
    fn run(&self, node: &NodeDefinition, input: &Value) -> Result<Value, NodeError> {
        self(node, input)
    }
}

/// Watches a run as it happens, so its nodes are recorded before the next one starts.
pub trait Observer {
    /// A node is about to run with `input`.
    fn started(&mut self, node: &NodeDefinition, input: &Value) -> Result<(), Error>;
    /// A node has finished with `result`, which the run records before it goes on or ends.
    fn finished(&mut self, node: &NodeDefinition, result: &StepResult) -> Result<(), Error>;
}

impl Observer for () {
    fn started(&mut self, _node: &NodeDefinition, _input: &Value) -> Result<(), Error> {
        Ok(())
    }

    fn finished(&mut self, _node: &NodeDefinition, _result: &StepResult) -> Result<(), Error> {
        Ok(())
    }
}

/// Follows `definition` from its trigger with `start`, recording every node through `observer`, and returns the value
/// left by the last node. An unreadable graph fails before any node runs.
pub fn execute(
    definition: &WorkflowDefinition,
    start: Value,
    limits: Limits,
    runner: &Arc<dyn Runner>,
    observer: &mut dyn Observer,
) -> Result<Value, Error> {
    definition
        .validate(&format!("workflows/{}.toml", definition.name))
        .map_err(|error| Error::Definition(error.to_string()))?;

    let mut from = TRIGGER_ID.to_string();
    let mut value = start;
    let mut ran = 0_u64;

    loop {
        let Some(edge) = definition.edges.iter().find(|edge| edge.from == from) else {
            return Ok(value);
        };
        let Some(node) = definition.nodes.iter().find(|node| node.id == edge.to) else {
            return Err(Error::UnknownNode {
                from,
                to: edge.to.clone(),
            });
        };
        if ran >= limits.max_nodes {
            return Err(Error::Ceiling(limits.max_nodes));
        }
        ran += 1;

        observer.started(node, &value)?;
        let result = run_one(runner, node, &value, limits.node_timeout);
        observer.finished(node, &result)?;

        match result {
            StepResult::Succeeded(output) => {
                value = output;
                from = node.id.clone();
            }
            StepResult::Failed(error) => return Err(error),
        }
    }
}

/// Runs one node on a thread of its own, and gives up on it after `timeout`.
fn run_one(
    runner: &Arc<dyn Runner>,
    node: &NodeDefinition,
    input: &Value,
    timeout: Duration,
) -> StepResult {
    let (sender, receiver) = channel();
    let runner = Arc::clone(runner);
    let running = node.clone();
    let given = input.clone();
    let spawned = thread::Builder::new()
        .name(format!("workflow node {}", node.id))
        .spawn(move || {
            let _ = sender.send(runner.run(&running, &given));
        });
    if let Err(error) = spawned {
        return StepResult::Failed(Error::Spawn {
            node: node.id.clone(),
            reason: error.to_string(),
        });
    }

    match receiver.recv_timeout(timeout) {
        Ok(Ok(output)) => StepResult::Succeeded(output),
        Ok(Err(error)) => StepResult::Failed(Error::Node(error)),
        Err(RecvTimeoutError::Timeout) => StepResult::Failed(Error::TimedOut {
            node: node.id.clone(),
            seconds: timeout.as_secs_f64(),
        }),
        Err(RecvTimeoutError::Disconnected) => StepResult::Failed(Error::NoValue(node.id.clone())),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use serde_json::{Value, json};

    use super::*;

    fn definition(text: &str) -> WorkflowDefinition {
        WorkflowDefinition::parse(text, "workflows/test.toml").unwrap()
    }

    fn chain() -> WorkflowDefinition {
        definition(
            r#"
name = "test"
description = "Run three nodes in a row."

[[trigger]]
kind = "test"

[[nodes]]
id = "one"
kind = "test"

[[nodes]]
id = "two"
kind = "test"

[[nodes]]
id = "three"
kind = "test"

[[edges]]
from = "trigger"
port = "out"
to = "one"

[[edges]]
from = "one"
port = "out"
to = "two"

[[edges]]
from = "two"
port = "out"
to = "three"
"#,
        )
    }

    #[derive(Default)]
    struct Seen {
        started: Mutex<Vec<String>>,
        finished: Mutex<Vec<(String, StepResult)>>,
    }

    impl Observer for Seen {
        fn started(&mut self, node: &NodeDefinition, _input: &Value) -> Result<(), Error> {
            self.started.lock().unwrap().push(node.id.clone());
            Ok(())
        }

        fn finished(&mut self, node: &NodeDefinition, result: &StepResult) -> Result<(), Error> {
            self.finished
                .lock()
                .unwrap()
                .push((node.id.clone(), result.clone()));
            Ok(())
        }
    }

    #[test]
    fn limits_come_from_the_definition_with_defaults() {
        let defaults = Limits::from_definition(&chain());
        assert_eq!(defaults.node_timeout, Duration::from_secs(300));
        assert_eq!(defaults.max_nodes, 100);

        let written = definition(
            r#"
name = "limits"
description = "Limits written in the file."
node_timeout_seconds = 7
max_nodes = 3

[[trigger]]
kind = "test"
"#,
        );
        let limits = Limits::from_definition(&written);
        assert_eq!(limits.node_timeout, Duration::from_secs(7));
        assert_eq!(limits.max_nodes, 3);
    }

    #[test]
    fn values_arrive_in_order_and_are_recorded() {
        let definition = chain();
        let runner: Arc<dyn Runner> = Arc::new(|node: &NodeDefinition, input: &Value| {
            let mut output = input.clone();
            output[&node.id] = json!(true);
            Ok(output)
        });
        let mut seen = Seen::default();

        let value = execute(
            &definition,
            json!({"started": true}),
            Limits::default(),
            &runner,
            &mut seen,
        )
        .unwrap();

        assert_eq!(value["started"], json!(true));
        assert_eq!(value["one"], json!(true));
        assert_eq!(value["two"], json!(true));
        assert_eq!(value["three"], json!(true));
        assert_eq!(*seen.started.lock().unwrap(), vec!["one", "two", "three"]);
        let finished = seen.finished.lock().unwrap();
        assert_eq!(finished.len(), 3);
        assert_eq!(
            finished[0].1,
            StepResult::Succeeded(json!({"started": true, "one": true}))
        );
    }

    #[test]
    fn a_failing_node_stops_the_run() {
        let definition = chain();
        let runner: Arc<dyn Runner> = Arc::new(|node: &NodeDefinition, input: &Value| {
            if node.id == "two" {
                Err(NodeError::Failed("two is broken".into()))
            } else {
                Ok(input.clone())
            }
        });
        let mut seen = Seen::default();

        let error =
            execute(&definition, json!(1), Limits::default(), &runner, &mut seen).unwrap_err();

        assert_eq!(
            error,
            Error::Node(NodeError::Failed("two is broken".into()))
        );
        assert_eq!(*seen.started.lock().unwrap(), vec!["one", "two"]);
    }

    #[test]
    fn a_node_that_runs_past_its_timeout_fails_the_run() {
        let definition = chain();
        // The first node waits until the test lets it go, so it cannot finish inside its timeout
        // however fast the machine is.
        let (release, released) = std::sync::mpsc::channel::<()>();
        let released = std::sync::Mutex::new(released);
        let runner: Arc<dyn Runner> = Arc::new(move |_node: &NodeDefinition, input: &Value| {
            let _ = released.lock().unwrap().recv();
            Ok(input.clone())
        });
        let limits = Limits {
            node_timeout: Duration::from_millis(20),
            max_nodes: 10,
        };

        let error = execute(&definition, json!(1), limits, &runner, &mut ()).unwrap_err();
        drop(release);

        assert!(matches!(error, Error::TimedOut { ref node, .. } if node == "one"));
    }

    #[test]
    fn a_run_that_reaches_its_ceiling_fails() {
        let definition = chain();
        let runner: Arc<dyn Runner> =
            Arc::new(|_node: &NodeDefinition, input: &Value| Ok(input.clone()));
        let limits = Limits {
            node_timeout: Duration::from_secs(1),
            max_nodes: 2,
        };

        let error = execute(&definition, json!(1), limits, &runner, &mut ()).unwrap_err();

        assert_eq!(error, Error::Ceiling(2));
    }

    #[test]
    fn a_definition_with_an_unresolved_edge_does_not_start() {
        let definition = definition(
            r#"
name = "test"
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
        let runner: Arc<dyn Runner> =
            Arc::new(|_node: &NodeDefinition, input: &Value| Ok(input.clone()));

        let error =
            execute(&definition, json!(1), Limits::default(), &runner, &mut ()).unwrap_err();

        assert!(error.to_string().contains("missing"), "{error}");
    }
}
