//! The form of a new issue, which opens above the columns of a board from its "New issue" button. Create sends
//! `POST /v1/repositories/{id}/issues`; the card appears the way every card does, through the event stream and the
//! snapshot. What the server refused is shown in the form. Instead of writing it, the person can say in a few words
//! what the issue is about and let the Moebius agent write and create it: that starts a chat with the agent in the
//! repository, which the app then shows.

use iced::widget::{column, container, row, text, text_editor, text_input};
use iced::{Border, Element, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Run};
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
    /// "Write it" (false) or "Let the Moebius agent write it" (true) was chosen.
    ByAgent(bool),
    /// "Create" was pressed, or "Ask the agent" when the agent writes it.
    Create,
    Cancel,
    /// The server answered.
    Answered(Result<(), ClientError>),
    /// The server started the Moebius agent's chat that writes the issue, or said why not.
    Asked(Result<Run, ClientError>),
}

/// The agent that writes an issue for the person.
const AGENT: &str = "Moebius";

/// What the Moebius agent is asked, with the repository's id and what the person said.
fn request(repository: i64, wanted: &str) -> String {
    format!(
        "Write an issue for this repository and create it with \
         `\"$MOEBIUS\" issue create {repository} \"<title>\" --body \"<body>\"`. Read the code it is about first, so \
         that the issue is concrete: a short title, what should be done and why, and how to tell that it is done. \
         Then say which issue you created.\n\nWhat the person wants:\n\n{wanted}"
    )
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
    /// Whether the Moebius agent writes the issue from what is in `body`.
    by_agent: bool,
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
            Message::ByAgent(by_agent) => self.by_agent = by_agent,
            Message::Create if self.by_agent => {
                let (Some(settings), Some(repository), false) =
                    (settings, self.open, self.creating)
                else {
                    return Task::none();
                };
                let wanted = self.body.text().trim().to_string();
                if wanted.is_empty() {
                    return Task::none();
                }
                self.failed = None;
                self.creating = true;
                let client = Client::new(settings.clone());
                let message = request(repository, &wanted);
                return Task::perform(
                    async move {
                        client
                            .start_chat(AGENT, &message, Some(repository), false)
                            .await
                    },
                    |result| AppMessage::NewIssue(Message::Asked(result)),
                );
            }
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
            Message::Answered(Ok(_)) | Message::Asked(Ok(_)) => *self = NewIssue::default(),
            Message::Answered(Err(e)) | Message::Asked(Err(e)) => {
                self.creating = false;
                self.failed = Some(e.to_string());
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        let message = AppMessage::NewIssue;
        let ready = if self.by_agent {
            !self.body.text().trim().is_empty()
        } else {
            !self.title.trim().is_empty()
        };
        let create = (!self.creating && ready).then_some(message(Message::Create));
        let ways = row![
            page::chip(
                "Write it".to_string(),
                !self.by_agent,
                false,
                message(Message::ByAgent(false))
            ),
            page::chip(
                "Let the Moebius agent write it".to_string(),
                self.by_agent,
                false,
                message(Message::ByAgent(true))
            ),
        ]
        .spacing(8);
        let body = text_editor(&self.body)
            .placeholder(if self.by_agent {
                "What the issue is about, in a few words. The agent reads the code and writes it."
            } else {
                "What should be done, in Markdown."
            })
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
        let mut content = column![ways].spacing(10);
        if !self.by_agent {
            content = content.push(
                text_input("Title", &self.title)
                    .on_input(move |title| message(Message::TitleEdited(title)))
                    .on_submit_maybe(create.clone())
                    .size(theme::BODY)
                    .padding([9, 12])
                    .style(connect::input_style),
            );
        }
        content = content.push(body).push(
            row![
                page::primary_button(
                    if self.by_agent {
                        "Ask the agent"
                    } else {
                        "Create"
                    },
                    create
                ),
                page::white_button("Cancel", Some(message(Message::Cancel))),
            ]
            .spacing(8),
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_is_told_how_to_create_the_issue_in_the_repository() {
        let asked = request(3, "cache the model downloads");
        assert!(asked.contains(r#"`"$MOEBIUS" issue create 3 "<title>" --body "<body>"`"#));
        assert!(asked.ends_with("What the person wants:\n\ncache the model downloads"));
    }
}
