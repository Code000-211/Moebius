//! The automations as the page and the API read them: a row with its workflow file over it, its next firing and its
//! latest one. The old routes save and delete the workflow file behind the row from here.

use serde::{Deserialize, Serialize};

use super::cron;
use crate::app::{App, AppError, Result};
use crate::board::Runs;
use crate::store::{Automation, Firing, NewAutomation};

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
    /// A person adds an automation, or replaces the automation `id`; both the workflow file and its row are saved.
    pub fn save_automation(
        &self,
        runs: &dyn Runs,
        id: Option<i64>,
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
        let id = match (id, previous) {
            (Some(id), Some(previous)) => {
                if let Some(harness) = self.harness_repo.get() {
                    let old_key = self.workflow_key(id, &previous.name)?;
                    // A renamed automation takes the file its new name allows, and leaves the one it had.
                    let key = if previous.name == new.name {
                        old_key.clone()
                    } else {
                        self.derived_workflow_key(id, &new.name)?
                    };
                    let old = if old_key != key {
                        self.automation_workflow(id, &previous.name)?
                    } else {
                        None
                    };
                    let definition = self.automation_definition(&new)?;
                    harness.save_workflow(
                        self,
                        &crate::decide::Actor::Person,
                        &key,
                        &definition,
                    )?;
                    self.store.set_automation_workflow_key(id, &key)?;
                    // A saved schedule is read from the minute it was saved in, so saving it cannot fire a
                    // minute that came due while the person was editing.
                    self.store
                        .record_schedule_firing(&key, &self.store.local_minute()?)?;
                    if old.is_some_and(|definition| super::is_automation(&definition)) {
                        harness.delete_workflow(self, &crate::decide::Actor::Person, &old_key)?;
                    }
                }
                if !self.store.update_automation(id, &new)? {
                    return Err(AppError::NotFound(format!("automation {id}")));
                }
                id
            }
            (None, None) => {
                let added = self
                    .store
                    .add_automation(&new, &self.store.local_minute()?)?;
                if let Some(harness) = self.harness_repo.get() {
                    let definition = self.automation_definition(&new)?;
                    let key = self.derived_workflow_key(added.id, &new.name)?;
                    if let Err(error) = harness.save_workflow(
                        self,
                        &crate::decide::Actor::Person,
                        &key,
                        &definition,
                    ) {
                        let _ = self.store.delete_automation(added.id);
                        return Err(error);
                    }
                    self.store.set_automation_workflow_key(added.id, &key)?;
                    self.store
                        .record_schedule_firing(&key, &self.store.local_minute()?)?;
                }
                added.id
            }
            _ => unreachable!("an id and a previous row come together"),
        };
        self.automation(id)
    }

    /// Deletes the automation `id` and its firings; the chats it started stay.
    pub fn delete_automation(&self, id: i64) -> Result<()> {
        let automation = self
            .store
            .automation(id)?
            .ok_or_else(|| AppError::NotFound(format!("automation {id}")))?;
        if let Some(harness) = self.harness_repo.get()
            && let Some(definition) = self.automation_workflow(id, &automation.name)?
            && super::is_automation(&definition)
        {
            let key = self.workflow_key(id, &automation.name)?;
            harness.delete_workflow(self, &crate::decide::Actor::Person, &key)?;
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

    /// The automations, by name: every automation-shaped workflow, and the rows an upgrade has not written yet. A row
    /// whose file was edited into an ordinary workflow is that workflow, not an automation, and is left out.
    pub fn automations(&self) -> Result<Vec<AutomationView>> {
        let mut rows = self.store.automations()?;
        let mut views = Vec::new();
        for (key, definition) in self.automation_definitions()? {
            let automation = match self.store.automation_with_workflow_key(&key)? {
                Some(row) => {
                    rows.retain(|candidate| candidate.id != row.id);
                    row
                }
                None => {
                    let row = self.ensure_automation(&key, &definition)?;
                    rows.retain(|candidate| candidate.id != row.id);
                    row
                }
            };
            views.push(
                self.automation_view(self.automation_from_workflow(automation, &definition)?)?,
            );
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
