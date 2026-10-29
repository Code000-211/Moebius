//! Reading the clone: the agents of `agents/`, checked against the skills of `skills/` and the servers of `mcp.toml`
//! they name, as the API shows them and as a run uses them, and the skills and MCP servers themselves.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::HarnessRepo;
use crate::app::{App, AppError, Result};
use crate::bus::AppEvent;
use crate::config::{
    AgentConfig, AgentSet, HarnessKind, ToolOutput, default_protocol, default_true,
    valid_agent_name,
};
use crate::harness::mcp::{McpServer, parse_file};
use crate::harness::skill;
use crate::router::Protocol;

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
    /// The harness the agent runs on, `pi-bolt` unless it says.
    #[serde(default, skip_serializing_if = "HarnessKind::is_default")]
    pub harness: HarnessKind,
    /// How the outputs of its tools are kept small (the table `[tool_output]`); left out when it is the default.
    #[serde(default, skip_serializing_if = "ToolOutput::is_default")]
    pub tool_output: ToolOutput,
    /// Whether it is given the engineering skills Moebius brings for it; on unless the file says `false`.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub engineering_skills: bool,
}

fn is_true(value: &bool) -> bool {
    *value
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

/// Where a skill Moebius copied from elsewhere comes from: the repository and the commit it was copied at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSource {
    pub url: String,
    pub commit: String,
}

impl SkillSource {
    /// The source of the engineering skills.
    fn vendored() -> Self {
        SkillSource {
            url: skill::VENDORED_URL.into(),
            commit: skill::VENDORED_COMMIT.into(),
        }
    }
}

/// A skill an agent can name, as `GET /v1/skills` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    /// The `description` of the skill's frontmatter; empty when it has none.
    pub description: String,
    /// Whether Moebius brings the skill itself, as it does `moebius`.
    pub built_in: bool,
    /// Where a built-in skill was copied from; left out for the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SkillSource>,
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

/// A skill as `GET /v1/skills/{name}` gives it, and `PUT` after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillView {
    pub name: String,
    pub description: String,
    /// The `SKILL.md` after its frontmatter.
    pub text: String,
    /// The paths of the skill's other files, relative to its directory.
    pub files: Vec<String>,
    /// The short id of the commit a `PUT` made; left out by `GET` and by a `PUT` that changed nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Where a built-in skill was copied from; left out for the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SkillSource>,
}

/// What `PUT /v1/skills/{name}` takes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillText {
    pub description: String,
    #[serde(default)]
    pub text: String,
}

/// An MCP server as `GET /v1/mcp-servers/{name}` gives it: its table of `mcp.toml`, with the `${NAME}` references of
/// its values and never what they stand for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpServerView {
    pub name: String,
    #[serde(flatten)]
    pub server: McpServer,
    /// The short id of the commit a `PUT` made; left out by `GET` and by a `PUT` that changed nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

/// What an agent's settings may name, as the repository has it.
pub(super) struct Layout {
    skills: BTreeSet<String>,
    /// The servers of `mcp.toml`, or why it cannot be read.
    mcp: std::result::Result<BTreeMap<String, std::result::Result<McpServer, String>>, String>,
}

impl Layout {
    pub(super) fn read(root: &Path, mcp_env: &[String]) -> Self {
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
    pub(super) fn check(
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
        let extensions = !settings.extensions.is_empty();
        if let Some(why) = settings.harness.refuses(settings.protocol, extensions) {
            return Err(why.into());
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
pub(super) fn agent_config(
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
        harness: settings.harness,
        tool_output: settings.tool_output,
        engineering_skills: settings.engineering_skills,
    }
}

impl HarnessRepo {
    /// Reads every agent from the working tree, which is the default branch, into the shared set. The error of each
    /// agent that is left out is published as `harness_error`.
    pub(super) fn read(&self, app: &App, read_at: &mut String, head: String) {
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
        set.skill_overrides = skill::VENDORED
            .iter()
            .map(|vendored| (vendored.name, self.path.join("skills").join(vendored.name)))
            .filter(|(_, path)| path.join("SKILL.md").is_file())
            .map(|(name, path)| (name.to_string(), path))
            .collect();
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

    /// The built-in skills, then the engineering skills that a skill of `skills/` does not replace, then those of
    /// `skills/` by name. A directory with the name of a built-in skill is left out, since agents cannot name it.
    pub fn skills(&self) -> Vec<SkillInfo> {
        let _read_at = self.lock();
        let built_in = skill::all().map(|built_in| SkillInfo {
            name: built_in.name.into(),
            description: built_in.description(),
            built_in: true,
            source: None,
        });
        let vendored = skill::VENDORED
            .iter()
            .filter(|vendored| self.replacement(vendored.name).is_none())
            .map(|vendored| SkillInfo {
                name: vendored.name.into(),
                description: vendored.description(),
                built_in: true,
                source: Some(SkillSource::vendored()),
            });
        let own = self
            .layout()
            .skills
            .into_iter()
            .filter(|name| skill::built_in(name).is_none())
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
                    source: None,
                }
            });
        built_in.chain(vendored).chain(own).collect()
    }

    /// The engineering skill `name` as Moebius brings it, unless a skill of `skills/` with its name replaces it.
    pub(super) fn vendored(&self, name: &str) -> Option<&'static skill::BuiltIn> {
        skill::vendored(name).filter(|_| self.replacement(name).is_none())
    }

    /// The `SKILL.md` of the skill of `skills/` named `name`, if there is one.
    fn replacement(&self, name: &str) -> Option<PathBuf> {
        Some(self.path.join("skills").join(name).join("SKILL.md")).filter(|page| page.is_file())
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

    /// The skill `name` as its `SKILL.md` says. A built-in skill is read from the binary.
    pub fn skill(&self, name: &str) -> Result<SkillView> {
        let _read_at = self.lock();
        if let Some(built_in) = skill::built_in(name).or_else(|| self.vendored(name)) {
            return Ok(SkillView {
                name: name.into(),
                description: built_in.description(),
                text: skill::body(built_in.page()).into(),
                files: built_in.files().into_iter().map(String::from).collect(),
                commit: None,
                source: skill::vendored(name).map(|_| SkillSource::vendored()),
            });
        }
        let page = std::fs::read_to_string(self.path.join("skills").join(name).join("SKILL.md"))
            .ok()
            .filter(|_| valid_agent_name(name))
            .ok_or_else(|| AppError::NotFound(format!("skill {name}")))?;
        Ok(self.skill_view(name, &page, None))
    }

    pub(super) fn skill_view(&self, name: &str, page: &str, commit: Option<String>) -> SkillView {
        let dir = self.path.join("skills").join(name);
        let mut files = vec![];
        files_below(&dir, &dir, &mut files);
        files.retain(|file| file != "SKILL.md");
        files.sort();
        SkillView {
            name: name.into(),
            description: skill::description(page).unwrap_or_default(),
            text: skill::body(page).into(),
            files,
            commit,
            source: None,
        }
    }

    /// The MCP server `name` as `mcp.toml` has it, or why it cannot be used.
    pub fn mcp_server(&self, name: &str) -> Result<McpServerView> {
        let _read_at = self.lock();
        let servers = self.layout().mcp.map_err(AppError::Invalid)?;
        match servers.get(name) {
            None => Err(AppError::NotFound(format!("MCP server {name}"))),
            Some(Err(error)) => Err(AppError::Invalid(format!(
                "the MCP server {name:?} in mcp.toml: {error}"
            ))),
            Some(Ok(server)) => Ok(McpServerView {
                name: name.into(),
                server: server.clone(),
                commit: None,
            }),
        }
    }
}

/// Adds to `files` the paths, relative to `root`, of the files below `dir`.
fn files_below(root: &Path, dir: &Path, files: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_below(root, &path, files);
        } else if let Ok(relative) = path.strip_prefix(root) {
            files.push(relative.to_string_lossy().into_owned());
        }
    }
}

/// The agent `name` as its files in the repository at `root` say, or why they cannot be used.
pub(super) fn read_agent_at(
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
pub(super) fn agent_names(root: &Path) -> BTreeSet<String> {
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
pub(super) fn broken_agents(root: &Path, mcp_env: &[String]) -> BTreeMap<String, String> {
    let layout = Layout::read(root, mcp_env);
    agent_names(root)
        .into_iter()
        .filter_map(|name| Some((name.clone(), read_agent_at(root, &layout, &name).err()?)))
        .collect()
}
