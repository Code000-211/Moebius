//! What the Models and Plugins pages of the desktop app show: how the server is set up, read from its configuration and
//! its plugin host, with the state of each plugin in its own process. Nothing here is a secret: a key is told as set or not, and a URL has no credentials in it. The
//! Settings page reads and writes the configuration itself (see `settings`).

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::board::AgentInfo;
use crate::config::Config;
use crate::decide::Decisions;
use crate::router::{AvailableModels, Flow, Protocol, TERMS_WARNING};

/// `url` without the user name and password in front of its host, if it has any.
pub fn without_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}{tail}")
}

/// An upstream of the router.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpstreamView {
    pub protocol: Protocol,
    pub base_url: String,
    /// Whether the upstream has a key; the key itself is never shown.
    pub api_key_set: bool,
    /// How the upstream logs in with OAuth, if it does instead of a key.
    pub oauth: Option<OAuthView>,
}

/// The OAuth login of an upstream, without any of its tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OAuthView {
    pub flow: Flow,
    /// Whether a refresh token is stored; `GET /v1/models/upstreams/{place}/login` tells whether it holds.
    pub logged_in: bool,
    /// What the person confirms before a login starts.
    pub warning: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TeacherView {
    pub model: String,
    pub protocol: Protocol,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionModelView {
    pub url: String,
    pub model: String,
    pub timeout_seconds: u64,
    pub api_key_set: bool,
}

/// The answer of `GET /v1/models`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Models {
    pub upstreams: Vec<UpstreamView>,
    pub agents: Vec<AgentInfo>,
    pub teacher: Option<TeacherView>,
    pub decision_model: Option<DecisionModelView>,
}

/// What `GET /v1/models` answers; `secrets` are the names of the stored secrets.
pub fn models(config: &Config, agents: Vec<AgentInfo>, secrets: &[String]) -> Models {
    Models {
        upstreams: config
            .upstreams
            .iter()
            .map(|upstream| UpstreamView {
                protocol: upstream.protocol,
                base_url: without_credentials(&upstream.base_url),
                api_key_set: upstream.api_key_env.is_some() || upstream.api_key_secret.is_some(),
                oauth: upstream.oauth.as_ref().map(|oauth| OAuthView {
                    flow: oauth.flow,
                    logged_in: secrets.contains(&oauth.secret),
                    warning: TERMS_WARNING,
                }),
            })
            .collect(),
        agents,
        teacher: config.teacher.as_ref().map(|teacher| TeacherView {
            model: teacher.model.clone(),
            protocol: teacher.protocol,
            timeout_seconds: teacher.timeout.as_secs(),
        }),
        decision_model: config
            .decision_model
            .as_ref()
            .map(|model| DecisionModelView {
                url: without_credentials(&model.url),
                model: model.model.clone(),
                timeout_seconds: model.timeout.as_secs(),
                api_key_set: model.api_key_env.is_some() || model.api_key_secret.is_some(),
            }),
    }
}

/// A plugin the host has loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginView {
    pub name: String,
    pub version: String,
    pub kinds: Vec<moebius_plugin_api::Kind>,
    /// The kinds of decision it answers, those the decision model took over included.
    pub decides: Vec<String>,
    /// How it runs when it is a plugin in its own process; none for one compiled in.
    pub process: Option<ProcessView>,
}

/// A plugin in its own process, as the Plugins page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessView {
    pub program: String,
    pub state: moebius_plugin_api::State,
    /// How many times it was started again after it stopped.
    pub restarts: u64,
    /// Why it stopped the last time, with secrets redacted.
    pub error: Option<String>,
    /// The file its standard error goes to.
    pub log: String,
}

/// The answer of `GET /v1/plugins`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plugins {
    /// How long a call into any plugin may take before it counts as refused.
    pub time_limit_seconds: u64,
    pub plugins: Vec<PluginView>,
}

pub fn plugins(decisions: &Decisions, data_dir: &Path) -> Plugins {
    let routes = decisions.routes();
    Plugins {
        time_limit_seconds: decisions.host().limit().as_secs(),
        plugins: decisions
            .host()
            .manifests()
            .into_iter()
            .map(|manifest| PluginView {
                process: decisions
                    .host()
                    .status(&manifest.name)
                    .map(|status| ProcessView {
                        program: status.program.display().to_string(),
                        state: status.state,
                        restarts: status.restarts,
                        error: status.error.as_deref().map(crate::blobs::redact),
                        log: super::plugins::log_path(data_dir, &manifest.name)
                            .display()
                            .to_string(),
                    }),
                decides: routes
                    .iter()
                    .filter(|(_, plugin)| **plugin == manifest.name)
                    .map(|(kind, _)| kind.clone())
                    .collect(),
                name: manifest.name,
                version: manifest.version,
                kinds: manifest.kinds,
            })
            .collect(),
    }
}

/// One thing that has to be in place before the first card can run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct StartStep {
    /// What the step is, for a client to tell which page fixes it: `upstream`, `pi_bolt`, `harness`, `coder` or
    /// `repository`.
    pub key: String,
    pub title: String,
    pub done: bool,
    /// In plain words, what is in place or what is missing. Never a secret.
    pub detail: String,
}

/// The answer of `GET /v1/start`: whether the server is ready for a first card, and the steps that get it there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Start {
    pub done: bool,
    pub steps: Vec<StartStep>,
    /// What the person should know even when every step is done, such as the warning of an upstream that logs in
    /// with OAuth.
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// How long `pi-bolt --version` may take.
const PI_BOLT_TIME: Duration = Duration::from_secs(10);

/// The agent the default workflow's code column starts.
const CODER: &str = "Coder";

fn step(key: &str, title: &str, result: Result<String, String>) -> StartStep {
    let done = result.is_ok();
    StartStep {
        key: key.into(),
        title: title.into(),
        done,
        detail: result.unwrap_or_else(|detail| detail),
    }
}

/// Whether the upstreams are there and name models, from what they answered.
fn upstream_step(available: &AvailableModels) -> Result<String, String> {
    if !available.models.is_empty() {
        let count = available.models.len();
        return Ok(format!(
            "{count} {} available.",
            if count == 1 { "model is" } else { "models are" }
        ));
    }
    if available.errors.is_empty() {
        return Err("Add an upstream on the Models page.".into());
    }
    Err(available
        .errors
        .iter()
        .map(|e| format!("Could not ask {}: {}.", e.upstream, e.message))
        .collect::<Vec<_>>()
        .join(" "))
}

/// Whether `program --version` runs and succeeds in [`PI_BOLT_TIME`].
fn pi_bolt_runs(program: &Path) -> Result<String, String> {
    let shown = program.display();
    let mut child = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            let why = match e.kind() {
                std::io::ErrorKind::NotFound => "it was not found".to_string(),
                std::io::ErrorKind::PermissionDenied => "it is not allowed to run".to_string(),
                _ => e.to_string(),
            };
            format!("Could not run {shown}: {why}. Set the program on the Settings page.")
        })?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(format!("{shown} runs.")),
            Ok(Some(status)) => {
                return Err(format!(
                    "{shown} --version failed ({status}). Set the program on the Settings page."
                ));
            }
            Ok(None) if started.elapsed() < PI_BOLT_TIME => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{shown} --version did not answer in {} seconds.",
                    PI_BOLT_TIME.as_secs()
                ));
            }
            Err(e) => return Err(format!("Could not wait for {shown}: {e}.")),
        }
    }
}

/// The steps to a first card, in the order a person takes them. `available` is what the upstreams answered.
pub async fn start(
    config: &Config,
    available: AvailableModels,
    agents: &[AgentInfo],
    repositories: usize,
) -> Start {
    let program = config.pi_bolt.clone();
    let pi_bolt = tokio::task::spawn_blocking(move || pi_bolt_runs(&program))
        .await
        .unwrap_or_else(|_| Err("Could not check the program.".into()));
    let coder = match agents.iter().find(|agent| agent.name == CODER) {
        Some(agent) if agent.error.is_none() && !agent.model.is_empty() => {
            Ok(format!("The {CODER} uses {}.", agent.model))
        }
        Some(agent) if agent.error.is_some() => Err(format!(
            "The {CODER}'s files cannot be used. Fix them on the Agents page."
        )),
        _ => Err(format!("Give the {CODER} a model on the Agents page.")),
    };
    let harness = match &config.harness_url {
        Some(url) => Ok(format!(
            "Agents, skills, MCP servers and rules come from {}.",
            without_credentials(url)
        )),
        None => Err("Agents and rules are in moebius.toml, and skills and MCP servers need a repository. Set up \
                     makes a git repository next to moebius.toml, moves the agents and rules into it and restarts \
                     the server."
            .into()),
    };
    let repository = match repositories {
        0 => Err("No repository is added yet.".to_string()),
        1 => Ok("1 repository is added.".to_string()),
        n => Ok(format!("{n} repositories are added.")),
    };
    let steps = vec![
        step(
            "upstream",
            "Connect a model provider",
            upstream_step(&available),
        ),
        step("pi_bolt", "Install Pi-Bolt", pi_bolt),
        step(
            "harness",
            "Keep agents, skills and MCP servers in a repository",
            harness,
        ),
        step("coder", "Choose the Coder's model", coder),
        step("repository", "Add a repository", repository),
    ];
    let warnings = config
        .upstreams
        .iter()
        .filter(|upstream| upstream.oauth.is_some())
        .map(|upstream| {
            format!(
                "{} logs in with OAuth. {TERMS_WARNING}",
                without_credentials(&upstream.base_url)
            )
        })
        .collect();
    Start {
        done: steps.iter().all(|s| s.done),
        steps,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_in_a_url_are_removed() {
        for (url, expected) in [
            (
                "https://user:hunter2@example.com/repo.git",
                "https://example.com/repo.git",
            ),
            ("https://token@example.com", "https://example.com"),
            ("http://a:b@host:8080/x?y=z@w", "http://host:8080/x?y=z@w"),
            (
                "git@github.com:acme/harness.git",
                "git@github.com:acme/harness.git",
            ),
            ("/srv/git/harness", "/srv/git/harness"),
        ] {
            assert_eq!(without_credentials(url), expected);
        }
    }

    #[tokio::test]
    async fn an_upstream_that_logs_in_with_oauth_carries_the_warning_to_the_models_and_the_start() {
        let config = Config::parse(
            "[[router.upstreams]]\nprotocol = \"anthropic\"\nbase_url = \"https://u:p@api.example.com\"\n\
             [router.upstreams.oauth]\nflow = \"code\"\nclient_id = \"moebius\"\n\
             token_url = \"https://login.example.com/token\"\n\
             authorize_url = \"https://login.example.com/authorize\"\nsecret = \"example-login\"\n\
             [pi_bolt]\nprogram = \"/nonexistent/pi-bolt\"\n",
            Path::new("/srv"),
            Path::new("/srv/moebius.toml"),
        )
        .unwrap();
        let models = models(&config, vec![], &["example-login".to_string()]);
        let oauth = models.upstreams[0].oauth.as_ref().unwrap();
        assert_eq!(
            (oauth.flow, oauth.logged_in, oauth.warning),
            (Flow::Code, true, TERMS_WARNING)
        );
        assert!(!models.upstreams[0].api_key_set);
        let start = start(&config, AvailableModels::default(), &[], 0).await;
        assert_eq!(
            start.warnings,
            [format!(
                "https://api.example.com logs in with OAuth. {TERMS_WARNING}"
            )]
        );
    }
}
