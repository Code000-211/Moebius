//! The chats and what grilling chats ended in: `chats` lists them, `chat` starts one, writes to it, offers its
//! changes and ends it; `grillings` lists the grillings, `grilling` shows, confirms and dismisses one.

use clap::Subcommand;
use serde_json::{Map, Value, json};

use super::{Plan, Request, Show, json_input, read_input};

#[derive(Debug, Subcommand)]
pub enum ChatCommand {
    /// Start a chat; the agent answers the message.
    Start {
        /// The first message, or `-` for standard input.
        message: String,
        /// The agent to talk to; the Moebius agent without it.
        #[arg(long)]
        agent: Option<String>,
        /// The id of the repository to work in.
        #[arg(long)]
        repository: Option<i64>,
        /// The id of an ended chat to go on from.
        #[arg(long)]
        continues: Option<i64>,
    },
    /// Write to a chat that has not ended.
    Say {
        id: i64,
        /// The message, or `-` for standard input.
        message: String,
    },
    /// Offer what a chat's worktree changes as a change, without ending the chat.
    Offer {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
    /// End a chat.
    End { id: i64 },
}

#[derive(Debug, Subcommand)]
pub enum GrillingCommand {
    Show {
        id: i64,
    },
    /// Make cards of the issues: the proposed ones, or the list in a JSON file (or `-`).
    Confirm {
        id: i64,
        #[arg(long)]
        issues: Option<String>,
    },
    Dismiss {
        id: i64,
    },
}

/// `chats`.
pub fn chats() -> Plan {
    Ok((
        Request::get("/v1/chats"),
        Show::Table(&["id", "agent", "status", "repository_id", "title"]),
    ))
}

/// `chat start`, `say`, `offer` and `end`.
pub fn chat(command: &ChatCommand) -> Plan {
    let request = match command {
        ChatCommand::Start {
            message,
            agent,
            repository,
            continues,
        } => {
            let mut body = Map::new();
            body.insert("message".into(), text_argument(message)?.into());
            for (key, value) in [
                ("agent", json!(agent)),
                ("repository", json!(repository)),
                ("continues", json!(continues)),
            ] {
                if !value.is_null() {
                    body.insert(key.into(), value);
                }
            }
            Request::post("/v1/chats", body.into())
        }
        ChatCommand::Say { id, message } => Request::post(
            format!("/v1/chats/{id}/messages"),
            json!({ "message": text_argument(message)? }),
        ),
        ChatCommand::Offer { id, reason } => {
            Request::post(format!("/v1/chats/{id}/offer"), json!({ "reason": reason }))
        }
        ChatCommand::End { id } => Request::post(format!("/v1/chats/{id}/end"), json!({})),
    };
    Ok((request, Show::Object))
}

/// `grillings`.
pub fn grillings() -> Plan {
    Ok((
        Request::get("/v1/grillings"),
        Show::Table(&["id", "status", "repository_id", "reason"]),
    ))
}

/// `grilling show`, `confirm` and `dismiss`; `confirm` without `--issues` reads the proposed issues first.
pub fn grilling(
    command: &GrillingCommand,
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
) -> Plan {
    let request = match command {
        GrillingCommand::Show { id } => Request::get(format!("/v1/grillings/{id}")),
        GrillingCommand::Confirm { id, issues } => {
            let issues = match issues {
                Some(source) => json_input(source)?,
                None => get(&format!("/v1/grillings/{id}"))?["issues"].clone(),
            };
            Request::post(
                format!("/v1/grillings/{id}/confirm"),
                json!({ "issues": issues }),
            )
        }
        GrillingCommand::Dismiss { id } => {
            Request::post(format!("/v1/grillings/{id}/dismiss"), json!({}))
        }
    };
    Ok((request, Show::Object))
}

/// A message given as an argument, or `-` for standard input.
fn text_argument(text: &str) -> Result<String, String> {
    if text == "-" {
        read_input("-")
    } else {
        Ok(text.to_string())
    }
}
