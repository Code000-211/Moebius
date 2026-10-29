//! The Retro screen: the Retros of the days, the errors of the one chosen grouped by kind, the proposals the Retro
//! analyst made, each with its status and what its trials found, and under them the lessons it learned (see
//! `lessons`). "Run the Retro now" is `POST /v1/retros`; the server says when a Retro ends with the event
//! `retro_done`, and when a trial or a proposal's status changes with `trial_done` and `proposal_status`, which the
//! app answers by reading the screen again.

pub mod lessons;

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, space, text};
use iced::{Alignment, Border, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Proposal, ReportError, RetroSummary, RetroView, Trial};
use crate::settings::Settings;
use crate::theme;
use crate::widgets::list_editor;
use crate::widgets::page::{self, Tone};
use lessons::Lessons;

/// The kinds of error of a report, in the order the page lists them, with what the page calls them.
const KINDS: [(&str, &str); 8] = [
    ("tool_failed", "Failed tool calls"),
    ("tool_refused", "Refused tool calls"),
    ("run_failed", "Failed runs"),
    ("run_stopped", "Stopped runs"),
    ("steered", "Steering"),
    ("checks_failed", "Failed checks"),
    ("conflict", "Merge conflicts"),
    ("review_findings", "Review findings"),
];

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Vec<RetroSummary>, ClientError>),
    /// A day of the list was chosen.
    Select(String),
    /// The Retro of this day was read.
    Opened(String, Result<RetroView, ClientError>),
    /// "Run the Retro now" was pressed.
    Run,
    /// The server answered it.
    Started(Result<RetroView, ClientError>),
    /// Something on the lessons box.
    Lessons(lessons::Message),
}

#[derive(Default)]
pub struct Retros {
    list: Vec<RetroSummary>,
    /// The day chosen and its Retro, once read.
    chosen: Option<String>,
    opened: Option<RetroView>,
    /// Why the last "Run the Retro now" did not start one.
    failed: Option<String>,
    lessons: Lessons,
}

impl Retros {
    /// Reads the list again, and the Retro on show.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        let list = Task::perform(async move { client.retros().await }, |result| {
            AppMessage::Retro(Message::Listed(result))
        });
        let lessons = self.lessons.reload(settings);
        match &self.chosen {
            Some(date) => Task::batch([list, lessons, open(settings, date)]),
            None => Task::batch([list, lessons]),
        }
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Listed(Err(ClientError::Unauthorized))
                | Message::Opened(_, Err(ClientError::Unauthorized))
                | Message::Started(Err(ClientError::Unauthorized))
        ) || matches!(message, Message::Lessons(m) if Lessons::refused(m))
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        match message {
            Message::Listed(Ok(list)) => {
                self.list = list;
                if self.chosen.is_none()
                    && let Some(latest) = self.list.first()
                {
                    return self.select(latest.date.clone(), settings);
                }
            }
            Message::Listed(Err(_)) => {}
            Message::Select(date) => return self.select(date, settings),
            Message::Opened(date, Ok(view)) => {
                if self.chosen.as_deref() == Some(date.as_str()) {
                    self.opened = Some(view);
                }
            }
            Message::Opened(_, Err(_)) => {}
            Message::Run => {
                self.failed = None;
                let client = Client::new(settings.clone());
                return Task::perform(async move { client.start_retro().await }, |result| {
                    AppMessage::Retro(Message::Started(result))
                });
            }
            Message::Started(Ok(view)) => {
                self.failed = None;
                self.chosen = Some(view.date.clone());
                self.opened = Some(view);
                return self.reload(settings);
            }
            Message::Started(Err(e)) => self.failed = Some(e.to_string()),
            Message::Lessons(message) => return self.lessons.update(message, settings),
        }
        Task::none()
    }

    fn select(&mut self, date: String, settings: &Settings) -> Task<AppMessage> {
        if self.chosen.as_ref() != Some(&date) {
            self.opened = None;
        }
        self.chosen = Some(date.clone());
        open(settings, &date)
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        let header = page::header(
            "Improve",
            "Retro",
            None,
            page::context("Errors of each day, and the changes the Retro proposes to avoid them."),
            page::primary_button("Run the Retro now", Some(AppMessage::Retro(Message::Run))),
        );
        let mut detail = column![].spacing(theme::PAGE_GAP).width(Length::Fill);
        if let Some(failed) = &self.failed {
            detail = detail.push(page::boxed(
                "The Retro did not start",
                text(failed.as_str())
                    .size(theme::BODY)
                    .color(theme::RED_TEXT),
            ));
        }
        detail = match &self.opened {
            Some(view) => detail
                .push(warnings_box(view))
                .push(errors_box(view))
                .push(proposals_box(&view.proposals)),
            None if self.list.is_empty() => detail.push(page::boxed(
                "No Retro yet",
                page::body("The first runs at retro_at, or press \"Run the Retro now\"."),
            )),
            None => detail.push(page::boxed("Retro", page::body("Reading the Retro…"))),
        }
        .push(self.lessons.view());
        column![
            header,
            row![
                scrollable(self.list_view()).width(theme::LIST_WIDTH_NARROW),
                scrollable(
                    container(detail).padding(iced::Padding::default().right(theme::SCROLL_GUTTER))
                )
                .width(Length::Fill)
            ]
            .spacing(theme::PAGE_GAP)
            .height(Length::Fill),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }

    fn list_view(&self) -> Element<'_, AppMessage> {
        if self.list.is_empty() {
            return page::boxed("Retros", page::body("None yet."));
        }
        column(
            self.list.iter().map(|retro| {
                retro_card(retro, self.chosen.as_deref() == Some(retro.date.as_str()))
            }),
        )
        .spacing(theme::SPACE_8)
        .padding(iced::Padding::default().right(theme::SCROLL_GUTTER))
        .into()
    }
}

fn open(settings: &Settings, date: &str) -> Task<AppMessage> {
    let (client, date) = (Client::new(settings.clone()), date.to_string());
    Task::perform(
        async move {
            let result = client.retro(&date).await;
            (date, result)
        },
        |(date, result)| AppMessage::Retro(Message::Opened(date, result)),
    )
}

fn status_tag(status: &str) -> (&'static str, Tone) {
    match status {
        "analysing" => ("Analysing", Tone::Amber),
        "proposed" => ("Proposed", Tone::Teal),
        "failed" => ("Failed", Tone::Red),
        _ => ("Reported", Tone::Plain),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn retro_card(retro: &RetroSummary, chosen: bool) -> Element<'_, AppMessage> {
    let counts = format!(
        "{} · {}",
        plural(retro.errors, "error", "errors"),
        plural(retro.proposals, "proposal", "proposals")
    );
    list_editor::list_card(
        &retro.date,
        vec![status_tag(&retro.status)],
        counts,
        false,
        chosen,
        Some(AppMessage::Retro(Message::Select(retro.date.clone()))),
    )
}

fn warnings_box(view: &RetroView) -> Element<'_, AppMessage> {
    if view.warnings.is_empty() {
        return space().into();
    }
    let lines = view.warnings.iter().map(|warning| {
        text(warning.as_str())
            .size(theme::BODY)
            .color(theme::AMBER_TEXT)
            .wrapping(Wrapping::WordOrGlyph)
            .into()
    });
    page::boxed("Left out", column(lines).spacing(theme::SPACE_6))
}

/// Where an error happened: its run, card and agent.
fn origin(error: &ReportError) -> String {
    let mut parts = vec![];
    if let Some(card) = error.card {
        parts.push(format!("card {card}"));
    }
    if let Some(run) = error.run {
        parts.push(format!("run {run}"));
    }
    if let Some(agent) = &error.agent {
        parts.push(agent.clone());
    }
    parts.join(" · ")
}

fn errors_box(view: &RetroView) -> Element<'_, AppMessage> {
    let heading = format!("Errors of {}", view.date);
    let mut groups = column![].spacing(theme::PAGE_GAP);
    for (kind, label) in KINDS {
        let errors: Vec<&ReportError> = view
            .report
            .errors
            .iter()
            .filter(|error| error.kind == kind)
            .collect();
        if errors.is_empty() {
            continue;
        }
        let lines = errors.into_iter().map(|error| {
            column![
                text(origin(error))
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED),
                text(error.text.as_str())
                    .size(theme::BODY)
                    .color(theme::TEXT_SECONDARY)
                    .wrapping(Wrapping::WordOrGlyph),
            ]
            .spacing(theme::SPACE_2)
            .into()
        });
        let count = view.report.errors.iter().filter(|e| e.kind == kind).count();
        groups = groups.push(
            column![
                text(format!("{label} · {count}"))
                    .size(theme::CARD_TITLE)
                    .font(theme::manrope(Weight::Semibold)),
                column(lines).spacing(theme::SPACE_8),
            ]
            .spacing(theme::SPACE_6),
        );
    }
    let content: Element<_> = if view.report.errors.is_empty() {
        page::body("No errors that day.")
    } else {
        groups.into()
    };
    // `page::boxed` takes a heading that lives as long as the page, and this one is made here.
    container(
        column![
            text(heading)
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold)),
            content
        ]
        .spacing(theme::SPACE_8),
    )
    .padding(theme::BOX_PADDING)
    .width(Length::Fill)
    .style(|_| page::box_style())
    .into()
}

fn proposals_box(proposals: &[Proposal]) -> Element<'_, AppMessage> {
    if proposals.is_empty() {
        return page::boxed("Proposals", page::body("The Retro proposed nothing."));
    }
    page::boxed(
        "Proposals",
        column(proposals.iter().map(proposal)).spacing(theme::PAGE_GAP),
    )
}

/// A proposal's status as a tag: how far its trials got, and what a person did with it.
fn proposal_status<'a>(status: &str) -> Element<'a, AppMessage> {
    let (label, tone) = match status {
        "trying" => ("Trying", Tone::Amber),
        "positive" => ("Positive", Tone::Teal),
        "negative" => ("Negative", Tone::Red),
        "untried" => ("Untried", Tone::Amber),
        "merged" => ("Merged", Tone::Teal),
        "dismissed" => ("Dismissed", Tone::Plain),
        _ => ("Open", Tone::Plain),
    };
    page::tag(label.to_string(), tone)
}

/// The cards of a proposal as the page says them: `Cards 48, 50`.
fn cards(cards: &[i64]) -> String {
    let numbers: Vec<String> = cards.iter().map(i64::to_string).collect();
    format!("Cards {}", numbers.join(", "))
}

/// One trial: its outcome, the card it was tried on and why it ended so.
fn trial(trial: &Trial) -> Element<'_, AppMessage> {
    let tone = match trial.outcome.as_str() {
        "positive" => Tone::Teal,
        "negative" => Tone::Red,
        _ => Tone::Plain,
    };
    let outcome = trial.outcome[..1].to_uppercase() + &trial.outcome[1..];
    let card = match (trial.card_id, trial.regression) {
        (Some(card), true) => format!("Regression card {card}"),
        (Some(card), false) => format!("Card {card}"),
        (None, _) => "No card".to_string(),
    };
    row![
        page::tag(outcome, tone),
        text(card)
            .size(theme::LABEL)
            .font(theme::MONO)
            .color(theme::TEXT_MUTED),
        text(trial.reason.as_str())
            .size(theme::BODY)
            .color(theme::TEXT_SECONDARY)
            .width(Length::Fill)
            .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(theme::SPACE_8)
    .align_y(Alignment::Center)
    .into()
}

/// What a proposal for Moebius's own configuration changes, as its tag says it; none for one of the harness
/// configuration.
pub fn own_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "settings" => Some("moebius.toml"),
        "repository" => Some(".moebius/"),
        _ => None,
    }
}

/// A proposal: its title and status, the agents it changes, its branch, what it says it does, the cards it addresses
/// and what the trials found, or the diff of a change to Moebius's own configuration.
pub fn proposal(proposal: &Proposal) -> Element<'_, AppMessage> {
    let mut head = row![
        text(proposal.title.as_str())
            .size(theme::CARD_TITLE)
            .font(theme::manrope(Weight::Semibold))
            .wrapping(Wrapping::WordOrGlyph),
        space::horizontal(),
    ]
    .spacing(theme::SPACE_6)
    .align_y(Alignment::Center);
    for agent in &proposal.agents {
        head = head.push(page::tag(agent.clone(), Tone::Plain));
    }
    if let Some(what) = own_kind(&proposal.kind) {
        head = head.push(page::tag(what.to_string(), Tone::Plain));
    }
    head = head.push(proposal_status(&proposal.status));
    let mut content = column![
        head,
        text(proposal.branch.as_str())
            .size(theme::CODE_SMALL)
            .font(theme::MONO)
            .color(theme::TEXT_MUTED)
            .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(theme::SPACE_4);
    if !proposal.body.is_empty() {
        content = content.push(
            text(proposal.body.as_str())
                .size(theme::BODY)
                .color(theme::TEXT_SECONDARY)
                .wrapping(Wrapping::WordOrGlyph),
        );
    }
    if !proposal.cards.is_empty() {
        content = content.push(
            text(cards(&proposal.cards))
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
        );
    }
    if !proposal.trials.is_empty() {
        content = content.push(column(proposal.trials.iter().map(trial)).spacing(theme::SPACE_6));
    }
    if !proposal.diff.is_empty() {
        content = content.push(crate::widgets::transcript::code_block(
            "diff",
            &proposal.diff,
        ));
    }
    container(content)
        .padding(theme::BOX_PADDING)
        .width(Length::Fill)
        .style(|_| card_style())
        .into()
}

/// The style of a card inside a box: a proposal or a lesson.
fn card_style() -> container::Style {
    container::Style {
        background: Some(theme::LANE.into()),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_CARD.into(),
        },
        ..container::Style::default()
    }
}
