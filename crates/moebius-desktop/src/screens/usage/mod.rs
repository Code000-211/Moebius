//! The Usage screen (`docs/design/usage.png`): what the model calls used, from `GET /v1/usage`. A window (today, 7 or 30
//! days) sets the tiles, the models, what the model did with its time, the agents and the tool waits; the heatmap
//! always shows the last year. The tokens used today and the day budget also feed the meter at the foot of the
//! sidebar, so the app reads the usage when it connects and not only when this page opens.
//!
//! `figures` tells the numbers, times and dates, `heatmap` draws the last year and gives each model its colour,
//! `boxes` the tiles and boxes under it, and `table` the panel and table they are made of.

mod boxes;
mod figures;
mod heatmap;
mod table;

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, UsageReport};
use crate::settings::Settings;
use crate::theme;
use crate::widgets::page;

use boxes::{agents_box, lens_box, models_box, tiles, time_box, waits_box};
use heatmap::{Colours, heatmap};

pub use figures::short;

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
    /// The answer, boxed: a report is much larger than the other messages.
    Loaded(Result<Box<UsageReport>, ClientError>),
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
            AppMessage::Usage(Message::Loaded(result.map(Box::new)))
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
                    self.report = Some(*report);
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
        let mut sections = column![
            tiles(report, self.window),
            heatmap(report, &colours),
            models_box(report, &colours, &span),
            row![time_box(report, &span), waits_box(report)]
                .spacing(theme::PAGE_GAP)
                .align_y(Alignment::Start),
        ];
        if let Some(lens) = lens_box(report) {
            sections = sections.push(lens);
        }
        sections
            .push(agents_box(report))
            .spacing(theme::PAGE_GAP)
            .width(Length::Fill)
            .into()
    }
}
