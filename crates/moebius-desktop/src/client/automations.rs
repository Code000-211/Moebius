//! Automations as the desktop API has them: chats a schedule starts with an agent and a prompt, and their firings.

use serde::{Deserialize, Serialize};

/// An automation, as `GET /v1/automations` lists it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Automation {
    pub id: i64,
    pub name: String,
    /// A cron expression of five fields in the server's local time.
    pub schedule: String,
    pub agent: String,
    pub prompt: String,
    #[serde(default)]
    pub repository_id: Option<i64>,
    /// Whether its chats work in the harness configuration repository.
    #[serde(default)]
    pub harness_repository: bool,
    pub enabled: bool,
    /// When it fires next, as Unix seconds; none while it is off.
    #[serde(default)]
    pub next_firing: Option<i64>,
    #[serde(default)]
    pub last_firing: Option<Firing>,
}

/// A firing of an automation: the chat it started, or why it started none.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Firing {
    pub id: i64,
    /// `started`, `skipped` or `failed`.
    pub status: String,
    #[serde(default)]
    pub run_id: Option<i64>,
    /// The status of its chat now.
    #[serde(default)]
    pub chat_status: Option<String>,
    /// Whether a person fired it ("Run now").
    #[serde(default)]
    pub manual: bool,
    #[serde(default)]
    pub reason: String,
    pub at: i64,
}

/// What a save of an automation sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutomationSettings {
    pub name: String,
    pub schedule: String,
    pub agent: String,
    pub prompt: String,
    pub repository: Option<i64>,
    pub harness: bool,
    pub enabled: bool,
}
