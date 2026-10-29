//! The page of a card that has no run to watch: the issue it is, with the pencil that edits it in place (`edits.rs`),
//! and its timeline from `GET /v1/cards/{id}/history`, oldest first.

use iced::Element;
use iced::widget::{column, row, scrollable, space, text};
use iced::{Length, Padding};

use crate::app::Message;
use crate::client::{Board, Card, Hint, HistoryEntry};
use crate::{theme, widgets::edits, widgets::inline_edit, widgets::page};

/// What a person reads of one entry.
fn describe(entry: &HistoryEntry) -> String {
    let data = &entry.data;
    let word = |key: &str| data[key].as_str().unwrap_or_default();
    match entry.kind.as_str() {
        "moved" => match data["note"].as_str() {
            Some(note) => format!("Moved from {} to {}: {note}", word("from"), word("to")),
            None => format!("Moved from {} to {}", word("from"), word("to")),
        },
        "run_started" => format!(
            "Run {} started: {}, {}",
            data["run"],
            word("agent"),
            word("purpose")
        ),
        "run_ended" => format!("Run {} {}", data["run"], word("status")),
        "decided" if word("kind") == "localize" => {
            match Hint::of("localize", word("choice"), data["outcome"].as_str()) {
                Some(hint) => format!(
                    "Hinted files: {}; {}",
                    hint.paths.join(", "),
                    hint.summary()
                ),
                None => "Hinted no files".to_string(),
            }
        }
        "decided" => format!(
            "{} decided {}: {}",
            word("decider"),
            word("choice"),
            word("reason")
        ),
        "inbox_added" => format!("Added to the inbox: {}", word("title")),
        "inbox_resolved" => format!("Resolved in the inbox: {}", word("title")),
        other => other.to_string(),
    }
}

/// The pencil next to a card's title, which opens the editor of its title and body.
pub fn card_pencil<'a>(card: &Card) -> Element<'a, Message> {
    inline_edit::pencil(
        "edit-card".to_string(),
        Message::Edit(edits::Message::EditCard(
            card.id,
            card.title.clone(),
            card.body.clone(),
        )),
    )
}

/// The page: the card's header, the issue (or its editor, `edit`, while it is open) and its timeline, or a line
/// saying it is being read (`None`).
pub fn view<'a>(
    board: &'a Board,
    card: &'a Card,
    entries: Option<&'a [HistoryEntry]>,
    now: i64,
    edit: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let rows: Element<'a, Message> = match entries {
        None => page::body("Reading the history…"),
        Some([]) => page::body("No history yet."),
        Some(entries) => scrollable(
            column(entries.iter().map(|entry| {
                row![
                    text(page::ago(now, entry.at))
                        .size(theme::LABEL)
                        .font(theme::MONO)
                        .color(theme::TEXT_MUTED)
                        .width(theme::TIME_COLUMN),
                    text(describe(entry))
                        .size(theme::BODY)
                        .color(theme::TEXT)
                        .width(Length::Fill),
                ]
                .into()
            }))
            .spacing(theme::SPACE_8)
            .padding(Padding::default().right(theme::SPACE_12)),
        )
        .height(Length::Fill)
        .into(),
    };
    column![
        page::header_with(
            Some(page::card_breadcrumb(
                &board.repository.name,
                card.number,
                Message::CloseCard
            )),
            &card.title,
            Some(card_pencil(card)),
            page::context(format!("{} · no run yet", card.column)),
            space().into(),
        ),
        edit.unwrap_or_else(|| {
            let body = match card.body.trim() {
                "" => "No description.",
                body => body,
            };
            page::boxed(
                "Issue",
                text(body).size(theme::BODY).color(theme::TEXT_SECONDARY),
            )
        }),
        page::boxed("History", rows),
    ]
    .spacing(theme::PAGE_GAP)
    .width(Length::Fill)
    .into()
}
