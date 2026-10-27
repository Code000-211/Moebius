//! The board of a repository. Only its page header is here; the columns and cards come with M6 (issue #34).

use iced::widget::{row, text};
use iced::{Element, Length};

use crate::app::Message;
use crate::client::{Board, Overview};
use crate::page;
use crate::theme;

/// The milestone that brings the columns and cards.
const MILESTONE: &str = "M6";

fn count(n: usize, one: &str, many: &str) -> String {
    match n {
        0 => format!("no {many}"),
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}

/// The page of a board: the repository's name and the workflow line, with a note where the columns will be.
pub fn view<'a>(board: &'a Board, overview: &'a Overview) -> Element<'a, Message> {
    let repository = &board.repository;
    let tag = match &repository.github {
        Some(github) => format!("GitHub · {github}"),
        None => "No forge".to_string(),
    };
    let live = overview
        .live_runs
        .get(&repository.id)
        .copied()
        .unwrap_or_default();
    let line = row![
        text(format!(
            "{} working · {} · workflow from ",
            count(live, "run", "runs"),
            count(board.waiting.len(), "card waits", "cards wait"),
        ))
        .size(theme::CONTEXT)
        .color(theme::TEXT_SECONDARY),
        text(".moebius/")
            .font(theme::MONO)
            .size(theme::CODE)
            .color(theme::TEXT_SECONDARY),
        text(format!(" on {}", repository.default_branch))
            .size(theme::CONTEXT)
            .color(theme::TEXT_SECONDARY),
    ]
    .align_y(iced::Alignment::Center);
    iced::widget::column![
        page::header(
            "Boards",
            &repository.name,
            Some(tag),
            line.into(),
            text("").into()
        ),
        page::boxed(
            "Columns and cards",
            page::body(format!(
                "{}. The columns and cards of this board come with {}.",
                count(board.cards.len(), "card", "cards"),
                MILESTONE
            ))
        ),
    ]
    .spacing(theme::PAGE_GAP)
    .width(Length::Fill)
    .into()
}
