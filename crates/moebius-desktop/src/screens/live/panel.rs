//! "This run", the panel beside the live view's conversation: the card's rounds and tokens, the agent, the
//! judgements and the branch.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, space, text};
use iced::{Border, Element, Length, Padding};

use crate::app::Message as AppMessage;
use crate::client::{Board, Card};
use crate::theme;
use crate::widgets::page;

use super::Live;
use super::view::{count_rules, harness_name};

impl Live {
    /// "This run": what the server knows of it.
    pub(super) fn panel<'a>(
        &'a self,
        board: &'a Board,
        card: &'a Card,
        refused: usize,
    ) -> Element<'a, AppMessage> {
        let mut items: Vec<Element<'a, AppMessage>> = Vec::new();
        if card.rounds > 0 && board.round_limit > 0 {
            let line = format!("{} of {}", card.rounds, board.round_limit);
            items.push(item(
                "Round",
                Some(round_bar(card.rounds, board.round_limit)),
                text(line).size(theme::BODY).into(),
            ));
        }
        if card.tokens > 0 {
            items.push(item(
                "Tokens on this card",
                None,
                text(tokens(card.tokens)).size(theme::BODY).into(),
            ));
        }
        items.push(item(
            "Agent",
            None,
            row![
                text(self.run.agent.as_str())
                    .size(theme::BODY)
                    .color(theme::TEAL),
                text(format!(" · {}", harness_name(&self.run.harness))).size(theme::BODY),
            ]
            .into(),
        ));
        if let Some(runner) = &self.run.runner {
            items.push(item(
                "Runner",
                None,
                text(runner.as_str()).size(theme::BODY).into(),
            ));
        }
        let mut judgements =
            row![text(format!("{} saved", self.judgements.len())).size(theme::BODY)];
        if refused > 0 {
            judgements = judgements.push(text(" · ").size(theme::BODY)).push(
                text(count_rules(refused))
                    .size(theme::BODY)
                    .font(theme::manrope(Weight::Semibold))
                    .color(theme::RED_TEXT),
            );
        }
        items.push(item("Judgements", None, judgements.into()));
        if let Some(change) = board.changes.iter().find(|c| c.card_id == Some(card.id)) {
            items.push(item(
                "Branch",
                None,
                text(change.branch.as_str())
                    .font(theme::MONO)
                    .size(theme::CODE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .into(),
            ));
        }
        let mut panel = column![
            text("This run")
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold))
        ]
        .spacing(2);
        for (index, item) in items.into_iter().enumerate() {
            if index > 0 {
                panel = panel.push(page::rule());
            }
            panel = panel.push(item);
        }
        container(panel)
            .padding([14, 18])
            .width(300)
            .style(|_| page::box_style())
            .into()
    }
}

/// A number of tokens: `412k`, `1.5M`.
pub fn tokens(n: i64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M tokens", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{}k tokens", n / 1_000),
        n => format!("{n} tokens"),
    }
}

/// A label over its value, in the panel; `bar` goes between them.
fn item<'a>(
    label: &'a str,
    bar: Option<Element<'a, AppMessage>>,
    value: Element<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    let mut item = column![
        text(label)
            .size(theme::LABEL)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::SIDEBAR_GROUP)
    ]
    .spacing(4);
    if let Some(bar) = bar {
        item = item.push(bar);
    }
    container(item.push(value))
        .padding(Padding::from([8, 0]))
        .into()
}

/// The segments of the rounds: one per round the card gets, the rounds so far filled.
fn round_bar<'a>(rounds: i64, limit: i64) -> Element<'a, AppMessage> {
    row((0..limit).map(|round| {
        let fill = if round < rounds {
            theme::TEAL
        } else {
            theme::TRACK
        };
        container(space())
            .width(Length::FillPortion(1))
            .height(4)
            .style(move |_| container::Style {
                background: Some(fill.into()),
                border: Border {
                    radius: theme::RADIUS_SEGMENT.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into()
    }))
    .spacing(3)
    .width(Length::Fill)
    .into()
}
