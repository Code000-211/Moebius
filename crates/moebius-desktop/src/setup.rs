//! The Setup pages: Models, Plugins and Settings. They only show how the server is set up, as `GET /v1/models`,
//! `GET /v1/plugins` and `GET /v1/settings` tell it, none of which holds a secret; a key is shown as set or not. The
//! Settings page also shows where this app is connected and leads to the connect screen to change that.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length, Padding, Task};

use crate::app::Message as AppMessage;
use crate::board::tokens;
use crate::client::{Client, ClientError, Models, Plugins, ServerSettings};
use crate::page::{self, Tone};
use crate::settings::{self, Settings};
use crate::theme;

#[derive(Debug, Clone)]
pub enum Message {
    Models(Result<Models, ClientError>),
    Plugins(Result<Plugins, ClientError>),
    Server(Result<ServerSettings, ClientError>),
}

/// What the server told of its setup, as far as it has.
#[derive(Default)]
pub struct Setup {
    models: Option<Models>,
    plugins: Option<Plugins>,
    server: Option<ServerSettings>,
}

impl Setup {
    /// Reads the three answers again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let (models, plugins, server) = (
            Client::new(settings.clone()),
            Client::new(settings.clone()),
            Client::new(settings.clone()),
        );
        Task::batch([
            Task::perform(async move { models.models().await }, |result| {
                AppMessage::Setup(Message::Models(result))
            }),
            Task::perform(async move { plugins.plugins().await }, |result| {
                AppMessage::Setup(Message::Plugins(result))
            }),
            Task::perform(async move { server.server_settings().await }, |result| {
                AppMessage::Setup(Message::Server(result))
            }),
        ])
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Models(Err(ClientError::Unauthorized))
                | Message::Plugins(Err(ClientError::Unauthorized))
                | Message::Server(Err(ClientError::Unauthorized))
        )
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Models(Ok(models)) => self.models = Some(models),
            Message::Plugins(Ok(plugins)) => self.plugins = Some(plugins),
            Message::Server(Ok(server)) => self.server = Some(server),
            Message::Models(Err(_)) | Message::Plugins(Err(_)) | Message::Server(Err(_)) => {}
        }
    }

    pub fn models_page(&self) -> Element<'_, AppMessage> {
        let Some(models) = &self.models else {
            return opened("Models", "Reading the models of the server.", column![]);
        };
        let upstreams: Element<AppMessage> = if models.upstreams.is_empty() {
            page::body("No upstream is configured.")
        } else {
            table(
                &[("Protocol", 2), ("Base URL", 6), ("Key", 2)],
                models
                    .upstreams
                    .iter()
                    .map(|upstream| {
                        vec![
                            strong(&upstream.protocol),
                            mono(upstream.base_url.clone()),
                            key_tag(upstream.api_key_set),
                        ]
                    })
                    .collect(),
            )
        };
        let agents: Element<AppMessage> = if models.agents.is_empty() {
            page::body("The server has no agent.")
        } else {
            table(
                &[
                    ("Agent", 3),
                    ("Model", 4),
                    ("Protocol", 2),
                    ("Thinking", 2),
                    ("Budget", 2),
                ],
                models
                    .agents
                    .iter()
                    .map(|agent| {
                        let (model, protocol) = match &agent.error {
                            Some(_) => ("Cannot run".to_string(), String::new()),
                            None => (agent.model.clone(), agent.protocol.clone()),
                        };
                        vec![
                            strong(&agent.name),
                            plain(model),
                            plain(protocol),
                            plain(agent.thinking.clone().unwrap_or_else(|| "–".into())),
                            plain(agent.token_budget.map_or_else(|| "–".into(), tokens)),
                        ]
                    })
                    .collect(),
            )
        };
        let teacher: Element<AppMessage> = match &models.teacher {
            Some(teacher) => facts(vec![
                ("Model", plain(teacher.model.clone())),
                ("Protocol", plain(teacher.protocol.clone())),
                (
                    "Time limit",
                    plain(format!("{} s", teacher.timeout_seconds)),
                ),
            ]),
            None => page::body(
                "No teacher is configured; a call is refused while a rule in prose applies.",
            ),
        };
        let decision_model: Element<AppMessage> = match &models.decision_model {
            Some(model) => facts(vec![
                ("Model", plain(model.model.clone())),
                ("URL", mono(model.url.clone())),
                ("Time limit", plain(format!("{} s", model.timeout_seconds))),
                ("Key", key_tag(model.api_key_set)),
            ]),
            None => page::body("No decision model is configured."),
        };
        opened(
            "Models",
            &format!(
                "{} upstreams and {} agents; keys are never shown.",
                models.upstreams.len(),
                models.agents.len()
            ),
            column![
                page::boxed("Upstreams", upstreams),
                page::boxed("Agents", agents),
                row![
                    container(page::boxed("Teacher", teacher)).width(Length::FillPortion(1)),
                    container(page::boxed("Decision model", decision_model))
                        .width(Length::FillPortion(1)),
                ]
                .spacing(theme::PAGE_GAP),
            ]
            .spacing(theme::PAGE_GAP),
        )
    }

    pub fn plugins_page(&self) -> Element<'_, AppMessage> {
        let Some(plugins) = &self.plugins else {
            return opened("Plugins", "Reading the plugins of the server.", column![]);
        };
        let list: Element<AppMessage> = if plugins.plugins.is_empty() {
            page::body("No plugin is loaded.")
        } else {
            table(
                &[
                    ("Plugin", 3),
                    ("Version", 2),
                    ("Kinds", 2),
                    ("Time limit", 2),
                    ("Decides", 4),
                ],
                plugins
                    .plugins
                    .iter()
                    .map(|plugin| {
                        vec![
                            mono(plugin.name.clone()),
                            plain(plugin.version.clone()),
                            plain(plugin.kinds.join(", ")),
                            plain(format!("{} s", plugins.time_limit_seconds)),
                            if plugin.decides.is_empty() {
                                plain("No kind of decision".to_string())
                            } else {
                                mono(plugin.decides.join(", "))
                            },
                        ]
                    })
                    .collect(),
            )
        };
        opened(
            "Plugins",
            &format!(
                "{} plugins loaded; a call into one is refused after {} s.",
                plugins.plugins.len(),
                plugins.time_limit_seconds
            ),
            column![page::boxed("Loaded plugins", list)],
        )
    }

    /// The Settings page: what the server runs with, and where this app is connected, `saved` being the connection.
    pub fn settings_page(&self, saved: Option<&Settings>) -> Element<'_, AppMessage> {
        let (address, token) = match saved {
            Some(saved) => (saved.address.clone(), settings::mask(&saved.token)),
            None => ("none".to_string(), "none".to_string()),
        };
        let app = page::boxed(
            "This app",
            column![
                facts(vec![
                    ("Server address", mono(address)),
                    ("Token", mono(token))
                ]),
                page::white_button(
                    "Connect to another server",
                    Some(AppMessage::ChangeConnection)
                ),
            ]
            .spacing(14),
        );
        let server: Element<AppMessage> = match &self.server {
            Some(server) => page::boxed("Server", server_facts(server)),
            None => page::boxed("Server", page::body("Reading the settings of the server.")),
        };
        opened(
            "Settings",
            "What the server runs with, and how this app reaches it. Change the server in its moebius.toml.",
            column![server, app].spacing(theme::PAGE_GAP),
        )
    }
}

fn server_facts(server: &ServerSettings) -> Element<'_, AppMessage> {
    facts(vec![
        ("Data directory", mono(server.data_dir.clone())),
        ("Listens on", mono(server.listen.clone())),
        (
            "Run limit",
            plain(format!("{} at a time", server.run_limit)),
        ),
        (
            "Round limit",
            plain(format!("{} coding rounds a card", server.round_limit)),
        ),
        (
            "Day budget",
            plain(
                server
                    .day_budget
                    .map_or_else(|| "No limit".to_string(), tokens),
            ),
        ),
        (
            "Retro at",
            plain(format!("{}, the server's time", server.retro_at)),
        ),
        (
            "Trial window",
            plain(format!(
                "{} to {}",
                server.trial_window.start, server.trial_window.end
            )),
        ),
        ("Board tick", plain(format!("{} ms", server.board_tick_ms))),
        (
            "Harness repository",
            match &server.harness_url {
                Some(url) => mono(url.clone()),
                None => plain("None; agents are configured in moebius.toml".to_string()),
            },
        ),
        (
            "MCP variables",
            if server.mcp_env.is_empty() {
                plain("None".to_string())
            } else {
                mono(server.mcp_env.join(", "))
            },
        ),
        (
            "GitHub",
            match &server.github {
                Some(github) => plain(format!("App {}", github.app_id)),
                None => plain("Not configured".to_string()),
            },
        ),
    ])
}

/// A page of the Setup group: the header and `content` that scrolls.
fn opened<'a>(
    title: &'a str,
    context: &str,
    content: iced::widget::Column<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    column![
        page::header(
            "Setup",
            title,
            None,
            page::context(context.to_string()),
            text("").into(),
        ),
        scrollable(container(content).padding(Padding::default().right(12))).height(Length::Fill),
    ]
    .spacing(theme::PAGE_GAP)
    .width(Length::Fill)
    .into()
}

/// A table with a row of headings; each of `rows` has one element for each column, of the portion of the heading.
fn table<'a>(
    columns: &[(&'static str, u16)],
    rows: Vec<Vec<Element<'a, AppMessage>>>,
) -> Element<'a, AppMessage> {
    let heads = columns
        .iter()
        .fold(row![].spacing(10), |heads, (label, portion)| {
            heads.push(cell(
                *portion,
                text(*label)
                    .size(theme::LABEL)
                    .font(theme::manrope(Weight::Semibold))
                    .color(theme::TEXT_MUTED),
            ))
        });
    let mut table = column![heads, page::rule()].spacing(8);
    for cells in rows {
        let line = cells
            .into_iter()
            .zip(columns)
            .fold(row![].spacing(10), |line, (content, (_, portion))| {
                line.push(cell(*portion, content))
            });
        table = table.push(line).push(page::rule());
    }
    table.into()
}

/// Labelled values, one to a line.
fn facts<'a>(lines: Vec<(&'static str, Element<'a, AppMessage>)>) -> Element<'a, AppMessage> {
    let mut facts = column![].spacing(10);
    for (label, value) in lines {
        facts = facts.push(
            row![
                container(
                    text(label)
                        .size(theme::BODY)
                        .font(theme::manrope(Weight::Semibold))
                        .color(theme::TEXT_MUTED)
                )
                .width(Length::Fixed(170.0)),
                container(value).width(Length::Fill),
            ]
            .spacing(10),
        );
    }
    facts.into()
}

fn cell<'a>(portion: u16, content: impl Into<Element<'a, AppMessage>>) -> Element<'a, AppMessage> {
    container(content.into())
        .width(Length::FillPortion(portion))
        .into()
}

fn key_tag<'a>(set: bool) -> Element<'a, AppMessage> {
    if set {
        page::tag("key set".into(), Tone::Teal)
    } else {
        page::tag("no key".into(), Tone::Plain)
    }
}

fn strong<'a>(line: &'a str) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::BODY)
        .font(theme::manrope(Weight::Semibold))
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

fn plain<'a>(line: String) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::BODY)
        .color(theme::TEXT_SECONDARY)
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

fn mono<'a>(line: String) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::CODE_SMALL)
        .font(theme::MONO)
        .color(theme::TEXT)
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}
