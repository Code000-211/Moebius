//! The node kinds a workflow can name. A node does one piece of work when the engine runs it, says which ports a value
//! may leave it by, and describes itself for the pages that list the kinds. The registry below is the one place a new
//! node kind is added: one arm of [`node`], next to the others.

use serde_json::Value;
use thiserror::Error;

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
    #[error("{0}")]
    Failed(String),
}

/// One piece of work in a workflow.
pub trait Node: Send + Sync {
    /// Do the node's work on the value that reached it and return the value that leaves by its output port.
    fn run(&self, input: Value) -> Result<Value, NodeError>;

    /// The ports a value enters and leaves by, whose names the workflow's edges use.
    fn ports(&self) -> Ports;

    /// The kind name, title and description shown to people.
    fn metadata(&self) -> Metadata;
}

/// Build the node a workflow names.
///
/// Each node-kind card adds one arm to the match below and constructs its own node from `params`. Until a kind's card
/// lands, naming it is an [`NodeError::UnknownKind`].
pub fn node(kind: &str, _params: &Value) -> Result<Box<dyn Node>, NodeError> {
    match kind {
        #[cfg(test)]
        TEST_KIND => Ok(Box::new(TestNode {
            params: _params.clone(),
        })),
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
        if self.params.get("fail").and_then(Value::as_bool) == Some(true) {
            return Err(NodeError::Failed("the test node failed".into()));
        }
        if let Some(millis) = self
            .params
            .get("delay_ms")
            .or_else(|| self.params.get("sleep_ms"))
            .and_then(Value::as_u64)
        {
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
