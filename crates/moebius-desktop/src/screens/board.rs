//! The board of a repository, the first tab of its project's page (`project` has the others): its columns of the default workflow, in order, with their cards,
//! and the zero-scroll `ZS-1` run inspector dock below them. Everything comes from the snapshot.
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

/// The card highlighted in the zero-scroll bottom inspector dock: the first live card, else the first waiting one.
fn inspected_card<'a>(board: &'a Board, inbox: &[InboxItem]) -> Option<&'a Card> {
    board
        .cards
        .iter()
        .find(|c| c.is_live())
        .or_else(|| board.cards.iter().find(|c| inbox_of(inbox, c).is_some()))
        .or_else(|| board.cards.iter().find(|c| c.column != "Done"))
}

/// The page of a board: its header, the project's tabs, the L2B columns with the ZS-1 zero-scroll dock underneath,
/// and the project's Files sidebar on the right.
pub fn view<'a>(
    board: &'a Board,
    inbox: &'a [InboxItem],
    new_issue: &'a NewIssue,
    edits: &'a Edits,
    tabs: Element<'a, Message>,
    files: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let repository = &board.repository;
    let tag = repository.forge_tag();
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
    let mut line = row![
        text(line)
            .size(theme::CONTEXT)
            .font(theme::MONO)
            .color(theme::TEXT_SECONDARY)
    ]
    .align_y(Alignment::Center);
    if repository.on_forge() {
        let trusted = match repository.trusted.as_slice() {
            [] => " · no trusted authors".to_string(),
            trusted => format!(" · trusts {}", trusted.join(", ")),
        };
        line = line.push(
            text(trusted)
                .size(theme::CONTEXT)
                .font(theme::MONO)
                .color(theme::TEXT_SECONDARY),
        );
        line = line.push(space().width(theme::SPACE_6));
        line = line.push(inline_edit::pencil(
            "edit-trusted".to_string(),
            Message::Edit(edits::Message::EditTrusted(
                repository.id,
                repository.trusted.clone(),
            )),
        ));
    }
    let inspected = inspected_card(board, inbox);
    let inspected_id = inspected.map(|c| c.id);
    let columns = COLUMNS
        .iter()
        .filter(|(name, _)| *name != "Backlog" || board.cards.iter().any(|c| c.column == *name))
        .map(|(name, script)| column_view(board, inbox, name, script, inspected_id));
    let mut page = column![page::header(
        "Projects",
        &repository.name,
        Some(tag),
        line.into(),
        page::primary_button(
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
    let columns = scrollable(
        row(columns)
            .spacing(theme::SPACE_10)
            .align_y(Alignment::Start)
            .width(Length::Fill),
    )
    .height(Length::Fill);

    let mut workspace = column![columns]
        .spacing(theme::SPACE_10)
        .width(Length::Fill)
        .height(Length::Fill);
    if let Some(card) = inspected {
        workspace = workspace.push(zs1_inspector_dock(board, card, inbox_of(inbox, card)));
    }

    // The Files sidebar of the project stands on the right of the workspace column.
    let mut body = row![workspace]
        .spacing(theme::PAGE_GAP)
        .height(Length::Fill);
    if let Some(files) = files {
        body = body.push(files);
    }
    page.push(body).into()
}

/// The ZS-1 zero-scroll 2-column inspector dock at the bottom of the board (fixed height, never scrolls).
fn zs1_inspector_dock<'a>(
    board: &'a Board,
    card: &'a Card,
    inbox_kind: Option<&str>,
) -> Element<'a, Message> {
    let status_text = if card.is_live() {
        "● Live"
    } else if inbox_kind.is_some() {
        "● Needs you"
    } else {
        card.column.as_str()
    };
    let status_color = if card.is_live() {
        theme::LIVE
    } else if inbox_kind.is_some() {
        theme::AMBER
    } else {
        theme::TEXT_SECONDARY
    };

    let left = container(
        column![
            row![
                text(format!("Card #{}", card.number))
                    .size(theme::SMALL)
                    .font(theme::MONO)
                    .color(theme::ACCENT),
                space::horizontal(),
                text(status_text)
                    .size(theme::SMALL)
                    .font(theme::MONO)
                    .color(status_color),
            ]
            .align_y(Alignment::Center),
            text(card.title.as_str())
                .size(theme::BODY)
                .font(theme::manrope(Weight::Bold))
                .color(theme::TEXT)
                .wrapping(Wrapping::None),
            text(card_line(card, board.round_limit))
                .size(theme::SMALL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED)
                .wrapping(Wrapping::None),
            space::vertical(),
            row![page::white_button(
                "Open live view",
                Some(Message::OpenCard(card.id)),
            )]
            .spacing(theme::SPACE_6),
        ]
        .spacing(theme::SPACE_4)
        .height(Length::Fill),
    )
    .width(Length::Fixed(theme::DOCK_META_WIDTH))
    .height(Length::Fill)
    .padding([theme::SPACE_10, theme::SPACE_14]);

    let wait_reason = board
        .waiting
        .get(&card.id)
        .map(String::as_str)
        .unwrap_or("Ready in column");
    let run_line = match &card.run {
        Some(run) => format!("{} · {} ({})", run.agent, run.purpose, run.status),
        None => "no run started yet".to_string(),
    };
    let check_line = match &card.change {
        Some(change) => format!("checks: {}", change.checks),
        None if !card.blocked_by.is_empty() => {
            let nums: Vec<_> = card.blocked_by.iter().map(|n| format!("#{n}")).collect();
            format!("blocked by {}", nums.join(", "))
        }
        None => format!("tokens: {}", tokens(card.tokens)),
    };

    let slot = |label: &'static str,
                detail: String,
                tone: Tone,
                tag_text: String|
     -> Element<'a, Message> {
        container(
            row![
                text(label)
                    .size(theme::CODE_SMALL)
                    .font(theme::MONO)
                    .color(theme::TEXT),
                text(detail)
                    .size(theme::CODE_SMALL)
                    .font(theme::MONO)
                    .color(theme::TEXT_SECONDARY)
                    .width(Length::Fill)
                    .wrapping(Wrapping::None),
                tag(tag_text, tone),
            ]
            .spacing(theme::SPACE_8)
            .align_y(Alignment::Center),
        )
        .padding([theme::SPACE_4, theme::SPACE_10])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::RECESSED.into()),
            border: Border {
                color: theme::LINE,
                width: 1.0,
                radius: theme::RADIUS_SMALL.into(),
            },
            ..container::Style::default()
        })
        .into()
    };

    let guard_tone = if card.refusals > 0 {
        Tone::Red
    } else {
        Tone::Teal
    };
    let guard_tag = if card.refusals > 0 {
        count(card.refusals as usize, "rule broken", "rules broken")
    } else {
        "Guard ok".to_string()
    };

    let right = container(
        column![
            row![
                text("LIVE CARD STATUS & GUARD SUMMARY")
                    .size(theme::SMALL)
                    .font(theme::MONO)
                    .color(theme::ACCENT),
                space::horizontal(),
                text(format!("{} · {}", board.repository.name, card.column))
                    .size(theme::SMALL)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED),
            ]
            .align_y(Alignment::Center),
            slot(
                "run   ",
                run_line,
                if card.is_live() {
                    Tone::Live
                } else {
                    Tone::Plain
                },
                if card.is_live() {
                    "Live".into()
                } else {
                    card.column.clone()
                },
            ),
            slot("guard ", wait_reason.to_string(), guard_tone, guard_tag),
            slot("state ", check_line, Tone::Plain, tokens(card.tokens)),
        ]
        .spacing(theme::SPACE_6)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding([theme::SPACE_10, theme::SPACE_14]);

    let v_rule = container(space().width(0))
        .width(theme::HAIRLINE)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::LINE.into()),
            ..container::Style::default()
        });

    container(row![left, v_rule, right].height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(theme::DOCK_HEIGHT))
        .style(|_| container::Style {
            background: Some(theme::SURFACE.into()),
            border: Border {
                color: theme::ACCENT_BORDER,
                width: 1.0,
                radius: theme::RADIUS_BOX.into(),
            },
            shadow: theme::shadow(),
            ..container::Style::default()
        })
        .into()
}

fn column_view<'a>(
    board: &'a Board,
    inbox: &'a [InboxItem],
    name: &'a str,
    script: &'a str,
    inspected_id: Option<i64>,
) -> Element<'a, Message> {
    let cards = cards_of(board, name);
    let total = cards.len();
    let waits = cards.iter().any(|card| waits_for_you(inbox, card));
    let has_live = cards.iter().any(|card| card.is_live());
    // The first card of Ready that nothing blocks is the one the board starts next.
    let next = (name == "Ready")
        .then(|| cards.iter().find(|card| card.blocked_by.is_empty()))
        .flatten()
        .map(|card| card.id);
    let head = row![
        text(name)
            .size(theme::COLUMN_NAME)
            .font(theme::manrope(Weight::Bold))
            .color(if has_live {
                theme::LIVE
            } else if waits {
                theme::AMBER
            } else {
                theme::TEXT
            }),
        space::horizontal(),
        text(total.to_string())
            .size(theme::SMALL)
            .font(theme::MONO)
            .color(theme::TEXT_MUTED),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill);
    // The script on its own line, so that a narrow column's name never runs into it.
    let head = column![
        head,
        row![
            text(script)
                .size(theme::SMALL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
            space::horizontal(),
            inline_edit::pencil(
                format!("edit-script-{name}"),
                Message::EditScript(board.repository.id, name.to_string()),
            ),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(theme::SPACE_2);

    let head = column![
        container(head).padding(Padding {
            top: theme::SPACE_6,
            right: theme::SPACE_4,
            bottom: theme::SPACE_8,
            left: theme::SPACE_4,
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
            inspected_id == Some(card.id),
        )
    });
    let border = if waits {
        theme::AMBER_BORDER
    } else if has_live {
        theme::LIVE_BORDER
    } else {
        theme::LINE
    };
    container(column![head, column(cards).spacing(theme::SPACE_8)].spacing(theme::SPACE_8))
        .width(Length::FillPortion(1))
        .padding(theme::SPACE_10)
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
        .height(theme::HAIRLINE)
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
        tags.push(tag("Live".into(), Tone::Live));
    }
    if card.tokens > 0 && matches!(card.column.as_str(), "In progress" | "In review") {
        tags.push(tag(tokens(card.tokens), Tone::Plain));
    }
    if !card.blocked_by.is_empty() {
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
        Some("pending") => tags.push(tag("CI running".into(), Tone::Live)),
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
fn round_bar<'a>(rounds: i64, limit: i64, selected: bool) -> Element<'a, Message> {
    let active_color = if selected { theme::ACCENT } else { theme::LIVE };
    row((0..limit).map(|round| {
        let fill = if round < rounds {
            active_color
        } else {
            theme::TRACK
        };
        container(space())
            .width(Length::FillPortion(1))
            .height(theme::BAR)
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
    .spacing(theme::SPACE_4)
    .width(Length::Fill)
    .into()
}

fn card_view<'a>(
    card: &'a Card,
    round_limit: i64,
    inbox_kind: Option<&str>,
    next: bool,
    selected: bool,
) -> Element<'a, Message> {
    let done = card.column == "Done";
    let (line_color, title_color, weight) = if done {
        (theme::TEXT_MUTED, theme::TEXT_MUTED, Weight::Medium)
    } else if selected {
        (theme::ACCENT, theme::TEXT, Weight::Semibold)
    } else {
        (theme::TEXT_MUTED, theme::TEXT, Weight::Semibold)
    };
    let mut content = column![
        text(card_line(card, round_limit))
            .size(theme::SMALL)
            .font(theme::MONO)
            .color(line_color),
        text(card.title.as_str())
            .size(theme::CARD_TITLE)
            .font(theme::manrope(weight))
            .color(title_color)
            .width(Length::Fill)
            .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(theme::SPACE_8)
    .width(Length::Fill);
    if round_limit > 0
        && card.rounds > 0
        && matches!(card.column.as_str(), "In progress" | "In review")
    {
        content = content.push(round_bar(card.rounds, round_limit, selected));
    }
    let tags = tags(card, inbox_kind, next);
    if !tags.is_empty() {
        content = content.push(
            row(tags)
                .spacing(theme::SPACE_6)
                .wrap()
                .vertical_spacing(theme::SPACE_6),
        );
    }
    let border = if selected {
        theme::ACCENT
    } else if card.is_live() {
        theme::LIVE_BORDER
    } else {
        theme::LINE
    };
    button(
        container(content)
            .padding([theme::SPACE_10, theme::SPACE_12])
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(theme::CARD.into()),
                border: Border {
                    color: border,
                    width: if selected { 1.5 } else { 1.0 },
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
