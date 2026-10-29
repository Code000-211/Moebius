//! The connect screen, for when there are no settings yet or the connection fails.

use iced::font::Weight;
use iced::widget::{column, container, row, text, text_input};
use iced::{Border, Center, Element, Length};

use crate::app::{Form, Message};
use crate::settings::{self, Settings};
use crate::theme;
use crate::{widgets::icons, widgets::page};

/// The style of a text input: a white field whose line darkens when hovered and turns dark when focused.
pub fn input_style(_: &iced::Theme, status: text_input::Status) -> text_input::Style {
    let line = match status {
        text_input::Status::Focused { .. } => theme::TEXT,
        text_input::Status::Hovered => theme::TEXT_MUTED,
        _ => theme::LINE,
    };
    text_input::Style {
        background: theme::SURFACE.into(),
        border: Border {
            color: line,
            width: theme::HAIRLINE,
            radius: theme::RADIUS_CONTROL.into(),
        },
        icon: theme::TEXT_MUTED,
        placeholder: theme::TEXT_MUTED,
        value: theme::TEXT,
        selection: theme::HEAD_PILL,
    }
}

fn label(name: &str) -> Element<'_, Message> {
    text(name)
        .size(theme::LABEL)
        .font(theme::manrope(Weight::Semibold))
        .color(theme::TEXT_SECONDARY)
        .into()
}

/// The screen that asks for the server's address and token. `saved` are the settings in use, whose token is shown
/// only in part.
pub fn view<'a>(form: &'a Form, saved: Option<&Settings>) -> Element<'a, Message> {
    let token_hint = match saved {
        Some(saved) => format!(
            "Saved token {} (empty keeps it)",
            settings::mask(&saved.token)
        ),
        None => "Content of the server's token file".to_string(),
    };
    let ready = !form.connecting;
    let mut fields = column![
        label("Address"),
        text_input("http://127.0.0.1:7171", &form.address)
            .on_input_maybe(ready.then_some(Message::AddressChanged))
            .on_submit(Message::Submit)
            .padding(theme::CONTROL_PADDING)
            .size(theme::BODY)
            .style(input_style),
        label("Token"),
        text_input(&token_hint, &form.token)
            .on_input_maybe(ready.then_some(Message::TokenChanged))
            .on_submit(Message::Submit)
            .secure(true)
            .padding(theme::CONTROL_PADDING)
            .size(theme::BODY)
            .style(input_style),
        text("Token file: .moebius/token next to moebius.toml.")
            .size(theme::LABEL)
            .font(theme::MONO)
            .color(theme::TEXT_MUTED),
    ]
    .spacing(theme::SPACE_6);
    if let Some(error) = &form.error {
        fields = fields.push(
            text(error)
                .size(theme::CONTEXT)
                .font(theme::MONO)
                .color(theme::RED_TEXT),
        );
    }
    let button_label = if form.connecting {
        "Connecting…"
    } else {
        "Connect"
    };
    let card = container(
        column![
            row![
                icons::logo(),
                text("Moebius")
                    .size(theme::APP_NAME)
                    .font(theme::manrope(Weight::Bold))
            ]
            .spacing(theme::SPACE_8)
            .align_y(Center),
            text("Connect to a server")
                .size(theme::PAGE_TITLE)
                .font(theme::manrope(Weight::Bold)),
            page::context("The address of the Moebius server and its token."),
            fields,
            page::primary_button(button_label, ready.then_some(Message::Submit)),
        ]
        .spacing(theme::SPACE_12),
    )
    .padding(theme::CONNECT_PADDING)
    .width(theme::CONNECT_WIDTH)
    .style(|_| container::Style {
        background: Some(theme::SURFACE.into()),
        border: Border {
            color: theme::LINE,
            width: theme::HAIRLINE,
            radius: theme::RADIUS_BOX.into(),
        },
        ..container::Style::default()
    });
    container(card).center(Length::Fill).into()
}
