//! The value an agent node takes from its agent's final answer: the last fenced block of it, parsed as JSON. A
//! missing block, or one that is not JSON, fails the node closed.

use serde_json::Value;

use super::{KIND, NodeError};

/// The value the node takes from the final answer `answer`: the last fenced block of it, parsed as JSON. A missing
/// block, or one that is not JSON, fails the node closed.
pub(super) fn value_of(answer: &str) -> Result<Value, NodeError> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Option<Vec<String>> = None;
    for line in answer.lines() {
        if line.trim_start().starts_with("```") {
            if let Some(lines) = current.take() {
                blocks.push(lines.join("\n"));
            } else {
                current = Some(Vec::new());
            }
        } else if let Some(lines) = &mut current {
            lines.push(line.to_string());
        }
    }
    let failed = |why: String| NodeError::Failed(format!("node {KIND}: {why}"));
    match blocks.last() {
        Some(block) => serde_json::from_str(block)
            .map_err(|e| failed(format!("the agent's fenced block is not JSON: {e}"))),
        None => Err(failed(
            "the agent ended without a fenced JSON block: the node's declared output is missing"
                .to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_last_fenced_block_of_the_answer_is_the_output() {
        let answer = "A draft first:\n```json\n{\"draft\": 1}\n```\nthen the answer:\n```json\n{\"done\": true}\n```";
        assert_eq!(value_of(answer).unwrap(), json!({"done": true}));
    }

    #[test]
    fn a_block_the_agent_never_closed_is_no_block() {
        let error = value_of("```json\n{\"done\": true}").unwrap_err();
        assert!(
            matches!(&error, NodeError::Failed(why) if why.contains("without a fenced JSON block")),
            "{error}"
        );
    }

    #[test]
    fn a_block_that_is_not_json_fails_the_node_closed() {
        let error = value_of("```json\n{\"done\": tru}\n```").unwrap_err();
        assert!(
            matches!(&error, NodeError::Failed(why) if why.contains("not JSON")),
            "{error}"
        );
    }

    #[test]
    fn an_answer_without_a_block_fails_the_node_closed() {
        let error = value_of("I read the issue and answered in prose.").unwrap_err();
        assert!(
            matches!(&error, NodeError::Failed(why) if why.contains("without a fenced JSON block")),
            "{error}"
        );
    }
}
