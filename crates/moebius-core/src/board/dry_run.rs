//! A dry run of a column script (`check_script`, for `moebius check-script`): the script runs once in the engine's
//! sandbox against a made-up card, with a `moebius` table that has every function of the real one but records what
//! the script does instead of doing it.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use mlua::{Lua, Table};

use super::sandbox::run_sandboxed;
use super::{Limits, Workflow};
use crate::app::PERSON;

/// Runs the column script `source`, called `name`, once in the same sandbox as the engine, on a made-up card in
/// `column` with the given title and author, and returns what it did, one line for each call that has an effect, in
/// order. Nothing is stored and no run is started: the `moebius` table records its calls. `decide` takes the
/// fail-closed option, `has_agent` says yes, and the card has no runs, no change and no blockers. Returns the error
/// as the engine would report it when the script fails.
pub fn check_script(
    name: &str,
    source: &str,
    column: &str,
    title: &str,
    author: &str,
) -> Result<Vec<String>, String> {
    let limits = Limits::default();
    let columns: Vec<String> = Workflow::default()
        .columns
        .into_iter()
        .map(|c| c.name)
        .collect();
    if !columns.contains(&column.to_string()) {
        return Err(format!(
            "no column {column:?}; the columns are {}",
            columns.join(", ")
        ));
    }
    let did = Rc::new(RefCell::new(Vec::new()));
    run_sandboxed(
        name,
        source,
        &limits,
        |lua, _| recording_api(lua, &limits, &columns, did.clone()),
        |lua| made_up_card(lua, column, title, author),
    )
    .map_err(|e| e.to_string())?;
    Ok(did.take())
}

/// The card of a dry run: one that has just been created, with nothing done for it yet.
fn made_up_card(lua: &Lua, column: &str, title: &str, author: &str) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("id", 1)?;
    t.set("number", 1)?;
    t.set("title", title)?;
    t.set("body", "")?;
    t.set("author", author)?;
    t.set("column", column)?;
    t.set("trusted", author == PERSON)?;
    t.set("confirmed", false)?;
    t.set("blocked_by", lua.create_table()?)?;
    t.set("rounds", 0)?;
    t.set("reviews", 0)?;
    t.set("tokens", 0)?;
    t.set("stopped", false)?;
    Ok(t)
}

/// A `moebius` table with every function of the real one, which records what a script does in `did` and changes
/// nothing.
fn recording_api(
    lua: &Lua,
    limits: &Limits,
    columns: &[String],
    did: Rc<RefCell<Vec<String>>>,
) -> mlua::Result<Table> {
    let m = lua.create_table()?;
    m.set("run_limit", limits.run_limit)?;
    m.set("round_limit", limits.round_limit)?;
    m.set("day_budget", limits.day_budget)?;
    let record = move |line: String| did.borrow_mut().push(line);
    let with_note = |note: Option<String>| note.map_or(String::new(), |n| format!(" (note: {n})"));

    let log = record.clone();
    m.set(
        "decide",
        lua.create_function(
            move |_,
                  (kind, _, options, fail_closed): (
                String,
                BTreeMap<String, String>,
                Vec<String>,
                String,
            )| {
                if !options.contains(&fail_closed) {
                    return Err(mlua::Error::runtime(format!(
                        "the fail-closed option {fail_closed:?} is not one of the options"
                    )));
                }
                log(format!(
                    "decided {kind}: nobody decides in a dry run, so the fail-closed option {fail_closed:?} was taken"
                ));
                Ok((fail_closed, "a dry run decides nothing".to_string()))
            },
        )?,
    )?;

    let (log, started) = (record.clone(), Cell::new(0));
    m.set(
        "start_run",
        lua.create_function(
            move |_, (agent, purpose, note): (String, String, Option<String>)| {
                started.set(started.get() + 1);
                log(format!(
                    "started a run of {agent} for {purpose}{}",
                    with_note(note)
                ));
                Ok(started.get())
            },
        )?,
    )?;

    m.set("has_agent", lua.create_function(|_, _: String| Ok(true))?)?;

    let (log, names) = (record.clone(), columns.to_vec());
    m.set(
        "move",
        lua.create_function(move |_, (column, note): (String, Option<String>)| {
            if !names.contains(&column) {
                return Err(mlua::Error::runtime(format!("no column {column:?}")));
            }
            log(format!("moved the card to {column}{}", with_note(note)));
            Ok(())
        })?,
    )?;

    let log = record.clone();
    m.set(
        "wait",
        lua.create_function(move |_, reason: String| {
            log(format!("waited: {reason}"));
            Ok(())
        })?,
    )?;

    let log = record.clone();
    m.set(
        "publish_change",
        lua.create_function(move |lua, ()| {
            log("published the change".into());
            made_up_change(lua)
        })?,
    )?;

    let log = record.clone();
    m.set(
        "run_checks",
        lua.create_function(move |lua, ()| {
            log("started the checks of the change".into());
            made_up_change(lua)
        })?,
    )?;

    let log = record.clone();
    m.set(
        "offer_change",
        lua.create_function(move |_, ()| {
            log("offered the change to a person, and the card moved to Ready to merge".into());
            Ok(1)
        })?,
    )?;

    let log = record.clone();
    m.set(
        "ask",
        lua.create_function(move |_, (kind, reason): (String, String)| {
            log(format!("asked a person ({kind}) and waited: {reason}"));
            Ok(())
        })?,
    )?;

    let log = record.clone();
    m.set(
        "stuck",
        lua.create_function(move |_, reason: String| {
            log(format!("put the card in the inbox as stuck: {reason}"));
            Ok(())
        })?,
    )?;

    let log = record;
    m.set(
        "record_outcome",
        lua.create_function(move |_, outcome: String| {
            log(format!("recorded the outcome {outcome:?}"));
            Ok(0)
        })?,
    )?;

    m.set(
        "agent_budget",
        lua.create_function(|_, _: String| Ok(None::<i64>))?,
    )?;
    m.set("tokens_today", lua.create_function(|_, ()| Ok(0))?)?;
    m.set("runs_active", lua.create_function(|_, ()| Ok(0))?)?;
    Ok(m)
}

/// The change that the dry run's `publish_change` and `run_checks` return: one without checks that merges cleanly.
fn made_up_change(lua: &Lua) -> mlua::Result<Table> {
    let c = lua.create_table()?;
    c.set("branch", "moebius/card-1")?;
    c.set("base", "main")?;
    c.set("checks", "none")?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Instant;

    use mlua::Value;

    use super::*;
    use crate::board::engine::Effects;
    use crate::board::testing::setup;
    use crate::board::workflow::DEFAULT_SCRIPTS;

    fn dry(source: &str, column: &str, author: &str) -> Result<Vec<String>, String> {
        check_script("test.luau", source, column, "Fix the parser", author)
    }

    #[test]
    fn a_dry_run_says_what_a_script_did_without_storing_or_starting_anything() {
        let source = r#"
            function on_card(card)
              local choice = moebius.decide("start", { title = card.title }, { "go", "wait" }, "wait")
              local run = moebius.start_run("Coder", "code", "Be careful.")
              local change = moebius.publish_change()
              moebius.ask("approve_issue", "a person reads " .. card.author .. "'s issue")
              moebius.stuck("choice " .. choice .. ", run " .. run .. ", checks " .. change.checks)
              moebius.move("Done", "all well")
              moebius.wait("done waiting")
            end
        "#;
        assert_eq!(
            dry(source, "Ready", "leon").unwrap(),
            [
                "decided start: nobody decides in a dry run, so the fail-closed option \"wait\" was taken",
                "started a run of Coder for code (note: Be careful.)",
                "published the change",
                "asked a person (approve_issue) and waited: a person reads leon's issue",
                "put the card in the inbox as stuck: choice wait, run 1, checks none",
                "moved the card to Done (note: all well)",
                "waited: done waiting",
            ]
        );
        assert_eq!(
            dry("function on_card(card) end", "Ready", "leon").unwrap(),
            [] as [String; 0]
        );
    }

    #[test]
    fn the_default_scripts_do_what_the_workflow_says_on_the_made_up_card() {
        let script = |name: &str| {
            DEFAULT_SCRIPTS
                .iter()
                .find(|(file, _)| *file == name)
                .unwrap()
                .1
        };
        assert_eq!(
            dry(script("backlog.luau"), "Backlog", PERSON).unwrap(),
            ["moved the card to Ready"]
        );
        assert_eq!(
            dry(script("backlog.luau"), "Backlog", "mallory").unwrap(),
            [
                "asked a person (approve_issue) and waited: waiting for a person to approve the issue"
            ]
        );
        assert_eq!(
            dry(script("ready.luau"), "Ready", PERSON).unwrap(),
            ["moved the card to In progress"]
        );
        let coding = dry(script("code.luau"), "In progress", PERSON).unwrap();
        assert_eq!(coding[0], "started a run of Coder for code");
    }

    #[test]
    fn a_dry_run_reports_a_script_that_fails_as_the_engine_would() {
        for (source, expected) in [
            ("function on_card(card", "syntax error"),
            ("function on_card(card) error('boom') end", "boom"),
            ("x = 1", "does not define a function on_card"),
            (
                "function on_card(card) moebius.move('In Progress') end",
                "no column \"In Progress\"",
            ),
            (
                "function on_card(card) moebius.start_run(1) end",
                "bad argument",
            ),
            (
                "function on_card(card) moebius.decide('k', {}, {'a'}, 'b') end",
                "fail-closed option",
            ),
            ("function on_card(card) local f = io.open end", "index nil"),
        ] {
            let error = dry(source, "Ready", PERSON).unwrap_err();
            assert!(error.contains(expected), "{source}: {error}");
        }
        let error = dry("function on_card(card) end", "In Progress", PERSON).unwrap_err();
        assert!(error.contains("no column \"In Progress\""), "{error}");
        assert!(error.contains("In progress"), "{error}");
    }

    /// The names of the fields of a table that are set.
    fn names(table: &Table) -> std::collections::BTreeSet<String> {
        table
            .pairs::<String, Value>()
            .map(|pair| pair.unwrap().0)
            .collect()
    }

    #[test]
    fn the_dry_run_has_everything_the_engine_gives_a_script() {
        let (_dir, app, engine, repo) = setup();
        let card = app
            .create_issue(repo, "Fix the parser", "", PERSON)
            .unwrap();
        let lua = Lua::new();
        let effects = Rc::new(RefCell::new(Effects::default()));
        let deadline = Rc::new(Cell::new(Instant::now()));
        let real = engine.api(&lua, &card, effects, deadline).unwrap();
        let columns: Vec<String> = engine
            .workflow
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect();
        let dry = recording_api(&lua, &engine.limits(), &columns, Rc::default()).unwrap();
        assert_eq!(names(&dry), names(&real));
        let real = engine.card_table(&lua, &card).unwrap();
        let dry = made_up_card(&lua, "Backlog", "Fix the parser", PERSON).unwrap();
        assert_eq!(names(&dry), names(&real));
    }
}
