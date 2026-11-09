//! What a workflow's `chat` node does when a run reaches it: starts an agent chat in the run's repository with the
//! node's agent, prompt and place, and records the chat on the firing the run carries when the workflow is an
//! automation.
//!
//! The node is the one an automation is made of, so this is where a scheduled automation turns into a chat; a person
//! who starts the run by hand gets the same chat.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::app::{AUTOMATION_ORIGIN, App};
use crate::store::WorkflowRun;
use crate::workflows::definition::NodeDefinition;
use crate::workflows::nodes::NodeError;
use crate::workflows::nodes::chat::Chat;

/// Starts the chat of the `chat` node of workflow run `run`.
pub fn start(app: &Arc<App>, run: i64, node: &NodeDefinition) -> Result<Value, NodeError> {
    let chat = Chat::from_params(&node.params)?;
    let run: WorkflowRun = app
        .store
        .workflow_run(run)
        .map_err(|error| NodeError::Failed(error.to_string()))?
        .ok_or_else(|| NodeError::Failed(format!("workflow run {run} is gone")))?;
    let runs = app
        .runs()
        .map_err(|error| NodeError::Failed(error.to_string()))?;
    let agent = (!chat.agent.is_empty()).then_some(chat.agent.as_str());
    let started = app
        .start_chat(
            &**runs,
            agent,
            Some(run.repository_id),
            chat.harness,
            Some(&chat.prompt),
            None,
            None,
            Some(AUTOMATION_ORIGIN),
            (None, None, &[]),
        )
        .map_err(|error| NodeError::Failed(error.to_string()))?;
    if let Some(firing) = app
        .store
        .firing_for_workflow_run(run.id)
        .map_err(|error| NodeError::Failed(error.to_string()))?
    {
        app.store
            .set_firing_run(firing.id, started.id)
            .map_err(|error| NodeError::Failed(error.to_string()))?;
    }
    Ok(json!({ "run": started.id }))
}
