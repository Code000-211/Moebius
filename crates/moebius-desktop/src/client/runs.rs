//! Runs and chats as the desktop API has them: a run, its stored events and the decisions on its tool calls, and the
//! chats, which are runs without a card.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Run {
    pub id: i64,
    pub agent: String,
    pub purpose: String,
    /// `starting`, `running`, `waiting`, `succeeded`, `failed` or `stopped`.
    pub status: String,
    /// The harness that runs the agent, as `GET /v1/runs/{id}` says; the snapshot's short form has none.
    #[serde(default)]
    pub harness: String,
    #[serde(default)]
    pub started_at: i64,
    /// Whether a chat works in the harness configuration repository.
    #[serde(default)]
    pub harness_repository: bool,
    /// The runner the run's harness went on; none for a run on the server.
    #[serde(default)]
    pub runner: Option<String>,
}

/// One stored event of a run, as `GET /v1/runs/{id}/events` lists it and the stream's `run_event` carries it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RunEvent {
    pub seq: i64,
    pub kind: String,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub at: i64,
}

/// A recorded decision, as far as the live view matches it to a tool call.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Judgement {
    pub kind: String,
    pub choice: String,
    #[serde(default)]
    pub reason: String,
    pub decider: String,
    /// What came of the decision, once known; for a refused call that was replayed, `replayed: ...`.
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub run: Option<i64>,
}

/// A chat with an agent, as `GET /v1/chats` lists it: a run without a card.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Chat {
    /// The id of the chat's run.
    pub id: i64,
    pub agent: String,
    /// The status of the run: `starting`, `running`, `waiting`, or how it ended.
    pub status: String,
    #[serde(default)]
    pub repository_id: Option<i64>,
    /// Whether the chat works in the harness configuration repository.
    #[serde(default)]
    pub harness_repository: bool,
    #[serde(default)]
    pub started_at: i64,
    /// The first line of the first message.
    #[serde(default)]
    pub title: String,
    /// The `seq` of the chat's last answer or error.
    #[serde(default)]
    pub answered: i64,
    /// How far the person has read the chat; unread while below `answered`.
    #[serde(default)]
    pub seen: i64,
    /// How many files the chat's worktree changes, committed or not: what offering its changes would offer.
    #[serde(default)]
    pub changed_files: usize,
    /// Where the chat was started: `chat`, or `assistant` for a conversation in the floating window.
    #[serde(default)]
    pub origin: String,
    /// The model the chat was given in place of its agent's; none while it runs on its agent's.
    #[serde(default)]
    pub model: Option<String>,
    /// The reasoning level the chat was given in place of its agent's; none while it runs with its agent's.
    #[serde(default)]
    pub thinking: Option<String>,
}

/// A chat to start: with `agent` (the Moebius agent without one), on `message` (without one the chat waits for the
/// first, so that its terminal can be opened), in `repository` or the harness configuration repository, and on `model`
/// and with the reasoning level `thinking` in place of the agent's when they are given.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct NewChat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub repository: Option<i64>,
    pub harness: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
}

impl Chat {
    /// Whether the chat was started in the floating window, where the person reads it: the Chat page leaves it out
    /// unless it is opened there on purpose, and it is never counted unread.
    pub fn is_assistant(&self) -> bool {
        self.origin == ASSISTANT
    }

    /// Whether the chat can still be written to.
    pub fn is_open(&self) -> bool {
        matches!(self.status.as_str(), "starting" | "running" | "waiting")
    }

    /// Whether the chat's agent is at work, whose worktree is not offered meanwhile.
    pub fn is_working(&self) -> bool {
        matches!(self.status.as_str(), "starting" | "running")
    }

    /// The branch that holds the chat's work in its repository.
    pub fn branch(&self) -> String {
        format!("{CHAT_BRANCH}{}", self.id)
    }
}

pub(super) const CHAT_BRANCH: &str = "moebius/chat-";

/// The origin of the chats the floating window starts.
pub(super) const ASSISTANT: &str = "assistant";
