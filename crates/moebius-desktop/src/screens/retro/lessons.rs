//! The lessons box of the Retro page: what the Retro learned from earlier cards (`GET /v1/lessons`), each with its kind,
//! the cards it comes from and how often a run was given it. A person edits a lesson in place (`PUT /v1/lessons/{id}`)
//! and deletes it (`DELETE /v1/lessons/{id}`, after asking); a retired lesson stays on the page with the reason, and can
//! only be deleted. The lessons are read again whenever the Retro on show is.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, space, text, text_editor, text_input};
use iced::{Alignment, Element, Length, Task};

use super::card_style;
use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Lesson};
use crate::screens::connect;
use crate::settings::Settings;
use crate::theme;
use crate::widgets::form;
use crate::widgets::inline_edit::{self, editor_style};
use crate::widgets::page::{self, Tone};

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Vec<Lesson>, ClientError>),
    /// The pencil of the lesson with this id.
    Edit(i64),
    Situation(String),
    What(text_editor::Action),
    Advice(text_editor::Action),
    Save,
    Cancel,
    Saved(Result<Lesson, ClientError>),
    /// "Delete" of this lesson, which asks first.
    AskDelete(i64),
    Keep,
    Delete(i64),
    Deleted(Result<(), ClientError>),
}

/// A lesson as it is being edited.
struct Draft {
    id: i64,
    situation: String,
    what: text_editor::Content,
    advice: text_editor::Content,
}

#[derive(Default)]
pub struct Lessons {
    list: Vec<Lesson>,
    /// The lesson being edited, at most one.
    draft: Option<Draft>,
    /// The lesson whose delete is being asked about.
    asking: Option<i64>,
    saving: bool,
    /// Why the last save or delete was refused.
    error: Option<String>,
}

/// The text of an editor without the line end the editor adds.
fn typed(content: &text_editor::Content) -> String {
    content.text().trim().to_string()
}

impl Lessons {
    /// Reads the lessons again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        Task::perform(async move { client.lessons().await }, |result| {
            AppMessage::Retro(super::Message::Lessons(Message::Listed(result)))
        })
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Listed(Err(ClientError::Unauthorized))
                | Message::Saved(Err(ClientError::Unauthorized))
                | Message::Deleted(Err(ClientError::Unauthorized))
        )
    }

    pub fn update(&mut self, message: Message, settings: &Settings) -> Task<AppMessage> {
        match message {
            Message::Listed(Ok(list)) => self.list = list,
            Message::Listed(Err(_)) => {}
            Message::Edit(id) => {
                self.asking = None;
                self.error = None;
                self.draft = self.list.iter().find(|l| l.id == id).map(|l| Draft {
                    id,
                    situation: l.situation.clone(),
                    what: text_editor::Content::with_text(&l.lesson),
                    advice: text_editor::Content::with_text(&l.advice),
                });
            }
            Message::Situation(text) => {
                if let Some(draft) = &mut self.draft {
                    draft.situation = text;
                }
            }
            Message::What(action) => {
                if let Some(draft) = &mut self.draft {
                    draft.what.perform(action);
                }
            }
            Message::Advice(action) => {
                if let Some(draft) = &mut self.draft {
                    draft.advice.perform(action);
                }
            }
            Message::Cancel => {
                self.draft = None;
                self.error = None;
            }
            Message::Save => {
                let Some(draft) = &self.draft else {
                    return Task::none();
                };
                self.saving = true;
                self.error = None;
                let client = Client::new(settings.clone());
                let (id, situation, what, advice) = (
                    draft.id,
                    draft.situation.trim().to_string(),
                    typed(&draft.what),
                    typed(&draft.advice),
                );
                return Task::perform(
                    async move { client.edit_lesson(id, (&situation, &what, &advice)).await },
                    |result| AppMessage::Retro(super::Message::Lessons(Message::Saved(result))),
                );
            }
            Message::Saved(Ok(saved)) => {
                self.saving = false;
                self.draft = None;
                if let Some(lesson) = self.list.iter_mut().find(|l| l.id == saved.id) {
                    *lesson = saved;
                }
            }
            Message::Saved(Err(e)) => {
                self.saving = false;
                self.error = Some(e.to_string());
            }
            Message::AskDelete(id) => {
                self.draft = None;
                self.error = None;
                self.asking = Some(id);
            }
            Message::Keep => self.asking = None,
            Message::Delete(id) => {
                self.asking = None;
                let client = Client::new(settings.clone());
                return Task::perform(async move { client.delete_lesson(id).await }, |result| {
                    AppMessage::Retro(super::Message::Lessons(Message::Deleted(result)))
                });
            }
            Message::Deleted(Ok(())) => return self.reload(settings),
            Message::Deleted(Err(e)) => self.error = Some(e.to_string()),
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        if self.list.is_empty() {
            return page::boxed(
                "Lessons",
                page::body(
                    "None yet. The Retro writes them from failed runs, rejected reviews and cards that merged at once.",
                ),
            );
        }
        let mut cards =
            column(self.list.iter().map(|lesson| self.lesson(lesson))).spacing(theme::SPACE_8);
        if let (Some(error), None) = (&self.error, &self.draft) {
            cards = cards.push(form::notice_line(error, true));
        }
        page::boxed("Lessons", cards)
    }

    fn lesson<'a>(&'a self, lesson: &'a Lesson) -> Element<'a, AppMessage> {
        let editing = self.draft.as_ref().filter(|d| d.id == lesson.id);
        let content: Element<_> = match editing {
            Some(draft) => self.editor(draft),
            None => self.reading(lesson),
        };
        container(content)
            .padding(theme::BOX_PADDING)
            .width(Length::Fill)
            .style(|_| card_style())
            .into()
    }

    fn reading<'a>(&'a self, lesson: &'a Lesson) -> Element<'a, AppMessage> {
        let retired = lesson.status == "retired";
        let (kind, tone) = match lesson.kind.as_str() {
            "success" => ("Worked", Tone::Teal),
            "rejection" => ("Rejected", Tone::Amber),
            _ => ("Failed", Tone::Red),
        };
        let mut head = row![
            text(lesson.situation.as_str())
                .size(theme::CARD_TITLE)
                .font(theme::manrope(Weight::Semibold))
                .wrapping(Wrapping::WordOrGlyph),
        ]
        .spacing(theme::SPACE_6)
        .align_y(Alignment::Center);
        if !retired {
            head = head.push(inline_edit::pencil(
                format!("lesson-{}", lesson.id),
                AppMessage::Retro(super::Message::Lessons(Message::Edit(lesson.id))),
            ));
        }
        head = head
            .push(space::horizontal())
            .push(page::tag(kind.to_string(), tone));
        if retired {
            head = head.push(page::tag("Retired".to_string(), Tone::Plain));
        }
        head = head.push(self.delete(lesson.id));
        let mut meta = String::new();
        if !lesson.cards.is_empty() {
            let cards: Vec<String> = lesson.cards.iter().map(i64::to_string).collect();
            meta = format!("Cards {} · ", cards.join(", "));
        }
        meta.push_str(&format!("given {} times", lesson.retrievals));
        if let Some(reason) = &lesson.retired_reason {
            meta.push_str(&format!(" · {reason}"));
        }
        column![
            head,
            text(lesson.lesson.as_str())
                .size(theme::BODY)
                .color(theme::TEXT_SECONDARY)
                .wrapping(Wrapping::WordOrGlyph),
            text(format!("Do: {}", lesson.advice))
                .size(theme::BODY)
                .wrapping(Wrapping::WordOrGlyph),
            text(meta)
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED)
                .wrapping(Wrapping::WordOrGlyph),
        ]
        .spacing(theme::SPACE_4)
        .into()
    }

    /// The Delete button of a lesson, or the question it asks first.
    fn delete(&self, id: i64) -> Element<'_, AppMessage> {
        let message = |m| AppMessage::Retro(super::Message::Lessons(m));
        if self.asking != Some(id) {
            return page::white_button("Delete", Some(message(Message::AskDelete(id))));
        }
        row![
            text("Delete this lesson?")
                .size(theme::CONTEXT)
                .color(theme::TEXT_SECONDARY),
            page::white_button("Keep", Some(message(Message::Keep))),
            page::white_button("Delete lesson", Some(message(Message::Delete(id)))),
        ]
        .spacing(theme::SPACE_8)
        .align_y(Alignment::Center)
        .into()
    }

    fn editor<'a>(&'a self, draft: &'a Draft) -> Element<'a, AppMessage> {
        let message = |m| AppMessage::Retro(super::Message::Lessons(m));
        let area = |content, on_action: fn(text_editor::Action) -> Message| {
            text_editor(content)
                .on_action(move |action| message(on_action(action)))
                .size(theme::BODY)
                .padding(theme::SPACE_10)
                .style(editor_style)
        };
        let complete = !draft.situation.trim().is_empty()
            && !typed(&draft.what).is_empty()
            && !typed(&draft.advice).is_empty();
        let fields = column![
            form::labelled(
                "Situation",
                text_input("The kind of task", &draft.situation)
                    .on_input(move |text| message(Message::Situation(text)))
                    .size(theme::BODY)
                    .padding(theme::SPACE_8)
                    .style(connect::input_style)
            ),
            form::labelled("What happened", area(&draft.what, Message::What)),
            form::labelled("What to do", area(&draft.advice, Message::Advice)),
        ]
        .spacing(theme::SPACE_8);
        inline_edit::editor(
            fields.into(),
            (complete && !self.saving).then(|| message(Message::Save)),
            message(Message::Cancel),
            self.error.as_deref(),
        )
    }
}
