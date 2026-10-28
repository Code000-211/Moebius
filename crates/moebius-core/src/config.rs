//! The server's configuration, read from `moebius.toml`. Every key has a default, so an empty file is a valid one.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::mcp::{McpServer, valid_variable};
use crate::router::Protocol;

/// The commented example `moebius init` writes. Every value in it is the default.
pub const EXAMPLE: &str = r#"# Moebius server configuration. Every key is optional; the values below are the defaults.

# Where Moebius keeps its store, blobs, clones and worktrees.
data_dir = ".moebius"

# The address the desktop app connects to. Keep it on a private network (ADR 0003).
listen = "127.0.0.1:7171"

# The file holding the token the desktop app authenticates with; relative paths are inside data_dir.
token_file = "token"

# How many runs may work at the same time, across all boards.
run_limit = 3

# When the daily Retro runs, as HH:MM in the server's local time.
retro_at = "23:30"

# Rules every run must follow, as Markdown with one section per rule (a heading `## name` and its text); relative paths are inside this
# file's directory. A repository adds its own in .moebius/rules.md but cannot replace these. Without the file, only
# the built-in rules apply. Not allowed together with [harness], whose repository holds rules.md.
# rules_file = "rules.md"

# The daily stretch in which trials, replays and autoresearch may run without asking.
[trial_window]
start = "00:00"
end = "06:00"

# How often the board engine runs the column scripts, and how many coding rounds a card gets.
[board]
tick_ms = 2000
round_limit = 5

# The most tokens (input plus output) all runs together may use in a day of the server's local time. Once it is spent,
# the router refuses model requests and no card starts or continues work until the next day. Without it there is no limit.
# [budgets]
# day_tokens = 20000000

# A repository without a forge names its own checks in .moebius/checks on its default branch: a command run in a
# worktree of the change before it is reviewed. They are stopped after timeout_seconds (at least 1) and then fail.
[checks]
timeout_seconds = 1800

# The router: the only model provider the harnesses see (ADR 0006). It listens on localhost only.
[router]
listen = "127.0.0.1:7172"

# Where the router sends requests, one upstream per protocol (openai or anthropic). Keys are read from the
# environment variable named here, never from this file.
# [[router.upstreams]]
# protocol = "openai"
# base_url = "http://127.0.0.1:8080"      # a local server such as Radiance
# [[router.upstreams]]
# protocol = "anthropic"
# base_url = "https://api.anthropic.com"
# api_key_env = "ANTHROPIC_API_KEY"

[pi_bolt]
program = "pi-bolt"

# The teacher model checks the rules written in prose (a rule the built-in patterns cannot see) on every tool call.
# It is asked through the router, so its tokens are counted against the run being checked, and it needs an upstream for
# its protocol. Without a teacher, a call is refused whenever any prose rule applies (fail closed); without prose
# rules the teacher is never asked. The answer must come within timeout_seconds (1 to 29: the plugin host cuts a
# decider off after 30 seconds); a teacher that fails, is too slow or answers nonsense refuses the call.
# [teacher]
# model = "qwen3.8-flash-next"
# protocol = "openai"                     # or anthropic
# timeout_seconds = 20

# The decision model decides alongside the teacher for the kinds of decision it is trained on (ADR 0010). It is asked
# about every judgement held back from training (see docs/reference/decision-model.md) in the background, and how
# often it chooses what was recorded is measured. Once it has been asked about min_judgements judgements of a kind and
# agrees on at least the share `agreement`, a proposal in the inbox offers it that kind; merging the proposal makes the
# model decide it. `url` is an OpenAI-compatible base URL, called at <url>/chat/completions. The key is read from the
# environment variable named by api_key_env, never from this file. Without this section nothing of this happens.
# [decision_model]
# url = "http://127.0.0.1:8081/v1"
# model = "moebius-decider"
# api_key_env = "DECISION_MODEL_KEY"      # optional
# timeout_seconds = 20                    # 1 to 29, like the teacher's
# shadow = ["tool_call", "merge"]         # the kinds to shadow; all of them when not given
# agreement = 0.95                        # the share of equal choices needed, above 0 and at most 1
# min_judgements = 100                    # how many held-out judgements of a kind must have been asked about

# Moebius acts as its own GitHub App. The key file is read at start; relative paths are inside this file's directory.
# [github]
# app_id = 123456
# private_key_file = "github-app.pem"
# api_url = "https://api.github.com"
# poll_seconds = 30                       # how often each GitHub repository's issues are polled

# The harness configuration repository: a git repository with agents/<name>.toml, agents/<name>.md, skills/, rules.md
# and mcp.toml, cloned to data_dir/harness. Agents and rules then come from it, edits through the API are commits to it,
# and [agents.*] and rules_file are errors.
# [harness]
# url = "https://github.com/acme/harness-configuration"
# mcp_env = ["DOCS_TOKEN"]               # the environment variables mcp.toml may take values from, as ${NAME}

# Agents run on Pi-Bolt. The default workflow starts the Coder, and the Reviewer when there is one.
# [agents.Coder]
# model = "qwen3.8-flash-next"
# protocol = "openai"
# thinking = "medium"
# instructions = "Keep changes small and run the tests."
# token_budget = 2000000
"#;

/// Why a configuration could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not a valid configuration: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{key} must be a time as HH:MM, not {value:?}")]
    Time { key: &'static str, value: String },
    #[error("run_limit must be at least 1")]
    RunLimit,
    #[error("router.listen must be a loopback address, not {0}")]
    RouterNotLocal(SocketAddr),
    #[error("teacher.timeout_seconds must be between 1 and {max}, not {value}")]
    TeacherTimeout { value: u64, max: u64 },
    #[error("decision_model.timeout_seconds must be between 1 and {max}, not {value}")]
    DecisionModelTimeout { value: u64, max: u64 },
    #[error("decision_model.agreement must be above 0 and at most 1, not {0}")]
    DecisionModelAgreement(f64),
    #[error("decision_model.min_judgements must be at least 1")]
    DecisionModelMinJudgements,
    #[error("decision_model.url and decision_model.model must not be empty")]
    DecisionModelEmpty,
    #[error("decision_model.api_key_env: {name:?} is not the name of an environment variable")]
    DecisionModelKeyEnv { name: String },
    #[error("checks.timeout_seconds must be at least 1")]
    ChecksTimeout,
    #[error("budgets.day_tokens must be at least 1")]
    DayTokens,
    #[error("{name} is not a valid agent name")]
    AgentName { name: String },
    #[error("harness.mcp_env: {name:?} is not the name of an environment variable")]
    McpEnv { name: String },
    #[error("harness.url must not be empty")]
    HarnessUrl,
    #[error(
        "{key} cannot be set together with [harness]: move it into the harness configuration repository ({place})"
    )]
    InHarness {
        key: &'static str,
        place: &'static str,
    },
}

/// Whether `name` can name an agent: letters, digits, `-` and `_`, and not empty.
pub fn valid_agent_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A time of day as minutes after midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOfDay(pub u16);

impl TimeOfDay {
    fn parse(key: &'static str, value: &str) -> Result<Self, ConfigError> {
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWindow {
    start: Option<String>,
    end: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBudgets {
    day_tokens: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChecks {
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBoard {
    tick_ms: Option<u64>,
    round_limit: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRouter {
    listen: Option<SocketAddr>,
    #[serde(default)]
    upstreams: Vec<UpstreamConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPiBolt {
    program: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHarness {
    url: String,
    #[serde(default)]
    mcp_env: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGitHub {
    app_id: u64,
    private_key_file: PathBuf,
    api_url: Option<String>,
    poll_seconds: Option<u64>,
}

/// How long the plugin host lets a decider take (see `server`), in seconds. The teacher must answer sooner.
pub const PLUGIN_TIME_LIMIT_SECONDS: u64 = 30;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTeacher {
    model: String,
    #[serde(default = "default_protocol")]
    protocol: Protocol,
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDecisionModel {
    url: String,
    model: String,
    api_key_env: Option<String>,
    timeout_seconds: Option<u64>,
    shadow: Option<Vec<String>>,
    agreement: Option<f64>,
    min_judgements: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    data_dir: Option<PathBuf>,
    listen: Option<SocketAddr>,
    token_file: Option<PathBuf>,
    run_limit: Option<u32>,
    retro_at: Option<String>,
    trial_window: Option<RawWindow>,
    #[serde(default)]
    board: RawBoard,
    #[serde(default)]
    budgets: RawBudgets,
    #[serde(default)]
    checks: RawChecks,
    #[serde(default)]
    router: RawRouter,
    #[serde(default)]
    pi_bolt: RawPiBolt,
    rules_file: Option<PathBuf>,
    harness: Option<RawHarness>,
    github: Option<RawGitHub>,
    teacher: Option<RawTeacher>,
    decision_model: Option<RawDecisionModel>,
    #[serde(default)]
    agents: BTreeMap<String, AgentConfig>,
}

/// One upstream of the router.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamConfig {
    pub protocol: Protocol,
    pub base_url: String,
    /// The environment variable holding the provider's key.
    pub api_key_env: Option<String>,
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

/// The server's configuration, with every default applied and every path made absolute against the file's directory.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub data_dir: PathBuf,
    pub listen: SocketAddr,
    pub token_file: PathBuf,
    pub run_limit: u32,
    pub retro_at: TimeOfDay,
    pub trial_window: (TimeOfDay, TimeOfDay),
    pub board_tick: Duration,
    pub round_limit: u32,
    /// The most tokens all runs may use in a day, if limited.
    pub day_tokens: Option<i64>,
    /// How long a repository's own checks may run.
    pub checks_timeout: Duration,
    pub router_listen: SocketAddr,
    pub upstreams: Vec<UpstreamConfig>,
    pub pi_bolt: PathBuf,
    /// The `moebius` executable that runs find as `$MOEBIUS`: the running one. It is no key of the file; a server that
    /// is not the `moebius` binary, such as a test, sets it.
    pub moebius: PathBuf,
    /// The server's rules (see `rules`), if it has any. Not set together with `harness_url`.
    pub rules_file: Option<PathBuf>,
    /// The harness configuration repository to clone, if there is one.
    pub harness_url: Option<String>,
    /// The environment variables `mcp.toml` may take values from.
    pub mcp_env: Vec<String>,
    pub github: Option<GitHubConfig>,
    /// The teacher model; without one, calls are refused while prose rules apply.
    pub teacher: Option<TeacherConfig>,
    /// The decision model; without one nothing is shadowed and nothing can be taken over.
    pub decision_model: Option<DecisionModelConfig>,
    pub agents: BTreeMap<String, AgentConfig>,
}

impl Config {
    /// Where the harness configuration repository is cloned.
    pub fn harness_dir(&self) -> PathBuf {
        self.data_dir.join("harness")
    }

    /// The server's rules file: `rules_file`, or the harness configuration repository's `rules.md`.
    pub fn rules_path(&self) -> Option<PathBuf> {
        self.rules_file.clone().or_else(|| {
            self.harness_url
                .as_ref()
                .map(|_| self.harness_dir().join("rules.md"))
        })
    }

    /// Parses a configuration whose relative paths are resolved against `base`.
    pub fn parse(text: &str, base: &Path, origin: &Path) -> Result<Self, ConfigError> {
        let mut raw: Raw = toml::from_str(text).map_err(|source| ConfigError::Parse {
            path: origin.to_path_buf(),
            source,
        })?;
        let data_dir = base.join(raw.data_dir.unwrap_or_else(|| PathBuf::from(".moebius")));
        let token_file = data_dir.join(raw.token_file.unwrap_or_else(|| PathBuf::from("token")));
        let run_limit = raw.run_limit.unwrap_or(3);
        if run_limit == 0 {
            return Err(ConfigError::RunLimit);
        }
        let router_listen = raw
            .router
            .listen
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 7172)));
        if !router_listen.ip().is_loopback() {
            return Err(ConfigError::RouterNotLocal(router_listen));
        }
        if let Some(name) = raw.agents.keys().find(|n| !valid_agent_name(n)) {
            return Err(ConfigError::AgentName { name: name.clone() });
        }
        if let Some(harness) = &raw.harness {
            if harness.url.trim().is_empty() {
                return Err(ConfigError::HarnessUrl);
            }
            if let Some(name) = harness.mcp_env.iter().find(|n| !valid_variable(n)) {
                return Err(ConfigError::McpEnv { name: name.clone() });
            }
            if !raw.agents.is_empty() {
                return Err(ConfigError::InHarness {
                    key: "agents",
                    place: "agents/<name>.toml",
                });
            }
            if raw.rules_file.is_some() {
                return Err(ConfigError::InHarness {
                    key: "rules_file",
                    place: "rules.md",
                });
            }
        }
        for agent in raw.agents.values_mut() {
            for path in agent.skills.iter_mut().chain(&mut agent.extensions) {
                *path = base.join(&path);
            }
        }
        let pi_bolt = raw
            .pi_bolt
            .program
            .unwrap_or_else(|| PathBuf::from("pi-bolt"));
        let teacher = match raw.teacher {
            Some(t) => {
                let seconds = t.timeout_seconds.unwrap_or(20);
                if !(1..PLUGIN_TIME_LIMIT_SECONDS).contains(&seconds) {
                    return Err(ConfigError::TeacherTimeout {
                        value: seconds,
                        max: PLUGIN_TIME_LIMIT_SECONDS - 1,
                    });
                }
                Some(TeacherConfig {
                    model: t.model,
                    protocol: t.protocol,
                    timeout: Duration::from_secs(seconds),
                })
            }
            None => None,
        };
        let decision_model = match raw.decision_model {
            Some(m) => {
                let seconds = m.timeout_seconds.unwrap_or(20);
                if !(1..PLUGIN_TIME_LIMIT_SECONDS).contains(&seconds) {
                    return Err(ConfigError::DecisionModelTimeout {
                        value: seconds,
                        max: PLUGIN_TIME_LIMIT_SECONDS - 1,
                    });
                }
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
                Some(DecisionModelConfig {
                    url: m.url.trim_end_matches('/').to_string(),
                    model: m.model,
                    api_key_env: m.api_key_env,
                    timeout: Duration::from_secs(seconds),
                    shadow: m.shadow,
                    agreement,
                    min_judgements,
                })
            }
            None => None,
        };
        if raw.budgets.day_tokens.is_some_and(|tokens| tokens < 1) {
            return Err(ConfigError::DayTokens);
        }
        let checks_timeout = raw.checks.timeout_seconds.unwrap_or(1800);
        if checks_timeout == 0 {
            return Err(ConfigError::ChecksTimeout);
        }
        let window = raw.trial_window.unwrap_or(RawWindow {
            start: None,
            end: None,
        });
        Ok(Self {
            listen: raw
                .listen
                .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 7171))),
            token_file,
            run_limit,
            retro_at: TimeOfDay::parse("retro_at", raw.retro_at.as_deref().unwrap_or("23:30"))?,
            trial_window: (
                TimeOfDay::parse(
                    "trial_window.start",
                    window.start.as_deref().unwrap_or("00:00"),
                )?,
                TimeOfDay::parse("trial_window.end", window.end.as_deref().unwrap_or("06:00"))?,
            ),
            board_tick: Duration::from_millis(raw.board.tick_ms.unwrap_or(2000).max(50)),
            round_limit: raw.board.round_limit.unwrap_or(5),
            day_tokens: raw.budgets.day_tokens,
            checks_timeout: Duration::from_secs(checks_timeout),
            router_listen,
            upstreams: raw.router.upstreams,
            moebius: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("moebius")),
            // A bare name is looked up on the PATH; anything with a directory in it is a path.
            pi_bolt: if pi_bolt.components().count() > 1 {
                base.join(pi_bolt)
            } else {
                pi_bolt
            },
            rules_file: raw.rules_file.map(|file| base.join(file)),
            harness_url: raw.harness.as_ref().map(|harness| harness.url.clone()),
            mcp_env: raw
                .harness
                .map(|harness| harness.mcp_env)
                .unwrap_or_default(),
            github: raw.github.map(|g| GitHubConfig {
                app_id: g.app_id,
                private_key_file: base.join(g.private_key_file),
                api_url: g
                    .api_url
                    .unwrap_or_else(|| "https://api.github.com".to_string()),
                poll: Duration::from_secs(g.poll_seconds.unwrap_or(30).max(1)),
            }),
            teacher,
            decision_model,
            agents: raw.agents,
            data_dir,
        })
    }

    /// Reads `path`, or uses every default when the file does not exist.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        // Paths are made absolute here, since the server runs git and harnesses in other directories.
        let parent = path.parent().unwrap_or(Path::new(""));
        let base = std::path::absolute(if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        })
        .map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, &base, path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::parse("", &base, path),
            Err(source) => Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config, ConfigError> {
        Config::parse(text, Path::new("/srv"), Path::new("/srv/moebius.toml"))
    }

    #[test]
    fn an_empty_file_gives_every_default() {
        let c = parse("").unwrap();
        assert_eq!(c.data_dir, PathBuf::from("/srv/.moebius"));
        assert_eq!(c.listen, "127.0.0.1:7171".parse().unwrap());
        assert_eq!(c.token_file, PathBuf::from("/srv/.moebius/token"));
        assert_eq!(c.run_limit, 3);
        assert_eq!(c.retro_at, TimeOfDay(23 * 60 + 30));
        assert_eq!(c.trial_window, (TimeOfDay(0), TimeOfDay(6 * 60)));
    }

    #[test]
    fn the_example_is_exactly_the_defaults() {
        assert_eq!(parse(EXAMPLE).unwrap(), parse("").unwrap());
    }

    #[test]
    fn every_key_can_be_set() {
        let c = parse(
            "data_dir = \"/var/lib/moebius\"\nlisten = \"10.0.0.2:9000\"\ntoken_file = \"/etc/moebius/token\"\n\
             run_limit = 8\nretro_at = \"22:05\"\n[trial_window]\nstart = \"01:00\"\nend = \"05:30\"\n",
        )
        .unwrap();
        assert_eq!(c.data_dir, PathBuf::from("/var/lib/moebius"));
        assert_eq!(c.listen, "10.0.0.2:9000".parse().unwrap());
        assert_eq!(c.token_file, PathBuf::from("/etc/moebius/token"));
        assert_eq!(c.run_limit, 8);
        assert_eq!(c.retro_at, TimeOfDay(22 * 60 + 5));
        assert_eq!(c.trial_window, (TimeOfDay(60), TimeOfDay(5 * 60 + 30)));
    }

    #[test]
    fn the_router_agents_and_upstreams_are_read() {
        let c = parse(
            "[router]\nlisten = \"127.0.0.1:9100\"\n[[router.upstreams]]\nprotocol = \"anthropic\"\n\
             base_url = \"https://api.anthropic.com\"\napi_key_env = \"ANTHROPIC_API_KEY\"\n\
             [pi_bolt]\nprogram = \"/opt/pi-bolt\"\n[agents.Coder]\nmodel = \"m\"\ntoken_budget = 100\n",
        )
        .unwrap();
        assert_eq!(c.router_listen, "127.0.0.1:9100".parse().unwrap());
        assert_eq!(c.upstreams[0].protocol, Protocol::Anthropic);
        assert_eq!(c.pi_bolt, PathBuf::from("/opt/pi-bolt"));
        assert_eq!(c.rules_file, None);
        assert_eq!(c.agents["Coder"].protocol, Protocol::OpenAi);
        assert_eq!(c.agents["Coder"].token_budget, Some(100));
        assert!(matches!(
            parse("[router]\nlisten = \"0.0.0.0:7172\""),
            Err(ConfigError::RouterNotLocal(_))
        ));
    }

    #[test]
    fn the_harness_and_the_agents_files_are_relative_to_the_configuration() {
        let program = |program: &str| {
            parse(&format!("[pi_bolt]\nprogram = \"{program}\"\n"))
                .unwrap()
                .pi_bolt
        };
        assert_eq!(program("pi-bolt"), PathBuf::from("pi-bolt"));
        assert_eq!(
            program("./bin/pi-bolt"),
            PathBuf::from("/srv/./bin/pi-bolt")
        );
        assert_eq!(
            program("tools/pi-bolt"),
            PathBuf::from("/srv/tools/pi-bolt")
        );
        assert_eq!(program("/opt/pi-bolt"), PathBuf::from("/opt/pi-bolt"));

        let c = parse(
            "[agents.Coder]\nmodel = \"m\"\nskills = [\"skills/a\", \"/abs/b\"]\nextensions = [\"ext.js\"]\n",
        )
        .unwrap();
        let coder = &c.agents["Coder"];
        assert_eq!(
            coder.skills,
            [PathBuf::from("/srv/skills/a"), PathBuf::from("/abs/b")]
        );
        assert_eq!(coder.extensions, [PathBuf::from("/srv/ext.js")]);
    }

    #[test]
    fn the_rules_file_is_relative_to_the_configuration() {
        let c = parse("rules_file = \"rules.md\"").unwrap();
        assert_eq!(c.rules_file, Some(PathBuf::from("/srv/rules.md")));
    }

    #[test]
    fn the_harness_configuration_repository_replaces_the_agents_and_the_rules_file() {
        assert_eq!(parse("").unwrap().harness_url, None);
        let c = parse("[harness]\nurl = \"/srv/harness.git\"\n").unwrap();
        assert_eq!(c.harness_url.as_deref(), Some("/srv/harness.git"));
        assert!(c.mcp_env.is_empty());
        let c = parse("[harness]\nurl = \"/h\"\nmcp_env = [\"DOCS_TOKEN\"]\n").unwrap();
        assert_eq!(c.mcp_env, ["DOCS_TOKEN"]);
        let error = parse("[harness]\nurl = \"/h\"\nmcp_env = [\"A-B\"]\n").unwrap_err();
        assert!(matches!(error, ConfigError::McpEnv { .. }), "{error}");
        assert_eq!(c.harness_dir(), PathBuf::from("/srv/.moebius/harness"));
        assert_eq!(
            c.rules_path(),
            Some(PathBuf::from("/srv/.moebius/harness/rules.md"))
        );
        assert_eq!(
            parse("rules_file = \"r.md\"").unwrap().rules_path(),
            Some(PathBuf::from("/srv/r.md"))
        );
        for text in [
            "[harness]\nurl = \"/h\"\n[agents.Coder]\nmodel = \"m\"\n",
            "rules_file = \"r.md\"\n[harness]\nurl = \"/h\"\n",
        ] {
            let error = parse(text).unwrap_err();
            assert!(matches!(error, ConfigError::InHarness { .. }), "{error}");
            assert!(error.to_string().contains("move it into"), "{error}");
        }
        assert!(matches!(
            parse("[harness]\nurl = \" \""),
            Err(ConfigError::HarnessUrl)
        ));
        assert!(matches!(parse("[harness]"), Err(ConfigError::Parse { .. })));
    }

    #[test]
    fn the_github_app_is_read() {
        assert_eq!(parse("").unwrap().github, None);
        let c = parse("[github]\napp_id = 42\nprivate_key_file = \"app.pem\"\n").unwrap();
        let github = c.github.unwrap();
        assert_eq!(github.app_id, 42);
        assert_eq!(github.private_key_file, PathBuf::from("/srv/app.pem"));
        assert_eq!(github.api_url, "https://api.github.com");
        assert_eq!(github.poll, Duration::from_secs(30));
        let c = parse(
            "[github]\napp_id = 1\nprivate_key_file = \"/k.pem\"\napi_url = \"http://127.0.0.1:1\"\npoll_seconds = 5\n",
        )
        .unwrap();
        let github = c.github.unwrap();
        assert_eq!(github.private_key_file, PathBuf::from("/k.pem"));
        assert_eq!(github.api_url, "http://127.0.0.1:1");
        assert_eq!(github.poll, Duration::from_secs(5));
        assert!(matches!(
            parse("[github]\napp_id = 1"),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn the_daily_token_budget_is_optional_and_positive() {
        assert_eq!(parse("").unwrap().day_tokens, None);
        let c = parse("[budgets]\nday_tokens = 5000").unwrap();
        assert_eq!(c.day_tokens, Some(5000));
        assert!(matches!(
            parse("[budgets]\nday_tokens = 0"),
            Err(ConfigError::DayTokens)
        ));
    }

    #[test]
    fn the_time_the_checks_may_take_defaults_to_half_an_hour() {
        assert_eq!(parse("").unwrap().checks_timeout, Duration::from_secs(1800));
        let c = parse("[checks]\ntimeout_seconds = 5").unwrap();
        assert_eq!(c.checks_timeout, Duration::from_secs(5));
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

    #[test]
    fn bad_values_are_refused_with_the_key_named() {
        assert!(matches!(
            parse("retro_at = \"25:00\""),
            Err(ConfigError::Time {
                key: "retro_at",
                ..
            })
        ));
        assert!(matches!(parse("run_limit = 0"), Err(ConfigError::RunLimit)));
        assert!(matches!(
            parse("[checks]\ntimeout_seconds = 0"),
            Err(ConfigError::ChecksTimeout)
        ));
        assert!(matches!(
            parse("unknown = 1"),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn a_relative_config_path_gives_absolute_paths() {
        let c = Config::load(Path::new("moebius-does-not-exist.toml")).unwrap();
        assert!(c.data_dir.is_absolute(), "{}", c.data_dir.display());
        assert!(c.token_file.is_absolute());
    }

    #[test]
    fn a_missing_file_means_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::load(&dir.path().join("moebius.toml")).unwrap();
        assert_eq!(c.data_dir, dir.path().join(".moebius"));
    }
}
