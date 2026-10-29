//! Setting up a harness configuration repository from the app. A server without one keeps its agents and rules in
//! `moebius.toml` and has no skills or MCP servers; the Skills, MCP servers and Agents pages then show a box at their top
//! with "Set up", and Get started a step with the same button. Pressed, it sends `POST /v1/harness/setup`, and the app
//! asks the server to restart as the Settings page does; once the server is back, the pages are read again.

use iced::font::Weight;
use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, SavedSettings};
use crate::screens::live::say;
use crate::settings::Settings;
use crate::theme;
use crate::widgets::form::notice_line;
use crate::widgets::page;

/// What the box says, and why the buttons that need a repository are muted.
pub const NEEDED: &str = "Skills, MCP servers and imports need a harness configuration repository. Set it up here; your agents and rules move into it.";

/// The tooltip of a button that needs a repository.
pub const WHY_MUTED: &str = "Needs a harness configuration repository: set it up above.";

#[derive(Debug, Clone)]
pub enum Message {
    /// "Set up" was pressed.
    SetUp,
    /// The server answered the setup.
    Answered(Result<SavedSettings, ClientError>),
}

/// Where the setup is.
#[derive(Default)]
pub struct HarnessSetup {
    /// Asked, and not answered yet.
    asking: bool,
    /// Set up, and the server asked to restart; cleared once the pages are read again.
    restarting: bool,
    /// Why the last setup failed, if it did.
    error: Option<String>,
}

impl HarnessSetup {
    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(message, Message::Answered(Err(ClientError::Unauthorized)))
    }

    /// Whether the setup succeeded with this answer, after which the app restarts the server.
    pub fn succeeded(message: &Message) -> bool {
        matches!(message, Message::Answered(Ok(_)))
    }

    /// Whether the server was set up and asked to restart, and the pages have not been read again since.
    pub fn restarting(&self) -> bool {
        self.restarting
    }

    /// The pages were read again after the restart.
    pub fn restarted(&mut self) {
        self.restarting = false;
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::SetUp => {
                let Some(settings) = settings.filter(|_| !self.asking && !self.restarting) else {
                    return Task::none();
                };
                self.asking = true;
                self.error = None;
                let client = Client::new(settings.clone());
                return Task::perform(async move { client.set_up_harness().await }, |result| {
                    AppMessage::Harness(Message::Answered(result))
                });
            }
            Message::Answered(Ok(_)) => {
                self.asking = false;
                self.restarting = true;
            }
            Message::Answered(Err(e)) => {
                self.asking = false;
                self.error = Some(say(&e));
            }
        }
        Task::none()
    }

    /// The "Set up" button, muted while the setup and the restart after it are under way.
    pub fn button(&self) -> Element<'_, AppMessage> {
        let pressable = !self.asking && !self.restarting;
        let label = if self.asking {
            "Setting up…"
        } else if self.restarting {
            "Restarting…"
        } else {
            "Set up"
        };
        page::primary_button(
            label,
            pressable.then_some(AppMessage::Harness(Message::SetUp)),
        )
    }

    /// Why the last setup failed, if it did.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The box at the top of a page that needs a repository: what is missing and the "Set up" button.
    pub fn view(&self) -> Element<'_, AppMessage> {
        let mut words = column![
            text(NEEDED)
                .size(theme::BODY)
                .font(theme::manrope(Weight::Semibold))
        ]
        .spacing(theme::SPACE_4)
        .width(Length::Fill);
        if self.restarting {
            words = words.push(page::body(
                "Set up. The server restarts and the app reconnects by itself.",
            ));
        }
        if let Some(error) = &self.error {
            words = words.push(notice_line(error, true));
        }
        container(
            row![
                words,
                space::horizontal().width(theme::SPACE_12),
                self.button(),
            ]
            .align_y(Alignment::Center),
        )
        .padding(theme::BOX_PADDING)
        .width(Length::Fill)
        .style(|_| page::box_style())
        .into()
    }
}

/// `button` with a tooltip that says why it is muted: it needs a repository.
pub fn muted<'a>(button: Element<'a, AppMessage>) -> Element<'a, AppMessage> {
    iced::widget::tooltip(
        button,
        container(text(WHY_MUTED).size(theme::LABEL).font(theme::MONO))
            .padding(theme::SMALL_BUTTON_PADDING)
            .style(|_| page::box_style()),
        iced::widget::tooltip::Position::Bottom,
    )
    .into()
}
