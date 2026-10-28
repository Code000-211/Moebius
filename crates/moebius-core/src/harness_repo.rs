//! The harness configuration repository (spec 5): a git repository the server clones to `<data_dir>/harness`. It holds
//! `agents/<name>.toml` (settings) and `agents/<name>.md` (instructions), `skills/<name>/SKILL.md`, `rules.md` and
//! `mcp.toml`; the format is in `docs/reference/harness-configuration.md`.
//!
//! The agents are read from the clone's default branch when the server starts and after each refresh that changes it.
//! An agent whose files are broken is left out with its error, so the others go on. An edit through the API is a commit
//! on the default branch, pushed to `origin`; a push that fails undoes the commit, so the clone never drifts from
//! `origin`. Refreshing and editing take turns.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{App, AppError, Result};
use crate::bus::AppEvent;
use crate::config::{AgentConfig, AgentSet, default_protocol, valid_agent_name};
use crate::decide::Question;
use crate::mcp::{McpServer, parse_file};
use crate::repos::{Repos, git};
use crate::router::Protocol;
use crate::skill;

/// The settings of an agent: the keys of `agents/<name>.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSettings {
    pub model: String,
    #[serde(default = "default_protocol")]
    pub protocol: Protocol,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
    /// Names of directories under `skills/`.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Paths of files in the repository.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Names of servers in `mcp.toml`.
    #[serde(default)]
    pub mcp: Vec<String>,
}

/// An agent as the API takes and gives it: its settings and its instructions (`agents/<name>.md`, empty without).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDefinition {
    pub settings: AgentSettings,
    #[serde(default)]
    pub instructions: String,
}

/// What `GET /v1/agents/{name}` answers, and `PUT` after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentView {
    pub name: String,
    #[serde(flatten)]
    pub definition: AgentDefinition,
    /// The short id of the commit a `PUT` made; left out by `GET` and by a `PUT` that changed nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

/// A skill an agent can name, as `GET /v1/skills` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    /// The `description` of the skill's frontmatter; empty when it has none.
    pub description: String,
    /// Whether Moebius brings the skill itself, as it does `moebius`.
    pub built_in: bool,
}

/// An MCP server an agent can name, as `GET /v1/mcp-servers` lists it: never its environment or headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    /// `stdio` or `remote`; `unknown` for a server too broken to say.
    pub kind: String,
    /// Why the server cannot be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What an agent's settings may name, as the repository has it.
struct Layout {
    skills: BTreeSet<String>,
    /// The servers of `mcp.toml`, or why it cannot be read.
    mcp: std::result::Result<BTreeMap<String, std::result::Result<McpServer, String>>, String>,
}

impl Layout {
    fn read(root: &Path, mcp_env: &[String]) -> Self {
        let skills = std::fs::read_dir(root.join("skills"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().join("SKILL.md").is_file())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        let mcp = match std::fs::read_to_string(root.join("mcp.toml")) {
            Ok(text) => parse_file(&text, mcp_env),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(format!("cannot read mcp.toml: {e}")),
        };
        Self { skills, mcp }
    }

    /// Why `settings` cannot be used in the repository at `root`, if they cannot; else the MCP servers they name.
    fn check(
        &self,
        root: &Path,
        settings: &AgentSettings,
    ) -> std::result::Result<BTreeMap<String, McpServer>, String> {
        if settings.model.trim().is_empty() {
            return Err("model must not be empty".into());
        }
        if settings.token_budget.is_some_and(|tokens| tokens < 1) {
            return Err("token_budget must be at least 1".into());
        }
        for skill in &settings.skills {
            if !self.skills.contains(skill) {
                return Err(format!("the skill {skill:?} is not in skills/"));
            }
        }
        for extension in &settings.extensions {
            let inside = Path::new(extension)
                .components()
                .all(|part| matches!(part, Component::Normal(_)));
            if !inside || !root.join(extension).is_file() {
                return Err(format!(
                    "the extension {extension:?} is not a file in the repository"
                ));
            }
        }
        if settings.mcp.is_empty() {
            return Ok(BTreeMap::new());
        }
        let servers = self.mcp.as_ref().map_err(String::clone)?;
        settings
            .mcp
            .iter()
            .map(|name| match servers.get(name) {
                None => Err(format!("the MCP server {name:?} is not in mcp.toml")),
                Some(Err(error)) => Err(format!("the MCP server {name:?} in mcp.toml: {error}")),
                Some(Ok(server)) => Ok((name.clone(), server.clone())),
            })
            .collect()
    }
}

/// The agent as a run uses it, with paths inside the repository at `root`.
fn agent_config(
    root: &Path,
    definition: &AgentDefinition,
    mcp: BTreeMap<String, McpServer>,
) -> AgentConfig {
    let settings = &definition.settings;
    AgentConfig {
        model: settings.model.clone(),
        protocol: settings.protocol,
        thinking: settings.thinking.clone(),
        instructions: Some(definition.instructions.trim().to_string())
            .filter(|text| !text.is_empty()),
        skills: settings
            .skills
            .iter()
            .map(|skill| root.join("skills").join(skill))
            .collect(),
        extensions: settings
            .extensions
            .iter()
            .map(|path| root.join(path))
            .collect(),
        mcp,
        token_budget: settings.token_budget,
    }
}

/// The harness configuration repository's clone.
pub struct HarnessRepo {
    path: PathBuf,
    url: String,
    repos: Repos,
    /// The environment variables `mcp.toml` may take values from.
    mcp_env: Vec<String>,
    /// The agents the harness runs; replaced whenever the repository is read.
    agents: Arc<RwLock<AgentSet>>,
    /// The commit the agents were read at. Held while the repository is refreshed or edited.
    read_at: Mutex<String>,
}

impl HarnessRepo {
    /// Clones `url` to `path` unless it is there already, and reads the agents into `agents`. A clone that cannot be
    /// made is an error; one that is there already is used as it is, so that the server starts without the network.
    pub fn open(
        app: &App,
        url: &str,
        path: &Path,
        mcp_env: Vec<String>,
        agents: Arc<RwLock<AgentSet>>,
    ) -> std::result::Result<Self, String> {
        let repo = Self {
            path: path.to_path_buf(),
            url: url.to_string(),
            repos: app.repos.clone(),
            mcp_env,
            agents,
            read_at: Mutex::default(),
        };
        if !path.join(".git").exists() {
            let cloned = app
                .git_auth(url)
                .map_err(|e| e.to_string())
                .and_then(|header| {
                    (repo.repos.clone_into(path, url, header.as_deref())).map_err(|e| e.to_string())
                });
            cloned
                .map_err(|e| format!("cannot clone the harness configuration repository: {e}"))?;
        }
        let head = git(path, &["rev-parse", "--verify", "HEAD"]).unwrap_or_default();
        repo.read(app, &mut repo.lock(), head);
        Ok(repo)
    }

    /// Fetches `origin`, fast-forwards the default branch to it and reads the agents again if that changed anything.
    pub fn refresh(&self, app: &App) -> Result<()> {
        let mut read_at = self.lock();
        let header = app.git_auth(&self.url)?;
        self.repos.fetch(&self.path, header.as_deref())?;
        let upstream = format!("origin/{}", self.repos.default_branch(&self.path)?);
        if git(&self.path, &["rev-parse", "--verify", "--quiet", &upstream]).is_ok() {
            git(&self.path, &["merge", "--ff-only", "--quiet", &upstream])?;
        }
        let head = git(&self.path, &["rev-parse", "HEAD"])?;
        if head != *read_at {
            self.read(app, &mut read_at, head);
        }
        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::read(&self.path, &self.mcp_env)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, String> {
        self.read_at.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn agent_file(&self, name: &str, extension: &str) -> PathBuf {
        self.path.join("agents").join(format!("{name}.{extension}"))
    }

    /// Reads every agent from the working tree, which is the default branch, into the shared set. The error of each
    /// agent that is left out is published as `harness_error`.
    fn read(&self, app: &App, read_at: &mut String, head: String) {
        let layout = self.layout();
        let names: BTreeSet<String> = std::fs::read_dir(self.path.join("agents"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let file = entry.file_name().to_string_lossy().into_owned();
                let (stem, extension) = file.rsplit_once('.')?;
                matches!(extension, "toml" | "md").then(|| stem.to_string())
            })
            .collect();
        let mut set = AgentSet::default();
        for name in names {
            match self.read_agent(&layout, &name) {
                Ok((definition, mcp)) => {
                    set.agents
                        .insert(name, agent_config(&self.path, &definition, mcp));
                }
                Err(error) => {
                    app.bus.publish(AppEvent::new(
                        "harness_error",
                        json!({ "agent": name, "message": error }),
                    ));
                    set.errors.insert(name, error);
                }
            }
        }
        *self.agents.write().unwrap_or_else(|e| e.into_inner()) = set;
        *read_at = head;
    }

    /// The agent `name` as its files say, or why they cannot be used.
    fn read_agent(
        &self,
        layout: &Layout,
        name: &str,
    ) -> std::result::Result<(AgentDefinition, BTreeMap<String, McpServer>), String> {
        if !valid_agent_name(name) {
            return Err(format!(
                "{name:?} is not a valid agent name (letters, digits, - and _)"
            ));
        }
        let file = self.agent_file(name, "toml");
        let text = std::fs::read_to_string(&file).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("agents/{name}.md has no agents/{name}.toml"),
            _ => format!("cannot read agents/{name}.toml: {e}"),
        })?;
        let settings: AgentSettings =
            toml::from_str(&text).map_err(|e| format!("agents/{name}.toml: {e}"))?;
        let mcp = layout
            .check(&self.path, &settings)
            .map_err(|e| format!("agents/{name}.toml: {e}"))?;
        let instructions = match std::fs::read_to_string(self.agent_file(name, "md")) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("cannot read agents/{name}.md: {e}")),
        };
        let definition = AgentDefinition {
            settings,
            instructions,
        };
        Ok((definition, mcp))
    }

    /// The agent `name` as its files in the repository say.
    pub fn agent(&self, name: &str) -> Result<AgentView> {
        let _read_at = self.lock();
        if !valid_agent_name(name)
            || (!self.agent_file(name, "toml").is_file() && !self.agent_file(name, "md").is_file())
        {
            return Err(AppError::NotFound(format!("agent {name}")));
        }
        let (definition, _) = self
            .read_agent(&self.layout(), name)
            .map_err(AppError::Invalid)?;
        Ok(AgentView {
            name: name.into(),
            definition,
            commit: None,
        })
    }

    /// The skills of `skills/`, by name.
    pub fn skills(&self) -> Vec<SkillInfo> {
        let _read_at = self.lock();
        let mut skills: Vec<SkillInfo> = self
            .layout()
            .skills
            .into_iter()
            .map(|name| {
                let text =
                    std::fs::read_to_string(self.path.join("skills").join(&name).join("SKILL.md"));
                SkillInfo {
                    name,
                    description: text
                        .ok()
                        .and_then(|text| skill::description(&text))
                        .unwrap_or_default(),
                    built_in: false,
                }
            })
            .collect();
        skills.insert(
            0,
            SkillInfo {
                name: skill::NAME.into(),
                description: skill::description(skill::PAGE).unwrap_or_default(),
                built_in: true,
            },
        );
        skills
    }

    /// The servers of `mcp.toml`, by name. A file that cannot be read is no servers.
    pub fn mcp_servers(&self) -> Vec<McpServerInfo> {
        let _read_at = self.lock();
        let Ok(servers) = self.layout().mcp else {
            return vec![];
        };
        servers
            .into_iter()
            .map(|(name, server)| match server {
                Ok(server) => McpServerInfo {
                    name,
                    kind: server.kind().into(),
                    error: None,
                },
                Err(error) => McpServerInfo {
                    name,
                    kind: "unknown".into(),
                    error: Some(error),
                },
            })
            .collect()
    }

    /// A person adds or changes the agent `name`: the definition is validated as a file read is, committed on the
    /// default branch and pushed to `origin`. An edit that changes nothing commits nothing.
    pub fn save_agent(
        &self,
        app: &App,
        name: &str,
        definition: &AgentDefinition,
    ) -> Result<AgentView> {
        let mut read_at = self.lock();
        if !valid_agent_name(name) {
            return Err(AppError::Invalid(format!(
                "{name:?} is not a valid agent name (letters, digits, - and _)"
            )));
        }
        self.layout()
            .check(&self.path, &definition.settings)
            .map_err(AppError::Invalid)?;
        let settings = toml::to_string(&definition.settings)
            .map_err(|e| AppError::Invalid(format!("cannot write the settings: {e}")))?;
        let verb = if self.agent_file(name, "toml").exists() {
            "Update"
        } else {
            "Add"
        };
        let context = BTreeMap::from([
            ("settings".to_string(), settings.clone()),
            ("instructions".to_string(), definition.instructions.clone()),
        ]);
        let write = || {
            std::fs::create_dir_all(self.path.join("agents"))?;
            std::fs::write(self.agent_file(name, "toml"), &settings)?;
            let markdown = self.agent_file(name, "md");
            if definition.instructions.trim().is_empty() {
                remove_if_there(&markdown)
            } else {
                Ok(std::fs::write(&markdown, &definition.instructions)?)
            }
        };
        let message = format!("{verb} the agent {name}");
        let commit = self.commit(app, &mut read_at, name, &message, context, write)?;
        Ok(AgentView {
            name: name.into(),
            definition: definition.clone(),
            commit,
        })
    }

    /// A person deletes the agent `name`, the same way.
    pub fn delete_agent(&self, app: &App, name: &str) -> Result<()> {
        let mut read_at = self.lock();
        let (settings, markdown) = (self.agent_file(name, "toml"), self.agent_file(name, "md"));
        if !valid_agent_name(name) || !(settings.exists() || markdown.exists()) {
            return Err(AppError::NotFound(format!("agent {name}")));
        }
        let write = || {
            remove_if_there(&settings)?;
            remove_if_there(&markdown)
        };
        let message = format!("Delete the agent {name}");
        self.commit(app, &mut read_at, name, &message, BTreeMap::new(), write)
            .map(drop)
    }

    /// Makes the change `write` does to the agent's files a commit on the default branch, records the person's decision
    /// and pushes. Whatever fails, the working tree and the default branch are as they were. On success the agents are
    /// read again, and the short id of the commit is given; a change that changed nothing makes no commit.
    fn commit(
        &self,
        app: &App,
        read_at: &mut String,
        name: &str,
        message: &str,
        mut context: BTreeMap<String, String>,
        write: impl FnOnce() -> Result<()>,
    ) -> Result<Option<String>> {
        let before = git(&self.path, &["rev-parse", "HEAD"]).map_err(|_| {
            AppError::Invalid("the harness configuration repository has no commits".into())
        })?;
        let undo = |error: AppError| {
            let _ = git(&self.path, &["reset", "--hard", "--quiet", &before]);
            let _ = git(&self.path, &["clean", "-fdq", "--", "agents"]);
            error
        };
        write().map_err(&undo)?;
        let files = format!("agents/{name}.*");
        git(&self.path, &["add", "-A", "--", &files]).map_err(|e| undo(e.into()))?;
        if git(&self.path, &["diff", "--cached", "--quiet"]).is_ok() {
            return Ok(None);
        }
        let branch = self
            .repos
            .default_branch(&self.path)
            .map_err(|e| undo(e.into()))?;
        git(&self.path, &["commit", "--quiet", "-m", message]).map_err(|e| undo(e.into()))?;
        context.insert("agent".into(), name.into());
        context.insert("change".into(), message.into());
        app.decisions
            .record_person(
                &Question {
                    kind: "edit_agent".into(),
                    context,
                    options: vec!["edit_agent".into(), "leave".into()],
                    fail_closed: "leave".into(),
                    run_id: None,
                    card_id: None,
                },
                "edit_agent",
                "",
            )
            .map_err(|e| undo(e.into()))?;
        let header = app.git_auth(&self.url).map_err(&undo)?;
        self.repos
            .push(&self.path, &branch, header.as_deref())
            .map_err(|e| undo(e.into()))?;
        let head = git(&self.path, &["rev-parse", "HEAD"])?;
        let short = git(&self.path, &["rev-parse", "--short", "HEAD"])?;
        self.read(app, read_at, head);
        Ok(Some(short))
    }
}

fn remove_if_there(file: &Path) -> Result<()> {
    match std::fs::remove_file(file) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}
