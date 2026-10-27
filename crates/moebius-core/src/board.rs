//! The board engine (ADR 0004, ADR 0005): each column's behaviour is a Luau script, run in a sandbox for one card at a
//! time. A script defines `on_card(card)` and acts through the `moebius` table:
//!
//! - `moebius.decide(kind, context, options, fail_closed) -> choice, reason`: a decision point (always recorded).
//! - `moebius.start_run(agent, purpose) -> run id`: starts a run for the card.
//! - `moebius.move(column)`: moves the card when the script returns.
//! - `moebius.wait(reason)`: leaves the card where it is.
//! - `moebius.offer_change()`: offers the card's last run as a change; the card moves to "Ready to merge".
//! - `moebius.ask(kind, reason)`: puts the card in the inbox for a person (once) and leaves it where it is.
//! - `moebius.stuck(reason)`: puts the card in the inbox as stuck (once).
//! - `moebius.record_outcome(outcome) -> count`: sets the outcome of the card's judgements that have none.
//! - `card.blocked_by`: the numbers of the issues blocking the card that are not done yet (an issue with no card counts
//!   as not done); empty when nothing blocks.
//! - `moebius.runs_active()`, `moebius.run_limit`, `moebius.round_limit`: the limits.
//!
//! Scripts have no file system, network, processes or `require`; each call has a memory limit and a time limit. The
//! time limit counts only the time spent in the script, not the time Moebius spends on its behalf (starting a run, a
//! decider thinking). A repository overrides a column's script with `.moebius/<script>` on its default branch.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mlua::{Lua, Table, Value, VmState};

use crate::app::{App, AppError, columns};
use crate::decide::Question;
use crate::repos::git;
use crate::store::{Card, Run};

/// Starts runs for the engine. The real one drives a harness (Pi-Bolt); tests use a fake.
pub trait Runs: Send + Sync {
    /// Starts a run for `card` and returns it without waiting for it to end.
    fn start(&self, app: &Arc<App>, card: &Card, agent: &str, purpose: &str)
    -> Result<Run, String>;
}

/// A column and the script behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub script: String,
}

/// The columns of a board, left to right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workflow {
    pub columns: Vec<Column>,
}

/// The default workflow's scripts (spec 6), built into the server.
const DEFAULT_SCRIPTS: &[(&str, &str)] = &[
    ("backlog.luau", include_str!("../workflow/backlog.luau")),
    ("ready.luau", include_str!("../workflow/ready.luau")),
    ("code.luau", include_str!("../workflow/code.luau")),
    ("review.luau", include_str!("../workflow/review.luau")),
    ("merge.luau", include_str!("../workflow/merge.luau")),
    ("done.luau", include_str!("../workflow/done.luau")),
];

impl Default for Workflow {
    fn default() -> Self {
        let columns = [
            (columns::BACKLOG, "backlog.luau"),
            (columns::READY, "ready.luau"),
            (columns::IN_PROGRESS, "code.luau"),
            (columns::IN_REVIEW, "review.luau"),
            (columns::READY_TO_MERGE, "merge.luau"),
            (columns::DONE, "done.luau"),
        ];
        Self {
            columns: columns
                .into_iter()
                .map(|(name, script)| Column {
                    name: name.into(),
                    script: script.into(),
                })
                .collect(),
        }
    }
}

/// The engine's limits.
#[derive(Debug, Clone)]
pub struct Limits {
    pub run_limit: u32,
    pub round_limit: u32,
    /// How long one script call may run, not counting the calls it makes into Moebius.
    pub script_time: Duration,
    /// How much memory one script call may use.
    pub script_memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            run_limit: 3,
            round_limit: 5,
            script_time: Duration::from_secs(2),
            script_memory: 16 << 20,
        }
    }
}

/// What one script call did to its card.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Step {
    /// The column the card is in afterwards, if it moved.
    pub moved: Option<String>,
    pub waiting: Option<String>,
    pub started_run: Option<i64>,
}

/// Why a script call failed.
#[derive(Debug, thiserror::Error)]
pub enum BoardError {
    #[error(transparent)]
    App(#[from] AppError),
    #[error("no column {0:?} in the workflow")]
    NoColumn(String),
    #[error("{script}: {message}")]
    Script { script: String, message: String },
}

pub struct Engine {
    app: Arc<App>,
    runs: Arc<dyn Runs>,
    workflow: Workflow,
    limits: Limits,
}

#[derive(Default)]
struct Effects {
    move_to: Option<String>,
    moved: Option<String>,
    waiting: Option<String>,
    started_run: Option<i64>,
}

impl Engine {
    pub fn new(app: Arc<App>, runs: Arc<dyn Runs>, workflow: Workflow, limits: Limits) -> Self {
        Self {
            app,
            runs,
            workflow,
            limits,
        }
    }

    /// The script for `column`: the repository's override on its default branch, or the built-in one.
    fn script(&self, card: &Card, column: &Column) -> Result<String, BoardError> {
        let repository = self
            .app
            .store
            .repository(card.repository_id)
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::NotFound(format!("repository {}", card.repository_id)))?;
        let spec = format!("{}:.moebius/{}", repository.base_ref(), column.script);
        if let Ok(source) = git(Path::new(&repository.path), &["show", &spec]) {
            return Ok(source);
        }
        DEFAULT_SCRIPTS
            .iter()
            .find(|(name, _)| *name == column.script)
            .map(|(_, source)| source.to_string())
            .ok_or_else(|| BoardError::Script {
                script: column.script.clone(),
                message: "no such script".into(),
            })
    }

    /// Runs the script of the card's column once and applies what it did.
    pub fn tick(&self, card_id: i64) -> Result<Step, BoardError> {
        let card = self.app.card(card_id)?;
        let column = self
            .workflow
            .columns
            .iter()
            .find(|c| c.name == card.column)
            .ok_or_else(|| BoardError::NoColumn(card.column.clone()))?;
        let source = self.script(&card, column)?;
        let effects = Rc::new(RefCell::new(Effects::default()));
        self.run_script(&column.script, &source, &card, effects.clone())
            .map_err(|e| BoardError::Script {
                script: column.script.clone(),
                message: e.to_string(),
            })?;
        let mut effects = effects.take();
        if let Some(to) = effects.move_to.take()
            && to != card.column
        {
            self.app.move_card(card_id, &to)?;
            effects.moved = Some(to);
        }
        // A card that moved no longer waits for what it waited for.
        let waiting = effects.waiting.clone().filter(|_| effects.moved.is_none());
        self.app.set_waiting(card_id, waiting);
        Ok(Step {
            moved: effects.moved,
            waiting: effects.waiting,
            started_run: effects.started_run,
        })
    }

    /// Ticks the card until its script leaves it where it is; returns the last step.
    pub fn settle(&self, card_id: i64) -> Result<Step, BoardError> {
        let mut step = self.tick(card_id)?;
        for _ in 0..self.workflow.columns.len() * 2 {
            if step.moved.is_none() {
                break;
            }
            step = self.tick(card_id)?;
        }
        Ok(step)
    }

    fn run_script(
        &self,
        name: &str,
        source: &str,
        card: &Card,
        effects: Rc<RefCell<Effects>>,
    ) -> mlua::Result<()> {
        let lua = Lua::new();
        lua.set_memory_limit(self.limits.script_memory)?;
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
        let deadline = Rc::new(Cell::new(Instant::now() + self.limits.script_time));
        globals.raw_set("moebius", self.api(&lua, card, effects, deadline.clone())?)?;
        lua.sandbox(true)?;
        lua.set_interrupt(move |_| {
            if Instant::now() > deadline.get() {
                Err(mlua::Error::runtime("the script ran out of time"))
            } else {
                Ok(VmState::Continue)
            }
        });
        lua.load(source).set_name(name).exec()?;
        let on_card: mlua::Function = lua.globals().get("on_card")?;
        on_card.call::<()>(self.card_table(&lua, card)?)
    }

    fn card_table(&self, lua: &Lua, card: &Card) -> mlua::Result<Table> {
        let runs = self
            .app
            .store
            .runs_for_card(card.id)
            .map_err(mlua::Error::external)?;
        let t = lua.create_table()?;
        t.set("id", card.id)?;
        t.set("number", card.number)?;
        t.set("title", card.title.as_str())?;
        t.set("body", card.body.as_str())?;
        t.set("author", card.author.as_str())?;
        t.set("column", card.column.as_str())?;
        t.set("trusted", self.app.is_trusted(card))?;
        let mut blocked_by = Vec::new();
        for number in self
            .app
            .store
            .dependencies(card.id)
            .map_err(mlua::Error::external)?
        {
            let blocker = self
                .app
                .store
                .card_by_number(card.repository_id, number)
                .map_err(mlua::Error::external)?;
            if blocker.is_none_or(|b| b.column != columns::DONE) {
                blocked_by.push(number);
            }
        }
        t.set("blocked_by", blocked_by)?;
        t.set(
            "rounds",
            runs.iter().filter(|r| r.purpose == "code").count(),
        )?;
        if let Some(run) = runs.last() {
            let r = lua.create_table()?;
            r.set("id", run.id)?;
            r.set("agent", run.agent.as_str())?;
            r.set("purpose", run.purpose.as_str())?;
            r.set("status", run.status.as_str())?;
            t.set("run", r)?;
        }
        Ok(t)
    }

    fn api(
        &self,
        lua: &Lua,
        card: &Card,
        effects: Rc<RefCell<Effects>>,
        deadline: Rc<Cell<Instant>>,
    ) -> mlua::Result<Table> {
        let m = lua.create_table()?;
        m.set("run_limit", self.limits.run_limit)?;
        m.set("round_limit", self.limits.round_limit)?;
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
            lua.create_function(move |_, (agent, purpose): (String, String)| {
                let run = outside_script(&clock, || runs.start(&app, &card_copy, &agent, &purpose))
                    .map_err(mlua::Error::runtime)?;
                fx.borrow_mut().started_run = Some(run.id);
                Ok(run.id)
            })?,
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
            lua.create_function(move |_, column: String| {
                if !names.contains(&column) {
                    return Err(mlua::Error::runtime(format!("no column {column:?}")));
                }
                fx.borrow_mut().move_to = Some(column);
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

        let (app, fx, clock) = (self.app.clone(), effects.clone(), deadline);
        m.set(
            "offer_change",
            lua.create_function(move |_, ()| {
                let run = app
                    .store
                    .runs_for_card(card_id)
                    .map_err(mlua::Error::external)?
                    .pop()
                    .ok_or_else(|| mlua::Error::runtime("the card has no run to offer"))?;
                let change = outside_script(&clock, || {
                    app.offer_change(card_id, &format!("moebius/run-{}", run.id))
                })
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

        let app = self.app.clone();
        m.set(
            "runs_active",
            lua.create_function(move |_, ()| {
                Ok(app
                    .store
                    .active_runs()
                    .map_err(mlua::Error::external)?
                    .len())
            })?,
        )?;

        Ok(m)
    }
}

/// Runs `f` on the script's behalf: the time it takes does not count against the script's time limit.
fn outside_script<T>(deadline: &Cell<Instant>, f: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = f();
    deadline.set(deadline.get() + started.elapsed());
    result
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::PERSON;
    use crate::store::Store;
    use moebius_plugin_api::Host;

    /// A harness that finishes at once: it commits a file on the run's branch.
    struct FakeHarness;

    impl Runs for FakeHarness {
        fn start(
            &self,
            app: &Arc<App>,
            card: &Card,
            agent: &str,
            purpose: &str,
        ) -> Result<Run, String> {
            let store = &app.store;
            let run = store
                .add_run(Some(card.id), agent, "fake", purpose)
                .map_err(|e| e.to_string())?;
            let repo = store.repository(card.repository_id).unwrap().unwrap();
            let clone = Path::new(&repo.path);
            let wt = app
                .repos
                .add_worktree(&repo.name, clone, run.id, &repo.default_branch)
                .map_err(|e| e.to_string())?;
            std::fs::write(
                wt.path.join(format!("card-{}.txt", card.number)),
                &card.title,
            )
            .unwrap();
            git(&wt.path, &["add", "."]).unwrap();
            git(&wt.path, &["commit", "--quiet", "-m", &card.title]).unwrap();
            app.repos.remove_worktree(clone, &wt).unwrap();
            store.set_run_status(run.id, "succeeded").unwrap();
            Ok(store.run(run.id).unwrap().unwrap())
        }
    }

    fn setup() -> (tempfile::TempDir, Arc<App>, Engine, i64) {
        setup_with(None)
    }

    /// A setup whose forge for repositories without one is `forge`, if given.
    fn setup_with(
        forge: Option<Box<dyn crate::forge::Forge>>,
    ) -> (tempfile::TempDir, Arc<App>, Engine, i64) {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(1)));
        let mut app = App::new(store, host, &dir.path().join("data"));
        if let Some(forge) = forge {
            app.add_forge(forge);
        }
        let app = Arc::new(app);
        let repo = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        let engine = Engine::new(
            app.clone(),
            Arc::new(FakeHarness),
            Workflow::default(),
            Limits {
                script_time: Duration::from_millis(300),
                ..Limits::default()
            },
        );
        (dir, app, engine, repo.id)
    }

    #[test]
    fn the_default_workflow_moves_a_card_from_backlog_to_done() {
        let (_dir, app, engine, repo) = setup();
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();

        // Backlog -> Ready -> In progress, where the Coder run starts.
        let step = engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "In progress");
        assert!(step.started_run.is_some());
        assert_eq!(step.waiting.as_deref(), Some("coding"));

        // The run has succeeded: its change is offered and waits for a person.
        let step = engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Ready to merge");
        assert_eq!(
            step.waiting.as_deref(),
            Some("waiting for a person to merge")
        );

        let change = app.store.changes(repo).unwrap().pop().unwrap();
        app.merge_change(change.id, "fine").unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Done");
        engine.settle(card.id).unwrap();
        let judgements = app.store.judgements_for_card(card.id).unwrap();
        assert!(!judgements.is_empty());
        assert!(
            judgements
                .iter()
                .all(|j| j.outcome.as_deref() == Some("merged"))
        );
    }

    /// A harness that takes longer to start a run than a script may run.
    struct SlowHarness;

    impl Runs for SlowHarness {
        fn start(
            &self,
            app: &Arc<App>,
            card: &Card,
            agent: &str,
            purpose: &str,
        ) -> Result<Run, String> {
            std::thread::sleep(Duration::from_millis(600));
            FakeHarness.start(app, card, agent, purpose)
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

    impl crate::forge::Forge for SlowForge {
        fn name(&self) -> &'static str {
            "none"
        }

        fn offer(
            &self,
            _: &crate::store::Repository,
            _: &Card,
            _: &str,
        ) -> Result<Option<i64>, crate::forge::ForgeError> {
            std::thread::sleep(Duration::from_millis(600));
            Ok(None)
        }

        fn merge(
            &self,
            _: &crate::store::Repository,
            _: &crate::store::Change,
            _: &str,
        ) -> Result<String, crate::forge::ForgeError> {
            unreachable!("nothing is merged")
        }

        fn refresh(&self, _: &crate::store::Repository) -> Result<(), crate::forge::ForgeError> {
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

    #[test]
    fn why_a_card_waits_is_kept_and_published_only_when_it_changes() {
        let (_dir, app, engine, repo) = setup();
        let mut events = app.bus.subscribe();
        let card = app
            .create_issue(repo, "Please add", "", "stranger")
            .unwrap();
        let reason = "waiting for a person to approve the issue";
        for _ in 0..3 {
            engine.settle(card.id).unwrap();
        }
        let cards = app.store.cards(repo).unwrap();
        assert_eq!(
            app.waiting(&cards),
            BTreeMap::from([(card.id, reason.into())])
        );

        // Only the first tick changed the reason, so only one event says so.
        let mut waiting = Vec::new();
        while let Ok(event) = events.try_recv() {
            if event.kind == "card_waiting" {
                waiting.push(event.data);
            }
        }
        assert_eq!(
            waiting,
            vec![serde_json::json!({"card": card.id, "reason": reason})]
        );

        // Once the card moves it waits for something else or for nothing.
        app.approve_issue(card.id, "fine").unwrap();
        assert!(app.waiting(&cards).is_empty());
        let cleared = std::iter::from_fn(|| events.try_recv().ok())
            .find(|event| event.kind == "card_waiting")
            .unwrap();
        assert_eq!(
            cleared.data,
            serde_json::json!({"card": card.id, "reason": null})
        );
    }

    #[test]
    fn an_outside_issue_waits_in_the_inbox_until_a_person_approves_it() {
        let (_dir, app, engine, repo) = setup();
        let card = app
            .create_issue(repo, "Please add", "", "stranger")
            .unwrap();
        for _ in 0..3 {
            let step = engine.settle(card.id).unwrap();
            assert_eq!(
                step.waiting.as_deref(),
                Some("waiting for a person to approve the issue")
            );
        }
        assert_eq!(app.card(card.id).unwrap().column, "Backlog");
        let inbox = app.store.inbox().unwrap();
        assert_eq!((inbox.len(), inbox[0].kind.as_str()), (1, "approve_issue"));

        app.approve_issue(card.id, "fine by me").unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Ready");
        assert!(app.store.inbox().unwrap().is_empty());
        assert_eq!(
            app.store.judgements("approve_issue").unwrap()[0].decider,
            "person"
        );
    }

    #[test]
    fn a_repository_overrides_a_script_on_its_default_branch() {
        let (_dir, app, engine, repo) = setup();
        let clone = std::path::PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        std::fs::create_dir_all(clone.join(".moebius")).unwrap();
        std::fs::write(
            clone.join(".moebius/ready.luau"),
            "function on_card(card) moebius.wait('held by the repository') end",
        )
        .unwrap();
        git(&clone, &["add", "."]).unwrap();
        git(&clone, &["commit", "--quiet", "-m", "hold"]).unwrap();
        let card = app.create_issue(repo, "Held", "", PERSON).unwrap();
        let step = engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Ready");
        assert_eq!(step.waiting.as_deref(), Some("held by the repository"));
    }

    #[test]
    fn a_blocked_card_waits_in_ready_until_its_blocker_is_done() {
        let (_dir, app, engine, repo) = setup();
        let blocker = app.create_issue(repo, "First", "", PERSON).unwrap();
        let card = app.create_issue(repo, "Second", "", PERSON).unwrap();
        app.store
            .set_dependencies(card.id, &[blocker.number].into())
            .unwrap();

        let reason = format!("waiting for #{}", blocker.number);
        for _ in 0..2 {
            let step = engine.settle(card.id).unwrap();
            assert_eq!(app.card(card.id).unwrap().column, "Ready");
            assert_eq!(step.waiting.as_deref(), Some(reason.as_str()));
        }

        app.store.move_card(blocker.id, columns::DONE).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "In progress");
    }

    #[test]
    fn a_card_blocked_by_an_issue_without_a_card_waits() {
        let (_dir, app, engine, repo) = setup();
        let card = app.create_issue(repo, "Second", "", PERSON).unwrap();
        app.store.set_dependencies(card.id, &[99].into()).unwrap();
        let step = engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Ready");
        assert_eq!(step.waiting.as_deref(), Some("waiting for #99"));
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
        let (_dir, app, engine, repo) = setup();
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
