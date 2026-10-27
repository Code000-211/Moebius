//! The server's configuration, read from `moebius.toml`. Every key has a default, so an empty file is a valid one.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

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
# the built-in rules apply. The harness configuration repository takes this over in M7.
# rules_file = "rules.md"

# The daily stretch in which trials, replays and autoresearch may run without asking.
[trial_window]
start = "00:00"
end = "06:00"

# How often the board engine runs the column scripts, and how many coding rounds a card gets.
[board]
tick_ms = 2000
round_limit = 5

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

# Moebius acts as its own GitHub App. The key file is read at start; relative paths are inside this file's directory.
# [github]
# app_id = 123456
# private_key_file = "github-app.pem"
# api_url = "https://api.github.com"
# poll_seconds = 30                       # how often each GitHub repository's issues are polled

# Agents run on Pi-Bolt. The default workflow starts the Coder.
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
    #[error("{name} is not a valid agent name")]
    AgentName { name: String },
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
    router: RawRouter,
    #[serde(default)]
    pi_bolt: RawPiBolt,
    rules_file: Option<PathBuf>,
    github: Option<RawGitHub>,
    teacher: Option<RawTeacher>,
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

/// An agent as configured; the harness configuration repository takes this over in M6.
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
    pub token_budget: Option<i64>,
}

fn default_protocol() -> Protocol {
    Protocol::OpenAi
}

/// The server's configuration, with every default applied and every path made absolute against the file's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub data_dir: PathBuf,
    pub listen: SocketAddr,
    pub token_file: PathBuf,
    pub run_limit: u32,
    pub retro_at: TimeOfDay,
    pub trial_window: (TimeOfDay, TimeOfDay),
    pub board_tick: Duration,
    pub round_limit: u32,
    pub router_listen: SocketAddr,
    pub upstreams: Vec<UpstreamConfig>,
    pub pi_bolt: PathBuf,
    /// The server's rules (see `rules`), if it has any.
    pub rules_file: Option<PathBuf>,
    pub github: Option<GitHubConfig>,
    /// The teacher model; without one, calls are refused while prose rules apply.
    pub teacher: Option<TeacherConfig>,
    pub agents: BTreeMap<String, AgentConfig>,
}

impl Config {
    /// Parses a configuration whose relative paths are resolved against `base`.
    pub fn parse(text: &str, base: &Path, origin: &Path) -> Result<Self, ConfigError> {
        let raw: Raw = toml::from_str(text).map_err(|source| ConfigError::Parse {
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
        if let Some(name) = raw.agents.keys().find(|n| {
            n.is_empty()
                || !n
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        }) {
            return Err(ConfigError::AgentName { name: name.clone() });
        }
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
            router_listen,
            upstreams: raw.router.upstreams,
            pi_bolt: raw
                .pi_bolt
                .program
                .unwrap_or_else(|| PathBuf::from("pi-bolt")),
            rules_file: raw.rules_file.map(|file| base.join(file)),
            github: raw.github.map(|g| GitHubConfig {
                app_id: g.app_id,
                private_key_file: base.join(g.private_key_file),
                api_url: g
                    .api_url
                    .unwrap_or_else(|| "https://api.github.com".to_string()),
                poll: Duration::from_secs(g.poll_seconds.unwrap_or(30).max(1)),
            }),
            teacher,
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
    fn the_rules_file_is_relative_to_the_configuration() {
        let c = parse("rules_file = \"rules.md\"").unwrap();
        assert_eq!(c.rules_file, Some(PathBuf::from("/srv/rules.md")));
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
