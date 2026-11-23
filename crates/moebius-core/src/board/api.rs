//! The `moebius` table, through which a script acts on its card:
//!
//! - `moebius.decide(kind, context, options, fail_closed) -> choice, reason`: a decision point (always recorded).
//! - `moebius.start_run(agent, purpose, note, runner) -> run id or nil`: starts a run for the card, or starts none and returns nil, with the card waiting for `the run limit is reached`, while `moebius.run_limit` runs of cards are going; `note` is added to its
//!   prompt, and `runner` names the runner (`[[runners]]`) it goes to in place of the agent's own.
//! - `moebius.start_runs(agent, n, opts) -> run ids`, `moebius.check_attempt(run)`, `moebius.settle_attempts(run, text)`:
//!   a card tried several times at once (see `attempts`).
//! - `moebius.has_agent(name)`: whether an agent of that name is configured.
//! - `moebius.move(column, note)`: moves the card when the script returns; `note` says why, for the next `code` run.
//! - `moebius.wait(reason)`: leaves the card where it is.
//! - `moebius.close_issue(reason)`: closes the card's issue (on its forge too) and moves the card to Done when the script
//!   returns.
//! - `moebius.archive(card)`: archives the card when the script returns: it leaves the board and stays in the store, in
//!   the issue list and in the analytics. `card` is the script's own card.
//! - `moebius.hold(reason)`: holds the card when the script returns, as a person does, with the script as the holder.
//! - `moebius.notify(kind, text)`: puts the card in the inbox (once while an item of that kind is open), given to the
//!   card's people; unlike `moebius.ask`, the card does not wait.
//! - `moebius.now() -> number`: the time in Unix seconds, to compare with `card.moved_at`.
//! - `moebius.set_label(label, present)`: gives the card's issue the label (`present` true) or takes it away (false)
//!   when the script returns, on its forge too.
//! - `moebius.publish_change() -> changes`: publishes the card's latest successful `code` run on the branch of its
//!   change in each of its repositories (once per commit), without a pull request, which `moebius.offer_change`
//!   opens, and returns the changes, as `card.changes` is.
//! - `moebius.open_pull_request() -> changes`: publishes the changes as `moebius.publish_change` does and opens their
//!   pull requests at once, so the forge runs its checks while the review of the card still reads the changes.
//! - `moebius.run_checks() -> changes`: starts each repository's own checks on its change, if it has any.
//! - `moebius.reproduction() -> table`: what the card's runs name in their summaries as the reproduction test, for test first.
//! - `moebius.run_test(command) -> test`, `moebius.run_test_at_base(command, files) -> test`: run the change's
//!   reproduction test on the change, or on the commit it was made from with the test's files added.
//! - `moebius.offer_change(judgements)`: publishes the changes and offers them to a person; the card moves to "Ready to
//!   merge". The optional `judgements` (text) is attached to the inbox item for the person to read.
//! - `moebius.merge_change(reason) -> commit, failure`: merges the card's changes on their forges, in the order the card
//!   names their repositories, and moves the card to "Done"; the judgement is recorded as the column's script
//!   (`script:<column>`). The optional `reason` says why. A merge that fails stops the rest: the card waits where it is,
//!   marked with an inbox item `merge_stopped`, and the call returns nil and why (see `App::merge_card`).
//! - `moebius.ask(kind, reason)`: puts the card in the inbox for a person (once) and leaves it where it is.
//! - `moebius.stuck(reason)`: puts the card in the inbox as stuck (once).
//! - `moebius.record_outcome(outcome) -> count`: sets the outcome of the card's judgements that have none.
//! - `moebius.runs_active()`, `moebius.run_limit`, `moebius.round_limit`: the limits.
//! - `moebius.agent_budget(name)`, `moebius.tokens_today()`, `moebius.day_budget`: the token budgets (nil when none).
//!
//! What Moebius does on a script's behalf (deciding, starting a run, publishing a change) does not count against the
//! script's time limit (see `sandbox`).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Instant;

use mlua::{Lua, Table};

use super::Engine;
use super::card::changes_table;
use super::engine::Effects;
use super::resolve_column;
use super::sandbox::outside_script;
use crate::app::{App, Reproduction, TestRun, columns};
use crate::decide::{Actor, Question};
use crate::store::Card;

/// What a card waits with when the run limit keeps its run from starting.
pub(super) const RUN_LIMIT_REACHED: &str = "the run limit is reached";

impl Engine {
    /// The `moebius` table for the script of `card`: what the script does to the card goes into `effects`, and the time
    /// Moebius spends on its behalf moves `deadline` on.
    pub(super) fn api(
        &self,
        lua: &Lua,
        card: &Card,
        effects: Rc<RefCell<Effects>>,
        deadline: Rc<Cell<Instant>>,
    ) -> mlua::Result<Table> {
        let m = lua.create_table()?;
        let limits = self.limits();
        m.set("run_limit", limits.run_limit)?;
        m.set("round_limit", limits.round_limit)?;
        m.set("day_budget", limits.day_budget)?;
        let card_id = card.id;

        let (app, clock) = (self.app.clone(), deadline.clone());
        m.set(
            "decide",
            lua.create_function(
                move |_,
                      (kind, context, options, fail_closed): (
                    String,
                    BTreeMap<String, String>,
                    Vec<String>,
                    String,
                )| {
                    let decision = outside_script(&clock, || {
                        app.decisions.decide(&Question {
                            kind,
                            context,
                            options,
                            fail_closed,
                            run_id: None,
                            card_id: Some(card_id),
                        })
                    })
                    .map_err(mlua::Error::external)?;
                    Ok((decision.choice, decision.reason))
                },
            )?,
        )?;

        let (app, runs, fx, card_copy, clock) = (
            self.app.clone(),
            self.runs.clone(),
            effects.clone(),
            card.clone(),
            deadline.clone(),
        );
        let run_limit = limits.run_limit;
        m.set(
            "start_run",
            lua.create_function(
                move |_,
                      (agent, purpose, note, runner): (
                    String,
                    String,
                    Option<String>,
                    Option<String>,
                )| {
                    let run = outside_script(&clock, || {
                        super::may_take(&app, &card_copy)?;
                        if super::free_run_slots(&app, run_limit)? == 0 {
                            return Ok(None);
                        }
                        let run = runs.start(
                            &app,
                            &card_copy,
                            &agent,
                            (&purpose, note.as_deref()),
                            crate::board::Variant {
                                model: None,
                                runner: runner.as_deref(),
                            },
                        )?;
                        // The person it is run for is the issue's assignee on the forge from now on (ADR 0027).
                        app.take_issue(&card_copy, None);
                        Ok(Some(run))
                    });
                    let run = match run {
                        // Someone else took the issue, or this server's agents do not take it: it waits here.
                        Err(crate::board::StartError::LeftAlone(why)) => {
                            fx.borrow_mut().waiting = Some(why);
                            return Ok(None);
                        }
                        other => other.map_err(|e| mlua::Error::runtime(e.to_string()))?,
                    };
                    let Some(run) = run else {
                        // The limit is reached: nothing starts, and the card waits for a slot.
                        fx.borrow_mut().waiting = Some(RUN_LIMIT_REACHED.to_string());
                        return Ok(None);
                    };
                    fx.borrow_mut().started_run = Some(run.id);
                    Ok(Some(run.id))
                },
            )?,
        )?;

        let runs = self.runs.clone();
        m.set(
            "has_agent",
            lua.create_function(move |_, name: String| Ok(runs.has_agent(&name)))?,
        )?;

        let (fx, names) = (
            effects.clone(),
            self.kanban
                .columns
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>(),
        );
        m.set(
            "move",
            lua.create_function(move |_, (column, note): (String, Option<String>)| {
                let column = resolve_column(&column).to_string();
                if !names.contains(&column) {
                    return Err(mlua::Error::runtime(format!("no column {column:?}")));
                }
                let mut fx = fx.borrow_mut();
                fx.move_to = Some(column);
                fx.note = note;
                Ok(())
            })?,
        )?;

        let fx = effects.clone();
        m.set(
            "wait",
            lua.create_function(move |_, reason: String| {
                fx.borrow_mut().waiting = Some(reason);
                Ok(())
            })?,
        )?;

        let (app, clock) = (self.app.clone(), deadline.clone());
        m.set(
            "publish_change",
            lua.create_function(move |lua, ()| {
                outside_script(&clock, || {
                    app.publish_card_changes(card_id, false, None)
                        .map_err(mlua::Error::external)?;
                    changes_table(&app, lua, card_id)
                })
            })?,
        )?;

        let (app, clock) = (self.app.clone(), deadline.clone());
        m.set(
            "open_pull_request",
            lua.create_function(move |lua, ()| {
                outside_script(&clock, || {
                    app.publish_card_changes(card_id, true, None)
                        .map_err(mlua::Error::external)?;
                    changes_table(&app, lua, card_id)
                })
            })?,
        )?;

        let (app, clock) = (self.app.clone(), deadline.clone());
        m.set(
            "run_checks",
            lua.create_function(move |lua, ()| {
                outside_script(&clock, || {
                    app.run_checks(card_id).map_err(mlua::Error::external)?;
                    changes_table(&app, lua, card_id)
                })
            })?,
        )?;

        let (app, clock) = (self.app.clone(), deadline.clone());
        m.set(
            "reproduction",
            lua.create_function(move |lua, ()| {
                let reproduction = outside_script(&clock, || app.reproduction(card_id))
                    .map_err(mlua::Error::external)?;
                let Some(reproduction) = reproduction else {
                    return Ok(None);
                };
                let table = lua.create_table()?;
                match reproduction {
                    Reproduction::Test { command, files } => {
                        table.set("command", command)?;
                        table.set("files", files)?;
                    }
                    Reproduction::Untestable(why) => {
                        table.set("files", Vec::<String>::new())?;
                        table.set("untestable", why)?;
                    }
                }
                Ok(Some(table))
            })?,
        )?;

        let (app, clock) = (self.app.clone(), deadline.clone());
        m.set(
            "run_test",
            lua.create_function(move |lua, command: String| {
                let run = outside_script(&clock, || app.run_test(card_id, &command, None))
                    .map_err(mlua::Error::external)?;
                test_table(lua, run)
            })?,
        )?;

        let (app, clock) = (self.app.clone(), deadline.clone());
        m.set(
            "run_test_at_base",
            lua.create_function(
                move |lua, (command, files): (String, Option<Vec<String>>)| {
                    let files = Some(files.unwrap_or_default());
                    let run = outside_script(&clock, || app.run_test(card_id, &command, files))
                        .map_err(mlua::Error::external)?;
                    test_table(lua, run)
                },
            )?,
        )?;

        self.attempts_api(lua, &m, card, effects.clone(), deadline.clone())?;

        let (app, fx, clock) = (self.app.clone(), effects.clone(), deadline.clone());
        m.set(
            "offer_change",
            lua.create_function(move |_, judgements: Option<String>| {
                let judgements = judgements.filter(|j| !j.trim().is_empty());
                let (changes, moved) = outside_script(&clock, || {
                    app.offer_change_noted(card_id, judgements.as_deref())
                })
                .map_err(mlua::Error::external)?;
                let mut fx = fx.borrow_mut();
                if moved {
                    fx.moved = Some(columns::READY_TO_MERGE.into());
                }
                fx.move_to = None;
                // The own repository's change, or the first when that was merged and nothing new is in it.
                let own = app.card(card_id).map_or(0, |card| card.repository_id);
                Ok(changes
                    .iter()
                    .find(|change| change.repository_id == Some(own))
                    .or(changes.first())
                    .map(|change| change.id))
            })?,
        )?;

        let (app, fx, column, clock) = (
            self.app.clone(),
            effects.clone(),
            card.column.clone(),
            deadline.clone(),
        );
        m.set(
            "merge_change",
            lua.create_function(move |_, reason: Option<String>| {
                let reason = reason.unwrap_or_default();
                let reason = if reason.trim().is_empty() {
                    format!("the script of the {column} column merged it")
                } else {
                    reason
                };
                let by = Actor::Script {
                    column: column.clone(),
                };
                let merge = outside_script(&clock, || app.merge_card(&by, card_id, &reason))
                    .map_err(mlua::Error::external)?;
                let mut fx = fx.borrow_mut();
                fx.move_to = None;
                if let Some((change, why)) = merge.stopped {
                    // The card stays where it is with what is merged marked (ADR 0019).
                    let repository = app
                        .store
                        .repository(change.repository_id.unwrap_or_default())
                        .ok()
                        .flatten()
                        .map_or_else(String::new, |r| r.name);
                    let why = format!("the merge in {repository} failed: {why}");
                    fx.waiting = Some(why.clone());
                    return Ok((None, Some(why)));
                }
                fx.moved = Some(columns::DONE.into());
                fx.waiting = None;
                let own = merge.merged.into_iter().next().map(|(_, commit)| commit);
                Ok((own, None))
            })?,
        )?;

        let (app, fx, title) = (self.app.clone(), effects.clone(), card.title.clone());
        m.set(
            "ask",
            lua.create_function(move |_, (kind, reason): (String, String)| {
                to_inbox_once(&app, card_id, &title, &kind, &reason)?;
                fx.borrow_mut().waiting = Some(reason);
                Ok(())
            })?,
        )?;

        let fx = effects.clone();
        m.set(
            "close_issue",
            lua.create_function(move |_, _reason: Option<String>| {
                fx.borrow_mut().close = true;
                Ok(())
            })?,
        )?;

        let fx = effects.clone();
        m.set(
            "archive",
            lua.create_function(move |_, card: Table| {
                if card.get::<i64>("id")? != card_id {
                    return Err(mlua::Error::runtime(
                        "a script archives its own card, the one it is given",
                    ));
                }
                fx.borrow_mut().archive = true;
                Ok(())
            })?,
        )?;

        let fx = effects.clone();
        m.set(
            "hold",
            lua.create_function(move |_, reason: String| {
                fx.borrow_mut().hold = Some(reason);
                Ok(())
            })?,
        )?;

        let (app, title, people) = (self.app.clone(), card.title.clone(), card.assignees.clone());
        m.set(
            "notify",
            lua.create_function(move |_, (kind, text): (String, String)| {
                notify(&app, card_id, &title, &people, &kind, &text)
            })?,
        )?;

        m.set("now", lua.create_function(|_, ()| Ok(crate::store::now()))?)?;

        let fx = effects.clone();
        m.set(
            "set_label",
            lua.create_function(move |_, (label, present): (String, bool)| {
                if label.trim().is_empty() {
                    return Err(mlua::Error::runtime("a label needs a name"));
                }
                fx.borrow_mut().labels.push((label, present));
                Ok(())
            })?,
        )?;

        let (app, fx, title) = (self.app.clone(), effects.clone(), card.title.clone());
        m.set(
            "stuck",
            lua.create_function(move |_, reason: String| {
                to_inbox_once(&app, card_id, &title, "stuck", &reason)?;
                fx.borrow_mut().waiting = Some(reason);
                Ok(())
            })?,
        )?;

        let app = self.app.clone();
        m.set(
            "record_outcome",
            lua.create_function(move |_, outcome: String| {
                app.store
                    .set_missing_outcomes(card_id, &outcome)
                    .map_err(mlua::Error::external)
            })?,
        )?;

        let (budgets, runs) = (limits.agent_budgets, self.runs.clone());
        m.set(
            "agent_budget",
            lua.create_function(move |_, name: String| {
                Ok(runs
                    .token_budget(&name)
                    .or_else(|| budgets.get(&name).copied()))
            })?,
        )?;

        let app = self.app.clone();
        m.set(
            "tokens_today",
            lua.create_function(move |_, ()| {
                app.store.tokens_today().map_err(mlua::Error::external)
            })?,
        )?;

        let app = self.app.clone();
        m.set(
            "runs_active",
            lua.create_function(move |_, ()| {
                Ok(app
                    .store
                    .active_runs()
                    .map_err(mlua::Error::external)?
                    .iter()
                    .filter(|run| run.card_id.is_some())
                    .count())
            })?,
        )?;

        Ok(m)
    }
}

/// A run of a reproduction test as scripts see it: `status` and `output`.
fn test_table(lua: &Lua, run: TestRun) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("status", run.status)?;
    table.set("output", run.output)?;
    Ok(table)
}

/// Puts the card in the inbox as `kind` for the people it is given to, unless an item of that kind is open for it.
fn notify(
    app: &App,
    card_id: i64,
    title: &str,
    people: &[String],
    kind: &str,
    text: &str,
) -> mlua::Result<()> {
    if app
        .store
        .has_open_inbox_item(card_id, kind)
        .map_err(mlua::Error::external)?
    {
        return Ok(());
    }
    let item = app
        .store
        .add_inbox_item(
            kind,
            Some(card_id),
            title,
            &serde_json::json!({ "reason": text }),
        )
        .map_err(mlua::Error::external)?;
    let mut ids = std::collections::BTreeSet::new();
    for handle in people {
        if let Some(person) = app
            .store
            .person_by_handle(handle)
            .map_err(mlua::Error::external)?
        {
            ids.insert(person.id);
        }
    }
    if !ids.is_empty() {
        app.store
            .set_inbox_assignees(item.id, &ids)
            .map_err(mlua::Error::external)?;
    }
    Ok(())
}

/// Puts the card in the inbox as `kind`, unless an item of that kind is already open for it.
fn to_inbox_once(
    app: &App,
    card_id: i64,
    title: &str,
    kind: &str,
    reason: &str,
) -> mlua::Result<()> {
    let open = app
        .store
        .has_open_inbox_item(card_id, kind)
        .map_err(mlua::Error::external)?;
    if !open {
        app.store
            .add_inbox_item(
                kind,
                Some(card_id),
                title,
                &serde_json::json!({ "reason": reason }),
            )
            .map_err(mlua::Error::external)?;
    }
    Ok(())
}
