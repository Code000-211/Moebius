//! The board of a repository, the first tab of its project's page (`project` has the others): its columns of the default workflow, in order, with their cards, and the page that a
//! click on a card opens. Everything a card shows comes from the snapshot; `docs/design/board.png` is the picture.
//!
//! Cards are ordered by number. Done lists the ten highest numbers, highest first, because it keeps growing.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Alignment, Border, Element, Length, Padding};

use crate::app::Message;
use crate::client::{Board, Card, InboxItem};
use crate::theme;
use crate::widgets::edits::{self, Edits};
use crate::widgets::inline_edit;
use crate::widgets::new_issue::{self, NewIssue};
use crate::widgets::page::{self, Tone, tag};

/// The columns of the default workflow, left to right, each with the script behind it.
const COLUMNS: [(&str, &str); 6] = [
    ("Backlog", "backlog.luau"),
    ("Ready", "ready.luau"),
    ("In progress", "code.luau"),
    ("In review", "review.luau"),
    ("Ready to merge", "merge.luau"),
    ("Done", "done.luau"),
];

/// How many cards Done lists.
const DONE_SHOWN: usize = 10;

/// What a person is asked, as the card's inbox item kind says; `stuck` is not a question but a card that is blocked.
const STUCK: &str = "stuck";

fn count(n: usize, one: &str, many: &str) -> String {
    match n {
        0 => format!("no {many}"),
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}

/// The open inbox item of a card, if it has one.
fn inbox_of<'a>(inbox: &'a [InboxItem], card: &Card) -> Option<&'a str> {
    inbox
        .iter()
        .find(|item| item.card_id == Some(card.id))
        .map(|item| item.kind.as_str())
}

/// Whether the card waits for a person to decide.
fn waits_for_you(inbox: &[InboxItem], card: &Card) -> bool {
    inbox_of(inbox, card).is_some_and(|kind| kind != STUCK)
}

/// The cards of a column in the order shown.
fn cards_of<'a>(board: &'a Board, column: &str) -> Vec<&'a Card> {
    let mut cards: Vec<_> = board.cards.iter().filter(|c| c.column == column).collect();
    cards.sort_by_key(|c| c.number);
    if column == "Done" {
        cards.reverse();
    }
    cards
}

/// The page of a board: its header, the project's tabs and the columns. A GitHub repository's header names the logins
/// it trusts, with the pencil that edits them in place.
pub fn view<'a>(
    board: &'a Board,
    inbox: &'a [InboxItem],
    new_issue: &'a NewIssue,
    edits: &'a Edits,
    tabs: Element<'a, Message>,
) -> Element<'a, Message> {
    let repository = &board.repository;
    let tag = match &repository.github {
        Some(github) => format!("GitHub · {github}"),
        None => "No forge".to_string(),
    };
    let waiting_changes = inbox
        .iter()
        .filter(|item| item.kind == "ready_to_merge")
        .filter(|item| board.cards.iter().any(|c| item.card_id == Some(c.id)))
        .count();
    let line = format!(
        "{} working · {} · default workflow",
        count(board.live_runs(), "run", "runs"),
        count(
            waiting_changes,
            "change waits for you",
            "changes wait for you"
        ),
    );
    let mut line = row![text(line).size(theme::CONTEXT).color(theme::TEXT_SECONDARY)]
        .align_y(Alignment::Center);
    if repository.github.is_some() {
        let trusted = match repository.trusted.as_slice() {
            [] => " · no trusted authors".to_string(),
            trusted => format!(" · trusts {}", trusted.join(", ")),
        };
        line = line.push(
            text(trusted)
                .size(theme::CONTEXT)
                .color(theme::TEXT_SECONDARY),
        );
        line = line.push(space().width(6));
        line = line.push(inline_edit::pencil(
            "edit-trusted".to_string(),
            Message::Edit(edits::Message::EditTrusted(
                repository.id,
                repository.trusted.clone(),
            )),
        ));
    }
    let columns = COLUMNS
        .iter()
        .filter(|(name, _)| *name != "Backlog" || board.cards.iter().any(|c| c.column == *name))
        .map(|(name, script)| column_view(board, inbox, name, script));
    let mut page = column![page::header(
        "Projects",
        &repository.name,
        Some(tag),
        line.into(),
        page::white_button(
            "New issue",
            (!new_issue.is_open_on(repository.id))
                .then_some(Message::NewIssue(new_issue::Message::Open(repository.id))),
        ),
    )]
    .spacing(theme::PAGE_GAP)
    .width(Length::Fill)
    .push(tabs);
    if let Some(editor) = edits.trusted_view(repository.id) {
        page = page.push(editor);
    }
    if new_issue.is_open_on(repository.id) {
        page = page.push(new_issue.view());
    }
    page.push(
        scrollable(
            row(columns)
                .spacing(10)
                .align_y(Alignment::Start)
                .width(Length::Fill),
        )
        .height(Length::Fill),
    )
    .into()
}

fn column_view<'a>(
    board: &'a Board,
    inbox: &'a [InboxItem],
    name: &'a str,
    script: &'a str,
) -> Element<'a, Message> {
    let cards = cards_of(board, name);
    let total = cards.len();
    let waits = cards.iter().any(|card| waits_for_you(inbox, card));
    // The first card of Ready that nothing blocks is the one the board starts next.
    let next = (name == "Ready")
        .then(|| cards.iter().find(|card| card.blocked_by.is_empty()))
        .flatten()
        .map(|card| card.id);
    let head = column![
        row![
            text(name)
                .size(theme::COLUMN_NAME)
                .font(theme::manrope(Weight::Bold)),
            space::horizontal(),
            text(total.to_string())
                .size(theme::SMALL)
                .color(theme::TEXT_MUTED),
        ]
        .align_y(Alignment::Center),
        row![
            text(script).size(theme::SMALL).color(theme::TEXT_MUTED),
            space::horizontal(),
            inline_edit::pencil(
                format!("edit-script-{name}"),
                Message::EditScript(board.repository.id, name.to_string()),
            ),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(2)
    .width(Length::Fill);
    let head = column![
        container(head).padding(Padding {
            top: 3.0,
            right: 3.0,
            bottom: 9.0,
            left: 3.0,
        }),
        rule(),
    ];
    let shown = if name == "Done" {
        DONE_SHOWN
    } else {
        usize::MAX
    };
    let cards = cards.into_iter().take(shown).map(|card| {
        card_view(
            card,
            board.round_limit,
            inbox_of(inbox, card),
            next == Some(card.id),
        )
    });
    let border = if waits {
        theme::AMBER_BORDER
    } else {
        theme::LINE
    };
    container(column![head, column(cards).spacing(9)].spacing(9))
        .width(Length::FillPortion(1))
        .padding(11)
        .style(move |_| container::Style {
            background: Some(theme::LANE.into()),
            border: Border {
                color: border,
                width: 1.0,
                radius: theme::RADIUS_BOX.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// The line under a column's heading.
fn rule<'a>() -> Element<'a, Message> {
    container(space())
        .width(Length::Fill)
        .height(1)
        .style(|_| container::Style {
            background: Some(theme::LINE.into()),
            ..container::Style::default()
        })
        .into()
}

/// The small line above a card's title: its number and what it is doing.
fn card_line(card: &Card, round_limit: i64) -> String {
    let number = format!("#{}", card.number);
    match card.column.as_str() {
        "In progress" | "In review" => {
            let mut line = number;
            if let Some(run) = &card.run {
                line = format!("{line} · {}", run.agent);
            }
            match (card.rounds, round_limit) {
                (0, _) => {}
                (round, 0) => line = format!("{line} · round {round}"),
                (round, limit) => line = format!("{line} · round {round} of {limit}"),
            }
            line
        }
        "Ready to merge" => {
            let rounds = count(card.rounds.max(0) as usize, "round", "rounds");
            format!("{number} · {rounds} · {}", tokens(card.tokens))
        }
        _ => number,
    }
}

/// A number of tokens as the board writes it: `96k tok`, `1.2M tok`.
pub fn tokens(n: i64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M tok", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{}k tok", n / 1_000),
        n => format!("{n} tok"),
    }
}

/// The tags of a card: what runs, what it costs, what stops it, how its change stands and who it waits for.
fn tags<'a>(card: &Card, inbox_kind: Option<&str>, next: bool) -> Vec<Element<'a, Message>> {
    let mut tags = Vec::new();
    if card.is_live() {
        tags.push(tag("Live".into(), Tone::Teal));
    }
    if card.tokens > 0 && matches!(card.column.as_str(), "In progress" | "In review") {
        tags.push(tag(tokens(card.tokens), Tone::Plain));
    }
    if !card.blocked_by.is_empty() {
        // A tag does not wrap, so a long list is cut short.
        let numbers: Vec<_> = card.blocked_by.iter().map(|n| format!("#{n}")).collect();
        let label = match numbers.len() {
            1..=2 => format!("Blocked by {}", numbers.join(", ")),
            n => format!("Blocked by {} and {} more", numbers[0], n - 1),
        };
        tags.push(tag(label, Tone::Red));
    }
    if card.refusals > 0 {
        let label = count(card.refusals as usize, "rule broken", "rules broken");
        tags.push(tag(label, Tone::Red));
    }
    match card.change.as_ref().map(|change| change.checks.as_str()) {
        Some("pending") => tags.push(tag("CI running".into(), Tone::Plain)),
        Some("success") => tags.push(tag("Green".into(), Tone::Teal)),
        Some("failure") => tags.push(tag("CI failed".into(), Tone::Red)),
        _ => {}
    }
    match inbox_kind {
        Some(STUCK) => tags.push(tag("Stuck".into(), Tone::Red)),
        Some(_) => tags.push(tag("Waits for you".into(), Tone::Amber)),
        None => {}
    }
    if next {
        tags.push(tag("Next".into(), Tone::Plain));
    }
    tags
}

/// The segments of the round bar: one per round the card gets, the rounds so far filled.
fn round_bar<'a>(rounds: i64, limit: i64) -> Element<'a, Message> {
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

fn card_view<'a>(
    card: &'a Card,
    round_limit: i64,
    inbox_kind: Option<&str>,
    next: bool,
) -> Element<'a, Message> {
    let done = card.column == "Done";
    let (line_color, title_color, weight) = if done {
        (theme::TEXT_MUTED, theme::TEXT_MUTED, Weight::Medium)
    } else {
        (theme::TEXT_MUTED, theme::TEXT, Weight::Semibold)
    };
    let mut content = column![
        text(card_line(card, round_limit))
            .size(theme::SMALL)
            .color(line_color),
        text(card.title.as_str())
            .size(theme::CARD_TITLE)
            .font(theme::manrope(weight))
            .color(title_color)
            .width(Length::Fill)
            .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(8)
    .width(Length::Fill);
    if round_limit > 0
        && card.rounds > 0
        && matches!(card.column.as_str(), "In progress" | "In review")
    {
        content = content.push(round_bar(card.rounds, round_limit));
    }
    let tags = tags(card, inbox_kind, next);
    if !tags.is_empty() {
        content = content.push(row(tags).spacing(6).wrap().vertical_spacing(6));
    }
    let border = if card.is_live() {
        theme::TEAL
    } else {
        theme::LINE
    };
    button(
        container(content)
            .padding([12, 13])
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(theme::SURFACE.into()),
                border: Border {
                    color: border,
                    width: 1.0,
                    radius: theme::RADIUS_CARD.into(),
                },
                ..container::Style::default()
            }),
    )
    .padding(0)
    .width(Length::Fill)
    .on_press(Message::OpenCard(card.id))
    .style(|_, _| button::Style::default())
    .into()
}
