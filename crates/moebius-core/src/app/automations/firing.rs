//! Firing an automation: starts its workflow run (or the chat itself when it names no repository), records the firing
//! and publishes it, and the schedule's check that ends answered chats and fires a row no workflow file holds yet.

use std::sync::Arc;

use serde_json::{Map, Value};
use time::{Duration, PrimitiveDateTime};

use super::{AUTOMATION_ORIGIN, cron, is_automation};
use crate::app::{App, AppError, Result};
use crate::blobs::redact;
use crate::board::Runs;
use crate::bus::Event;
use crate::store::{Automation, Firing, Run};
use crate::workflows::definition::WorkflowDefinition;
use crate::workflows::trigger::schedule as schedule_trigger;

/// How far back a firing that came due while the server was not running is still made up, when an automation is
/// fired from its row because no workflow file holds it yet.
const LOOK_BACK: Duration = Duration::minutes(5);

/// Whether the chat `run` of an automation still runs: it has not ended and has not answered and waited since.
fn still_runs(app: &App, run: &Run) -> Result<bool> {
    if run.ended_at.is_some() {
        return Ok(false);
    }
    Ok(!(run.status == "waiting" && app.store.answered_seq(run.id)? > 0))
}

impl App {
    /// A person fires the automation `id` now ("Run now"), whether it is on or not; skipped, as a firing of the
    /// schedule would be, while the chat of the one before still runs.
    pub fn run_automation(self: &Arc<Self>, runs: &dyn Runs, id: i64) -> Result<Firing> {
        let automation = self
            .store
            .automation(id)?
            .ok_or_else(|| AppError::NotFound(format!("automation {id}")))?;
        let automation = self.stored_automation(automation)?;
        let definition = self.definition_of(&automation)?;
        fire(self, runs, &automation, &definition, None, true)
    }

    /// Fires the automation-shaped workflow `definition` because its schedule came due at `at`.
    pub fn fire_automation_definition(
        self: &Arc<Self>,
        runs: &dyn Runs,
        key: &str,
        definition: &WorkflowDefinition,
        at: PrimitiveDateTime,
    ) -> Result<Firing> {
        if !is_automation(definition) {
            return Err(AppError::Invalid(format!(
                "{} is not an automation",
                definition.name
            )));
        }
        let row = self.ensure_automation(key, definition)?;
        let automation = self.automation_from_workflow(row.clone(), definition)?;
        // The row is the shortcut's copy: what the file now says is what the next firing, the page and the CLI see.
        self.store.update_automation(
            row.id,
            &crate::store::NewAutomation {
                name: automation.name.clone(),
                schedule: automation.schedule.clone(),
                agent: automation.agent.clone(),
                prompt: automation.prompt.clone(),
                repository: automation.repository_id,
                harness: automation.harness_repository,
                enabled: automation.enabled,
            },
        )?;
        fire(self, runs, &automation, definition, Some(at), false)
    }
}

/// Fires `automation`: starts its workflow run (whose `chat` node starts the chat) or, for a workflow that names no
/// repository, starts the chat itself; or records why not.
fn fire(
    app: &Arc<App>,
    runs: &dyn Runs,
    automation: &Automation,
    definition: &WorkflowDefinition,
    at: Option<PrimitiveDateTime>,
    manual: bool,
) -> Result<Firing> {
    let named_repository = schedule_trigger::of(definition).and_then(|trigger| trigger.repository);
    let before = app.store.last_started_firing(automation.id)?;
    let running = match before.as_ref() {
        Some(firing) => match (firing.run_id, firing.workflow_run_id) {
            (Some(run_id), _) => app
                .store
                .run(run_id)?
                .map(|run| still_runs(app, &run))
                .transpose()?
                .unwrap_or(false),
            // The chat node of the run has not started its chat yet.
            (None, Some(workflow_run_id)) => app
                .store
                .workflow_run(workflow_run_id)?
                .is_some_and(|run| matches!(run.status.as_str(), "queued" | "running" | "waiting")),
            (None, None) => false,
        },
        None => false,
    };
    let firing = if running {
        let chat = before
            .as_ref()
            .and_then(|f| f.run_id)
            .map(|id| id.to_string())
            .unwrap_or_else(|| "not started yet".into());
        let reason = format!("the chat of the firing before ({chat}) still runs");
        app.store
            .add_firing(automation.id, "skipped", None, None, manual, &reason)?
    } else if let Some(repository_id) = automation.repository_id {
        let value = at
            .and_then(|at| schedule_trigger::of(definition).map(|trigger| trigger.firing_value(at)))
            .unwrap_or_else(|| Value::Object(Map::new()));
        // The firing is recorded before its run starts, so that the run's chat node finds it and links the chat.
        let started = app
            .store
            .add_firing(automation.id, "started", None, None, manual, "")?;
        match app.start_workflow_run_for_firing(
            definition.clone(),
            repository_id,
            value,
            started.id,
        ) {
            Ok(_) => app
                .store
                .firing(started.id)?
                .ok_or_else(|| AppError::NotFound(format!("firing {}", started.id)))?,
            Err(error) => {
                let reason = redact(&error.to_string());
                app.store.fail_firing(started.id, &reason)?
            }
        }
    } else if let Some(repository) = named_repository {
        let reason = format!(
            "the automation {} names the repository {repository}, which the server does not have",
            automation.name
        );
        app.store
            .add_firing(automation.id, "failed", None, None, manual, &reason)?
    } else {
        let started = app.start_chat(
            runs,
            Some(&automation.agent),
            None,
            automation.harness_repository,
            Some(&automation.prompt),
            None,
            None,
            Some(AUTOMATION_ORIGIN),
            (None, None, &[]),
        );
        match started {
            Ok(run) => {
                app.store
                    .add_firing(automation.id, "started", Some(run.id), None, manual, "")?
            }
            Err(e) => {
                let reason = redact(&e.to_string());
                app.store
                    .add_firing(automation.id, "failed", None, None, manual, &reason)?
            }
        }
    };
    app.bus.publish(Event::AutomationFired {
        automation: automation.id,
        firing: firing.id,
        workflow: automation.name.clone(),
        status: firing.status.clone(),
        run: firing.run_id,
        workflow_run: firing.workflow_run_id,
    });
    Ok(firing)
}

/// The schedule's check, once a minute: ends the chats of automations that have answered, and fires an automation
/// that has nowhere its workflow file would be. Where a harness configuration repository is, the workflow file is
/// what fires an automation (see `server::schedule::on_workflow_schedules`), so one automation has one clock there;
/// deleting its file stops it until the next start writes it again.
pub fn on_schedule(app: &Arc<App>, runs: &dyn Runs) -> Result<()> {
    for run_id in app.store.waiting_automation_chats()? {
        // A chat that ended meanwhile cannot be ended again; it is left as it is.
        if app.store.answered_seq(run_id)? > 0 {
            let _ = app.end_chat(runs, run_id);
        }
    }
    let now_text = app.store.local_minute()?;
    let Some(now) = cron::parse_minute(&now_text) else {
        return Ok(());
    };
    let earliest = now - LOOK_BACK;
    let anywhere = app.harness_repo.get().is_some();
    for automation in app.store.automations()? {
        // Where the workflow file of an automation can be, that file is all that fires it, and the row is only its
        // firings' history: firing it from both clocks would fire the automation twice, and a file a person deleted
        // between starts stops the firing until the next start writes it again. Only a server with nowhere to
        // write a file keeps firing its automations from their rows.
        if anywhere || !automation.enabled {
            continue;
        }
        let Ok(schedule) = cron::parse(&automation.schedule) else {
            continue;
        };
        let checked = app
            .store
            .automation_checked(automation.id)?
            .as_deref()
            .and_then(cron::parse_minute)
            // The row has been looked at before now; the first minute after it is the one still to come.
            .map_or(now - Duration::minutes(1), |checked| checked.max(earliest));
        let Some(at) = schedule.next_after(checked).filter(|at| *at <= now) else {
            continue;
        };
        let definition = app.definition_of(&automation)?;
        fire(app, runs, &automation, &definition, Some(at), false)?;
    }
    app.store.set_automations_checked(&now_text)?;
    Ok(())
}
