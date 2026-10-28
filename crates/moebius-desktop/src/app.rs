//! The app's state, its messages, and how one becomes the other: the connection to the server, the page shown, and
//! the views of the shell. The state is plain data; `update` is the only place it changes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use iced::widget::{column, container, opaque, row, stack};
use iced::{Element, Length, Padding, Subscription, Task};

use crate::add_repository::{self, AddRepository};
use crate::agents::{self, Agents};
use crate::chat::Chats;
use crate::client::{self, Client, ClientError, HistoryEntry, RunEvent, Snapshot, StreamEvent};
use crate::decision_model::{self, DecisionModel};
use crate::edits::{self, Edits};
use crate::experiments::{self, Experiments};
use crate::fleet::{self, Fleet};
use crate::float_chat::{self, FloatChat};
use crate::import;
use crate::inbox::{self, Inbox};
use crate::live::{self, Live};
use crate::mcp_servers::{self, McpServers};
use crate::new_issue::{self, NewIssue};
use crate::project::{self, Project, Tab};
use crate::retro::{self, Retros};
use crate::rules::{self, Rules};
use crate::script_editor::{self, ScriptEditor};
use crate::search::{self, Search, Target};
use crate::server_terminal::{self, ServerTerminal};
use crate::settings::{self, Settings};
use crate::setup::{self, Setup};
use crate::skills::{self, Skills};
use crate::start::{self, Start};
use crate::usage::{self, Usage};
use crate::{board, chat, connect, history, page, sidebar, theme};

/// What the sidebar says about the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Connecting,
    Online,
    Offline,
}

/// A page the sidebar opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Inbox,
    Chat,
    /// Every run that is working, one tile each.
    Fleet,
    /// The project of the repository with this id: its board, files and editor, in tabs.
    Board(i64),
    /// The form that adds a repository, opened by the "+" of the sidebar.
    AddRepository,
    Agents,
    Skills,
    McpServers,
    Rules,
    Retro,
    Experiments,
    DecisionModel,
    Usage,
    /// What the server still needs before the first card runs; the sidebar lists it only while something is missing.
    Start,
    Models,
    Plugins,
    /// A shell on the server.
    Terminal,
    Settings,
}

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Inbox => "Inbox",
            Page::Chat => "Chat",
            Page::Fleet => "Fleet",
            Page::Board(_) => "Board",
            Page::AddRepository => "Add a repository",
            Page::Agents => "Agents",
            Page::Skills => "Skills",
            Page::McpServers => "MCP servers",
            Page::Rules => "Rules",
            Page::Retro => "Retro",
            Page::Experiments => "Experiments",
            Page::DecisionModel => "Decision model",
            Page::Usage => "Usage",
            Page::Start => "Get started",
            Page::Models => "Models",
            Page::Plugins => "Plugins",
            Page::Terminal => "Terminal",
            Page::Settings => "Settings",
        }
    }

    /// The sidebar group of the page, shown as the breadcrumb of its header.
    pub fn group(self) -> &'static str {
        match self {
            Page::Inbox | Page::Chat | Page::Fleet => "",
            Page::Board(_) | Page::AddRepository => "Projects",
            Page::Agents | Page::Skills | Page::McpServers | Page::Rules => "Build",
            Page::Retro | Page::Experiments | Page::DecisionModel | Page::Usage => "Improve",
            Page::Start | Page::Models | Page::Plugins | Page::Terminal | Page::Settings => "Setup",
        }
    }
}

/// The connect screen's fields.
#[derive(Debug, Clone, Default)]
pub struct Form {
    pub address: String,
    pub token: String,
    pub error: Option<String>,
    pub connecting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Connect,
    Main,
}

#[derive(Debug, Clone)]
pub enum Message {
    AddressChanged(String),
    TokenChanged(String),
    Submit,
    /// The settings were tried against the server.
    Checked(Result<(Settings, Snapshot), ClientError>),
    /// The snapshot was read again, the `u64`-th time asked.
    Loaded(u64, Result<Snapshot, ClientError>),
    Stream(StreamEvent),
    /// A page of the sidebar was picked: it opens, and the way back starts again from it.
    Select(Page),
    /// A link on a page to another page: it opens, and "Back" returns to the page left.
    Visit(Page),
    /// "← Back to …", Alt+Left or the mouse's back button: the place left last opens again as it was.
    Back,
    /// A card of the board was clicked: its latest run is shown, or its history when it has no run.
    OpenCard(i64),
    /// Back from a card's page to the board.
    CloseCard,
    /// The pencil of a column of the board with this repository id: the editor of the column's script opens.
    EditScript(i64, String),
    Script(script_editor::Message),
    /// Something on the Files or Editor tab of the project of the repository with this id.
    Project(i64, project::Message),
    /// Ctrl+S: the Editor saves the file it shows.
    SaveFile,
    Live(live::Message),
    Inbox(inbox::Message),
    Chat(chat::Message),
    Fleet(fleet::Message),
    Agents(agents::Message),
    Skills(skills::Message),
    Mcp(mcp_servers::Message),
    Usage(usage::Message),
    Retro(retro::Message),
    Rules(rules::Message),
    Experiments(experiments::Message),
    DecisionModel(decision_model::Message),
    Setup(setup::Message),
    Terminal(server_terminal::Message),
    Start(start::Message),
    AddRepository(add_repository::Message),
    NewIssue(new_issue::Message),
    /// Something on an editor in place of a card or a repository.
    Edit(edits::Message),
    /// Something on the floating chat.
    Float(float_chat::Message),
    /// A chat was asked for, as from the inbox: its screen opens on it.
    OpenChat(i64),
    /// The history of the card was read.
    History(i64, Result<Vec<HistoryEntry>, ClientError>),
    Search(search::Message),
    ToggleSidebar,
    ChangeConnection,
}

/// Where the person is, enough to show it again as it was: the page and what is open on it.
#[derive(Debug, Clone, PartialEq)]
struct Location {
    page: Page,
    place: Place,
}

/// What is open on a page.
#[derive(Debug, Clone, PartialEq)]
enum Place {
    /// The page as it opens.
    Whole,
    /// A tab of a project, with no card open.
    Tab(Tab),
    /// A card of a project's board, its run or its history.
    Card(i64),
    /// The editor of the script of the column with this name.
    Script(String),
    /// The chat with this id on the Chat page.
    Chat(i64),
    /// The editor of the agent with this name.
    Agent(String),
}

/// What a click on a card opened instead of the board.
enum Opened {
    Run(Box<Live>),
    /// A card without a run: its history, once read, and the time it was read.
    History {
        card: i64,
        entries: Option<Vec<HistoryEntry>>,
        now: i64,
    },
    /// The editor of a column's script.
    Script(Box<ScriptEditor>),
}

pub struct App {
    settings_path: Option<PathBuf>,
    /// The settings in use, as saved.
    settings: Option<Settings>,
    screen: Screen,
    form: Form,
    connection: Connection,
    snapshot: Snapshot,
    /// The page the person chose; the first board, or the Inbox, until then.
    page: Option<Page>,
    /// The card's page shown instead of the board.
    opened: Option<Opened>,
    inbox: Inbox,
    chats: Chats,
    fleet: Fleet,
    agents: Agents,
    skills: Skills,
    mcp_servers: McpServers,
    usage: Usage,
    retros: Retros,
    rules: Rules,
    experiments: Experiments,
    decision_model: DecisionModel,
    setup: Setup,
    terminal: ServerTerminal,
    start: Start,
    /// Set when the connection was made, until the server has said whether the first start is done: the app then
    /// opens on Get started if the person has not chosen a page by then.
    open_on_start: bool,
    add_repository: AddRepository,
    new_issue: NewIssue,
    /// The editor in place of a card's title and body, or of a repository's trusted logins.
    edits: Edits,
    /// The tabs of each project that has been opened past its board, by repository id.
    projects: BTreeMap<i64, Project>,
    /// The chat with the Moebius agent over every page.
    float: FloatChat,
    collapsed: bool,
    search: Search,
    /// The places left by a click on a page, the last left last: "Back" returns to it. A pick of the sidebar empties it.
    history: Vec<Location>,
    /// Counts the reads of the snapshot, so that an older answer never replaces a newer one.
    requested: u64,
    shown: u64,
}

/// How many characters of a text being edited the floating chat tells the agent, at most.
const EXCERPT: usize = 2_500;

/// `text` under `label`, cut to [`EXCERPT`] characters, saying so when it was cut.
fn excerpt(label: &str, text: &str) -> String {
    let count = text.chars().count();
    if count <= EXCERPT {
        return format!("{label}:\n{text}");
    }
    let start: String = text.chars().take(EXCERPT).collect();
    format!("{label} (the first {EXCERPT} of {count} characters):\n{start}")
}

/// The other ways back: Alt+Left where nothing else takes it (a text field moves by words with it), and the mouse's back
/// button.
pub fn back_key(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. })
            if modifiers.alt()
                && key == iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft)
                && status == iced::event::Status::Ignored =>
        {
            Some(Message::Back)
        }
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Back)) => {
            Some(Message::Back)
        }
        _ => None,
    }
}

async fn check(settings: Settings) -> Result<(Settings, Snapshot), ClientError> {
    let client = Client::new(settings.clone());
    client.health().await?;
    Ok((settings, client.snapshot().await?))
}

impl App {
    /// Starts with the saved settings, or on the connect screen when there are none.
    pub fn new() -> (Self, Task<Message>) {
        Self::start(settings::default_path())
    }

    pub fn start(settings_path: Option<PathBuf>) -> (Self, Task<Message>) {
        let mut app = App {
            settings_path,
            settings: None,
            screen: Screen::Connect,
            form: Form::default(),
            connection: Connection::Connecting,
            snapshot: Snapshot::default(),
            page: None,
            opened: None,
            inbox: Inbox::new(live::now()),
            chats: Chats::new(live::now()),
            fleet: Fleet::new(live::now()),
            agents: Agents::default(),
            skills: Skills::default(),
            mcp_servers: McpServers::default(),
            usage: Usage::default(),
            retros: Retros::default(),
            rules: Rules::default(),
            experiments: Experiments::default(),
            decision_model: DecisionModel::default(),
            setup: Setup::default(),
            terminal: ServerTerminal::default(),
            start: Start::default(),
            open_on_start: false,
            add_repository: AddRepository::default(),
            new_issue: NewIssue::default(),
            edits: Edits::default(),
            projects: BTreeMap::new(),
            float: FloatChat::new(live::now()),
            collapsed: false,
            search: Search::default(),
            history: Vec::new(),
            requested: 0,
            shown: 0,
        };
        let loaded = match &app.settings_path {
            Some(path) => settings::load(path),
            None => Ok(None),
        };
        match loaded {
            Ok(Some(saved)) => {
                app.form.address = saved.address.clone();
                app.settings = Some(saved.clone());
                app.screen = Screen::Main;
                (app, Task::perform(check(saved), Message::Checked))
            }
            Ok(None) => (app, Task::none()),
            Err(e) => {
                app.form.error = Some(e.to_string());
                (app, Task::none())
            }
        }
    }

    /// An app that shows `snapshot` as if connected to a server, for the render tests.
    pub fn with_snapshot(snapshot: Snapshot) -> Self {
        let (mut app, _) = Self::start(None);
        app.settings = Some(Settings {
            address: "http://127.0.0.1:7171".to_string(),
            token: "not-a-real-token".to_string(),
        });
        app.screen = Screen::Main;
        app.connection = Connection::Online;
        app.snapshot = snapshot;
        app.inbox.follow(&app.snapshot);
        app
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        // The steps are read when their page opens, and when any page opens while one is missing, so that the
        // sidebar drops the entry soon after the last step is done.
        let reads_start = matches!(&message, Message::Select(page) | Message::Visit(page) if *page == Page::Start || self.start.incomplete());
        let left = Self::navigates(&message).then(|| self.location());
        let task = self.handle(message);
        if let Some(left) = left {
            self.arrive(left);
        }
        let task = match (&self.settings, reads_start) {
            (Some(settings), true) => Task::batch([task, self.start.reload(settings)]),
            _ => task,
        };
        // What the chat on the screen holds is read, however it got there. The Fleet shows the working runs as they
        // are now, and nothing once it is left.
        match (&self.screen, self.page(), &self.settings) {
            (Screen::Main, Page::Chat, Some(settings)) => {
                let read = self.chats.mark_read(&settings.clone());
                self.fleet.clear();
                Task::batch([task, read])
            }
            (Screen::Main, Page::Fleet, Some(settings)) => {
                let follow = self
                    .fleet
                    .follow(&self.snapshot, self.chats.list(), settings);
                Task::batch([task, follow])
            }
            _ => {
                self.fleet.clear();
                task
            }
        }
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddressChanged(address) => self.form.address = address,
            Message::TokenChanged(token) => self.form.token = token,
            Message::Submit => return self.submit(),
            Message::Checked(Ok((settings, snapshot))) => return self.adopt(settings, snapshot),
            Message::Checked(Err(e)) => self.show_connect_error(&e),
            Message::Loaded(count, Ok(snapshot)) => {
                if count > self.shown {
                    self.shown = count;
                    self.snapshot = snapshot;
                    self.inbox.follow(&self.snapshot);
                }
            }
            Message::Loaded(_, Err(ClientError::Unauthorized)) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Loaded(_, Err(_)) => self.connection = Connection::Offline,
            Message::Stream(event) => return self.on_stream(event),
            Message::Select(page) => {
                self.history.clear();
                return self.show_page(page);
            }
            Message::Visit(page) => return self.show_page(page),
            Message::Back => {
                let Some(location) = self.history.pop() else {
                    return Task::none();
                };
                return self.go_to(location);
            }
            Message::OpenCard(id) => {
                self.edits.close();
                return self.open(id);
            }
            Message::CloseCard => {
                self.edits.close();
                self.opened = None;
            }
            Message::Edit(message) if Edits::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Edit(message) => {
                let saved = Edits::saved(&message);
                let task = self.edits.update(message, self.settings.as_ref());
                if saved {
                    return Task::batch([task, self.reload()]);
                }
                return task;
            }
            Message::EditScript(repository, column) => {
                let (Some(settings), Some(board)) = (
                    &self.settings,
                    self.snapshot
                        .boards
                        .iter()
                        .find(|board| board.repository.id == repository),
                ) else {
                    return Task::none();
                };
                let (editor, task) =
                    ScriptEditor::open(repository, &board.repository.name, &column, settings);
                self.opened = Some(Opened::Script(Box::new(editor)));
                self.projects
                    .entry(repository)
                    .or_default()
                    .show_tab(Tab::Board);
                return task;
            }
            Message::Project(_, message) if Project::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Project(repository, message) => {
                if matches!(message, project::Message::Tab(_)) {
                    // A tab is chosen over a card's page, which belongs to the board.
                    self.opened = None;
                }
                return self.projects.entry(repository).or_default().update(
                    repository,
                    message,
                    self.settings.as_ref(),
                );
            }
            Message::SaveFile => {
                if let (Page::Board(repository), None) = (self.page(), &self.opened)
                    && let Some(project) = self.projects.get_mut(&repository)
                    && project.tab() == Tab::Editor
                {
                    return project.update(
                        repository,
                        project::Message::Save,
                        self.settings.as_ref(),
                    );
                }
            }
            Message::Script(message) if ScriptEditor::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Script(script_editor::Message::AskAgent) => {
                // The floating chat opens with the start of a message about the script; nothing is sent.
                let Some(Opened::Script(editor)) = &self.opened else {
                    return Task::none();
                };
                return self.float.prefill(editor.about());
            }
            Message::Script(message) => {
                if let Some(Opened::Script(editor)) = &mut self.opened {
                    return editor.update(message, self.settings.as_ref());
                }
            }
            Message::Live(message) => {
                if let Some(Opened::Run(live)) = &mut self.opened {
                    return live.update(message, self.settings.as_ref());
                }
            }
            Message::Inbox(message) => return self.on_inbox(message),
            Message::Chat(chat::Message::Listed(Err(ClientError::Unauthorized))) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Agents(agents::Message::Asked(Ok(run))) => {
                // The Moebius agent writes the instructions in a chat, which the floating chat shows.
                let settings = self.settings.clone();
                let task = self
                    .agents
                    .update(agents::Message::Asked(Ok(run.clone())), settings.as_ref());
                return Task::batch([task, self.float_adopt(&run, None)]);
            }
            Message::Agents(message) if Agents::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Agents(message) => return self.agents.update(message, self.settings.as_ref()),
            Message::Skills(skills::Message::Import(import::Message::Asked(Ok(run)))) => {
                // The Moebius agent imports in a chat, which the floating chat shows.
                let (asked, settings) = (
                    import::Message::Asked(Ok(run.clone())),
                    self.settings.clone(),
                );
                let task = self
                    .skills
                    .update(skills::Message::Import(asked), settings.as_ref());
                return Task::batch([task, self.float_adopt(&run, None)]);
            }
            Message::Mcp(mcp_servers::Message::Import(import::Message::Asked(Ok(run)))) => {
                let (asked, settings) = (
                    import::Message::Asked(Ok(run.clone())),
                    self.settings.clone(),
                );
                let task = self
                    .mcp_servers
                    .update(mcp_servers::Message::Import(asked), settings.as_ref());
                return Task::batch([task, self.float_adopt(&run, None)]);
            }
            Message::Skills(message) if Skills::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Skills(message) => return self.skills.update(message, self.settings.as_ref()),
            Message::Mcp(message) if McpServers::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Mcp(message) => {
                return self.mcp_servers.update(message, self.settings.as_ref());
            }
            Message::Usage(message) if Usage::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Usage(message) => return self.usage.update(message, self.settings.as_ref()),
            Message::Retro(message) if Retros::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Retro(message) => return self.retros.update(message, self.settings.as_ref()),
            Message::Rules(rules::Message::Asked(Ok(run))) => {
                // The Moebius agent changes the rules in a chat, which the floating chat shows.
                let settings = self.settings.clone();
                let task = self
                    .rules
                    .update(rules::Message::Asked(Ok(run.clone())), settings.as_ref());
                return Task::batch([task, self.float_adopt(&run, None)]);
            }
            Message::Rules(message) if Rules::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Rules(message) => return self.rules.update(message, self.settings.as_ref()),
            Message::Experiments(message) if Experiments::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Experiments(message) => {
                return self
                    .experiments
                    .update(message, self.settings.as_ref(), &self.snapshot);
            }
            Message::DecisionModel(message) if DecisionModel::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::DecisionModel(message) => {
                return self.decision_model.update(message, self.settings.as_ref());
            }
            Message::Setup(message) if Setup::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Setup(message) => return self.setup.update(message, self.settings.as_ref()),
            Message::Terminal(message) => self.terminal.update(message),
            Message::Start(message) if Start::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Start(message) => {
                let first = matches!(message, start::Message::Loaded(_));
                let task = self.start.update(message, self.settings.as_ref());
                let opening = first && std::mem::take(&mut self.open_on_start);
                if opening && self.start.incomplete() && self.page.is_none() {
                    self.page = Some(Page::Start);
                }
                return task;
            }
            Message::AddRepository(message) if AddRepository::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::AddRepository(message) => {
                // The new board opens, and the snapshot is read again without waiting for the event.
                let added = match &message {
                    add_repository::Message::Answered(Ok(repository)) => Some(repository.id),
                    _ => None,
                };
                let task = self.add_repository.update(message, self.settings.as_ref());
                if let Some(id) = added {
                    self.page = Some(Page::Board(id));
                    self.opened = None;
                    let start = match &self.settings {
                        Some(settings) => self.start.reload(settings),
                        None => Task::none(),
                    };
                    return Task::batch([task, self.reload(), start]);
                }
                return task;
            }
            Message::NewIssue(message) if NewIssue::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::NewIssue(new_issue::Message::Asked(Ok(run))) => {
                // The agent writes the issue in a chat in the board's repository, which the floating chat shows.
                let task = self
                    .new_issue
                    .update(new_issue::Message::Asked(Ok(run.clone())), None);
                return Task::batch([task, self.float_adopt(&run, self.page_repository())]);
            }
            Message::NewIssue(message) => {
                let created = matches!(message, new_issue::Message::Answered(Ok(_)));
                let task = self.new_issue.update(message, self.settings.as_ref());
                if created {
                    return Task::batch([task, self.reload()]);
                }
                return task;
            }
            Message::Chat(message) => return self.chats.update(message, self.settings.as_ref()),
            Message::Float(message) => return self.on_float(message),
            Message::Fleet(message) => self.fleet.update(message),
            Message::OpenChat(id) => {
                self.page = Some(Page::Chat);
                self.opened = None;
                if let Some(settings) = &self.settings {
                    return self.chats.open(id, settings);
                }
            }
            Message::History(id, Ok(entries)) => {
                if let Some(Opened::History {
                    card,
                    entries: shown,
                    ..
                }) = &mut self.opened
                    && *card == id
                {
                    *shown = Some(entries);
                }
            }
            Message::History(_, Err(_)) => {}
            Message::Search(message) => return self.on_search(message),
            Message::ToggleSidebar => self.collapsed = !self.collapsed,
            Message::ChangeConnection => {
                self.form.error = None;
                self.form.token.clear();
                self.screen = Screen::Connect;
            }
        }
        Task::none()
    }

    /// Something on the search field or its list.
    fn on_search(&mut self, message: search::Message) -> Task<Message> {
        match message {
            search::Message::Edited(text) => {
                let opening = !self.search.is_open();
                self.search.text = text;
                self.search.selected = 0;
                self.search.notice = None;
                // The agents are listed only when their page is opened; the search needs them.
                if opening
                    && self.search.is_open()
                    && let Some(settings) = &self.settings
                {
                    return self.agents.reload(settings);
                }
            }
            search::Message::Move(direction) => {
                let count = self.search_entries().len();
                self.search.step(direction, count);
            }
            search::Message::Submit => {
                let selected = self.search.selected;
                return self.open_entry(selected);
            }
            search::Message::Pick(index) => return self.open_entry(index),
            // Escape closes the list, and otherwise goes back from an editor.
            search::Message::Close if self.search.is_open() => self.search.clear(),
            search::Message::Close => return self.escape(),
            search::Message::Focus => {
                self.collapsed = false;
                return iced::widget::operation::focus(search::FIELD);
            }
            search::Message::Started(_, Ok(run)) => {
                self.page = Some(Page::Chat);
                self.opened = None;
                return self
                    .chats
                    .update(chat::Message::Started(Ok(run)), self.settings.as_ref());
            }
            search::Message::Started(text, Err(ClientError::Unauthorized)) => {
                self.search.text = text;
                self.show_connect_error(&ClientError::Unauthorized);
            }
            search::Message::Started(text, Err(e)) => {
                self.search.text = text;
                self.search.notice = Some(live::say(&e));
            }
        }
        Task::none()
    }

    /// The entries the search matches now.
    fn search_entries(&self) -> Vec<search::Entry> {
        search::matches(
            &self.search.text,
            &self.snapshot,
            self.chats.list(),
            self.agents.list(),
        )
    }

    /// Opens the entry at `index` of the list, and clears the field.
    fn open_entry(&mut self, index: usize) -> Task<Message> {
        let Some(entry) = self.search_entries().into_iter().nth(index) else {
            return Task::none();
        };
        self.search.clear();
        match entry.target {
            Target::Card(id) => self.open(id),
            Target::Chat(id) => self.handle(Message::OpenChat(id)),
            Target::Agent(name) => Task::batch([
                self.handle(Message::Visit(Page::Agents)),
                self.handle(Message::Agents(agents::Message::Select(name))),
            ]),
            Target::Page(page) => self.handle(Message::Visit(page)),
            Target::Ask(text) => match &self.settings {
                Some(settings) => search::ask(settings, text),
                None => Task::none(),
            },
        }
    }

    /// Something on the inbox page. The snapshot is read again when a request succeeded, without waiting for the event.
    fn on_inbox(&mut self, message: inbox::Message) -> Task<Message> {
        match &message {
            inbox::Message::Done(_, Err(ClientError::Unauthorized)) => {
                self.show_connect_error(&ClientError::Unauthorized);
                return Task::none();
            }
            inbox::Message::Done(_, Ok(())) => {
                let task = self.inbox.update(message, self.settings.as_ref());
                return Task::batch([task, self.reload()]);
            }
            _ => {}
        }
        self.inbox.update(message, self.settings.as_ref())
    }

    /// Something on the floating chat. Sending adds where the person is, and the repository of the page when the
    /// message starts the chat.
    fn on_float(&mut self, message: float_chat::Message) -> Task<Message> {
        match message {
            float_chat::Message::Send => {
                let Some(settings) = self.settings.clone() else {
                    return Task::none();
                };
                let (context, repository) = (self.page_context(), self.page_repository());
                self.float.send(&settings, context, repository)
            }
            float_chat::Message::OpenInChat => {
                let Some(id) = self.float.chat() else {
                    return Task::none();
                };
                // The window gives way to the Chat page, which shows the same chat.
                let _ = self.float.update(float_chat::Message::Show(false), None);
                self.handle(Message::OpenChat(id))
            }
            float_chat::Message::Started(Err(ClientError::Unauthorized))
            | float_chat::Message::Sent(Err(ClientError::Unauthorized)) => {
                self.show_connect_error(&ClientError::Unauthorized);
                Task::none()
            }
            message => {
                // A chat that started is in the Chat page's list too.
                let started = matches!(message, float_chat::Message::Started(Ok(_)));
                let task = self.float.update(message, self.settings.as_ref());
                match &self.settings {
                    Some(settings) if started => Task::batch([task, self.chats.reload(settings)]),
                    _ => task,
                }
            }
        }
    }

    /// Opens the floating chat on `run`, a chat with the Moebius agent started by an "Ask the agent", in `repository`.
    fn float_adopt(&mut self, run: &client::Run, repository: Option<i64>) -> Task<Message> {
        let Some(settings) = self.settings.clone() else {
            return Task::none();
        };
        Task::batch([
            self.float.adopt(run, repository, &settings),
            self.chats.reload(&settings),
        ])
    }

    /// The repository of the page shown, if it is a board's: a chat the floating chat starts there works in it.
    fn page_repository(&self) -> Option<i64> {
        match self.page() {
            Page::Board(id) => Some(id),
            _ => None,
        }
    }

    /// Where the person is in the app, in a few lines for the Moebius agent: the page, the repository, what is open on
    /// it, and the text being edited, cut to a sensible size. The floating chat sends it with every message.
    pub fn page_context(&self) -> String {
        let page = self.page();
        // A project's page is named by its tab; a card's page belongs to the board.
        let project = match page {
            Page::Board(id) if self.opened.is_none() => self
                .projects
                .get(&id)
                .filter(|project| project.tab() != Tab::Board),
            _ => None,
        };
        let title = match (page, project) {
            (Page::Board(_), Some(project)) => project.tab().title(),
            _ => page.title(),
        };
        let mut lines = vec![match page.group() {
            "" => format!("Page: {title}"),
            group => format!("Page: {group} > {title}"),
        }];
        match page {
            Page::Board(id) => {
                let Some(board) = self.snapshot.boards.iter().find(|b| b.repository.id == id)
                else {
                    return lines.join("\n");
                };
                let repository = &board.repository;
                lines.push(format!(
                    "Repository: {} (id {})",
                    repository.name, repository.id
                ));
                if let Some(project) = project {
                    lines.extend(project.context());
                    return lines.join("\n");
                }
                let card_of = |id| board.cards.iter().find(|card| card.id == id);
                let describe = |card: &client::Card| {
                    format!(
                        "Open card: #{} {} (column {}, card id {})",
                        card.number, card.title, card.column, card.id
                    )
                };
                match &self.opened {
                    Some(Opened::Run(live)) => {
                        if let Some(card) = card_of(live.card) {
                            lines.push(describe(card));
                        }
                        let run = live.run();
                        lines.push(format!(
                            "Watching run {} of {}: {}",
                            run.id, run.agent, run.status
                        ));
                    }
                    Some(Opened::History { card, .. }) => {
                        if let Some(card) = card_of(*card) {
                            lines.push(describe(card));
                            lines
                                .push("The card has no run yet; its history is shown.".to_string());
                        }
                    }
                    Some(Opened::Script(editor)) => {
                        let path = match editor.path() {
                            "" => "not read yet",
                            path => path,
                        };
                        lines.push(format!(
                            "Editing the script of the {} column ({path})",
                            editor.column()
                        ));
                        lines.push(excerpt(
                            "The script as written in the editor",
                            &editor.request(),
                        ));
                    }
                    None => lines.push(format!(
                        "The board: {} cards, {} changes",
                        board.cards.len(),
                        board.changes.len()
                    )),
                }
            }
            Page::Chat => match self.chats.open_chat() {
                Some(chat) => lines.push(format!(
                    "Open chat {}: \"{}\" with {}, {}",
                    chat.id, chat.title, chat.agent, chat.status
                )),
                None => lines.push("No chat is open.".to_string()),
            },
            Page::Rules => lines.push(excerpt("The rules", &self.rules.context_text())),
            Page::Agents => match self.agents.editing() {
                Some(name) => lines.push(format!("Editing the agent {name}")),
                None => lines.push("No agent is open.".to_string()),
            },
            Page::Models => {
                lines.push("The upstreams, models and API keys the server uses.".to_string())
            }
            Page::Settings => lines
                .push("The settings of moebius.toml, edited while the server runs.".to_string()),
            Page::Inbox
            | Page::Fleet
            | Page::AddRepository
            | Page::Skills
            | Page::McpServers
            | Page::Retro
            | Page::Experiments
            | Page::DecisionModel
            | Page::Usage
            | Page::Start
            | Page::Plugins
            | Page::Terminal => {}
        }
        lines.join("\n")
    }

    /// Tries what was typed; an empty token keeps the saved one.
    fn submit(&mut self) -> Task<Message> {
        let address = settings::normalize_address(&self.form.address);
        let token = match (&self.form.token, &self.settings) {
            (typed, _) if !typed.is_empty() => typed.clone(),
            (_, Some(saved)) => saved.token.clone(),
            _ => String::new(),
        };
        if address == "http://" || token.is_empty() {
            self.form.error = Some("Enter the address and the token.".to_string());
            return Task::none();
        }
        self.form.error = None;
        self.form.connecting = true;
        Task::perform(check(Settings { address, token }), Message::Checked)
    }

    /// The server took the settings: save them if they are new and show the shell.
    fn adopt(&mut self, settings: Settings, snapshot: Snapshot) -> Task<Message> {
        if self.settings.as_ref() != Some(&settings) {
            let saved = match &self.settings_path {
                Some(path) => settings::save(path, &settings),
                None => Ok(()),
            };
            if let Err(e) = saved {
                self.form.connecting = false;
                self.form.error = Some(e.to_string());
                self.screen = Screen::Connect;
                return Task::none();
            }
        }
        self.form = Form {
            address: settings.address.clone(),
            ..Form::default()
        };
        self.settings = Some(settings);
        self.snapshot = snapshot;
        self.inbox.follow(&self.snapshot);
        self.screen = Screen::Main;
        // The server just answered. The event stream may already be open, and it does not say so again.
        self.connection = Connection::Online;
        self.open_on_start = true;
        match &self.settings {
            Some(settings) => self.start.reload(settings),
            None => Task::none(),
        }
    }

    fn show_connect_error(&mut self, error: &ClientError) {
        self.form.connecting = false;
        self.form.error = Some(match error {
            ClientError::Unauthorized => "The token is not accepted.".to_string(),
            ClientError::Unreachable(address) => format!("Cannot reach {address}."),
            other => other.to_string(),
        });
        self.screen = Screen::Connect;
    }

    fn on_stream(&mut self, event: StreamEvent) -> Task<Message> {
        match event {
            StreamEvent::Connected => {
                self.connection = Connection::Online;
                // Events are not replayed: the run shown reads what it missed.
                let tiles = match &self.settings {
                    Some(settings) => self.fleet.reload(settings),
                    None => Task::none(),
                };
                let (missed, chats) = match (&self.opened, &self.settings) {
                    (Some(Opened::Run(live)), Some(settings)) => (
                        live::load(settings.clone(), live.card, live.run_id(), live.last_seq()),
                        self.chats.reload(settings),
                    ),
                    (_, Some(settings)) => (Task::none(), self.chats.reload(settings)),
                    _ => (Task::none(), Task::none()),
                };
                let usage = match &self.settings {
                    Some(settings) => self.usage.reload(settings),
                    None => Task::none(),
                };
                // A server that restarted tells what it runs with now.
                let setup = match &self.settings {
                    Some(settings) if self.setup.restarting() => self.setup.reload(settings),
                    _ => Task::none(),
                };
                let start = match &self.settings {
                    Some(settings) if self.start.incomplete() => self.start.reload(settings),
                    _ => Task::none(),
                };
                let floating = match &self.settings {
                    Some(settings) => self.float.reload(settings),
                    None => Task::none(),
                };
                Task::batch([
                    self.reload(),
                    missed,
                    chats,
                    tiles,
                    usage,
                    setup,
                    start,
                    floating,
                ])
            }
            StreamEvent::Disconnected(_) => {
                self.connection = Connection::Offline;
                Task::none()
            }
            StreamEvent::Unauthorized => {
                self.show_connect_error(&ClientError::Unauthorized);
                Task::none()
            }
            StreamEvent::Event(event) if event.kind == "run_event" => {
                self.on_run_event(&event.data)
            }
            StreamEvent::Event(event) => {
                let mut tasks = vec![self.reload()];
                if matches!(event.kind.as_str(), "run_started" | "run_ended")
                    && let Some(settings) = &self.settings
                {
                    tasks.push(self.chats.reload(settings));
                }
                if matches!(
                    event.kind.as_str(),
                    "retro_done" | "trial_done" | "proposal_status"
                ) && let Some(settings) = &self.settings
                {
                    // The Retro on show, and the proposals the Agents screen offers, have changed.
                    tasks.push(self.retros.reload(settings));
                    tasks.push(self.agents.reload(settings));
                }
                if matches!(event.kind.as_str(), "takeover_proposed" | "takeover_status")
                    && let Some(settings) = &self.settings
                {
                    tasks.push(self.decision_model.reload(settings));
                }
                if matches!(event.kind.as_str(), "research_status" | "experiment_done")
                    && let Some(settings) = &self.settings
                {
                    tasks.push(self.experiments.reload(settings));
                }
                if event.kind == "replayed"
                    && let Some(settings) = &self.settings
                {
                    // The refusal on show has an outcome now.
                    tasks.push(self.chats.reload_judgements(settings));
                    if let Some(Opened::Run(live)) = &self.opened
                        && event.data["run"] == live.run_id()
                    {
                        tasks.push(live.reload_judgements(settings.clone()));
                    }
                }
                if event.kind == "file_changed"
                    && let (Some(settings), Some(repository), Some(path)) = (
                        &self.settings,
                        event.data["repository"].as_i64(),
                        event.data["path"].as_str(),
                    )
                    && let Some(project) = self.projects.get(&repository)
                    && project.follows(path)
                {
                    // A file open without changes of its own shows what was saved elsewhere, by the agent say.
                    tasks.push(Project::read(repository, path, settings));
                }
                if event.kind == "run_ended" {
                    // A run that ended has used tokens, which the meter of the sidebar shows.
                    if let Some(settings) = &self.settings {
                        tasks.push(self.usage.reload(settings));
                    }
                    let status = event.data["status"].as_str().unwrap_or_default();
                    if let Some(run) = event.data["run"].as_i64() {
                        self.chats.on_ended(run, status);
                        self.float.on_ended(run, status);
                    }
                    if let Some(Opened::Run(live)) = &mut self.opened
                        && event.data["run"] == live.run_id()
                    {
                        live.on_ended(status);
                    }
                }
                Task::batch(tasks)
            }
        }
    }

    /// A step of a run. The steps of a run come in great numbers and change nothing the shell shows, except in the
    /// live view of that run and in the chat screen.
    fn on_run_event(&mut self, data: &serde_json::Value) -> Task<Message> {
        let Ok(event) = serde_json::from_value::<RunEvent>(data.clone()) else {
            return Task::none();
        };
        if let Some(run) = data["run"].as_i64() {
            self.fleet.on_event(run, event.clone());
        }
        let chat_checked = data["run"]
            .as_i64()
            .is_some_and(|run| self.chats.on_event(run, &event));
        let float_checked = data["run"]
            .as_i64()
            .is_some_and(|run| self.float.on_event(run, &event));
        // A chat's answer can change the files its worktree changes, which "Offer changes" counts.
        let listed = match (&self.settings, data["run"].as_i64()) {
            (Some(settings), Some(run))
                if event.kind == "agent_settled" && self.chats.lists(run) =>
            {
                self.chats.reload_list(settings)
            }
            _ => Task::none(),
        };
        let live = self.on_live_event(data, event);
        let Some(settings) = &self.settings else {
            return Task::batch([live, listed]);
        };
        let mut tasks = vec![live, listed];
        if chat_checked {
            tasks.push(self.chats.reload_judgements(settings));
        }
        if float_checked {
            tasks.push(self.float.reload_judgements(settings));
        }
        Task::batch(tasks)
    }

    /// A `run_event` of the run on the live view, if that is the one on show. The task reads its judgements again
    /// when a check arrived.
    fn on_live_event(&mut self, data: &serde_json::Value, event: RunEvent) -> Task<Message> {
        let (Some(Opened::Run(live)), Some(settings)) = (&mut self.opened, &self.settings) else {
            return Task::none();
        };
        if data["run"] != live.run_id() {
            return Task::none();
        }
        if live.on_event(event) {
            live.reload_judgements(settings.clone())
        } else {
            Task::none()
        }
    }

    /// Opens a card's page: the live view of its latest run, or its history.
    fn open(&mut self, id: i64) -> Task<Message> {
        let Some(settings) = self.settings.clone() else {
            return Task::none();
        };
        let Some(card) = self
            .snapshot
            .boards
            .iter()
            .flat_map(|board| &board.cards)
            .find(|card| card.id == id)
        else {
            return Task::none();
        };
        // A card is opened on the page of its board, also when the click was in the inbox.
        self.page = Some(Page::Board(card.repository_id));
        self.projects
            .entry(card.repository_id)
            .or_default()
            .show_tab(Tab::Board);
        match card.run.clone() {
            Some(run) => {
                let task = live::load(settings, id, run.id, 0);
                self.opened = Some(Opened::Run(Box::new(Live::new(id, run))));
                task
            }
            None => {
                self.opened = Some(Opened::History {
                    card: id,
                    entries: None,
                    now: live::now(),
                });
                Task::perform(
                    async move { Client::new(settings).history(id).await },
                    move |entries| Message::History(id, entries),
                )
            }
        }
    }

    /// Opens `page` as the sidebar does, with what was last open on it.
    fn show_page(&mut self, page: Page) -> Task<Message> {
        self.page = Some(page);
        self.opened = None;
        self.edits.close();
        self.chats.leave();
        if page == Page::Terminal {
            self.terminal.open();
        }
        match (page, &self.settings) {
            (Page::Chat | Page::Fleet, Some(settings)) => self.chats.reload(settings),
            (Page::Agents, Some(settings)) => self.agents.reload(settings),
            (Page::Skills, Some(settings)) => self.skills.reload(settings),
            (Page::McpServers, Some(settings)) => self.mcp_servers.reload(settings),
            (Page::Usage, Some(settings)) => self.usage.reload(settings),
            (Page::Retro, Some(settings)) => self.retros.reload(settings),
            (Page::Rules, Some(settings)) => self.rules.reload(settings),
            (Page::Experiments, Some(settings)) => self.experiments.reload(settings),
            (Page::DecisionModel, Some(settings)) => self.decision_model.reload(settings),
            (Page::Models | Page::Plugins | Page::Settings, Some(settings)) => {
                self.setup.reload(settings)
            }
            _ => Task::none(),
        }
    }

    /// Whether `message` is a click on a page that may take the person to another place, which "Back" returns from.
    fn navigates(message: &Message) -> bool {
        matches!(
            message,
            Message::Visit(_)
                | Message::OpenCard(_)
                | Message::CloseCard
                | Message::EditScript(..)
                | Message::OpenChat(_)
                | Message::Float(float_chat::Message::OpenInChat)
                | Message::Search(
                    search::Message::Submit
                        | search::Message::Pick(_)
                        | search::Message::Started(_, Ok(_))
                )
        )
    }

    /// Where the person is now.
    fn location(&self) -> Location {
        let page = self.page();
        let place = match page {
            Page::Board(id) => match &self.opened {
                Some(Opened::Run(live)) => Place::Card(live.card),
                Some(Opened::History { card, .. }) => Place::Card(*card),
                Some(Opened::Script(editor)) => Place::Script(editor.column().to_string()),
                None => Place::Tab(self.projects.get(&id).map_or(Tab::Board, Project::tab)),
            },
            Page::Chat => self.chats.opened().map_or(Place::Whole, Place::Chat),
            Page::Agents => self
                .agents
                .editing()
                .map_or(Place::Whole, |name| Place::Agent(name.to_string())),
            _ => Place::Whole,
        };
        Location { page, place }
    }

    /// A click took the person from `left` to where they are now: `left` is kept to go back to, unless the click went
    /// back to the place kept last, which is then dropped, as "Back" would have done.
    fn arrive(&mut self, left: Location) {
        let now = self.location();
        if now == left {
            return;
        }
        if self.history.last() == Some(&now) {
            self.history.pop();
        } else {
            self.history.push(left);
        }
    }

    /// Shows `location` again: its page, the tab of its project, and the card, script, chat or agent that was open.
    fn go_to(&mut self, location: Location) -> Task<Message> {
        let page = self.show_page(location.page);
        let place = match (location.page, location.place) {
            (Page::Board(id), Place::Tab(tab)) => self.projects.entry(id).or_default().update(
                id,
                project::Message::Tab(tab),
                self.settings.as_ref(),
            ),
            (Page::Board(_), Place::Card(card)) => self.open(card),
            (Page::Board(id), Place::Script(column)) => {
                self.handle(Message::EditScript(id, column))
            }
            (Page::Chat, Place::Chat(chat)) => match &self.settings {
                Some(settings) => self.chats.open(chat, settings),
                None => Task::none(),
            },
            (Page::Agents, Place::Agent(name)) => self
                .agents
                .update(agents::Message::Select(name), self.settings.as_ref()),
            _ => Task::none(),
        };
        Task::batch([page, place])
    }

    /// What the link back to `location` calls it.
    fn location_title(&self, location: &Location) -> String {
        let repository = |id| {
            self.snapshot
                .boards
                .iter()
                .find(|board| board.repository.id == id)
                .map_or("the board", |board| board.repository.name.as_str())
        };
        match (&location.page, &location.place) {
            (Page::Board(id), Place::Tab(Tab::Board)) => repository(*id).to_string(),
            (Page::Board(id), Place::Tab(tab)) => format!("{} of {}", tab.title(), repository(*id)),
            (Page::Board(_), Place::Card(card)) => self
                .snapshot
                .boards
                .iter()
                .flat_map(|board| &board.cards)
                .find(|c| c.id == *card)
                .map_or_else(|| "the card".to_string(), |c| format!("#{}", c.number)),
            (Page::Board(_), Place::Script(column)) => format!("the {column} script"),
            (Page::Agents, Place::Agent(name)) => name.clone(),
            (page, _) => page.title().to_string(),
        }
    }

    /// Escape in an editor goes back, when the editor came from another place and holds nothing unsaved.
    fn escape(&mut self) -> Task<Message> {
        let unsaved = match (self.page(), &self.opened) {
            (Page::Board(_), Some(Opened::Script(editor))) => Some(editor.unsaved()),
            (Page::Board(id), None) => self
                .projects
                .get(&id)
                .filter(|project| project.tab() == Tab::Editor)
                .map(Project::unsaved),
            (Page::Rules, _) => Some(self.rules.unsaved()),
            (Page::Agents, _) => self.agents.unsaved(),
            _ => None,
        };
        match unsaved {
            Some(false) => self.handle(Message::Back),
            _ => Task::none(),
        }
    }

    /// Reads the snapshot again.
    fn reload(&mut self) -> Task<Message> {
        let Some(settings) = self.settings.clone() else {
            return Task::none();
        };
        self.requested += 1;
        let count = self.requested;
        Task::perform(
            async move { Client::new(settings).snapshot().await },
            move |result| Message::Loaded(count, result),
        )
    }

    pub fn subscription(&self) -> Subscription<Message> {
        match (&self.screen, &self.settings) {
            (Screen::Main, Some(settings)) => {
                let stream = Subscription::run_with(settings.clone(), |settings| {
                    client::events(settings.clone())
                })
                .map(Message::Stream);
                // The time a run started is told as how long ago, so the page needs the clock while it is open.
                let clock = match self.opened {
                    Some(Opened::Run(_)) => iced::time::every(Duration::from_secs(30))
                        .map(|_| Message::Live(live::Message::Tick(live::now()))),
                    _ => Subscription::none(),
                };
                let inbox = match self.page() {
                    Page::Inbox => iced::time::every(Duration::from_secs(30))
                        .map(|_| Message::Inbox(inbox::Message::Tick(live::now()))),
                    _ => Subscription::none(),
                };
                let fleet = match self.page() {
                    Page::Fleet => iced::time::every(Duration::from_secs(5))
                        .map(|_| Message::Fleet(fleet::Message::Tick(live::now()))),
                    _ => Subscription::none(),
                };
                let chats = match self.page() {
                    Page::Chat => iced::time::every(Duration::from_secs(30))
                        .map(|_| Message::Chat(chat::Message::Tick(live::now()))),
                    _ => Subscription::none(),
                };
                let shell = match &self.opened {
                    Some(Opened::Run(live)) => live.subscription(settings),
                    _ => Subscription::none(),
                };
                let terminal = match self.page() {
                    Page::Terminal => self.terminal.subscription(settings),
                    _ => Subscription::none(),
                };
                let chat_terminal = match self.page() {
                    Page::Chat => self.chats.terminal_subscription(settings),
                    _ => Subscription::none(),
                };
                let floating = if self.float.working() {
                    iced::time::every(Duration::from_secs(30))
                        .map(|_| Message::Float(float_chat::Message::Tick(live::now())))
                } else {
                    Subscription::none()
                };
                let keys = iced::event::listen_with(search::key);
                let save = iced::event::listen_with(project::key);
                let back = iced::event::listen_with(back_key);
                Subscription::batch([
                    stream,
                    clock,
                    inbox,
                    fleet,
                    chats,
                    shell,
                    terminal,
                    chat_terminal,
                    floating,
                    keys,
                    save,
                    back,
                ])
            }
            _ => Subscription::none(),
        }
    }

    pub fn theme(&self) -> iced::Theme {
        theme::theme()
    }

    /// The page shown: the one chosen if it still exists, else the first board, else the Inbox.
    pub fn page(&self) -> Page {
        let boards = &self.snapshot.boards;
        match self.page {
            Some(Page::Board(id)) if boards.iter().any(|b| b.repository.id == id) => {
                Page::Board(id)
            }
            Some(Page::Board(_)) | None => boards
                .first()
                .map_or(Page::Inbox, |b| Page::Board(b.repository.id)),
            Some(page) => page,
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::Connect => connect::view(&self.form, self.settings.as_ref()),
            Screen::Main => {
                let page = self.page();
                let shell = row![
                    sidebar::view(
                        &self.snapshot,
                        self.connection,
                        &page,
                        sidebar::Counts {
                            chats_unread: self.chats.unread(),
                            working: fleet::working(&self.snapshot, self.chats.list()).len(),
                            tokens_today: self.usage.today(),
                            setup_incomplete: self.start.incomplete(),
                        },
                        self.collapsed,
                        &self.search.text
                    ),
                    container(self.page_with_back(page))
                        .padding(theme::PAGE_PADDING)
                        .width(Length::Fill)
                        .height(Length::Fill),
                ];
                // The floating chat stands over the page, except on the Chat page, which holds every chat and whose
                // own Send is where the button would stand.
                let floating: Element<'_, Message> = if page == Page::Chat {
                    iced::widget::space().into()
                } else {
                    self.float.view(self.float_place())
                };
                if !self.search.is_open() {
                    return stack![shell, floating].into();
                }
                // The list stands under the field, over the page.
                let entries = self.search_entries();
                let list = container(opaque(self.search.view(&entries)))
                    .padding(Padding {
                        top: 100.0,
                        left: 12.0,
                        ..Padding::ZERO
                    })
                    .width(Length::Fill)
                    .height(Length::Fill);
                stack![shell, floating, list].into()
            }
        }
    }

    /// Where the floating chat's chat works, or where its next message would start one: the page's repository.
    fn float_place(&self) -> String {
        let repository = match self.float.chat() {
            Some(_) => self.float.repository(),
            None => self.page_repository(),
        };
        let name = self
            .snapshot
            .boards
            .iter()
            .find(|board| Some(board.repository.id) == repository)
            .map(|board| board.repository.name.as_str());
        match name {
            Some(name) => format!("Works in {name}"),
            None => "No repository".to_string(),
        }
    }

    /// The page, under the link back to the place left last when it was reached from there.
    fn page_with_back(&self, page: Page) -> Element<'_, Message> {
        match self.history.last() {
            Some(left) => column![
                page::back(self.location_title(left), Message::Back),
                self.page_view(page)
            ]
            .spacing(10)
            .into(),
            None => self.page_view(page),
        }
    }

    fn page_view(&self, page: Page) -> Element<'_, Message> {
        match page {
            Page::Board(id) => {
                let Some(board) = self.snapshot.boards.iter().find(|b| b.repository.id == id)
                else {
                    // A board that is gone (its repository was removed) leaves the inbox.
                    return self.inbox.view(&self.snapshot);
                };
                let card_of = |id| board.cards.iter().find(|card| card.id == id);
                match &self.opened {
                    Some(Opened::Run(live)) => card_of(live.card)
                        .map(|card| live.view(board, card, self.edits.card_view(card.id))),
                    Some(Opened::History { card, entries, now }) => card_of(*card).map(|card| {
                        history::view(
                            board,
                            card,
                            entries.as_deref(),
                            *now,
                            self.edits.card_view(card.id),
                        )
                    }),
                    Some(Opened::Script(editor)) => Some(editor.view()),
                    None => None,
                }
                .unwrap_or_else(|| {
                    let project = self.projects.get(&id);
                    let tabs = project::tabs(id, project);
                    match project {
                        Some(project) if project.tab() != Tab::Board => {
                            project.view(&board.repository, tabs)
                        }
                        _ => board::view(
                            board,
                            &self.snapshot.inbox,
                            &self.new_issue,
                            &self.edits,
                            tabs,
                        ),
                    }
                })
            }
            Page::AddRepository => self.add_repository.view(),
            Page::Inbox => self.inbox.view(&self.snapshot),
            Page::Chat => self.chats.view(&self.snapshot, &self.inbox),
            Page::Fleet => self.fleet.view(&self.snapshot, self.chats.list()),
            Page::Agents => self.agents.view(),
            Page::Skills => self.skills.view(),
            Page::McpServers => self.mcp_servers.view(),
            Page::Usage => self.usage.view(),
            Page::Retro => self.retros.view(),
            Page::Rules => self.rules.view(),
            Page::Experiments => self.experiments.view(&self.snapshot),
            Page::DecisionModel => self.decision_model.view(),
            Page::Start => self.start.view(),
            Page::Models => self.setup.models_page(),
            Page::Plugins => self.setup.plugins_page(),
            Page::Terminal => self.terminal.view(),
            Page::Settings => self.setup.settings_page(self.settings.as_ref()),
        }
    }
}
