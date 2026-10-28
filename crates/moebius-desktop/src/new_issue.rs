//! The form of a new issue, which opens above the columns of a board from its "New issue" button. Create sends
//! `POST /v1/repositories/{id}/issues`; the card appears the way every card does, through the event stream and the
//! snapshot. What the server refused is shown in the form.

use iced::widget::{column, container, row, text, text_editor, text_input};
use iced::{Border, Element, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError};
use crate::connect;
use crate::page;
use crate::settings::Settings;
use crate::theme;

#[derive(Debug, Clone)]
pub enum Message {
    /// "New issue" of the board of the repository was pressed.
    Open(i64),
    TitleEdited(String),
    BodyEdited(text_editor::Action),
    /// "Create" was pressed.
    Create,
    Cancel,
    /// The server answered.
    Answered(Result<(), ClientError>),
}

#[derive(Default)]
pub struct NewIssue {
    /// The repository whose board shows the form.
    open: Option<i64>,
    title: String,
    body: text_editor::Content,
    /// Why the server refused the issue.
    failed: Option<String>,
    /// Whether a request is on its way.
    creating: bool,
}

impl NewIssue {
    /// Whether the board of `repository` shows the form.
    pub fn is_open_on(&self, repository: i64) -> bool {
        self.open == Some(repository)
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(message, Message::Answered(Err(ClientError::Unauthorized)))
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Open(repository) => {
                if self.open != Some(repository) {
                    *self = NewIssue {
                        open: Some(repository),
                        ..NewIssue::default()
                    };
                }
            }
            Message::TitleEdited(title) => self.title = title,
            Message::BodyEdited(action) => self.body.perform(action),
            Message::Create => {
                let (Some(settings), Some(repository), false) =
                    (settings, self.open, self.creating)
                else {
                    return Task::none();
                };
                if self.title.trim().is_empty() {
                    return Task::none();
                }
                self.failed = None;
                self.creating = true;
                let client = Client::new(settings.clone());
                let (title, body) = (self.title.trim().to_string(), self.body.text());
                return Task::perform(
                    async move {
                        let made = client.create_issue(repository, &title, body.trim()).await;
                        made.map(|_| ())
                    },
                    |result| AppMessage::NewIssue(Message::Answered(result)),
                );
            }
            Message::Cancel => *self = NewIssue::default(),
            Message::Answered(Ok(_)) => *self = NewIssue::default(),
            Message::Answered(Err(e)) => {
                self.creating = false;
                self.failed = Some(e.to_string());
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        let message = AppMessage::NewIssue;
        let create =
            (!self.creating && !self.title.trim().is_empty()).then_some(message(Message::Create));
        let body = text_editor(&self.body)
            .placeholder("What should be done, in Markdown.")
            .on_action(move |action| message(Message::BodyEdited(action)))
            .size(theme::BODY)
            .padding(12)
            .height(120)
            .style(|_, status| text_editor::Style {
                background: theme::SURFACE.into(),
                border: Border {
                    color: match status {
                        text_editor::Status::Focused { .. } => theme::TEAL,
                        _ => theme::LINE,
                    },
                    width: 1.0,
                    radius: theme::RADIUS_CONTROL.into(),
                },
                placeholder: theme::TEXT_MUTED,
                value: theme::TEXT,
                selection: theme::TEAL_FILL,
            });
        let mut content = column![
            text_input("Title", &self.title)
                .on_input(move |title| message(Message::TitleEdited(title)))
                .on_submit_maybe(create.clone())
                .size(theme::BODY)
                .padding([9, 12])
                .style(connect::input_style),
            body,
            row![
                page::primary_button("Create", create),
                page::white_button("Cancel", Some(message(Message::Cancel))),
            ]
            .spacing(8),
        ]
        .spacing(10);
        if let Some(failed) = &self.failed {
            content = content.push(
                text(failed.as_str())
                    .size(theme::BODY)
                    .color(theme::RED_TEXT),
            );
        }
        container(page::boxed("Describe the issue", content)).into()
    }
}
