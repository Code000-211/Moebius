//! The Projects page: every repository Moebius has, what it is on, the board it opens, and the way to remove it.
//!
//! Removing asks first, because it takes the repository's cards, changes and chats with it and deletes its clone. The
//! server refuses while one of the repository's runs is still going, and says so under the list.

use iced::widget::{column, row, space, text};
use iced::{Alignment, Element, Length, Task};

use crate::app::{Message as AppMessage, Page};
use crate::client::{Board, Client, ClientError, Repository};
use crate::settings::Settings;
use crate::theme;
use crate::widgets::form::notice_line;
use crate::widgets::{list_editor, page};

use super::live::say;

/// The state of the Projects page.
#[derive(Debug, Default)]
pub struct Repositories {
    /// The repository whose removal is being asked about.
    asking: Option<i64>,
    /// The repository being removed, so that it cannot be asked for twice.
    removing: Option<i64>,
    /// What the server refused, shown under the list.
    failed: Option<String>,
}

/// What a person does on the Projects page.
#[derive(Debug, Clone)]
pub enum Message {
    /// A repository was pressed: open its board.
    Open(i64),
    /// "Delete" was pressed: ask first.
    Ask(i64),
    /// "Keep" was pressed.
    Keep,
    /// The question was answered: remove the repository.
    Remove(i64),
    /// The server answered.
    Removed(i64, Result<Repository, ClientError>),
}

impl Repositories {
    /// Whether the server refused the token, so that the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(message, Message::Removed(_, Err(ClientError::Unauthorized)))
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Open(id) => return Task::done(AppMessage::Select(Page::Board(id))),
            Message::Ask(id) => self.asking = Some(id),
            Message::Keep => self.asking = None,
            Message::Remove(id) => {
                let Some(settings) = settings else {
                    return Task::none();
                };
                self.asking = None;
                self.removing = Some(id);
                self.failed = None;
                let client = Client::new(settings.clone());
                return Task::perform(
                    async move { client.remove_repository(id).await },
                    move |result| AppMessage::Repositories(Message::Removed(id, result)),
                );
            }
            Message::Removed(_, Ok(_)) => self.removing = None,
            Message::Removed(_, Err(error)) => {
                self.removing = None;
                self.failed = Some(say(&error));
            }
        }
        Task::none()
    }

    pub fn view<'a>(&'a self, boards: &'a [Board]) -> Element<'a, AppMessage> {
        let header = page::header(
            "",
            "Projects",
            None,
            page::context(
                "Every repository Moebius has; open one, or remove it with the work stored for it.",
            ),
            page::white_button(
                "Add a repository",
                Some(AppMessage::Select(Page::AddRepository)),
            ),
        );
        let mut list = column![].spacing(theme::SPACE_8).width(Length::Fill);
        if boards.is_empty() {
            list = list.push(page::body(
                "There is no repository yet. Add one to give Moebius a place to work.",
            ));
        }
        for board in boards {
            let repository = &board.repository;
            let tags: Vec<(&'static str, page::Tone)> = Vec::new();
            list = list.push(
                row![
                    list_editor::list_card(
                        &repository.name,
                        tags,
                        format!("{} · {}", repository.forge_tag(), cards(board.cards.len())),
                        false,
                        false,
                        Some(AppMessage::Repositories(Message::Open(repository.id))),
                    ),
                    self.actions(repository),
                ]
                .spacing(theme::SPACE_8)
                .align_y(Alignment::Center),
            );
        }
        let mut content = column![list].spacing(theme::PAGE_GAP).width(Length::Fill);
        if let Some(failed) = &self.failed {
            content = content.push(notice_line(failed, true));
        }
        column![header, content]
            .spacing(theme::PAGE_GAP)
            .width(Length::Fill)
            .into()
    }

    /// What stands to the right of a repository's row: Delete, the question it asks, or that it is going.
    fn actions<'a>(&'a self, repository: &'a Repository) -> Element<'a, AppMessage> {
        let id = repository.id;
        if self.removing == Some(id) {
            return page::context("Removing…");
        }
        if self.asking != Some(id) {
            return page::white_button("Delete", Some(AppMessage::Repositories(Message::Ask(id))));
        }
        row![
            text(format!(
                "Delete {} with its cards and chats?",
                repository.name
            ))
            .size(theme::CONTEXT)
            .color(theme::TEXT_MUTED),
            space::horizontal(),
            page::white_button("Keep", Some(AppMessage::Repositories(Message::Keep))),
            page::white_button(
                "Delete",
                Some(AppMessage::Repositories(Message::Remove(id))),
            ),
        ]
        .spacing(theme::SPACE_8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
    }
}

/// How many cards a repository has, told as a person would say it.
fn cards(count: usize) -> String {
    match count {
        1 => "1 card".to_string(),
        count => format!("{count} cards"),
    }
}
