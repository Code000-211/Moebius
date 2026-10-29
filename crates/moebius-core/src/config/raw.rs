//! `moebius.toml` as it is written: every key optional, unknown keys refused. `Config::parse` makes a `Config` of it.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use serde::Deserialize;

use super::{AgentConfig, PluginConfig, RunnerConfig, UpstreamConfig, default_protocol};
use crate::router::{Classes, Protocol};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawWindow {
    pub(super) start: Option<String>,
    pub(super) end: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawBudgets {
    pub(super) day_tokens: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawChecks {
    pub(super) timeout_seconds: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawRegression {
    pub(super) size: Option<u32>,
    pub(super) gate: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawBoard {
    pub(super) tick_ms: Option<u64>,
    pub(super) round_limit: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawRouter {
    pub(super) listen: Option<SocketAddr>,
    #[serde(default)]
    pub(super) upstreams: Vec<UpstreamConfig>,
    #[serde(default)]
    pub(super) classes: Classes,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawPiBolt {
    pub(super) program: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawClaudeCode {
    pub(super) program: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawHarness {
    pub(super) url: String,
    #[serde(default)]
    pub(super) mcp_env: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawGitLab {
    pub(super) url: Option<String>,
    pub(super) token_file: Option<PathBuf>,
    pub(super) token_secret: Option<String>,
    pub(super) poll_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawForgejo {
    pub(super) url: Option<String>,
    pub(super) token_file: Option<PathBuf>,
    pub(super) token_secret: Option<String>,
    pub(super) poll_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawGitHub {
    pub(super) app_id: u64,
    pub(super) private_key_file: PathBuf,
    pub(super) api_url: Option<String>,
    pub(super) poll_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawTeacher {
    pub(super) model: String,
    #[serde(default = "default_protocol")]
    pub(super) protocol: Protocol,
    pub(super) timeout_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawDecisionModel {
    pub(super) url: String,
    pub(super) model: String,
    pub(super) api_key_env: Option<String>,
    pub(super) api_key_secret: Option<String>,
    pub(super) timeout_seconds: Option<u64>,
    pub(super) shadow: Option<Vec<String>>,
    pub(super) agreement: Option<f64>,
    pub(super) min_judgements: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Raw {
    pub(super) data_dir: Option<PathBuf>,
    pub(super) listen: Option<SocketAddr>,
    pub(super) token_file: Option<PathBuf>,
    pub(super) run_limit: Option<u32>,
    pub(super) retro_at: Option<String>,
    pub(super) trial_window: Option<RawWindow>,
    #[serde(default)]
    pub(super) regression: RawRegression,
    #[serde(default)]
    pub(super) board: RawBoard,
    #[serde(default)]
    pub(super) budgets: RawBudgets,
    #[serde(default)]
    pub(super) checks: RawChecks,
    #[serde(default)]
    pub(super) router: RawRouter,
    #[serde(default)]
    pub(super) pi_bolt: RawPiBolt,
    #[serde(default)]
    pub(super) claude_code: RawClaudeCode,
    pub(super) rules_file: Option<PathBuf>,
    pub(super) harness: Option<RawHarness>,
    pub(super) github: Option<RawGitHub>,
    pub(super) gitlab: Option<RawGitLab>,
    pub(super) forgejo: Option<RawForgejo>,
    pub(super) teacher: Option<RawTeacher>,
    pub(super) decision_model: Option<RawDecisionModel>,
    #[serde(default)]
    pub(super) plugins: Vec<PluginConfig>,
    #[serde(default)]
    pub(super) runners: Vec<RunnerConfig>,
    #[serde(default)]
    pub(super) agents: BTreeMap<String, AgentConfig>,
}
