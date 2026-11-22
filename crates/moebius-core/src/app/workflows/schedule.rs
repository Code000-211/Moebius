//! When a workflow's schedule last fired and fires next, as the workflow list and the workflow's page show it.
//!
//! Both come from what the scheduler itself uses (`server::schedule`): the trigger's cron in the server's local time
//! (`workflows::trigger::schedule`) and the minute the store says it last fired at (`store::workflow_schedules`). An
//! automation shows the same through its own row (`app::automations`); this is for any workflow with a `schedule`
//! trigger, automation or not.

use serde::Serialize;

use crate::app::{App, Result, cron};
use crate::workflows::definition::WorkflowDefinition;
use crate::workflows::trigger::schedule;

/// When a scheduled workflow last fired and fires next, as Unix seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ScheduleTimes {
    /// The firing that comes next; none while the schedule is off or when it never comes due.
    pub next_firing: Option<i64>,
    /// The occurrence it last fired at; none when it has not fired yet.
    pub last_fired: Option<i64>,
}

impl App {
    /// When the schedule trigger of the workflow `name` of the project `project_id`, whose definition is `definition`,
    /// last fired and fires next; both none for a workflow that is not on a schedule.
    pub fn workflow_schedule_times(
        &self,
        project_id: i64,
        name: &str,
        definition: &WorkflowDefinition,
    ) -> Result<ScheduleTimes> {
        let Some(trigger) = schedule::of(definition) else {
            return Ok(ScheduleTimes::default());
        };
        let last_fired = match self.store.project_schedule_firing(project_id, name)? {
            Some(minute) => self.store.unix_of_minute(&minute)?,
            None => None,
        };
        let next_firing = match cron::parse_minute(&self.store.local_minute()?) {
            Some(now) => trigger
                .next_firing(now, |minute| {
                    self.store
                        .local_minute_exists(&cron::format_minute(minute))
                        .unwrap_or(true)
                })
                .map(|next| self.store.local_minute_to_unix(&cron::format_minute(next)))
                .transpose()?
                .flatten(),
            None => None,
        };
        Ok(ScheduleTimes {
            next_firing,
            last_fired,
        })
    }
}
