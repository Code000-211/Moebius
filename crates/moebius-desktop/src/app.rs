//! The app's state, its messages, and how one becomes the other: the connection to the server, the page shown, and
//! the views of the shell. The state is plain data; `update` is the only place it changes.

use std::path::PathBuf;
use std::time::Duration;

use iced::widget::{container, row};
use iced::{Element, Length, Subscription, Task};

use crate::agents::{self, Agents};
use crate::chat::Chats;
use crate::client::{self, Client, ClientError, HistoryEntry, RunEvent, Snapshot, StreamEvent};
use crate::inbox::{self, Inbox};
use crate::live::{self, Live};
use crate::retro::{self, Retros};
use crate::settings::{self, Settings};
use crate::usage::{self, Usage};
use crate::{board, chat, connect, history, sidebar, soon, theme};

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
    /// The board of the repository with this id.
    Board(i64),
    Agents,
    Skills,
    McpServers,
    Rules,
    Retro,
    Experiments,
    DecisionModel,
    Usage,
    Models,
    Plugins,
    Settings,
}

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Inbox => "Inbox",
            Page::Chat => "Chat",
            Page::Board(_) => "Board",
            Page::Agents => "Agents",
            Page::Skills => "Skills",
            Page::McpServers => "MCP servers",
            Page::Rules => "Rules",
            Page::Retro => "Retro",
            Page::Experiments => "Experiments",
            Page::DecisionModel => "Decision model",
            Page::Usage => "Usage",
            Page::Models => "Models",
            Page::Plugins => "Plugins",
            Page::Settings => "Settings",
        }
    }

    /// The sidebar group of the page, shown as the breadcrumb of its header.
    pub fn group(self) -> &'static str {
        match self {
            Page::Inbox | Page::Chat => "",
            Page::Board(_) => "Boards",
            Page::Agents | Page::Skills | Page::McpServers | Page::Rules => "Build",
            Page::Retro | Page::Experiments | Page::DecisionModel | Page::Usage => "Improve",
            Page::Models | Page::Plugins | Page::Settings => "Setup",
        }
    }

    /// The milestone that builds the page's screen, from `docs/issues-m2-m9.md`; none when no issue names one.
    pub fn milestone(self) -> Option<&'static str> {
        match self {
            Page::Inbox | Page::Chat | Page::Board(_) => Some("M6"),
            Page::Experiments => Some("M8"),
            Page::DecisionModel => Some("M9"),
            Page::Agents
            | Page::Skills
            | Page::McpServers
            | Page::Rules
            | Page::Usage
            | Page::Retro
            | Page::Models
            | Page::Plugins
            | Page::Settings => None,
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
    Select(Page),
    /// A card of the board was clicked: its latest run is shown, or its history when it has no run.
    OpenCard(i64),
    /// Back from a card's page to the board.
    CloseCard,
    Live(live::Message),
    Inbox(inbox::Message),
    Chat(chat::Message),
    Agents(agents::Message),
    Usage(usage::Message),
    Retro(retro::Message),
    /// A chat was asked for, as from the inbox: its screen opens on it.
    OpenChat(i64),
    /// The history of the card was read.
    History(i64, Result<Vec<HistoryEntry>, ClientError>),
    ToggleSidebar,
    ChangeConnection,
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
    agents: Agents,
    usage: Usage,
    retros: Retros,
    collapsed: bool,
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
            agents: Agents::default(),
            usage: Usage::default(),
            retros: Retros::default(),
            collapsed: false,
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
        app
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
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
                }
            }
            Message::Loaded(_, Err(ClientError::Unauthorized)) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Loaded(_, Err(_)) => self.connection = Connection::Offline,
            Message::Stream(event) => return self.on_stream(event),
            Message::Select(page) => {
                self.page = Some(page);
                self.opened = None;
                match (page, &self.settings) {
                    (Page::Chat, Some(settings)) => return self.chats.reload(settings),
                    (Page::Agents | Page::Skills | Page::McpServers, Some(settings)) => {
                        return self.agents.reload(settings);
                    }
                    (Page::Usage, Some(settings)) => return self.usage.reload(settings),
                    (Page::Retro, Some(settings)) => return self.retros.reload(settings),
                    _ => {}
                }
            }
            Message::OpenCard(id) => return self.open(id),
            Message::CloseCard => self.opened = None,
            Message::Live(message) => {
                if let Some(Opened::Run(live)) = &mut self.opened {
                    return live.update(message, self.settings.as_ref());
                }
            }
            Message::Inbox(message) => return self.on_inbox(message),
            Message::Chat(chat::Message::Listed(Err(ClientError::Unauthorized))) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Agents(message) if Agents::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Agents(message) => return self.agents.update(message, self.settings.as_ref()),
            Message::Usage(message) if Usage::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Usage(message) => return self.usage.update(message, self.settings.as_ref()),
            Message::Retro(message) if Retros::refused(&message) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Retro(message) => return self.retros.update(message, self.settings.as_ref()),
            Message::Chat(message) => return self.chats.update(message, self.settings.as_ref()),
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
            Message::ToggleSidebar => self.collapsed = !self.collapsed,
            Message::ChangeConnection => {
                self.form.error = None;
                self.form.token.clear();
                self.screen = Screen::Connect;
            }
        }
        Task::none()
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
        self.screen = Screen::Main;
        self.connection = Connection::Connecting;
        Task::none()
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
                Task::batch([self.reload(), missed, chats, usage])
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
                if event.kind == "run_ended" {
                    // A run that ended has used tokens, which the meter of the sidebar shows.
                    if let Some(settings) = &self.settings {
                        tasks.push(self.usage.reload(settings));
                    }
                    let status = event.data["status"].as_str().unwrap_or_default();
                    if let Some(run) = event.data["run"].as_i64() {
                        self.chats.on_ended(run, status);
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
        let chat_checked = data["run"]
            .as_i64()
            .is_some_and(|run| self.chats.on_event(run, &event));
        let live = self.on_live_event(data, event);
        match &self.settings {
            Some(settings) if chat_checked => {
                Task::batch([self.chats.reload_judgements(settings), live])
            }
            _ => live,
        }
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
                let chats = match self.page() {
                    Page::Chat => iced::time::every(Duration::from_secs(30))
                        .map(|_| Message::Chat(chat::Message::Tick(live::now()))),
                    _ => Subscription::none(),
                };
                let shell = match &self.opened {
                    Some(Opened::Run(live)) => live.subscription(settings),
                    _ => Subscription::none(),
                };
                Subscription::batch([stream, clock, inbox, chats, shell])
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
                row![
                    sidebar::view(
                        &self.snapshot,
                        self.connection,
                        &page,
                        self.chats.waiting(),
                        self.usage.today(),
                        self.collapsed
                    ),
                    container(self.page_view(page))
                        .padding(theme::PAGE_PADDING)
                        .width(Length::Fill)
                        .height(Length::Fill),
                ]
                .into()
            }
        }
    }

    fn page_view(&self, page: Page) -> Element<'_, Message> {
        match page {
            Page::Board(id) => {
                let Some(board) = self.snapshot.boards.iter().find(|b| b.repository.id == id)
                else {
                    return soon::view(page);
                };
                let card_of = |id| board.cards.iter().find(|card| card.id == id);
                match &self.opened {
                    Some(Opened::Run(live)) => {
                        card_of(live.card).map(|card| live.view(board, card))
                    }
                    Some(Opened::History { card, entries, now }) => card_of(*card)
                        .map(|card| history::view(board, card, entries.as_deref(), *now)),
                    None => None,
                }
                .unwrap_or_else(|| board::view(board, &self.snapshot.inbox))
            }
            Page::Inbox => self.inbox.view(&self.snapshot),
            Page::Chat => self.chats.view(&self.snapshot, &self.inbox),
            Page::Agents => self.agents.view(),
            Page::Skills => self.agents.skills_page(),
            Page::McpServers => self.agents.mcp_page(),
            Page::Usage => self.usage.view(),
            Page::Retro => self.retros.view(),
            Page::Settings => connect::settings_page(self.settings.as_ref()),
            page => soon::view(page),
        }
    }
}
