//! The client of the desktop API (`docs/reference/desktop-api.md`), the only way the app talks to the server: typed
//! answers for what the shell shows, and the event stream as an iced subscription that reconnects by itself.
//!
//! The types keep to what the reference says and are tolerant: a field that may be missing has a default and a field
//! the app does not know is ignored, so a newer server does not break an older app.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use iced::futures::channel::mpsc;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::settings::Settings;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("the token is not accepted")]
    Unauthorized,
    #[error("cannot reach {0}")]
    Unreachable(String),
    #[error("the server answered {status}: {message}")]
    Server { status: u16, message: String },
    #[error("the server's answer is not understood: {0}")]
    Decode(String),
}

/// A file the server stored: `path` is where it is on the server.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Upload {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Health {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Repository {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub path: String,
    /// `none` or `github`.
    #[serde(default)]
    pub forge: String,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub github: Option<String>,
    #[serde(default)]
    pub trusted: Vec<String>,
}

/// What `POST /v1/repositories` takes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NewRepository {
    pub name: String,
    pub url: String,
    /// `none` or `github`.
    pub forge: &'static str,
    /// `owner/name`, with the forge `github`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trusted: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Card {
    pub id: i64,
    pub repository_id: i64,
    pub number: i64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub author: String,
    pub column: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub retried_after: Option<i64>,
    /// The `code` rounds and the tokens since the card was last retried.
    #[serde(default)]
    pub rounds: i64,
    #[serde(default)]
    pub tokens: i64,
    /// The card's latest run.
    #[serde(default)]
    pub run: Option<Run>,
    /// The numbers of the issues that block the card.
    #[serde(default)]
    pub blocked_by: Vec<i64>,
    /// How many tool calls of the latest run were refused.
    #[serde(default)]
    pub refusals: i64,
    /// The card's open change.
    #[serde(default)]
    pub change: Option<CardChange>,
}

impl Card {
    /// Whether the card's latest run is working.
    pub fn is_live(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|run| matches!(run.status.as_str(), "starting" | "running"))
    }
}

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

const CHAT_BRANCH: &str = "moebius/chat-";

/// The origin of the chats the floating window starts.
const ASSISTANT: &str = "assistant";

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

/// An agent the server can run, as `GET /v1/agents` lists it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Agent {
    pub name: String,
    pub model: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub token_budget: Option<i64>,
    /// Whether Moebius brings the agent itself.
    #[serde(default)]
    pub built_in: bool,
    /// Why the agent's files cannot be used; such an agent cannot run.
    #[serde(default)]
    pub error: Option<String>,
}

/// An upstream of the router, as `GET /v1/models` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Upstream {
    pub protocol: String,
    pub base_url: String,
    pub api_key_set: bool,
}

/// The teacher model, as `GET /v1/models` tells it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Teacher {
    pub model: String,
    pub protocol: String,
    pub timeout_seconds: u64,
}

/// The decision model's endpoint, as `GET /v1/models` tells it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DecisionModelEndpoint {
    pub url: String,
    pub model: String,
    pub timeout_seconds: u64,
    pub api_key_set: bool,
}

/// What `GET /v1/models` says: the models the server uses and where it reaches them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Models {
    pub upstreams: Vec<Upstream>,
    pub agents: Vec<Agent>,
    pub teacher: Option<Teacher>,
    pub decision_model: Option<DecisionModelEndpoint>,
}

/// A model an upstream serves, as `GET /v1/models/available` lists it. It reads as its id.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AvailableModel {
    pub id: String,
    pub protocol: String,
    pub upstream: String,
}

impl std::fmt::Display for AvailableModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.id)
    }
}

/// An upstream that could not say which models it serves.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UpstreamError {
    pub upstream: String,
    pub message: String,
}

/// What `GET /v1/models/available` says: the models of all upstreams and the upstreams that could not be asked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Available {
    pub models: Vec<AvailableModel>,
    pub errors: Vec<UpstreamError>,
}

/// A plugin the server has loaded, as `GET /v1/plugins` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub kinds: Vec<String>,
    pub decides: Vec<String>,
}

/// What `GET /v1/plugins` says.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Plugins {
    pub time_limit_seconds: u64,
    pub plugins: Vec<Plugin>,
}

/// One thing to set up before the first card, as `GET /v1/start` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StartStep {
    /// `upstream`, `pi_bolt`, `coder` or `repository`.
    pub key: String,
    pub title: String,
    pub done: bool,
    pub detail: String,
}

/// What `GET /v1/start` says.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Start {
    pub done: bool,
    pub steps: Vec<StartStep>,
}

/// One key of `moebius.toml` as `GET /v1/settings` tells it: what the file says, or the default.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Setting {
    pub value: serde_json::Value,
    /// True when the file does not set the key and `value` is the default.
    pub default: bool,
}

/// What `GET /v1/settings` says: every key of the configuration, the stored keys' names and what needs a restart.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ServerSettings {
    /// The configuration file on the server.
    pub file: String,
    pub values: std::collections::BTreeMap<String, Setting>,
    /// The names of the stored API keys, never the keys.
    #[serde(default)]
    pub secrets: Vec<String>,
    #[serde(default)]
    pub restart_needed: bool,
    #[serde(default)]
    pub restart_keys: Vec<String>,
}

impl ServerSettings {
    /// The setting `key`, or null when the server did not tell it.
    pub fn value(&self, key: &str) -> &serde_json::Value {
        self.values
            .get(key)
            .map_or(&serde_json::Value::Null, |setting| &setting.value)
    }

    /// Whether the file sets `key`; the default is in use when it does not.
    pub fn is_set(&self, key: &str) -> bool {
        self.values.get(key).is_some_and(|setting| !setting.default)
    }
}

/// What `PUT /v1/settings` answers: whether some changed key is used only after a restart.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SavedSettings {
    pub restart_needed: bool,
    #[serde(default)]
    pub restart_keys: Vec<String>,
}

/// The settings of an agent of the harness configuration repository, as `GET` and `PUT /v1/agents/{name}` have them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSettings {
    pub model: String,
    pub protocol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub mcp: Vec<String>,
}

/// An agent: what `GET /v1/agents/{name}` answers, and `PUT` after it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AgentDetail {
    pub name: String,
    /// The settings of its files in the harness configuration repository; none for a built-in agent without files.
    #[serde(default)]
    pub settings: Option<AgentSettings>,
    #[serde(default)]
    pub instructions: String,
    /// What a built-in agent brings itself, read only.
    #[serde(default)]
    pub built_in: Option<BuiltInAgent>,
    /// The short id of the commit a `PUT` made; none when the edit changed nothing.
    #[serde(default)]
    pub commit: Option<String>,
}

/// The built-in instructions and skills of a built-in agent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BuiltInAgent {
    pub instructions: String,
    #[serde(default)]
    pub skills: Vec<BuiltInSkill>,
}

/// A built-in skill of a built-in agent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BuiltInSkill {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// A built-in rule, as `GET /v1/rules` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RulePattern {
    pub name: String,
    pub description: String,
}

/// A rule of a `rules.md`: its name and what it says.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Rule {
    pub name: String,
    pub text: String,
}

/// The rules in prose of the server, as `GET /v1/rules` has them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProseRules {
    /// `harness`, `file` or none.
    #[serde(default)]
    pub source: Option<String>,
    /// Whether `PUT /v1/rules` can change the text.
    #[serde(default)]
    pub editable: bool,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub error: Option<String>,
}

/// A repository's own `.moebius/rules.md`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepositoryRules {
    pub repository: String,
    /// The repository's id, to save its `.moebius/rules.md` with.
    #[serde(default)]
    pub repository_id: i64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub error: Option<String>,
}

/// What `GET` and `PUT /v1/rules` answer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RulesView {
    pub patterns: Vec<RulePattern>,
    pub prose: ProseRules,
    #[serde(default)]
    pub repositories: Vec<RepositoryRules>,
    /// The refusals of the last 7 days by rule name.
    #[serde(default)]
    pub refusals: std::collections::BTreeMap<String, i64>,
    /// The short id of the commit a `PUT` made; none when the text changed nothing.
    #[serde(default)]
    pub commit: Option<String>,
}

/// A skill an agent can name, as `GET /v1/skills` lists it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Skill {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub built_in: bool,
}

/// An MCP server an agent can name, as `GET /v1/mcp-servers` lists it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct McpServer {
    pub name: String,
    /// `stdio` or `remote`.
    pub kind: String,
    #[serde(default)]
    pub error: Option<String>,
}

/// A column's script as `GET /v1/repositories/{id}/scripts/{column}` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScriptDetail {
    pub column: String,
    /// Where the repository keeps its own script for the column.
    pub path: String,
    /// `repository` or `built-in`.
    pub source: String,
    pub text: String,
}

/// A directory of a repository's default branch as `GET /v1/repositories/{id}/tree` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tree {
    /// The directory, `""` for the root.
    pub path: String,
    /// Directories first, each group by name.
    pub entries: Vec<TreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    /// From the repository's root.
    pub path: String,
    /// `dir` or `file`.
    pub kind: String,
    #[serde(default)]
    pub size: u64,
}

/// A file of a repository's default branch as `GET /v1/repositories/{id}/files/{path}` answers it: its text, or for a
/// binary file or one too large to edit only its size.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileDetail {
    pub path: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub binary: bool,
    #[serde(default)]
    pub too_large: bool,
}

/// A skill as `GET /v1/skills/{name}` answers it: its description, the text of its `SKILL.md` and the other files.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkillDetail {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub text: String,
    /// The paths of the skill's other files, which the app shows and does not edit.
    #[serde(default)]
    pub files: Vec<String>,
    /// The short id of the commit a `PUT` made; none when the edit changed nothing.
    #[serde(default)]
    pub commit: Option<String>,
}

/// What an MCP server is: a command with arguments and environment, or a URL with headers. Values may be `${NAME}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct McpDefinition {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
}

/// An MCP server as `GET /v1/mcp-servers/{name}` answers it, and `PUT` after it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct McpServerDetail {
    pub name: String,
    #[serde(flatten)]
    pub definition: McpDefinition,
    /// The short id of the commit a `PUT` made; none when the edit changed nothing.
    #[serde(default)]
    pub commit: Option<String>,
}

/// A Retro as `GET /v1/retros` lists it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RetroSummary {
    /// The local day, `YYYY-MM-DD`.
    pub date: String,
    /// `reported`, `analysing`, `proposed` or `failed`.
    pub status: String,
    pub errors: usize,
    pub proposals: usize,
    #[serde(default)]
    pub at: i64,
}

/// One error of a Retro's report.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ReportError {
    pub kind: String,
    #[serde(default)]
    pub run: Option<i64>,
    #[serde(default)]
    pub card: Option<i64>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Report {
    #[serde(default)]
    pub errors: Vec<ReportError>,
}

/// A change to the harness configuration that a Retro proposes, on a branch.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Proposal {
    pub id: i64,
    pub branch: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// The agents whose files the branch changes.
    #[serde(default)]
    pub agents: Vec<String>,
    /// The cards whose errors the proposal addresses.
    #[serde(default)]
    pub cards: Vec<i64>,
    /// `open`, `trying`, `positive`, `negative`, `untried`, `merged` or `dismissed`.
    pub status: String,
    #[serde(default)]
    pub trials: Vec<Trial>,
}

/// A proposal tried on one card.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Trial {
    /// The card it was tried on; none when the proposal names no cards.
    #[serde(default)]
    pub card_id: Option<i64>,
    /// `positive`, `negative`, `skipped` or `failed`.
    pub outcome: String,
    pub reason: String,
}

/// A Retro with its report and proposals, as `GET /v1/retros/{date}` answers.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RetroView {
    pub date: String,
    pub status: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub report: Report,
    #[serde(default)]
    pub proposals: Vec<Proposal>,
}

/// One lever of a research, tried on a branch of its own.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Experiment {
    pub id: i64,
    pub branch: String,
    pub title: String,
    /// What the measure printed on the branch; none when the measurement failed.
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub checks: Option<bool>,
    pub kept: bool,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub change_id: Option<i64>,
}

/// What the server holds of one kind of judgement, as `GET /v1/judgements/kinds` says.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct KindCounts {
    pub kind: String,
    pub count: i64,
    pub with_outcome: i64,
    pub by_person: i64,
    pub held_out: i64,
}

/// A training-set export a person started: when, and what it holds of each kind.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Export {
    pub id: i64,
    pub at: i64,
    pub kinds: Vec<KindCounts>,
}

/// A proposal that the decision model take a kind of decision over.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Takeover {
    pub id: i64,
    pub kind: String,
    /// The agreement when it was proposed, between 0 and 1.
    pub agreement: f64,
    pub count: i64,
    /// `open`, `merged`, `dismissed` or `released`.
    pub status: String,
}

/// How the decision model does on one kind, as `GET /v1/decision-model` says.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ShadowedKind {
    pub kind: String,
    pub shadows: i64,
    #[serde(default)]
    pub agreement: Option<f64>,
    pub failed: i64,
    pub dropped: i64,
    /// The model decides the kind now.
    pub routed: bool,
}

/// The shadowing of the decision model, as `GET /v1/decision-model` says.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Shadowing {
    pub configured: bool,
    #[serde(default)]
    pub model: Option<String>,
    /// The agreement a takeover needs.
    #[serde(default)]
    pub agreement: Option<f64>,
    #[serde(default)]
    pub min_judgements: Option<i64>,
    pub kinds: Vec<ShadowedKind>,
    pub takeovers: Vec<Takeover>,
}

/// A research as `GET /v1/research` lists it: the brief, how it stands and its experiments.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Research {
    pub id: i64,
    pub repository_id: i64,
    pub goal: String,
    pub measure: String,
    /// `lower` or `higher`.
    pub better: String,
    pub token_budget: i64,
    /// `draft`, `confirmed`, `running`, `done`, `spent`, `stopped` or `failed`.
    pub status: String,
    #[serde(default)]
    pub baseline: Option<f64>,
    #[serde(default)]
    pub reason: String,
    /// The tokens its runs have used.
    #[serde(default)]
    pub tokens: i64,
    #[serde(default)]
    pub experiments: Vec<Experiment>,
}

/// What the model calls used in a window of days, as `GET /v1/usage` answers.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UsageReport {
    /// The server's date, `YYYY-MM-DD`.
    pub today: String,
    pub days_in_window: i64,
    pub totals: UsageTotals,
    /// The days of the last year that had usage, oldest first.
    pub days: Vec<UsageDay>,
    pub models: Vec<ModelUsage>,
    pub agents: Vec<AgentUsage>,
    /// What the model did with its time, the activity with most time first.
    pub activities: Vec<ActivityUsage>,
    pub tool_waits: Vec<ToolWait>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UsageTotals {
    pub tokens: i64,
    pub calls: i64,
    /// The share of the input tokens served from the provider's cache, 0 to 1.
    pub cached_share: f64,
    pub model_ms: i64,
    pub day_budget: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UsageDay {
    pub date: String,
    pub tokens: i64,
    /// The model that used most tokens on the day.
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub tokens: i64,
    pub calls: i64,
    pub input_tokens: i64,
    pub cached_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AgentUsage {
    pub agent: String,
    pub calls: i64,
    pub tokens: i64,
    pub input_tokens: i64,
    pub cached_tokens: i64,
    pub model_ms: i64,
}

/// What the model did with some of its time: a tool's name, `answering` or `unknown`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ActivityUsage {
    pub activity: String,
    pub model_ms: i64,
    pub calls: i64,
    pub output_tokens: i64,
}

/// A tool whose calls made runs wait, with the seconds they waited in all.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ToolWait {
    pub tool: String,
    pub calls: i64,
    pub seconds: i64,
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

/// One entry of a card's timeline.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HistoryEntry {
    pub at: i64,
    pub kind: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

/// The open change of a card, as far as the board shows it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CardChange {
    pub id: i64,
    /// `pending`, `success`, `failure` or `none`.
    pub checks: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Change {
    pub id: i64,
    /// None for a change of the harness configuration repository.
    #[serde(default)]
    pub repository_id: Option<i64>,
    /// Whether the change belongs to the harness configuration repository.
    #[serde(default)]
    pub harness_repository: bool,
    #[serde(default)]
    pub card_id: Option<i64>,
    pub branch: String,
    /// `open`, `merged` or `closed`.
    pub state: String,
    #[serde(default)]
    pub pull_request: Option<i64>,
    #[serde(default)]
    pub checks: Option<String>,
    #[serde(default)]
    pub review: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Board {
    pub repository: Repository,
    #[serde(default)]
    pub cards: Vec<Card>,
    /// The most `code` rounds a card gets; 0 when the server does not say.
    #[serde(default)]
    pub round_limit: i64,
    /// The reason each waiting card waits, by card id.
    #[serde(default)]
    pub waiting: HashMap<i64, String>,
    #[serde(default)]
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InboxItem {
    pub id: i64,
    pub kind: String,
    #[serde(default)]
    pub card_id: Option<i64>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub at: i64,
}

/// One issue a grilling proposes, or the person made of it: `blocked_by` are indexes into the list it is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrillingIssue {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub blocked_by: Vec<usize>,
}

impl InboxItem {
    /// The id of the grilling a `grilling_issues` or `grilling_report` item is about.
    pub fn grilling(&self) -> Option<i64> {
        self.data["grilling"].as_i64()
    }

    /// The id of the repository a grilling's item is about.
    pub fn repository(&self) -> Option<i64> {
        self.data["repository"].as_i64()
    }

    /// The issues a `grilling_issues` item proposes.
    pub fn issues(&self) -> Vec<GrillingIssue> {
        serde_json::from_value(self.data["issues"].clone()).unwrap_or_default()
    }

    /// The report of a `grilling_report` item.
    pub fn report(&self) -> &str {
        self.data["report"].as_str().unwrap_or_default()
    }

    /// The id of the change a `ready_to_merge` or `change_closed` item is about.
    pub fn change(&self) -> Option<i64> {
        self.data["change"].as_i64()
    }

    /// The id of the proposal a `proposal` item is about.
    pub fn proposal(&self) -> Option<i64> {
        self.data["proposal"].as_i64()
    }

    /// The id of the takeover a `takeover` item is about.
    pub fn takeover(&self) -> Option<i64> {
        self.data["takeover"].as_i64()
    }

    /// The id of the chat whose work a `ready_to_merge` item offers: the number in its branch `moebius/chat-<id>`.
    pub fn chat(&self) -> Option<i64> {
        self.data["branch"]
            .as_str()?
            .strip_prefix(CHAT_BRANCH)?
            .parse()
            .ok()
    }
}

impl Board {
    /// How many runs are working on the board's cards.
    pub fn live_runs(&self) -> usize {
        self.cards.iter().filter(|card| card.is_live()).count()
    }
}

/// Everything the app shows, as `GET /v1/snapshot` answers.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub boards: Vec<Board>,
    /// The changes of the harness configuration repository, which belongs to no board.
    #[serde(default)]
    pub harness_changes: Vec<Change>,
    #[serde(default)]
    pub inbox: Vec<InboxItem>,
}

/// How many events one request for a run's events asks for.
const EVENT_PAGE: usize = 500;

/// An event of `GET /v1/events`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ServerEvent {
    pub kind: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

/// What the event stream tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Connected,
    /// The connection failed or ended; the stream tries again by itself.
    Disconnected(String),
    /// The server does not accept the token; the stream stops.
    Unauthorized,
    Event(ServerEvent),
}

pub struct Client {
    http: reqwest::Client,
    settings: Settings,
}

/// The route of a column's script; a column's name may have spaces.
fn script_path(repository: i64, column: &str) -> String {
    format!(
        "/v1/repositories/{repository}/scripts/{}",
        column.replace(' ', "%20")
    )
}

/// `text` with every byte but letters, digits and `-._~` percent-encoded, and `/` kept when `slashes` says so, for a
/// path or a query value of a route.
fn encoded(text: &str, slashes: bool) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            b'/' if slashes => "/".to_string(),
            b => format!("%{b:02X}"),
        })
        .collect()
}

/// The route of a file of a repository.
fn file_path(repository: i64, path: &str) -> String {
    format!(
        "/v1/repositories/{repository}/files/{}",
        encoded(path, true)
    )
}

impl Client {
    pub fn new(settings: Settings) -> Self {
        Client {
            http: reqwest::Client::new(),
            settings,
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let request = self.http.get(format!("{}{path}", self.settings.address));
        self.send(request).await
    }

    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, ClientError> {
        let request = self
            .http
            .post(format!("{}{path}", self.settings.address))
            .json(&body);
        self.send(request).await
    }

    async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, ClientError> {
        let request = self
            .http
            .put(format!("{}{path}", self.settings.address))
            .json(&body);
        self.send(request).await
    }

    async fn send<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, ClientError> {
        let response = request
            .bearer_auth(&self.settings.token)
            .send()
            .await
            .map_err(|_| ClientError::Unreachable(self.settings.address.clone()))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(ClientError::Server {
                status: status.as_u16(),
                message,
            });
        }
        response
            .json()
            .await
            .map_err(|e| ClientError::Decode(e.to_string()))
    }

    pub async fn health(&self) -> Result<Health, ClientError> {
        self.get("/v1/health").await
    }

    pub async fn snapshot(&self) -> Result<Snapshot, ClientError> {
        self.get("/v1/snapshot").await
    }

    pub async fn run(&self, id: i64) -> Result<Run, ClientError> {
        self.get(&format!("/v1/runs/{id}")).await
    }

    /// Every event of a run with a `seq` above `after`, read in pages.
    pub async fn run_events(&self, id: i64, after: i64) -> Result<Vec<RunEvent>, ClientError> {
        let mut events: Vec<RunEvent> = Vec::new();
        loop {
            let from = events.last().map_or(after, |event| event.seq);
            let page: Vec<RunEvent> = self
                .get(&format!(
                    "/v1/runs/{id}/events?after={from}&limit={EVENT_PAGE}"
                ))
                .await?;
            let last = page.len() < EVENT_PAGE;
            events.extend(page);
            if last {
                return Ok(events);
            }
        }
    }

    /// The decisions recorded for a card, oldest first.
    pub async fn judgements(&self, card: i64) -> Result<Vec<Judgement>, ClientError> {
        self.oldest_first(&format!("card={card}")).await
    }

    /// The decisions recorded for the tool calls of a run, oldest first.
    pub async fn tool_judgements(&self, run: i64) -> Result<Vec<Judgement>, ClientError> {
        self.oldest_first(&format!("run={run}&kind=tool_call"))
            .await
    }

    async fn oldest_first(&self, filter: &str) -> Result<Vec<Judgement>, ClientError> {
        let mut judgements: Vec<Judgement> = self
            .get(&format!("/v1/judgements?{filter}&limit=1000"))
            .await?;
        judgements.reverse();
        Ok(judgements)
    }

    /// The agents a chat can be with, the Moebius agent first.
    pub async fn agents(&self) -> Result<Vec<Agent>, ClientError> {
        self.get("/v1/agents").await
    }

    /// The upstreams, agents, teacher and decision model of the server.
    pub async fn models(&self) -> Result<Models, ClientError> {
        self.get("/v1/models").await
    }

    /// The models the upstreams serve; `refresh` asks them again instead of using what the server kept.
    pub async fn available_models(&self, refresh: bool) -> Result<Available, ClientError> {
        let path = if refresh {
            "/v1/models/available?refresh=true"
        } else {
            "/v1/models/available"
        };
        self.get(path).await
    }

    /// The plugins the server has loaded.
    pub async fn plugins(&self) -> Result<Plugins, ClientError> {
        self.get("/v1/plugins").await
    }

    /// Whether the server is ready for a first card, step by step.
    pub async fn start(&self) -> Result<Start, ClientError> {
        self.get("/v1/start").await
    }

    /// What the server runs with.
    pub async fn server_settings(&self) -> Result<ServerSettings, ClientError> {
        self.get("/v1/settings").await
    }

    /// Changes `moebius.toml`: the dotted keys to set to a value and those to remove.
    pub async fn save_settings(
        &self,
        set: serde_json::Map<String, serde_json::Value>,
        remove: Vec<String>,
    ) -> Result<SavedSettings, ClientError> {
        let body = serde_json::json!({ "set": set, "remove": remove });
        self.put("/v1/settings", body).await
    }

    /// Stores an API key under `name`, on the server only.
    pub async fn store_secret(&self, name: &str, key: &str) -> Result<Vec<String>, ClientError> {
        self.put(
            &format!("/v1/secrets/{name}"),
            serde_json::json!({ "value": key }),
        )
        .await
    }

    /// Asks the server to restart, which it answers before it does.
    pub async fn restart_server(&self) -> Result<(), ClientError> {
        let request = self
            .http
            .post(format!("{}/v1/server/restart", self.settings.address))
            .json(&serde_json::json!({}))
            .bearer_auth(&self.settings.token);
        let response = request
            .send()
            .await
            .map_err(|_| ClientError::Unreachable(self.settings.address.clone()))?;
        match response.status() {
            reqwest::StatusCode::UNAUTHORIZED => Err(ClientError::Unauthorized),
            status if status.is_success() => Ok(()),
            status => Err(ClientError::Server {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            }),
        }
    }

    /// An agent of the harness configuration repository, with its instructions.
    pub async fn agent(&self, name: &str) -> Result<AgentDetail, ClientError> {
        self.get(&format!("/v1/agents/{name}")).await
    }

    /// Adds or changes an agent; the server commits it to the harness configuration repository.
    pub async fn save_agent(
        &self,
        name: &str,
        settings: &AgentSettings,
        instructions: &str,
    ) -> Result<AgentDetail, ClientError> {
        let body = serde_json::json!({ "settings": settings, "instructions": instructions });
        self.put(&format!("/v1/agents/{name}"), body).await
    }

    /// Deletes an agent of the harness configuration repository.
    pub async fn delete_agent(&self, name: &str) -> Result<(), ClientError> {
        let request = self
            .http
            .delete(format!("{}/v1/agents/{name}", self.settings.address));
        self.send::<()>(request).await
    }

    /// The rules page: the built-in patterns, the rules in prose and those of each repository.
    pub async fn rules(&self) -> Result<RulesView, ClientError> {
        self.get("/v1/rules").await
    }

    /// Replaces the rules in prose; the server commits them to the harness configuration repository.
    pub async fn save_rules(&self, text: &str) -> Result<RulesView, ClientError> {
        self.put("/v1/rules", serde_json::json!({ "text": text }))
            .await
    }

    /// The skills an agent can name; none without a harness configuration repository.
    pub async fn skills(&self) -> Result<Vec<Skill>, ClientError> {
        self.get("/v1/skills").await
    }

    /// The MCP servers an agent can name.
    pub async fn mcp_servers(&self) -> Result<Vec<McpServer>, ClientError> {
        self.get("/v1/mcp-servers").await
    }

    /// A skill with its text.
    pub async fn skill(&self, name: &str) -> Result<SkillDetail, ClientError> {
        self.get(&format!("/v1/skills/{name}")).await
    }

    /// Adds or changes a skill; the server commits it to the harness configuration repository.
    pub async fn save_skill(
        &self,
        name: &str,
        description: &str,
        text: &str,
    ) -> Result<SkillDetail, ClientError> {
        let body = serde_json::json!({ "description": description, "text": text });
        self.put(&format!("/v1/skills/{name}"), body).await
    }

    /// Deletes a skill of the harness configuration repository.
    pub async fn delete_skill(&self, name: &str) -> Result<(), ClientError> {
        let request = self
            .http
            .delete(format!("{}/v1/skills/{name}", self.settings.address));
        self.send::<()>(request).await
    }

    /// Sends a zip file to the server, which keeps it where the Moebius agent can read it.
    pub async fn upload(&self, zip: Vec<u8>) -> Result<Upload, ClientError> {
        let request = self
            .http
            .post(format!("{}/v1/uploads", self.settings.address))
            .header(reqwest::header::CONTENT_TYPE, "application/zip")
            .body(zip);
        self.send(request).await
    }

    /// The directory `dir` of a repository's default branch; `""` is the root.
    pub async fn tree(&self, repository: i64, dir: &str) -> Result<Tree, ClientError> {
        self.get(&format!(
            "/v1/repositories/{repository}/tree?path={}",
            encoded(dir, false)
        ))
        .await
    }

    /// A file of a repository's default branch.
    pub async fn file(&self, repository: i64, path: &str) -> Result<FileDetail, ClientError> {
        self.get(&file_path(repository, path)).await
    }

    /// Writes a file of a repository; the server commits it on the default branch.
    pub async fn save_file(
        &self,
        repository: i64,
        path: &str,
        text: &str,
    ) -> Result<FileDetail, ClientError> {
        let body = serde_json::json!({ "text": text });
        self.put(&file_path(repository, path), body).await
    }

    /// The script a column runs for a repository.
    pub async fn script(&self, repository: i64, column: &str) -> Result<ScriptDetail, ClientError> {
        self.get(&script_path(repository, column)).await
    }

    /// Saves the repository's own script for a column; the server commits it.
    pub async fn save_script(
        &self,
        repository: i64,
        column: &str,
        text: &str,
    ) -> Result<ScriptDetail, ClientError> {
        let body = serde_json::json!({ "text": text });
        self.put(&script_path(repository, column), body).await
    }

    /// Removes the repository's own script for a column, so that the built-in one runs.
    pub async fn delete_script(
        &self,
        repository: i64,
        column: &str,
    ) -> Result<ScriptDetail, ClientError> {
        let request = self.http.delete(format!(
            "{}{}",
            self.settings.address,
            script_path(repository, column)
        ));
        self.send(request).await
    }

    /// An MCP server as `mcp.toml` has it.
    pub async fn mcp_server(&self, name: &str) -> Result<McpServerDetail, ClientError> {
        self.get(&format!("/v1/mcp-servers/{name}")).await
    }

    /// Adds or changes an MCP server; the server commits it to the harness configuration repository.
    pub async fn save_mcp_server(
        &self,
        name: &str,
        definition: &McpDefinition,
    ) -> Result<McpServerDetail, ClientError> {
        let body = serde_json::to_value(definition).expect("a definition is JSON");
        self.put(&format!("/v1/mcp-servers/{name}"), body).await
    }

    /// Deletes an MCP server of the harness configuration repository.
    pub async fn delete_mcp_server(&self, name: &str) -> Result<(), ClientError> {
        let request = self
            .http
            .delete(format!("{}/v1/mcp-servers/{name}", self.settings.address));
        self.send::<()>(request).await
    }

    /// What the model calls used in the last `days` days.
    pub async fn usage(&self, days: i64) -> Result<UsageReport, ClientError> {
        self.get(&format!("/v1/usage?days={days}")).await
    }

    /// The Retros, the latest day first.
    pub async fn retros(&self) -> Result<Vec<RetroSummary>, ClientError> {
        self.get("/v1/retros").await
    }

    pub async fn retro(&self, date: &str) -> Result<RetroView, ClientError> {
        self.get(&format!("/v1/retros/{date}")).await
    }

    /// Runs today's Retro now.
    pub async fn start_retro(&self) -> Result<RetroView, ClientError> {
        self.post("/v1/retros", serde_json::json!({})).await
    }

    /// The proposals of all Retros, the newest first.
    pub async fn proposals(&self) -> Result<Vec<Proposal>, ClientError> {
        self.get("/v1/proposals").await
    }

    /// How well the decision model agrees with the recorded choices, and its takeovers.
    pub async fn shadowing(&self) -> Result<Shadowing, ClientError> {
        self.get("/v1/decision-model").await
    }

    pub async fn judgement_kinds(&self) -> Result<Vec<KindCounts>, ClientError> {
        self.get("/v1/judgements/kinds").await
    }

    /// The training-set exports, the newest first.
    pub async fn exports(&self) -> Result<Vec<Export>, ClientError> {
        self.get("/v1/exports").await
    }

    /// Writes an export of every kind on the server.
    pub async fn start_export(&self) -> Result<Export, ClientError> {
        self.post("/v1/exports", serde_json::json!({})).await
    }

    /// All research, the newest first.
    pub async fn researches(&self) -> Result<Vec<Research>, ClientError> {
        self.get("/v1/research").await
    }

    /// Writes the brief of a draft anew.
    pub async fn edit_research(
        &self,
        research: i64,
        goal: &str,
        measure: &str,
        better: &str,
        token_budget: i64,
    ) -> Result<Research, ClientError> {
        let body = serde_json::json!({
            "goal": goal, "measure": measure, "better": better, "token_budget": token_budget,
        });
        self.put(&format!("/v1/research/{research}"), body).await
    }

    /// Stores a brief as a draft.
    pub async fn create_research(
        &self,
        repository: i64,
        goal: &str,
        measure: &str,
        better: &str,
        token_budget: i64,
    ) -> Result<Research, ClientError> {
        let body = serde_json::json!({
            "repository": repository, "goal": goal, "measure": measure,
            "better": better, "token_budget": token_budget,
        });
        self.post("/v1/research", body).await
    }

    /// What a person does with a research: `confirm`, `start` or `stop`.
    pub async fn act_on_research(
        &self,
        research: i64,
        action: &str,
    ) -> Result<Research, ClientError> {
        let path = format!("/v1/research/{research}/{action}");
        self.post(&path, serde_json::json!({})).await
    }

    /// Adds a repository; the server clones it.
    pub async fn add_repository(
        &self,
        repository: &NewRepository,
    ) -> Result<Repository, ClientError> {
        let body =
            serde_json::to_value(repository).map_err(|e| ClientError::Decode(e.to_string()))?;
        self.post("/v1/repositories", body).await
    }

    /// Changes a card's title and body; on a forge the server changes the issue there too.
    pub async fn edit_card(&self, card: i64, title: &str, body: &str) -> Result<Card, ClientError> {
        let path = format!("/v1/cards/{card}");
        self.put(&path, serde_json::json!({ "title": title, "body": body }))
            .await
    }

    /// Replaces the GitHub logins whose issues on a repository need no approval.
    pub async fn edit_repository(
        &self,
        repository: i64,
        trusted: &[String],
    ) -> Result<Repository, ClientError> {
        let path = format!("/v1/repositories/{repository}");
        self.put(&path, serde_json::json!({ "trusted": trusted }))
            .await
    }

    /// Creates an issue in a repository.
    pub async fn create_issue(
        &self,
        repository: i64,
        title: &str,
        body: &str,
    ) -> Result<Card, ClientError> {
        let path = format!("/v1/repositories/{repository}/issues");
        self.post(&path, serde_json::json!({ "title": title, "body": body }))
            .await
    }

    pub async fn history(&self, card: i64) -> Result<Vec<HistoryEntry>, ClientError> {
        self.get(&format!("/v1/cards/{card}/history")).await
    }

    pub async fn steer(&self, run: i64, message: &str) -> Result<Run, ClientError> {
        self.post(
            &format!("/v1/runs/{run}/steer"),
            serde_json::json!({ "message": message }),
        )
        .await
    }

    /// What a person does with an inbox item. The answers are not read: the snapshot shows what changed.
    async fn decide(&self, path: String, body: serde_json::Value) -> Result<(), ClientError> {
        self.post::<serde_json::Value>(&path, body).await.map(drop)
    }

    /// Merges an open change.
    pub async fn merge(&self, change: i64) -> Result<(), ClientError> {
        self.decide(format!("/v1/changes/{change}/merge"), serde_json::json!({}))
            .await
    }

    /// Sends a change back to coding with the reason as the card's note.
    pub async fn reject(&self, change: i64, reason: &str) -> Result<(), ClientError> {
        let body = serde_json::json!({ "reason": reason });
        self.decide(format!("/v1/changes/{change}/reject"), body)
            .await
    }

    /// Approves an outside issue that waits in the backlog.
    pub async fn approve(&self, card: i64) -> Result<(), ClientError> {
        self.decide(format!("/v1/cards/{card}/approve"), serde_json::json!({}))
            .await
    }

    /// Gives a stuck card, or one whose pull request was closed, a fresh start.
    pub async fn retry(&self, card: i64) -> Result<(), ClientError> {
        self.decide(format!("/v1/cards/{card}/retry"), serde_json::json!({}))
            .await
    }

    /// Merges a proposal of the Retro into the harness configuration.
    pub async fn merge_proposal(&self, proposal: i64) -> Result<(), ClientError> {
        let path = format!("/v1/proposals/{proposal}/merge");
        self.decide(path, serde_json::json!({})).await
    }

    /// Dismisses a proposal of the Retro.
    pub async fn dismiss_proposal(&self, proposal: i64) -> Result<(), ClientError> {
        let path = format!("/v1/proposals/{proposal}/dismiss");
        self.decide(path, serde_json::json!({})).await
    }

    /// Lets the decision model take a kind over.
    pub async fn merge_takeover(&self, takeover: i64) -> Result<(), ClientError> {
        let path = format!("/v1/takeovers/{takeover}/merge");
        self.decide(path, serde_json::json!({})).await
    }

    /// Refuses a takeover proposal.
    pub async fn dismiss_takeover(&self, takeover: i64) -> Result<(), ClientError> {
        let path = format!("/v1/takeovers/{takeover}/dismiss");
        self.decide(path, serde_json::json!({})).await
    }

    /// Hands a kind that the decision model took over back to its decider.
    pub async fn release_takeover(&self, takeover: i64) -> Result<(), ClientError> {
        let path = format!("/v1/takeovers/{takeover}/release");
        self.decide(path, serde_json::json!({})).await
    }

    /// Confirms a grilling with the issues as the person left them: a card is made of each.
    pub async fn confirm_grilling(
        &self,
        grilling: i64,
        issues: &[GrillingIssue],
    ) -> Result<(), ClientError> {
        let path = format!("/v1/grillings/{grilling}/confirm");
        self.decide(path, serde_json::json!({ "issues": issues }))
            .await
    }

    /// Dismisses the issues or the report of a grilling.
    pub async fn dismiss_grilling(&self, grilling: i64) -> Result<(), ClientError> {
        let path = format!("/v1/grillings/{grilling}/dismiss");
        self.decide(path, serde_json::json!({})).await
    }

    /// The chats, newest first, those of the floating window included.
    pub async fn chats(&self) -> Result<Vec<Chat>, ClientError> {
        self.get("/v1/chats?all=true").await
    }

    /// Starts a chat with `agent` (the Moebius agent when blank), in `repository` if given, or in the harness
    /// configuration repository when `harness` is set.
    pub async fn start_chat(
        &self,
        agent: &str,
        message: &str,
        repository: Option<i64>,
        harness: bool,
    ) -> Result<Run, ClientError> {
        let mut body =
            serde_json::json!({ "message": message, "repository": repository, "harness": harness });
        if !agent.trim().is_empty() {
            body["agent"] = agent.trim().into();
        }
        self.post("/v1/chats", body).await
    }

    /// Starts a chat with the Moebius agent in `repository`, or in none, that the floating chat shows: the agent is
    /// also told `context` (blank for none), where the person is in the app, which the chat does not show. The chat's
    /// origin is `assistant`, so the Chat page leaves it out.
    pub async fn ask(
        &self,
        message: &str,
        repository: Option<i64>,
        context: &str,
    ) -> Result<Run, ClientError> {
        let body = serde_json::json!({
            "agent": crate::chat::DEFAULT_AGENT,
            "message": message,
            "repository": repository,
            "context": context,
            "origin": ASSISTANT,
        });
        self.post("/v1/chats", body).await
    }

    /// Starts a chat that continues the ended chat `chat`: its agent and repository, told what was said in it.
    pub async fn continue_chat(&self, chat: i64, message: &str) -> Result<Run, ClientError> {
        self.post(
            "/v1/chats",
            serde_json::json!({ "message": message, "continues": chat }),
        )
        .await
    }

    /// Says that the person has read the chat up to the event `seq`.
    pub async fn mark_seen(&self, chat: i64, seq: i64) -> Result<(), ClientError> {
        let _: serde_json::Value = self
            .post(
                &format!("/v1/chats/{chat}/seen"),
                serde_json::json!({ "seq": seq }),
            )
            .await?;
        Ok(())
    }

    /// Writes to a chat; a working agent gets the message as a follow-up. The agent is also told `context`, where the
    /// person is in the app, when there is one.
    pub async fn say(
        &self,
        chat: i64,
        message: &str,
        context: Option<&str>,
    ) -> Result<Run, ClientError> {
        self.post(
            &format!("/v1/chats/{chat}/messages"),
            serde_json::json!({ "message": message, "context": context }),
        )
        .await
    }

    /// Offers what a chat's worktree changes as a change; the chat goes on.
    pub async fn offer_chat(&self, chat: i64) -> Result<Change, ClientError> {
        self.post(&format!("/v1/chats/{chat}/offer"), serde_json::json!({}))
            .await
    }

    /// Ends a chat: its work is committed and offered as a change.
    pub async fn end_chat(&self, chat: i64) -> Result<Run, ClientError> {
        self.post(&format!("/v1/chats/{chat}/end"), serde_json::json!({}))
            .await
    }

    pub async fn stop(&self, run: i64) -> Result<Run, ClientError> {
        self.post(&format!("/v1/runs/{run}/stop"), serde_json::json!({}))
            .await
    }
}

/// What a shell sends the app.
#[derive(Debug, Clone)]
pub enum TerminalEvent {
    /// The socket is open; what is sent to the channel goes to the shell.
    Connected(mpsc::Sender<TerminalInput>),
    /// What the shell printed.
    Output(Vec<u8>),
    /// The shell exited or the socket could not be opened or ended; nothing follows.
    Closed(String),
}

/// What the app sends a shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalInput {
    Bytes(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

/// A shell on the WebSocket `path` (`/v1/runs/{id}/terminal` for a run's worktree, `/v1/terminal` for the server) for as
/// long as the stream is held: dropping it closes the socket, which kills the shell.
pub fn terminal(
    settings: Settings,
    path: String,
) -> impl iced::futures::Stream<Item = TerminalEvent> {
    iced::stream::channel(64, async move |mut out: mpsc::Sender<TerminalEvent>| {
        let mut socket = match connect(&settings, &path).await {
            Ok(socket) => socket,
            Err(e) => {
                let reason = match e {
                    tungstenite::Error::Http(response) => format!(
                        "the server answered {}: {}",
                        response.status().as_u16(),
                        String::from_utf8_lossy(response.body().as_deref().unwrap_or_default())
                    ),
                    other => other.to_string(),
                };
                let _ = out.send(TerminalEvent::Closed(reason)).await;
                return;
            }
        };
        let (input, mut typed) = mpsc::channel(64);
        if out.send(TerminalEvent::Connected(input)).await.is_err() {
            return;
        }
        let ended = loop {
            tokio::select! {
                message = socket.next() => match message {
                    Some(Ok(Message::Binary(bytes))) => {
                        if out.send(TerminalEvent::Output(bytes.to_vec())).await.is_err() {
                            return;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break "the shell ended".to_string(),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => break e.to_string(),
                },
                sent = typed.next() => {
                    let Some(sent) = sent else { return };
                    let message = match sent {
                        TerminalInput::Bytes(bytes) => Message::Binary(bytes.into()),
                        TerminalInput::Resize { cols, rows } => Message::Text(
                            serde_json::json!({"type": "resize", "cols": cols, "rows": rows})
                                .to_string()
                                .into(),
                        ),
                    };
                    if let Err(e) = socket.send(message).await {
                        break e.to_string();
                    }
                }
            }
        };
        let _ = out.send(TerminalEvent::Closed(ended)).await;
    })
}

/// How long to wait before trying to connect again, after the `failures`-th failure in a row.
fn backoff(failures: u32) -> Duration {
    Duration::from_secs(1 << failures.min(5)).min(Duration::from_secs(30))
}

/// The event stream of the server: `Connected` once the socket is open, then every event, and `Disconnected` when it
/// ends or cannot be opened, after which it connects again with a growing wait. Events are not replayed, so the app
/// reads the snapshot on every `Connected`.
pub fn events(settings: Settings) -> impl iced::futures::Stream<Item = StreamEvent> {
    iced::stream::channel(64, async move |mut out: mpsc::Sender<StreamEvent>| {
        let mut failures = 0;
        loop {
            match connect(&settings, "/v1/events").await {
                Ok(mut socket) => {
                    failures = 0;
                    let _ = out.send(StreamEvent::Connected).await;
                    let ended = loop {
                        match socket.next().await {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(event) = serde_json::from_str(text.as_str()) {
                                    let _ = out.send(StreamEvent::Event(event)).await;
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                break "the server closed the connection".to_string();
                            }
                            Some(Ok(_)) => {}
                            Some(Err(e)) => break e.to_string(),
                        }
                    };
                    let _ = out.send(StreamEvent::Disconnected(ended)).await;
                }
                Err(tungstenite::Error::Http(response)) if response.status() == 401 => {
                    let _ = out.send(StreamEvent::Unauthorized).await;
                    return;
                }
                Err(e) => {
                    let _ = out.send(StreamEvent::Disconnected(e.to_string())).await;
                }
            }
            tokio::time::sleep(backoff(failures)).await;
            failures += 1;
        }
    })
}

async fn connect(
    settings: &Settings,
    path: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Error,
> {
    let url = settings.address.replacen("http", "ws", 1) + path;
    let mut request = url.into_client_request()?;
    let bearer = format!("Bearer {}", settings.token)
        .parse()
        .map_err(|_| tungstenite::Error::Utf8("the token is not a header value".into()))?;
    request.headers_mut().insert("authorization", bearer);
    tokio_tungstenite::connect_async(request)
        .await
        .map(|(socket, _)| socket)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The snapshot of the reference, with a field the app does not know and a card without the optional fields.
    const SNAPSHOT: &str = r#"{
      "boards": [
        {"repository": {"id": 1, "name": "demo", "path": "/srv/.moebius/repos/demo", "forge": "none",
                        "default_branch": "main", "github": null, "trusted": []},
         "cards": [{"id": 1, "repository_id": 1, "number": 1, "title": "Add a feature", "body": "", "author": "person",
                    "column": "Backlog", "created_at": 1790000000, "added_later": true},
                   {"id": 2, "repository_id": 1, "number": 2, "title": "Work", "column": "In progress",
                    "rounds": 2, "tokens": 900, "blocked_by": [1], "refusals": 1,
                    "run": {"id": 4, "agent": "Coder", "purpose": "code", "status": "running"},
                    "change": {"id": 3, "state": "open", "checks": "pending", "review_verdict": null}}],
         "round_limit": 5,
         "waiting": {"1": "needs a person"},
         "changes": []}
      ],
      "inbox": [{"id": 7, "kind": "approve_issue", "card_id": 1, "title": "Add a feature",
                 "data": {"reason": "new author"}, "resolved_at": null, "at": 1790000001}]
    }"#;

    #[test]
    fn the_snapshot_of_the_reference_is_read() {
        let snapshot: Snapshot = serde_json::from_str(SNAPSHOT).unwrap();
        let board = &snapshot.boards[0];
        assert_eq!(board.repository.name, "demo");
        assert_eq!(board.repository.github, None);
        assert_eq!(board.cards[0].column, "Backlog");
        assert_eq!(board.cards[0].note, None);
        assert_eq!(board.waiting[&1], "needs a person");
        assert_eq!(
            (board.cards[0].rounds, board.cards[0].run.as_ref()),
            (0, None)
        );
        let work = &board.cards[1];
        assert_eq!((work.rounds, work.tokens, work.refusals), (2, 900, 1));
        assert_eq!(work.blocked_by, [1]);
        assert_eq!(work.change.as_ref().unwrap().checks, "pending");
        assert_eq!((board.round_limit, board.live_runs()), (5, 1));
        assert_eq!(snapshot.inbox[0].kind, "approve_issue");
    }

    #[test]
    fn missing_parts_of_the_snapshot_are_empty() {
        let snapshot: Snapshot = serde_json::from_str("{}").unwrap();
        assert_eq!(snapshot, Snapshot::default());
    }

    #[test]
    fn an_event_keeps_its_kind_and_data() {
        let event: ServerEvent = serde_json::from_str(
            r#"{"kind":"card_moved","data":{"card":1,"from":"Ready","to":"In progress"}}"#,
        )
        .unwrap();
        assert_eq!(event.kind, "card_moved");
        assert_eq!(event.data["to"], "In progress");
    }

    #[test]
    fn the_wait_before_connecting_again_grows_to_a_limit() {
        let waits: Vec<u64> = (0..8).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30, 30]);
    }
}
