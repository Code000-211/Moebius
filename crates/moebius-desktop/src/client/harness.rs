//! The harness configuration as the desktop API has it: agents, rules, skills, MCP servers, and the files uploaded to
//! import them.

use serde::{Deserialize, Serialize};

/// An agent the server can run, as `GET /v1/agents` lists it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Agent {
    pub name: String,
    pub model: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub token_budget: Option<i64>,
    /// The harness it runs on, `pi-bolt` or `claude-code`.
    #[serde(default)]
    pub harness: String,
    /// Whether Moebius brings the agent itself.
    #[serde(default)]
    pub built_in: bool,
    /// Why the agent's files cannot be used; such an agent cannot run.
    #[serde(default)]
    pub error: Option<String>,
}

/// The settings of an agent of the harness configuration repository, as `GET` and `PUT /v1/agents/{name}` have them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSettings {
    pub model: String,
    pub protocol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub mcp: Vec<String>,
    /// The harness it runs on; `pi-bolt` when none is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    #[serde(default, skip_serializing_if = "ToolOutput::is_default")]
    pub tool_output: ToolOutput,
    /// The runner its runs go to; the server itself when none is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    /// Whether it is given the engineering skills Moebius brings for it (only the Coder, the Reviewer and the Grilling
    /// partner have any).
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub engineering_skills: bool,
}

fn yes() -> bool {
    true
}

fn is_yes(value: &bool) -> bool {
    *value
}

/// A runner an agent can name, as `GET /v1/runners` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Runner {
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub user: Option<String>,
    pub max_runs: u32,
    /// How many runs go on it now.
    #[serde(default)]
    pub busy: u32,
}

/// How the outputs of an agent's tools are kept small: older ones hidden, each one compressed and cut to a budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolOutput {
    /// Outputs older than this many turns are hidden; 0 keeps them all.
    pub hide_after_turns: u32,
    /// The most lines of one result; 0 is no limit.
    pub max_lines: u32,
    /// The most bytes of one result; 0 is no limit.
    pub max_bytes: u32,
    pub compress: bool,
}

impl Default for ToolOutput {
    fn default() -> Self {
        Self {
            hide_after_turns: 10,
            max_lines: 200,
            max_bytes: 16384,
            compress: true,
        }
    }
}

impl ToolOutput {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// An agent: what `GET /v1/agents/{name}` answers, and `PUT` after it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AgentDetail {
    pub name: String,
    /// The settings of its files in the harness configuration repository; none for a built-in agent without files.
    #[serde(default)]
    pub settings: Option<AgentSettings>,
    #[serde(default)]
    pub instructions: String,
    /// What a built-in agent brings itself, read only.
    #[serde(default)]
    pub built_in: Option<BuiltInAgent>,
    /// The short id of the commit a `PUT` made; none when the edit changed nothing.
    #[serde(default)]
    pub commit: Option<String>,
}

/// The built-in instructions and skills of a built-in agent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BuiltInAgent {
    pub instructions: String,
    #[serde(default)]
    pub skills: Vec<BuiltInSkill>,
}

/// A built-in skill of a built-in agent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BuiltInSkill {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// A built-in rule, as `GET /v1/rules` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RulePattern {
    pub name: String,
    pub description: String,
}

/// A rule of a `rules.md`: its name and what it says.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Rule {
    pub name: String,
    pub text: String,
}

/// The rules in prose of the server, as `GET /v1/rules` has them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProseRules {
    /// `harness`, `file` or none.
    #[serde(default)]
    pub source: Option<String>,
    /// Whether `PUT /v1/rules` can change the text.
    #[serde(default)]
    pub editable: bool,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub error: Option<String>,
}

/// A repository's own `.moebius/rules.md`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepositoryRules {
    pub repository: String,
    /// The repository's id, to save its `.moebius/rules.md` with.
    #[serde(default)]
    pub repository_id: i64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub error: Option<String>,
}

/// What `GET` and `PUT /v1/rules` answer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RulesView {
    pub patterns: Vec<RulePattern>,
    pub prose: ProseRules,
    #[serde(default)]
    pub repositories: Vec<RepositoryRules>,
    /// The refusals of the last 7 days by rule name.
    #[serde(default)]
    pub refusals: std::collections::BTreeMap<String, i64>,
    /// The short id of the commit a `PUT` made; none when the text changed nothing.
    #[serde(default)]
    pub commit: Option<String>,
}

/// A skill an agent can name, as `GET /v1/skills` lists it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Skill {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub built_in: bool,
    /// Where a built-in skill was copied from.
    #[serde(default)]
    pub source: Option<SkillSource>,
}

/// The repository a built-in skill was copied from and the commit it was copied at.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkillSource {
    pub url: String,
    pub commit: String,
}

/// A skill as `GET /v1/skills/{name}` answers it: its description, the text of its `SKILL.md` and the other files.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkillDetail {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub text: String,
    /// The paths of the skill's other files, each read and edited on its own.
    #[serde(default)]
    pub files: Vec<String>,
    /// The short id of the commit a `PUT` made; none when the edit changed nothing.
    #[serde(default)]
    pub commit: Option<String>,
    /// Where a built-in skill was copied from.
    #[serde(default)]
    pub source: Option<SkillSource>,
}

/// An MCP server an agent can name, as `GET /v1/mcp-servers` lists it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct McpServer {
    pub name: String,
    /// `stdio` or `remote`.
    pub kind: String,
    #[serde(default)]
    pub error: Option<String>,
}

/// What an MCP server is: a command with arguments and environment, or a URL with headers. Values may be `${NAME}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct McpDefinition {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
}

/// An MCP server as `GET /v1/mcp-servers/{name}` answers it, and `PUT` after it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct McpServerDetail {
    pub name: String,
    #[serde(flatten)]
    pub definition: McpDefinition,
    /// The short id of the commit a `PUT` made; none when the edit changed nothing.
    #[serde(default)]
    pub commit: Option<String>,
}

/// A file the server stored: `path` is where it is on the server.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Upload {
    pub id: String,
    pub path: String,
}
