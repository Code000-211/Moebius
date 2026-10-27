//! The app's state, its messages, and how one becomes the other: the connection to the server, the page shown, and
//! the views of the shell. The state is plain data; `update` is the only place it changes.

use std::path::PathBuf;

use iced::widget::{container, row};
use iced::{Element, Length, Subscription, Task};

use crate::client::{self, Client, ClientError, Overview, StreamEvent};
use crate::settings::{self, Settings};
use crate::{board, connect, sidebar, soon, theme};

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
            Page::Inbox => "",
            Page::Board(_) => "Boards",
            Page::Agents | Page::Skills | Page::McpServers | Page::Rules => "Build",
            Page::Retro | Page::Experiments | Page::DecisionModel | Page::Usage => "Improve",
            Page::Models | Page::Plugins | Page::Settings => "Setup",
        }
    }

    /// The milestone that builds the page's screen, from `docs/issues-m2-m9.md`; none when no issue names one.
    pub fn milestone(self) -> Option<&'static str> {
        match self {
            Page::Inbox | Page::Board(_) => Some("M6"),
            Page::Agents | Page::Skills | Page::McpServers | Page::Usage => Some("M7"),
            Page::Retro | Page::Experiments => Some("M8"),
            Page::DecisionModel => Some("M9"),
            Page::Rules | Page::Models | Page::Plugins | Page::Settings => None,
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
    Checked(Result<(Settings, Overview), ClientError>),
    /// The overview was read again, the `u64`-th time asked.
    Loaded(u64, Result<Overview, ClientError>),
    Stream(StreamEvent),
    Select(Page),
    ToggleSidebar,
    ChangeConnection,
}

pub struct App {
    settings_path: Option<PathBuf>,
    /// The settings in use, as saved.
    settings: Option<Settings>,
    screen: Screen,
    form: Form,
    connection: Connection,
    overview: Overview,
    /// The page the person chose; the first board, or the Inbox, until then.
    page: Option<Page>,
    collapsed: bool,
    /// Counts the reads of the overview, so that an older answer never replaces a newer one.
    requested: u64,
    shown: u64,
}

async fn check(settings: Settings) -> Result<(Settings, Overview), ClientError> {
    let client = Client::new(settings.clone());
    client.health().await?;
    Ok((settings, client.overview().await?))
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
            overview: Overview::default(),
            page: None,
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

    /// An app that shows `overview` as if connected to a server, for the render tests.
    pub fn with_overview(overview: Overview) -> Self {
        let (mut app, _) = Self::start(None);
        app.settings = Some(Settings {
            address: "http://127.0.0.1:7171".to_string(),
            token: "not-a-real-token".to_string(),
        });
        app.screen = Screen::Main;
        app.connection = Connection::Online;
        app.overview = overview;
        app
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddressChanged(address) => self.form.address = address,
            Message::TokenChanged(token) => self.form.token = token,
            Message::Submit => return self.submit(),
            Message::Checked(Ok((settings, overview))) => return self.adopt(settings, overview),
            Message::Checked(Err(e)) => self.show_connect_error(&e),
            Message::Loaded(count, Ok(overview)) => {
                if count > self.shown {
                    self.shown = count;
                    self.overview = overview;
                }
            }
            Message::Loaded(_, Err(ClientError::Unauthorized)) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Loaded(_, Err(_)) => self.connection = Connection::Offline,
            Message::Stream(event) => return self.on_stream(event),
            Message::Select(page) => self.page = Some(page),
            Message::ToggleSidebar => self.collapsed = !self.collapsed,
            Message::ChangeConnection => {
                self.form.error = None;
                self.form.token.clear();
                self.screen = Screen::Connect;
            }
        }
        Task::none()
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
    fn adopt(&mut self, settings: Settings, overview: Overview) -> Task<Message> {
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
        self.overview = overview;
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
                self.reload()
            }
            StreamEvent::Disconnected(_) => {
                self.connection = Connection::Offline;
                Task::none()
            }
            StreamEvent::Unauthorized => {
                self.show_connect_error(&ClientError::Unauthorized);
                Task::none()
            }
            // The steps of a run come in great numbers and change nothing the shell shows.
            StreamEvent::Event(event) if event.kind == "run_event" => Task::none(),
            StreamEvent::Event(_) => self.reload(),
        }
    }

    /// Reads the overview again.
    fn reload(&mut self) -> Task<Message> {
        let Some(settings) = self.settings.clone() else {
            return Task::none();
        };
        self.requested += 1;
        let count = self.requested;
        Task::perform(
            async move { Client::new(settings).overview().await },
            move |result| Message::Loaded(count, result),
        )
    }

    pub fn subscription(&self) -> Subscription<Message> {
        match (&self.screen, &self.settings) {
            (Screen::Main, Some(settings)) => {
                Subscription::run_with(settings.clone(), |settings| {
                    client::events(settings.clone())
                })
                .map(Message::Stream)
            }
            _ => Subscription::none(),
        }
    }

    pub fn theme(&self) -> iced::Theme {
        theme::theme()
    }

    /// The page shown: the one chosen if it still exists, else the first board, else the Inbox.
    pub fn page(&self) -> Page {
        let boards = &self.overview.snapshot.boards;
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
                    sidebar::view(&self.overview, self.connection, &page, self.collapsed),
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
                match self
                    .overview
                    .snapshot
                    .boards
                    .iter()
                    .find(|b| b.repository.id == id)
                {
                    Some(board) => board::view(board, &self.overview),
                    None => soon::view(page),
                }
            }
            Page::Settings => connect::settings_page(self.settings.as_ref()),
            page => soon::view(page),
        }
    }
}
