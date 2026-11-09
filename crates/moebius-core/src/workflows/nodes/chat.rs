//! The `chat` node: an agent chat on a schedule, which is how an automation is stored as a workflow.
//!
//! An automation is a `schedule` trigger and one `chat` node. When a run reaches the node the executor starts the
//! chat from its `agent`, `prompt` and `harness` (see `app::workflows::chat`); its own `run` passes the value it
//! received through, for a caller that only reads the definition.

use serde_json::Value;

use super::{Metadata, Node, NodeError, Ports};

/// The kind name a workflow file writes.
pub const KIND: &str = "chat";

/// The chat a schedule starts with an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    /// The agent the chat is with; empty means the Moebius agent.
    pub agent: String,
    /// The first message of the chat.
    pub prompt: String,
    /// Whether the chat works in the harness configuration repository.
    pub harness: bool,
}

impl Chat {
    /// Read the node's parameters: `agent` (optional, empty for the Moebius agent), `prompt` and `harness`
    /// (optional, default `false`).
    pub fn from_params(params: &Value) -> Result<Self, NodeError> {
        let invalid = |message: &str| NodeError::Parameters {
            kind: KIND.to_string(),
            message: message.to_string(),
        };
        let Value::Object(params) = params else {
            return Err(invalid("parameters must be a table"));
        };
        for key in params.keys() {
            if !["agent", "prompt", "harness"].contains(&key.as_str()) {
                return Err(invalid(&format!(
                    "unknown parameter {key:?}; a chat takes \"agent\", \"prompt\" and \"harness\""
                )));
            }
        }
        let agent = match params.get("agent") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(agent)) => agent.clone(),
            Some(_) => return Err(invalid("\"agent\" must be an agent's name")),
        };
        let prompt = match params.get("prompt") {
            Some(Value::String(prompt)) if !prompt.trim().is_empty() => prompt.clone(),
            Some(Value::String(_)) => return Err(invalid("\"prompt\" must not be empty")),
            Some(_) => return Err(invalid("\"prompt\" must be the chat's first message")),
            None => return Err(invalid("needs a \"prompt\"")),
        };
        let harness = match params.get("harness") {
            None => false,
            Some(Value::Bool(harness)) => *harness,
            Some(_) => return Err(invalid("\"harness\" must be true or false")),
        };
        Ok(Self {
            agent,
            prompt,
            harness,
        })
    }
}

impl Node for Chat {
    /// Passes the value it received through; the executor starts the chat itself (see `app::workflows::chat`).
    fn run(&self, input: Value) -> Result<Value, NodeError> {
        Ok(input)
    }

    fn ports(&self) -> Ports {
        Ports::new(["in"], ["out"])
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            kind: KIND,
            title: "Chat",
            description: "Starts an agent chat when an automation's schedule comes due.",
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn chat(params: Value) -> Chat {
        Chat::from_params(&params).unwrap_or_else(|error| panic!("{params} refused: {error}"))
    }

    #[test]
    fn a_chat_reads_its_agent_prompt_and_place() {
        assert_eq!(
            chat(json!({"agent": "Coder", "prompt": "Triage the board.", "harness": true})),
            Chat {
                agent: "Coder".into(),
                prompt: "Triage the board.".into(),
                harness: true,
            }
        );
        // The agent is optional and empty means the Moebius agent; a place defaults to a repository chat.
        assert_eq!(
            chat(json!({"prompt": "Triage the board."})),
            Chat {
                agent: String::new(),
                prompt: "Triage the board.".into(),
                harness: false,
            }
        );
    }

    #[test]
    fn a_chat_refuses_what_it_cannot_read() {
        for (params, says) in [
            (json!("not a table"), "table"),
            (json!({"prompt": "p", "message": "m"}), "unknown parameter"),
            (json!({"prompt": "p", "agent": 3}), "agent's name"),
            (json!({"prompt": " "}), "must not be empty"),
            (json!({"prompt": 3}), "first message"),
            (json!({}), "needs a \"prompt\""),
            (json!({"prompt": "p", "harness": "yes"}), "true or false"),
        ] {
            let refused = Chat::from_params(&params).unwrap_err();
            assert!(
                refused.to_string().contains(says),
                "{params} refused with {refused}, not {says}"
            );
        }
    }

    #[test]
    fn a_chat_run_passes_its_input_through() {
        let input = json!({"fired": "2026-11-09 03:30"});
        assert_eq!(
            chat(json!({"prompt": "p"})).run(input.clone()).unwrap(),
            input
        );
    }

    #[test]
    fn a_chat_has_one_way_in_and_one_way_out() {
        let ports = chat(json!({"prompt": "p"})).ports();
        assert_eq!(ports.inputs, ["in"]);
        assert_eq!(ports.outputs, ["out"]);
    }
}
