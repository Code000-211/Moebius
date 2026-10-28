//! Editing in place: the pencil that stands next to what a person or the Moebius agent wrote, and the editor it opens
//! where that text was, with Save and Cancel under it and the server's refusal in red. Every screen that edits text in
//! place draws it with these, so that it looks and works the same everywhere (`docs/design.md`, "Editing in place").

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text, text_editor};
use iced::{Border, Element};

use crate::icons::Icon;
use crate::{page, theme};

/// The pencil that opens the editor, muted until hovered. `id` names it, so that a test can find and press it.
pub fn pencil<'a, Message: Clone + 'a>(id: String, message: Message) -> Element<'a, Message> {
    container(
        button(Icon::Pencil.view(theme::TEXT_MUTED, 13.0))
            .padding(2)
            .on_press(message)
            .style(|_, status| button::Style {
                background: (status == button::Status::Hovered).then(|| theme::SURFACE.into()),
                border: Border {
                    radius: theme::RADIUS_SMALL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    )
    .id(id)
    .into()
}

/// The editor in place: the `fields` being edited, then Save (grey and inert without `save`, as while the request is
/// on its way or nothing can be saved yet) and Cancel, and under them why the server refused the last save.
pub fn editor<'a, Message: Clone + 'a>(
    fields: Element<'a, Message>,
    save: Option<Message>,
    cancel: Message,
    error: Option<&'a str>,
) -> Element<'a, Message> {
    editor_with(fields, save, cancel, None, error)
}

/// Like `editor`, with `more` at the right end of the row of Save and Cancel, such as a Delete that asks first.
pub fn editor_with<'a, Message: Clone + 'a>(
    fields: Element<'a, Message>,
    save: Option<Message>,
    cancel: Message,
    more: Option<Element<'a, Message>>,
    error: Option<&'a str>,
) -> Element<'a, Message> {
    let mut buttons = row![
        page::primary_button("Save", save),
        page::white_button("Cancel", Some(cancel)),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if let Some(more) = more {
        buttons = buttons.push(iced::widget::space::horizontal()).push(more);
    }
    let mut content = column![fields, buttons].spacing(10);
    if let Some(error) = error {
        content = content.push(
            text(error)
                .size(theme::CONTEXT)
                .font(theme::manrope(Weight::Semibold))
                .color(theme::RED_TEXT)
                .wrapping(Wrapping::WordOrGlyph),
        );
    }
    content.into()
}

/// The style of a text editor in place: the lane's grey, teal while it has focus.
pub fn editor_style(_: &iced::Theme, status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: theme::LANE.into(),
        border: Border {
            color: match status {
                text_editor::Status::Focused { .. } => theme::TEAL,
                _ => theme::LINE,
            },
            width: 1.0,
            radius: theme::RADIUS_CARD.into(),
        },
        placeholder: theme::TEXT_MUTED,
        value: theme::TEXT,
        selection: theme::TEAL_FILL,
    }
}
