//! The app's state, its messages, and how one becomes the other: the connection to the server, the page shown, and
//! the views of the shell. The state is plain data; `update` is the only place it changes.
//!
//! `message` has the pages and the messages, `update` how a message changes the state, `stream` what the server's
//! events change, `navigation` the way back, `context` what the floating chat tells the agent of the page, and `view`
//! how it is all drawn.

mod context;
mod message;
mod navigation;
mod stream;
mod update;
mod view;

use std::collections::BTreeMap;
use std::path::PathBuf;

use iced::Task;

use crate::client::{Client, ClientError, HistoryEntry, Snapshot};
use crate::screens::add_repository::AddRepository;
use crate::screens::agents::Agents;
use crate::screens::automations::Automations;
use crate::screens::chat::Chats;
use crate::screens::decision_model::DecisionModel;
use crate::screens::experiments::Experiments;
use crate::screens::fleet::Fleet;
use crate::screens::harness_setup::HarnessSetup;
use crate::screens::inbox::Inbox;
use crate::screens::live::{self, Live};
use crate::screens::mcp_servers::McpServers;
use crate::screens::project::Project;
use crate::screens::retro::Retros;
use crate::screens::rules::Rules;
use crate::screens::server_terminal::ServerTerminal;
use crate::screens::setup::Setup;
use crate::screens::skills::Skills;
use crate::screens::start::Start;
use crate::screens::usage::Usage;
use crate::search::Search;
use crate::settings::{self, Settings};
use crate::widgets::edits::Edits;
use crate::widgets::float_chat::FloatChat;
use crate::widgets::new_issue::NewIssue;
use crate::widgets::script_editor::ScriptEditor;

use navigation::Location;

pub use message::{Message, Page};
pub use navigation::back_key;

/// What the sidebar says about the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Connecting,
    Online,
    Offline,
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
    automations: Automations,
    agents: Agents,
    skills: Skills,
    mcp_servers: McpServers,
    usage: Usage,
    retros: Retros,
    rules: Rules,
    experiments: Experiments,
    decision_model: DecisionModel,
    setup: Setup,
    harness: HarnessSetup,
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
            automations: Automations::new(live::now()),
            agents: Agents::default(),
            skills: Skills::default(),
            mcp_servers: McpServers::default(),
            usage: Usage::default(),
            retros: Retros::default(),
            rules: Rules::default(),
            experiments: Experiments::default(),
            decision_model: DecisionModel::default(),
            setup: Setup::default(),
            harness: HarnessSetup::default(),
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
}
