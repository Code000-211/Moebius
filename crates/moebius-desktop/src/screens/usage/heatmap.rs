//! The heatmap of the Usage page, the last year a cell a day, and the colour each model keeps on every chart of the
//! page.

use std::collections::HashMap;

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Border, Color, Element, Length, Padding};

use crate::app::Message as AppMessage;
use crate::client::{UsageDay, UsageReport};
use crate::theme;
use crate::widgets::page;

use super::figures::{MONTHS, WEEKDAYS, day_number, month_and_day, short, weekday};
use super::table::panel;

/// The weeks of the heatmap: a year and the week it starts in.
const WEEKS: i64 = 53;
/// The height of a heatmap cell, and the gap around it.
const CELL: f32 = 16.0;
const CELL_GAP: f32 = 2.0;
/// The width of the column that names the days of the week.
const WEEKDAY_LABELS: f32 = 40.0;

/// The colour of each model: the places of the models in the order in which they appear in the heatmap, then in the
/// table, so that a model keeps its colour on every chart.
pub struct Colours<'a>(Vec<&'a str>);

impl<'a> Colours<'a> {
    pub fn of(report: &'a UsageReport) -> Self {
        let mut order: Vec<&str> = vec![];
        let names = report
            .days
            .iter()
            .map(|day| day.model.as_str())
            .chain(report.models.iter().map(|m| m.model.as_str()));
        for name in names {
            if !order.contains(&name) {
                order.push(name);
            }
        }
        Colours(order)
    }

    pub fn of_model(&self, model: &str) -> Color {
        theme::model_colour(self.0.iter().position(|m| *m == model).unwrap_or(0))
    }

    /// The models that led on a day of the heatmap, which its legend names.
    fn leading(&self, report: &UsageReport) -> Vec<&'a str> {
        self.0
            .iter()
            .copied()
            .filter(|model| report.days.iter().any(|day| day.model == *model))
            .collect()
    }
}

/// A square of `fill` with round corners; none leaves it empty.
fn square<'a>(
    width: impl Into<Length>,
    height: f32,
    fill: Option<Color>,
) -> container::Container<'a, AppMessage> {
    container(space())
        .width(width)
        .height(height)
        .style(move |_| container::Style {
            background: fill.map(Into::into),
            border: Border {
                radius: 3.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
}

/// The heatmap of the last 53 weeks: a column a week, Sunday at the top, a cell a day, in the colour of the model that
/// led the day and in one of four shades by the tokens of the day against the busiest day.
pub fn heatmap<'a>(report: &'a UsageReport, colours: &Colours<'a>) -> Element<'a, AppMessage> {
    let Some(today) = day_number(&report.today) else {
        return panel("Tokens per day", None, page::body("The date is not known."));
    };
    let first = today - weekday(today) as i64 - (WEEKS - 1) * 7;
    let by_day: HashMap<i64, &UsageDay> = report
        .days
        .iter()
        .filter_map(|day| Some((day_number(&day.date)?, day)))
        .collect();
    let busiest = report.days.iter().max_by_key(|day| day.tokens);
    let most = busiest.map_or(1, |day| day.tokens.max(1));

    let cell = |number: i64| -> Element<'a, AppMessage> {
        let fill = if number > today {
            None
        } else {
            Some(match by_day.get(&number) {
                Some(day) if day.tokens > 0 => {
                    let level = ((day.tokens * 4 + most - 1) / most).clamp(1, 4) as usize;
                    theme::shade(colours.of_model(&day.model), level)
                }
                _ => theme::HEAT_EMPTY,
            })
        };
        container(square(Length::Fill, CELL, fill))
            .padding(CELL_GAP)
            .width(Length::Fill)
            .into()
    };
    let weekday_label = |row_of_week: usize| -> Element<'a, AppMessage> {
        let name = if row_of_week % 2 == 1 {
            WEEKDAYS[row_of_week]
        } else {
            ""
        };
        container(text(name).size(13).color(theme::TEXT_SECONDARY))
            .width(WEEKDAY_LABELS)
            .align_y(Alignment::Center)
            .height(CELL + 2.0 * CELL_GAP)
            .into()
    };
    let weeks = (0..7).map(|day_of_week| {
        let cells = (0..WEEKS).map(|week| cell(first + week * 7 + day_of_week as i64));
        row![weekday_label(day_of_week)]
            .extend(cells)
            .align_y(Alignment::Center)
            .into()
    });

    // A month's name stands over the weeks that start in it, as far as there is room for the name.
    let mut months: Vec<(usize, u16)> = vec![];
    for week in 0..WEEKS {
        let (month, _) = month_and_day(first + week * 7);
        match months.last_mut() {
            Some((last, weeks)) if *last == month => *weeks += 1,
            _ => months.push((month, 1)),
        }
    }
    let month_names = months.into_iter().map(|(month, weeks)| {
        let name = if weeks >= 2 { MONTHS[month] } else { "" };
        container(
            text(name)
                .size(13)
                .wrapping(Wrapping::None)
                .color(theme::TEXT_SECONDARY),
        )
        .width(Length::FillPortion(weeks))
        .padding(Padding::default().left(CELL_GAP))
        .into()
    });
    let months_row = row![space().width(WEEKDAY_LABELS)].extend(month_names);

    let legend = row(colours.leading(report).into_iter().map(|model| {
        let swatches = row((1..=4).map(|level| {
            square(
                14.0,
                14.0,
                Some(theme::shade(colours.of_model(model), level)),
            )
            .into()
        }))
        .spacing(2);
        row![
            swatches,
            text(model.to_string())
                .size(theme::CONTEXT)
                .color(theme::TEXT_SECONDARY)
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }))
    .spacing(26)
    .push(
        text("Lighter means fewer tokens, darker means more")
            .size(theme::CONTEXT)
            .color(theme::TEXT_MUTED),
    )
    .padding(Padding::default().left(WEEKDAY_LABELS))
    .align_y(Alignment::Center);

    let aside = match busiest {
        Some(day) => {
            let total: i64 = report.days.iter().map(|day| day.tokens).sum();
            let number = day_number(&day.date).unwrap_or(today);
            let (month, of_month) = month_and_day(number);
            format!(
                "{} tokens on {} days in the last year · busiest day {} {} {} with {}",
                short(total),
                report.days.len(),
                WEEKDAYS[weekday(number)],
                of_month,
                MONTHS[month],
                short(day.tokens)
            )
        }
        None => "No tokens in the last year".to_string(),
    };
    panel(
        "Tokens per day",
        Some(aside),
        column![months_row, column(weeks).spacing(0), legend]
            .spacing(6)
            .into(),
    )
}
