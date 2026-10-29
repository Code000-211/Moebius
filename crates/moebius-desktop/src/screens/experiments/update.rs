//! How a message changes the Experiments page: the research read, a brief written and stored, a research confirmed,
//! started or stopped, and a draft's brief edited in place.

use iced::Task;

use crate::app::Message as AppMessage;
use crate::client::{Client, Snapshot};
use crate::screens::live::say;
use crate::settings::Settings;

use super::{BriefEdit, Experiments, Form, Message};

impl Experiments {
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
                self.brief = None;
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
            Message::EditBrief(id) => {
                if let Some(research) = self.list.iter().find(|r| r.id == id) {
                    self.brief = Some(BriefEdit {
                        research: id,
                        form: Form {
                            repository: Some(research.repository_id),
                            goal: research.goal.clone(),
                            measure: research.measure.clone(),
                            better: if research.better == "higher" {
                                "higher"
                            } else {
                                "lower"
                            },
                            budget: research.token_budget.to_string(),
                        },
                        saving: false,
                        error: None,
                    });
                }
            }
            Message::BriefGoal(goal) => self.edit_form(|form| form.goal = goal),
            Message::BriefMeasure(measure) => self.edit_form(|form| form.measure = measure),
            Message::BriefBetter(better) => self.edit_form(|form| form.better = better),
            Message::BriefBudget(budget) => self.edit_form(|form| form.budget = budget),
            Message::SaveBrief => {
                let Some(edit) = self.brief.as_mut().filter(|e| !e.saving && e.form.ready()) else {
                    return Task::none();
                };
                let Some(budget) = edit.form.budget() else {
                    return Task::none();
                };
                edit.saving = true;
                edit.error = None;
                let (client, id, form) = (
                    Client::new(settings.clone()),
                    edit.research,
                    edit.form.clone(),
                );
                return Task::perform(
                    async move {
                        client
                            .edit_research(
                                id,
                                form.goal.trim(),
                                form.measure.trim(),
                                form.better,
                                budget,
                            )
                            .await
                    },
                    |result| AppMessage::Experiments(Message::BriefSaved(result)),
                );
            }
            Message::CancelBrief => self.brief = None,
            Message::BriefSaved(Ok(_)) => {
                self.brief = None;
                return self.reload(settings);
            }
            Message::BriefSaved(Err(e)) => {
                if let Some(edit) = &mut self.brief {
                    edit.saving = false;
                    edit.error = Some(say(&e));
                }
            }
        }
        Task::none()
    }

    /// Changes the brief being edited in place.
    fn edit_form(&mut self, change: impl FnOnce(&mut Form)) {
        if let Some(edit) = &mut self.brief {
            change(&mut edit.form);
        }
    }
}
