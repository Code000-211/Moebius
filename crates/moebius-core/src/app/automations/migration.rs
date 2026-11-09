//! The upgrade that writes every older automation's row to a workflow file, so that an automation is a workflow from
//! then on and no row needs its own clock.

use crate::app::{App, Result};
use crate::store::{Automation, NewAutomation};

impl App {
    /// Writes an older automation's row to a workflow file, so that an upgrade keeps every automation and its place
    /// in the harness configuration repository. A file that already holds the automation is left alone, and so is one
    /// a person edited into an ordinary workflow; an automation whose name a workflow already holds gets a file of its
    /// own beside it.
    pub fn migrate_automations(&self) -> Result<()> {
        if self.harness_repo.get().is_none() {
            return Ok(());
        }
        for automation in self.store.automations()? {
            // The row has been read up to its checked minute, so the workflow starts from there rather than from
            // the five minutes before now: the minute an upgrade happens in cannot fire a second time.
            let checked = self
                .store
                .automation_checked(automation.id)?
                .unwrap_or(self.store.local_minute()?);
            let new = NewAutomation {
                name: automation.name.clone(),
                schedule: automation.schedule.clone(),
                agent: automation.agent.clone(),
                prompt: automation.prompt.clone(),
                repository: automation.repository_id,
                harness: automation.harness_repository,
                enabled: automation.enabled,
            };
            match self.store.automation_workflow_key(automation.id)? {
                Some(key) => match self.automation_workflow(automation.id, &automation.name)? {
                    Some(definition) if super::is_automation(&definition) => {
                        if self.store.schedule_firing(&key)?.is_none() {
                            self.store.record_schedule_firing(&key, &checked)?;
                        }
                    }
                    // A file the row remembers was filled with an ordinary workflow: it is that workflow now.
                    Some(_) => {}
                    // The file the row remembers is gone: write it again so that its schedule sees it.
                    None => self.write_automation(&automation, &new, &key, &checked)?,
                },
                // An older row has no file of its own yet. Its name may already be another row's file, which its
                // key accounts for; writing it makes the file it gets its own, whichever row wrote it before.
                None => {
                    let key = self.derived_workflow_key(automation.id, &automation.name)?;
                    self.write_automation(&automation, &new, &key, &checked)?;
                }
            }
        }
        Ok(())
    }

    /// Writes the row of `automation` to the workflow file `key`, remembers that file, and reads its schedule from
    /// the minute the row was read up to.
    fn write_automation(
        &self,
        automation: &Automation,
        new: &NewAutomation,
        key: &str,
        checked: &str,
    ) -> Result<()> {
        let harness = self.harness_repo()?;
        let definition = self.automation_definition(new)?;
        harness.save_workflow(self, &crate::decide::Actor::Person, key, &definition)?;
        self.store.set_automation_workflow_key(automation.id, key)?;
        self.store.record_schedule_firing(key, checked)?;
        Ok(())
    }
}
