//! The Usage screen (`docs/design/usage.png`): what the model calls used, from `GET /v1/usage`. A window (today, 7 or 30
//! days) sets the tiles, the models, what the model did with its time, the agents and the tool waits; the heatmap always shows the last year. The tokens
//! used today and the day budget also feed the meter at the foot of the sidebar, so the app reads the usage when it
//! connects and not only when this page opens.

use std::collections::HashMap;

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, space, text};
use iced::{Alignment, Border, Color, Element, Length, Padding, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, UsageDay, UsageReport};
use crate::settings::Settings;
use crate::theme;
use crate::widgets::page;

/// The weeks of the heatmap: a year and the week it starts in.
const WEEKS: i64 = 53;
/// The height of a heatmap cell, and the gap around it.
const CELL: f32 = 16.0;
const CELL_GAP: f32 = 2.0;
/// The width of the column that names the days of the week.
const WEEKDAY_LABELS: f32 = 40.0;
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// How many days the figures of the page reach back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Today,
    Week,
    Month,
}

impl Window {
    const ALL: [Window; 3] = [Window::Today, Window::Week, Window::Month];

    fn days(self) -> i64 {
        match self {
            Window::Today => 1,
            Window::Week => 7,
            Window::Month => 30,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Window::Today => "Today",
            Window::Week => "7 days",
            Window::Month => "30 days",
        }
    }

    /// How a total over the window is told: "today", "in the last 7 days".
    fn span(self) -> String {
        match self {
            Window::Today => "today".to_string(),
            other => format!("in the last {} days", other.days()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<UsageReport, ClientError>),
    Choose(Window),
}

pub struct Usage {
    window: Window,
    /// The latest answer; it can be of another window while the answer for the chosen one is on its way.
    report: Option<UsageReport>,
}

impl Default for Usage {
    fn default() -> Self {
        Usage {
            window: Window::Today,
            report: None,
        }
    }
}

impl Usage {
    /// Reads the usage of the chosen window again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let (client, days) = (Client::new(settings.clone()), self.window.days());
        Task::perform(async move { client.usage(days).await }, |result| {
            AppMessage::Usage(Message::Loaded(result))
        })
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(message, Message::Loaded(Err(ClientError::Unauthorized)))
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Loaded(Ok(report)) => {
                // An answer for a window that is no longer the chosen one is of no use.
                if report.days_in_window == self.window.days() {
                    self.report = Some(report);
                }
            }
            Message::Loaded(Err(_)) => {}
            Message::Choose(window) => {
                self.window = window;
                if let Some(settings) = settings {
                    return self.reload(settings);
                }
            }
        }
        Task::none()
    }

    /// The tokens used today and the day's budget, for the meter of the sidebar; none until the usage is read.
    pub fn today(&self) -> Option<(i64, Option<i64>)> {
        let report = self.report.as_ref()?;
        let tokens = report
            .days
            .iter()
            .find(|day| day.date == report.today)
            .map_or(0, |day| day.tokens);
        Some((tokens, report.totals.day_budget))
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        let header = page::header(
            "",
            "Usage",
            None,
            page::context(
                "Which models ran when, how many tokens went where, and what the models spent their time on.",
            ),
            text("").into(),
        );
        let chips = row(Window::ALL.map(|window| {
            page::chip(
                window.label().to_string(),
                window == self.window,
                false,
                AppMessage::Usage(Message::Choose(window)),
            )
        }))
        .spacing(8);
        let body: Element<'_, AppMessage> = match &self.report {
            Some(report) => self.report_view(report),
            None => page::boxed("Usage", page::body("Reading the usage…")),
        };
        column![
            header,
            chips,
            scrollable(container(body).padding(Padding::default().right(12))).height(Length::Fill)
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }

    fn report_view<'a>(&self, report: &'a UsageReport) -> Element<'a, AppMessage> {
        let colours = Colours::of(report);
        let span = self.window.span();
        column![
            tiles(report, self.window),
            heatmap(report, &colours),
            models_box(report, &colours, &span),
            row![time_box(report, &span), waits_box(report)]
                .spacing(theme::PAGE_GAP)
                .align_y(Alignment::Start),
            agents_box(report),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }
}

/// The colour of each model: the places of the models in the order in which they appear in the heatmap, then in the
/// table, so that a model keeps its colour on every chart.
struct Colours<'a>(Vec<&'a str>);

impl<'a> Colours<'a> {
    fn of(report: &'a UsageReport) -> Self {
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

    fn of_model(&self, model: &str) -> Color {
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

// Numbers

/// A number of tokens in three figures: 960k, 1.84M, 148M.
pub fn short(n: i64) -> String {
    let (value, unit) = match n {
        ..1_000 => return n.to_string(),
        1_000..1_000_000 => (n as f64 / 1e3, "k"),
        _ => (n as f64 / 1e6, "M"),
    };
    let decimals = if value < 10.0 {
        2
    } else if value < 100.0 {
        1
    } else {
        0
    };
    let shown = format!("{value:.decimals$}");
    let shown = if shown.contains('.') {
        shown.trim_end_matches('0').trim_end_matches('.')
    } else {
        &shown
    };
    format!("{shown}{unit}")
}

/// A whole number with thousands separators: 1,284.
fn grouped(n: i64) -> String {
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if n < 0 { format!("-{out}") } else { out }
}

fn percent(share: f64) -> String {
    format!("{:.0}%", share * 100.0)
}

/// A share of a whole as a percentage; 0 of nothing.
fn share(part: i64, whole: i64) -> String {
    percent(if whole > 0 {
        part as f64 / whole as f64
    } else {
        0.0
    })
}

/// A time as hours and minutes ("3 h 12 m"), minutes ("38 m") or seconds ("45 s").
fn duration(seconds: i64) -> String {
    match seconds {
        ..60 => format!("{seconds} s"),
        60..3600 => format!("{} m", seconds / 60),
        _ => format!("{} h {} m", seconds / 3600, seconds % 3600 / 60),
    }
}

// Dates: days since 1970-01-01 and the calendar, by Howard Hinnant's algorithms.

/// The day number of a `YYYY-MM-DD` date.
fn day_number(date: &str) -> Option<i64> {
    let mut parts = date.split('-').map(|part| part.parse::<i64>().ok());
    let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

/// The month (0 to 11) and day of the month of a day number.
fn month_and_day(number: i64) -> (usize, i64) {
    let z = number + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 2
    } else {
        shifted - 10
    };
    (month as usize, day)
}

/// The day of the week of a day number, Sunday 0 (1970-01-01 was a Thursday).
fn weekday(number: i64) -> usize {
    (number + 4).rem_euclid(7) as usize
}

// Parts of the page

/// The radii of the corners of a bar, clockwise from the top left.
fn corners([top_left, top_right, bottom_right, bottom_left]: [f32; 4]) -> iced::border::Radius {
    iced::border::Radius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    }
}

/// A white box with a heading and, at the right of it, a line that sums it up.
fn panel<'a>(
    heading: &'a str,
    aside: Option<String>,
    content: Element<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    let mut head = row![
        text(heading)
            .size(theme::BOX_HEADING)
            .font(theme::manrope(Weight::Bold)),
        space::horizontal(),
    ]
    .align_y(Alignment::Center);
    if let Some(aside) = aside {
        head = head.push(text(aside).size(theme::LABEL).color(theme::TEXT_SECONDARY));
    }
    container(column![head, content].spacing(14))
        .padding([16, 18])
        .width(Length::Fill)
        .style(|_| page::box_style())
        .into()
}

fn tiles(report: &UsageReport, window: Window) -> Element<'_, AppMessage> {
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
    .spacing(12)
    .into()
}

fn tile<'a>(label: &'a str, value: String, note: String) -> Element<'a, AppMessage> {
    container(
        column![
            text(label)
                .size(13)
                .font(theme::manrope(Weight::Semibold))
                .color(theme::SIDEBAR_GROUP),
            text(value)
                .size(theme::TILE_VALUE)
                .font(theme::manrope(Weight::Bold)),
            text(note).size(13).color(theme::TEXT_SECONDARY),
        ]
        .spacing(4),
    )
    .padding([14, 16])
    .width(Length::FillPortion(1))
    .style(|_| page::box_style())
    .into()
}

/// The heatmap of the last 53 weeks: a column a week, Sunday at the top, a cell a day, in the colour of the model that
/// led the day and in one of four shades by the tokens of the day against the busiest day.
fn heatmap<'a>(report: &'a UsageReport, colours: &Colours<'a>) -> Element<'a, AppMessage> {
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
        container(
            container(space())
                .width(Length::Fill)
                .height(CELL)
                .style(move |_| container::Style {
                    background: fill.map(Into::into),
                    border: Border {
                        radius: 3.0.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                }),
        )
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
            container(space())
                .width(14)
                .height(14)
                .style({
                    let colour = theme::shade(colours.of_model(model), level);
                    move |_| container::Style {
                        background: Some(colour.into()),
                        border: Border {
                            radius: 3.0.into(),
                            ..Border::default()
                        },
                        ..container::Style::default()
                    }
                })
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

/// The stacked bar of the models' shares, and the table of what each used.
fn models_box<'a>(
    report: &'a UsageReport,
    colours: &Colours<'a>,
    span: &str,
) -> Element<'a, AppMessage> {
    let total: i64 = report.models.iter().map(|m| m.tokens).sum();
    let last = report.models.len().saturating_sub(1);
    let segments = report.models.iter().enumerate().map(|(i, model)| {
        let colour = colours.of_model(&model.model);
        let radius = match (i == 0, i == last) {
            (true, true) => [7.0; 4],
            (true, false) => [7.0, 0.0, 0.0, 7.0],
            (false, true) => [0.0, 7.0, 7.0, 0.0],
            (false, false) => [0.0; 4],
        };
        let portion = (model.tokens.max(0) * 1000 / total.max(1)).max(1) as u16;
        container(space())
            .width(Length::FillPortion(portion))
            .height(14)
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
    let bar = row(segments).spacing(2);
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
        page::body("No model call in this window.")
    } else {
        column![bar, table(&columns, rows)].spacing(16).into()
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

/// The most activities the time box shows before it gathers the rest under "Other".
const ACTIVITY_BARS: usize = 8;

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
fn time_box<'a>(report: &'a UsageReport, span: &str) -> Element<'a, AppMessage> {
    let doings = doings(report);
    let longest = doings.iter().map(|d| d.model_ms).max().unwrap_or(1).max(1);
    let bars = doings.into_iter().map(|doing| {
        let portion = (doing.model_ms * 1000 / longest).clamp(0, 1000) as u16;
        let bar = container(space())
            .width(Length::FillPortion(portion.max(1)))
            .height(14)
            .style(|_| container::Style {
                background: Some(theme::model_colour(0).into()),
                border: Border {
                    radius: corners([0.0, 4.0, 4.0, 0.0]),
                    ..Border::default()
                },
                ..container::Style::default()
            });
        let name = text(doing.name).size(14).wrapping(Wrapping::None);
        let name = if doing.tool {
            name.font(theme::MONO).size(13)
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
                    .size(12.5)
                    .color(theme::TEXT_SECONDARY)
                    .wrapping(Wrapping::None)
            ])
            .width(220),
            container(row![
                bar,
                space().width(Length::FillPortion(1001 - portion.max(1)))
            ])
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(theme::HEAD_PILL.into()),
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
            container(
                text(duration(doing.model_ms / 1000))
                    .size(12.5)
                    .color(theme::TEXT_SECONDARY)
            )
            .width(90)
            .align_x(Alignment::End),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
    });
    let content: Element<'a, AppMessage> = if report.activities.is_empty() {
        page::body("No model call in this window.")
    } else {
        column(bars).spacing(12).into()
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
fn waits_box(report: &UsageReport) -> Element<'_, AppMessage> {
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
    panel("Longest tool waits", None, content)
}

fn agents_box(report: &UsageReport) -> Element<'_, AppMessage> {
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
        page::body("No model call in this window.")
    } else {
        table(&columns, rows)
    };
    panel("By agent", None, content)
}

/// One cell of a table.
struct Cell {
    text: String,
    /// The square of a model's colour before the text.
    lead: Option<Color>,
    code: bool,
}

impl Cell {
    fn plain(text: String) -> Self {
        Cell {
            text,
            lead: None,
            code: false,
        }
    }

    /// A name, with the colour of the model it names if it names one.
    fn named(name: &str, lead: Option<Color>) -> Self {
        Cell {
            text: name.to_string(),
            lead,
            code: false,
        }
    }

    fn code(text: &str) -> Self {
        Cell {
            text: text.to_string(),
            lead: None,
            code: true,
        }
    }
}

/// A table: bold heads over a line, then a row for each of `rows` with a thin line under it. The first column is
/// left-aligned, the others right-aligned like numbers.
fn table<'a>(columns: &[(&'a str, Length)], rows: Vec<Vec<Cell>>) -> Element<'a, AppMessage> {
    let head = row(columns.iter().enumerate().map(|(i, (name, width))| {
        let label = text(*name)
            .size(12.5)
            .font(theme::manrope(Weight::Bold))
            .color(theme::SIDEBAR_GROUP);
        aligned(label.into(), *width, i == 0)
    }))
    .padding([8, 10]);
    let line = |color: Color| {
        container(space())
            .width(Length::Fill)
            .height(1)
            .style(move |_| container::Style {
                background: Some(color.into()),
                ..container::Style::default()
            })
    };
    let mut table = column![head, line(theme::LINE)];
    for cells in rows {
        let cells = cells.into_iter().enumerate().map(|(i, cell)| {
            let width = columns[i].1;
            let mut label = text(cell.text).size(14).wrapping(Wrapping::None);
            if cell.code {
                label = label.font(theme::MONO).size(theme::CODE);
            }
            let content: Element<'a, AppMessage> = match cell.lead {
                Some(colour) => row![
                    container(space())
                        .width(10)
                        .height(10)
                        .style(move |_| container::Style {
                            background: Some(colour.into()),
                            border: Border {
                                radius: 2.0.into(),
                                ..Border::default()
                            },
                            ..container::Style::default()
                        }),
                    label
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
                None => label.into(),
            };
            aligned(content, width, i == 0)
        });
        table = table
            .push(row(cells).padding([9, 10]))
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

    #[test]
    fn numbers_are_told_in_three_figures_with_separators_and_hours() {
        assert_eq!(short(950), "950");
        assert_eq!(short(960_000), "960k");
        assert_eq!(short(1_840_000), "1.84M");
        assert_eq!(short(8_000_000), "8M");
        assert_eq!(short(148_000_000), "148M");
        assert_eq!(grouped(1284), "1,284");
        assert_eq!(grouped(12), "12");
        assert_eq!(duration(45), "45 s");
        assert_eq!(duration(38 * 60), "38 m");
        assert_eq!(duration(3 * 3600 + 12 * 60), "3 h 12 m");
    }

    #[test]
    fn dates_are_day_numbers_with_their_weekday_and_month() {
        let day = day_number("2026-10-28").unwrap();
        assert_eq!(day, 20_754);
        assert_eq!(WEEKDAYS[weekday(day)], "Wed");
        assert_eq!(month_and_day(day), (9, 28));
        assert_eq!(month_and_day(day_number("2024-02-29").unwrap()), (1, 29));
        assert_eq!(day_number("nonsense"), None);
    }
}
