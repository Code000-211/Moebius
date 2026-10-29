//! How a research is shown: its card in the list, its brief with the buttons its status allows, its baseline and the
//! table of its experiments, with the numbers and changes as the page tells them.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::client::{Experiment, Research};
use crate::theme;
use crate::widgets::inline_edit;
use crate::widgets::page::{self, Tone};

use super::Message;

/// A research's status as a tag.
fn status_tag<'a>(status: &str) -> Element<'a, AppMessage> {
    let (label, tone) = match status {
        "confirmed" => ("Confirmed", Tone::Amber),
        "running" => ("Running", Tone::Teal),
        "done" => ("Done", Tone::Teal),
        "spent" => ("Spent", Tone::Plain),
        "stopped" => ("Stopped", Tone::Plain),
        "failed" => ("Failed", Tone::Red),
        _ => ("Draft", Tone::Plain),
    };
    page::tag(label.to_string(), tone)
}

/// A number as the page shows it: no more than four decimals, none when it is whole.
pub fn number(value: f64) -> String {
    let shown = format!("{value:.4}");
    shown
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

/// How far `value` is from `baseline`, in percent of the baseline: `-20 %`.
pub fn change(baseline: f64, value: f64) -> Option<String> {
    (baseline != 0.0).then(|| {
        let percent = (value - baseline) / baseline.abs() * 100.0;
        format!("{} %", format!("{percent:.1}").trim_end_matches(".0"))
    })
}

pub fn research_card<'a>(
    research: &'a Research,
    repository: &'a str,
    chosen: bool,
) -> Element<'a, AppMessage> {
    let head = row![
        text(repository)
            .size(theme::CARD_TITLE)
            .font(theme::manrope(Weight::Semibold)),
        space::horizontal(),
        status_tag(&research.status),
    ]
    .align_y(Alignment::Center);
    let kept = research.experiments.iter().filter(|e| e.kept).count();
    let counts = format!(
        "{} · {} kept",
        plural(research.experiments.len(), "experiment", "experiments"),
        kept
    );
    let content = column![
        head,
        text(research.goal.as_str())
            .size(theme::BODY)
            .color(theme::TEXT_SECONDARY)
            .wrapping(Wrapping::WordOrGlyph),
        text(counts).size(theme::LABEL).color(theme::TEXT_MUTED),
    ]
    .spacing(4);
    button(content)
        .width(Length::Fill)
        .padding([12, 14])
        .on_press(AppMessage::Experiments(Message::Select(research.id)))
        .style(move |_, status| {
            let (fill, line) = match (chosen, status) {
                (true, _) => (theme::SURFACE, theme::TEAL),
                (false, button::Status::Hovered) => (theme::LANE, theme::LINE),
                (false, _) => (theme::SURFACE, theme::LINE),
            };
            button::Style {
                background: Some(fill.into()),
                text_color: theme::TEXT,
                border: Border {
                    color: line,
                    width: if chosen { 2.0 } else { 1.0 },
                    radius: theme::RADIUS_CARD.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The brief of the research chosen, with the buttons its status allows.
pub fn brief_box<'a>(research: &'a Research, repository: &'a str) -> Element<'a, AppMessage> {
    let act = |label: &'static str, action: &'static str, primary: bool| {
        let message = Some(AppMessage::Experiments(Message::Act(research.id, action)));
        if primary {
            page::primary_button(label, message)
        } else {
            page::white_button(label, message)
        }
    };
    let mut buttons = row![].spacing(8);
    match research.status.as_str() {
        "draft" => {
            buttons = buttons
                .push(act("Confirm", "confirm", true))
                .push(act("Drop", "stop", false));
        }
        "confirmed" => {
            buttons = buttons
                .push(act("Start now", "start", true))
                .push(act("Drop", "stop", false));
        }
        "running" => buttons = buttons.push(act("Stop", "stop", false)),
        _ => {}
    }
    let direction = match research.better.as_str() {
        "higher" => "Higher is better",
        _ => "Lower is better",
    };
    let budget = format!(
        "{direction} · {} of {} tokens used",
        research.tokens, research.token_budget
    );
    let mut content = column![
        {
            let mut title = row![
                text(research.goal.as_str())
                    .size(theme::CARD_TITLE)
                    .font(theme::manrope(Weight::Semibold))
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            if research.status == "draft" {
                title = title.push(inline_edit::pencil(
                    "edit-brief".to_string(),
                    AppMessage::Experiments(Message::EditBrief(research.id)),
                ));
            }
            title.push(status_tag(&research.status))
        },
        text(research.measure.as_str())
            .size(theme::CODE)
            .font(theme::MONO)
            .color(theme::TEXT)
            .wrapping(Wrapping::WordOrGlyph),
        text(format!("{repository} · {budget}"))
            .size(theme::LABEL)
            .color(theme::TEXT_MUTED),
    ]
    .spacing(6);
    if !research.reason.is_empty() {
        content = content.push(
            text(research.reason.as_str())
                .size(theme::BODY)
                .color(theme::TEXT_SECONDARY)
                .wrapping(Wrapping::WordOrGlyph),
        );
    }
    content = content.push(buttons);
    page::boxed("Brief", content)
}

pub fn baseline_box(research: &Research) -> Element<'_, AppMessage> {
    let line = match research.baseline {
        Some(baseline) => text(number(baseline))
            .size(theme::TILE_VALUE)
            .font(theme::manrope(Weight::Bold))
            .into(),
        None => page::body("Not measured yet."),
    };
    page::boxed("Baseline", line)
}

/// The widths of the columns of the experiments table.
const COLUMNS: [u16; 6] = [5, 5, 2, 2, 2, 7];

fn cell<'a>(portion: u16, content: impl Into<Element<'a, AppMessage>>) -> Element<'a, AppMessage> {
    container(content.into())
        .width(Length::FillPortion(portion))
        .into()
}

fn head_cell(portion: u16, label: &'static str) -> Element<'static, AppMessage> {
    cell(
        portion,
        text(label)
            .size(theme::LABEL)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::TEXT_MUTED),
    )
}

pub fn experiments_box(research: &Research) -> Element<'_, AppMessage> {
    if research.experiments.is_empty() {
        return page::boxed("Experiments", page::body("No lever has been tried yet."));
    }
    let mut table = column![
        row![
            head_cell(COLUMNS[0], "Lever"),
            head_cell(COLUMNS[1], "Branch"),
            head_cell(COLUMNS[2], "Value"),
            head_cell(COLUMNS[3], "Change"),
            head_cell(COLUMNS[4], "Result"),
            head_cell(COLUMNS[5], "Reason"),
        ]
        .spacing(10),
        page::rule(),
    ]
    .spacing(8);
    for experiment in &research.experiments {
        table = table
            .push(experiment_row(experiment, research.baseline))
            .push(page::rule());
    }
    page::boxed("Experiments", table)
}

fn experiment_row(experiment: &Experiment, baseline: Option<f64>) -> Element<'_, AppMessage> {
    let value = experiment.value.map_or("—".to_string(), number);
    let changed = experiment
        .value
        .zip(baseline)
        .and_then(|(value, baseline)| change(baseline, value))
        .unwrap_or_else(|| "—".to_string());
    let (label, tone) = match (experiment.kept, experiment.value) {
        (true, _) => ("Kept", Tone::Teal),
        (false, None) => ("Failed", Tone::Red),
        (false, Some(_)) => ("Not kept", Tone::Plain),
    };
    let plain = |line: String| {
        text(line)
            .size(theme::BODY)
            .color(theme::TEXT_SECONDARY)
            .wrapping(Wrapping::WordOrGlyph)
    };
    row![
        cell(
            COLUMNS[0],
            text(experiment.title.as_str())
                .size(theme::BODY)
                .font(theme::manrope(Weight::Semibold))
                .wrapping(Wrapping::WordOrGlyph)
        ),
        cell(
            COLUMNS[1],
            text(experiment.branch.as_str())
                .size(theme::CODE_SMALL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED)
                .wrapping(Wrapping::WordOrGlyph)
        ),
        cell(COLUMNS[2], plain(value)),
        cell(COLUMNS[3], plain(changed)),
        cell(COLUMNS[4], page::tag(label.to_string(), tone)),
        cell(COLUMNS[5], plain(experiment.reason.clone())),
    ]
    .spacing(10)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_shown_without_trailing_zeros() {
        assert_eq!(number(80.0), "80");
        assert_eq!(number(0.25), "0.25");
        assert_eq!(number(-3.5), "-3.5");
        assert_eq!(number(1.0 / 3.0), "0.3333");
    }

    #[test]
    fn a_change_is_a_percent_of_the_baseline() {
        assert_eq!(change(100.0, 80.0).as_deref(), Some("-20 %"));
        assert_eq!(change(100.0, 120.0).as_deref(), Some("20 %"));
        assert_eq!(change(372.0, 301.0).as_deref(), Some("-19.1 %"));
        assert_eq!(change(0.0, 1.0), None);
    }
}
