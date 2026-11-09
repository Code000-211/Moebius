//! The agent node: the node kind that runs an agent of the harness configuration. The first agent node of a workflow
//! run checks the run's repository out into a worktree on a branch named for the run, and the run keeps the worktree
//! path with itself, so the run's later agent nodes reuse the worktree and the branch; each agent is started through
//! the harness picker, failing closed, and its transcript is kept in its run's log. The node's value is the fenced
//! JSON block the prompt asks the agent to end with, which leaves by the declared `output` port: it is parsed, and a
//! missing or malformed block fails the node closed.

use serde_json::Value;

use super::{Metadata, Node, NodeError, Ports};

mod run;
mod value;

/// The kind of the node.
pub const KIND: &str = "agent";

/// What the node adds to its prompt: the value of the node is the fenced JSON block the agent ends with.
const OUTPUT_REQUEST: &str = "End your final message with a fenced JSON block containing the value of the \
node's declared output; it is the value the node takes from you.";

/// The agent node: `agent` names an agent of the harness configuration, `prompt` is built from parameters and
/// expressions, and the node's value, the fenced JSON block the agent ends with, leaves by the declared `output` port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    agent: String,
    prompt: String,
    output: String,
}

impl Agent {
    /// The node from its parameters: `agent`, `prompt` and the declared `output`, each a text of its own.
    pub fn from_params(params: &Value) -> Result<Self, NodeError> {
        let invalid = |message: &str| NodeError::Parameters {
            kind: KIND.to_string(),
            message: message.to_string(),
        };
        let parameter = |name: &str| {
            params
                .get(name)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
                .ok_or_else(|| invalid(&format!("`{name}` must be a non-empty string")))
        };
        Ok(Self {
            agent: parameter("agent")?,
            prompt: parameter("prompt")?,
            output: parameter("output")?,
        })
    }
}

impl Node for Agent {
    /// The node is run for a workflow run, which the plain executor does not give it; see [`Agent::run_node`].
    fn run(&self, _input: Value) -> Result<Value, NodeError> {
        Err(NodeError::Failed(
            "an agent node is run by the executor, which gives it the workflow run and its worktree".to_string(),
        ))
    }

    fn ports(&self) -> Ports {
        Ports::new(["in"], [self.output.clone()])
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            kind: KIND,
            title: "Agent",
            description: "Runs an agent of the harness configuration in a worktree the run owns, and takes its \
                declared output from the fenced JSON block it ends with.",
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::workflows::nodes::node;

    fn params() -> Value {
        json!({
            "agent": "Coder",
            "prompt": "Read the issue {{ trigger.issue }} and answer.",
            "output": "summary",
        })
    }

    #[test]
    fn the_parameters_name_the_agent_the_prompt_and_the_output() {
        let agent = Agent::from_params(&params()).unwrap();
        assert_eq!(agent.agent, "Coder");
        assert_eq!(
            agent.prompt,
            "Read the issue {{ trigger.issue }} and answer."
        );
        assert_eq!(agent.output, "summary");
    }

    #[test]
    fn a_parameter_that_is_not_a_non_empty_string_is_refused() {
        for missing in ["agent", "prompt", "output"] {
            let mut params = params();
            params.as_object_mut().unwrap().remove(missing);
            let error = Agent::from_params(&params).unwrap_err();
            assert!(
                matches!(&error, NodeError::Parameters { kind, message }
                    if kind == KIND && message.contains(missing)),
                "{error}"
            );
            let mut empty = params.clone();
            empty[missing] = json!("");
            assert!(
                Agent::from_params(&empty).is_err(),
                "{missing} taken while empty"
            );
        }
    }

    #[test]
    fn the_registry_builds_the_agent_node_and_keeps_its_declared_output() {
        let built = node(KIND, &params());
        assert!(built.is_ok());
        let ports = built.unwrap().ports();
        assert_eq!(ports.inputs, ["in"]);
        assert_eq!(ports.outputs, ["summary"]);
        assert!(
            matches!(node(KIND, &json!({})), Err(NodeError::Parameters { .. })),
            "the registry takes an agent node without its parameters"
        );
    }

    #[test]
    fn the_node_is_run_for_a_workflow_run_only() {
        let agent = Agent::from_params(&params()).unwrap();
        // The plain executor has no workflow run to give the node, so the node fails closed rather than run blind.
        let error = agent.run(json!(null)).unwrap_err();
        assert!(matches!(error, NodeError::Failed(_)), "{error}");
    }
}
