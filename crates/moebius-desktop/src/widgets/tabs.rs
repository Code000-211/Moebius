//! The tabs of a page: a label over a dark line when it is the one shown, and for a tab that can be closed a cross
//! beside the label. The project page's strip is made of them (`screens/project/strip.rs`).

use iced::font::Weight;
use iced::widget::{button, column, container, row, space, text, tooltip};
use iced::{Alignment, Border, Element, Length, Padding};

use crate::theme;
use crate::widgets::icons::Icon;
use crate::widgets::page;

/// The label of a tab: `semibold` text in the page's font, or the code font for a name taken from a file.
pub fn label<'a, M: 'a>(label: String, selected: bool, code: bool) -> Element<'a, M> {
    let ink = if selected {
        theme::TEXT
    } else {
        theme::TEXT_SECONDARY
    };
    if code {
        return text(label)
            .size(theme::BREADCRUMB)
            .font(theme::MONO)
            .color(ink)
            .into();
    }
    text(label)
        .size(theme::CONTEXT)
        .font(theme::manrope(if selected {
            Weight::Bold
        } else {
            Weight::Semibold
        }))
        .color(ink)
        .into()
}

/// A tab: `label` (made by [`label`]) pressed sends `press`; `close` is the id and message of its cross, `mark` an
/// element after the label (a dot for unsaved changes) and `tip` what a hover says (a file's path).
pub struct Tab<'a, M> {
    pub id: String,
    pub label: Element<'a, M>,
    pub selected: bool,
    pub press: M,
    pub close: Option<(String, M)>,
    pub mark: Option<Element<'a, M>>,
    pub tip: Option<String>,
}

impl<'a, M: Clone + 'a> Tab<'a, M> {
    pub fn view(self) -> Element<'a, M> {
        let selected = self.selected;
        let mut label = row![self.label]
            .spacing(theme::SPACE_6)
            .align_y(Alignment::Center);
        if let Some(mark) = self.mark {
            label = label.push(mark);
        }
        let press = container(
            button(label)
                .padding(Padding {
                    top: theme::SPACE_2,
                    bottom: 0.0,
                    left: theme::SPACE_2,
                    right: theme::SPACE_2,
                })
                .on_press(self.press)
                .style(|_, _| button::Style::default()),
        )
        .id(self.id);
        let press: Element<'a, M> = match self.tip {
            Some(tip) => tooltip(
                press,
                container(text(tip).size(theme::LABEL).font(theme::MONO))
                    .padding([theme::SPACE_4, theme::SPACE_8])
                    .style(|_| page::box_style()),
                tooltip::Position::Bottom,
            )
            .into(),
            None => press.into(),
        };
        let mut head = row![press].align_y(Alignment::Center);
        if let Some((id, message)) = self.close {
            head = head.push(
                container(
                    button(Icon::Close.view(theme::TEXT_MUTED, theme::ICON_SMALL))
                        .padding(theme::SPACE_2)
                        .on_press(message)
                        .style(|_, status| button::Style {
                            background: (status == button::Status::Hovered)
                                .then(|| theme::TRACK.into()),
                            border: Border {
                                radius: theme::RADIUS_SMALL.into(),
                                ..Border::default()
                            },
                            ..button::Style::default()
                        }),
                )
                .id(id),
            );
        }
        let underline = container(space())
            .width(Length::Fill)
            .height(theme::SPACE_2)
            .style(move |_| container::Style {
                background: selected.then(|| theme::TEXT.into()),
                ..container::Style::default()
            });
        column![head, underline]
            .spacing(theme::SPACE_6)
            .width(Length::Shrink)
            .into()
    }
}

/// A dot after a label: something in the tab is not saved.
pub fn unsaved_dot<'a, M: 'a>() -> Element<'a, M> {
    container(space().width(theme::DOT).height(theme::DOT))
        .style(|_| container::Style {
            background: Some(theme::AMBER.into()),
            border: Border {
                radius: (theme::DOT / 2.0).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}
