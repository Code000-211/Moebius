//! The pieces of the forms on the pages that edit something (Agents, Skills, MCP servers, Rules, Setup, a new chat): a
//! labelled input, the line that says how a save went, the heading of a box, the editor in the code font, the
//! dropdowns' style, and the choices that more than one form offers.

use iced::font::Weight;
use iced::widget::{Column, column, pick_list, row, space, text, text_editor};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::theme;
use crate::widgets::inline_edit::editor_style;

/// The reasoning levels an agent or a chat can be given, as Pi-Bolt takes them.
pub const LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

/// The protocols an upstream or an agent can speak.
pub const PROTOCOLS: [&str; 2] = ["openai", "anthropic"];

/// `input` under its `label`.
pub fn labelled<'a>(
    label: &'a str,
    input: impl Into<Element<'a, AppMessage>>,
) -> Column<'a, AppMessage> {
    column![
        text(label)
            .size(13)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::TEXT_SECONDARY),
        input.into(),
    ]
    .spacing(5)
}

/// `input` under its `label`, sharing a row equally with the fields beside it.
pub fn field<'a>(
    label: &'a str,
    input: impl Into<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    labelled(label, input).width(Length::FillPortion(1)).into()
}

/// One line about what happened to the edit, red when it failed.
pub fn notice_line<'a>(line: &str, red: bool) -> Element<'a, AppMessage> {
    text(line.to_string())
        .size(theme::CONTEXT)
        .font(theme::manrope(Weight::Semibold))
        .color(if red {
            theme::RED_TEXT
        } else {
            theme::ONLINE_TEXT
        })
        .into()
}

/// The line after a save to the harness configuration repository: the commit it made, or that nothing changed.
pub fn committed_line<'a>(commit: Option<&str>) -> Element<'a, AppMessage> {
    match commit {
        Some(commit) => notice_line(&format!("Saved as a commit: {commit}"), false),
        None => notice_line("Nothing changed, so nothing was committed.", false),
    }
}

/// The heading of a box, with what stands on its right.
pub fn box_heading<'a>(
    heading: &'a str,
    aside: impl Into<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    row![
        text(heading)
            .size(theme::BOX_HEADING)
            .font(theme::manrope(Weight::Bold)),
        space::horizontal(),
        aside.into(),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// A muted line on the right of a box's heading.
pub fn aside<'a>(line: impl text::IntoFragment<'a>) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::LABEL)
        .color(theme::TEXT_MUTED)
        .into()
}

/// The text editor of the pages, in the code font. Without `on_action` the text can be read and not changed.
pub fn mono_editor<'a>(
    content: &'a text_editor::Content,
    placeholder: &'a str,
    on_action: Option<fn(text_editor::Action) -> AppMessage>,
    height: u32,
) -> Element<'a, AppMessage> {
    let mut input = text_editor(content)
        .placeholder(placeholder)
        .font(theme::MONO)
        .size(theme::CODE)
        .padding(14)
        .height(height)
        .style(editor_style);
    if let Some(on_action) = on_action {
        input = input.on_action(on_action);
    }
    input.into()
}

pub fn pick_style(_: &iced::Theme, status: pick_list::Status) -> pick_list::Style {
    let line = match status {
        pick_list::Status::Opened { .. } => theme::TEAL,
        _ => theme::LINE,
    };
    pick_list::Style {
        text_color: theme::TEXT,
        placeholder_color: theme::TEXT_MUTED,
        handle_color: theme::TEXT_MUTED,
        background: theme::SURFACE.into(),
        border: Border {
            color: line,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
    }
}

pub fn menu_style(_: &iced::Theme) -> iced::overlay::menu::Style {
    iced::overlay::menu::Style {
        background: theme::SURFACE.into(),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        text_color: theme::TEXT,
        selected_text_color: theme::TEXT,
        selected_background: theme::LANE.into(),
        shadow: iced::Shadow::default(),
    }
}
