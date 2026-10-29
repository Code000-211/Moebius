//! The Improve pages' answers: the Retro's reports and proposals, autoresearch, the decision model's shadowing and
//! takeovers, training-set exports and usage.

use serde::Deserialize;

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

/// A change that a Retro proposes: to the harness configuration, on a branch, or to Moebius's own configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Proposal {
    pub id: i64,
    /// `harness`, `settings` (a change to `moebius.toml`) or `repository` (a change to a repository's `.moebius/`).
    #[serde(default = "harness")]
    pub kind: String,
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
    /// What a `settings` or `repository` proposal changes, as a unified diff.
    #[serde(default)]
    pub diff: String,
}

fn harness() -> String {
    "harness".into()
}

/// A proposal tried on one card.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Trial {
    /// The card it was tried on; none when the proposal names no cards.
    #[serde(default)]
    pub card_id: Option<i64>,
    /// Whether the card is one of the regression set rather than one whose errors the proposal addresses.
    #[serde(default)]
    pub regression: bool,
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
    /// What making tool outputs smaller saved; nothing from a server that does not say.
    #[serde(default)]
    pub tool_output: ToolOutputSaving,
}

/// How many tool results were made smaller in the window, and their bytes before and after.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct ToolOutputSaving {
    pub results: i64,
    pub bytes_before: i64,
    pub bytes_after: i64,
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
