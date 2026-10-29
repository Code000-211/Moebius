//! The functions of the `moebius` table for a card with several attempts, and the `card` fields that show them:
//!
//! - `moebius.start_runs(agent, n, opts) -> run ids`: starts `n` runs of `agent` (purpose `attempt`), each in a worktree
//!   and on a branch of its own, all from the card's latest `code` branch or the default branch. `opts.models` is a list
//!   of models or model classes the attempts use in turn in place of the agent's, `opts.note` is added to every prompt
//!   and `opts.runner` names the runner they go to.
//! - `moebius.check_attempt(run) -> {status, output}`: judges the attempt against the repository's checks and, test
//!   first, its reproduction test (see `App::check_attempt`).
//! - `moebius.settle_attempts(run, text)`: picks the attempt `run`, which passed, for the reason `text`, or with no run
//!   settles the attempts as a failed round and makes `text` the card's note (see `App::settle_attempts`).
//! - `card.attempts`: the attempts not settled yet; `card.selection`: what the card's selection run picked.

use mlua::{Lua, Table};

use super::Engine;
use super::engine::Effects;
use super::sandbox::outside_script;
use crate::app::read_pick;
use crate::board::Variant;
use crate::store::{Attempt, Card, Run};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

/// The most attempts one call of `moebius.start_runs` starts.
pub(super) const MOST_ATTEMPTS: usize = 8;

impl Engine {
    /// Adds the functions for attempts to the `moebius` table `m` of `card`'s script.
    pub(super) fn attempts_api(
        &self,
        lua: &Lua,
        m: &Table,
        card: &Card,
        effects: Rc<RefCell<Effects>>,
        deadline: Rc<Cell<Instant>>,
    ) -> mlua::Result<()> {
        let (app, runs, fx, card_copy, clock) = (
            self.app.clone(),
            self.runs.clone(),
            effects,
            card.clone(),
            deadline.clone(),
        );
        m.set(
            "start_runs",
            lua.create_function(move |_, (agent, n, opts): (String, i64, Option<Table>)| {
                let n = usize::try_from(n)
                    .ok()
                    .filter(|n| (1..=MOST_ATTEMPTS).contains(n))
                    .ok_or_else(|| {
                        mlua::Error::runtime(format!("n is 1 to {MOST_ATTEMPTS} attempts"))
                    })?;
                let (models, note, runner) = match &opts {
                    Some(opts) => (
                        opts.get::<Option<Vec<String>>>("models")?
                            .unwrap_or_default(),
                        opts.get::<Option<String>>("note")?,
                        opts.get::<Option<String>>("runner")?,
                    ),
                    None => Default::default(),
                };
                let mut started = Vec::new();
                let mut failure = None;
                outside_script(&clock, || {
                    for index in 0..n {
                        let variant = Variant {
                            model: (!models.is_empty())
                                .then(|| models[index % models.len()].as_str()),
                            runner: runner.as_deref(),
                        };
                        let run = runs.start_with(
                            &app,
                            &card_copy,
                            &agent,
                            ("attempt", note.as_deref()),
                            variant,
                        );
                        let recorded = run.and_then(|run| {
                            let group = started.first().copied().unwrap_or(run.id);
                            app.store
                                .add_attempt(run.id, card_copy.id, group, variant.model)
                                .map(|()| run.id)
                                .map_err(|e| e.to_string())
                        });
                        match recorded {
                            Ok(id) => started.push(id),
                            Err(e) => {
                                failure = Some(e);
                                break;
                            }
                        }
                    }
                });
                if let Some(first) = started.first() {
                    fx.borrow_mut().started_run = Some(*first);
                }
                match failure {
                    // What did start stays: the next tick sees those attempts.
                    Some(e) if started.is_empty() => Err(mlua::Error::runtime(e)),
                    Some(e) => Err(mlua::Error::runtime(format!(
                        "{e} (attempts {started:?} were started)"
                    ))),
                    None => Ok(started),
                }
            })?,
        )?;

        let (app, clock) = (self.app.clone(), deadline.clone());
        m.set(
            "check_attempt",
            lua.create_function(move |lua, run: i64| {
                let check = outside_script(&clock, || app.check_attempt(run))
                    .map_err(mlua::Error::external)?;
                let judged = lua.create_table()?;
                judged.set("status", check.status)?;
                judged.set("output", check.output)?;
                Ok(judged)
            })?,
        )?;

        let (app, card_id) = (self.app.clone(), card.id);
        m.set(
            "settle_attempts",
            lua.create_function(move |_, (run, text): (Option<i64>, Option<String>)| {
                app.settle_attempts(card_id, run, text.as_deref().unwrap_or(""))
                    .map_err(mlua::Error::external)
            })?,
        )?;
        Ok(())
    }

    /// Sets `card.attempts` and `card.selection` on `t`, from the card's `runs` (oldest first).
    pub(super) fn attempts_table(
        &self,
        lua: &Lua,
        t: &Table,
        card: &Card,
        runs: &[Run],
    ) -> mlua::Result<()> {
        let open = self
            .app
            .store
            .open_attempts(card.id)
            .map_err(mlua::Error::external)?;
        let list = lua.create_table()?;
        for attempt in &open {
            let Some(run) = runs.iter().find(|r| r.id == attempt.run_id) else {
                continue;
            };
            let a = lua.create_table()?;
            a.set("run", run.id)?;
            a.set("agent", run.agent.as_str())?;
            a.set("model", attempt.model.as_deref())?;
            a.set("status", run.status.as_str())?;
            a.set("result", attempt.result.as_deref())?;
            a.set("output", attempt.output.as_str())?;
            list.push(a)?;
        }
        t.set("attempts", list)?;
        if let Some(selection) = selection(runs, &open) {
            let s = lua.create_table()?;
            s.set("run", selection.run)?;
            s.set("status", selection.status)?;
            s.set("pick", selection.pick)?;
            s.set("reason", selection.reason)?;
            t.set("selection", s)?;
        }
        Ok(())
    }
}

/// What the selection run of the open attempts came to.
struct Selection {
    run: i64,
    status: String,
    pick: Option<i64>,
    reason: String,
}

/// The latest selection run (`review-select`) since the open attempts were started.
fn selection(runs: &[Run], open: &[Attempt]) -> Option<Selection> {
    let group = open.iter().map(|a| a.group_id).min()?;
    let run = runs
        .iter()
        .rev()
        .find(|r| r.purpose == "review-select" && r.id > group)?;
    let (pick, reason) = if run.status == "succeeded" {
        read_pick(run.summary.as_deref())
    } else {
        (None, String::new())
    };
    Some(Selection {
        run: run.id,
        status: run.status.clone(),
        pick,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use crate::app::PERSON;
    use crate::board::engine::Effects;
    use crate::board::testing::{FakeHarness, setup_with_harness};
    use crate::store::Card;

    use super::*;

    /// Runs `body` as the `on_card` of a script for `card` and returns what it did, or why it failed.
    fn run(engine: &Engine, card: &Card, source: &str) -> Result<Effects, String> {
        let effects = Rc::new(RefCell::new(Effects::default()));
        engine
            .run_script(
                "test.luau",
                &format!("function on_card(card)\n{source}\nend"),
                card,
                effects.clone(),
            )
            .map_err(|e| e.to_string())?;
        Ok(effects.take())
    }

    #[test]
    fn start_runs_starts_attempts_in_turn_with_the_models_and_records_them_as_one_group() {
        let harness = Arc::new(FakeHarness::default());
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app.create_issue(repo, "Hard", "", PERSON).unwrap();
        let effects = run(
            &engine,
            &card,
            r#"
            local runs = moebius.start_runs("Coder", 3, { models = { "strong", "fast" }, note = "Be careful." })
            moebius.wait(#runs .. " started")
            "#,
        )
        .unwrap();
        assert_eq!(effects.waiting.as_deref(), Some("3 started"));
        let attempts = app.store.attempts_for_card(card.id).unwrap();
        assert_eq!(
            attempts
                .iter()
                .map(|a| a.model.as_deref())
                .collect::<Vec<_>>(),
            [Some("strong"), Some("fast"), Some("strong")]
        );
        assert!(
            attempts
                .iter()
                .all(|a| a.group_id == attempts[0].run_id && !a.settled)
        );
        assert_eq!(effects.started_run, Some(attempts[0].run_id));
        assert_eq!(
            *harness.models.lock().unwrap(),
            [
                Some("strong".to_string()),
                Some("fast".into()),
                Some("strong".into())
            ]
        );
        assert_eq!(
            *harness.notes.lock().unwrap(),
            vec![Some("Be careful.".to_string()); 3]
        );
        // Each attempt is a run of its own on a branch of its own.
        let runs = app.store.runs_for_card(card.id).unwrap();
        assert!(runs.iter().all(|r| r.purpose == "attempt"));
        let branches: std::collections::BTreeSet<_> = runs.iter().map(|r| r.branch()).collect();
        assert_eq!(branches.len(), 3);

        // The card shows them, and its latest run is not one of them.
        let effects = run(
            &engine,
            &card,
            r#"
            local a = card.attempts
            moebius.wait(#a .. " " .. a[1].model .. " " .. a[2].status .. " " .. tostring(a[3].result) .. " "
              .. tostring(card.run) .. " " .. card.rounds)
            "#,
        )
        .unwrap();
        assert_eq!(
            effects.waiting.as_deref(),
            Some("3 strong succeeded nil nil 0")
        );

        // Without models the agents use their own, and a second call is a group of its own.
        run(&engine, &card, r#"moebius.start_runs("Coder", 1)"#).unwrap();
        let attempts = app.store.attempts_for_card(card.id).unwrap();
        assert_eq!(attempts.len(), 4);
        assert_eq!(attempts[3].model, None);
        assert_eq!(attempts[3].group_id, attempts[3].run_id);
    }

    #[test]
    fn start_runs_refuses_a_number_of_attempts_it_does_not_allow() {
        let (_dir, app, engine, repo) = setup_with_harness(None, Arc::new(FakeHarness::default()));
        let card = app.create_issue(repo, "Hard", "", PERSON).unwrap();
        for n in [0, 9, -1] {
            let error = run(
                &engine,
                &card,
                &format!(r#"moebius.start_runs("Coder", {n})"#),
            )
            .err()
            .expect("the script fails");
            assert!(error.contains("1 to 8 attempts"), "{error}");
        }
        assert!(app.store.runs_for_card(card.id).unwrap().is_empty());
    }

    #[test]
    fn a_card_whose_attempts_all_ended_shows_the_selection_of_the_reviewer() {
        let (_dir, app, engine, repo) = setup_with_harness(None, Arc::new(FakeHarness::default()));
        let card = app.create_issue(repo, "Hard", "", PERSON).unwrap();
        run(&engine, &card, r#"moebius.start_runs("Coder", 2)"#).unwrap();
        let first = app.store.open_attempts(card.id).unwrap()[0].run_id;
        let select = app
            .store
            .add_run(Some(card.id), "Reviewer", "fake", "review-select")
            .unwrap();
        app.store
            .set_run_summary(select.id, &format!("Smaller.\nPICK: {first}"))
            .unwrap();
        let source = r#"
            local s = card.selection
            moebius.wait(s.status .. " " .. tostring(s.pick) .. " " .. s.reason)
        "#;
        let waiting = |engine: &Engine| {
            run(engine, &app.card(card.id).unwrap(), source)
                .unwrap()
                .waiting
        };
        assert_eq!(
            waiting(&engine).as_deref(),
            Some("starting nil "),
            "a run that goes picks nothing yet"
        );
        app.store.set_run_status(select.id, "succeeded").unwrap();
        assert_eq!(
            waiting(&engine),
            Some(format!("succeeded {first} Smaller."))
        );
    }

    #[test]
    fn the_token_budget_counts_the_tokens_of_every_attempt_together() {
        let harness = Arc::new(FakeHarness::default());
        let (_dir, app, _, repo) = setup_with_harness(None, harness);
        let card = app.create_issue(repo, "Hard", "", PERSON).unwrap();
        let engine = crate::board::testing::engine_with(&app, crate::board::Limits::default());
        run(&engine, &card, r#"moebius.start_runs("Coder", 3)"#).unwrap();
        let runs = app.store.open_attempts(card.id).unwrap();
        for attempt in &runs {
            app.store
                .add_usage(&crate::store::Usage {
                    run_id: Some(attempt.run_id),
                    model: "local".into(),
                    input_tokens: 300,
                    output_tokens: 0,
                    cached_tokens: 0,
                    duration_ms: 0,
                    tools: Vec::new(),
                })
                .unwrap();
        }
        let effects = run(&engine, &card, "moebius.wait(tostring(card.tokens))").unwrap();
        assert_eq!(effects.waiting.as_deref(), Some("900"));
        assert_eq!(app.store.tokens_for_card(card.id).unwrap(), 900);
    }
}
