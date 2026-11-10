//! The `schedule` trigger: a workflow whose runs start when a cron comes due, in the server's local time.
//!
//! The cron is the same five fields an automation is written with (see `app::cron`), and it is read when the workflow
//! file loads, so a cron with a field that is wrong is refused then and not at three in the morning. The server's
//! nightly schedule (`server::schedule`) looks at every stored workflow once a minute: the ones whose cron came due
//! since they last fired start a run, and the minute each fired at is in the store, so one occurrence starts one run
//! even when the server restarts in between. It reads each workflow file on its own, so a file that cannot be read is
//! reported and the schedules of the others go on. A run starts with the minute it fired at and the defaults the
//! trigger gives its inputs:
//!
//! ```toml
//! [[trigger]]
//! kind = "schedule"
//! params = { cron = "30 3 * * *", repository = "Moebius", inputs = { goal = "Triage the new issues." } }
//! ```

use serde_json::{Map, Value};
use time::PrimitiveDateTime;

use crate::app::cron;
use crate::workflows::definition::WorkflowDefinition;
use crate::workflows::nodes::Ports;

/// The kind's name in a workflow file.
pub const KIND: &str = "schedule";

/// The key of the minute the run fired at, in the value its run starts with.
const FIRED: &str = "fired";

/// The key of the inputs' defaults, in the value its run starts with.
const INPUTS: &str = "inputs";

/// How far back a firing that came due while the server was not running is still made up. The clock automations look
/// back over at most the same five minutes (see `app::automations`), so a server that was away over the night starts
/// no run for the hours it missed.
const LOOK_BACK: time::Duration = time::Duration::minutes(5);

/// What a workflow's `schedule` trigger asks for.
#[derive(Debug)]
pub struct ScheduleTrigger {
    /// The five cron fields as the file wrote them.
    cron: String,
    /// The repository the runs started by this trigger belong to, by name; `None` when the trigger names none.
    pub repository: Option<String>,
    /// What a run starts its inputs with, as an object of the input names and their defaults.
    inputs: Value,
    /// Whether the schedule is on; a switched-off schedule never comes due.
    enabled: bool,
}

impl ScheduleTrigger {
    /// Reads the trigger's parameters: its `cron`, the `repository` its runs belong to, its `inputs` defaults and
    /// whether it is `enabled`. A cron the clock cannot read is refused with what is wrong with it, naming which
    /// field it is in.
    pub fn parse(params: &Value) -> Result<Self, String> {
        let invalid = |message: &str| format!("the {KIND} trigger {message}");
        let table = params
            .as_object()
            .ok_or_else(|| invalid("takes an object of parameters"))?;
        let unknown: Vec<&str> = table
            .keys()
            .filter(|key| !["cron", "repository", "inputs", "enabled"].contains(&key.as_str()))
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            return Err(format!("unknown trigger parameters {unknown:?}"));
        }
        let cron = match table.get("cron") {
            Some(Value::String(cron)) => cron.clone(),
            Some(_) => return Err(invalid("\"cron\" must be five cron fields")),
            None => return Err(invalid("needs a \"cron\" of five fields")),
        };
        // Reading the cron is what checks it; the message it answers with says which field is wrong.
        cron::parse(&cron).map_err(|field| format!("\"cron\": {field}"))?;
        let repository = match table.get("repository") {
            None | Some(Value::Null) => None,
            Some(Value::String(repository)) if !repository.is_empty() => Some(repository.clone()),
            Some(_) => {
                return Err(invalid(
                    "\"repository\" must name the repository the runs belong to",
                ));
            }
        };
        let inputs = match table.get("inputs") {
            None | Some(Value::Null) => Value::Object(Map::new()),
            Some(Value::Object(fields)) => Value::Object(fields.clone()),
            Some(_) => {
                return Err(invalid(
                    "\"inputs\" must be a table of input names and their defaults",
                ));
            }
        };
        let enabled = match table.get("enabled") {
            None => true,
            Some(Value::Bool(enabled)) => *enabled,
            Some(_) => return Err(invalid("\"enabled\" must be true or false")),
        };
        Ok(Self {
            cron,
            repository,
            inputs,
            enabled,
        })
    }

    /// The trigger has no input and one output, `out`, carrying what the firing gives the run.
    pub fn ports(&self) -> Ports {
        Ports::new(Vec::<String>::new(), ["out"])
    }

    /// The firing that came due by `now`: the one nearest the minute just reached, after the minute `fired_at` fired
    /// at, when it has come within what [`LOOK_BACK`] reaches back. `fired_at` is when the schedule last fired, or
    /// `None` before it first does, when the schedule is read from five minutes back.
    ///
    /// Reading from five minutes back makes up the latest firing that came due in that reach and no older one. A
    /// schedule finer than the reach therefore starts on the minute just reached rather than trailing the firings
    /// before it, and a schedule coarser than it still makes up one that came due a couple of minutes late.
    ///
    /// The minutes are the server's local ones, read as the clock reads them, so the season change the store reasons
    /// about (see `store::automations`) fires a schedule once and not twice: the minute a fall-back day reads twice
    /// is never later than the minute already fired at.
    pub fn due(
        &self,
        fired_at: Option<PrimitiveDateTime>,
        now: PrimitiveDateTime,
    ) -> Option<PrimitiveDateTime> {
        if !self.enabled {
            return None;
        }
        let schedule = cron::parse(&self.cron).ok()?;
        let earliest = now - LOOK_BACK;
        let since = fired_at.map_or(earliest, |fired| fired.max(earliest));
        let mut firing = schedule.next_after(since)?;
        if firing > now {
            return None;
        }
        while let Some(later) = schedule.next_after(firing) {
            if later > now {
                break;
            }
            firing = later;
        }
        Some(firing)
    }

    /// The value a run started by the firing at `at` begins with: the minute it fired at and the inputs' defaults.
    pub fn firing_value(&self, at: PrimitiveDateTime) -> Value {
        Value::Object(Map::from_iter([
            (FIRED.to_string(), Value::String(cron::format_minute(at))),
            (INPUTS.to_string(), self.inputs.clone()),
        ]))
    }
}

/// The schedule a stored workflow asks for, or `None` when its trigger is another kind. A file whose cron cannot be
/// read asks for none: it was refused when it loaded, and the schedule of every other workflow goes on.
pub fn of(definition: &WorkflowDefinition) -> Option<ScheduleTrigger> {
    let trigger = definition.trigger.first()?;
    if trigger.kind != KIND {
        return None;
    }
    ScheduleTrigger::parse(&trigger.params).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::macros::datetime;

    fn trigger(cron: &str) -> ScheduleTrigger {
        ScheduleTrigger::parse(&json!({ "cron": cron })).unwrap()
    }

    #[test]
    fn the_trigger_is_read_from_its_parameters() {
        let schedule = ScheduleTrigger::parse(&json!({
            "cron": "30 3 * * *",
            "repository": "Moebius",
            "inputs": { "goal": "Triage the new issues." },
        }))
        .unwrap();
        assert_eq!(schedule.cron, "30 3 * * *");
        assert_eq!(schedule.repository.as_deref(), Some("Moebius"));
        assert_eq!(schedule.inputs, json!({ "goal": "Triage the new issues." }));
    }

    #[test]
    fn a_trigger_with_nothing_but_a_cron_has_no_repository_and_no_inputs() {
        let schedule = trigger("0 3 1 * *");
        assert_eq!(schedule.repository, None);
        assert_eq!(schedule.inputs, json!({}));
    }

    #[test]
    fn a_cron_that_is_not_five_fields_is_refused_saying_so() {
        let error = ScheduleTrigger::parse(&json!({ "cron": "30 3 * *" })).unwrap_err();
        assert!(
            error.contains("five fields"),
            "the cron is refused saying {error}"
        );
    }

    #[test]
    fn a_cron_with_a_field_out_of_range_is_refused_naming_that_field() {
        // Each field is named by the cron's own reader, and the message says which one it is in.
        for (cron, field) in [
            ("60 3 * * *", "minute"),
            ("30 24 * * *", "hour"),
            ("30 3 32 * *", "day of the month"),
            ("30 3 * 13 *", "month"),
            ("30 3 * * 8", "day of the week"),
        ] {
            let error = ScheduleTrigger::parse(&json!({ "cron": cron })).unwrap_err();
            assert!(error.contains(field), "{cron} is refused saying {error}");
        }
    }

    #[test]
    fn a_cron_that_is_not_a_field_of_numbers_is_refused_naming_the_field() {
        let error = ScheduleTrigger::parse(&json!({ "cron": "30 three * * *" })).unwrap_err();
        assert!(error.contains("hour"), "the hour is named: {error}");
        let error = ScheduleTrigger::parse(&json!({ "cron": "*/0 3 * * *" })).unwrap_err();
        assert!(error.contains("minute"), "the minute is named: {error}");
    }

    #[test]
    fn a_parameter_that_is_not_one_of_the_triggers_own_is_refused_and_named() {
        let error =
            ScheduleTrigger::parse(&json!({ "cron": "* * * * *", "count": 3 })).unwrap_err();
        assert!(error.contains("\"count\""), "{error}");
    }

    #[test]
    fn a_parameter_that_is_not_an_object_is_refused() {
        let error = ScheduleTrigger::parse(&json!([])).unwrap_err();
        assert!(error.contains("object"), "{error}");
    }

    #[test]
    fn a_cron_that_is_not_a_string_or_is_missing_is_refused() {
        let error = ScheduleTrigger::parse(&json!({ "cron": 30 })).unwrap_err();
        assert!(error.contains("cron"), "{error}");
        let error = ScheduleTrigger::parse(&json!({})).unwrap_err();
        assert!(error.contains("cron"), "{error}");
        let error =
            ScheduleTrigger::parse(&json!({ "cron": "* * * * *", "inputs": [] })).unwrap_err();
        assert!(error.contains("inputs"), "{error}");
        let error =
            ScheduleTrigger::parse(&json!({ "cron": "* * * * *", "repository": 7 })).unwrap_err();
        assert!(error.contains("repository"), "{error}");
    }

    #[test]
    fn a_run_starts_with_the_minute_it_fired_at_and_the_inputs_defaults() {
        let schedule = ScheduleTrigger::parse(&json!({
            "cron": "30 3 * * *",
            "inputs": { "goal": "Triage the new issues." },
        }))
        .unwrap();
        assert_eq!(
            schedule.firing_value(datetime!(2026-11-09 03:30:00)),
            json!({
                "fired": "2026-11-09 03:30",
                "inputs": { "goal": "Triage the new issues." },
            })
        );
    }

    #[test]
    fn a_schedule_is_on_unless_it_says_it_is_off() {
        assert!(trigger("* * * * *").enabled);
        let off =
            ScheduleTrigger::parse(&json!({ "cron": "* * * * *", "enabled": false })).unwrap();
        assert!(!off.enabled);
        assert_eq!(off.due(None, datetime!(2026-11-09 03:30:00)), None);
        let error =
            ScheduleTrigger::parse(&json!({ "cron": "* * * * *", "enabled": "no" })).unwrap_err();
        assert!(error.contains("enabled"), "{error}");
    }

    #[test]
    fn a_schedule_fires_once_for_each_occurrence() {
        let schedule = trigger("30 3 * * *");
        // Nothing has fired yet, and three-thirty has come: the schedule is due.
        let now = datetime!(2026-11-09 03:30:00);
        assert_eq!(schedule.due(None, now), Some(now));
        // The minute it fired at is recorded, so the same occurrence is due no more.
        assert_eq!(schedule.due(Some(now), now), None);
        // The next night it is due again.
        let next = datetime!(2026-11-10 03:30:00);
        assert_eq!(schedule.due(Some(now), next), Some(next));
    }

    #[test]
    fn a_schedule_finer_than_the_look_back_fires_the_minute_just_reached() {
        // Every minute is finer than the five-minute reach, so it starts on the minute it is read at and does not
        // trail four minutes behind the firings that came before it.
        let schedule = trigger("* * * * *");
        let now = datetime!(2026-11-09 03:30:00);
        assert_eq!(schedule.due(None, now), Some(now));
        // The next minute is the one it fires at, even when the check came a few minutes late.
        assert_eq!(
            schedule.due(Some(now), datetime!(2026-11-09 03:31:00)),
            Some(datetime!(2026-11-09 03:31:00))
        );
        assert_eq!(
            schedule.due(Some(now), datetime!(2026-11-09 03:35:00)),
            Some(datetime!(2026-11-09 03:35:00))
        );
    }

    #[test]
    fn a_firing_that_came_due_while_the_server_was_not_running_is_made_up_briefly() {
        let schedule = trigger("30 3 * * *");
        // The server woke two minutes after the firing came due, and makes it up.
        assert_eq!(
            schedule.due(None, datetime!(2026-11-09 03:32:00)),
            Some(datetime!(2026-11-09 03:30:00))
        );
        // A week later the night it missed is not made up: the schedule waits for the next one.
        assert_eq!(schedule.due(None, datetime!(2026-11-16 09:00:00)), None);
    }

    #[test]
    fn the_minute_a_fall_back_day_reads_twice_fires_once() {
        // The same reasoning as the store's reading of a fall-back day: a minute the clock has gone back over is
        // the minute still to come, never one that has passed again. A schedule read at the minute it already fired
        // at waits, whether the clock reached it for the first time or the second.
        let schedule = trigger("30 1 * * *");
        let repeated = datetime!(2026-11-01 01:30:00);
        assert_eq!(schedule.due(Some(repeated), repeated), None);
        // Going forward over the season change is the same: the hour a spring-forward day skips waits too.
        let schedule = trigger("30 2 * * *");
        let fired = datetime!(2026-03-08 02:30:00);
        assert_eq!(
            schedule.due(Some(fired), datetime!(2026-03-08 03:30:00)),
            None
        );
    }

    #[test]
    fn the_next_firing_is_the_next_minute_the_cron_came_due() {
        let schedule = trigger("*/15 * * * *");
        let fired = datetime!(2026-11-09 03:30:00);
        assert_eq!(
            schedule.due(Some(fired), datetime!(2026-11-09 03:45:00)),
            Some(datetime!(2026-11-09 03:45:00))
        );
        // A minute the quarter-hour has not reached yet is not due.
        assert_eq!(
            schedule.due(Some(fired), datetime!(2026-11-09 03:44:00)),
            None
        );
    }

    #[test]
    fn a_workflow_asks_for_a_schedule_only_when_its_trigger_is_one() {
        let definition = WorkflowDefinition::parse(
            r#"
name = "nightly-triage"
description = "Reads the board each night."

[[trigger]]
kind = "schedule"
params = { cron = "30 3 * * *" }

[[nodes]]
id = "sweep"
kind = "other"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#,
            "workflows/nightly-triage.toml",
        )
        .unwrap();
        assert_eq!(of(&definition).unwrap().cron, "30 3 * * *");

        let definition = WorkflowDefinition::parse(
            r#"
name = "on-card"
description = "Started by a card."

[[trigger]]
kind = "other"

[[nodes]]
id = "sweep"
kind = "other"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#,
            "workflows/on-card.toml",
        )
        .unwrap();
        assert!(of(&definition).is_none());
    }

    #[test]
    fn a_workflow_with_a_cron_that_is_wrong_is_refused_when_its_file_loads() {
        let text = r#"
name = "nightly-triage"
description = "Reads the board each night."

[[trigger]]
kind = "schedule"
params = { cron = "70 * * * *", repository = "Moebius" }

[[nodes]]
id = "sweep"
kind = "other"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#;
        let error = crate::workflows::definition::parse("nightly-triage", text).unwrap_err();
        assert!(
            error.to_string().contains("minute"),
            "the wrong field is named: {error}"
        );
    }

    #[test]
    fn a_workflow_whose_cron_cannot_be_read_asks_for_no_schedule() {
        // Its file was refused when it loaded; the schedules of the other workflows go on regardless.
        let definition = WorkflowDefinition::parse(
            r#"
name = "nightly-triage"
description = "Reads the board each night."

[[trigger]]
kind = "schedule"
params = { cron = "70 * * * *" }

[[nodes]]
id = "sweep"
kind = "other"

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
"#,
            "workflows/nightly-triage.toml",
        )
        .unwrap();
        assert!(of(&definition).is_none());
    }
}
