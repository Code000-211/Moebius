//! What the app is told: the pages the sidebar opens, and every `Message`, most of which carry a screen's own.

use crate::client::{ClientError, HistoryEntry, Snapshot, StreamEvent};
use crate::screens::add_repository;
use crate::screens::agents;
use crate::screens::automations;
use crate::screens::chat;
use crate::screens::connect_ssh;
use crate::screens::decision_model;
use crate::screens::experiments;
use crate::screens::fleet;
use crate::screens::harness_setup;
use crate::screens::inbox;
use crate::screens::live;
use crate::screens::mcp_servers;
use crate::screens::ports;
use crate::screens::project;
use crate::screens::repositories;
use crate::screens::retro;
use crate::screens::rules;
use crate::screens::server_terminal;
use crate::screens::setup;
use crate::screens::skills;
use crate::screens::start;
use crate::screens::usage;
use crate::search;
use crate::settings::Settings;
use crate::ssh::askpass::Askpass;
use crate::ssh::remote;
use crate::ssh::tunnel::TunnelEvent;
use crate::widgets::edits;
use crate::widgets::float_chat;
use crate::widgets::new_issue;
use crate::widgets::script_editor;

/// A page the sidebar opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Inbox,
    Chat,
    /// Every run that is working, one tile each.
    Fleet,
    /// Chats that start on their own, on a schedule.
    Automations,
    /// The project of the repository with this id: its board, files and editor, in tabs.
    Board(i64),
    /// Every project: the overview that lists the repositories, opened by the "Projects" heading.
    Repositories,
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
    /// The ports of the server, forwarded to this computer.
    Ports,
    Settings,
}

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Inbox => "Inbox",
            Page::Chat => "Chat",
            Page::Fleet => "Fleet",
            Page::Automations => "Automations",
            Page::Board(_) => "Board",
            Page::Repositories => "Projects",
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
            Page::Ports => "Ports",
            Page::Settings => "Settings",
        }
    }

    /// The sidebar group of the page, shown as the breadcrumb of its header.
    pub fn group(self) -> &'static str {
        match self {
            Page::Inbox | Page::Chat | Page::Fleet | Page::Automations => "",
            Page::Board(_) | Page::Repositories | Page::AddRepository => "Projects",
            Page::Agents | Page::Skills | Page::McpServers | Page::Rules => "Build",
            Page::Retro | Page::Experiments | Page::DecisionModel | Page::Usage => "Improve",
            Page::Start
            | Page::Models
            | Page::Plugins
            | Page::Terminal
            | Page::Ports
            | Page::Settings => "Setup",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    AddressChanged(String),
    TokenChanged(String),
    Submit,
    /// A "Copy" button was pressed: its text goes to the clipboard.
    Copy(String),
    /// The settings were tried against the server.
    Checked(Result<(Settings, Snapshot), ClientError>),
    /// The server took the report of where this app runs, or did not; nothing depends on it.
    Reported(Result<(), ClientError>),
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
    /// Something on the tabs or the Files sidebar of the project of the repository with this id.
    Project(i64, project::Message),
    /// Ctrl+S: the editor saves the file it shows.
    SaveFile,
    /// A key for the terminal tab of the project page shown.
    ProjectKey(iced::keyboard::Event),
    /// A key of "Go to file" or "Search in files" in the project page shown.
    Find(project::find::Message),
    Live(live::Message),
    Inbox(inbox::Message),
    Chat(chat::Message),
    Fleet(fleet::Message),
    Automations(automations::Message),
    Agents(agents::Message),
    Skills(skills::Message),
    Mcp(mcp_servers::Message),
    Usage(usage::Message),
    Retro(retro::Message),
    Rules(rules::Message),
    Experiments(experiments::Message),
    DecisionModel(decision_model::Message),
    Setup(setup::Message),
    /// Something on the box that sets up a harness configuration repository.
    Harness(harness_setup::Message),
    Terminal(server_terminal::Message),
    /// Something on the Ports page or an offer to open a port.
    Ports(ports::Message),
    /// The ports of a run, chat or terminal are shown (or all of them, without a scope), as a page the way back returns from.
    OpenPorts(Option<ports::Scope>),
    Start(start::Message),
    /// Something on the Projects overview.
    Repositories(repositories::Message),
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
    /// Something on the connect screen's way over SSH.
    Ssh(connect_ssh::Message),
    /// The listener for the passwords of `ssh` of this host is ready, and the setup starts from this step.
    SshAskpass(String, remote::Start, Result<Askpass, String>),
    /// The setup of the host over SSH told something.
    SshSetup(remote::Event),
    /// The tunnel told something.
    Tunnel(TunnelEvent),
    /// The sidebar's list of servers opens or closes.
    ToggleServers,
    /// The server at this place of the remembered ones is connected to.
    SwitchServer(usize),
}
