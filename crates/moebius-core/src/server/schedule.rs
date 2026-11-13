//! The schedule: once a minute, whether the Retro is due, a proposal of the Retro is to be tried, a research is to
//! start, a replay is to run, a lesson is to be retired, the installed Pi packages are to be checked for updates or an automation is to fire (see `app::automations`), the nightly look for newer versions of the installed Moebius packages (`packages::check_if_due`), and the pause of the Pi of each
//! chat that has waited long for its next message (`PiBolt::pause_idle_chats`). It is also where the workflows on a
//! `schedule` trigger are woken (`on_workflow_schedules`), so that everything that happens by itself goes off the
//! one clock. What fails is reported on the bus as `retro_error`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::Server;
use super::loops::every;
use crate::app::App;
use crate::app::cron;
use crate::board::Runs;
use crate::bus::Event;
use crate::harness::repo::HarnessRepo;
use crate::improve::{lessons, replay, research, retro, trial};
use crate::store::WorkflowRun;
use crate::workflows::definition;
use crate::workflows::trigger::schedule;
use time::PrimitiveDateTime;

/// How often the schedule looks at the clock to see whether the Retro is due, trials may start or chats wait idle.
const RETRO_CHECK: Duration = Duration::from_secs(60);

/// Looks at the clock every [`RETRO_CHECK`] until the future is dropped.
pub(super) async fn run(server: Arc<Server>) {
    every(server, RETRO_CHECK, Server::on_schedule).await
}

impl Server {
    /// Starts what is due now: the Retro, trials, research and replays, and pauses the Pi of chats that wait idle.
    fn on_schedule(&self) {
        let runs: Arc<dyn Runs> = self.harness.clone();
        let repo = self.harness_repo.as_ref();
        let report = |result: crate::app::Result<()>| {
            if let Err(e) = result {
                self.app.bus.publish(Event::RetroError {
                    message: e.to_string(),
                });
            }
        };
        let config = self.live.get();
        report(retro::on_schedule(
            &self.app,
            (&runs, &self.live),
            repo,
            config.retro_at,
        ));
        let (window, limit) = (config.trial_window, config.run_limit);
        let regression = config.regression();
        report(trial::on_schedule(
            &self.app,
            &runs,
            repo,
            (window, limit),
            regression,
            config.trial_token_factor,
        ));
        report(research::on_schedule(
            &self.app,
            &runs,
            window,
            research::MEASURE_LIMIT,
        ));
        report(replay::on_schedule(&self.app, window));
        lessons::on_schedule(&self.app);
        report(self.on_automations(&*runs));
        report(self.on_workflow_schedules());
        if let Err(e) =
            crate::server::packages::check_if_due(&self.app, &self.live, self.harness_repo.as_ref())
        {
            self.app.bus.publish(Event::PackageCheckError {
                registry: String::new(),
                message: e.to_string(),
            });
        }
        if let Err(e) = self.app.on_package_schedule(config.retro_at) {
            self.app.bus.publish(Event::HarnessError {
                agent: None,
                message: format!("cannot check the Pi packages for updates: {e}"),
            });
        }
        self.harness.pause_idle_chats(&self.app, Instant::now());
    }

    /// Ends the chats of automations that have answered, at the local minute.
    fn on_automations(&self, runs: &dyn Runs) -> crate::app::Result<()> {
        crate::app::on_automation_schedule(&self.app, runs)
    }

    /// Starts a run of every workflow whose `schedule` trigger came due, at the local minute; an automation's run
    /// reaches its `chat` node and starts the chat there.
    fn on_workflow_schedules(&self) -> crate::app::Result<()> {
        let now = self.app.store.local_minute()?;
        match cron::parse_minute(&now) {
            Some(now) => on_workflow_schedules(
                &self.app,
                self.harness_repo.as_deref(),
                Some(&*self.harness),
                now,
            )
            .map(|_| ()),
            None => Ok(()),
        }
    }
}

/// Wakes the workflows on a `schedule` trigger: every workflow whose cron came due since that workflow last fired
/// starts a run, and the minute it fired at is recorded, so one occurrence starts one run even when the server
/// restarts in between. The runs started come back in name order.
///
/// A firing is recorded before its run starts, so an occurrence goes once whether a run comes of it or not: what is
/// wrong is written to the log and the workflow waits for its next occurrence rather than start runs every minute.
pub(crate) fn on_workflow_schedules(
    app: &Arc<App>,
    harness: Option<&HarnessRepo>,
    runs: Option<&dyn Runs>,
    now: PrimitiveDateTime,
) -> crate::app::Result<Vec<WorkflowRun>> {
    // With no harness configuration repository there are no workflow files, so no workflow is on a schedule.
    let Some(harness) = harness else {
        return Ok(Vec::new());
    };
    let mut started = Vec::new();
    for name in workflow_names(harness.path())? {
        let definition = match definition::read(harness.path(), &name) {
            Ok(definition) => definition,
            Err(error) => {
                app.bus.publish(Event::RetroError {
                    message: error.to_string(),
                });
                continue;
            }
        };
        let Some(trigger) = schedule::of(&definition) else {
            continue;
        };
        let fired_at = app
            .store
            .schedule_firing(&name)?
            .as_deref()
            .and_then(cron::parse_minute);
        let Some(at) = trigger.due(fired_at, now) else {
            continue;
        };
        app.store
            .record_schedule_firing(&name, &cron::format_minute(at))?;
        if crate::app::is_automation(&definition) {
            let Some(runs) = runs else {
                eprintln!("moebius: automation {name} fires on its schedule with no harness");
                continue;
            };
            match app.fire_automation_definition(runs, &name, &definition, at) {
                Ok(firing) => {
                    if let Some(id) = firing.workflow_run_id
                        && let Some(run) = app.store.workflow_run(id)?
                    {
                        started.push(run);
                    }
                }
                Err(why) => eprintln!(
                    "moebius: automation {name} fires on its schedule and starts no run: {why}"
                ),
            }
            continue;
        }
        match start_on_schedule(app, &name, &trigger, at) {
            Ok(run) => started.push(run),
            Err(why) => {
                eprintln!("moebius: workflow {name} fires on its schedule and starts no run: {why}")
            }
        }
    }
    Ok(started)
}

/// The names of the workflow files in `root`, in name order: what the schedule reads, one file at a time, so a file
/// that cannot be read leaves the others alone.
fn workflow_names(root: &std::path::Path) -> crate::app::Result<Vec<String>> {
    let dir = root.join(definition::DIRECTORY);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "toml")
            && let Some(name) = path.file_stem().and_then(|name| name.to_str())
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

/// The run one firing starts, through the one door a trigger has into a run. The trigger's `repository` names the
/// repository the run belongs to; a name the server does not have starts no run, since a run belongs to a
/// repository and not to a guess at one.
fn start_on_schedule(
    app: &Arc<App>,
    name: &str,
    trigger: &schedule::ScheduleTrigger,
    at: PrimitiveDateTime,
) -> crate::app::Result<WorkflowRun> {
    let wanted = trigger.repository.as_deref().ok_or_else(|| {
        crate::app::AppError::Invalid(format!(
            "the {name} workflow's schedule names no repository, and a run belongs to one"
        ))
    })?;
    let repository = app
        .store
        .repositories()?
        .into_iter()
        .find(|repository| repository.name == wanted)
        .ok_or_else(|| {
            crate::app::AppError::Invalid(format!(
                "the {name} workflow's schedule names the repository {wanted}, which the server does not have"
            ))
        })?;
    app.start_workflow_run(name, repository.id, trigger.firing_value(at))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use moebius_plugin_api::Host;
    use serde_json::json;
    use time::macros::datetime;

    use super::*;
    use crate::harness::repo::testing;
    use crate::repos::git;
    use crate::store::Store;

    /// A workflow that fires at half past three every night and ends at once.
    const NIGHTLY: &str = r#"
name = "nightly-triage"
description = "Reads the board each night."

[[trigger]]
kind = "schedule"
params = { cron = "30 3 * * *", repository = "demo", inputs = { goal = "Read the board." } }

[[nodes]]
id = "sweep"
kind = "test"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#;

    /// Another workflow, on the very same minute as the first.
    const WEEKLY: &str = r#"
name = "weekly-sweep"
description = "Reads the board each week."

[[trigger]]
kind = "schedule"
params = { cron = "30 3 * * *", repository = "demo" }

[[nodes]]
id = "sweep"
kind = "test"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#;

    /// A workflow that fires on every minute, finer than the five-minute reach.
    const EVERY_MINUTE: &str = r#"
name = "every-minute"
description = "Reads the board every minute."

[[trigger]]
kind = "schedule"
params = { cron = "* * * * *", repository = "demo" }

[[nodes]]
id = "sweep"
kind = "test"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#;

    /// A workflow that fires at half past one in the morning, the minute a fall-back day reads twice.
    const FALL_BACK: &str = r#"
name = "fall-back"
description = "Reads the board in the hour the clock repeats."

[[trigger]]
kind = "schedule"
params = { cron = "30 1 * * *", repository = "demo" }

[[nodes]]
id = "sweep"
kind = "test"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#;

    /// A schedule whose runs belong to a repository the server has not been given.
    const STRAY: &str = r#"
name = "stray"
description = "Names a repository that is not here."

[[trigger]]
kind = "schedule"
params = { cron = "30 3 * * *", repository = "nowhere" }

[[nodes]]
id = "sweep"
kind = "test"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#;

    /// A server with a repository called `demo`, a harness configuration repository, and one file per workflow.
    fn app_with_workflows(dir: &Path, files: &[(&str, &str)]) -> Arc<App> {
        let origin = dir.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let store = Arc::new(Store::in_memory(&dir.join("blobs")).unwrap());
        let app = Arc::new(App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(1))),
            &dir.join("data"),
        ));
        testing::use_harness(&app, dir);
        app.add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        let folder = app.harness_repo().unwrap().path().join("workflows");
        std::fs::create_dir_all(&folder).unwrap();
        for (name, text) in files {
            std::fs::write(folder.join(format!("{name}.toml")), text).unwrap();
        }
        app
    }

    /// The workflows the server has woken at the local minute `now`.
    fn wake(app: &Arc<App>, now: PrimitiveDateTime) -> Vec<WorkflowRun> {
        let harness = app.harness_repo().ok().map(|repo| &**repo);
        on_workflow_schedules(app, harness, None, now).unwrap()
    }

    /// A server that starts again over the same store, which is all a restart keeps.
    fn restart(app: &Arc<App>) -> Arc<App> {
        let again = Arc::new(App::new(
            app.store.clone(),
            Arc::new(Host::new(Duration::from_secs(1))),
            &app.data_dir,
        ));
        if let Ok(harness) = app.harness_repo() {
            again.set_harness_repo(harness.clone());
        }
        again
    }

    #[test]
    fn a_workflow_whose_schedule_came_due_starts_one_run() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_workflows(dir.path(), &[("nightly-triage", NIGHTLY)]);
        // The minute its cron names, and no run of it yet.
        let started = wake(&app, datetime!(2026-11-09 03:30:00));
        assert_eq!(started.len(), 1, "the schedule fires once");
        let run = &started[0];
        assert_eq!(run.workflow, "nightly-triage");
        assert_eq!(run.trigger, "schedule");
        assert_eq!(run.repository_id, app.store.repositories().unwrap()[0].id);
        // The run starts with the minute it fired at and the inputs' defaults.
        assert_eq!(
            run.value,
            json!({
                "fired": "2026-11-09 03:30",
                "inputs": { "goal": "Read the board." },
            })
        );
        assert_eq!(
            app.store
                .schedule_firing("nightly-triage")
                .unwrap()
                .as_deref(),
            Some("2026-11-09 03:30")
        );
    }

    #[test]
    fn a_schedule_waits_until_its_minute_and_fires_no_twice_for_it() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_workflows(dir.path(), &[("nightly-triage", NIGHTLY)]);
        assert!(wake(&app, datetime!(2026-11-09 03:29:00)).is_empty());
        assert_eq!(wake(&app, datetime!(2026-11-09 03:30:00)).len(), 1);
        // The minutes after it are quiet, and so is the next night until its own minute.
        assert!(wake(&app, datetime!(2026-11-09 03:31:00)).is_empty());
        assert!(wake(&app, datetime!(2026-11-09 04:00:00)).is_empty());
        assert_eq!(wake(&app, datetime!(2026-11-10 03:30:00)).len(), 1);
    }

    #[test]
    fn a_schedule_finer_than_the_look_back_starts_on_the_minute_just_reached() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_workflows(dir.path(), &[("every-minute", EVERY_MINUTE)]);
        let started = wake(&app, datetime!(2026-11-09 03:30:00));
        assert_eq!(started.len(), 1);
        // It fires on the minute it was read at, not on one of the firings four minutes before it.
        assert_eq!(started[0].value["fired"], "2026-11-09 03:30");
        // The next minute it is on the next minute and no longer behind.
        let started = wake(&app, datetime!(2026-11-09 03:31:00));
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].value["fired"], "2026-11-09 03:31");
    }

    #[test]
    fn a_restart_does_not_start_the_same_occurrence_again() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_workflows(dir.path(), &[("nightly-triage", NIGHTLY)]);
        assert_eq!(wake(&app, datetime!(2026-11-09 03:30:00)).len(), 1);
        // The server starts again over the same store, a minute after the firing came due.
        let again = restart(&app);
        assert!(
            wake(&again, datetime!(2026-11-09 03:31:00)).is_empty(),
            "the firing already recorded is what a restart remembers"
        );
        // The next night it fires again, as it does on the server that never stopped.
        assert_eq!(wake(&again, datetime!(2026-11-10 03:30:00)).len(), 1);
    }

    #[test]
    fn two_workflows_on_the_same_schedule_both_fire() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_workflows(
            dir.path(),
            &[("nightly-triage", NIGHTLY), ("weekly-sweep", WEEKLY)],
        );
        let started = wake(&app, datetime!(2026-11-09 03:30:00));
        let names: Vec<&str> = started.iter().map(|run| run.workflow.as_str()).collect();
        assert_eq!(names, ["nightly-triage", "weekly-sweep"]);
        // Each keeps its own firing, so neither stops the other from firing the next night.
        assert_eq!(wake(&app, datetime!(2026-11-10 03:30:00)).len(), 2);
    }

    #[test]
    fn the_minute_a_fall_back_day_reads_twice_fires_once() {
        // The clock goes back an hour on the first of November, so 01:30 is read twice. The reasoning is the
        // store's (see `store::automations`): a minute the clock has gone back over is the minute still to come,
        // so the schedule fires in the first reading and waits for the next night rather than fire again.
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_workflows(dir.path(), &[("fall-back", FALL_BACK)]);
        assert_eq!(wake(&app, datetime!(2026-11-01 01:30:00)).len(), 1);
        // The clock has gone back to 01:00 and reaches 01:30 a second time; the occurrence is spent.
        assert!(wake(&app, datetime!(2026-11-01 01:00:00)).is_empty());
        assert!(wake(&app, datetime!(2026-11-01 01:30:00)).is_empty());
        assert!(wake(&app, datetime!(2026-11-01 02:00:00)).is_empty());
        // The next night it is the minute still to come, and it fires.
        assert_eq!(wake(&app, datetime!(2026-11-02 01:30:00)).len(), 1);
    }

    #[test]
    fn a_workflow_file_that_cannot_be_read_wakes_no_schedule_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        // The cron has a minute that is not a minute, so the file is refused when it is read.
        let app = app_with_workflows(
            dir.path(),
            &[(
                "broken",
                r#"
name = "broken"
description = "Names a minute that does not exist."

[[trigger]]
kind = "schedule"
params = { cron = "70 3 * * *", repository = "demo" }

[[nodes]]
id = "sweep"
kind = "test"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#,
            )],
        );
        let mut events = app.bus.subscribe();
        assert!(wake(&app, datetime!(2026-11-09 03:30:00)).is_empty());
        let error = events.try_recv().expect("the refused file is reported");
        assert_eq!(error.kind, "retro_error");
        assert!(
            error.data["message"].as_str().unwrap().contains("minute"),
            "the minute named in the file is what is said: {}",
            error.data["message"]
        );
        // Nothing of it is remembered: the file stays as it is until the person who wrote it fixes it.
        assert_eq!(app.store.schedule_firing("broken").unwrap(), None);
    }

    #[test]
    fn a_workflow_file_that_cannot_be_read_leaves_the_other_schedules_going() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_workflows(
            dir.path(),
            &[
                ("broken", "not a workflow file"),
                ("nightly-triage", NIGHTLY),
            ],
        );
        let started = wake(&app, datetime!(2026-11-09 03:30:00));
        let names: Vec<&str> = started.iter().map(|run| run.workflow.as_str()).collect();
        assert_eq!(names, ["nightly-triage"]);
        assert_eq!(app.store.schedule_firing("broken").unwrap(), None);
        assert_eq!(
            app.store
                .schedule_firing("nightly-triage")
                .unwrap()
                .as_deref(),
            Some("2026-11-09 03:30")
        );
    }

    #[test]
    fn a_workflow_on_another_trigger_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_workflows(
            dir.path(),
            &[(
                "on-card",
                r#"
name = "on-card"
description = "Started by a card, not by the clock."

[[trigger]]
kind = "test"

[[nodes]]
id = "sweep"
kind = "test"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#,
            )],
        );
        assert!(wake(&app, datetime!(2026-11-09 03:30:00)).is_empty());
        assert_eq!(
            app.store.schedule_firing("on-card").unwrap(),
            None,
            "a workflow that is not on a schedule records no firing"
        );
    }

    #[test]
    fn a_firing_names_a_repository_the_server_does_not_have_starts_no_run() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_workflows(dir.path(), &[("stray", STRAY)]);
        assert!(
            wake(&app, datetime!(2026-11-09 03:30:00)).is_empty(),
            "a run belongs to a repository and not to a guess at one"
        );
        // The occurrence is spent all the same: the workflow waits for its next minute rather than try every
        // minute it is awake.
        assert_eq!(
            app.store.schedule_firing("stray").unwrap().as_deref(),
            Some("2026-11-09 03:30")
        );
    }
}
