//! The board engine (ADR 0004, ADR 0005): each column's behaviour is a Luau script, run in a sandbox for one card at a
//! time. A script defines `on_card(card)` and acts through the `moebius` table:
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
//! - `card.blocked_by`: the numbers of the issues blocking the card that are not done yet (an issue with no card counts
//!   as not done); empty when nothing blocks.
//! - `card.review`: the verdict and findings of the review since the card's latest `code` run, if there is one.
//! - `card.note`: why the card went back to coding, if it did and no `code` run has succeeded since.
//! - `card.change`: the card's open change: its checks, the failing ones and whether it still merges cleanly.
//! - `card.stopped`: whether a person stopped the card's latest run and has not retried the card since.
//! - `card.rounds`, `card.reviews`, `card.tokens`: the `code` runs, `review` runs and tokens of the card since it was
//!   last retried.
//! - `moebius.runs_active()`, `moebius.run_limit`, `moebius.round_limit`: the limits.
//! - `moebius.agent_budget(name)`, `moebius.tokens_today()`, `moebius.day_budget`: the token budgets (nil when none).
//!
//! Scripts have no file system, network, processes or `require`; each call has a memory limit and a time limit. The
//! time limit counts only the time spent in the script, not the time Moebius spends on its behalf (starting a run, a
//! decider thinking). A repository overrides a column's script with `.moebius/<script>` on its default branch.
//! `check_script` runs a script once in the same sandbox against a made-up card, for `moebius check-script`.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mlua::{Lua, Table, Value, VmState};

use crate::app::{App, AppError, PERSON, columns, runs_since_retry};
use crate::config::AgentConfig;
use crate::decide::Question;
use crate::repos::git;
use crate::router::Protocol;
use crate::store::{Card, Repository, Run};

/// An agent the harness can run, as the desktop API lists it: never its instructions or anything secret.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub model: String,
    pub protocol: Protocol,
    /// How hard the agent thinks, if its configuration says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    /// The most tokens a card of the agent may use, if its configuration says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
    /// Whether Moebius brings the agent itself, as it does the Moebius agent.
    pub built_in: bool,
    /// Why the agent's files in the harness configuration repository could not be used. Such an agent has no model
    /// and cannot be run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Starts runs for the engine. The real one drives a harness (Pi-Bolt); tests use a fake.
pub trait Runs: Send + Sync {
    /// Starts a run for `card` and returns it without waiting for it to end.
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
        note: Option<&str>,
    ) -> Result<Run, String>;

    /// Whether an agent of this name is configured.
    fn has_agent(&self, name: &str) -> bool;

    /// The agents this harness can run.
    fn agents(&self) -> Vec<AgentInfo> {
        Vec::new()
    }

    /// The token budget of `agent` for a card, if it has one.
    fn token_budget(&self, _agent: &str) -> Option<i64> {
        None
    }

    /// Sends a message to a running run's agent, which takes it into account as it works. False when the run is not
    /// running.
    fn steer(&self, run_id: i64, message: &str) -> bool;

    /// Asks a running run to stop; it ends as `stopped`. False when the run is not running.
    fn stop(&self, run_id: i64) -> bool;

    /// Starts a chat with `agent`, which answers `message`, in a branch of `repository` or in an empty directory,
    /// and returns it without waiting for the answer. A harness without chats refuses.
    fn start_chat(
        &self,
        _app: &Arc<App>,
        _agent: &str,
        _repository: Option<&Repository>,
        _message: &str,
    ) -> Result<Run, String> {
        Err("this harness has no chats".into())
    }

    /// Starts the Retro analyst with `prompt` in a fresh worktree of the harness configuration clone at `clone`,
    /// checked out at `base`, and returns the run without waiting for it to end. A harness without retros refuses.
    fn start_retro(
        &self,
        _app: &Arc<App>,
        _clone: &Path,
        _base: &str,
        _prompt: &str,
    ) -> Result<Run, String> {
        Err("this harness has no retros".into())
    }

    /// Starts the Autoresearcher (see `research`) with `prompt` in a fresh worktree of `repository` checked out at
    /// `base`, and returns the run without waiting for it to end. A harness without research refuses.
    fn start_research(
        &self,
        _app: &Arc<App>,
        _repository: &Repository,
        _base: &str,
        _prompt: &str,
    ) -> Result<Run, String> {
        Err("this harness has no research".into())
    }

    /// Starts a trial run (see `trial`) for `card`: the agent `agent`, as `definition` has it, is given the card's prompt
    /// in a fresh worktree of the card's repository on a branch of its own, checked out at `base`. The branch is named
    /// after `proposal` and the card. It returns the run without waiting for it to end. A harness without trials
    /// refuses.
    fn start_trial(
        &self,
        _app: &Arc<App>,
        _card: &Card,
        _agent: &str,
        _definition: AgentConfig,
        _proposal: i64,
        _base: &str,
    ) -> Result<Run, String> {
        Err("this harness has no trials".into())
    }

    /// Sends a person's message to a chat's agent; it answers when it is done with what it is doing. False when the
    /// chat is not running.
    fn say(&self, _run_id: i64, _message: &str) -> bool {
        false
    }

    /// Ends a chat: its agent is shut down and its work kept. False when the chat is not running.
    fn end(&self, _run_id: i64) -> bool {
        false
    }
}

/// What a review run concluded: `verdict` is `approve` or `changes`, and `findings` is what it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    pub verdict: &'static str,
    pub findings: String,
}

/// Reads the verdict from the end of a review run's summary: a last line `VERDICT: approve` or `VERDICT: changes`,
/// with the findings before it. A summary without a verdict, or with another one, asks for changes, because work that
/// was not reviewed is not approved.
fn verdict(summary: Option<&str>) -> Review {
    let text = summary.unwrap_or("").trim_end();
    let (findings, last) = text.rsplit_once('\n').unwrap_or(("", text));
    let said = last
        .trim()
        .strip_prefix("VERDICT:")
        .map(|v| v.trim().to_ascii_lowercase());
    let (verdict, findings) = match said.as_deref() {
        Some("approve") => ("approve", findings.trim()),
        Some("changes") if !findings.trim().is_empty() => ("changes", findings.trim()),
        Some("changes") => (
            "changes",
            "The review asked for changes but gave no findings.",
        ),
        _ => ("changes", "The review gave no verdict."),
    };
    Review {
        verdict,
        findings: findings.into(),
    }
}

/// The review that counts: the latest finished one after the latest `code` run.
pub fn latest_review(runs: &[Run]) -> Option<Review> {
    let mut since_code = runs.iter().rev().take_while(|r| r.purpose != "code");
    let run = since_code.find(|r| r.purpose == "review" && r.status == "succeeded")?;
    Some(verdict(run.summary.as_deref()))
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
    /// Token budgets for a card by agent name, besides those the harness knows (`Runs::token_budget`), which win.
    pub agent_budgets: BTreeMap<String, i64>,
    /// The most tokens all runs may use in a day, if limited.
    pub day_budget: Option<i64>,
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
            agent_budgets: BTreeMap::new(),
            day_budget: None,
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
    note: Option<String>,
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
            if let Some(note) = &effects.note {
                self.app
                    .store
                    .set_card_note(card_id, note)
                    .map_err(AppError::from)?;
            }
            if card.column == columns::READY_TO_MERGE {
                // The change is no longer waiting for a person.
                self.app
                    .store
                    .resolve_inbox_for_card(card_id)
                    .map_err(AppError::from)?;
            }
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
        run_sandboxed(
            name,
            source,
            &self.limits,
            |lua, deadline| self.api(lua, card, effects, deadline.clone()),
            |lua| self.card_table(lua, card),
        )
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
        t.set("note", card.note.as_deref())?;
        t.set("change", change_table(&self.app, lua, card.id)?)?;
        t.set("trusted", self.app.is_trusted(card))?;
        t.set("confirmed", card.confirmed)?;
        let blocked_by = self.app.blocked_by(card).map_err(mlua::Error::external)?;
        t.set("blocked_by", blocked_by)?;
        let since_retry = || runs_since_retry(&runs, card);
        let count = |purpose: &str| since_retry().filter(|r| r.purpose == purpose).count();
        t.set("rounds", count("code"))?;
        t.set("reviews", count("review"))?;
        t.set(
            "tokens",
            self.app
                .store
                .tokens_for_card(card.id)
                .map_err(mlua::Error::external)?,
        )?;
        t.set(
            "stopped",
            runs.last()
                .is_some_and(|r| r.status == "stopped" && Some(r.id) > card.retried_after),
        )?;
        if let Some(run) = runs.last() {
            let r = lua.create_table()?;
            r.set("id", run.id)?;
            r.set("agent", run.agent.as_str())?;
            r.set("purpose", run.purpose.as_str())?;
            r.set("status", run.status.as_str())?;
            r.set("summary", run.summary.as_deref())?;
            t.set("run", r)?;
        }
        if let Some(review) = latest_review(&runs) {
            let v = lua.create_table()?;
            v.set("verdict", review.verdict)?;
            v.set("findings", review.findings)?;
            t.set("review", v)?;
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
        m.set("day_budget", self.limits.day_budget)?;
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

        let (budgets, runs) = (self.limits.agent_budgets.clone(), self.runs.clone());
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

/// Runs `source` in the sandbox every script runs in, and calls its `on_card` with the table `card` makes. The script
/// has no unsafe globals, a memory limit, and a time limit that the table `api` makes (the `moebius` table) can
/// extend while it acts for the script.
fn run_sandboxed(
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

/// The card's open change as scripts see it, or nothing when the card has none. How its checks stand is for its current
/// commit; what failed is left out when that is of an older one.
fn change_table(app: &App, lua: &Lua, card_id: i64) -> mlua::Result<Option<Table>> {
    let Some(change) = app
        .store
        .open_change(card_id)
        .map_err(mlua::Error::external)?
    else {
        return Ok(None);
    };
    let repository = app
        .store
        .repository(change.repository_id)
        .map_err(mlua::Error::external)?
        .ok_or_else(|| mlua::Error::runtime("the change's repository is gone"))?;
    let current = change.checks_head == change.head;
    let c = lua.create_table()?;
    c.set("branch", change.branch.as_str())?;
    c.set("base", repository.base_ref())?;
    c.set("pull_request", change.pull_request)?;
    c.set(
        "checks",
        app.checks_of(&change).map_err(mlua::Error::external)?,
    )?;
    c.set(
        "failed_checks",
        change.failed_checks.as_deref().filter(|_| current),
    )?;
    c.set(
        "check_output",
        change.check_output.as_deref().filter(|_| current),
    )?;
    c.set(
        "conflict",
        app.conflict_of(&change).map_err(mlua::Error::external)?,
    )?;
    Ok(Some(c))
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
    use crate::store::Store;
    use moebius_plugin_api::Host;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// A harness that finishes at once. A `code` run commits a file on a branch that starts where a real run's would;
    /// a `review` run changes nothing and ends with the next of `answers`, or an approval when there are none left.
    #[derive(Default)]
    struct FakeHarness {
        reviewer: bool,
        answers: Mutex<VecDeque<String>>,
        /// The note of each run started, in order.
        notes: Mutex<Vec<Option<String>>>,
        /// Files, as path and text, that each `code` run also commits.
        files: Vec<(&'static str, &'static str)>,
    }

    impl FakeHarness {
        fn with_reviewer(answers: &[&str]) -> Self {
            Self {
                reviewer: true,
                answers: Mutex::new(answers.iter().map(|a| a.to_string()).collect()),
                ..Self::default()
            }
        }
    }

    impl Runs for FakeHarness {
        fn start(
            &self,
            app: &Arc<App>,
            card: &Card,
            agent: &str,
            purpose: &str,
            note: Option<&str>,
        ) -> Result<Run, String> {
            self.notes.lock().unwrap().push(note.map(String::from));
            let store = &app.store;
            let run = store
                .add_run(Some(card.id), agent, "fake", purpose)
                .map_err(|e| e.to_string())?;
            let repo = store.repository(card.repository_id).unwrap().unwrap();
            let clone = Path::new(&repo.path);
            let base = store
                .code_branch(card.id)
                .unwrap()
                .unwrap_or_else(|| repo.base_ref());
            let wt = app
                .repos
                .add_worktree(&repo.name, clone, run.id, &base)
                .map_err(|e| e.to_string())?;
            if purpose == "code" {
                std::fs::write(wt.path.join(format!("run-{}.txt", run.id)), &card.title).unwrap();
                for (path, text) in &self.files {
                    std::fs::create_dir_all(wt.path.join(path).parent().unwrap()).unwrap();
                    std::fs::write(wt.path.join(path), text).unwrap();
                }
                git(&wt.path, &["add", "."]).unwrap();
                git(&wt.path, &["commit", "--quiet", "-m", &card.title]).unwrap();
                store.set_run_summary(run.id, "Done.").unwrap();
            } else {
                let answer = self.answers.lock().unwrap().pop_front();
                store
                    .set_run_summary(run.id, &answer.unwrap_or("VERDICT: approve".into()))
                    .unwrap();
            }
            app.repos.remove_worktree(clone, &wt.path).unwrap();
            store.set_run_status(run.id, "succeeded").unwrap();
            Ok(store.run(run.id).unwrap().unwrap())
        }

        fn has_agent(&self, name: &str) -> bool {
            name == "Coder" || (self.reviewer && name == "Reviewer")
        }

        fn steer(&self, _: i64, _: &str) -> bool {
            false
        }

        fn stop(&self, _: i64) -> bool {
            false
        }
    }

    fn setup() -> (tempfile::TempDir, Arc<App>, Engine, i64) {
        setup_with(None)
    }

    /// A setup whose forge for repositories without one is `forge`, if given.
    fn setup_with(
        forge: Option<Box<dyn crate::forge::Forge>>,
    ) -> (tempfile::TempDir, Arc<App>, Engine, i64) {
        setup_with_harness(forge, Arc::new(FakeHarness::default()))
    }

    fn setup_with_harness(
        forge: Option<Box<dyn crate::forge::Forge>>,
        harness: Arc<FakeHarness>,
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
        // The real limits: a short time limit would make tests fail on a busy machine. Only the tests of the limit
        // itself shorten it.
        let engine = Engine::new(app.clone(), harness, Workflow::default(), Limits::default());
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

        // This server has no Reviewer, so the change was offered without a review run.
        let runs = app.store.runs_for_card(card.id).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].summary.as_deref(), Some("Done."));
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(change.branch, "moebius/card-1");
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

    fn column(app: &App, card: &Card) -> String {
        app.card(card.id).unwrap().column
    }

    #[test]
    fn a_card_goes_back_to_coding_until_the_reviewer_approves() {
        let harness = Arc::new(FakeHarness::with_reviewer(&[
            "There is no test.\nVERDICT: changes",
            "Fine now.\nVERDICT: approve",
        ]));
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();

        // Round one: coded, then reviewed, and the review wants changes.
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        let step = engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        assert_eq!(step.waiting.as_deref(), Some("reviewing"));

        // Back in progress, the next Coder round starts with the findings.
        let step = engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert_eq!(step.waiting.as_deref(), Some("coding, round 2"));
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");

        let runs = app.store.runs_for_card(card.id).unwrap();
        let purposes: Vec<_> = runs.iter().map(|r| r.purpose.as_str()).collect();
        assert_eq!(purposes, ["code", "review", "code", "review"]);
        assert_eq!(
            *harness.notes.lock().unwrap(),
            [None, None, Some("There is no test.".to_string()), None]
        );

        // The change is one branch that carries both rounds' work.
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.store.changes(repo).unwrap().len(), 1);
        assert_eq!(change.branch, "moebius/card-1");
        let clone = Path::new(&app.store.repository(repo).unwrap().unwrap().path).to_owned();
        for run in [&runs[0], &runs[2]] {
            let file = format!("{}:run-{}.txt", change.branch, run.id);
            assert_eq!(git(&clone, &["show", &file]).unwrap(), "Add a feature");
        }
    }

    #[test]
    fn a_review_without_a_verdict_sends_the_card_back_to_coding() {
        let harness = Arc::new(FakeHarness::with_reviewer(&["Looks fine to me."]));
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert_eq!(
            harness.notes.lock().unwrap()[2].as_deref(),
            Some("The review gave no verdict.")
        );
    }

    #[test]
    fn a_failed_review_is_tried_again_until_the_round_limit() {
        let harness = Arc::new(FakeHarness::with_reviewer(&[]));
        let (_dir, app, engine, repo) = setup_with_harness(None, harness);
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        // The code run is done and the card moves to review.
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        let fail_latest = || {
            let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
            app.store.set_run_status(run.id, "failed").unwrap();
        };
        for tries in 1..=Limits::default().round_limit {
            engine.settle(card.id).unwrap();
            let reviews = |app: &App| {
                let runs = app.store.runs_for_card(card.id).unwrap();
                runs.iter().filter(|r| r.purpose == "review").count()
            };
            assert_eq!(reviews(&app), tries as usize);
            fail_latest();
        }
        let step = engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        assert_eq!(step.waiting.as_deref(), Some("the round limit is reached"));
        assert_eq!(app.store.inbox().unwrap()[0].kind, "stuck");
    }

    #[test]
    fn a_review_run_is_read_for_its_verdict() {
        let review = |text: &str| {
            let r = verdict(Some(text));
            (r.verdict, r.findings)
        };
        assert_eq!(
            review("Good.\nVERDICT: approve\n"),
            ("approve", "Good.".into())
        );
        assert_eq!(review("VERDICT: approve"), ("approve", "".into()));
        assert_eq!(
            review("Line one\nLine two\n  VERDICT:  Changes  "),
            ("changes", "Line one\nLine two".into())
        );
        assert_eq!(
            review("VERDICT: changes").1,
            "The review asked for changes but gave no findings."
        );
        for text in [
            "",
            "All good",
            "VERDICT: approve\nand more",
            "VERDICT: maybe",
        ] {
            assert_eq!(
                review(text),
                ("changes", "The review gave no verdict.".into()),
                "{text:?}"
            );
        }
        assert_eq!(verdict(None).verdict, "changes");
    }

    /// The engine of `setup`, with other limits.
    fn engine_with(app: &Arc<App>, limits: Limits) -> Engine {
        Engine::new(
            app.clone(),
            Arc::new(FakeHarness::default()),
            Workflow::default(),
            limits,
        )
    }

    /// Records `tokens` used by a run of the card.
    fn use_tokens(app: &App, run: i64, tokens: i64) {
        app.store
            .add_usage(&crate::store::Usage {
                run_id: Some(run),
                model: "local".into(),
                input_tokens: tokens,
                output_tokens: 0,
                cached_tokens: 0,
                duration_ms: 0,
            })
            .unwrap();
    }

    /// A card in progress whose latest `code` run ended as `status`.
    fn card_after_a_run(app: &App, repo: i64, status: &str) -> (Card, Run) {
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        app.move_card(card.id, "In progress").unwrap();
        let run = app
            .store
            .add_run(Some(card.id), "Coder", "fake", "code")
            .unwrap();
        app.store.set_run_status(run.id, status).unwrap();
        (card, run)
    }

    fn runs_of(app: &App, card: &Card) -> usize {
        app.store.runs_for_card(card.id).unwrap().len()
    }

    fn stuck_items(app: &App) -> Vec<String> {
        let inbox = app.store.inbox().unwrap();
        assert!(inbox.iter().all(|item| item.kind == "stuck"), "{inbox:?}");
        inbox
            .iter()
            .map(|item| item.data["reason"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn a_card_at_the_round_limit_is_stuck_in_the_inbox() {
        let (_dir, app, engine, repo) = setup();
        let (card, _) = card_after_a_run(&app, repo, "failed");
        for _ in 1..Limits::default().round_limit {
            let run = app
                .store
                .add_run(Some(card.id), "Coder", "fake", "code")
                .unwrap();
            app.store.set_run_status(run.id, "failed").unwrap();
        }
        let step = engine.settle(card.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("the round limit is reached"));
        assert_eq!(step.started_run, None);
        assert_eq!(stuck_items(&app), ["the round limit is reached"]);
        assert_eq!(runs_of(&app, &card), 5);
    }

    #[test]
    fn a_card_that_spent_the_agents_token_budget_is_stuck_without_starting_a_run() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                agent_budgets: BTreeMap::from([("Coder".to_string(), 1000)]),
                ..Limits::default()
            },
        );
        let (card, run) = card_after_a_run(&app, repo, "failed");
        use_tokens(&app, run.id, 600);
        let step = engine.settle(card.id).unwrap();
        assert_eq!(step.started_run, Some(2), "there is budget left");
        let second = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        app.store.set_run_status(second.id, "failed").unwrap();
        use_tokens(&app, second.id, 400);

        let step = engine.settle(card.id).unwrap();
        assert_eq!(step.started_run, None);
        assert_eq!(
            step.waiting.as_deref(),
            Some("the token budget of 1000 is spent")
        );
        assert_eq!(stuck_items(&app), ["the token budget of 1000 is spent"]);
        assert_eq!(runs_of(&app, &card), 2);
    }

    #[test]
    fn the_reviewers_token_budget_stops_the_review() {
        let harness = Arc::new(FakeHarness::with_reviewer(&[]));
        let (_dir, app, _, repo) = setup_with_harness(None, harness.clone());
        let engine = Engine::new(
            app.clone(),
            harness,
            Workflow::default(),
            Limits {
                agent_budgets: BTreeMap::from([("Reviewer".to_string(), 100)]),
                ..Limits::default()
            },
        );
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        let code = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        use_tokens(&app, code.id, 100);
        let step = engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        assert_eq!(
            step.waiting.as_deref(),
            Some("the token budget of 100 is spent")
        );
        assert_eq!(runs_of(&app, &card), 1);
        assert_eq!(stuck_items(&app).len(), 1);
    }

    #[test]
    fn a_chat_does_not_count_against_the_run_limit() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                run_limit: 1,
                ..Limits::default()
            },
        );
        let chat = app.store.add_run(None, "Coder", "fake", "chat").unwrap();
        app.store.set_run_status(chat.id, "waiting").unwrap();

        let first = app.create_issue(repo, "First", "", PERSON).unwrap();
        let second = app.create_issue(repo, "Second", "", PERSON).unwrap();
        app.move_card(second.id, "Ready").unwrap();

        // A card's run uses the one run the limit allows; the chat does not.
        let coding = app
            .store
            .add_run(Some(first.id), "Coder", "fake", "code")
            .unwrap();
        let step = engine.settle(second.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("the run limit is reached"));
        app.store.set_run_status(coding.id, "succeeded").unwrap();
        assert!(engine.settle(second.id).unwrap().started_run.is_some());
    }

    #[test]
    fn a_spent_daily_budget_keeps_cards_from_starting_and_from_going_on() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                day_budget: Some(500),
                ..Limits::default()
            },
        );
        let (busy, run) = card_after_a_run(&app, repo, "succeeded");
        use_tokens(&app, run.id, 499);
        let ready = app.create_issue(repo, "Waiting", "", PERSON).unwrap();
        app.move_card(ready.id, "Ready").unwrap();
        let (second_round, failed) = card_after_a_run(&app, repo, "failed");

        // 499 of 500 tokens are used: cards go on.
        engine.tick(ready.id).unwrap();
        assert_eq!(column(&app, &ready), "In progress");
        app.move_card(ready.id, "Ready").unwrap();

        use_tokens(&app, failed.id, 1);
        let step = engine.settle(ready.id).unwrap();
        assert_eq!(column(&app, &ready), "Ready");
        assert_eq!(
            step.waiting.as_deref(),
            Some("the daily token budget is spent")
        );
        // A card that needs another round waits, too, instead of starting a run the router would refuse.
        let step = engine.settle(second_round.id).unwrap();
        assert_eq!(step.started_run, None);
        assert_eq!(
            step.waiting.as_deref(),
            Some("the daily token budget is spent")
        );
        assert!(app.store.inbox().unwrap().is_empty());
        // A card that is done with coding goes on to review.
        engine.tick(busy.id).unwrap();
        assert_eq!(column(&app, &busy), "In review");
    }

    #[test]
    fn a_stopped_run_puts_its_card_in_the_inbox_and_it_is_not_retried_by_itself() {
        let (_dir, app, engine, repo) = setup();
        let (card, _) = card_after_a_run(&app, repo, "stopped");
        for _ in 0..3 {
            let step = engine.settle(card.id).unwrap();
            assert_eq!(step.waiting.as_deref(), Some("stopped by a person"));
        }
        assert_eq!(runs_of(&app, &card), 1);
        assert_eq!(stuck_items(&app), ["stopped by a person"]);
    }

    #[test]
    fn a_retry_gives_a_stuck_card_a_fresh_start() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                agent_budgets: BTreeMap::from([("Coder".to_string(), 1000)]),
                ..Limits::default()
            },
        );
        let (card, run) = card_after_a_run(&app, repo, "failed");
        for _ in 1..Limits::default().round_limit {
            let run = app
                .store
                .add_run(Some(card.id), "Coder", "fake", "code")
                .unwrap();
            app.store.set_run_status(run.id, "failed").unwrap();
        }
        use_tokens(&app, run.id, 1000);
        engine.settle(card.id).unwrap();
        assert_eq!(stuck_items(&app).len(), 1);
        assert!(
            app.waiting(std::slice::from_ref(&card))
                .contains_key(&card.id)
        );

        // Neither the round limit nor the budget stops it any more.
        app.retry_card(card.id, "I raised the budget.").unwrap();
        assert!(app.waiting(std::slice::from_ref(&card)).is_empty());
        assert!(app.store.inbox().unwrap().is_empty());
        let step = engine.settle(card.id).unwrap();
        assert!(step.started_run.is_some(), "{step:?}");
        let judgements = app.store.judgements("retry").unwrap();
        assert_eq!(
            (judgements.len(), judgements[0].decider.as_str()),
            (1, "person")
        );

        // A stopped card, too, starts again instead of being stuck again.
        let (stopped, _) = card_after_a_run(&app, repo, "stopped");
        engine.settle(stopped.id).unwrap();
        app.retry_card(stopped.id, "Go on.").unwrap();
        let step = engine.settle(stopped.id).unwrap();
        assert!(step.started_run.is_some(), "{step:?}");
        assert_eq!(app.card(stopped.id).unwrap().column, "In progress");
    }

    #[test]
    fn a_rejected_change_goes_back_to_coding_with_the_reason_and_stays_the_same_change() {
        let harness = Arc::new(FakeHarness::default());
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();

        app.reject_change(change.id, "Please add a test.").unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert!(app.store.inbox().unwrap().is_empty());
        let step = engine.settle(card.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("coding, round 2"));
        assert_eq!(
            harness.notes.lock().unwrap()[1].as_deref(),
            Some("Please add a test.")
        );

        // The next round updates the change, which is offered again.
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let changes = app.store.changes(repo).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(
            (changes[0].id, changes[0].state.as_str()),
            (change.id, "open")
        );
        assert_ne!(changes[0].head, change.head);
    }

    /// Commits `text` as `path` on the repository's default branch, as someone merging elsewhere would.
    fn commit_to_base(app: &App, repo: i64, path: &str, text: &str) {
        let clone = std::path::PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        std::fs::create_dir_all(clone.join(path).parent().unwrap()).unwrap();
        std::fs::write(clone.join(path), text).unwrap();
        git(&clone, &["add", "."]).unwrap();
        git(&clone, &["commit", "--quiet", "-m", path]).unwrap();
    }

    /// Waits until the repository's checks of the card's change have ended.
    fn wait_for_checks(app: &App, card: &Card) {
        let give_up = Instant::now() + Duration::from_secs(60);
        while app
            .store
            .open_change(card.id)
            .unwrap()
            .unwrap()
            .checks
            .as_deref()
            == Some("pending")
        {
            assert!(Instant::now() < give_up, "the checks never ended");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn note(app: &App, card: &Card) -> Option<String> {
        app.card(card.id).unwrap().note
    }

    #[test]
    fn failing_checks_send_the_card_back_with_their_output_and_passing_ones_let_it_on() {
        let harness = Arc::new(FakeHarness::default());
        let (dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let count = dir.path().join("count.txt");
        let checks = format!(
            "echo x >> {}\necho 'boom: 3 tests failed'\nexit 1\n",
            count.display()
        );
        commit_to_base(&app, repo, ".moebius/checks", &checks);
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();

        engine.settle(card.id).unwrap();
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        let step = engine.tick(card.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("waiting for the checks"));
        // Asked again, whether they still run or ended, they are not started again.
        app.run_checks(card.id).unwrap();
        wait_for_checks(&app, &card);

        // The checks failed: the card goes back to coding and the next round is told what failed.
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert!(
            note(&app, &card)
                .unwrap()
                .starts_with("The checks failed:\nboom: 3 tests failed")
        );
        engine.tick(card.id).unwrap();
        let notes = harness.notes.lock().unwrap().clone();
        assert_eq!(notes.len(), 2);
        assert!(
            notes[1]
                .as_deref()
                .unwrap()
                .contains("boom: 3 tests failed")
        );
        assert_eq!(note(&app, &card), None, "the round succeeded");
        let leftovers = std::fs::read_dir(dir.path().join("data/worktrees/demo"))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("checks")
            })
            .count();
        assert_eq!(leftovers, 0, "the checks' worktree is removed");

        // The repository fixes its checks; the second round's commit is published on the same branch and checked.
        commit_to_base(&app, repo, ".moebius/checks", "ls run-*.txt\n");
        engine.tick(card.id).unwrap();
        engine.tick(card.id).unwrap();
        wait_for_checks(&app, &card);
        assert_eq!(app.store.changes(repo).unwrap().len(), 1);
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.checks_of(&change).unwrap(), "success");
        assert_eq!(std::fs::read_to_string(count).unwrap(), "x\n");
    }

    #[test]
    fn a_repository_without_checks_has_none_and_the_card_goes_on() {
        let (_dir, app, engine, repo) = setup();
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.checks_of(&change).unwrap(), "none");
        assert_eq!(change.checks, None);
    }

    #[test]
    fn the_checks_are_read_from_the_default_branch_not_from_the_change() {
        let harness = Arc::new(FakeHarness {
            files: vec![(".moebius/checks", "exit 1\n")],
            ..FakeHarness::default()
        });
        let (_dir, app, engine, repo) = setup_with_harness(None, harness);
        commit_to_base(&app, repo, ".moebius/checks", "exit 0\n");
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.tick(card.id).unwrap();
        engine.tick(card.id).unwrap();
        wait_for_checks(&app, &card);
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.checks_of(&change).unwrap(), "success");
    }

    #[test]
    fn a_change_that_no_longer_merges_goes_back_with_the_conflicting_files() {
        let harness = Arc::new(FakeHarness::default());
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        // The base meanwhile took a different file of the same name.
        commit_to_base(&app, repo, "run-1.txt", "something else\n");
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        let note = note(&app, &card).unwrap();
        assert!(note.contains("run-1.txt"), "{note}");
        assert!(
            note.contains("Merge main into your branch and resolve the conflicts"),
            "{note}"
        );

        engine.tick(card.id).unwrap();
        assert_eq!(harness.notes.lock().unwrap()[1].as_deref(), Some(&*note));
    }

    #[test]
    fn a_conflict_that_appears_while_the_change_waits_to_be_merged_sends_it_back() {
        let (_dir, app, engine, repo) = setup();
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        assert_eq!(app.store.inbox().unwrap().len(), 1);
        let step = engine.tick(card.id).unwrap();
        assert_eq!(
            step.waiting.as_deref(),
            Some("waiting for a person to merge")
        );

        commit_to_base(&app, repo, "run-1.txt", "something else\n");
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert!(note(&app, &card).unwrap().contains("run-1.txt"));
        assert!(app.store.inbox().unwrap().is_empty());
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

    impl crate::forge::Forge for SlowForge {
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
        let dry = recording_api(&lua, &engine.limits, &columns, Rc::default()).unwrap();
        assert_eq!(names(&dry), names(&real));
        let real = engine.card_table(&lua, &card).unwrap();
        let dry = made_up_card(&lua, "Backlog", "Fix the parser", PERSON).unwrap();
        assert_eq!(names(&dry), names(&real));
    }
}
