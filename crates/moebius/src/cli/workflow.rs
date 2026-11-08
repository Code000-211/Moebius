//! The workflow commands: list the workflows of the harness configuration repository, show one, save a file as one,
//! and delete one. Saving and deleting are commits on that repository.

use clap::Subcommand;
use serde_json::json;

use super::{Plan, Request, Show, read_input};

/// What `moebius workflow` does.
#[derive(Debug, Subcommand)]
pub enum WorkflowCommand {
    /// The workflows, with their trigger kind and how many nodes they have.
    List,
    /// One workflow: its file and its parsed form.
    Show {
        /// The workflow's name.
        name: String,
    },
    /// Add a workflow or replace it, committed to the harness configuration repository.
    Save {
        /// The workflow's name.
        name: String,
        /// The TOML file to save, or `-` for standard input.
        file: String,
    },
    /// Delete a workflow; a run of it queued or running refuses the deletion.
    Delete {
        /// The workflow's name.
        name: String,
    },
}

/// The request a workflow command stands for and how to show its answer.
pub fn workflow(command: &WorkflowCommand) -> Plan {
    Ok(match command {
        WorkflowCommand::List => (
            Request::get("/v1/workflows"),
            Show::Table(&["name", "description", "trigger_kind", "node_count"]),
        ),
        WorkflowCommand::Show { name } => {
            (Request::get(format!("/v1/workflows/{name}")), Show::Raw)
        }
        WorkflowCommand::Save { name, file } => {
            let text = read_input(file)?;
            (
                Request::put(format!("/v1/workflows/{name}"), json!({ "file": text })),
                Show::Object,
            )
        }
        WorkflowCommand::Delete { name } => (
            Request::delete(format!("/v1/workflows/{name}")),
            Show::Object,
        ),
    })
}
