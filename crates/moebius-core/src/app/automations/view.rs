//! The automations as the page and the API read them: a row with its workflow file over it, its next firing and its
//! latest one. The old routes save and delete the workflow file behind the row from here.

use serde::{Deserialize, Serialize};

use super::cron;
use crate::app::{App, AppError, Result};
use crate::board::Runs;
use crate::store::{Automation, Firing, NewAutomation, Project};

/// How many firings of an automation are listed.
const FIRINGS_LISTED: i64 = 50;

/// An automation as the API shows it: as stored, with the next time it fires and its latest firing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationView {
    #[serde(flatten)]
    pub automation: Automation,
    /// When it fires next, as Unix seconds; none while it is off or when its schedule never comes due.
    pub next_firing: Option<i64>,
    pub last_firing: Option<Firing>,
}

impl App {
    /// A person adds an automation to the project `project`, or replaces the automation `id`; both the workflow file
    /// and its row are saved. The file is in the project's folder (ADR 0025), so a replaced automation stays in the
    /// project that owns it, and a `project` that names another is refused. An automation that is still kept
    /// server-wide, waiting for the migration to give it a project, is saved as its row only.
    pub fn save_automation(
        &self,
        runs: &dyn Runs,
        id: Option<i64>,
        project: Option<i64>,
        new: &NewAutomation,
    ) -> Result<AutomationView> {
        let new = self.check_automation(runs, new)?;
        let previous = match id {
            Some(id) => Some(
                self.store
                    .automation(id)?
                    .ok_or_else(|| AppError::NotFound(format!("automation {id}")))?,
            ),
            None => None,
        };
        if let (Some(previous), Some(project)) = (&previous, project)
            && previous.project_id.is_some_and(|owner| owner != project)
        {
            return Err(AppError::Invalid(
                "an automation cannot move to another project".into(),
            ));
        }
        let id = match (id, previous) {
            (Some(id), Some(previous)) => {
                if let (Some(harness), Some(home)) =
                    (self.harness_repo.get(), self.automation_home(id)?)
                {
                    self.check_automation_place(&home, &new)?;
                    let old_key = self.workflow_key(&home, id, &previous.name)?;
                    // A renamed automation takes the file its new name allows, and leaves the one it had.
                    let key = if previous.name == new.name {
                        old_key.clone()
                    } else {
                        self.derived_workflow_key(&home, id, &new.name)?
                    };
                    let old = if old_key != key {
                        self.automation_workflow(id, &previous.name)?
                    } else {
                        None
                    };
                    let definition = self.automation_definition(&new)?;
                    harness.save_project_workflow(
                        self,
                        &crate::decide::Actor::first_owner(),
                        &home.name,
                        &key,
                        &definition,
                    )?;
                    self.store.set_automation_workflow_key(id, &key)?;
                    // A saved schedule is read from the minute it was saved in, so saving it cannot fire a
                    // minute that came due while the person was editing.
                    self.store.record_project_schedule_firing(
                        home.id,
                        &key,
                        &self.store.local_minute()?,
                    )?;
                    if old.is_some_and(|definition| super::is_automation(&definition)) {
                        harness.delete_project_workflow(
                            self,
                            &crate::decide::Actor::first_owner(),
                            &home.name,
                            &old_key,
                        )?;
                    }
                }
                if !self.store.update_automation(id, &new)? {
                    return Err(AppError::NotFound(format!("automation {id}")));
                }
                id
            }
            (None, None) => {
                let home = match (self.harness_repo.get(), project) {
                    (Some(_), Some(project)) => Some(
                        self.store
                            .project(project)?
                            .ok_or_else(|| AppError::NotFound(format!("project {project}")))?,
                    ),
                    (Some(_), None) => {
                        return Err(AppError::Invalid(
                            "an automation belongs to a project: name its project".into(),
                        ));
                    }
                    (None, _) => None,
                };
                if let Some(home) = &home {
                    self.check_automation_place(home, &new)?;
                }
                let added = self.store.add_automation(
                    &new,
                    &self.store.local_minute()?,
                    home.as_ref().map(|home| home.id),
                )?;
                if let (Some(harness), Some(home)) = (self.harness_repo.get(), &home) {
                    let definition = self.automation_definition(&new)?;
                    let key = self.derived_workflow_key(home, added.id, &new.name)?;
                    if let Err(error) = harness.save_project_workflow(
                        self,
                        &crate::decide::Actor::first_owner(),
                        &home.name,
                        &key,
                        &definition,
                    ) {
                        let _ = self.store.delete_automation(added.id);
                        return Err(error);
                    }
                    self.store.set_automation_workflow_key(added.id, &key)?;
                    self.store.record_project_schedule_firing(
                        home.id,
                        &key,
                        &self.store.local_minute()?,
                    )?;
                }
                added.id
            }
            _ => unreachable!("an id and a previous row come together"),
        };
        self.automation(id)
    }

    /// Refuses an automation that works in a repository that is not one of the repositories of the project that owns
    /// it, with the message of every run of that project's workflows (ADR 0025).
    fn check_automation_place(&self, project: &Project, new: &NewAutomation) -> Result<()> {
        let Some(id) = new.repository else {
            return Ok(());
        };
        let repository = self.repository(id)?;
        self.workflow_run_repository(project, Some(&repository.name))
            .map(|_| ())
    }

    /// Deletes the automation `id` and its firings; the chats it started stay. The workflow file in its project goes
    /// with it, so that nothing is left to fire without a row.
    pub fn delete_automation(&self, id: i64) -> Result<()> {
        let automation = self
            .store
            .automation(id)?
            .ok_or_else(|| AppError::NotFound(format!("automation {id}")))?;
        if let Some(harness) = self.harness_repo.get()
            && let Some(home) = self.automation_home(id)?
            && let Some(definition) = self.automation_workflow(id, &automation.name)?
            && super::is_automation(&definition)
        {
            let key = self.workflow_key(&home, id, &automation.name)?;
            harness.delete_project_workflow(
                self,
                &crate::decide::Actor::first_owner(),
                &home.name,
                &key,
            )?;
        }
        if !self.store.delete_automation(id)? {
            return Err(AppError::NotFound(format!("automation {id}")));
        }
        Ok(())
    }

    /// The automation `id` as the page reads it, with its workflow file over its row.
    pub fn automation(&self, id: i64) -> Result<AutomationView> {
        let automation = self
            .store
            .automation(id)?
            .ok_or_else(|| AppError::NotFound(format!("automation {id}")))?;
        self.automation_view(self.stored_automation(automation)?)
    }

    /// The automations, by name: every automation-shaped workflow of every project, and the rows an upgrade has not
    /// written yet. A row whose file was edited into an ordinary workflow is that workflow, not an automation, and is
    /// left out.
    pub fn automations(&self) -> Result<Vec<AutomationView>> {
        let mut rows = self.store.automations()?;
        let mut views = Vec::new();
        for project in self.store.projects()? {
            for (key, definition) in self.automation_definitions(&project)? {
                let automation = self.ensure_automation(&project, &key, &definition)?;
                rows.retain(|candidate| candidate.id != automation.id);
                views.push(
                    self.automation_view(self.automation_from_workflow(automation, &definition)?)?,
                );
            }
        }
        for automation in rows {
            if self
                .automation_workflow(automation.id, &automation.name)?
                .is_some()
            {
                continue;
            }
            views.push(self.automation_view(self.stored_automation(automation)?)?);
        }
        views.sort_by(|left, right| left.automation.name.cmp(&right.automation.name));
        Ok(views)
    }

    fn automation_view(&self, automation: Automation) -> Result<AutomationView> {
        let now = cron::parse_minute(&self.store.local_minute()?);
        let next = match (automation.enabled, cron::parse(&automation.schedule), now) {
            (true, Ok(schedule), Some(now)) => schedule.next_after(now),
            _ => None,
        };
        let next_firing = match next {
            Some(next) => self
                .store
                .local_minute_to_unix(&cron::format_minute(next))?,
            None => None,
        };
        let last_firing = self.store.firings(automation.id, 1)?.pop();
        Ok(AutomationView {
            automation,
            next_firing,
            last_firing,
        })
    }

    /// The firings of the automation `id`, newest first, each with its chat.
    pub fn firings(&self, id: i64) -> Result<Vec<Firing>> {
        self.automation(id)?;
        Ok(self.store.firings(id, FIRINGS_LISTED)?)
    }
}
