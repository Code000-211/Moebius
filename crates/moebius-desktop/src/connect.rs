//! The connect screen, for when there are no settings yet or the connection fails, and the Settings page, which shows
//! the connection.

use iced::font::Weight;
use iced::widget::{column, container, row, text, text_input};
use iced::{Border, Center, Element, Length};

use crate::app::{Form, Message};
use crate::settings::{self, Settings};
use crate::theme;
use crate::{icons, page};

pub fn input_style(_: &iced::Theme, status: text_input::Status) -> text_input::Style {
    let border = match status {
        text_input::Status::Focused { .. } => theme::TEAL,
        _ => theme::LINE,
    };
    text_input::Style {
        background: theme::SURFACE.into(),
        border: Border {
            color: border,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        icon: theme::TEXT_MUTED,
        placeholder: theme::TEXT_MUTED,
        value: theme::TEXT,
        selection: theme::TEAL_FILL,
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
            "Saved token {} (leave empty to keep it)",
            settings::mask(&saved.token)
        ),
        None => "The content of the server's token file".to_string(),
    };
    let ready = !form.connecting;
    let mut fields = column![
        label("Address"),
        text_input("http://127.0.0.1:7171", &form.address)
            .on_input_maybe(ready.then_some(Message::AddressChanged))
            .on_submit(Message::Submit)
            .padding([8, 10])
            .size(theme::BODY)
            .style(input_style),
        label("Token"),
        text_input(&token_hint, &form.token)
            .on_input_maybe(ready.then_some(Message::TokenChanged))
            .on_submit(Message::Submit)
            .secure(true)
            .padding([8, 10])
            .size(theme::BODY)
            .style(input_style),
    ]
    .spacing(6);
    if let Some(error) = &form.error {
        fields = fields.push(
            text(error)
                .size(theme::CONTEXT)
                .font(theme::manrope(Weight::Semibold))
                .color(theme::AMBER),
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
            .spacing(9)
            .align_y(Center),
            text("Connect to a server")
                .size(theme::PAGE_TITLE)
                .font(theme::manrope(Weight::Bold)),
            page::context("The address of the Moebius server and its token."),
            fields,
            page::primary_button(button_label, ready.then_some(Message::Submit)),
        ]
        .spacing(14),
    )
    .padding(28)
    .width(Length::Fixed(440.0))
    .style(|_| container::Style {
        background: Some(theme::SURFACE.into()),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_BOX.into(),
        },
        ..container::Style::default()
    });
    container(card).center(Length::Fill).into()
}

/// The Settings page: where the app is connected, with the token only in part.
pub fn settings_page<'a>(saved: Option<&Settings>) -> Element<'a, Message> {
    let (address, token) = match saved {
        Some(saved) => (saved.address.clone(), settings::mask(&saved.token)),
        None => ("none".to_string(), "none".to_string()),
    };
    column![
        page::header(
            "Setup",
            "Settings",
            None,
            page::context("How this app reaches the server."),
            iced::widget::text("").into(),
        ),
        page::boxed(
            "Connection",
            column![
                page::body(format!("Address {address}")),
                page::body(format!("Token {token}")),
                page::primary_button("Change connection", Some(Message::ChangeConnection)),
            ]
            .spacing(10),
        ),
    ]
    .spacing(theme::PAGE_GAP)
    .into()
}
