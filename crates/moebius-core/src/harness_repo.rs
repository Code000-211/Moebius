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
use crate::decide::{Actor, Question};
use crate::mcp::{McpServer, parse_file};
use crate::repos::{Repos, git};
use crate::router::Protocol;
use crate::rules;
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

    /// The clone's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The clone's default branch.
    pub fn default_branch(&self) -> Result<String> {
        Ok(self.repos.default_branch(&self.path)?)
    }

    /// The branches of the clone whose names start with `prefix` that are proposals: each is checked, and pushed to
    /// `origin` if it is fit. A branch that changes nothing, changes a file that is not harness configuration, breaks
    /// an agent that worked on the default branch, or cannot be pushed is left out, with a warning saying why.
    pub fn proposed(&self, app: &App, prefix: &str) -> Result<(Vec<Proposed>, Vec<String>)> {
        let _read_at = self.lock();
        let base = self.repos.default_branch(&self.path)?;
        let pattern = format!("refs/heads/{prefix}*");
        let branches = git(
            &self.path,
            &["for-each-ref", "--format=%(refname:short)", &pattern],
        )?;
        let broken_before = broken_agents(&self.path, &self.mcp_env);
        let (mut proposed, mut warnings) = (vec![], vec![]);
        for branch in branches.lines() {
            match self.check_branch(app, &base, branch, &broken_before) {
                Ok(p) => proposed.push(p),
                Err(why) => warnings.push(format!("{branch} is left out: {why}")),
            }
        }
        Ok((proposed, warnings))
    }

    fn check_branch(
        &self,
        app: &App,
        base: &str,
        branch: &str,
        broken_before: &BTreeMap<String, String>,
    ) -> std::result::Result<Proposed, String> {
        let git = |args: &[&str]| git(&self.path, args).map_err(|e| e.to_string());
        let range = format!("{base}...{branch}");
        let files = git(&["diff", "--name-only", &range])?;
        let files: Vec<&str> = files.lines().collect();
        if files.is_empty() {
            return Err("it changes nothing".into());
        }
        if let Some(file) = files.iter().find(|file| !is_configuration(file)) {
            return Err(format!(
                "it changes {file}, which is not harness configuration"
            ));
        }
        let check = format!("check-{}", branch.replace('/', "-"));
        let root = app
            .repos
            .add_detached_worktree("harness", &self.path, &check, branch)
            .map_err(|e| e.to_string())?;
        let broken = broken_agents(&root, &self.mcp_env);
        let _ = app.repos.remove_worktree(&self.path, &root);
        if let Some((name, error)) = broken
            .iter()
            .find(|(name, _)| !broken_before.contains_key(*name))
        {
            return Err(format!("the agent {name} cannot be used with it: {error}"));
        }
        let message = git(&["log", "-1", "--format=%B", branch])?;
        let (message, cards) = split_cards(&message);
        let (title, body) = message.split_once('\n').unwrap_or((&message, ""));
        let mut agents: Vec<String> = files
            .iter()
            .filter_map(|file| file.strip_prefix("agents/")?.rsplit_once('.'))
            .map(|(name, _)| name.to_string())
            .collect();
        agents.dedup();
        let header = app.git_auth(&self.url).map_err(|e| e.to_string())?;
        self.repos
            .push(&self.path, branch, header.as_deref())
            .map_err(|e| format!("it cannot be pushed: {e}"))?;
        Ok(Proposed {
            branch: branch.into(),
            title: title.trim().into(),
            body: body.trim().into(),
            agents,
            cards,
        })
    }

    /// A throwaway worktree of the harness configuration clone with `branch` checked out, named `name`, and the agent
    /// `agent` as the branch defines it, which is what a trial of the branch (see `trial`) runs. The agent's skills and
    /// extensions are files of the worktree, so the caller removes it (`remove_worktree`) once the run has ended.
    pub fn agent_on_branch(
        &self,
        app: &App,
        branch: &str,
        name: &str,
        agent: &str,
    ) -> Result<(AgentConfig, PathBuf)> {
        let _read_at = self.lock();
        let root = app
            .repos
            .add_detached_worktree("harness", &self.path, name, branch)?;
        let layout = Layout::read(&root, &self.mcp_env);
        match read_agent_at(&root, &layout, agent) {
            Ok((definition, mcp)) => Ok((agent_config(&root, &definition, mcp), root)),
            Err(why) => {
                let _ = app.repos.remove_worktree(&self.path, &root);
                Err(AppError::Invalid(why))
            }
        }
    }

    /// Removes a worktree made by `agent_on_branch`.
    pub fn remove_worktree(&self, app: &App, root: &Path) {
        let _ = app.repos.remove_worktree(&self.path, root);
    }

    /// A person merges the proposal `branch`: a merge commit on the default branch, pushed to `origin`. A merge that
    /// conflicts, and a push that fails, leave the clone as it was. On success the agents are read again and the
    /// person's decision is recorded; the merge commit's short id is given.
    pub fn merge_proposal(
        &self,
        app: &App,
        by: &Actor,
        branch: &str,
        title: &str,
        reason: &str,
    ) -> Result<String> {
        let mut read_at = self.lock();
        let before = git(&self.path, &["rev-parse", "HEAD"])?;
        let default = self.repos.default_branch(&self.path)?;
        let message = format!("Merge the proposal: {title}");
        self.repos.merge(&self.path, branch, &message)?;
        let pushed = app
            .git_auth(&self.url)
            .and_then(|header| Ok(self.repos.push(&self.path, &default, header.as_deref())?));
        if let Err(e) = pushed {
            let _ = git(&self.path, &["reset", "--hard", "--quiet", &before]);
            return Err(e);
        }
        let head = git(&self.path, &["rev-parse", "HEAD"])?;
        let short = git(&self.path, &["rev-parse", "--short", "HEAD"])?;
        self.read(app, &mut read_at, head);
        self.record_proposal(app, by, branch, title, "merge_proposal", reason)?;
        Ok(short)
    }

    /// A person dismisses the proposal `branch`; nothing changes in the repository.
    pub fn dismiss_proposal(
        &self,
        app: &App,
        by: &Actor,
        branch: &str,
        title: &str,
        reason: &str,
    ) -> Result<()> {
        self.record_proposal(app, by, branch, title, "dismiss", reason)
    }

    fn record_proposal(
        &self,
        app: &App,
        by: &Actor,
        branch: &str,
        title: &str,
        choice: &str,
        reason: &str,
    ) -> Result<()> {
        app.decisions.record_by(
            by,
            &Question {
                kind: "merge_proposal".into(),
                context: BTreeMap::from([
                    ("branch".to_string(), branch.to_string()),
                    ("proposal".to_string(), title.to_string()),
                ]),
                options: vec!["merge_proposal".into(), "dismiss".into()],
                fail_closed: "dismiss".into(),
                run_id: None,
                card_id: None,
            },
            choice,
            reason,
        )?;
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
        let names = agent_names(&self.path);
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
        read_agent_at(&self.path, layout, name)
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
        by: &Actor,
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
            ("agent".to_string(), name.to_string()),
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
        let files = format!("agents/{name}.*");
        let commit = self.commit(
            app,
            by,
            &mut read_at,
            &Edit {
                kind: "edit_agent",
                files: &files,
                message: &message,
            },
            context,
            write,
        )?;
        Ok(AgentView {
            name: name.into(),
            definition: definition.clone(),
            commit,
        })
    }

    /// A person deletes the agent `name`, the same way.
    pub fn delete_agent(&self, app: &App, by: &Actor, name: &str) -> Result<()> {
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
        let files = format!("agents/{name}.*");
        let edit = Edit {
            kind: "edit_agent",
            files: &files,
            message: &message,
        };
        let context = BTreeMap::from([("agent".to_string(), name.to_string())]);
        self.commit(app, by, &mut read_at, &edit, context, write)
            .map(drop)
    }

    /// A person replaces the server's rules, the `rules.md` of the repository: committed on the default branch and
    /// pushed to `origin`, the same way. The text must pass `rules::check`. The short id of the commit is given;
    /// text that changes nothing makes no commit. Every tool call reads the file afresh, so the next one is checked
    /// against the new rules.
    pub fn save_rules(&self, app: &App, by: &Actor, text: &str) -> Result<Option<String>> {
        let mut read_at = self.lock();
        rules::check(text).map_err(AppError::Invalid)?;
        let mut text = text.to_string();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        let edit = Edit {
            kind: "edit_rules",
            files: "rules.md",
            message: "Update the rules",
        };
        let context = BTreeMap::from([("text".to_string(), text.clone())]);
        let write = || Ok(std::fs::write(self.path.join("rules.md"), &text)?);
        self.commit(app, by, &mut read_at, &edit, context, write)
    }

    /// Makes the change `write` does to the files of `edit` a commit on the default branch and pushes it. Whatever
    /// fails before the push is through, the working tree and the default branch are as they were. On success the
    /// agents are read again, the person's decision is recorded and the short id of the commit is given; a change
    /// that changed nothing makes no commit.
    fn commit(
        &self,
        app: &App,
        by: &Actor,
        read_at: &mut String,
        edit: &Edit,
        mut context: BTreeMap<String, String>,
        write: impl FnOnce() -> Result<()>,
    ) -> Result<Option<String>> {
        let Edit {
            kind,
            files,
            message,
        } = *edit;
        let before = git(&self.path, &["rev-parse", "HEAD"]).map_err(|_| {
            AppError::Invalid("the harness configuration repository has no commits".into())
        })?;
        let undo = |error: AppError| {
            let _ = git(&self.path, &["reset", "--hard", "--quiet", &before]);
            let _ = git(&self.path, &["clean", "-fdq", "--", files]);
            error
        };
        write().map_err(undo)?;
        git(&self.path, &["add", "-A", "--", files]).map_err(|e| undo(e.into()))?;
        if git(&self.path, &["diff", "--cached", "--quiet"]).is_ok() {
            return Ok(None);
        }
        let branch = self
            .repos
            .default_branch(&self.path)
            .map_err(|e| undo(e.into()))?;
        git(&self.path, &["commit", "--quiet", "-m", message]).map_err(|e| undo(e.into()))?;
        let header = app.git_auth(&self.url).map_err(undo)?;
        self.repos
            .push(&self.path, &branch, header.as_deref())
            .map_err(|e| undo(e.into()))?;
        let head = git(&self.path, &["rev-parse", "HEAD"])?;
        let short = git(&self.path, &["rev-parse", "--short", "HEAD"])?;
        self.read(app, read_at, head);
        context.insert("change".into(), message.into());
        app.decisions.record_by(
            by,
            &Question {
                kind: kind.into(),
                context,
                options: vec![kind.into(), "leave".into()],
                fail_closed: "leave".into(),
                run_id: None,
                card_id: None,
            },
            kind,
            "",
        )?;
        Ok(Some(short))
    }
}

/// A change a person makes to the repository's files, as `HarnessRepo::commit` takes it.
#[derive(Clone, Copy)]
struct Edit<'a> {
    /// The kind of the decision recorded once the change is pushed.
    kind: &'a str,
    /// The pathspec of the files the change touches.
    files: &'a str,
    /// The commit message.
    message: &'a str,
}

/// The agent `name` as its files in the repository at `root` say, or why they cannot be used.
fn read_agent_at(
    root: &Path,
    layout: &Layout,
    name: &str,
) -> std::result::Result<(AgentDefinition, BTreeMap<String, McpServer>), String> {
    let file = |extension: &str| root.join("agents").join(format!("{name}.{extension}"));
    if !valid_agent_name(name) {
        return Err(format!(
            "{name:?} is not a valid agent name (letters, digits, - and _)"
        ));
    }
    let text = std::fs::read_to_string(file("toml")).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("agents/{name}.md has no agents/{name}.toml"),
        _ => format!("cannot read agents/{name}.toml: {e}"),
    })?;
    let settings: AgentSettings =
        toml::from_str(&text).map_err(|e| format!("agents/{name}.toml: {e}"))?;
    let mcp = layout
        .check(root, &settings)
        .map_err(|e| format!("agents/{name}.toml: {e}"))?;
    let instructions = match std::fs::read_to_string(file("md")) {
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

/// The names of the agents that have a file in `root/agents`.
fn agent_names(root: &Path) -> BTreeSet<String> {
    std::fs::read_dir(root.join("agents"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let (stem, extension) = file.rsplit_once('.')?;
            matches!(extension, "toml" | "md").then(|| stem.to_string())
        })
        .collect()
}

/// Why each agent of the repository at `root` cannot be used.
fn broken_agents(root: &Path, mcp_env: &[String]) -> BTreeMap<String, String> {
    let layout = Layout::read(root, mcp_env);
    agent_names(root)
        .into_iter()
        .filter_map(|name| Some((name.clone(), read_agent_at(root, &layout, &name).err()?)))
        .collect()
}

/// A branch of the harness configuration clone that is fit to be proposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposed {
    pub branch: String,
    pub title: String,
    pub body: String,
    pub agents: Vec<String>,
    /// The cards named by the `Cards:` line that ends the commit message.
    pub cards: Vec<i64>,
}

/// The commit message without its last line when that is `Cards: 48, 50`, and the numbers it names. Anything in the
/// list that is not a number is left out.
fn split_cards(message: &str) -> (String, Vec<i64>) {
    let message = message.trim_end();
    match message.rsplit_once('\n') {
        Some((rest, last)) if last.starts_with("Cards:") => {
            let cards = last["Cards:".len()..]
                .split([',', ' '])
                .filter_map(|n| n.parse().ok())
                .collect();
            (rest.trim_end().to_string(), cards)
        }
        _ => (message.to_string(), vec![]),
    }
}

/// Whether `file` is one of the files the harness configuration consists of.
fn is_configuration(file: &str) -> bool {
    ["agents/", "skills/", "extensions/"]
        .iter()
        .any(|dir| file.starts_with(dir))
        || matches!(file, "rules.md" | "mcp.toml")
}

fn remove_if_there(file: &Path) -> Result<()> {
    match std::fs::remove_file(file) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cards_are_the_last_line_of_a_commit_message() {
        let (message, cards) = split_cards("Title\n\nBody.\n\nCards: 48, 50\n");
        assert_eq!((message.as_str(), cards), ("Title\n\nBody.", vec![48, 50]));
        let (message, cards) = split_cards("Title\n\nCards: 7,x, 9 ,");
        assert_eq!((message.as_str(), cards), ("Title", vec![7, 9]));
        // Only the last line counts, and a message without the line has no cards.
        let (message, cards) = split_cards("Title\n\nCards: 1\n\nMore.");
        assert_eq!(
            (message.as_str(), cards),
            ("Title\n\nCards: 1\n\nMore.", vec![])
        );
        let (message, cards) = split_cards("Title");
        assert_eq!((message.as_str(), cards), ("Title", vec![]));
    }
}
