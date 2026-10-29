//! The server's configuration, read from `moebius.toml`. Every key has a default, so an empty file is a valid one.
//!
//! This module holds `Config` itself, read and checked from the file, and its errors; `example` is the commented
//! example `moebius init` writes, `raw` the file as it is written, and `sections` the types of its sections, with the
//! checks of each.

mod example;
mod raw;
mod sections;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use raw::{Raw, RawWindow};
use sections::{check_key, decision_model, forgejo, gitlab, plugins, teacher};

use crate::harness::mcp::valid_variable;
use crate::router::{Classes, check_class};

pub use example::EXAMPLE;
pub use sections::{
    AgentConfig, AgentSet, DecisionModelConfig, ForgejoConfig, GitHubConfig, GitLabConfig,
    HarnessKind, PLUGIN_TIME_LIMIT_SECONDS, PluginConfig, TeacherConfig, TimeOfDay, TokenSource,
    ToolOutput, UpstreamConfig, valid_agent_name,
};
pub(crate) use sections::{default_protocol, default_true};

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
    #[error("{key}: api_key_env and api_key_secret cannot both be set")]
    KeyTwice { key: &'static str },
    #[error("{key}.api_key_secret: {name:?} is not a name (letters, digits, - and _)")]
    KeySecret { key: &'static str, name: String },
    #[error("router.upstreams.oauth: {0}")]
    OAuth(String),
    #[error(
        "router.upstreams: an upstream that logs in with oauth cannot also set api_key_env or api_key_secret"
    )]
    OAuthAndKey,
    #[error("router.classes: {0}")]
    Class(String),
    #[error("checks.timeout_seconds must be at least 1")]
    ChecksTimeout,
    #[error("budgets.day_tokens must be at least 1")]
    DayTokens,
    #[error("{name} is not a valid agent name")]
    AgentName { name: String },
    #[error("agents.{name}: {why}")]
    AgentHarness { name: String, why: &'static str },
    #[error("harness.mcp_env: {name:?} is not the name of an environment variable")]
    McpEnv { name: String },
    #[error("harness.url must not be empty")]
    HarnessUrl,
    #[error("[gitlab] needs exactly one of token_file and token_secret")]
    GitLabToken,
    #[error("gitlab.token_secret: {name:?} is not a name (letters, digits, - and _)")]
    GitLabSecret { name: String },
    #[error("gitlab.url must not be empty")]
    GitLabUrl,
    #[error("[forgejo] needs exactly one of token_file and token_secret")]
    ForgejoToken,
    #[error("forgejo.token_secret: {name:?} is not a name (letters, digits, - and _)")]
    ForgejoSecret { name: String },
    #[error("forgejo.url must not be empty")]
    ForgejoUrl,
    #[error(
        "{key} cannot be set together with [harness]: move it into the harness configuration repository ({place})"
    )]
    InHarness {
        key: &'static str,
        place: &'static str,
    },
    #[error(
        "plugins.name: {name:?} is not a name (letters, digits, - and _) or is the name of a built-in plugin"
    )]
    PluginName { name: String },
    #[error("plugins: two plugins are named {name:?}")]
    PluginTwice { name: String },
    #[error("plugins.path of {name:?} must not be empty")]
    PluginPath { name: String },
    #[error("plugins.decides: {kind:?} is decided by two plugins")]
    PluginDecidesTwice { kind: String },
}

/// The server's configuration, with every default applied and every path made absolute against the file's directory.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The configuration file, made absolute: where settings edited through the API are written.
    pub file: PathBuf,
    pub data_dir: PathBuf,
    pub listen: SocketAddr,
    pub token_file: PathBuf,
    pub run_limit: u32,
    pub retro_at: TimeOfDay,
    pub trial_window: (TimeOfDay, TimeOfDay),
    /// How many past merged cards of a repository a proposal is also tried on (see `trial::regression`).
    pub regression_size: usize,
    /// Whether a proposal must pass the regression set to be positive; when off, it is not tried on it.
    pub regression_gate: bool,
    pub board_tick: Duration,
    pub round_limit: u32,
    /// The most tokens all runs may use in a day, if limited.
    pub day_tokens: Option<i64>,
    /// How long a repository's own checks may run.
    pub checks_timeout: Duration,
    pub router_listen: SocketAddr,
    pub upstreams: Vec<UpstreamConfig>,
    /// The model classes a request may name instead of a model, by name.
    pub classes: Classes,
    pub pi_bolt: PathBuf,
    /// Claude Code's command line, for the agents whose harness is `claude-code`.
    pub claude_code: PathBuf,
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
    /// The GitLab, if repositories on one are polled.
    pub gitlab: Option<GitLabConfig>,
    /// The Forgejo or Gitea (such as Codeberg), if repositories on one are polled.
    pub forgejo: Option<ForgejoConfig>,
    /// The teacher model; without one, calls are refused while prose rules apply.
    pub teacher: Option<TeacherConfig>,
    /// The decision model; without one nothing is shadowed and nothing can be taken over.
    pub decision_model: Option<DecisionModelConfig>,
    /// The plugins in their own process, with their paths made absolute.
    pub plugins: Vec<PluginConfig>,
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
        for upstream in &raw.router.upstreams {
            check_key(
                "router.upstreams",
                upstream.api_key_env.as_deref(),
                upstream.api_key_secret.as_deref(),
            )?;
            if let Some(oauth) = &upstream.oauth {
                if upstream.api_key_env.is_some() || upstream.api_key_secret.is_some() {
                    return Err(ConfigError::OAuthAndKey);
                }
                if let Some(problem) = oauth.problem() {
                    return Err(ConfigError::OAuth(problem));
                }
            }
        }
        for (name, models) in &raw.router.classes {
            check_class(name, models).map_err(ConfigError::Class)?;
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
        for (name, agent) in &raw.agents {
            if let Some(why) = agent
                .harness
                .refuses(agent.protocol, !agent.extensions.is_empty())
            {
                return Err(ConfigError::AgentHarness {
                    name: name.clone(),
                    why,
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
        let plugins = plugins(raw.plugins, base)?;
        let claude_code = raw
            .claude_code
            .program
            .unwrap_or_else(|| PathBuf::from("claude"));
        let teacher = raw.teacher.map(teacher).transpose()?;
        let gitlab = raw.gitlab.map(|g| gitlab(g, base)).transpose()?;
        let forgejo = raw.forgejo.map(|f| forgejo(f, base)).transpose()?;
        let decision_model = raw.decision_model.map(decision_model).transpose()?;
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
            file: base.join(origin.file_name().unwrap_or_default()),
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
            regression_size: raw.regression.size.unwrap_or(5) as usize,
            regression_gate: raw.regression.gate.unwrap_or(true),
            board_tick: Duration::from_millis(raw.board.tick_ms.unwrap_or(2000).max(50)),
            round_limit: raw.board.round_limit.unwrap_or(5),
            day_tokens: raw.budgets.day_tokens,
            checks_timeout: Duration::from_secs(checks_timeout),
            router_listen,
            upstreams: raw.router.upstreams,
            classes: raw.router.classes,
            moebius: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("moebius")),
            pi_bolt: program(base, pi_bolt),
            claude_code: program(base, claude_code),
            // Without a harness repository the server's rules are a file next to this one unless named; a missing
            // file means no rules.
            rules_file: match &raw.harness {
                Some(_) => None,
                None => Some(base.join(raw.rules_file.unwrap_or_else(|| "rules.md".into()))),
            },
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
            gitlab,
            forgejo,
            teacher,
            decision_model,
            plugins,
            agents: raw.agents,
            data_dir,
        })
    }

    /// The size of the regression set a proposal is tried on: 0 when the gate is off.
    pub fn regression(&self) -> usize {
        if self.regression_gate {
            self.regression_size
        } else {
            0
        }
    }

    /// The directory of the configuration file, which the plugins in their own process run in.
    pub fn dir(&self) -> &Path {
        self.file.parent().unwrap_or(Path::new("/"))
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

/// An executable as the file names it: a bare name is looked up on the PATH; anything with a directory in it is a path
/// inside the file's directory `base`.
fn program(base: &Path, program: PathBuf) -> PathBuf {
    if program.components().count() > 1 {
        base.join(program)
    } else {
        program
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::Protocol;

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
        assert_eq!((c.regression_size, c.regression_gate), (5, true));
        assert_eq!(c.regression(), 5);
    }

    #[test]
    fn the_regression_set_has_a_size_and_a_gate_that_turns_it_off() {
        let c = parse("[regression]\nsize = 2\n").unwrap();
        assert_eq!(c.regression(), 2);
        let c = parse("[regression]\nsize = 2\ngate = false\n").unwrap();
        assert_eq!((c.regression_size, c.regression()), (2, 0));
        assert!(parse("[regression]\nsize = -1\n").is_err());
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
    fn an_upstream_can_log_in_with_oauth_but_not_also_have_a_key() {
        let oauth = "[[router.upstreams]]\nprotocol = \"anthropic\"\nbase_url = \"https://api.example.com\"\n\
                     [router.upstreams.oauth]\nflow = \"device\"\nclient_id = \"moebius\"\n\
                     token_url = \"https://login.example.com/token\"\n\
                     device_url = \"https://login.example.com/device\"\nscopes = [\"models\"]\nsecret = \"example-login\"\n";
        let c = parse(oauth).unwrap();
        let login = c.upstreams[0].oauth.as_ref().unwrap();
        assert_eq!(login.flow, crate::router::Flow::Device);
        assert_eq!(login.scopes, ["models"]);
        assert_eq!(login.secret, "example-login");
        let keyed = oauth.replace(
            "[router.upstreams.oauth]",
            "api_key_env = \"KEY\"\n[router.upstreams.oauth]",
        );
        assert!(matches!(parse(&keyed), Err(ConfigError::OAuthAndKey)));
        let coded = oauth.replace("\"device\"", "\"code\"");
        assert_eq!(
            parse(&coded).unwrap_err().to_string(),
            "router.upstreams.oauth: authorize_url is needed by the code flow"
        );
        let unknown = oauth.replace("scopes", "scope");
        assert!(matches!(parse(&unknown), Err(ConfigError::Parse { .. })));
    }

    #[test]
    fn model_classes_are_read_and_checked() {
        let c = parse(
            "[router.classes]\nfast = [\"qwen-local\", { model = \"haiku\", day_tokens = 9 }]\nstrong = [\"big\"]\n",
        )
        .unwrap();
        assert_eq!(c.classes["fast"][1].model, "haiku");
        assert_eq!(c.classes["fast"][1].day_tokens, Some(9));
        assert_eq!(c.classes["strong"][0].day_tokens, None);
        assert!(matches!(
            parse("[router.classes]\nfast = []\n"),
            Err(ConfigError::Class(_))
        ));
        assert!(matches!(
            parse("[router.classes]\nfast = [{ model = \"a\", day_tokens = 0 }]\n"),
            Err(ConfigError::Class(_))
        ));
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
        assert_eq!(c.rules_file, Some(PathBuf::from("/srv/rules.md")));
        assert_eq!(c.agents["Coder"].protocol, Protocol::OpenAi);
        assert_eq!(c.agents["Coder"].token_budget, Some(100));
        assert!(matches!(
            parse("[router]\nlisten = \"0.0.0.0:7172\""),
            Err(ConfigError::RouterNotLocal(_))
        ));
    }

    #[test]
    fn an_agents_tool_output_has_defaults_and_can_be_turned_off() {
        let c = parse("[agents.Coder]\nmodel = \"m\"\n").unwrap();
        assert_eq!(c.agents["Coder"].tool_output, ToolOutput::default());
        assert_eq!(c.agents["Coder"].tool_output.hide_after_turns, 10);
        let c = parse(
            "[agents.Coder]\nmodel = \"m\"\n[agents.Coder.tool_output]\nhide_after_turns = 0\n\
             max_lines = 50\ncompress = false\n",
        )
        .unwrap();
        let output = c.agents["Coder"].tool_output;
        assert_eq!((output.hide_after_turns, output.max_lines), (0, 50));
        assert_eq!((output.max_bytes, output.compress), (16 * 1024, false));
        assert!(
            parse("[agents.Coder]\nmodel = \"m\"\n[agents.Coder.tool_output]\nlines = 5\n")
                .is_err()
        );
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
    fn an_agent_chooses_its_harness_and_claude_code_is_found_like_pi_bolt() {
        let c = parse("[agents.Coder]\nmodel = \"m\"\n").unwrap();
        assert_eq!(c.agents["Coder"].harness, HarnessKind::PiBolt);
        assert_eq!(c.claude_code, PathBuf::from("claude"));
        let c = parse(
            "[claude_code]\nprogram = \"bin/claude\"\n[agents.Coder]\nmodel = \"m\"\nprotocol = \"anthropic\"\n\
             harness = \"claude-code\"\n",
        )
        .unwrap();
        assert_eq!(c.agents["Coder"].harness, HarnessKind::ClaudeCode);
        assert_eq!(c.claude_code, PathBuf::from("/srv/bin/claude"));
        assert!(matches!(
            parse("[agents.Coder]\nmodel = \"m\"\nharness = \"codex\"\n"),
            Err(ConfigError::Parse { .. })
        ));
        // Claude Code speaks only the anthropic protocol, and loads no Pi extensions.
        let refused = |extra: &str| {
            let text = format!("[agents.Coder]\nmodel = \"m\"\nharness = \"claude-code\"\n{extra}");
            parse(&text).unwrap_err().to_string()
        };
        assert!(refused("").contains("anthropic"));
        assert!(
            refused("protocol = \"anthropic\"\nextensions = [\"e.ts\"]\n").contains("extensions")
        );
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
    fn the_gitlab_is_read_with_its_token_from_a_file_or_a_secret() {
        assert_eq!(parse("").unwrap().gitlab, None);
        let gitlab = parse("[gitlab]\ntoken_file = \"gitlab-token\"\n")
            .unwrap()
            .gitlab
            .unwrap();
        assert_eq!(gitlab.url, "https://gitlab.com");
        assert_eq!(
            gitlab.token,
            TokenSource::File(PathBuf::from("/srv/gitlab-token"))
        );
        assert_eq!(gitlab.poll, Duration::from_secs(30));
        let gitlab = parse(
            "[gitlab]\nurl = \"https://git.example.org/\"\ntoken_secret = \"gitlab\"\npoll_seconds = 0\n",
        )
        .unwrap()
        .gitlab
        .unwrap();
        assert_eq!(gitlab.url, "https://git.example.org");
        assert_eq!(gitlab.token, TokenSource::Secret("gitlab".into()));
        assert_eq!(gitlab.poll, Duration::from_secs(1));
        assert!(matches!(parse("[gitlab]\n"), Err(ConfigError::GitLabToken)));
        assert!(matches!(
            parse("[gitlab]\ntoken_file = \"t\"\ntoken_secret = \"s\"\n"),
            Err(ConfigError::GitLabToken)
        ));
        assert!(matches!(
            parse("[gitlab]\ntoken_secret = \"a b\"\n"),
            Err(ConfigError::GitLabSecret { .. })
        ));
        assert!(matches!(
            parse("[gitlab]\nurl = \" \"\ntoken_file = \"t\"\n"),
            Err(ConfigError::GitLabUrl)
        ));
    }

    #[test]
    fn the_forgejo_is_codeberg_unless_named_with_its_token_from_a_file_or_a_secret() {
        assert_eq!(parse("").unwrap().forgejo, None);
        let forgejo = parse("[forgejo]\ntoken_file = \"forgejo-token\"\n")
            .unwrap()
            .forgejo
            .unwrap();
        assert_eq!(forgejo.url, "https://codeberg.org");
        assert_eq!(
            forgejo.token,
            TokenSource::File(PathBuf::from("/srv/forgejo-token"))
        );
        assert_eq!(forgejo.poll, Duration::from_secs(30));
        let forgejo = parse(
            "[forgejo]\nurl = \"https://gitea.example.org/\"\ntoken_secret = \"forgejo\"\npoll_seconds = 0\n",
        )
        .unwrap()
        .forgejo
        .unwrap();
        assert_eq!(forgejo.url, "https://gitea.example.org");
        assert_eq!(forgejo.token, TokenSource::Secret("forgejo".into()));
        assert_eq!(forgejo.poll, Duration::from_secs(1));
        assert!(matches!(
            parse("[forgejo]\n"),
            Err(ConfigError::ForgejoToken)
        ));
        assert!(matches!(
            parse("[forgejo]\ntoken_file = \"t\"\ntoken_secret = \"s\"\n"),
            Err(ConfigError::ForgejoToken)
        ));
        assert!(matches!(
            parse("[forgejo]\ntoken_secret = \"a b\"\n"),
            Err(ConfigError::ForgejoSecret { .. })
        ));
        assert!(matches!(
            parse("[forgejo]\nurl = \" \"\ntoken_file = \"t\"\n"),
            Err(ConfigError::ForgejoUrl)
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
    fn plugins_in_their_own_process_are_read_and_checked() {
        let c = parse(
            "[[plugins]]\nname = \"allow-reads\"\npath = \"plugins/allow-reads\"\nargs = [\"--plugin\"]\n\
             decides = [\"start_card\"]\n[[plugins]]\nname = \"notes\"\npath = \"notes-plugin\"\n",
        )
        .unwrap();
        assert_eq!(c.plugins[0].path, PathBuf::from("/srv/plugins/allow-reads"));
        assert_eq!(c.plugins[0].args, ["--plugin"]);
        assert_eq!(c.plugins[0].decides, ["start_card"]);
        assert_eq!(c.plugins[1].path, PathBuf::from("notes-plugin"));
        assert!(c.plugins[1].decides.is_empty());
        let plugin = |name: &str, decides: &str| {
            format!("[[plugins]]\nname = \"{name}\"\npath = \"p\"\ndecides = [{decides}]\n")
        };
        for (text, wanted) in [
            (plugin("guard", ""), "built-in"),
            (plugin("a b", ""), "not a name"),
            (plugin("a", "") + &plugin("a", ""), "two plugins"),
            (
                plugin("a", "\"x\"") + &plugin("b", "\"x\""),
                "decided by two",
            ),
            (
                "[[plugins]]\nname = \"a\"\npath = \"\"\n".to_string(),
                "must not be empty",
            ),
        ] {
            let error = parse(&text).unwrap_err().to_string();
            assert!(error.contains(wanted), "{error}");
        }
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
