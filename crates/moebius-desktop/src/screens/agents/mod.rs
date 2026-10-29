//! The Agents screen: the agents the server can run and an editor for the one chosen. An edit is `PUT /v1/agents/{name}`,
//! which the server makes a commit in the harness configuration repository. A server without one keeps its agents in
//! `moebius.toml`: the same editor then reads the agent from `GET /v1/settings` and saves it with `PUT /v1/settings`
//! (`agents.<name>`), and an agent has skill and extension paths instead of the repository's skills and MCP servers.
//! The skills and MCP servers an agent can name come from `GET /v1/skills` and `GET /v1/mcp-servers`,.
//!
//! `editor` has the editor of one agent and what a save sends, `update` how a message changes the page, `view` the
//! list and the editor's head, and `boxes` the boxes under it.

mod boxes;
mod editor;
mod update;
mod view;

use iced::Task;
use iced::widget::text_editor;

use crate::app::Message as AppMessage;
use crate::client::{
    Agent, AgentDetail, AgentSettings, Available, AvailableModel, Client, ClientError, McpServer,
    Proposal, Run, ServerSettings, Skill, unauthorized,
};
use crate::settings::Settings;

use editor::Editor;

// The pieces of the forms, which other pages took from here first.
pub(crate) use crate::widgets::form::{field, menu_style, notice_line, pick_style};

/// What the thinking list says for no level at all.
const NOT_SET: &str = "Not set";
/// What the list of whether tool outputs are compressed offers: on, then off.
const COMPRESS: [&str; 2] = ["Compressed", "As they are"];
/// What the list of whether the engineering skills are given offers: on, then off.
const ENGINEERING: [&str; 2] = ["Given", "Not given"];
/// The agents Moebius brings engineering skills for.
const ENGINEERING_AGENTS: [&str; 3] = ["Coder", "Reviewer", "Grilling-partner"];

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Vec<Agent>, ClientError>),
    SkillsListed(Result<Vec<Skill>, ClientError>),
    McpListed(Result<Vec<McpServer>, ClientError>),
    ProposalsListed(Result<Vec<Proposal>, ClientError>),
    /// The models the upstreams serve.
    Available(Result<Available, ClientError>),
    /// The link under the Model field was pressed: ask the upstreams again.
    RefreshAvailable,
    /// A model was picked from the list.
    ModelPicked(AvailableModel),
    /// An agent of the list was chosen.
    Select(String),
    /// The files of the agent with this name were read.
    Opened(String, Result<AgentDetail, ClientError>),
    /// The built-in part of the built-in agent with this name was read, for a server that keeps its agents in
    /// `moebius.toml`.
    BuiltIn(String, Result<AgentDetail, ClientError>),
    /// The settings of a server that keeps its agents in `moebius.toml` were read to open the agent with this name.
    OpenedFromFile(String, Result<ServerSettings, ClientError>),
    NewAgent,
    NameEdited(String),
    ModelEdited(String),
    /// "Another model…" was chosen: the Model field becomes an input.
    TypeModel,
    ProtocolChosen(String),
    HarnessChosen(String),
    ThinkingChosen(String),
    BudgetEdited(String),
    /// A field of the tool outputs was edited.
    HideAfterEdited(String),
    MaxLinesEdited(String),
    MaxBytesEdited(String),
    CompressChosen(String),
    EngineeringChosen(String),
    Instructions(text_editor::Action),
    /// "Write them" (false) or "Let the Moebius agent write them" (true) was chosen for the instructions.
    InstructionsByAgent(bool),
    /// What the person wants the agent to do, for the Moebius agent to write the instructions from.
    WishEdited(text_editor::Action),
    /// "Ask the agent" was pressed.
    AskAgent,
    /// The server started the Moebius agent's chat that writes the instructions, or said why not.
    Asked(Result<Run, ClientError>),
    ToggleSkill(String),
    ToggleMcp(String),
    ExtensionEdited(String),
    AddExtension,
    RemoveExtension(String),
    SkillPathEdited(String),
    AddSkillPath,
    RemoveSkillPath(String),
    Save,
    /// The server answered the edit: the agent as committed, or why not.
    Saved(Result<AgentDetail, ClientError>),
    AskDelete,
    KeepAgent,
    Delete,
    Deleted(Result<(), ClientError>),
}

/// How far the editor's agent has been read.
enum Files {
    Reading,
    /// The agent's files are broken, for this reason; it cannot be edited here.
    Broken(String),
    Ready,
}

enum Notice {
    /// The edit is a commit with this short id; none when the edit changed nothing.
    Saved(Option<String>),
    /// The edit was written to `moebius.toml`.
    Written,
    Failed(String),
}

#[derive(Default)]
pub struct Agents {
    list: Vec<Agent>,
    /// The skills, once read. A server with a harness configuration repository always lists the built-in one, so an
    /// empty list means agents are kept in `moebius.toml` and cannot be edited.
    skills: Option<Vec<Skill>>,
    mcp: Vec<McpServer>,
    /// The proposals of the Retros, newest first; those still to be decided are offered under the agent they change.
    proposals: Vec<Proposal>,
    /// What the upstreams serve, once they have been asked.
    available: Option<Available>,
    editor: Option<Editor>,
}

impl Agents {
    /// Whether the agents can be edited: once the server has said where it keeps them.
    fn editable(&self) -> bool {
        self.skills.is_some()
    }

    /// Whether the server keeps its agents in the tables of `moebius.toml`. A server with a harness configuration
    /// repository always lists the built-in skill, so an empty list means there is none.
    fn in_file(&self) -> bool {
        self.skills.as_ref().is_some_and(Vec::is_empty)
    }

    /// Whether the agent being edited has changes that are not saved; none while no agent is open.
    pub fn unsaved(&self) -> Option<bool> {
        self.editor.as_ref().map(Editor::unsaved)
    }

    /// The name of the agent being edited, if any.
    pub fn editing(&self) -> Option<&str> {
        self.editor.as_ref().map(|editor| editor.name.as_str())
    }

    /// The agents, as the server last listed them.
    pub fn list(&self) -> &[Agent] {
        &self.list
    }

    /// Reads the agents, skills and MCP servers again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let client = |settings: &Settings| Client::new(settings.clone());
        let (agents, skills, mcp) = (client(settings), client(settings), client(settings));
        let proposals = client(settings);
        Task::batch([
            self.ask_available(settings, false),
            Task::perform(async move { proposals.proposals().await }, |result| {
                AppMessage::Agents(Message::ProposalsListed(result))
            }),
            Task::perform(async move { agents.agents().await }, |result| {
                AppMessage::Agents(Message::Listed(result))
            }),
            Task::perform(async move { skills.skills().await }, |result| {
                AppMessage::Agents(Message::SkillsListed(result))
            }),
            Task::perform(async move { mcp.mcp_servers().await }, |result| {
                AppMessage::Agents(Message::McpListed(result))
            }),
        ])
    }

    /// Asks the upstreams which models they serve; `refresh` makes the server ask them again.
    fn ask_available(&self, settings: &Settings, refresh: bool) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        Task::perform(
            async move { client.available_models(refresh).await },
            |result| AppMessage::Agents(Message::Available(result)),
        )
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        match message {
            Message::Listed(result) => unauthorized(result),
            Message::SkillsListed(result) => unauthorized(result),
            Message::McpListed(result) => unauthorized(result),
            Message::ProposalsListed(result) => unauthorized(result),
            Message::Available(result) => unauthorized(result),
            _ => false,
        }
    }

    /// What a save would send: the agent's name, settings and instructions, or what is wrong with the editor.
    pub fn save_request(&self) -> Result<(String, AgentSettings, String), String> {
        match &self.editor {
            Some(editor) => editor.request(),
            None => Err("No agent is open.".into()),
        }
    }
}
