//! The Get started page: what a server still needs before the first card can run (an upstream with models, Pi-Bolt,
//! a harness configuration repository, the Coder's model, a repository), read from `GET /v1/start`, with a button to
//! the page that fixes each step, or for the repository the "Set up" that makes it (see `harness_setup`). It is
//! read again each time the page is opened, when any page is opened while a step is missing, and by its Refresh
//! button; the sidebar shows its entry only while a step is missing.

use iced::font::Weight;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding, Task};

use crate::app::{Message as AppMessage, Page};
use crate::client::{Client, ClientError, Start as Answer, StartStep};
use crate::screens::harness_setup::HarnessSetup;
use crate::settings::Settings;
use crate::theme;
use crate::widgets::page::{self, Tone};

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    Loaded(Result<Answer, ClientError>),
}

/// What the server last said about its first start.
#[derive(Default)]
pub struct Start {
    answer: Option<Answer>,
    /// Why the last read failed, if it did.
    error: Option<String>,
}

impl Start {
    /// Reads the steps again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        Task::perform(async move { client.start().await }, |result| {
            AppMessage::Start(Message::Loaded(result))
        })
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Refresh => settings.map_or_else(Task::none, |settings| self.reload(settings)),
            Message::Loaded(Ok(answer)) => {
                self.answer = Some(answer);
                self.error = None;
                Task::none()
            }
            Message::Loaded(Err(error)) => {
                self.error = Some(crate::screens::live::say(&error));
                Task::none()
            }
        }
    }

    /// Whether the server said that a step is missing. Until it has said anything, nothing is.
    pub fn incomplete(&self) -> bool {
        self.answer.as_ref().is_some_and(|answer| !answer.done)
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(message, Message::Loaded(Err(ClientError::Unauthorized)))
    }

    pub fn view<'a>(&'a self, harness: &'a HarnessSetup) -> Element<'a, AppMessage> {
        let context = match &self.answer {
            None => "Reading what the server still needs.",
            Some(answer) if answer.done => "Everything is set up. Cards can run.",
            Some(_) => "A few things have to be set up before the first card can run.",
        };
        let mut content = column![].spacing(theme::PAGE_GAP);
        if let Some(error) = &self.error {
            content = content.push(
                text(error.clone())
                    .size(theme::CONTEXT)
                    .font(theme::MONO)
                    .color(theme::RED_TEXT),
            );
        }
        if let Some(answer) = &self.answer {
            let mut steps = column![].spacing(theme::PAGE_GAP);
            for (index, step) in answer.steps.iter().enumerate() {
                if index > 0 {
                    steps = steps.push(page::rule());
                }
                steps = steps.push(step_row(step, harness));
            }
            content = content.push(page::boxed("Before the first card", steps));
        }
        column![
            page::header(
                "Setup",
                Page::Start.title(),
                None,
                page::context(context),
                page::white_button("Refresh", Some(AppMessage::Start(Message::Refresh))),
            ),
            scrollable(container(content).padding(Padding::default().right(theme::SCROLL_GUTTER)))
                .height(Length::Fill),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }
}

/// The page that fixes a step, and what its button says.
fn fix(key: &str) -> Option<(Page, &'static str)> {
    match key {
        "upstream" => Some((Page::Models, "Open Models")),
        "pi_bolt" => Some((Page::Settings, "Open Settings")),
        "coder" => Some((Page::Agents, "Open Agents")),
        "repository" => Some((Page::AddRepository, "Add repository")),
        _ => None,
    }
}

fn step_row<'a>(step: &'a StartStep, harness: &'a HarnessSetup) -> Element<'a, AppMessage> {
    let mark = if step.done {
        page::tag("Done".to_string(), Tone::Teal)
    } else {
        page::tag("To do".to_string(), Tone::Amber)
    };
    let mut words = column![
        text(step.title.as_str())
            .size(theme::BODY)
            .font(theme::manrope(Weight::Semibold)),
        text(step.detail.as_str())
            .size(theme::CONTEXT)
            .font(theme::MANROPE)
            .color(theme::TEXT_SECONDARY),
    ]
    .spacing(theme::SPACE_4)
    .width(Length::Fill);
    let set_up = step.key == "harness" && !step.done;
    if let Some(error) = harness.error().filter(|_| set_up) {
        words = words.push(text(error).size(theme::CONTEXT).color(theme::RED_TEXT));
    }
    let mut line = row![container(mark).width(theme::STEP_TAG_WIDTH), words]
        .spacing(theme::PAGE_GAP)
        .align_y(Alignment::Center);
    if set_up {
        // The step is fixed here: the repository is made and the server restarts.
        line = line.push(harness.button());
    } else if let (false, Some((target, label))) = (step.done, fix(&step.key)) {
        line = line.push(page::white_button(label, Some(AppMessage::Visit(target))));
    }
    line.into()
}
