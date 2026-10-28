//! The "Add a repository" page the "+" of the sidebar opens: a name, where to clone from, the forge and the trusted
//! authors. Add sends `POST /v1/repositories`; the new board opens when the server took it, and what it refused is
//! shown under the form.

use iced::font::Weight;
use iced::widget::{column, row, space, text, text_input};
use iced::{Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, NewRepository, Repository};
use crate::connect;
use crate::page;
use crate::settings::Settings;
use crate::theme;

#[derive(Debug, Clone)]
pub enum Message {
    NameEdited(String),
    UrlEdited(String),
    /// The forge was chosen: `true` for GitHub.
    ForgeChosen(bool),
    GithubEdited(String),
    TrustedEdited(String),
    /// "Add" was pressed.
    Add,
    /// The server answered.
    Answered(Result<Repository, ClientError>),
}

#[derive(Default)]
pub struct AddRepository {
    name: String,
    url: String,
    github_forge: bool,
    github: String,
    trusted: String,
    /// Why the server refused the repository.
    failed: Option<String>,
    /// Whether a request is on its way.
    adding: bool,
}

impl AddRepository {
    /// What the form asks the server for, if it is filled in.
    pub fn request(&self) -> Option<NewRepository> {
        let (name, url, github) = (self.name.trim(), self.url.trim(), self.github.trim());
        if name.is_empty() || url.is_empty() || (self.github_forge && github.is_empty()) {
            return None;
        }
        Some(NewRepository {
            name: name.to_string(),
            url: url.to_string(),
            forge: if self.github_forge { "github" } else { "none" },
            github: self.github_forge.then(|| github.to_string()),
            trusted: if self.github_forge {
                self.trusted
                    .split(',')
                    .map(str::trim)
                    .filter(|login| !login.is_empty())
                    .map(str::to_string)
                    .collect()
            } else {
                Vec::new()
            },
        })
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(message, Message::Answered(Err(ClientError::Unauthorized)))
    }

    /// Empties the form for the next repository.
    pub fn clear(&mut self) {
        *self = AddRepository::default();
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::NameEdited(name) => self.name = name,
            Message::UrlEdited(url) => self.url = url,
            Message::ForgeChosen(github) => self.github_forge = github,
            Message::GithubEdited(github) => self.github = github,
            Message::TrustedEdited(trusted) => self.trusted = trusted,
            Message::Add => {
                let (Some(settings), Some(request), false) =
                    (settings, self.request(), self.adding)
                else {
                    return Task::none();
                };
                self.failed = None;
                self.adding = true;
                let client = Client::new(settings.clone());
                return Task::perform(
                    async move { client.add_repository(&request).await },
                    |result| AppMessage::AddRepository(Message::Answered(result)),
                );
            }
            Message::Answered(Ok(_)) => self.clear(),
            Message::Answered(Err(e)) => {
                self.adding = false;
                self.failed = Some(e.to_string());
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        let message = AppMessage::AddRepository;
        let label = |label: &'static str| {
            text(label)
                .size(theme::LABEL)
                .font(theme::manrope(Weight::Semibold))
                .color(theme::SIDEBAR_GROUP)
        };
        let field = |placeholder: &'static str, value: &str, edited: fn(String) -> Message| {
            text_input(placeholder, value)
                .on_input(move |value| message(edited(value)))
                .size(theme::BODY)
                .padding([9, 12])
                .style(connect::input_style)
        };
        let add = (!self.adding && self.request().is_some()).then_some(message(Message::Add));
        let mut content = column![
            label("Name"),
            field("radiance", &self.name, Message::NameEdited),
            label("URL or path"),
            field(
                "https://github.com/acme/radiance.git",
                &self.url,
                Message::UrlEdited
            ),
            page::body(
                "Anything git clone takes: a URL, or a path on the server. A path that does not exist yet, \
                 or an empty folder, becomes a new repository.",
            ),
            label("Forge"),
            row![
                page::chip(
                    "None".to_string(),
                    !self.github_forge,
                    false,
                    message(Message::ForgeChosen(false))
                ),
                page::chip(
                    "GitHub".to_string(),
                    self.github_forge,
                    false,
                    message(Message::ForgeChosen(true))
                ),
            ]
            .spacing(8),
        ]
        .spacing(10);
        if self.github_forge {
            content = content
                .push(label("GitHub repository"))
                .push(field("acme/radiance", &self.github, Message::GithubEdited))
                .push(label("Trusted authors"))
                .push(field("leon, mona", &self.trusted, Message::TrustedEdited))
                .push(page::body(
                    "GitHub logins, separated by commas. Their issues need no approval.",
                ));
        }
        content = content.push(row![page::primary_button("Add", add)]);
        if let Some(failed) = &self.failed {
            content = content.push(
                text(failed.as_str())
                    .size(theme::BODY)
                    .color(theme::RED_TEXT),
            );
        }
        column![
            page::header(
                "Boards",
                "Add a repository",
                None,
                page::context("The server clones it and opens its board."),
                space().into(),
            ),
            page::boxed("Repository", content),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }
}
