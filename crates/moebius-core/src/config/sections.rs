//! The types of the configuration's sections as the server uses them: the router's upstreams, the GitHub App, the
//! teacher, the decision model, the plugins in their own process and the agents, with times of day and the checks of names, keys and timeouts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::ConfigError;
use super::raw::{RawDecisionModel, RawForgejo, RawGitLab, RawTeacher};
use crate::harness::mcp::{McpServer, valid_variable};
use crate::router::{OAuthConfig, Protocol};

/// Whether `name` can name an agent: letters, digits, `-` and `_`, and not empty.
pub fn valid_agent_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Refuses a key that is given twice, or a secret whose name is not one.
pub(super) fn check_key(
    key: &'static str,
    env: Option<&str>,
    secret: Option<&str>,
) -> Result<(), ConfigError> {
    if env.is_some() && secret.is_some() {
        return Err(ConfigError::KeyTwice { key });
    }
    match secret.filter(|name| !valid_agent_name(name)) {
        Some(name) => Err(ConfigError::KeySecret {
            key,
            name: name.to_string(),
        }),
        None => Ok(()),
    }
}

/// A time of day as minutes after midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOfDay(pub u16);

impl TimeOfDay {
    pub(super) fn parse(key: &'static str, value: &str) -> Result<Self, ConfigError> {
        let bad = || ConfigError::Time {
            key,
            value: value.to_string(),
        };
        let (h, m) = value.split_once(':').ok_or_else(bad)?;
        if h.len() != 2 || m.len() != 2 {
            return Err(bad());
        }
        let (h, m): (u16, u16) = (h.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?);
        if h > 23 || m > 59 {
            return Err(bad());
        }
        Ok(Self(h * 60 + m))
    }
}

/// How long the plugin host lets a decider take (see `server`), in seconds. The teacher must answer sooner.
pub const PLUGIN_TIME_LIMIT_SECONDS: u64 = 30;

/// One upstream of the router.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamConfig {
    pub protocol: Protocol,
    pub base_url: String,
    /// The environment variable holding the provider's key.
    pub api_key_env: Option<String>,
    /// The name of the secret holding the provider's key (see `settings`); not set together with `api_key_env`.
    pub api_key_secret: Option<String>,
    /// How the upstream logs in with OAuth instead of a key; not set together with a key.
    pub oauth: Option<OAuthConfig>,
}

/// The GitHub App Moebius acts as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubConfig {
    pub app_id: u64,
    pub private_key_file: PathBuf,
    pub api_url: String,
    /// How often the issues of each GitHub repository are polled.
    pub poll: Duration,
}

/// The GitLab Moebius reaches with an access token: gitlab.com or one hosted elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabConfig {
    /// The GitLab's web address, such as `https://gitlab.com`, without a trailing `/`; its API is at `<url>/api/v4`.
    pub url: String,
    /// Where the access token is read from when the server starts.
    pub token: TokenSource,
    /// How often the issues and merge requests of each GitLab repository are polled.
    pub poll: Duration,
}

/// Where a token is kept: a file (made absolute against the configuration's directory) or a secret (see `settings`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenSource {
    File(PathBuf),
    Secret(String),
}

/// The GitLab of `[gitlab]`, with its defaults: exactly one of `token_file` and `token_secret`.
pub(super) fn gitlab(raw: RawGitLab, base: &std::path::Path) -> Result<GitLabConfig, ConfigError> {
    let token = match (raw.token_file, raw.token_secret) {
        (Some(file), None) => TokenSource::File(base.join(file)),
        (None, Some(name)) if valid_agent_name(&name) => TokenSource::Secret(name),
        (None, Some(name)) => return Err(ConfigError::GitLabSecret { name }),
        _ => return Err(ConfigError::GitLabToken),
    };
    let url = raw.url.unwrap_or_else(|| "https://gitlab.com".to_string());
    let url = url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(ConfigError::GitLabUrl);
    }
    Ok(GitLabConfig {
        url,
        token,
        poll: Duration::from_secs(raw.poll_seconds.unwrap_or(30).max(1)),
    })
}

/// The Forgejo or Gitea Moebius reaches with an access token: Codeberg, or one hosted elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgejoConfig {
    /// The forge's web address, such as `https://codeberg.org`, without a trailing `/`; its API is at `<url>/api/v1`.
    pub url: String,
    /// Where the access token is read from when the server starts.
    pub token: TokenSource,
    /// How often the issues and pull requests of each repository on it are polled.
    pub poll: Duration,
}

/// The Forgejo of `[forgejo]`, with its defaults: exactly one of `token_file` and `token_secret`.
pub(super) fn forgejo(
    raw: RawForgejo,
    base: &std::path::Path,
) -> Result<ForgejoConfig, ConfigError> {
    let token = match (raw.token_file, raw.token_secret) {
        (Some(file), None) => TokenSource::File(base.join(file)),
        (None, Some(name)) if valid_agent_name(&name) => TokenSource::Secret(name),
        (None, Some(name)) => return Err(ConfigError::ForgejoSecret { name }),
        _ => return Err(ConfigError::ForgejoToken),
    };
    let url = raw
        .url
        .unwrap_or_else(|| "https://codeberg.org".to_string());
    let url = url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(ConfigError::ForgejoUrl);
    }
    Ok(ForgejoConfig {
        url,
        token,
        poll: Duration::from_secs(raw.poll_seconds.unwrap_or(30).max(1)),
    })
}

/// Whether a code run is told which files are likely relevant, and how many (see `decide::localize`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalizeConfig {
    /// Whether the hint is asked for at the start of a code run; a repository can turn it on or off for itself.
    pub enabled: bool,
    /// The most files a hint names, from 1 to [`MAX_LOCALIZE_FILES`].
    pub files: usize,
}

/// The most files a hint can name.
pub const MAX_LOCALIZE_FILES: usize = 10;

impl Default for LocalizeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            files: MAX_LOCALIZE_FILES,
        }
    }
}

/// The teacher model, which judges the rules written in prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeacherConfig {
    pub model: String,
    pub protocol: Protocol,
    /// How long the teacher may take to answer.
    pub timeout: Duration,
}

/// The decision model, which shadows the teacher and may take kinds over (see `decision_model`).
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionModelConfig {
    /// The OpenAI-compatible base URL, without a trailing slash.
    pub url: String,
    pub model: String,
    /// The environment variable holding the key, if the endpoint needs one.
    pub api_key_env: Option<String>,
    /// The name of the secret holding the key; not set together with `api_key_env`.
    pub api_key_secret: Option<String>,
    /// How long the model may take to answer.
    pub timeout: Duration,
    /// The kinds to shadow; all of them when none are named.
    pub shadow: Option<Vec<String>>,
    /// The share of equal choices a takeover needs, above 0 and at most 1.
    pub agreement: f64,
    /// How many shadowed judgements of a kind a takeover needs.
    pub min_judgements: i64,
}

/// A plugin in its own process, from `[[plugins]]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginConfig {
    /// The name the plugin is known by, which its manifest must say too.
    pub name: String,
    /// The program: a bare name is looked up on the PATH, anything else is made absolute against the file's directory.
    pub path: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    /// The kinds of decision it answers.
    #[serde(default)]
    pub decides: Vec<String>,
}

/// A machine that runs harnesses over SSH, from `[[runners]]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerConfig {
    /// The name an agent's `runner` and a column script name it by.
    pub name: String,
    /// The machine's host name or address.
    pub host: String,
    /// The user to log in as; without one, the SSH configuration's.
    pub user: Option<String>,
    /// The SSH port; without one, the SSH configuration's.
    pub port: Option<u16>,
    /// The private key to log in with, made absolute against the file's directory; without one, the SSH
    /// configuration's.
    pub key_file: Option<PathBuf>,
    /// The absolute directory on the machine that holds the clones and the runs' directories.
    pub working_dir: String,
    /// How many runs may go on the machine at once.
    #[serde(default = "default_max_runs")]
    pub max_runs: u32,
    /// The directory on the machine that holds `pi-bolt` and `claude`; without one they are looked up on its `PATH`.
    pub bin_dir: Option<String>,
    /// The `ssh` program, looked up on the `PATH` when it is a bare name.
    #[serde(default = "default_ssh")]
    pub ssh: PathBuf,
}

fn default_max_runs() -> u32 {
    1
}

fn default_ssh() -> PathBuf {
    PathBuf::from("ssh")
}

/// The runners, checked, with their key files and `ssh` programs resolved like `pi_bolt.program`.
pub(super) fn runners(
    mut runners: Vec<RunnerConfig>,
    base: &Path,
) -> Result<Vec<RunnerConfig>, ConfigError> {
    let mut names = std::collections::BTreeSet::new();
    for runner in &mut runners {
        let name = runner.name.clone();
        if !valid_agent_name(&name) {
            return Err(ConfigError::RunnerName { name });
        }
        if !names.insert(name.clone()) {
            return Err(ConfigError::RunnerTwice { name });
        }
        let word = |text: &str| {
            !text.is_empty()
                && !text.starts_with('-')
                && !text.contains(|c: char| c.is_whitespace())
        };
        if !word(&runner.host) || !runner.user.as_deref().is_none_or(word) {
            return Err(ConfigError::RunnerHost { name });
        }
        if !runner.working_dir.starts_with('/') || runner.working_dir.contains(['\'', '\n', '\0']) {
            return Err(ConfigError::RunnerDir { name });
        }
        if runner
            .bin_dir
            .as_deref()
            .is_some_and(|dir| !dir.starts_with('/') || dir.contains(['\'', '\n', '\0']))
        {
            return Err(ConfigError::RunnerBinDir { name });
        }
        if runner.max_runs == 0 {
            return Err(ConfigError::RunnerMaxRuns { name });
        }
        if let Some(key) = &mut runner.key_file {
            *key = base.join(&key);
        }
        if runner.ssh.components().count() > 1 {
            runner.ssh = base.join(&runner.ssh);
        }
    }
    Ok(runners)
}

/// An agent as a run uses it: from `[agents.<name>]` or from the harness configuration repository.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    pub model: String,
    #[serde(default = "default_protocol")]
    pub protocol: Protocol,
    pub thinking: Option<String>,
    pub instructions: Option<String>,
    #[serde(default)]
    pub skills: Vec<PathBuf>,
    #[serde(default)]
    pub extensions: Vec<PathBuf>,
    /// The MCP servers the agent is given, by name. Only the harness configuration repository sets them.
    #[serde(skip)]
    pub mcp: BTreeMap<String, McpServer>,
    pub token_budget: Option<i64>,
    /// The harness the agent runs on.
    #[serde(default)]
    pub harness: HarnessKind,
    /// How the outputs of the agent's tools are kept small (`[agents.<name>.tool_output]`).
    #[serde(default)]
    pub tool_output: ToolOutput,
    /// The runner (`[[runners]]`) the agent's runs go to; without one they run on the server.
    #[serde(default)]
    pub runner: Option<String>,
    /// Whether the agent is given the engineering skills Moebius brings for it, with `moebius-autonomous` (see
    /// `harness::skill::engineering`). Only the Coder, the Reviewer and the Grilling partner have any.
    #[serde(default = "default_true")]
    pub engineering_skills: bool,
}

/// The default of the keys that are on unless they are turned off.
pub(crate) fn default_true() -> bool {
    true
}

/// How the outputs of an agent's tools are kept small before the model reads them: older ones hidden (by the bridge),
/// and each one compressed and cut to a budget where it is made (by `harness::output`). The bridge is given it as
/// the JSON of `MOEBIUS_TOOL_OUTPUT` and hands the budget back with each result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ToolOutput {
    /// The output of a tool call older than this many turns is hidden behind a placeholder; 0 keeps every output.
    pub hide_after_turns: u32,
    /// The most lines of one result; above it, its head and tail are kept. 0 is no limit.
    pub max_lines: u32,
    /// The most bytes of one result; above it, its head and tail are kept. 0 is no limit.
    pub max_bytes: u32,
    /// Whether the outputs of well-known commands (test runners, cargo's build logs) are compressed, and escape
    /// codes, progress bars and repeated lines removed.
    pub compress: bool,
}

impl Default for ToolOutput {
    fn default() -> Self {
        Self {
            hide_after_turns: 10,
            max_lines: 200,
            max_bytes: 16 * 1024,
            compress: true,
        }
    }
}

impl ToolOutput {
    /// Whether these are the defaults, which an agent's file need not say.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// The harness an agent runs on: Pi-Bolt, or Claude Code's command line, which has no bridge, so that the router
/// checks the tool calls of its model's answers (see `router`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessKind {
    #[default]
    PiBolt,
    ClaudeCode,
}

impl HarnessKind {
    /// The name the configuration, the API and a run's `harness` give it.
    pub fn name(self) -> &'static str {
        match self {
            HarnessKind::PiBolt => "pi-bolt",
            HarnessKind::ClaudeCode => "claude-code",
        }
    }

    /// Whether this is the default, which a file leaves out.
    pub fn is_default(&self) -> bool {
        *self == HarnessKind::PiBolt
    }

    /// Why an agent speaking `protocol`, with extensions or without, cannot run on this harness, if it cannot: Claude
    /// Code speaks only the anthropic protocol and loads no Pi extensions.
    pub fn refuses(self, protocol: Protocol, extensions: bool) -> Option<&'static str> {
        match self {
            HarnessKind::PiBolt => None,
            HarnessKind::ClaudeCode if protocol != Protocol::Anthropic => {
                Some("an agent on claude-code speaks the anthropic protocol")
            }
            HarnessKind::ClaudeCode if extensions => {
                Some("an agent on claude-code has no extensions: they are Pi-Bolt's")
            }
            HarnessKind::ClaudeCode => None,
        }
    }
}

pub(crate) fn default_protocol() -> Protocol {
    Protocol::OpenAi
}

/// The agents the harness can run, and why the others cannot be: an agent whose files in the harness configuration
/// repository are broken is left out and has its error here. Shared, since a refresh of the repository swaps it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentSet {
    pub agents: BTreeMap<String, AgentConfig>,
    /// Why each agent that is left out is, by name.
    pub errors: BTreeMap<String, String>,
    /// The engineering skills the harness configuration repository replaces with a skill of its own of the same name:
    /// the name, and where the skill is.
    pub skill_overrides: BTreeMap<String, PathBuf>,
}

/// The timeout of a decider, `seconds`, which must be at least 1 and below the plugin limit; else `error` with
/// `seconds` and the most it may be.
fn decider_timeout(
    seconds: u64,
    error: fn(u64, u64) -> ConfigError,
) -> Result<Duration, ConfigError> {
    if (1..PLUGIN_TIME_LIMIT_SECONDS).contains(&seconds) {
        Ok(Duration::from_secs(seconds))
    } else {
        Err(error(seconds, PLUGIN_TIME_LIMIT_SECONDS - 1))
    }
}

/// The teacher of `[teacher]`, with its defaults.
pub(super) fn teacher(raw: RawTeacher) -> Result<TeacherConfig, ConfigError> {
    let timeout = decider_timeout(raw.timeout_seconds.unwrap_or(20), |value, max| {
        ConfigError::TeacherTimeout { value, max }
    })?;
    Ok(TeacherConfig {
        model: raw.model,
        protocol: raw.protocol,
        timeout,
    })
}

/// The decision model of `[decision_model]`, with its defaults.
pub(super) fn decision_model(m: RawDecisionModel) -> Result<DecisionModelConfig, ConfigError> {
    let timeout = decider_timeout(m.timeout_seconds.unwrap_or(20), |value, max| {
        ConfigError::DecisionModelTimeout { value, max }
    })?;
    let agreement = m.agreement.unwrap_or(0.95);
    if !(agreement > 0.0 && agreement <= 1.0) {
        return Err(ConfigError::DecisionModelAgreement(agreement));
    }
    let min_judgements = m.min_judgements.unwrap_or(100);
    if min_judgements < 1 {
        return Err(ConfigError::DecisionModelMinJudgements);
    }
    if m.url.trim().is_empty() || m.model.trim().is_empty() {
        return Err(ConfigError::DecisionModelEmpty);
    }
    if let Some(name) = m.api_key_env.as_ref().filter(|n| !valid_variable(n)) {
        return Err(ConfigError::DecisionModelKeyEnv { name: name.clone() });
    }
    check_key(
        "decision_model",
        m.api_key_env.as_deref(),
        m.api_key_secret.as_deref(),
    )?;
    Ok(DecisionModelConfig {
        url: m.url.trim_end_matches('/').to_string(),
        model: m.model,
        api_key_env: m.api_key_env,
        api_key_secret: m.api_key_secret,
        timeout,
        shadow: m.shadow,
        agreement,
        min_judgements,
    })
}

/// The plugins in their own process, checked, with their paths resolved like `pi_bolt.program`.
pub(super) fn plugins(
    mut plugins: Vec<PluginConfig>,
    base: &Path,
) -> Result<Vec<PluginConfig>, ConfigError> {
    let built_in = [crate::decide::guard::NAME, crate::decide::model::NAME];
    let mut names = std::collections::BTreeSet::new();
    let mut kinds = std::collections::BTreeSet::new();
    for plugin in &mut plugins {
        let name = plugin.name.clone();
        if !valid_agent_name(&name) || built_in.contains(&name.as_str()) {
            return Err(ConfigError::PluginName { name });
        }
        if !names.insert(name.clone()) {
            return Err(ConfigError::PluginTwice { name });
        }
        if plugin.path.as_os_str().is_empty() {
            return Err(ConfigError::PluginPath { name });
        }
        if let Some(kind) = plugin
            .decides
            .iter()
            .find(|kind| !kinds.insert(kind.to_string()))
        {
            return Err(ConfigError::PluginDecidesTwice { kind: kind.clone() });
        }
        if plugin.path.components().count() > 1 {
            plugin.path = base.join(&plugin.path);
        }
    }
    Ok(plugins)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::config::Config;

    fn parse(text: &str) -> Result<Config, ConfigError> {
        Config::parse(text, Path::new("/srv"), Path::new("/srv/moebius.toml"))
    }

    #[test]
    fn the_decision_model_is_read_with_its_defaults_and_checked() {
        assert_eq!(parse("").unwrap().decision_model, None);
        let c = parse("[decision_model]\nurl = \"http://m:1/v1/\"\nmodel = \"m\"\n").unwrap();
        assert_eq!(
            c.decision_model,
            Some(DecisionModelConfig {
                url: "http://m:1/v1".into(),
                model: "m".into(),
                api_key_env: None,
                api_key_secret: None,
                timeout: Duration::from_secs(20),
                shadow: None,
                agreement: 0.95,
                min_judgements: 100,
            })
        );
        let c = parse(
            "[decision_model]\nurl = \"u\"\nmodel = \"m\"\napi_key_env = \"KEY\"\ntimeout_seconds = 29\n\
             shadow = [\"tool_call\"]\nagreement = 1\nmin_judgements = 5",
        )
        .unwrap()
        .decision_model
        .unwrap();
        assert_eq!(c.api_key_env.as_deref(), Some("KEY"));
        assert_eq!(c.shadow, Some(vec!["tool_call".to_string()]));
        assert_eq!((c.agreement, c.min_judgements), (1.0, 5));
        let with = |extra: &str| {
            parse(&format!(
                "[decision_model]\nurl = \"u\"\nmodel = \"m\"\n{extra}"
            ))
        };
        assert!(matches!(
            with("timeout_seconds = 30"),
            Err(ConfigError::DecisionModelTimeout { .. })
        ));
        assert!(matches!(
            with("agreement = 0"),
            Err(ConfigError::DecisionModelAgreement(_))
        ));
        assert!(matches!(
            with("agreement = 1.5"),
            Err(ConfigError::DecisionModelAgreement(_))
        ));
        assert!(matches!(
            with("min_judgements = 0"),
            Err(ConfigError::DecisionModelMinJudgements)
        ));
        assert!(matches!(
            with("api_key_env = \"a b\""),
            Err(ConfigError::DecisionModelKeyEnv { .. })
        ));
        assert!(matches!(
            parse("[decision_model]\nurl = \"\"\nmodel = \"m\""),
            Err(ConfigError::DecisionModelEmpty)
        ));
        assert!(matches!(
            parse("[decision_model]\nurl = \"u\""),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn the_teacher_is_read_and_its_timeout_stays_below_the_plugin_limit() {
        assert_eq!(parse("").unwrap().teacher, None);
        let c = parse("[teacher]\nmodel = \"m\"\n").unwrap();
        assert_eq!(
            c.teacher,
            Some(TeacherConfig {
                model: "m".into(),
                protocol: Protocol::OpenAi,
                timeout: Duration::from_secs(20),
            })
        );
        let c = parse("[teacher]\nmodel = \"m\"\nprotocol = \"anthropic\"\ntimeout_seconds = 29\n")
            .unwrap();
        let teacher = c.teacher.unwrap();
        assert_eq!(teacher.protocol, Protocol::Anthropic);
        assert_eq!(teacher.timeout, Duration::from_secs(29));
        for seconds in [0, 30, 60] {
            assert!(matches!(
                parse(&format!(
                    "[teacher]\nmodel = \"m\"\ntimeout_seconds = {seconds}"
                )),
                Err(ConfigError::TeacherTimeout { .. })
            ));
        }
        assert!(matches!(
            parse("[teacher]\ntimeout_seconds = 5"),
            Err(ConfigError::Parse { .. })
        ));
    }
}
