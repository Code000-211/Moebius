//! A list with an editor beside it, as the Agents, Skills and MCP servers pages have it: the cards of the list on the
//! left, the white boxes of the editor on the right, and the editor's head with the name and its Save and Delete.

use std::borrow::Cow;

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{Row, button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::screens::connect;
use crate::theme;
use crate::widgets::page::{self, Tone};

/// The page: its `header`, what stands `above` the list (a box to set something up, say), and the `list` beside the
/// `detail`, each scrolling on its own.
pub fn layout<'a>(
    header: Element<'a, AppMessage>,
    above: Option<Element<'a, AppMessage>>,
    list: impl Into<Element<'a, AppMessage>>,
    detail: impl Into<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    column![
        header,
        above,
        row![
            scrollable(list).width(240),
            scrollable(detail).width(Length::Fill)
        ]
        .spacing(12)
        .height(Length::Fill),
    ]
    .spacing(theme::PAGE_GAP)
    .width(Length::Fill)
    .into()
}

/// A white box around `content`.
pub fn boxed<'a>(content: impl Into<Element<'a, AppMessage>>) -> Element<'a, AppMessage> {
    container(content)
        .padding(14)
        .width(Length::Fill)
        .style(|_| page::box_style())
        .into()
}

/// A card of the list: a name, its tags, and a line that is red when something is wrong. A card without `on_press`
/// cannot be chosen.
pub fn list_card<'a>(
    name: &'a str,
    tags: Vec<(&'static str, Tone)>,
    line: impl Into<Cow<'a, str>>,
    red: bool,
    chosen: bool,
    on_press: Option<AppMessage>,
) -> Element<'a, AppMessage> {
    let line = line.into();
    let mut head = row![
        text(name)
            .size(theme::CARD_TITLE)
            .font(theme::manrope(Weight::Semibold)),
        space::horizontal(),
    ]
    .align_y(Alignment::Center);
    for (tag, tone) in tags {
        head = head.push(page::tag(tag.to_string(), tone));
    }
    let mut content = column![head].spacing(3);
    if !line.is_empty() {
        content = content.push(
            text(line)
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(if red {
                    theme::RED_TEXT
                } else {
                    theme::TEXT_MUTED
                })
                .wrapping(Wrapping::WordOrGlyph),
        );
    }
    button(content)
        .width(Length::Fill)
        .padding([9, 11])
        .on_press_maybe(on_press)
        .style(move |_, status| {
            let (fill, border_col) = match (red, chosen, status) {
                (true, true, _) => (theme::RED_FILL, theme::RED),
                (true, false, _) => (theme::RED_FILL, theme::RED_BORDER),
                (false, true, _) => (theme::HEAD_PILL, theme::TEXT),
                (false, false, button::Status::Hovered) => (theme::LANE, theme::TEXT_MUTED),
                (false, false, _) => (theme::SURFACE, theme::LINE),
            };
            button::Style {
                background: Some(fill.into()),
                text_color: theme::TEXT,
                border: Border {
                    color: border_col,
                    width: 1.0,
                    radius: theme::RADIUS_CARD.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

/// The editor's head: the name, as an input with `placeholder` while a new one is named, and the `actions` on the
/// right.
pub fn head<'a>(
    naming: bool,
    name: &'a str,
    placeholder: &'a str,
    on_name: impl Fn(String) -> AppMessage + 'a,
    actions: Row<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    let title: Element<_> = if naming {
        text_input(placeholder, name)
            .on_input(on_name)
            .size(theme::BODY)
            .padding([6, 10])
            .width(240)
            .style(connect::input_style)
            .into()
    } else {
        text(name)
            .size(16)
            .font(theme::manrope(Weight::Bold))
            .into()
    };
    row![title, space::horizontal(), actions]
        .align_y(Alignment::Center)
        .into()
}

/// What a person does to the thing in the editor, by the buttons of its head.
pub struct Actions {
    pub ask_delete: AppMessage,
    pub keep: AppMessage,
    pub delete: AppMessage,
    pub save: Option<AppMessage>,
}

/// The buttons of the editor's head: Delete (when the thing `exists`) and Save, or, while a delete is `confirming`,
/// the question with Keep and `delete_label`.
pub fn actions<'a>(
    name: &str,
    exists: bool,
    confirming: bool,
    delete_label: &'a str,
    messages: Actions,
) -> Row<'a, AppMessage> {
    let actions = row![].spacing(8).align_y(Alignment::Center);
    if confirming {
        return actions
            .push(
                text(format!("Delete {name}?"))
                    .size(theme::CONTEXT)
                    .color(theme::TEXT_SECONDARY),
            )
            .push(page::white_button("Keep", Some(messages.keep)))
            .push(page::white_button(delete_label, Some(messages.delete)));
    }
    let actions = if exists {
        actions.push(page::white_button("Delete", Some(messages.ask_delete)))
    } else {
        actions
    };
    actions.push(page::primary_button("Save", messages.save))
}
