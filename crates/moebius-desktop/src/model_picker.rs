//! The Model fields of the Models and Agents screens: a dropdown of the models the upstreams serve (`GET
//! /v1/models/available`), whose last entry, "Another model…", turns the field into an input, so that a model no
//! upstream lists can be entered. Without a list the field is an input.

use std::fmt;

use iced::font::Weight;
use iced::widget::{button, column, pick_list, row, space, text, text_input};
use iced::{Border, Element, Length};

use crate::agents::{menu_style, pick_style};
use crate::client::{Available, AvailableModel};
use crate::connect;
use crate::theme;

/// An entry of the dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Model(AvailableModel),
    /// Type a model the list does not have.
    Other,
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Choice::Model(model) => f.write_str(&model.id),
            Choice::Other => f.write_str("Another model…"),
        }
    }
}

/// The field. `current` is what the model is now, listed or not; `typing` is whether the person chose to type one.
pub fn field<'a, Message: Clone + 'a>(
    models: &[AvailableModel],
    typing: bool,
    placeholder: &str,
    current: &str,
    on_input: impl Fn(String) -> Message + 'a,
    on_pick: impl Fn(AvailableModel) -> Message + 'a,
    on_type: Message,
) -> Element<'a, Message> {
    if typing || models.is_empty() {
        return text_input(placeholder, current)
            .on_input(on_input)
            .font(theme::MANROPE)
            .size(theme::BODY)
            .padding([9, 12])
            .width(Length::Fill)
            .style(connect::input_style)
            .into();
    }
    let selected = (!current.is_empty()).then(|| {
        Choice::Model(
            models
                .iter()
                .find(|model| model.id == current)
                .cloned()
                .unwrap_or_else(|| AvailableModel {
                    id: current.to_string(),
                    protocol: String::new(),
                    upstream: String::new(),
                }),
        )
    });
    let choices: Vec<Choice> = models
        .iter()
        .cloned()
        .map(Choice::Model)
        .chain([Choice::Other])
        .collect();
    pick_list(choices, selected, move |choice| match choice {
        Choice::Model(model) => on_pick(model),
        Choice::Other => on_type.clone(),
    })
    .placeholder(placeholder.to_string())
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style)
    .into()
}

/// What goes under a Model field: the upstreams that could not be asked, once each, and the link that asks again.
pub fn note<'a, Message: Clone + 'a>(
    available: &Available,
    refresh: Message,
) -> Element<'a, Message> {
    let muted = |line: String| {
        text(line)
            .size(theme::LABEL)
            .font(theme::manrope(Weight::Medium))
            .color(theme::TEXT_MUTED)
    };
    let refresh = button(
        text("Refresh")
            .size(theme::LABEL)
            .font(theme::manrope(Weight::Semibold)),
    )
    .padding(0)
    .on_press(refresh)
    .style(|_, _| button::Style {
        text_color: theme::TEAL,
        border: Border::default(),
        ..button::Style::default()
    });
    row![
        column(
            available.errors.iter().map(|e| muted(format!(
                "Could not ask {}: {}",
                e.upstream, e.message
            ))
            .into())
        )
        .spacing(2),
        space::horizontal(),
        refresh,
    ]
    .into()
}
