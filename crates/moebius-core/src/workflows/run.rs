//! The executor of a workflow run. It follows the edges from the trigger, giving each node the value its predecessor
//! left and taking the one value the node returns, and it follows the edge that leaves by the port the node names. A
//! node that fails, a node that takes longer than its timeout, or a run that reaches its node ceiling fails the whole
//! run closed; the edge from the node that just ran decides whether there is more work or the run is done. A person's
//! cancellation ends the run `cancelled` at the node that is going.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use thiserror::Error;

use super::decorate::Scope;
use super::definition::{NodeDefinition, TRIGGER_ID, WorkflowDefinition};
use super::nodes::{NodeError, Routed};

/// Why a run ended `failed` or `cancelled`, or could not start.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum Error {
    /// The definition cannot be followed; no node ran.
    #[error("{0}")]
    Definition(String),
    /// A person cancelled the run; the node going was given up and no further node starts.
    #[error("the run was cancelled")]
    Cancelled,
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
    /// A node named an output port with no edge, so its value has nowhere to go.
    #[error("node \"{node}\" named port \"{port}\", which has no edge leaving by it")]
    NoEdge {
        /// The node that named the port.
        node: String,
        /// The port it named.
        port: String,
    },
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

    /// Runs one node with what an expression may read, and returns the port it chose, if it chose one. A runner whose
    /// nodes never choose needs only [`Runner::run`].
    fn run_routed(&self, node: &NodeDefinition, scope: &Scope) -> Result<Routed, NodeError> {
        Ok(Routed {
            value: self.run(node, &scope.input)?,
            port: None,
        })
    }

    /// How long this node may run before the run fails it: a node that waits for something the run itself arranges
    /// says for how long it waits, and the rest get the run's own node timeout.
    fn timeout(&self, _node: &NodeDefinition) -> Option<Duration> {
        None
    }
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
/// left by the last node. An unreadable graph fails before any node runs. While `cancellation` is set, no node starts
/// and the node going is given up, so a cancelled run stops at its current node.
pub fn execute(
    definition: &WorkflowDefinition,
    start: Value,
    limits: Limits,
    runner: &Arc<dyn Runner>,
    observer: &mut dyn Observer,
    cancellation: &AtomicBool,
) -> Result<Value, Error> {
    definition
        .validate(&format!("workflows/{}.toml", definition.name))
        .map_err(|error| Error::Definition(error.to_string()))?;
    let trigger = start.clone();
    walk(
        definition,
        TRIGGER_ID.to_string(),
        trigger,
        start,
        None,
        BTreeMap::new(),
        0,
        limits,
        runner,
        observer,
        cancellation,
    )
}

/// Where a run a stopped server left waiting starts again: the node that waited, the run's trigger, the value that
/// reached the node, and the values the nodes before it left.
pub struct Resume {
    pub node: String,
    pub trigger: Value,
    pub input: Value,
    pub earlier: BTreeMap<String, Value>,
}

/// Runs `definition` again from `resume`'s node, where a server restart left a run waiting: the node runs again with
/// the value that reached it, while expressions see the run's trigger and the values the nodes before it left, and
/// the run follows from it, so no node before it runs a second time. A cancellation set before it starts means no node
/// runs. An unreadable graph fails before any node runs.
pub fn execute_from_node(
    definition: &WorkflowDefinition,
    resume: Resume,
    limits: Limits,
    runner: &Arc<dyn Runner>,
    observer: &mut dyn Observer,
    cancellation: &AtomicBool,
) -> Result<Value, Error> {
    let Resume {
        node: node_id,
        trigger,
        input: start,
        earlier,
    } = resume;
    definition
        .validate(&format!("workflows/{}.toml", definition.name))
        .map_err(|error| Error::Definition(error.to_string()))?;
    let node = definition
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| Error::UnknownNode {
            from: TRIGGER_ID.to_string(),
            to: node_id.to_string(),
        })?;
    if limits.max_nodes == 0 {
        return Err(Error::Ceiling(limits.max_nodes));
    }
    let mut earlier = earlier;
    if cancellation.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    let (value, port) = step(
        runner,
        node,
        &trigger,
        &start,
        &earlier,
        limits,
        observer,
        cancellation,
    )?;
    earlier.insert(node.id.clone(), value.clone());
    walk(
        definition,
        node.id.clone(),
        trigger,
        value,
        port,
        earlier,
        1,
        limits,
        runner,
        observer,
        cancellation,
    )
}

/// Follows the edges from `from`, running each node it reaches with the value the one before left.
#[allow(clippy::too_many_arguments)]
fn walk(
    definition: &WorkflowDefinition,
    from: String,
    trigger: Value,
    start: Value,
    port: Option<String>,
    earlier: BTreeMap<String, Value>,
    ran: u64,
    limits: Limits,
    runner: &Arc<dyn Runner>,
    observer: &mut dyn Observer,
    cancellation: &AtomicBool,
) -> Result<Value, Error> {
    let mut from = from;
    let mut value = start;
    let mut port = port;
    let mut earlier = earlier;
    let mut ran = ran;

    loop {
        if cancellation.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        // The next node is the one the edge out of this one leads to; a routing node chooses which of its output
        // ports that edge leaves by. A named port with no edge is a choice that has nowhere to go, so the run fails
        // closed rather than ending with it.
        let Some(edge) = definition
            .edges
            .iter()
            .find(|edge| edge.from == from && port.as_ref().is_none_or(|port| &edge.port == port))
        else {
            return match port {
                None => Ok(value),
                Some(port) => Err(Error::NoEdge { node: from, port }),
            };
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

        let (output, chosen) = step(
            runner,
            node,
            &trigger,
            &value,
            &earlier,
            limits,
            observer,
            cancellation,
        )?;
        earlier.insert(node.id.clone(), output.clone());
        value = output;
        port = chosen;
        from = node.id.clone();
    }
}

/// Runs one node, telling the observer before and after it, giving it its own timeout if it named one or the run's
/// node timeout for the rest, and gives back the value it left and the port it chose, or the failure that ends the
/// run.
#[allow(clippy::too_many_arguments)]
fn step(
    runner: &Arc<dyn Runner>,
    node: &NodeDefinition,
    trigger: &Value,
    input: &Value,
    earlier: &BTreeMap<String, Value>,
    limits: Limits,
    observer: &mut dyn Observer,
    cancellation: &AtomicBool,
) -> Result<(Value, Option<String>), Error> {
    observer.started(node, input)?;
    let scope = Scope {
        trigger: trigger.clone(),
        input: input.clone(),
        nodes: earlier.clone(),
    };
    let timeout = runner.timeout(node).unwrap_or(limits.node_timeout);
    let (result, port) = run_one(runner, node, scope, timeout, cancellation);
    observer.finished(node, &result)?;
    match result {
        StepResult::Succeeded(output) => Ok((output, port)),
        StepResult::Failed(error) => Err(error),
    }
}

/// How often a going node is looked at for a cancellation, while its timeout is counted down.
const CANCEL_POLL: Duration = Duration::from_millis(20);

/// Runs one node on a thread of its own, and gives up on it after `timeout` — or sooner, when `cancellation` is set,
/// the way a timed-out node is given up: the node's thread is left and the run goes on without it. Along with the
/// result it returns the output port the node chose, if it chose one.
fn run_one(
    runner: &Arc<dyn Runner>,
    node: &NodeDefinition,
    scope: Scope,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> (StepResult, Option<String>) {
    let (sender, receiver) = channel();
    let runner = Arc::clone(runner);
    let running = node.clone();
    let spawned = thread::Builder::new()
        .name(format!("workflow node {}", node.id))
        .spawn(move || {
            let _ = sender.send(runner.run_routed(&running, &scope));
        });
    if let Err(error) = spawned {
        let failed = Error::Spawn {
            node: node.id.clone(),
            reason: error.to_string(),
        };
        return (StepResult::Failed(failed), None);
    }

    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            let failed = Error::TimedOut {
                node: node.id.clone(),
                seconds: timeout.as_secs_f64(),
            };
            return (StepResult::Failed(failed), None);
        }
        let look = (deadline - now).min(CANCEL_POLL);
        match receiver.recv_timeout(look) {
            Ok(Ok(Routed { value, port })) => return (StepResult::Succeeded(value), port),
            Ok(Err(NodeError::Cancelled)) => return (StepResult::Failed(Error::Cancelled), None),
            Ok(Err(error)) => return (StepResult::Failed(Error::Node(error)), None),
            Err(RecvTimeoutError::Timeout) => {
                if cancellation.load(Ordering::Relaxed) {
                    return (StepResult::Failed(Error::Cancelled), None);
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return (StepResult::Failed(Error::NoValue(node.id.clone())), None);
            }
        }
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
            &std::sync::atomic::AtomicBool::new(false),
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
    fn a_run_driven_from_a_node_does_not_run_the_nodes_before_it() {
        let definition = chain();
        let runner: Arc<dyn Runner> = Arc::new(|node: &NodeDefinition, input: &Value| {
            let mut output = input.clone();
            output[&node.id] = json!(true);
            Ok(output)
        });
        let mut seen = Seen::default();

        let value = execute_from_node(
            &definition,
            Resume {
                node: "two".into(),
                trigger: json!({"trigger": true}),
                input: json!({"waited": true}),
                earlier: BTreeMap::new(),
            },
            Limits::default(),
            &runner,
            &mut seen,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(*seen.started.lock().unwrap(), vec!["two", "three"]);
        assert_eq!(value["waited"], json!(true));
        assert_eq!(value["two"], json!(true));
        assert_eq!(value["three"], json!(true));
        assert!(value.get("one").is_none(), "no node before the wait ran");
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

        let error = execute(
            &definition,
            json!(1),
            Limits::default(),
            &runner,
            &mut seen,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap_err();

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

        let error = execute(
            &definition,
            json!(1),
            limits,
            &runner,
            &mut (),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap_err();
        drop(release);

        assert!(matches!(error, Error::TimedOut { ref node, .. } if node == "one"));
    }

    #[test]
    fn a_node_that_names_a_port_with_no_edge_fails_the_run() {
        struct NamingPort;
        impl Runner for NamingPort {
            fn run(&self, _node: &NodeDefinition, input: &Value) -> Result<Value, NodeError> {
                Ok(input.clone())
            }
            fn run_routed(
                &self,
                node: &NodeDefinition,
                scope: &Scope,
            ) -> Result<Routed, NodeError> {
                let port = (node.id == "one").then_some("nowhere".to_string());
                Ok(Routed {
                    value: scope.input.clone(),
                    port,
                })
            }
        }
        let definition = chain();
        let runner: Arc<dyn Runner> = Arc::new(NamingPort);

        let error = execute(
            &definition,
            json!(1),
            Limits::default(),
            &runner,
            &mut (),
            &AtomicBool::new(false),
        )
        .unwrap_err();

        assert!(
            matches!(error, Error::NoEdge { ref node, ref port } if node == "one" && port == "nowhere"),
            "got {error:?}"
        );
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

        let error = execute(
            &definition,
            json!(1),
            limits,
            &runner,
            &mut (),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap_err();

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

        let error = execute(
            &definition,
            json!(1),
            Limits::default(),
            &runner,
            &mut (),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap_err();

        assert!(error.to_string().contains("missing"), "{error}");
    }

    #[test]
    fn a_cancellation_before_the_run_starts_nothing() {
        let definition = chain();
        let ran = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counting = ran.clone();
        let runner: Arc<dyn Runner> = Arc::new(move |_node: &NodeDefinition, input: &Value| {
            counting.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(input.clone())
        });
        let mut seen = Seen::default();
        let cancelled = std::sync::atomic::AtomicBool::new(true);

        let error = execute(
            &definition,
            json!(1),
            Limits::default(),
            &runner,
            &mut seen,
            &cancelled,
        )
        .unwrap_err();

        assert_eq!(error, Error::Cancelled);
        assert_eq!(*seen.started.lock().unwrap(), [] as [String; 0]);
        assert_eq!(ran.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn a_cancellation_before_a_run_driven_from_a_node_starts_nothing() {
        let definition = chain();
        let ran = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counting = ran.clone();
        let runner: Arc<dyn Runner> = Arc::new(move |_node: &NodeDefinition, input: &Value| {
            counting.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(input.clone())
        });
        let mut seen = Seen::default();
        let cancelled = std::sync::atomic::AtomicBool::new(true);

        let error = execute_from_node(
            &definition,
            Resume {
                node: "two".into(),
                trigger: json!({"trigger": true}),
                input: json!({"waited": true}),
                earlier: BTreeMap::new(),
            },
            Limits::default(),
            &runner,
            &mut seen,
            &cancelled,
        )
        .unwrap_err();

        assert_eq!(error, Error::Cancelled);
        assert_eq!(*seen.started.lock().unwrap(), [] as [String; 0]);
        assert_eq!(ran.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn a_cancellation_stops_the_run_at_its_current_node_and_starts_nothing_after() {
        let definition = chain();
        let cancellation = Arc::new(std::sync::atomic::AtomicBool::new(false));
        // The first node cancels the run, as a person would while it is going, and would then have run on for a
        // long time.
        let going = cancellation.clone();
        let runner: Arc<dyn Runner> = Arc::new(move |_node: &NodeDefinition, input: &Value| {
            going.store(true, std::sync::atomic::Ordering::Relaxed);
            thread::sleep(Duration::from_secs(30));
            Ok(input.clone())
        });
        let mut seen = Seen::default();

        let error = execute(
            &definition,
            json!(1),
            Limits::default(),
            &runner,
            &mut seen,
            &cancellation,
        )
        .unwrap_err();

        assert_eq!(error, Error::Cancelled);
        assert_eq!(*seen.started.lock().unwrap(), vec!["one"]);
        let finished = seen.finished.lock().unwrap();
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].0, "one");
        assert_eq!(finished[0].1, StepResult::Failed(Error::Cancelled));
    }
}
