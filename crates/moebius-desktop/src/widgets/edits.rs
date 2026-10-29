//! The editors in place that belong to no screen of their own: a card's title and body, opened by the pencil next to
//! the title of the card's page (`PUT /v1/cards/{id}`, which changes the issue on the forge too), and the logins a
//! repository on a forge trusts, opened by the pencil in its board's header (`PUT /v1/repositories/{id}`). A save that went
//! through closes the editor and the snapshot is read again; a refusal stays in red under the editor.

use iced::widget::{column, text, text_editor, text_input};
use iced::{Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Card, Client, ClientError, Repository};
use crate::screens::live::say;
use crate::settings::Settings;
use crate::widgets::inline_edit::{self, editor_style};
use crate::{screens::connect, theme, widgets::page};

#[derive(Debug, Clone)]
pub enum Message {
    /// The pencil of a card's page: the card with this id, its title and its body.
    EditCard(i64, String, String),
    Title(String),
    Body(text_editor::Action),
    /// The pencil in the header of a repository on a forge: its id and the logins it trusts.
    EditTrusted(i64, Vec<String>),
    Trusted(String),
    Save,
    Cancel,
    /// The server answered saving the card.
    CardSaved(Result<Box<Card>, ClientError>),
    /// The server answered saving the logins.
    TrustedSaved(Result<Repository, ClientError>),
}

/// A card's title and body as they are being edited.
struct CardDraft {
    card: i64,
    title: String,
    body: text_editor::Content,
}

/// The logins of a repository as they are being edited, separated by commas.
struct TrustedDraft {
    repository: i64,
    logins: String,
}

/// The editor open, at most one at a time, with whether its save is on its way and why the last one was refused.
#[derive(Default)]
pub struct Edits {
    card: Option<CardDraft>,
    trusted: Option<TrustedDraft>,
    saving: bool,
    error: Option<String>,
}

/// The logins typed, separated by commas or spaces.
fn logins(typed: &str) -> Vec<String> {
    typed
        .split([',', ' ', '\n'])
        .map(str::trim)
        .filter(|login| !login.is_empty())
        .map(str::to_string)
        .collect()
}

impl Edits {
    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::CardSaved(Err(ClientError::Unauthorized))
                | Message::TrustedSaved(Err(ClientError::Unauthorized))
        )
    }

    /// Whether the message is a save that went through, after which the snapshot is read again.
    pub fn saved(message: &Message) -> bool {
        matches!(
            message,
            Message::CardSaved(Ok(_)) | Message::TrustedSaved(Ok(_))
        )
    }

    /// Whether the card with this id is being edited.
    pub fn edits_card(&self, card: i64) -> bool {
        self.card.as_ref().is_some_and(|draft| draft.card == card)
    }

    /// Whether the logins of the repository with this id are being edited.
    pub fn edits_trusted(&self, repository: i64) -> bool {
        self.trusted
            .as_ref()
            .is_some_and(|draft| draft.repository == repository)
    }

    /// Closes the editor, as when its page is left.
    pub fn close(&mut self) {
        *self = Edits::default();
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::EditCard(card, title, body) => {
                *self = Edits {
                    card: Some(CardDraft {
                        card,
                        title,
                        body: text_editor::Content::with_text(&body),
                    }),
                    ..Edits::default()
                };
            }
            Message::EditTrusted(repository, trusted) => {
                *self = Edits {
                    trusted: Some(TrustedDraft {
                        repository,
                        logins: trusted.join(", "),
                    }),
                    ..Edits::default()
                };
            }
            Message::Title(title) => {
                if let Some(draft) = &mut self.card {
                    draft.title = title;
                }
            }
            Message::Body(action) => {
                if let Some(draft) = &mut self.card {
                    draft.body.perform(action);
                }
            }
            Message::Trusted(typed) => {
                if let Some(draft) = &mut self.trusted {
                    draft.logins = typed;
                }
            }
            Message::Save => {
                let Some(settings) = settings else {
                    return Task::none();
                };
                if self.saving {
                    return Task::none();
                }
                let client = Client::new(settings.clone());
                if let Some(draft) = &self.card {
                    if draft.title.trim().is_empty() {
                        return Task::none();
                    }
                    let (card, title, body) = (draft.card, draft.title.clone(), self.body());
                    self.saving = true;
                    self.error = None;
                    return Task::perform(
                        async move { client.edit_card(card, &title, &body).await.map(Box::new) },
                        |result| AppMessage::Edit(Message::CardSaved(result)),
                    );
                }
                if let Some(draft) = &self.trusted {
                    let (repository, trusted) = (draft.repository, logins(&draft.logins));
                    self.saving = true;
                    self.error = None;
                    return Task::perform(
                        async move { client.edit_repository(repository, &trusted).await },
                        |result| AppMessage::Edit(Message::TrustedSaved(result)),
                    );
                }
            }
            Message::Cancel => self.close(),
            Message::CardSaved(Ok(_)) | Message::TrustedSaved(Ok(_)) => self.close(),
            Message::CardSaved(Err(e)) | Message::TrustedSaved(Err(e)) => {
                self.saving = false;
                self.error = Some(say(&e));
            }
        }
        Task::none()
    }

    /// The body as it is in the editor, without the final line break the editor adds.
    fn body(&self) -> String {
        self.card.as_ref().map_or(String::new(), |draft| {
            draft.body.text().trim_end_matches('\n').to_string()
        })
    }

    /// The editor of the card's title and body, where the card's page shows them; none while it is not open.
    pub fn card_view(&self, card: i64) -> Option<Element<'_, AppMessage>> {
        let draft = self.card.as_ref().filter(|draft| draft.card == card)?;
        let message = AppMessage::Edit;
        let fields = column![
            text_input("Title", &draft.title)
                .on_input(move |title| message(Message::Title(title)))
                .size(theme::BODY)
                .padding(theme::FIELD_PADDING)
                .style(connect::input_style),
            text_editor(&draft.body)
                .placeholder("What the issue asks for. \"Blocked by #12\" makes #12 block it.")
                .on_action(move |action| message(Message::Body(action)))
                .size(theme::BODY)
                .padding(theme::SPACE_10)
                .height(theme::EDITOR_HEIGHT)
                .style(editor_style),
        ]
        .spacing(theme::SPACE_10)
        .width(Length::Fill);
        let save =
            (!self.saving && !draft.title.trim().is_empty()).then_some(message(Message::Save));
        Some(page::boxed(
            "Edit the issue",
            inline_edit::editor(
                fields.into(),
                save,
                message(Message::Cancel),
                self.error.as_deref(),
            ),
        ))
    }

    /// The editor of the logins a repository trusts, under its board's header; none while it is not open.
    pub fn trusted_view(&self, repository: i64) -> Option<Element<'_, AppMessage>> {
        let draft = self
            .trusted
            .as_ref()
            .filter(|draft| draft.repository == repository)?;
        let message = AppMessage::Edit;
        let save = (!self.saving).then_some(message(Message::Save));
        let fields = column![
            text_input("leon, mona", &draft.logins)
                .on_input(move |typed| message(Message::Trusted(typed)))
                .on_submit_maybe(save.clone())
                .size(theme::BODY)
                .padding(theme::FIELD_PADDING)
                .style(connect::input_style),
            text("The issues of these logins on the forge move on without your approval.")
                .size(theme::CONTEXT)
                .font(theme::MONO)
                .color(theme::TEXT_SECONDARY),
        ]
        .spacing(theme::SPACE_8);
        Some(page::boxed(
            "Trusted authors",
            inline_edit::editor(
                fields.into(),
                save,
                message(Message::Cancel),
                self.error.as_deref(),
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logins_are_read_from_commas_and_spaces() {
        assert_eq!(
            logins(" leon,mona  dependabot[bot], "),
            ["leon", "mona", "dependabot[bot]"]
        );
        assert!(logins(" , ").is_empty());
    }
}
