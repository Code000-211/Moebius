//! The `approval` node: it pauses a workflow run until a person decides, and the run continues with the value the
//! person approved or edited, or it fails with the reason they gave, or at the node's deadline when no decision
//! comes. The node does not wait by itself: a card run never waits for a person, and the one place that changes is a
//! workflow run, which waits where its run drives it (see `app/workflows/approval.rs`).

use serde_json::Value;

use super::{Metadata, Node, NodeError, Ports};
#[cfg(test)]
use crate::workflows::definition::WorkflowDefinition;

/// The kind name a workflow file writes.
pub const KIND: &str = "approval";

/// How long a decision is waited for when the node's parameters give no timeout of their own.
pub const DEFAULT_TIMEOUT_SECONDS: i64 = 3600;

/// A person's decision on the value a workflow run waits on.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// The value goes on as it is.
    Approve,
    /// The value is replaced with the person's.
    Edit(Value),
    /// The run ends `failed` with the reason.
    Reject(String),
}

/// A node that pauses a run until a person decides on its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    prompt: String,
    timeout_seconds: i64,
}

impl Approval {
    /// Read the node's parameters: `prompt`, the text a person reads when deciding (required), and
    /// `timeout_seconds`, how long the value is waited for (optional, a positive whole number of seconds).
    pub fn from_params(params: &Value) -> Result<Self, NodeError> {
        let invalid = |message: &str| NodeError::Parameters {
            kind: KIND.to_string(),
            message: message.to_string(),
        };
        let prompt = match params.get("prompt") {
            Some(Value::String(prompt)) if !prompt.trim().is_empty() => prompt.clone(),
            _ => {
                return Err(invalid(
                    "\"prompt\" must be the text a person reads when deciding",
                ));
            }
        };
        let timeout_seconds = match params.get("timeout_seconds") {
            None => DEFAULT_TIMEOUT_SECONDS,
            Some(value) => value
                .as_i64()
                .filter(|seconds| *seconds > 0)
                .ok_or_else(|| {
                    invalid("\"timeout_seconds\" must be a positive whole number of seconds")
                })?,
        };
        Ok(Self {
            prompt,
            timeout_seconds,
        })
    }

    /// The text a person reads when deciding.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// The deadline, in whole seconds since the epoch, from `now` and the node's own timeout.
    pub fn deadline(&self, now: i64) -> i64 {
        now + self.timeout_seconds
    }

    /// The value a run continues with, from the person's decision: the value for an approval, the person's for an
    /// edit; a rejection fails the node with the reason.
    pub fn continues_with(&self, value: &Value, decision: &Decision) -> Result<Value, NodeError> {
        match decision {
            Decision::Approve => Ok(value.clone()),
            Decision::Edit(value) => Ok(value.clone()),
            Decision::Reject(reason) => Err(NodeError::Failed(reason.clone())),
        }
    }

    /// The failure a run ends with when the wait reaches its deadline without a decision, naming the deadline.
    pub fn deadline_error(deadline: i64) -> NodeError {
        NodeError::Failed(format!(
            "no decision for the approval before its deadline {}",
            format_deadline(deadline)
        ))
    }
}

/// A deadline, in whole seconds since the epoch, as the text a person reads.
fn format_deadline(deadline: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(deadline)
        .map(|at| {
            at.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default()
        })
        .unwrap_or_else(|_| deadline.to_string())
}

impl Node for Approval {
    fn run(&self, _input: Value) -> Result<Value, NodeError> {
        Err(NodeError::Failed(
            "an approval node does not run unattended: the workflow run that drives it waits for a person's decision"
                .to_string(),
        ))
    }

    fn ports(&self) -> Ports {
        Ports::new(["in"], ["out", "failed"])
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            kind: KIND,
            title: "Approval",
            description: "Pauses the run until a person approves or edits the value, rejects it, or the wait reaches its deadline.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::nodes::node;
    use serde_json::json;

    #[test]
    fn a_prompt_is_required_and_a_timeout_must_be_positive() {
        let node = Approval::from_params(&json!({"prompt": "Ship it?"})).unwrap();
        assert_eq!(node.prompt(), "Ship it?");
        assert_eq!(node.deadline(0), DEFAULT_TIMEOUT_SECONDS);

        let node = Approval::from_params(&json!({
            "prompt": "Ship it?",
            "timeout_seconds": 60
        }))
        .unwrap();
        assert_eq!(node.deadline(0), 60);

        let error = Approval::from_params(&json!({})).unwrap_err();
        assert!(
            matches!(error, NodeError::Parameters { ref message, .. } if message.contains("prompt")),
            "{error:?}"
        );
        let error = Approval::from_params(&json!({"prompt": ""})).unwrap_err();
        assert!(matches!(error, NodeError::Parameters { .. }), "{error:?}");
        let error =
            Approval::from_params(&json!({"prompt": "x", "timeout_seconds": 0})).unwrap_err();
        assert!(
            matches!(error, NodeError::Parameters { ref message, .. } if message.contains("timeout_seconds")),
            "{error:?}"
        );
    }

    #[test]
    fn the_registry_builds_the_node_from_its_parameters() {
        let built = node(KIND, &json!({"prompt": "Ship it?"})).unwrap();
        assert_eq!(built.metadata().title, "Approval");
        assert_eq!(built.ports().inputs, ["in"]);
        assert_eq!(built.ports().outputs, ["out", "failed"]);
    }

    #[test]
    fn the_node_does_not_run_unattended() {
        let node = Approval::from_params(&json!({"prompt": "Ship it?"})).unwrap();
        let error = node.run(json!(1)).unwrap_err();
        assert!(matches!(error, NodeError::Failed(ref reason) if reason.contains("unattended")));
    }

    #[test]
    fn each_decision_leaves_its_own_value_or_its_reason() {
        let node = Approval::from_params(&json!({"prompt": "Ship it?"})).unwrap();
        let value = json!({"version": "1.2.3"});

        assert_eq!(
            node.continues_with(&value, &Decision::Approve).unwrap(),
            value
        );
        let edited = json!({"version": "1.2.4"});
        assert_eq!(
            node.continues_with(&value, &Decision::Edit(edited.clone()))
                .unwrap(),
            edited
        );
        let error = node
            .continues_with(&value, &Decision::Reject("not this one".into()))
            .unwrap_err();
        assert_eq!(error, NodeError::Failed("not this one".into()));
    }

    #[test]
    fn the_deadline_is_named_when_the_wait_ends_without_a_decision() {
        let error = Approval::deadline_error(0);
        match error {
            NodeError::Failed(reason) => {
                assert!(reason.contains("deadline"), "{reason}");
                assert!(reason.contains("1970-01-01T00:00:00"), "{reason}");
            }
            other => panic!("the deadline fails the node with a reason, not {other:?}"),
        }
        assert_eq!(format_deadline(1700000000), "2023-11-14T22:13:20Z");
    }

    /// A workflow file whose one node is an approval with these parameters.
    fn asking(params: &str) -> String {
        format!(
            r#"
name = "asking"
description = "Ask before going on."

[[trigger]]
kind = "test"

[[nodes]]
id = "ask"
kind = "approval"
params = {params}

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
"#
        )
    }

    #[test]
    fn a_workflow_file_is_refused_with_the_parameter_that_is_wrong() {
        let file = "workflows/asking.toml";
        let no_prompt = "node \"ask\": invalid parameters for node kind \"approval\": \"prompt\" must be the text a person reads when deciding";
        let no_timeout = "node \"ask\": invalid parameters for node kind \"approval\": \"timeout_seconds\" must be a positive whole number of seconds";
        for (params, expected) in [
            ("{}", no_prompt),
            ("{ prompt = 7 }", no_prompt),
            ("{ prompt = \"Ship it?\", timeout_seconds = 0 }", no_timeout),
            (
                "{ prompt = \"Ship it?\", timeout_seconds = \"an hour\" }",
                no_timeout,
            ),
        ] {
            let definition = WorkflowDefinition::parse(&asking(params), file).unwrap();
            assert_eq!(definition.validate(file).unwrap_err().message(), expected);
        }

        let definition =
            WorkflowDefinition::parse(&asking("{ prompt = \"Ship it?\" }"), file).unwrap();
        definition.validate(file).unwrap();
    }
}
