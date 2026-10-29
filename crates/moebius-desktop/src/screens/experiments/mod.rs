//! The Experiments screen: the research the server has, a form for a new brief, and for the one chosen its baseline and
//! the experiments, one for each lever the Autoresearcher proposed. A brief is stored as a draft (`POST /v1/research`),
//! then a person confirms it, which lets it run in the trial window, or starts it now; a running research can be
//! stopped. The brief of a draft has a pencil that edits it in place (`PUT /v1/research/{id}`). The server says when
//! something changed with `research_status` and `experiment_done`, which the app answers by reading the screen again.
//!
//! `update` has how a message changes the page, `brief` the form of a new brief and the editor of a draft's, and
//! `research` how a research is shown: its card, brief, baseline and experiments.

mod brief;
mod research;
mod update;

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Research, Snapshot};
use crate::settings::Settings;
use crate::theme;
use crate::widgets::page;

use research::{baseline_box, brief_box, experiments_box, research_card};

pub use research::{change, number};

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
    /// The pencil of a draft's brief: it is edited in place.
    EditBrief(i64),
    BriefGoal(String),
    BriefMeasure(String),
    BriefBetter(&'static str),
    BriefBudget(String),
    SaveBrief,
    CancelBrief,
    /// The server answered the brief edited in place.
    BriefSaved(Result<Research, ClientError>),
}

/// The brief of a draft as it is being edited in place.
struct BriefEdit {
    research: i64,
    form: Form,
    saving: bool,
    error: Option<String>,
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
    brief: Option<BriefEdit>,
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
                | Message::BriefSaved(Err(ClientError::Unauthorized))
        )
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
        detail =
            match (self.composing, chosen) {
                (true, _) => detail.push(self.form_box(snapshot)),
                (false, Some(research)) => detail
                    .push(self.brief_editor(research).unwrap_or_else(|| {
                        brief_box(research, repository_name(snapshot, research))
                    }))
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
}

fn repository_name<'a>(snapshot: &'a Snapshot, research: &Research) -> &'a str {
    snapshot
        .boards
        .iter()
        .find(|board| board.repository.id == research.repository_id)
        .map_or("", |board| board.repository.name.as_str())
}
