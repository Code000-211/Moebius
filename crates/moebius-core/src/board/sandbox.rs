//! The Luau sandbox every column script runs in, for the engine and for a dry run alike: no file system, network,
//! processes or `require`, a memory limit, and a time limit that counts only the time spent in the script, not the
//! time Moebius spends on its behalf (starting a run, a decider thinking).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use mlua::{Lua, Table, Value, VmState};

use super::Limits;

/// Runs `source` in the sandbox every script runs in, and calls its `on_card` with the table `card` makes. The script
/// has no unsafe globals, a memory limit, and a time limit that the table `api` makes (the `moebius` table) can
/// extend while it acts for the script.
pub(super) fn run_sandboxed(
    name: &str,
    source: &str,
    limits: &Limits,
    api: impl FnOnce(&Lua, &Rc<Cell<Instant>>) -> mlua::Result<Table>,
    card: impl FnOnce(&Lua) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    let lua = Lua::new();
    lua.set_memory_limit(limits.script_memory)?;
    let globals = lua.globals();
    for unsafe_global in [
        "require",
        "dofile",
        "loadfile",
        "load",
        "loadstring",
        "getfenv",
        "setfenv",
    ] {
        globals.raw_set(unsafe_global, Value::Nil)?;
    }
    let deadline = Rc::new(Cell::new(Instant::now() + limits.script_time));
    globals.raw_set("moebius", api(&lua, &deadline)?)?;
    lua.sandbox(true)?;
    // Describing the card takes git, which is not the script's time.
    let card_table = card(&lua)?;
    deadline.set(Instant::now() + limits.script_time);
    lua.set_interrupt(move |_| {
        if Instant::now() > deadline.get() {
            Err(mlua::Error::runtime("the script ran out of time"))
        } else {
            Ok(VmState::Continue)
        }
    });
    lua.load(source).set_name(name).exec()?;
    let on_card: Option<mlua::Function> = lua.globals().get("on_card")?;
    let on_card = on_card.ok_or_else(|| {
        mlua::Error::runtime("the script does not define a function on_card(card)")
    })?;
    on_card.call::<()>(card_table)
}

/// Runs `f` on the script's behalf: the time it takes does not count against the script's time limit.
pub(super) fn outside_script<T>(deadline: &Cell<Instant>, f: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = f();
    deadline.set(deadline.get() + started.elapsed());
    result
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::app::{App, PERSON};
    use crate::board::engine::Effects;
    use crate::board::testing::{FakeHarness, engine_with, setup, setup_with};
    use crate::board::{Engine, Limits, Runs, Step, Workflow};
    use crate::store::{Card, Run};

    /// A harness that takes longer to start a run than a script may run.
    struct SlowHarness;

    impl Runs for SlowHarness {
        fn start(
            &self,
            app: &Arc<App>,
            card: &Card,
            agent: &str,
            purpose: &str,
            note: Option<&str>,
        ) -> Result<Run, String> {
            std::thread::sleep(Duration::from_millis(600));
            FakeHarness::default().start(app, card, agent, purpose, note)
        }

        fn has_agent(&self, name: &str) -> bool {
            FakeHarness::default().has_agent(name)
        }

        fn steer(&self, _: i64, _: &str) -> bool {
            false
        }

        fn stop(&self, _: i64) -> bool {
            false
        }
    }

    #[test]
    fn time_spent_starting_a_run_does_not_count_against_the_script() {
        let (_dir, app, _, repo) = setup();
        let engine = Engine::new(
            app.clone(),
            Arc::new(SlowHarness),
            Workflow::default(),
            Limits {
                script_time: Duration::from_millis(300),
                ..Limits::default()
            },
        );
        let card = app.create_issue(repo, "Slow", "", PERSON).unwrap();
        let step = engine.settle(card.id).unwrap();
        assert!(step.started_run.is_some());
    }

    /// A forge that takes longer to offer a change than a script may run.
    struct SlowForge;

    impl crate::repos::forge::Forge for SlowForge {
        fn name(&self) -> &'static str {
            "none"
        }

        fn publish(
            &self,
            _: &crate::store::Repository,
            _: &str,
            _: &str,
            _: &str,
            _: Option<i64>,
        ) -> Result<Option<i64>, crate::repos::forge::ForgeError> {
            std::thread::sleep(Duration::from_millis(600));
            Ok(None)
        }

        fn merge(
            &self,
            _: &crate::store::Repository,
            _: &crate::store::Change,
            _: &str,
        ) -> Result<String, crate::repos::forge::ForgeError> {
            unreachable!("nothing is merged")
        }

        fn refresh(
            &self,
            _: &crate::store::Repository,
        ) -> Result<(), crate::repos::forge::ForgeError> {
            Ok(())
        }
    }

    #[test]
    fn time_spent_offering_a_change_does_not_count_against_the_script() {
        let (_dir, app, engine, repo) = setup_with(Some(Box::new(SlowForge)));
        let card = app.create_issue(repo, "Slow", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Ready to merge");
    }

    fn run(engine: &Engine, card: &Card, source: &str) -> Result<Step, String> {
        let effects = Rc::new(RefCell::new(Effects::default()));
        engine
            .run_script("test.luau", source, card, effects.clone())
            .map_err(|e| e.to_string())?;
        Ok(Step {
            waiting: effects.take().waiting,
            ..Step::default()
        })
    }

    #[test]
    fn a_script_cannot_reach_files_network_or_processes() {
        let (_dir, app, engine, repo) = setup();
        let card = app.create_issue(repo, "Escape", "", PERSON).unwrap();
        let probe = r#"
            function on_card(card)
              local reached = {}
              if io ~= nil then table.insert(reached, "io") end
              if os ~= nil and os.execute ~= nil then table.insert(reached, "os.execute") end
              if os ~= nil and os.remove ~= nil then table.insert(reached, "os.remove") end
              if os ~= nil and os.getenv ~= nil then table.insert(reached, "os.getenv") end
              if require ~= nil then table.insert(reached, "require") end
              if dofile ~= nil or loadfile ~= nil or load ~= nil or loadstring ~= nil then table.insert(reached, "load") end
              if package ~= nil then table.insert(reached, "package") end
              if debug ~= nil and debug.getregistry ~= nil then table.insert(reached, "debug") end
              if socket ~= nil or http ~= nil then table.insert(reached, "network") end
              local ok = pcall(function() string.rep = nil end)
              if ok then table.insert(reached, "shared library tables") end
              moebius.wait(table.concat(reached, ","))
            end
        "#;
        assert_eq!(
            run(&engine, &card, probe).unwrap().waiting.as_deref(),
            Some("")
        );
    }

    #[test]
    fn a_script_that_loops_or_floods_memory_is_cut_off() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                script_time: Duration::from_millis(300),
                ..Limits::default()
            },
        );
        let card = app.create_issue(repo, "Loop", "", PERSON).unwrap();
        let started = Instant::now();
        let e = run(&engine, &card, "function on_card(c) while true do end end").unwrap_err();
        assert!(e.contains("ran out of time"), "{e}");
        assert!(started.elapsed() < Duration::from_secs(5));
        let e = run(&engine, &card,
            "function on_card(c) local t = {} for i = 1, 1e9 do t[i] = string.rep('x', 1024) .. i end end",
        )
        .unwrap_err();
        assert!(e.to_lowercase().contains("memory"), "{e}");
    }
}
