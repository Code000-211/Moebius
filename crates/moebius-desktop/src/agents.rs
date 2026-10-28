//! The Agents screen: the agents the server can run and an editor for the one chosen. An edit is `PUT /v1/agents/{name}`,
//! which the server makes a commit in the harness configuration repository. A server without one keeps its agents in
//! `moebius.toml`: the same editor then reads the agent from `GET /v1/settings` and saves it with `PUT /v1/settings`
//! (`agents.<name>`), and an agent has skill and extension paths instead of the repository's skills and MCP servers.
//! The skills and MCP servers an agent can name come from `GET /v1/skills` and `GET /v1/mcp-servers`,.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, pick_list, row, scrollable, space, text, text_editor, text_input,
};
use iced::{Alignment, Border, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{
    Agent, AgentDetail, AgentSettings, Available, AvailableModel, BuiltInAgent, Client,
    ClientError, McpServer, Proposal, Run, ServerSettings, Skill,
};
use crate::live::say;
use crate::model_picker;
use crate::page::{self, Tone};
use crate::settings::Settings;
use crate::{connect, retro, theme};

/// Pi's thinking levels, as `--thinking` takes them.
const LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
const PROTOCOLS: [&str; 2] = ["openai", "anthropic"];
/// What the thinking list says for no level at all.
const NOT_SET: &str = "Not set";

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
    ThinkingChosen(String),
    BudgetEdited(String),
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

struct Editor {
    name: String,
    /// Whether the name is still being typed, as for a new agent.
    naming: bool,
    /// Whether the agent has files in the repository, so that it can be deleted.
    exists: bool,
    files: Files,
    model: String,
    /// Whether the Model field is an input rather than the list.
    typing_model: bool,
    protocol: String,
    thinking: Option<String>,
    budget: String,
    instructions: text_editor::Content,
    /// Whether the Moebius agent writes the instructions from `wish`.
    by_agent: bool,
    wish: text_editor::Content,
    /// Whether the Moebius agent's chat is being started.
    asking: bool,
    skills: Vec<String>,
    mcp: Vec<String>,
    extensions: Vec<String>,
    extension: String,
    /// The skill path being typed, for an agent of `moebius.toml`.
    skill_path: String,
    notice: Option<Notice>,
    /// Whether "Delete" asks to be confirmed.
    confirming: bool,
    /// What a built-in agent brings itself, shown read only.
    built_in: Option<BuiltInAgent>,
}

impl Editor {
    fn blank(name: &str) -> Self {
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
        }
    }

    fn fill(&mut self, detail: AgentDetail) {
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
    }

    /// The agent's name and what a `PUT` sends for it, or what is wrong with the fields.
    fn request(&self) -> Result<(String, AgentSettings, String), String> {
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

/// Toggles `name` in `chosen`.
fn toggle(chosen: &mut Vec<String>, name: String) {
    match chosen.iter().position(|held| *held == name) {
        Some(at) => {
            chosen.remove(at);
        }
        None => chosen.push(name),
    }
}

/// Adds to `names` what an agent has chosen that is not on offer, so that it can be taken off.
fn add_chosen<'a>(names: &mut Vec<&'a str>, chosen: &'a [String]) {
    for name in chosen {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
}

/// The agent `name` as the `[agents.<name>]` table of `moebius.toml` says, or a 404 when the file has none.
fn from_file(name: &str, server: &ServerSettings) -> Result<AgentDetail, ClientError> {
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
fn file_table(agent: &AgentSettings, instructions: &str) -> serde_json::Value {
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

/// Whether an answer says the token is not accepted.
fn unauthorized(result: &Result<impl Sized, ClientError>) -> bool {
    matches!(result, Err(ClientError::Unauthorized))
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

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        let in_file = self.in_file();
        match message {
            Message::Listed(Ok(list)) => self.list = list,
            Message::SkillsListed(Ok(skills)) => self.skills = Some(skills),
            Message::McpListed(Ok(mcp)) => self.mcp = mcp,
            Message::ProposalsListed(Ok(proposals)) => self.proposals = proposals,
            Message::Available(Ok(available)) => self.available = Some(available),
            Message::Available(Err(_)) => {}
            Message::RefreshAvailable => return self.ask_available(settings, true),
            Message::Listed(Err(_))
            | Message::SkillsListed(Err(_))
            | Message::McpListed(Err(_))
            | Message::ProposalsListed(Err(_)) => {}
            Message::Select(name) => return self.open(name, settings),
            Message::Opened(name, result) => self.opened(&name, result),
            Message::BuiltIn(name, Ok(detail)) => {
                if let Some(editor) = self.editor.as_mut().filter(|editor| editor.name == name) {
                    editor.built_in = detail.built_in;
                }
            }
            Message::BuiltIn(_, Err(_)) => {}
            Message::OpenedFromFile(name, result) => {
                let result = result.and_then(|server| from_file(&name, &server));
                self.opened(&name, result);
            }
            Message::NewAgent => {
                self.editor = Some(Editor {
                    naming: true,
                    ..Editor::blank("")
                });
            }
            Message::Save => return self.save(settings),
            Message::AskAgent => {
                let Some(editor) = &mut self.editor else {
                    return Task::none();
                };
                let wanted = editor.wish.text().trim().to_string();
                if wanted.is_empty() || editor.asking {
                    return Task::none();
                }
                editor.asking = true;
                editor.notice = None;
                let client = Client::new(settings.clone());
                let message = instructions_request(&editor.name, &wanted);
                return Task::perform(
                    async move { client.ask(&message, None, "").await },
                    |result| AppMessage::Agents(Message::Asked(result)),
                );
            }
            Message::Asked(result) => {
                if let Some(editor) = &mut self.editor {
                    editor.asking = false;
                    match result {
                        Ok(_) => {
                            editor.by_agent = false;
                            editor.wish = text_editor::Content::new();
                        }
                        Err(e) => editor.notice = Some(Notice::Failed(say(&e))),
                    }
                }
            }
            Message::Saved(Ok(detail)) => {
                if let Some(editor) = &mut self.editor
                    && editor.name == detail.name
                {
                    editor.naming = false;
                    editor.exists = true;
                    editor.notice = Some(if in_file {
                        Notice::Written
                    } else {
                        Notice::Saved(detail.commit)
                    });
                }
                return self.reload(settings);
            }
            Message::Delete => {
                if let Some(editor) = &self.editor {
                    let (client, name) = (Client::new(settings.clone()), editor.name.clone());
                    return Task::perform(
                        async move {
                            if in_file {
                                client
                                    .save_settings(
                                        Default::default(),
                                        vec![format!("agents.{name}")],
                                    )
                                    .await
                                    .map(drop)
                            } else {
                                client.delete_agent(&name).await
                            }
                        },
                        |result| AppMessage::Agents(Message::Deleted(result)),
                    );
                }
            }
            Message::Deleted(Ok(())) => {
                self.editor = None;
                return self.reload(settings);
            }
            Message::Saved(Err(e)) | Message::Deleted(Err(e)) => {
                if let Some(editor) = &mut self.editor {
                    editor.confirming = false;
                    editor.notice = Some(Notice::Failed(say(&e)));
                }
            }
            edit => self.edit(edit),
        }
        Task::none()
    }

    /// The changes of the editor's fields.
    fn edit(&mut self, message: Message) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        editor.notice = None;
        match message {
            Message::NameEdited(name) => editor.name = name,
            Message::ModelEdited(model) => editor.model = model,
            Message::TypeModel => editor.typing_model = true,
            Message::ModelPicked(model) => {
                editor.model = model.id;
                if PROTOCOLS.contains(&model.protocol.as_str()) {
                    editor.protocol = model.protocol;
                }
            }
            Message::ProtocolChosen(protocol) => editor.protocol = protocol,
            Message::ThinkingChosen(level) => {
                editor.thinking = (level != NOT_SET).then_some(level);
            }
            Message::BudgetEdited(budget) => editor.budget = budget,
            Message::Instructions(action) => editor.instructions.perform(action),
            Message::InstructionsByAgent(by_agent) => editor.by_agent = by_agent,
            Message::WishEdited(action) => editor.wish.perform(action),
            Message::ToggleSkill(name) => toggle(&mut editor.skills, name),
            Message::ToggleMcp(name) => toggle(&mut editor.mcp, name),
            Message::SkillPathEdited(path) => editor.skill_path = path,
            Message::AddSkillPath => {
                let path = editor.skill_path.trim().to_string();
                if !path.is_empty() && !editor.skills.contains(&path) {
                    editor.skills.push(path);
                }
                editor.skill_path.clear();
            }
            Message::RemoveSkillPath(path) => editor.skills.retain(|held| *held != path),
            Message::ExtensionEdited(path) => editor.extension = path,
            Message::AddExtension => {
                let path = editor.extension.trim().to_string();
                if !path.is_empty() && !editor.extensions.contains(&path) {
                    editor.extensions.push(path);
                }
                editor.extension.clear();
            }
            Message::RemoveExtension(path) => editor.extensions.retain(|held| *held != path),
            Message::AskDelete => editor.confirming = true,
            Message::KeepAgent => editor.confirming = false,
            _ => {}
        }
    }

    fn open(&mut self, name: String, settings: &Settings) -> Task<AppMessage> {
        let mut editor = Editor::blank(&name);
        editor.files = Files::Reading;
        self.editor = Some(editor);
        let client = Client::new(settings.clone());
        if self.in_file() {
            let built_in = self
                .list
                .iter()
                .any(|agent| agent.name == name && agent.built_in);
            let read = {
                let client = Client::new(settings.clone());
                let name = name.clone();
                Task::perform(
                    async move { client.server_settings().await },
                    move |result| AppMessage::Agents(Message::OpenedFromFile(name.clone(), result)),
                )
            };
            if !built_in {
                return read;
            }
            // The server answers a built-in agent's built-in part without a harness configuration repository.
            let built_in = Task::perform(
                {
                    let name = name.clone();
                    async move { client.agent(&name).await }
                },
                move |result| AppMessage::Agents(Message::BuiltIn(name.clone(), result)),
            );
            return Task::batch([read, built_in]);
        }
        Task::perform(
            {
                let name = name.clone();
                async move { client.agent(&name).await }
            },
            move |result| AppMessage::Agents(Message::Opened(name.clone(), result)),
        )
    }

    fn opened(&mut self, name: &str, result: Result<AgentDetail, ClientError>) {
        let built_in_without_files = self
            .list
            .iter()
            .find(|agent| agent.name == name)
            .filter(|agent| agent.built_in)
            .cloned();
        let Some(editor) = self.editor.as_mut().filter(|editor| editor.name == name) else {
            return;
        };
        match result {
            Ok(detail) => editor.fill(detail),
            // The Moebius agent is there without files; saving it makes them.
            Err(ClientError::Server { status: 404, .. }) if built_in_without_files.is_some() => {
                editor.files = Files::Ready;
            }
            Err(e) => {
                // A broken agent has files, so it can be deleted.
                editor.exists = true;
                editor.files = Files::Broken(say(&e));
            }
        }
    }

    fn save(&mut self, settings: &Settings) -> Task<AppMessage> {
        let (request, in_file) = (self.save_request(), self.in_file());
        let Some(editor) = &mut self.editor else {
            return Task::none();
        };
        match request {
            Ok((name, agent, instructions)) => {
                // An agent saved under a new name is open under it from now on.
                editor.name.clone_from(&name);
                editor.notice = None;
                let client = Client::new(settings.clone());
                if in_file {
                    let set = serde_json::Map::from_iter([(
                        format!("agents.{name}"),
                        file_table(&agent, &instructions),
                    )]);
                    return Task::perform(
                        async move {
                            client
                                .save_settings(set, vec![])
                                .await
                                .map(|_| AgentDetail {
                                    name,
                                    settings: Some(agent),
                                    instructions,
                                    built_in: None,
                                    commit: None,
                                })
                        },
                        |result| AppMessage::Agents(Message::Saved(result)),
                    );
                }
                Task::perform(
                    async move { client.save_agent(&name, &agent, &instructions).await },
                    |result| AppMessage::Agents(Message::Saved(result)),
                )
            }
            Err(problem) => {
                editor.notice = Some(Notice::Failed(problem));
                Task::none()
            }
        }
    }

    /// The Agents page: the list, and the editor of the agent chosen.
    pub fn view(&self) -> Element<'_, AppMessage> {
        let editable = self.editable();
        let new_agent = editable.then_some(AppMessage::Agents(Message::NewAgent));
        let header = page::header(
            "",
            "Agents",
            None,
            page::context(if self.in_file() {
                "What each run works with: its model, instructions, skills and extensions. An edit is saved in moebius.toml."
            } else {
                "What each run works with: its model, instructions, skills, MCP servers and extensions. An edit is a commit."
            }),
            page::white_button("New agent", new_agent),
        );
        let mut detail = column![].spacing(theme::PAGE_GAP).width(Length::Fill);
        if let Some(editor) = &self.editor {
            detail = detail.push(self.editor_view(editor));
        }
        detail = detail.push(self.proposals_box());
        column![
            header,
            row![
                scrollable(self.list_view()).width(280),
                scrollable(detail).width(Length::Fill)
            ]
            .spacing(14)
            .height(Length::Fill),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }

    /// The Retro's proposals still to be decided (open, trying, positive or untried) whose branch changes the files of
    /// the agent in the editor.
    fn proposals_box(&self) -> Element<'_, AppMessage> {
        let name = self.editor.as_ref().map(|editor| editor.name.as_str());
        let open: Vec<&Proposal> = self
            .proposals
            .iter()
            .filter(|p| {
                matches!(
                    p.status.as_str(),
                    "open" | "trying" | "positive" | "untried"
                ) && name.is_some_and(|name| p.agents.iter().any(|a| a == name))
            })
            .collect();
        if open.is_empty() {
            return page::boxed(
                "From the Retro",
                page::body("No proposal waiting for a decision changes this agent."),
            );
        }
        page::boxed(
            "From the Retro",
            column(open.into_iter().map(retro::proposal)).spacing(12),
        )
    }

    fn list_view(&self) -> Element<'_, AppMessage> {
        if self.list.is_empty() {
            return page::boxed("Agents", page::body("No agent yet."));
        }
        let chosen = self.editor.as_ref().map(|editor| editor.name.as_str());
        column(self.list.iter().map(|agent| {
            agent_card(
                agent,
                self.editable() && chosen == Some(agent.name.as_str()),
                self.editable(),
            )
        }))
        .spacing(8)
        .padding(iced::Padding::default().right(12))
        .into()
    }

    fn editor_view<'a>(&'a self, editor: &'a Editor) -> Element<'a, AppMessage> {
        let message = |message| AppMessage::Agents(message);
        let title: Element<_> = if editor.naming {
            text_input("Name of the new agent", &editor.name)
                .on_input(move |name| message(Message::NameEdited(name)))
                .size(theme::BODY)
                .padding([9, 12])
                .width(280)
                .style(connect::input_style)
                .into()
        } else {
            text(editor.name.as_str())
                .size(21)
                .font(theme::manrope(Weight::Bold))
                .into()
        };
        let ready = matches!(editor.files, Files::Ready);
        let mut actions = row![].spacing(10).align_y(Alignment::Center);
        if editor.confirming {
            actions = actions
                .push(
                    text(format!("Delete {}?", editor.name))
                        .size(theme::CONTEXT)
                        .color(theme::TEXT_SECONDARY),
                )
                .push(page::white_button(
                    "Keep",
                    Some(message(Message::KeepAgent)),
                ))
                .push(page::white_button(
                    "Delete agent",
                    Some(message(Message::Delete)),
                ));
        } else {
            if editor.exists {
                actions = actions.push(page::white_button(
                    "Delete",
                    Some(message(Message::AskDelete)),
                ));
            }
            actions = actions.push(page::primary_button(
                "Save",
                ready.then_some(message(Message::Save)),
            ));
        }
        let head = row![title, space::horizontal(), actions].align_y(Alignment::Center);
        let mut content = column![head].spacing(12);
        match &editor.files {
            Files::Reading => content = content.push(page::body("Reading the agent…")),
            Files::Broken(error) => {
                content = content
                    .push(notice_line(error, true))
                    .push(page::body(
                        "Its files cannot be read, so it cannot run and is not edited here. Fix them in the repository, or delete the agent.",
                    ));
                return self.with_boxes(editor, content, false);
            }
            Files::Ready => {}
        }
        if ready {
            content = content.push(self.settings_row(editor));
            if let Some(available) = &self.available {
                content = content.push(model_picker::note(
                    available,
                    AppMessage::Agents(Message::RefreshAvailable),
                ));
            }
        }
        if let Some(notice) = &editor.notice {
            content = content.push(match notice {
                Notice::Saved(Some(commit)) => {
                    notice_line(&format!("Saved as a commit: {commit}"), false)
                }
                Notice::Saved(None) => {
                    notice_line("Nothing changed, so nothing was committed.", false)
                }
                Notice::Written => notice_line("Saved in moebius.toml.", false),
                Notice::Failed(error) => notice_line(error, true),
            });
        }
        self.with_boxes(editor, content, ready)
    }

    /// The editor's first box and, when `ready`, the boxes under it.
    fn with_boxes<'a>(
        &'a self,
        editor: &'a Editor,
        head: iced::widget::Column<'a, AppMessage>,
        ready: bool,
    ) -> Element<'a, AppMessage> {
        let mut boxes = column![
            container(head)
                .padding(20)
                .width(Length::Fill)
                .style(|_| page::box_style())
        ]
        .spacing(theme::PAGE_GAP);
        if ready {
            let paths = if self.in_file() {
                // The agents of `moebius.toml` name their skills by path, and have no MCP servers.
                row![skill_paths_box(editor), extensions_box(editor)]
            } else {
                row![
                    self.skills_box(editor),
                    self.mcp_box(editor),
                    extensions_box(editor)
                ]
            };
            if let Some(built_in) = &editor.built_in {
                boxes = boxes
                    .push(built_in_instructions_box(built_in))
                    .push(built_in_skills_box(built_in));
            }
            boxes = boxes.push(instructions_box(editor)).push(paths.spacing(14));
        }
        boxes.into()
    }

    fn skills_box<'a>(&'a self, editor: &'a Editor) -> Element<'a, AppMessage> {
        let offered: Vec<&Skill> = self
            .skills
            .iter()
            .flatten()
            .filter(|skill| !skill.built_in)
            .collect();
        let mut names: Vec<&str> = offered.iter().map(|skill| skill.name.as_str()).collect();
        add_chosen(&mut names, &editor.skills);
        let items = names.into_iter().map(|name| {
            let description = offered
                .iter()
                .find(|skill| skill.name == name)
                .map_or("", |skill| skill.description.as_str());
            let chosen = editor.skills.iter().any(|held| held == name);
            let chip = page::chip(
                name.to_string(),
                chosen,
                false,
                AppMessage::Agents(Message::ToggleSkill(name.to_string())),
            );
            item(chip, description, false)
        });
        let aside = format!("{} of {}", editor.skills.len(), offered.len());
        section(
            "Skills",
            &aside,
            items,
            "No skill in skills/ of the repository.",
            None,
        )
    }

    fn mcp_box<'a>(&'a self, editor: &'a Editor) -> Element<'a, AppMessage> {
        let mut names: Vec<&str> = self.mcp.iter().map(|server| server.name.as_str()).collect();
        add_chosen(&mut names, &editor.mcp);
        let items = names.into_iter().map(|name| {
            let server = self.mcp.iter().find(|server| server.name == name);
            let chosen = editor.mcp.iter().any(|held| held == name);
            let broken = server.is_none_or(|server| server.error.is_some());
            let chip = page::chip(
                name.to_string(),
                chosen,
                broken,
                AppMessage::Agents(Message::ToggleMcp(name.to_string())),
            );
            match server {
                Some(McpServer {
                    error: Some(error), ..
                }) => item(chip, error, true),
                Some(server) => item(chip, &server.kind, false),
                None => item(chip, "Not in mcp.toml", true),
            }
        });
        let aside = format!("{} of {}", editor.mcp.len(), self.mcp.len());
        section(
            "MCP servers",
            &aside,
            items,
            "No server in mcp.toml of the repository.",
            None,
        )
    }
}

/// A row of a list: what names it, and a line under it that is red when something is wrong.
fn item<'a>(name: Element<'a, AppMessage>, line: &str, red: bool) -> Element<'a, AppMessage> {
    let mut content = column![name].spacing(3);
    if !line.is_empty() {
        content = content.push(text(line.to_string()).size(theme::LABEL).color(if red {
            theme::RED_TEXT
        } else {
            theme::TEXT_MUTED
        }));
    }
    content.into()
}

/// A box of the editor: a heading with a muted word on its right, and a list of items.
fn section<'a>(
    heading: &'a str,
    aside: &str,
    items: impl Iterator<Item = Element<'a, AppMessage>>,
    empty: &'a str,
    footer: Option<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    let items: Vec<_> = items.collect();
    let list: Element<_> = if items.is_empty() {
        page::body(empty)
    } else {
        column(items).spacing(10).into()
    };
    let mut content = column![
        row![
            text(heading)
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold)),
            space::horizontal(),
            text(aside.to_string())
                .size(theme::LABEL)
                .color(theme::TEXT_MUTED),
        ]
        .align_y(Alignment::Center),
        list,
    ]
    .spacing(12);
    if let Some(footer) = footer {
        content = content.push(footer);
    }
    container(content)
        .padding(20)
        .width(Length::Fill)
        .style(|_| page::box_style())
        .into()
}

/// One line about what happened to the edit, red when it failed.
pub(crate) fn notice_line<'a>(line: &str, red: bool) -> Element<'a, AppMessage> {
    text(line.to_string())
        .size(theme::CONTEXT)
        .font(theme::manrope(Weight::Semibold))
        .color(if red {
            theme::RED_TEXT
        } else {
            theme::ONLINE_TEXT
        })
        .into()
}

fn agent_card(agent: &Agent, chosen: bool, editable: bool) -> Element<'_, AppMessage> {
    let broken = agent.error.is_some();
    let mut head = row![
        text(agent.name.as_str())
            .size(theme::CARD_TITLE)
            .font(theme::manrope(Weight::Semibold)),
        space::horizontal(),
    ]
    .align_y(Alignment::Center);
    if agent.built_in {
        head = head.push(page::tag("Built in".to_string(), Tone::Plain));
    }
    if broken {
        head = head.push(page::tag("Broken".to_string(), Tone::Red));
    }
    let mut content = column![head].spacing(4);
    content = match &agent.error {
        Some(error) => content.push(
            text(error.as_str())
                .size(theme::LABEL)
                .color(theme::RED_TEXT)
                .wrapping(Wrapping::WordOrGlyph),
        ),
        None => content.push(
            text(format!("{} · {}", agent.model, agent.protocol))
                .size(theme::LABEL)
                .color(theme::TEXT_SECONDARY)
                .wrapping(Wrapping::WordOrGlyph),
        ),
    };
    button(content)
        .width(Length::Fill)
        .padding([12, 14])
        .on_press_maybe(editable.then(|| AppMessage::Agents(Message::Select(agent.name.clone()))))
        .style(move |_, status| {
            let (fill, line) = match (broken, chosen, status) {
                (true, _, _) => (theme::RED_FILL, theme::RED_BORDER),
                (false, true, _) => (theme::SURFACE, theme::TEAL),
                (false, false, button::Status::Hovered) => (theme::LANE, theme::LINE),
                (false, false, _) => (theme::SURFACE, theme::LINE),
            };
            button::Style {
                background: Some(fill.into()),
                text_color: theme::TEXT,
                border: Border {
                    color: line,
                    width: if chosen { 2.0 } else { 1.0 },
                    radius: theme::RADIUS_CARD.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

pub(crate) fn field<'a>(
    label: &'a str,
    input: impl Into<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    column![
        text(label)
            .size(13)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::TEXT_SECONDARY),
        input.into(),
    ]
    .spacing(5)
    .width(Length::FillPortion(1))
    .into()
}

pub(crate) fn pick_style(_: &iced::Theme, status: pick_list::Status) -> pick_list::Style {
    let line = match status {
        pick_list::Status::Opened { .. } => theme::TEAL,
        _ => theme::LINE,
    };
    pick_list::Style {
        text_color: theme::TEXT,
        placeholder_color: theme::TEXT_MUTED,
        handle_color: theme::TEXT_MUTED,
        background: theme::SURFACE.into(),
        border: Border {
            color: line,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
    }
}

pub(crate) fn menu_style(_: &iced::Theme) -> iced::overlay::menu::Style {
    iced::overlay::menu::Style {
        background: theme::SURFACE.into(),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        text_color: theme::TEXT,
        selected_text_color: theme::TEXT,
        selected_background: theme::LANE.into(),
        shadow: iced::Shadow::default(),
    }
}

impl Agents {
    /// Model, protocol, thinking level and token budget.
    fn settings_row<'a>(&'a self, editor: &'a Editor) -> Element<'a, AppMessage> {
        let models = self.available.as_ref().map_or(&[][..], |a| &a.models[..]);
        settings_row(editor, models)
    }
}

fn settings_row<'a>(editor: &'a Editor, models: &[AvailableModel]) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Agents(message);
    let model = model_picker::field(
        models,
        editor.typing_model,
        "The model's name",
        &editor.model,
        move |model| message(Message::ModelEdited(model)),
        move |model| message(Message::ModelPicked(model)),
        message(Message::TypeModel),
    );
    let protocols: Vec<String> = PROTOCOLS.iter().map(|p| p.to_string()).collect();
    let protocol = pick_list(protocols, Some(editor.protocol.clone()), move |p| {
        message(Message::ProtocolChosen(p))
    })
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let mut levels: Vec<String> = std::iter::once(NOT_SET)
        .chain(LEVELS)
        .map(String::from)
        .collect();
    if let Some(level) = &editor.thinking
        && !levels.contains(level)
    {
        levels.push(level.clone());
    }
    let thinking = pick_list(
        levels,
        Some(
            editor
                .thinking
                .clone()
                .unwrap_or_else(|| NOT_SET.to_string()),
        ),
        move |level| message(Message::ThinkingChosen(level)),
    )
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let budget = text_input("No limit", &editor.budget)
        .on_input(move |budget| message(Message::BudgetEdited(budget)))
        .size(theme::BODY)
        .padding([9, 12])
        .style(connect::input_style);
    row![
        field("Model", model),
        field("Protocol", protocol),
        field("Thinking", thinking),
        field("Token budget per card", budget),
    ]
    .spacing(14)
    .into()
}

/// What the Moebius agent is asked to write the instructions of the agent `name`, with what the person wants.
fn instructions_request(name: &str, wanted: &str) -> String {
    format!(
        "Write the instructions of the agent {name} and save them. Read the agent first with \
         `\"$MOEBIUS\" agent show {name}`, and the moebius skill's guide on agents. `\"$MOEBIUS\" agent save` \
         replaces the whole agent, so keep its model, protocol, thinking, token budget, skills, MCP servers and \
         extensions as they are, and pass the new instructions with `--instructions <file>`. Then say what you \
         wrote.\n\nWhat the person wants {name} to do:\n\n{wanted}"
    )
}

fn instructions_box(editor: &Editor) -> Element<'_, AppMessage> {
    let message = AppMessage::Agents;
    // A new agent is saved before the Moebius agent can change it.
    let ways: Element<'_, AppMessage> = if editor.naming {
        space().into()
    } else {
        row![
            page::chip(
                "Write them".to_string(),
                !editor.by_agent,
                false,
                message(Message::InstructionsByAgent(false))
            ),
            page::chip(
                "Let the Moebius agent write them".to_string(),
                editor.by_agent,
                false,
                message(Message::InstructionsByAgent(true))
            ),
        ]
        .spacing(8)
        .into()
    };
    let heading = row![
        text("Instructions")
            .size(theme::BOX_HEADING)
            .font(theme::manrope(Weight::Bold)),
        space::horizontal(),
        text(if editor.built_in.is_some() {
            "Markdown · added after the built-in instructions"
        } else {
            "Markdown · added to the harness's system prompt"
        })
        .size(theme::LABEL)
        .color(theme::TEXT_MUTED),
    ]
    .align_y(Alignment::Center);
    if editor.by_agent && !editor.naming {
        let ask = (!editor.asking && !editor.wish.text().trim().is_empty())
            .then_some(message(Message::AskAgent));
        let wish = text_editor(&editor.wish)
            .placeholder(
                "What this agent should do and how, in a few words. The Moebius agent writes the \
                 instructions and saves them; the chat opens.",
            )
            .on_action(|action| AppMessage::Agents(Message::WishEdited(action)))
            .size(theme::BODY)
            .padding(14)
            .height(120)
            .style(editor_style);
        return container(
            column![
                heading,
                ways,
                wish,
                row![page::primary_button("Ask the agent", ask)]
            ]
            .spacing(12),
        )
        .padding(20)
        .width(Length::Fill)
        .style(|_| page::box_style())
        .into();
    }
    let input = text_editor(&editor.instructions)
        .placeholder("What the agent is told before it starts, in Markdown.")
        .on_action(|action| AppMessage::Agents(Message::Instructions(action)))
        .font(theme::MONO)
        .size(theme::CODE)
        .padding(14)
        .height(220)
        .style(editor_style);
    container(column![heading, ways, input].spacing(12))
        .padding(20)
        .width(Length::Fill)
        .style(|_| page::box_style())
        .into()
}

/// What a built-in agent is always told, which the person cannot change.
fn built_in_instructions_box(built_in: &BuiltInAgent) -> Element<'_, AppMessage> {
    let body = container(
        text(built_in.instructions.as_str())
            .font(theme::MONO)
            .size(theme::CODE_SMALL)
            .color(theme::TEXT_SECONDARY)
            .wrapping(Wrapping::WordOrGlyph),
    )
    .padding(14)
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(theme::LANE.into()),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_CARD.into(),
        },
        ..container::Style::default()
    });
    section(
        "Built-in instructions",
        "Read only · Moebius brings them",
        std::iter::once(body.into()),
        "",
        None,
    )
}

/// The skills a built-in agent always has, before any of its own.
fn built_in_skills_box(built_in: &BuiltInAgent) -> Element<'_, AppMessage> {
    let items = built_in.skills.iter().map(|skill| {
        let name = text(skill.name.as_str())
            .font(theme::MONO)
            .size(theme::CODE)
            .color(theme::TEXT);
        item(name.into(), &skill.description, false)
    });
    let aside = match built_in.skills.len() {
        1 => "Read only · 1 skill".to_string(),
        n => format!("Read only · {n} skills"),
    };
    section("Built-in skills", &aside, items, "No built-in skill.", None)
}

fn editor_style(_: &iced::Theme, status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: theme::LANE.into(),
        border: Border {
            color: match status {
                text_editor::Status::Focused { .. } => theme::TEAL,
                _ => theme::LINE,
            },
            width: 1.0,
            radius: theme::RADIUS_CARD.into(),
        },
        placeholder: theme::TEXT_MUTED,
        value: theme::TEXT,
        selection: theme::TEAL_FILL,
    }
}

fn extensions_box(editor: &Editor) -> Element<'_, AppMessage> {
    paths_box(
        "Extensions",
        "Files in the repository",
        "No extension.",
        "extensions/log.ts",
        "Add extension",
        &editor.extensions,
        &editor.extension,
        (
            Message::ExtensionEdited,
            Message::AddExtension,
            Message::RemoveExtension,
        ),
    )
}

/// The skills of an agent of `moebius.toml`: paths to skill directories.
fn skill_paths_box(editor: &Editor) -> Element<'_, AppMessage> {
    paths_box(
        "Skills",
        "Directories, from moebius.toml",
        "No skill.",
        "skills/house-style",
        "Add skill",
        &editor.skills,
        &editor.skill_path,
        (
            Message::SkillPathEdited,
            Message::AddSkillPath,
            Message::RemoveSkillPath,
        ),
    )
}

/// What a box of paths sends: the input edited, a path added, a path removed.
type PathMessages = (fn(String) -> Message, Message, fn(String) -> Message);

/// A box with a list of paths, each of which can be removed, and an input to add one.
#[allow(clippy::too_many_arguments)]
fn paths_box<'a>(
    heading: &'a str,
    aside: &'a str,
    empty: &'a str,
    placeholder: &'a str,
    add_label: &'a str,
    chosen: &'a [String],
    input: &'a str,
    (edited, add, remove): PathMessages,
) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Agents(message);
    let paths = chosen.iter().map(|path| {
        row![
            text(path.as_str())
                .size(theme::CODE_SMALL)
                .font(theme::MONO)
                .width(Length::Fill)
                .wrapping(Wrapping::WordOrGlyph),
            button(text("Remove").size(theme::LABEL))
                .padding([3, 8])
                .on_press(message(remove(path.clone())))
                .style(|_, _| button::Style {
                    background: Some(theme::SURFACE.into()),
                    text_color: theme::TEXT_SECONDARY,
                    border: Border {
                        color: theme::LINE,
                        width: 1.0,
                        radius: theme::RADIUS_SMALL.into(),
                    },
                    ..button::Style::default()
                }),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    });
    let can_add = !input.trim().is_empty();
    let adder = column![
        text_input(placeholder, input)
            .on_input(move |path| message(edited(path)))
            .on_submit_maybe(can_add.then(|| message(add.clone())))
            .font(theme::MONO)
            .size(theme::CODE)
            .padding([8, 10])
            .style(connect::input_style),
        page::white_button(add_label, can_add.then(|| message(add))),
    ]
    .spacing(8);
    section(heading, aside, paths, empty, Some(adder.into()))
}
