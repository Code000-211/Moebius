//! The inputs of the forms: a key of the configuration as the input its kind asks for, an upstream's row, where an
//! API key comes from, and the box of a part that may be left out.

use iced::font::Weight;
use iced::widget::{column, container, pick_list, row, space, text, text_input};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message as AppMessage;
use crate::screens::connect;
use crate::theme;
use crate::widgets::form::{PROTOCOLS, labelled, menu_style, pick_style};
use crate::widgets::model_picker;
use crate::widgets::page;

use super::KeyMode;
use super::fields::{Field, Kind};
use super::forms::{KeyChoice, Optional, Upstream};
use super::{Message, Owner};

/// A box that holds the keys of a table, which can be turned off.
pub(super) fn optional_box<'a>(
    heading: &'a str,
    about: &'a str,
    off: &'a str,
    optional: &'a Optional,
    toggle: Message,
    key: Option<(Owner, &'a [String])>,
    models_note: Option<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    let toggle = AppMessage::Setup(toggle);
    if !optional.on {
        let line = if optional.was_on {
            "Turned off; Save takes it out of the file."
        } else {
            off
        };
        return page::boxed(
            heading,
            column![
                page::body(line),
                page::white_button("Turn on", Some(toggle))
            ]
            .spacing(12),
        );
    }
    let mut models_note = models_note;
    let mut fields = column(optional.form.fields.iter().map(|field| {
        let note = if field.kind == Kind::Model {
            models_note.take()
        } else {
            None
        };
        field_view(field, note)
    }))
    .spacing(12);
    if let (Some((owner, secrets)), Some((now, _))) = (key, &optional.key) {
        fields = fields.push(key_view(owner, now, secrets));
    }
    page::boxed(
        heading,
        column![
            page::body(about),
            fields,
            page::white_button("Turn off", Some(toggle)),
        ]
        .spacing(12),
    )
}

/// An input with its label and, when the file does not set the key, its default.
pub(super) fn field_view<'a>(
    field: &'a Field,
    note: Option<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    let key = field.key;
    let on_input = move |text| AppMessage::Setup(Message::Edit(key, text));
    let input: Element<AppMessage> = match field.kind {
        Kind::Pick(options) => {
            let options: Vec<String> = options.iter().map(|o| o.to_string()).collect();
            pick_list(options, Some(field.text.clone()), on_input)
                .text_size(theme::BODY)
                .padding([9, 12])
                .width(Length::Fill)
                .style(pick_style)
                .menu_style(menu_style)
                .into()
        }
        Kind::Model => model_picker::field(
            &field.models,
            field.typing,
            field.default.as_deref().unwrap_or(""),
            &field.text,
            on_input,
            move |model| AppMessage::Setup(Message::PickModel(key, model)),
            AppMessage::Setup(Message::TypeModel(key)),
        ),
        kind => text_input(field.default.as_deref().unwrap_or(""), &field.text)
            .on_input(on_input)
            .font(if kind == Kind::Code {
                theme::MONO
            } else {
                theme::MANROPE
            })
            .size(if kind == Kind::Code {
                theme::CODE
            } else {
                theme::BODY
            })
            .padding([9, 12])
            .style(connect::input_style)
            .into(),
    };
    let mut head = row![
        text(field.label)
            .size(13)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::TEXT_SECONDARY),
        space::horizontal(),
    ]
    .align_y(Alignment::Center);
    if let Some(default) = &field.default
        && field.text.trim().is_empty()
    {
        head = head.push(
            text(format!("Default {default}"))
                .size(theme::LABEL)
                .color(theme::TEXT_MUTED),
        );
    }
    let mut shown = column![head, input].spacing(5);
    if let Some(note) = note {
        shown = shown.push(note);
    }
    shown.into()
}

/// One upstream as a row of inputs.
pub(super) fn upstream_row<'a>(
    place: usize,
    upstream: &'a Upstream,
    secrets: &'a [String],
) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Setup(message);
    let protocols: Vec<String> = PROTOCOLS.iter().map(|p| p.to_string()).collect();
    let protocol = pick_list(protocols, Some(upstream.protocol.clone()), move |p| {
        message(Message::UpstreamProtocol(place, p))
    })
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let url = text_input("http://127.0.0.1:8080", &upstream.base_url)
        .on_input(move |url| message(Message::UpstreamUrl(place, url)))
        .font(theme::MONO)
        .size(theme::CODE)
        .padding([9, 12])
        .style(connect::input_style);
    row![
        container(labelled("Protocol", protocol)).width(Length::FillPortion(2)),
        container(labelled("Base URL", url)).width(Length::FillPortion(6)),
        container(key_view(Owner::Upstream(place), &upstream.key, secrets))
            .width(Length::FillPortion(7)),
        container(page::white_button(
            "Remove",
            Some(message(Message::RemoveUpstream(place)))
        ))
        .padding(Padding::default().top(21)),
    ]
    .spacing(12)
    .align_y(Alignment::Start)
    .into()
}

/// The inputs for the key of an upstream or of the decision model: where it comes from, and the variable or the key.
fn key_view<'a>(
    owner: Owner,
    key: &'a KeyChoice,
    secrets: &'a [String],
) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Setup(message);
    let modes: Vec<String> = KeyMode::ALL.iter().map(|m| m.label().to_string()).collect();
    let chosen = key.mode;
    let mode = pick_list(
        modes,
        Some(chosen.label().to_string()),
        move |label: String| {
            let mode = KeyMode::ALL
                .into_iter()
                .find(|m| m.label() == label)
                .unwrap_or(KeyMode::None);
            message(Message::KeyMode(owner, mode))
        },
    )
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let detail: Element<AppMessage> = match key.mode {
        KeyMode::None => space().width(Length::Fill).into(),
        KeyMode::Environment => text_input("ANTHROPIC_API_KEY", &key.env)
            .on_input(move |env| message(Message::KeyEnv(owner, env)))
            .font(theme::MONO)
            .size(theme::CODE)
            .padding([9, 12])
            .style(connect::input_style)
            .into(),
        KeyMode::Typed => {
            let stored = key
                .stored
                .as_ref()
                .is_some_and(|name| secrets.contains(name));
            text_input(
                if stored {
                    "Stored; type to replace it"
                } else {
                    "Paste the key"
                },
                &key.typed,
            )
            .on_input(move |typed| message(Message::KeyTyped(owner, typed)))
            .secure(true)
            .size(theme::BODY)
            .padding([9, 12])
            .style(connect::input_style)
            .into()
        }
    };
    row![
        container(labelled("Key", mode)).width(Length::FillPortion(1)),
        container(labelled(" ", detail)).width(Length::FillPortion(1)),
    ]
    .spacing(12)
    .into()
}
