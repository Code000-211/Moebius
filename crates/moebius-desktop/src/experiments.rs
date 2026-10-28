//! The Experiments screen: the research the server has, a form for a new brief, and for the one chosen its baseline and
//! the experiments, one for each lever the Autoresearcher proposed. A brief is stored as a draft (`POST /v1/research`),
//! then a person confirms it, which lets it run in the trial window, or starts it now; a running research can be
//! stopped. The server says when something changed with `research_status` and `experiment_done`, which the app
//! answers by reading the screen again.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Border, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Experiment, Research, Snapshot};
use crate::connect;
use crate::page::{self, Tone};
use crate::settings::Settings;
use crate::theme;

/// The tokens a new brief starts with.
const DEFAULT_BUDGET: &str = "200000";

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Vec<Research>, ClientError>),
    /// A research of the list was chosen.
    Select(i64),
    /// "New brief" was pressed: the form opens.
    Compose,
    RepositoryChosen(i64),
    GoalEdited(String),
    MeasureEdited(String),
    BetterChosen(&'static str),
    BudgetEdited(String),
    /// "Store the brief" was pressed.
    Store,
    /// A person asked for `confirm`, `start` or `stop` of a research.
    Act(i64, &'static str),
    /// The server answered the brief, or one of the actions.
    Answered(Result<Research, ClientError>),
}

/// The brief being written.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Form {
    repository: Option<i64>,
    goal: String,
    measure: String,
    better: &'static str,
    budget: String,
}

impl Default for Form {
    fn default() -> Self {
        Form {
            repository: None,
            goal: String::new(),
            measure: String::new(),
            better: "lower",
            budget: DEFAULT_BUDGET.to_string(),
        }
    }
}

impl Form {
    /// The token budget, if what was typed is a number above 0.
    fn budget(&self) -> Option<i64> {
        self.budget.trim().parse().ok().filter(|budget| *budget > 0)
    }

    fn ready(&self) -> bool {
        self.repository.is_some()
            && !self.goal.trim().is_empty()
            && !self.measure.trim().is_empty()
            && self.budget().is_some()
    }
}

#[derive(Default)]
pub struct Experiments {
    list: Vec<Research>,
    chosen: Option<i64>,
    /// Whether the form of a new brief is on show instead of the research chosen.
    composing: bool,
    form: Form,
    /// Why the last request of the page was refused.
    failed: Option<String>,
}

impl Experiments {
    /// Reads the research again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        Task::perform(async move { client.researches().await }, |result| {
            AppMessage::Experiments(Message::Listed(result))
        })
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Listed(Err(ClientError::Unauthorized))
                | Message::Answered(Err(ClientError::Unauthorized))
        )
    }

    pub fn update(
        &mut self,
        message: Message,
        settings: Option<&Settings>,
        snapshot: &Snapshot,
    ) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        match message {
            Message::Listed(Ok(list)) => {
                self.list = list;
                if !self.list.iter().any(|r| Some(r.id) == self.chosen) {
                    self.chosen = self.list.first().map(|r| r.id);
                }
            }
            Message::Listed(Err(_)) => {}
            Message::Select(id) => {
                self.chosen = Some(id);
                self.composing = false;
                self.failed = None;
            }
            Message::Compose => {
                self.composing = true;
                self.failed = None;
                if self.form.repository.is_none() {
                    self.form.repository = snapshot.boards.first().map(|b| b.repository.id);
                }
            }
            Message::RepositoryChosen(id) => self.form.repository = Some(id),
            Message::GoalEdited(goal) => self.form.goal = goal,
            Message::MeasureEdited(measure) => self.form.measure = measure,
            Message::BetterChosen(better) => self.form.better = better,
            Message::BudgetEdited(budget) => self.form.budget = budget,
            Message::Store => {
                let (Some(repository), Some(budget)) = (self.form.repository, self.form.budget())
                else {
                    return Task::none();
                };
                if !self.form.ready() {
                    return Task::none();
                }
                self.failed = None;
                let (client, form) = (Client::new(settings.clone()), self.form.clone());
                return Task::perform(
                    async move {
                        client
                            .create_research(
                                repository,
                                form.goal.trim(),
                                form.measure.trim(),
                                form.better,
                                budget,
                            )
                            .await
                    },
                    |result| AppMessage::Experiments(Message::Answered(result)),
                );
            }
            Message::Act(id, action) => {
                self.failed = None;
                let client = Client::new(settings.clone());
                return Task::perform(
                    async move { client.act_on_research(id, action).await },
                    |result| AppMessage::Experiments(Message::Answered(result)),
                );
            }
            Message::Answered(Ok(research)) => {
                self.failed = None;
                if self.composing {
                    self.form = Form {
                        repository: self.form.repository,
                        better: self.form.better,
                        ..Form::default()
                    };
                }
                self.composing = false;
                self.chosen = Some(research.id);
                return self.reload(settings);
            }
            Message::Answered(Err(e)) => self.failed = Some(e.to_string()),
        }
        Task::none()
    }

    pub fn view<'a>(&'a self, snapshot: &'a Snapshot) -> Element<'a, AppMessage> {
        let header = page::header(
            "Improve",
            "Experiments",
            None,
            page::context(
                "A number to improve, a command that measures it, and the levers the Autoresearcher tries.",
            ),
            page::primary_button(
                "New brief",
                (!self.composing).then_some(AppMessage::Experiments(Message::Compose)),
            ),
        );
        let mut detail = column![].spacing(theme::PAGE_GAP).width(Length::Fill);
        if let Some(failed) = &self.failed {
            detail = detail.push(page::boxed(
                "The request was refused",
                text(failed.as_str())
                    .size(theme::BODY)
                    .color(theme::RED_TEXT),
            ));
        }
        let chosen = self.list.iter().find(|r| Some(r.id) == self.chosen);
        detail = match (self.composing, chosen) {
            (true, _) => detail.push(self.form_box(snapshot)),
            (false, Some(research)) => detail
                .push(brief_box(research, repository_name(snapshot, research)))
                .push(baseline_box(research))
                .push(experiments_box(research)),
            (false, None) => detail.push(page::boxed(
                "No research yet",
                page::body("Press \"New brief\" to say what to improve and how to measure it."),
            )),
        };
        column![
            header,
            row![
                scrollable(self.list_view(snapshot)).width(280),
                scrollable(container(detail).padding(iced::Padding::default().right(12)))
                    .width(Length::Fill)
            ]
            .spacing(14)
            .height(Length::Fill),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }

    fn list_view<'a>(&'a self, snapshot: &'a Snapshot) -> Element<'a, AppMessage> {
        if self.list.is_empty() {
            return page::boxed("Research", page::body("None yet."));
        }
        column(self.list.iter().map(|research| {
            research_card(
                research,
                repository_name(snapshot, research),
                !self.composing && self.chosen == Some(research.id),
            )
        }))
        .spacing(8)
        .padding(iced::Padding::default().right(12))
        .into()
    }

    /// The form of a new brief.
    fn form_box<'a>(&'a self, snapshot: &'a Snapshot) -> Element<'a, AppMessage> {
        let form = &self.form;
        let label = |label: &'static str| {
            text(label)
                .size(theme::LABEL)
                .font(theme::manrope(Weight::Semibold))
                .color(theme::SIDEBAR_GROUP)
        };
        let message = AppMessage::Experiments;
        let mut repositories = row![].spacing(8);
        for board in &snapshot.boards {
            let id = board.repository.id;
            repositories = repositories.push(page::chip(
                board.repository.name.clone(),
                form.repository == Some(id),
                false,
                message(Message::RepositoryChosen(id)),
            ));
        }
        let better = row![
            page::chip(
                "Lower is better".to_string(),
                form.better == "lower",
                false,
                message(Message::BetterChosen("lower")),
            ),
            page::chip(
                "Higher is better".to_string(),
                form.better == "higher",
                false,
                message(Message::BetterChosen("higher")),
            ),
        ]
        .spacing(8);
        let store = form
            .ready()
            .then_some(AppMessage::Experiments(Message::Store));
        let content = column![
            label("Repository"),
            repositories.wrap().vertical_spacing(8),
            label("Goal"),
            text_input("What should get better?", &form.goal)
                .on_input(move |goal| message(Message::GoalEdited(goal)))
                .size(theme::BODY)
                .padding([9, 12])
                .style(connect::input_style),
            label("Measure"),
            text_input("cargo bench | tail -1", &form.measure)
                .on_input(move |measure| message(Message::MeasureEdited(measure)))
                .size(theme::CODE)
                .font(theme::MONO)
                .padding([9, 12])
                .style(connect::input_style),
            page::body(
                "A shell command that runs in a worktree. Its result is the last number it prints; a failed command or no number is a failed measurement.",
            ),
            label("Direction"),
            better,
            label("Token budget"),
            text_input(DEFAULT_BUDGET, &form.budget)
                .on_input(move |budget| message(Message::BudgetEdited(budget)))
                .on_submit_maybe(store.clone())
                .size(theme::BODY)
                .padding([9, 12])
                .width(220)
                .style(connect::input_style),
            row![page::white_button("Store the brief", store)].spacing(8),
        ]
        .spacing(10);
        page::boxed("New brief", content)
    }
}

fn repository_name<'a>(snapshot: &'a Snapshot, research: &Research) -> &'a str {
    snapshot
        .boards
        .iter()
        .find(|board| board.repository.id == research.repository_id)
        .map_or("", |board| board.repository.name.as_str())
}

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

fn research_card<'a>(
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
fn brief_box<'a>(research: &'a Research, repository: &'a str) -> Element<'a, AppMessage> {
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
                .push(act("Stop", "stop", false));
        }
        "confirmed" => {
            buttons = buttons
                .push(act("Start now", "start", true))
                .push(act("Stop", "stop", false));
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
        row![
            text(research.goal.as_str())
                .size(theme::CARD_TITLE)
                .font(theme::manrope(Weight::Semibold))
                .width(Length::Fill)
                .wrapping(Wrapping::WordOrGlyph),
            status_tag(&research.status),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
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

fn baseline_box(research: &Research) -> Element<'_, AppMessage> {
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

fn experiments_box(research: &Research) -> Element<'_, AppMessage> {
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
