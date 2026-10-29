//! Writing a brief: the form of a new one, and the editor that takes the place of a draft's brief. Both have the same
//! goal, measure, direction and budget.

use iced::Element;
use iced::font::Weight;
use iced::widget::{TextInput, column, row, text, text_input};

use crate::app::Message as AppMessage;
use crate::client::{Research, Snapshot};
use crate::screens::connect;
use crate::theme;
use crate::widgets::inline_edit;
use crate::widgets::page;

use super::{DEFAULT_BUDGET, Experiments, Message};

/// The field of the goal.
fn goal_input<'a>(goal: &'a str, on: fn(String) -> Message) -> TextInput<'a, AppMessage> {
    text_input("What should get better?", goal)
        .on_input(move |goal| AppMessage::Experiments(on(goal)))
        .size(theme::BODY)
        .padding(theme::FIELD_PADDING)
        .style(connect::input_style)
}

/// The field of the command that measures, in the code font.
fn measure_input<'a>(measure: &'a str, on: fn(String) -> Message) -> TextInput<'a, AppMessage> {
    text_input("cargo bench | tail -1", measure)
        .on_input(move |measure| AppMessage::Experiments(on(measure)))
        .size(theme::CODE)
        .font(theme::MONO)
        .padding(theme::FIELD_PADDING)
        .style(connect::input_style)
}

/// The chips of the direction, "lower" or "higher" is better.
fn better_chips<'a>(better: &str, on: fn(&'static str) -> Message) -> Element<'a, AppMessage> {
    row![
        page::chip(
            "Lower is better".to_string(),
            better == "lower",
            false,
            AppMessage::Experiments(on("lower")),
        ),
        page::chip(
            "Higher is better".to_string(),
            better == "higher",
            false,
            AppMessage::Experiments(on("higher")),
        ),
    ]
    .spacing(theme::SPACE_8)
    .into()
}

/// The field of the token budget.
fn budget_input<'a>(budget: &'a str, on: fn(String) -> Message) -> TextInput<'a, AppMessage> {
    text_input(DEFAULT_BUDGET, budget)
        .on_input(move |budget| AppMessage::Experiments(on(budget)))
        .size(theme::BODY)
        .padding(theme::FIELD_PADDING)
        .width(theme::FIELD_WIDTH)
        .style(connect::input_style)
}

impl Experiments {
    /// The editor in place of the brief of `research`, if it is the one being edited.
    pub(super) fn brief_editor(&self, research: &Research) -> Option<Element<'_, AppMessage>> {
        let edit = self.brief.as_ref().filter(|e| e.research == research.id)?;
        let message = AppMessage::Experiments;
        let form = &edit.form;
        let fields = column![
            goal_input(&form.goal, Message::BriefGoal),
            measure_input(&form.measure, Message::BriefMeasure),
            better_chips(form.better, Message::BriefBetter),
            budget_input(&form.budget, Message::BriefBudget),
        ]
        .spacing(theme::SPACE_8);
        let save = (!edit.saving && form.ready()).then_some(message(Message::SaveBrief));
        Some(page::boxed(
            "Brief",
            inline_edit::editor(
                fields.into(),
                save,
                message(Message::CancelBrief),
                edit.error.as_deref(),
            ),
        ))
    }

    /// The form of a new brief.
    pub(super) fn form_box<'a>(&'a self, snapshot: &'a Snapshot) -> Element<'a, AppMessage> {
        let form = &self.form;
        let label = |label: &'static str| {
            text(label)
                .size(theme::LABEL)
                .font(theme::manrope(Weight::Semibold))
                .color(theme::TEXT_SECONDARY)
        };
        let mut repositories = row![].spacing(theme::SPACE_8);
        for board in &snapshot.boards {
            let id = board.repository.id;
            repositories = repositories.push(page::chip(
                board.repository.name.clone(),
                form.repository == Some(id),
                false,
                AppMessage::Experiments(Message::RepositoryChosen(id)),
            ));
        }
        let store = form
            .ready()
            .then_some(AppMessage::Experiments(Message::Store));
        let content = column![
            label("Repository"),
            repositories.wrap().vertical_spacing(theme::SPACE_8),
            label("Goal"),
            goal_input(&form.goal, Message::GoalEdited),
            label("Measure"),
            measure_input(&form.measure, Message::MeasureEdited),
            page::body(
                "A shell command run in a worktree. Its result is the last number it prints; no number is a failed measurement.",
            ),
            label("Direction"),
            better_chips(form.better, Message::BetterChosen),
            label("Token budget"),
            budget_input(&form.budget, Message::BudgetEdited).on_submit_maybe(store.clone()),
            row![page::white_button("Store the brief", store)].spacing(theme::SPACE_8),
        ]
        .spacing(theme::SPACE_8);
        page::boxed("New brief", content)
    }
}
