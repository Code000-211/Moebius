//! The node kinds a workflow can name. A node does one piece of work when the engine runs it, says which ports a value
//! may leave it by, and describes itself for the pages that list the kinds. The registry below is the one place a new
//! node kind is added: one arm of [`node`], next to the others.

pub mod agent;
pub mod approval;
pub mod chat;
pub mod code;
pub mod condition;
pub mod for_each;
pub mod http;
pub mod merge;
pub mod publish;

use serde_json::Value;
use thiserror::Error;

use super::decorate::Scope;

/// The named ways a value enters and leaves a node.
///
/// An edge's `port` names one of the source node's outputs, so an executor can follow the edge that the value it made
/// belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ports {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

impl Ports {
    /// Ports with the given inputs and outputs.
    pub fn new(
        inputs: impl IntoIterator<Item = impl Into<String>>,
        outputs: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            inputs: inputs.into_iter().map(Into::into).collect(),
            outputs: outputs.into_iter().map(Into::into).collect(),
        }
    }
}

/// What a node kind says about itself in the reference and in the web app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// The kind name a workflow file writes.
    pub kind: &'static str,
    /// A short name for people.
    pub title: &'static str,
    /// One sentence on what the node does.
    pub description: &'static str,
}

/// Why a node could not be built or run.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NodeError {
    #[error("unknown node kind \"{0}\"")]
    UnknownKind(String),
    #[error("invalid parameters for node kind \"{kind}\": {message}")]
    Parameters { kind: String, message: String },
    /// A person cancelled the run while the node waited; the node leaves what it was doing.
    #[error("the run was cancelled")]
    Cancelled,
    #[error("{0}")]
    Failed(String),
}

/// What a node made and, when it chooses, the output port the value leaves by.
#[derive(Debug, Clone, PartialEq)]
pub struct Routed {
    pub value: Value,
    /// The output port to follow. `None` leaves the choice to the executor, which follows the first edge out of the node.
    pub port: Option<String>,
}

/// One piece of work in a workflow.
pub trait Node: Send + Sync {
    /// Do the node's work on the value that reached it and return the value that leaves by its output port.
    fn run(&self, input: Value) -> Result<Value, NodeError>;

    /// Do the node's work as the executor asks it to: with the trigger's value and the earlier nodes' values in
    /// `scope`, and with the choice of an output port. A node that does not choose a port needs only [`Node::run`].
    fn run_routed(&self, scope: &Scope) -> Result<Routed, NodeError> {
        Ok(Routed {
            value: self.run(scope.input.clone())?,
            port: None,
        })
    }

    /// The ports a value enters and leaves by, whose names the workflow's edges use.
    fn ports(&self) -> Ports;

    /// `true` when the node chooses which output port the value leaves by; such a node needs an edge leaving by every
    /// port it reports, which the workflow's validation refuses to let go missing.
    fn routes(&self) -> bool {
        false
    }

    /// The kind name, title and description shown to people.
    fn metadata(&self) -> Metadata;
}

/// Build the node a workflow names.
///
/// Each node-kind card adds one arm to the match below and constructs its own node from `params`. Until a kind's card
/// lands, naming it is an [`NodeError::UnknownKind`].
pub fn node(kind: &str, params: &Value) -> Result<Box<dyn Node>, NodeError> {
    match kind {
        chat::KIND => Ok(Box::new(chat::Chat::from_params(params)?)),
        #[cfg(test)]
        TEST_KIND => Ok(Box::new(TestNode {
            params: params.clone(),
        })),
        agent::KIND => Ok(Box::new(agent::Agent::from_params(params)?)),
        code::KIND => Ok(Box::new(code::CodeNode::from_params(params)?)),
        condition::KIND => Ok(Box::new(condition::Condition::new(params)?)),
        for_each::KIND => Ok(Box::new(for_each::ForEach::from_params(params)?)),
        http::KIND => Ok(Box::new(http::HttpNode::from_params(params)?)),
        merge::KIND => Ok(Box::new(merge::Merge::from_params(params)?)),
        publish::KIND => Ok(Box::new(publish::Publish::from_params(params)?)),
        approval::KIND => Ok(Box::new(approval::Approval::from_params(params)?)),
        other => Err(NodeError::UnknownKind(other.to_string())),
    }
}

/// A node kind that exists only in tests, so validation can be exercised before any real kind lands.
#[cfg(test)]
pub(crate) const TEST_KIND: &str = "test";

#[cfg(test)]
pub(crate) mod test_gate {
    use std::collections::HashMap;
    use std::sync::{Arc, Condvar, Mutex, OnceLock};

    type Gate = Arc<(Mutex<bool>, Condvar)>;

    fn gates() -> &'static Mutex<HashMap<String, Gate>> {
        static GATES: OnceLock<Mutex<HashMap<String, Gate>>> = OnceLock::new();
        GATES.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// The gate called `name`, which a test node waits on until it is released.
    pub(crate) fn gate(name: &str) -> Gate {
        gates()
            .lock()
            .unwrap()
            .entry(name.to_string())
            .or_default()
            .clone()
    }

    /// Opens the gate called `name`, letting a node waiting on it go on.
    pub(crate) fn release(name: &str) {
        let gate = gate(name);
        let (open, wake) = &*gate;
        *open.lock().unwrap() = true;
        wake.notify_all();
    }
}

#[cfg(test)]
struct TestNode {
    params: Value,
}

#[cfg(test)]
impl Node for TestNode {
    fn run(&self, input: Value) -> Result<Value, NodeError> {
        if let Some(message) = self.params.get("fail").and_then(Value::as_str) {
            return Err(NodeError::Failed(message.to_string()));
        }
        if let Some(millis) = self.params.get("delay_ms").and_then(Value::as_u64) {
            std::thread::sleep(std::time::Duration::from_millis(millis));
        }
        if let Some(name) = self.params.get("wait_for_gate").and_then(Value::as_str) {
            let gate = test_gate::gate(name);
            let (open, wake) = &*gate;
            let mut open = open.lock().unwrap();
            while !*open {
                open = wake.wait(open).unwrap();
            }
        }
        Ok(input)
    }

    fn ports(&self) -> Ports {
        Ports::new(["in"], ["out", "failed"])
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            kind: TEST_KIND,
            title: "Test",
            description: "A node that exists only in tests.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_unknown_kind_is_refused() {
        let error = match node("missing", &json!({})) {
            Ok(_) => panic!("an unknown kind built a node"),
            Err(error) => error,
        };
        assert_eq!(error.to_string(), "unknown node kind \"missing\"");
    }

    #[test]
    fn a_test_node_has_ports_and_metadata() {
        let node = node(TEST_KIND, &json!({})).unwrap();
        assert_eq!(node.ports().outputs, ["out", "failed"]);
        assert_eq!(node.metadata().kind, TEST_KIND);
        assert_eq!(node.run(json!(1)).unwrap(), json!(1));
    }
}
