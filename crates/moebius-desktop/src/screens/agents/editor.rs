//! The editor of one agent: what it holds, what a save sends, and how an agent of `moebius.toml` is read from the
//! server's settings and written back as a table.

use iced::widget::text_editor;

use crate::client::{AgentDetail, AgentSettings, BuiltInAgent, ClientError, ServerSettings};
use crate::widgets::form::PROTOCOLS;

use super::{Files, Notice};

pub(super) struct Editor {
    pub(super) name: String,
    /// Whether the name is still being typed, as for a new agent.
    pub(super) naming: bool,
    /// Whether the agent has files in the repository, so that it can be deleted.
    pub(super) exists: bool,
    pub(super) files: Files,
    pub(super) model: String,
    /// Whether the Model field is an input rather than the list.
    pub(super) typing_model: bool,
    pub(super) protocol: String,
    pub(super) thinking: Option<String>,
    pub(super) budget: String,
    pub(super) instructions: text_editor::Content,
    /// Whether the Moebius agent writes the instructions from `wish`.
    pub(super) by_agent: bool,
    pub(super) wish: text_editor::Content,
    /// Whether the Moebius agent's chat is being started.
    pub(super) asking: bool,
    pub(super) skills: Vec<String>,
    pub(super) mcp: Vec<String>,
    pub(super) extensions: Vec<String>,
    pub(super) extension: String,
    /// The skill path being typed, for an agent of `moebius.toml`.
    pub(super) skill_path: String,
    pub(super) notice: Option<Notice>,
    /// Whether "Delete" asks to be confirmed.
    pub(super) confirming: bool,
    /// What a built-in agent brings itself, shown read only.
    pub(super) built_in: Option<BuiltInAgent>,
    /// What a save would send for the agent as it was read or last saved; none for an agent that has no files.
    pub(super) kept: Option<(String, AgentSettings, String)>,
}

impl Editor {
    pub(super) fn blank(name: &str) -> Self {
        Editor {
            name: name.to_string(),
            naming: false,
            exists: false,
            files: Files::Ready,
            model: String::new(),
            typing_model: false,
            protocol: PROTOCOLS[0].to_string(),
            thinking: None,
            budget: String::new(),
            instructions: text_editor::Content::new(),
            by_agent: false,
            wish: text_editor::Content::new(),
            asking: false,
            skills: vec![],
            mcp: vec![],
            extensions: vec![],
            extension: String::new(),
            skill_path: String::new(),
            notice: None,
            confirming: false,
            built_in: None,
            kept: None,
        }
    }

    pub(super) fn fill(&mut self, detail: AgentDetail) {
        let AgentDetail {
            settings,
            instructions,
            built_in,
            ..
        } = detail;
        self.files = Files::Ready;
        if built_in.is_some() {
            self.built_in = built_in;
        }
        // A built-in agent without files is edited from blank; saving it makes them.
        let Some(settings) = settings else {
            return;
        };
        self.exists = true;
        self.model = settings.model;
        self.protocol = settings.protocol;
        self.thinking = settings.thinking;
        self.budget = settings
            .token_budget
            .map(|n| n.to_string())
            .unwrap_or_default();
        self.instructions = text_editor::Content::with_text(&instructions);
        self.skills = settings.skills;
        self.mcp = settings.mcp;
        self.extensions = settings.extensions;
        self.kept = self.request().ok();
    }

    /// Whether the fields hold changes that are not saved.
    pub(super) fn unsaved(&self) -> bool {
        self.request().ok() != self.kept
    }

    /// The agent's name and what a `PUT` sends for it, or what is wrong with the fields.
    pub(super) fn request(&self) -> Result<(String, AgentSettings, String), String> {
        let name = self.name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("A name is made of letters, digits, - and _.".into());
        }
        if self.model.trim().is_empty() {
            return Err("Enter the model.".into());
        }
        let token_budget = match self.budget.trim() {
            "" => None,
            budget => match budget.parse::<i64>() {
                Ok(tokens) if tokens >= 1 => Some(tokens),
                _ => return Err("The token budget is a whole number of at least 1.".into()),
            },
        };
        let settings = AgentSettings {
            model: self.model.trim().to_string(),
            protocol: self.protocol.clone(),
            thinking: self.thinking.clone(),
            token_budget,
            skills: self.skills.clone(),
            extensions: self.extensions.clone(),
            mcp: self.mcp.clone(),
        };
        let mut instructions = self.instructions.text();
        // The editor keeps a final line break of its own; the file is not given a second one.
        if instructions.ends_with('\n') {
            instructions.pop();
        }
        Ok((name.to_string(), settings, instructions))
    }
}

/// Toggles `name` in `chosen`.
pub(super) fn toggle(chosen: &mut Vec<String>, name: String) {
    match chosen.iter().position(|held| *held == name) {
        Some(at) => {
            chosen.remove(at);
        }
        None => chosen.push(name),
    }
}

/// Adds to `names` what an agent has chosen that is not on offer, so that it can be taken off.
pub(super) fn add_chosen<'a>(names: &mut Vec<&'a str>, chosen: &'a [String]) {
    for name in chosen {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
}

/// The agent `name` as the `[agents.<name>]` table of `moebius.toml` says, or a 404 when the file has none.
pub(super) fn from_file(name: &str, server: &ServerSettings) -> Result<AgentDetail, ClientError> {
    let Some(table) = server.value("agents").get(name) else {
        return Err(ClientError::Server {
            status: 404,
            message: format!("agent {name} not found"),
        });
    };
    let text = |key: &str| table.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let paths = |key: &str| -> Vec<String> {
        table
            .get(key)
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    Ok(AgentDetail {
        name: name.to_string(),
        settings: Some(AgentSettings {
            model: text("model").unwrap_or_default(),
            protocol: text("protocol").unwrap_or_else(|| PROTOCOLS[0].to_string()),
            thinking: text("thinking"),
            token_budget: table.get("token_budget").and_then(|v| v.as_i64()),
            skills: paths("skills"),
            extensions: paths("extensions"),
            mcp: vec![],
        }),
        instructions: text("instructions").unwrap_or_default(),
        built_in: None,
        commit: None,
    })
}

/// The `[agents.<name>]` table that `agent` and its `instructions` are saved as.
pub(super) fn file_table(agent: &AgentSettings, instructions: &str) -> serde_json::Value {
    let mut table = serde_json::Map::new();
    table.insert("model".into(), agent.model.clone().into());
    table.insert("protocol".into(), agent.protocol.clone().into());
    if let Some(thinking) = &agent.thinking {
        table.insert("thinking".into(), thinking.clone().into());
    }
    if !instructions.trim().is_empty() {
        table.insert("instructions".into(), instructions.into());
    }
    if let Some(budget) = agent.token_budget {
        table.insert("token_budget".into(), budget.into());
    }
    for (key, paths) in [("skills", &agent.skills), ("extensions", &agent.extensions)] {
        if !paths.is_empty() {
            table.insert(key.into(), paths.clone().into());
        }
    }
    table.into()
}
