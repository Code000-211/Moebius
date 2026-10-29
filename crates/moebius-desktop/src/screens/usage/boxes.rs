//! The boxes of the Usage page under its window: the tiles of the totals, the tokens by model, where the model time
//! goes, the longest tool waits, where the lens's lint and type checks spent their time and the usage by agent.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::client::UsageReport;
use crate::theme;
use crate::widgets::page;

use super::Window;
use super::figures::{bytes, duration, grouped, percent, share, short};
use super::heatmap::Colours;
use super::table::{Cell, corners, panel, table};

/// The most activities the time box shows before it gathers the rest under "Other".
const ACTIVITY_BARS: usize = 8;

/// What a box says when the window has no model call.
const NO_CALL: &str = "No model call in this window.";

pub fn tiles(report: &UsageReport, window: Window) -> Element<'_, AppMessage> {
    let totals = &report.totals;
    let tokens_note = match (window, totals.day_budget) {
        (Window::Today, Some(budget)) => format!(
            "{} of the {} daily budget",
            share(totals.tokens, budget),
            short(budget)
        ),
        (Window::Today, None) => "Input and output tokens".to_string(),
        (window, _) => format!(
            "{} tokens a day on average",
            short(totals.tokens / window.days())
        ),
    };
    let calls_note = if totals.calls > 0 {
        format!(
            "{} tokens per call on average",
            grouped(totals.tokens / totals.calls)
        )
    } else {
        "No model call yet".to_string()
    };
    row![
        tile("Tokens", short(totals.tokens), tokens_note),
        tile("Model calls", grouped(totals.calls), calls_note),
        tile(
            "From cache",
            percent(totals.cached_share),
            "Input tokens served from cache".to_string(),
        ),
        tile(
            "Model time",
            duration(totals.model_ms / 1000),
            "Time the models spent answering".to_string(),
        ),
    ]
    .spacing(theme::PAGE_GAP)
    .into()
}

fn tile<'a>(label: &'a str, value: String, note: String) -> Element<'a, AppMessage> {
    container(
        column![
            text(label)
                .size(theme::LABEL)
                .font(theme::manrope(Weight::Semibold))
                .color(theme::TEXT_MUTED),
            text(value)
                .size(theme::TILE_VALUE)
                .font(theme::manrope(Weight::Bold)),
            text(note)
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
        ]
        .spacing(theme::SPACE_4),
    )
    .padding(theme::BOX_PADDING)
    .width(Length::FillPortion(1))
    .style(|_| page::box_style())
    .into()
}

/// The stacked bar of the models' shares, and the table of what each used.
pub fn models_box<'a>(
    report: &'a UsageReport,
    colours: &Colours<'a>,
    span: &str,
) -> Element<'a, AppMessage> {
    let total: i64 = report.models.iter().map(|m| m.tokens).sum();
    let last = report.models.len().saturating_sub(1);
    let segments = report.models.iter().enumerate().map(|(i, model)| {
        let colour = colours.of_model(&model.model);
        let radius = match (i == 0, i == last) {
            (true, true) => [theme::RADIUS_SMALL; 4],
            (true, false) => [theme::RADIUS_SMALL, 0.0, 0.0, theme::RADIUS_SMALL],
            (false, true) => [0.0, theme::RADIUS_SMALL, theme::RADIUS_SMALL, 0.0],
            (false, false) => [0.0; 4],
        };
        let portion = (model.tokens.max(0) * 1000 / total.max(1)).max(1) as u16;
        container(space())
            .width(Length::FillPortion(portion))
            .height(theme::USAGE_BAR)
            .style(move |_| container::Style {
                background: Some(colour.into()),
                border: Border {
                    radius: corners(radius),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into()
    });
    let bar = row(segments).spacing(theme::SPACE_2);
    let rows = report
        .models
        .iter()
        .map(|model| {
            vec![
                Cell::named(&model.model, Some(colours.of_model(&model.model))),
                Cell::plain(short(model.tokens)),
                Cell::plain(share(model.tokens, total)),
                Cell::plain(grouped(model.calls)),
                Cell::plain(grouped(model.tokens / model.calls.max(1))),
                Cell::plain(share(model.cached_tokens, model.input_tokens)),
            ]
        })
        .collect();
    let columns = [
        ("Model", Length::FillPortion(4)),
        ("Tokens", Length::FillPortion(2)),
        ("Share", Length::FillPortion(2)),
        ("Calls", Length::FillPortion(2)),
        ("Tokens per call", Length::FillPortion(3)),
        ("From cache", Length::FillPortion(3)),
    ];
    let content: Element<'a, AppMessage> = if report.models.is_empty() {
        page::body(NO_CALL)
    } else {
        column![bar, table(&columns, rows)]
            .spacing(theme::PAGE_GAP)
            .into()
    };
    panel(
        "Tokens by model",
        Some(format!(
            "{} {span} across {} models",
            short(total),
            report.models.len()
        )),
        content,
    )
}

/// What the model did with some of its time, as the time box names it.
#[derive(Debug, PartialEq, Eq)]
struct Doing {
    name: String,
    /// Whether the name is a tool's, which is set in the code font.
    tool: bool,
    calls: i64,
    model_ms: i64,
}

/// The activities of the report as the bars of the time box: the first eight by time, then the rest as "Other".
fn doings(report: &UsageReport) -> Vec<Doing> {
    let mut doings: Vec<_> = report
        .activities
        .iter()
        .map(|a| {
            let (name, tool) = match a.activity.as_str() {
                "answering" => ("Answering", false),
                "unknown" => ("Unknown", false),
                tool => (tool, true),
            };
            Doing {
                name: name.to_string(),
                tool,
                calls: a.calls,
                model_ms: a.model_ms,
            }
        })
        .collect();
    if doings.len() > ACTIVITY_BARS {
        let rest = doings.split_off(ACTIVITY_BARS);
        doings.push(Doing {
            name: "Other".into(),
            tool: false,
            calls: rest.iter().map(|d| d.calls).sum(),
            model_ms: rest.iter().map(|d| d.model_ms).sum(),
        });
    }
    doings
}

/// What the model did with its time, as a bar for each activity against the one that took longest: the tool it called
/// or answering, with the calls in its label and the time at the right.
pub fn time_box<'a>(report: &'a UsageReport, span: &str) -> Element<'a, AppMessage> {
    let doings = doings(report);
    let longest = doings.iter().map(|d| d.model_ms).max().unwrap_or(1).max(1);
    let bars = doings.into_iter().map(|doing| {
        let portion = (doing.model_ms * 1000 / longest).clamp(0, 1000) as u16;
        let bar = container(space())
            .width(Length::FillPortion(portion.max(1)))
            .height(theme::USAGE_BAR)
            .style(|_| container::Style {
                background: Some(theme::model_colour(0).into()),
                border: Border {
                    radius: corners([0.0, theme::RADIUS_ITEM, theme::RADIUS_ITEM, 0.0]),
                    ..Border::default()
                },
                ..container::Style::default()
            });
        let name = text(doing.name).size(theme::BODY).wrapping(Wrapping::None);
        let name = if doing.tool {
            name.font(theme::MONO).size(theme::CODE)
        } else {
            name
        };
        let calls = format!(
            " · {} {}",
            grouped(doing.calls),
            if doing.calls == 1 { "call" } else { "calls" }
        );
        row![
            container(row![
                name,
                text(calls)
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED)
                    .wrapping(Wrapping::None)
            ])
            .width(theme::ACTIVITY_NAME_WIDTH),
            container(row![
                bar,
                space().width(Length::FillPortion(1001 - portion.max(1)))
            ])
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(theme::HEAD_PILL.into()),
                border: Border {
                    radius: theme::RADIUS_ITEM.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
            container(
                text(duration(doing.model_ms / 1000))
                    .size(theme::CONTEXT)
                    .font(theme::MONO)
                    .color(theme::TEXT_SECONDARY)
            )
            .width(theme::ACTIVITY_TIME_WIDTH)
            .align_x(Alignment::End),
        ]
        .spacing(theme::PAGE_GAP)
        .align_y(Alignment::Center)
        .into()
    });
    let content: Element<'a, AppMessage> = if report.activities.is_empty() {
        page::body(NO_CALL)
    } else {
        column(bars).spacing(theme::PAGE_GAP).into()
    };
    panel(
        "Where the model time goes",
        Some(format!(
            "{} {span}",
            duration(report.totals.model_ms / 1000)
        )),
        content,
    )
}

/// The tools whose calls made runs wait longest.
pub fn waits_box(report: &UsageReport) -> Element<'_, AppMessage> {
    let rows = report
        .tool_waits
        .iter()
        .map(|wait| {
            vec![
                Cell::code(&wait.tool),
                Cell::plain(grouped(wait.calls)),
                Cell::plain(duration(wait.seconds)),
            ]
        })
        .collect();
    let columns = [
        ("Tool call", Length::FillPortion(4)),
        ("Calls", Length::FillPortion(1)),
        ("Waited", Length::FillPortion(1)),
    ];
    let content: Element<'_, AppMessage> = if report.tool_waits.is_empty() {
        page::body("No tool call waited in this window.")
    } else {
        table(&columns, rows)
    };
    // What making the tools' outputs smaller saved, beside how long they took.
    let saving = &report.tool_output;
    let aside = (saving.results > 0).then(|| {
        format!(
            "Outputs cut from {} to {}",
            bytes(saving.bytes_before),
            bytes(saving.bytes_after)
        )
    });
    panel("Longest tool waits", aside, content)
}

/// Where the time of the lint and type checks after the agents' edits went, by phase and tool; none when there was none.
pub fn lens_box(report: &UsageReport) -> Option<Element<'_, AppMessage>> {
    if report.lens.is_empty() {
        return None;
    }
    let rows = report
        .lens
        .iter()
        .map(|check| {
            vec![
                Cell::code(&check.tool),
                Cell::plain(
                    match check.phase.as_str() {
                        "deep" => "Before the run ends",
                        "syntax" => "Syntax, after an edit",
                        _ => "After an edit",
                    }
                    .to_string(),
                ),
                Cell::plain(grouped(check.runs)),
                Cell::plain(grouped(check.skipped)),
                Cell::plain(milliseconds(check.ms)),
                Cell::plain(grouped(check.shown)),
            ]
        })
        .collect();
    let columns = [
        ("Check", Length::FillPortion(3)),
        ("When", Length::FillPortion(3)),
        ("Runs", Length::FillPortion(1)),
        ("Skipped", Length::FillPortion(1)),
        ("Time", Length::FillPortion(1)),
        ("Told to the agent", Length::FillPortion(2)),
    ];
    let total: i64 = report.lens.iter().map(|check| check.ms).sum();
    Some(panel(
        "Lint and type checks",
        Some(milliseconds(total)),
        table(&columns, rows),
    ))
}

/// A time in milliseconds up to a second, then as `duration` says it.
fn milliseconds(ms: i64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        duration(ms / 1000)
    }
}

pub fn agents_box(report: &UsageReport) -> Element<'_, AppMessage> {
    let rows = report
        .agents
        .iter()
        .map(|agent| {
            vec![
                Cell::named(&agent.agent, None),
                Cell::plain(grouped(agent.calls)),
                Cell::plain(short(agent.tokens)),
                Cell::plain(share(agent.cached_tokens, agent.input_tokens)),
                Cell::plain(duration(agent.model_ms / 1000)),
            ]
        })
        .collect();
    let columns = [
        ("Agent", Length::FillPortion(4)),
        ("Calls", Length::FillPortion(2)),
        ("Tokens", Length::FillPortion(2)),
        ("From cache", Length::FillPortion(2)),
        ("Model time", Length::FillPortion(2)),
    ];
    let content: Element<'_, AppMessage> = if report.agents.is_empty() {
        page::body(NO_CALL)
    } else {
        table(&columns, rows)
    };
    panel("By agent", None, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_box_shows_eight_activities_and_gathers_the_rest_as_other() {
        let activity = |name: &str, model_ms, calls| crate::client::ActivityUsage {
            activity: name.into(),
            model_ms,
            calls,
            output_tokens: 0,
        };
        let mut report: UsageReport = serde_json::from_value(serde_json::json!({
            "today": "2026-10-28", "days_in_window": 1,
            "totals": {"tokens": 0, "calls": 0, "cached_share": 0.0, "model_ms": 0, "day_budget": null},
            "days": [], "models": [], "agents": [], "activities": [], "tool_waits": []
        }))
        .unwrap();
        report.activities = vec![
            activity("bash", 9000, 214),
            activity("answering", 5000, 40),
            activity("unknown", 1000, 3),
        ];
        assert_eq!(
            doings(&report)
                .iter()
                .map(|d| (d.name.as_str(), d.tool, d.calls))
                .collect::<Vec<_>>(),
            [
                ("bash", true, 214),
                ("Answering", false, 40),
                ("Unknown", false, 3)
            ]
        );

        report.activities = (0..11)
            .map(|i| activity(&format!("tool{i}"), 100 - i, 1))
            .collect();
        let shown = doings(&report);
        assert_eq!(shown.len(), 9);
        assert_eq!(shown[7].name, "tool7");
        assert_eq!(
            shown[8],
            Doing {
                name: "Other".into(),
                tool: false,
                calls: 3,
                model_ms: 92 + 91 + 90
            }
        );
    }
}
