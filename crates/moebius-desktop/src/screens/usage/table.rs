//! The pieces the Usage page's boxes are made of: the white panel with its heading and the line that sums it up, the
//! tables of models, agents and tool waits, and the corners of a bar.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Border, Color, Element, Length};

use crate::app::Message as AppMessage;
use crate::theme;
use crate::widgets::form::{aside, box_heading};
use crate::widgets::page;

/// The radii of the corners of a bar, clockwise from the top left.
pub fn corners([top_left, top_right, bottom_right, bottom_left]: [f32; 4]) -> iced::border::Radius {
    iced::border::Radius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    }
}

/// A white box with a heading and, at the right of it, a line that sums it up.
pub fn panel<'a>(
    heading: &'a str,
    aside: Option<String>,
    content: Element<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    let line = aside.map(self::aside);
    container(column![box_heading(heading, line), content].spacing(theme::PAGE_GAP))
        .padding(theme::BOX_PADDING)
        .width(Length::Fill)
        .style(|_| page::box_style())
        .into()
}

/// One cell of a table.
pub struct Cell {
    text: String,
    /// The square of a model's colour before the text.
    lead: Option<Color>,
    code: bool,
}

impl Cell {
    pub fn plain(text: String) -> Self {
        Cell {
            text,
            lead: None,
            code: false,
        }
    }

    /// A name, with the colour of the model it names if it names one.
    pub fn named(name: &str, lead: Option<Color>) -> Self {
        Cell {
            text: name.to_string(),
            lead,
            code: false,
        }
    }

    pub fn code(text: &str) -> Self {
        Cell {
            text: text.to_string(),
            lead: None,
            code: true,
        }
    }
}

/// A table: bold heads over a line, then a row for each of `rows` with a thin line under it. The first column is
/// left-aligned, the others right-aligned like numbers.
pub fn table<'a>(columns: &[(&'a str, Length)], rows: Vec<Vec<Cell>>) -> Element<'a, AppMessage> {
    let head = row(columns.iter().enumerate().map(|(i, (name, width))| {
        let label = text(*name)
            .size(theme::LABEL)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::TEXT_MUTED);
        aligned(label.into(), *width, i == 0)
    }))
    .padding(theme::TILE_PADDING);
    let line = |color: Color| {
        container(space())
            .width(Length::Fill)
            .height(theme::HAIRLINE)
            .style(move |_| container::Style {
                background: Some(color.into()),
                ..container::Style::default()
            })
    };
    let mut table = column![head, line(theme::LINE)];
    for cells in rows {
        let cells = cells.into_iter().enumerate().map(|(i, cell)| {
            let width = columns[i].1;
            let mut label = text(cell.text).size(theme::BODY).wrapping(Wrapping::None);
            if cell.code {
                label = label.font(theme::MONO).size(theme::CODE);
            }
            let content: Element<'a, AppMessage> = match cell.lead {
                Some(colour) => row![
                    container(space())
                        .width(theme::SWATCH)
                        .height(theme::SWATCH)
                        .style(move |_| container::Style {
                            background: Some(colour.into()),
                            border: Border {
                                radius: theme::RADIUS_SEGMENT.into(),
                                ..Border::default()
                            },
                            ..container::Style::default()
                        }),
                    label
                ]
                .spacing(theme::SPACE_8)
                .align_y(Alignment::Center)
                .into(),
                None => label.into(),
            };
            aligned(content, width, i == 0)
        });
        table = table
            .push(row(cells).padding(theme::FIELD_PADDING))
            .push(line(theme::TRACK));
    }
    table.into()
}

/// A table cell of a given width, aligned left or right.
fn aligned<'a>(
    content: Element<'a, AppMessage>,
    width: Length,
    left: bool,
) -> Element<'a, AppMessage> {
    container(content)
        .width(width)
        .align_x(if left {
            Alignment::Start
        } else {
            Alignment::End
        })
        .into()
}
