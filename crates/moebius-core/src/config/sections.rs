//! The types of the configuration's sections as the server uses them: the router's upstreams, the GitHub App, the
//! teacher, the decision model and the agents, with times of day and the checks of names, keys and timeouts.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use super::ConfigError;
use super::raw::{RawDecisionModel, RawTeacher};
use crate::harness::mcp::{McpServer, valid_variable};
use crate::router::Protocol;

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
