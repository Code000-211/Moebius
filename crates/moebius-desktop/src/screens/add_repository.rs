//! The "Add a repository" page the "+" of the sidebar opens: a name, where to clone from, the forge (none, GitHub,
//! GitLab or Forgejo) with the repository's name there, and the trusted authors. Add sends `POST /v1/repositories`; the new board opens when the server took it, and what it refused is
//! shown under the form.

use iced::font::Weight;
use iced::widget::{column, row, space, text, text_input};
use iced::{Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, NewRepository, Repository};
use crate::screens::connect;
use crate::settings::Settings;
use crate::theme;
use crate::widgets::form::notice_line;
use crate::widgets::page;

/// The forge a new repository is on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Forge {
    #[default]
    None,
    GitHub,
    GitLab,
    /// Codeberg, or any Forgejo or Gitea.
    Forgejo,
}

impl Forge {
    /// The name the desktop API takes.
    fn name(self) -> &'static str {
        match self {
            Forge::None => "none",
            Forge::GitHub => "github",
            Forge::GitLab => "gitlab",
            Forge::Forgejo => "forgejo",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    NameEdited(String),
    UrlEdited(String),
    ForgeChosen(Forge),
    GithubEdited(String),
    /// The project's path on GitLab, or the repository on Forgejo, was edited.
    ProjectEdited(String),
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
    forge: Forge,
    github: String,
    project: String,
    trusted: String,
    /// Why the server refused the repository.
    failed: Option<String>,
    /// Whether a request is on its way.
    adding: bool,
}

impl AddRepository {
    /// What the form asks the server for, if it is filled in.
    pub fn request(&self) -> Option<NewRepository> {
        let (name, url) = (self.name.trim(), self.url.trim());
        let (github, project) = (self.github.trim(), self.project.trim());
        let forge_name = match self.forge {
            Forge::None => "-",
            Forge::GitHub => github,
            Forge::GitLab | Forge::Forgejo => project,
        };
        if name.is_empty() || url.is_empty() || forge_name.is_empty() {
            return None;
        }
        Some(NewRepository {
            name: name.to_string(),
            url: url.to_string(),
            forge: self.forge.name(),
            github: (self.forge == Forge::GitHub).then(|| github.to_string()),
            project: matches!(self.forge, Forge::GitLab | Forge::Forgejo)
                .then(|| project.to_string()),
            trusted: if self.forge != Forge::None {
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
            Message::ForgeChosen(forge) => self.forge = forge,
            Message::GithubEdited(github) => self.github = github,
            Message::ProjectEdited(project) => self.project = project,
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
                .color(theme::TEXT_SECONDARY)
        };
        let field = |placeholder: &'static str, value: &str, edited: fn(String) -> Message| {
            text_input(placeholder, value)
                .on_input(move |value| message(edited(value)))
                .size(theme::BODY)
                .padding(theme::CONTROL_PADDING)
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
                "Anything git clone takes, or a path on the server. A new or empty path becomes a new repository.",
            ),
            label("Forge"),
            row([
                ("None", Forge::None),
                ("GitHub", Forge::GitHub),
                ("GitLab", Forge::GitLab),
                ("Forgejo", Forge::Forgejo)
            ]
            .map(|(label, forge)| page::chip(
                label.to_string(),
                self.forge == forge,
                false,
                message(Message::ForgeChosen(forge))
            )))
            .spacing(theme::SPACE_8),
        ]
        .spacing(theme::SPACE_12);
        match self.forge {
            Forge::None => {}
            Forge::GitHub => {
                content = content
                    .push(label("GitHub repository"))
                    .push(field("acme/radiance", &self.github, Message::GithubEdited))
                    .push(label("Trusted authors"))
                    .push(field("leon, mona", &self.trusted, Message::TrustedEdited))
                    .push(page::body(
                        "GitHub logins, comma separated. Their issues need no approval.",
                    ));
            }
            Forge::GitLab => {
                content = content
                    .push(label("GitLab project"))
                    .push(field(
                        "acme/platform/radiance",
                        &self.project,
                        Message::ProjectEdited,
                    ))
                    .push(page::body(
                        "Project path, groups first. The server uses the token of [gitlab] in its settings.",
                    ))
                    .push(label("Trusted authors"))
                    .push(field("leon, mona", &self.trusted, Message::TrustedEdited))
                    .push(page::body(
                        "GitLab usernames, comma separated. Their issues need no approval.",
                    ));
            }
            Forge::Forgejo => {
                content = content
                    .push(label("Forgejo repository"))
                    .push(field("acme/radiance", &self.project, Message::ProjectEdited))
                    .push(page::body(
                        "Owner and name, as on Codeberg. The server uses the token of [forgejo] in its settings.",
                    ))
                    .push(label("Trusted authors"))
                    .push(field("leon, mona", &self.trusted, Message::TrustedEdited))
                    .push(page::body(
                        "Forgejo logins, comma separated. Their issues need no approval.",
                    ));
            }
        }
        content = content.push(row![page::primary_button("Add", add)]);
        if let Some(failed) = &self.failed {
            content = content.push(notice_line(failed, true));
        }
        column![
            page::header(
                "Projects",
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
