//! The Model fields of the Models and Agents screens: a list of the models the upstreams serve (`GET
//! /v1/models/available`) that can still be typed into, so that a model no upstream lists can be entered. The list
//! shows each model's id.

use iced::font::Weight;
use iced::widget::{button, column, combo_box, row, space, text};
use iced::{Border, Element, Length};

use crate::agents::menu_style;
use crate::client::{Available, AvailableModel};
use crate::connect;
use crate::theme;

/// The list to pick from, with nothing typed.
pub fn state(available: Option<&Available>) -> combo_box::State<AvailableModel> {
    combo_box::State::new(available.map(|a| a.models.clone()).unwrap_or_default())
}

/// The field. `current` is what the model is now, listed or not.
pub fn combo<'a, Message: Clone + 'a>(
    state: &'a combo_box::State<AvailableModel>,
    placeholder: &str,
    current: &str,
    on_input: impl Fn(String) -> Message + 'static,
    on_pick: impl Fn(AvailableModel) -> Message + 'static,
) -> Element<'a, Message> {
    let selected = (!current.is_empty()).then(|| AvailableModel {
        id: current.to_string(),
        protocol: String::new(),
        upstream: String::new(),
    });
    combo_box(state, placeholder, selected.as_ref(), on_pick)
        .on_input(on_input)
        .font(theme::MANROPE)
        .size(theme::BODY)
        .padding([9, 12])
        .width(Length::Fill)
        .input_style(connect::input_style)
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
