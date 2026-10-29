//! The `moebius` table, through which a script acts on its card:
//!
//! - `moebius.decide(kind, context, options, fail_closed) -> choice, reason`: a decision point (always recorded).
//! - `moebius.start_run(agent, purpose, note) -> run id`: starts a run for the card; `note` is added to its prompt.
//! - `moebius.has_agent(name)`: whether an agent of that name is configured.
//! - `moebius.move(column, note)`: moves the card when the script returns; `note` says why, for the next `code` run.
//! - `moebius.wait(reason)`: leaves the card where it is.
//! - `moebius.publish_change() -> change`: publishes the card's latest successful `code` run on the branch of its
//!   change (once per commit) and returns the change, as `card.change` is.
//! - `moebius.run_checks() -> change`: starts the repository's own checks on the change, if it has any.
//! - `moebius.offer_change()`: publishes the change and offers it to a person; the card moves to "Ready to merge".
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
use super::card::change_table;
use super::engine::Effects;
use super::sandbox::outside_script;
use crate::app::{App, columns};
use crate::decide::Question;
use crate::store::Card;

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
        m.set(
            "start_run",
            lua.create_function(
                move |_, (agent, purpose, note): (String, String, Option<String>)| {
                    let run = outside_script(&clock, || {
                        runs.start(&app, &card_copy, &agent, &purpose, note.as_deref())
                    })
                    .map_err(mlua::Error::runtime)?;
                    fx.borrow_mut().started_run = Some(run.id);
                    Ok(run.id)
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
            self.workflow
                .columns
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>(),
        );
        m.set(
            "move",
            lua.create_function(move |_, (column, note): (String, Option<String>)| {
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
                    app.publish_change(card_id).map_err(mlua::Error::external)?;
                    change_table(&app, lua, card_id)
                })
            })?,
        )?;

        let (app, clock) = (self.app.clone(), deadline.clone());
        m.set(
            "run_checks",
            lua.create_function(move |lua, ()| {
                outside_script(&clock, || {
                    app.run_checks(card_id).map_err(mlua::Error::external)?;
                    change_table(&app, lua, card_id)
                })
            })?,
        )?;

        let (app, fx, clock) = (self.app.clone(), effects.clone(), deadline);
        m.set(
            "offer_change",
            lua.create_function(move |_, ()| {
                let change = outside_script(&clock, || app.offer_change(card_id))
                    .map_err(mlua::Error::external)?;
                let mut fx = fx.borrow_mut();
                fx.moved = Some(columns::READY_TO_MERGE.into());
                fx.move_to = None;
                Ok(change.id)
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
