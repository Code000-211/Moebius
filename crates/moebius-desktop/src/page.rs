//! The parts every page is made of: the header that opens it and the white box that holds its content.

use iced::widget::{column, container, row, text};
use iced::{Border, Element, Length, Padding};

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
    let mut title_row = row![
        text(title)
            .size(theme::PAGE_TITLE)
            .font(theme::manrope(iced::font::Weight::Bold))
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);
    if let Some(tag) = tag {
        title_row = title_row.push(
            container(
                text(tag)
                    .size(theme::LABEL)
                    .font(theme::manrope(iced::font::Weight::Semibold))
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
            }),
        );
    }
    let mut heading = column![].spacing(4).width(Length::Fill);
    if !breadcrumb.is_empty() {
        heading = heading.push(
            text(breadcrumb)
                .size(theme::BREADCRUMB)
                .color(theme::TEXT_MUTED),
        );
    }
    row![heading.push(title_row).push(context), actions]
        .align_y(iced::Alignment::End)
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
