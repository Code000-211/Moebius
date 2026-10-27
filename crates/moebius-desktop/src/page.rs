//! The parts every page is made of: the header that opens it and the white box that holds its content.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Border, Color, Element, Length, Padding};

use crate::theme;

/// The header of a page: a breadcrumb (none when empty), the title with an optional tag, one line of context. The actions of the page
/// go on the right, in `actions`.
pub fn header<'a, Message: 'a>(
    breadcrumb: &'a str,
    title: &'a str,
    tag: Option<String>,
    context: Element<'a, Message>,
    actions: Element<'a, Message>,
) -> Element<'a, Message> {
    let breadcrumb = (!breadcrumb.is_empty()).then(|| {
        text(breadcrumb)
            .size(theme::BREADCRUMB)
            .color(theme::TEXT_MUTED)
            .into()
    });
    let tag = tag.map(|tag| {
        container(
            text(tag)
                .size(theme::LABEL)
                .font(theme::manrope(Weight::Semibold))
                .color(theme::TEXT_SECONDARY),
        )
        .padding(Padding::from([2.0, 9.0]))
        .style(|_| container::Style {
            border: Border {
                color: theme::LINE,
                width: 1.0,
                radius: theme::RADIUS_ROUND.into(),
            },
            ..container::Style::default()
        })
        .into()
    });
    header_with(breadcrumb, title, tag, context, actions)
}

/// Like `header`, with the breadcrumb and the tag as elements, for a breadcrumb that is a link or a tag that has a colour.
pub fn header_with<'a, Message: 'a>(
    breadcrumb: Option<Element<'a, Message>>,
    title: &'a str,
    tag: Option<Element<'a, Message>>,
    context: Element<'a, Message>,
    actions: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut title_row = row![
        text(title)
            .size(theme::PAGE_TITLE)
            .font(theme::manrope(Weight::Bold))
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if let Some(tag) = tag {
        title_row = title_row.push(tag);
    }
    let mut heading = column![].spacing(4).width(Length::Fill);
    if let Some(breadcrumb) = breadcrumb {
        heading = heading.push(breadcrumb);
    }
    row![heading.push(title_row).push(context), actions]
        .align_y(Alignment::End)
        .into()
}

/// The breadcrumb of a card's page: the repository, which takes back to the board, and the card's number.
pub fn card_breadcrumb<'a, Message: Clone + 'a>(
    repository: &'a str,
    number: i64,
    back: Message,
) -> Element<'a, Message> {
    row![
        button(
            text(repository)
                .size(theme::BREADCRUMB)
                .color(theme::TEXT_SECONDARY)
        )
        .padding(0)
        .on_press(back)
        .style(|_, _| button::Style::default()),
        text(format!(" / #{number}"))
            .size(theme::BREADCRUMB)
            .color(theme::TEXT_MUTED),
    ]
    .into()
}

/// A line of context under a title.
pub fn context<'a, Message: 'a>(line: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    text(line)
        .size(theme::CONTEXT)
        .color(theme::TEXT_SECONDARY)
        .into()
}

/// A white box with a bold heading and its content.
pub fn boxed<'a, Message: 'a>(
    heading: &'a str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(
        column![
            text(heading)
                .size(theme::BOX_HEADING)
                .font(theme::manrope(iced::font::Weight::Bold)),
            content.into(),
        ]
        .spacing(10),
    )
    .padding(20)
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(theme::SURFACE.into()),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_BOX.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// Body text inside a box.
pub fn body<'a, Message: 'a>(line: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    text(line)
        .size(theme::BODY)
        .color(theme::TEXT_SECONDARY)
        .into()
}

#[derive(Clone, Copy)]
pub enum Tone {
    Plain,
    Teal,
    Amber,
    Red,
}

impl Tone {
    /// Border, fill and text of a tag.
    fn colors(self) -> (Color, Color, Color) {
        match self {
            Tone::Plain => (theme::LINE, Color::TRANSPARENT, theme::TEXT_SECONDARY),
            Tone::Teal => (theme::TEAL_BORDER, theme::TEAL_FILL, theme::ONLINE_TEXT),
            Tone::Amber => (theme::AMBER_BORDER, theme::AMBER_FILL, theme::AMBER_TEXT),
            Tone::Red => (theme::RED_BORDER, theme::RED_TAG_FILL, theme::RED_TEXT),
        }
    }
}

/// A tag; a red one has a red dot.
pub fn tag<'a, Message: 'a>(label: String, tone: Tone) -> Element<'a, Message> {
    let (border, fill, color) = tone.colors();
    let label = text(label)
        .size(theme::LABEL)
        .font(theme::manrope(Weight::Semibold))
        .wrapping(Wrapping::None)
        .color(color);
    let content: Element<'a, Message> = if matches!(tone, Tone::Red) {
        row![
            container(space().width(7).height(7)).style(|_| container::Style {
                background: Some(theme::RED.into()),
                border: Border {
                    radius: 3.5.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
            label,
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else {
        label.into()
    };
    container(content)
        .padding([2, 9])
        .style(move |_| container::Style {
            background: Some(fill.into()),
            border: Border {
                color: border,
                width: 1.0,
                radius: theme::RADIUS_ROUND.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// How long ago `at` was, at `now` (both Unix seconds): `just now`, `6 min ago`, `3 h ago`, `2 d ago`.
pub fn ago(now: i64, at: i64) -> String {
    match (now - at).max(0) {
        0..60 => "just now".to_string(),
        seconds @ 60..3600 => format!("{} min ago", seconds / 60),
        seconds @ 3600..86400 => format!("{} h ago", seconds / 3600),
        seconds => format!("{} d ago", seconds / 86400),
    }
}
