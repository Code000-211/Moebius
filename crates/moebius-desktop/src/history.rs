//! The page of a card that has no run to watch: its timeline from `GET /v1/cards/{id}/history`, oldest first.

use iced::Element;
use iced::widget::{column, row, scrollable, space, text};
use iced::{Length, Padding};

use crate::app::Message;
use crate::client::{Board, Card, HistoryEntry};
use crate::{page, theme};

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

/// The page: the card's header and its timeline, or a line saying it is being read (`None`).
pub fn view<'a>(
    board: &'a Board,
    card: &'a Card,
    entries: Option<&'a [HistoryEntry]>,
    now: i64,
) -> Element<'a, Message> {
    let rows: Element<'a, Message> = match entries {
        None => page::body("Reading the history."),
        Some([]) => page::body("Nothing has happened to this card yet."),
        Some(entries) => scrollable(
            column(entries.iter().map(|entry| {
                row![
                    text(page::ago(now, entry.at))
                        .size(theme::LABEL)
                        .color(theme::TEXT_MUTED)
                        .width(90),
                    text(describe(entry))
                        .size(theme::BODY)
                        .color(theme::TEXT)
                        .width(Length::Fill),
                ]
                .into()
            }))
            .spacing(8)
            .padding(Padding::default().right(12)),
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
            None,
            page::context(format!("{} · no run yet", card.column)),
            space().into(),
        ),
        page::boxed("History", rows),
    ]
    .spacing(theme::PAGE_GAP)
    .width(Length::Fill)
    .into()
}
