//! What an inbox item says of itself: the card, change and board it is about, its kind as a tag and a sentence, why
//! it is there, and its facts, report and trials.

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Color, Element, Length};

use crate::app::Message as AppMessage;
use crate::client::{Board, Card, Change, InboxItem, Snapshot};
use crate::widgets::page::{self, Tone, tag};
use crate::{screens::board, theme};

use super::{
    APPROVE_ISSUE, CHANGE_CLOSED, GRILLING_ISSUES, GRILLING_REPORT, Message, PROPOSAL,
    READY_TO_MERGE, STUCK, TAKEOVER,
};

/// What an item is about, looked up in the snapshot: the board of its repository, its card and its change.
pub(super) struct Found<'a> {
    pub(super) board: Option<&'a Board>,
    pub(super) card: Option<&'a Card>,
    pub(super) change: Option<&'a Change>,
}

pub(super) fn found<'a>(snapshot: &'a Snapshot, item: &InboxItem) -> Found<'a> {
    let change = item.change();
    let board = snapshot.boards.iter().find(|board| {
        board.cards.iter().any(|card| Some(card.id) == item.card_id)
            || board.changes.iter().any(|c| Some(c.id) == change)
            || (item.grilling().is_some() && item.repository() == Some(board.repository.id))
    });
    Found {
        board,
        card: board.and_then(|b| b.cards.iter().find(|c| Some(c.id) == item.card_id)),
        change: board
            .and_then(|b| b.changes.iter().find(|c| Some(c.id) == change))
            .or_else(|| {
                snapshot
                    .harness_changes
                    .iter()
                    .find(|c| Some(c.id) == change)
            }),
    }
}

/// The name of a kind on its chip.
pub(super) fn chip_name(kind: &str) -> String {
    match kind {
        READY_TO_MERGE => "To merge".to_string(),
        APPROVE_ISSUE => "Approvals".to_string(),
        STUCK => "Stuck".to_string(),
        CHANGE_CLOSED => "Closed".to_string(),
        PROPOSAL => "Proposals".to_string(),
        TAKEOVER => "Takeovers".to_string(),
        GRILLING_ISSUES => "Grilled issues".to_string(),
        GRILLING_REPORT => "Reports".to_string(),
        other => sentence(other),
    }
}

/// A kind of a column script, such as `needs_design`, as a label: `Needs design`.
fn sentence(kind: &str) -> String {
    let mut words = kind.replace('_', " ");
    if let Some(first) = words.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    words
}

/// What the item is, and how loud: red for what is blocked, amber for what waits for the person's move.
pub(super) fn kind_tag(item: &InboxItem) -> (String, Tone) {
    match item.kind.as_str() {
        STUCK => ("Stuck".into(), Tone::Red),
        READY_TO_MERGE if item.card_id.is_none() => ("Change from a chat".into(), Tone::Amber),
        READY_TO_MERGE => ("Ready to merge".into(), Tone::Amber),
        APPROVE_ISSUE => ("Outside issue".into(), Tone::Amber),
        CHANGE_CLOSED => ("Closed without a merge".into(), Tone::Amber),
        PROPOSAL => ("Retro proposal".into(), Tone::Amber),
        TAKEOVER => ("Decision model".into(), Tone::Amber),
        GRILLING_ISSUES => ("Issues from a grilling".into(), Tone::Amber),
        GRILLING_REPORT => ("Report from a grilling".into(), Tone::Amber),
        other => (sentence(other), Tone::Plain),
    }
}

/// The sentence under the title: why the item is here.
pub(super) fn why(item: &InboxItem, found: &Found) -> Option<String> {
    let reason = item.data["reason"].as_str().filter(|r| !r.is_empty());
    match item.kind.as_str() {
        STUCK => Some(reason.map_or("The card is stuck.".into(), |r| format!("Stopped: {r}."))),
        APPROVE_ISSUE => Some(match reason {
            Some(reason) => format!("A person has to approve this issue: {reason}."),
            None => "A person has to approve this issue.".into(),
        }),
        CHANGE_CLOSED => {
            let number = item.data["pull_request"]
                .as_i64()
                .or(found.change.and_then(|c| c.pull_request));
            Some(match number {
                Some(number) => format!("Pull request #{number} was closed without a merge."),
                None => "The change's pull request was closed without a merge.".into(),
            })
        }
        READY_TO_MERGE => None,
        TAKEOVER => Some(format!(
            "It chose what was recorded on {:.0}% of {} judgements held back from training. Merging lets it decide this kind from now on.",
            item.data["agreement"].as_f64().unwrap_or_default() * 100.0,
            item.data["count"].as_i64().unwrap_or_default()
        )),
        PROPOSAL => Some("The Retro proposes this change to the harness configuration.".into()),
        GRILLING_ISSUES => Some(
            "The Grilling partner ended in these issues. Edit them, remove what you do not want, and confirm to make a card of each."
                .into(),
        ),
        GRILLING_REPORT => {
            Some("The Grilling partner ended in a research report. Read it, then dismiss it.".into())
        }
        _ => reason.map(str::to_string),
    }
}

/// The report of a grilling, as text on a field of its own.
pub(super) fn report(report: &str) -> Element<'_, AppMessage> {
    container(
        text(report)
            .size(theme::BODY)
            .color(theme::TEXT)
            .width(Length::Fill)
            .wrapping(Wrapping::WordOrGlyph),
    )
    .padding(14)
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(theme::LANE.into()),
        border: Border {
            color: theme::TRACK,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// What the trials of a proposal found, one line for each: the card, the outcome and why.
pub(super) fn trials(item: &InboxItem) -> Element<'_, AppMessage> {
    let lines = item.data["trials"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|trial| {
            let outcome = trial["outcome"].as_str().unwrap_or_default();
            let card = match trial["card"].as_i64() {
                Some(card) => format!("Card {card}"),
                None => "No card".to_string(),
            };
            row![
                tag(card, Tone::Plain),
                tag(
                    sentence(outcome),
                    if outcome == "positive" {
                        Tone::Teal
                    } else {
                        Tone::Plain
                    }
                ),
                text(trial["reason"].as_str().unwrap_or_default())
                    .size(theme::BODY)
                    .color(theme::TEXT_SECONDARY)
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        });
    column(lines).spacing(6).into()
}

/// The facts of a change that waits to be merged: what it took, how its checks stand and where it is.
pub(super) fn facts<'a>(item: &'a InboxItem, found: &Found<'a>) -> Element<'a, AppMessage> {
    let mut facts = Vec::new();
    if let Some(card) = found.card {
        facts.push(fact("Rounds", card.rounds.to_string(), theme::TEXT, None));
        facts.push(fact(
            "Tokens",
            board::tokens(card.tokens),
            theme::TEXT,
            None,
        ));
    }
    let checks = match found.change.and_then(|c| c.checks.as_deref()) {
        Some("success") => ("CI green", theme::TEAL),
        Some("pending") => ("CI running", theme::TEXT),
        Some("failure") => ("CI failed", theme::TEXT),
        _ => ("No checks", theme::TEXT),
    };
    let mut verdict = checks.0.to_string();
    if let Some(review) = found.change.and_then(|c| c.review.as_deref()) {
        verdict = format!("{verdict} · review {review}");
    }
    facts.push(fact("Checks", verdict, checks.1, None));
    let branch = item.data["branch"]
        .as_str()
        .or(found.change.map(|c| c.branch.as_str()));
    if let Some(branch) = branch {
        facts.push(fact(
            "Branch",
            branch.to_string(),
            theme::TEXT,
            Some(theme::MONO),
        ));
    }
    row(facts).spacing(8).width(Length::Fill).into()
}

/// One box of facts: a small label over a value.
fn fact<'a>(
    label: &'static str,
    value: String,
    color: Color,
    font: Option<iced::Font>,
) -> Element<'a, AppMessage> {
    let mut value = text(value).size(theme::BODY).color(color);
    if let Some(font) = font {
        value = value.font(font).size(theme::CODE);
    }
    container(
        column![
            text(label).size(theme::LABEL).color(theme::TEXT_MUTED),
            value
        ]
        .spacing(2),
    )
    .padding([8, 11])
    .width(Length::FillPortion(1))
    .style(|_| container::Style {
        background: Some(theme::LANE.into()),
        border: Border {
            color: theme::TRACK,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// A filter chip of the inbox.
pub(super) fn chip<'a>(
    label: String,
    selected: bool,
    red: bool,
    message: Message,
) -> Element<'a, AppMessage> {
    page::chip(label, selected, red, AppMessage::Inbox(message))
}
