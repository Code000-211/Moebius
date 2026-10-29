//! The engine: runs the script of a card's column once (`tick`) or until the card stays where it is (`settle`), and
//! applies what the script did: the column it moves the card to, with the note it gives, and why the card waits. The
//! script of a column is the repository's override, or the built-in one (see `workflow`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use super::sandbox::run_sandboxed;
use super::{Column, Limits, Runs, Workflow, built_in_script, repository_script};
use crate::app::{App, AppError, columns};
use crate::store::Card;

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

/// Runs the scripts of the workflow's columns, with the harness they start runs on and the limits they see.
pub struct Engine {
    pub(super) app: Arc<App>,
    pub(super) runs: Arc<dyn Runs>,
    pub(super) workflow: Workflow,
    /// Replaced when the limits are edited while the server runs (see `set_limits`).
    limits: RwLock<Limits>,
}

/// What a script call did through the `moebius` table, applied when it returns.
#[derive(Default)]
pub(super) struct Effects {
    pub(super) move_to: Option<String>,
    pub(super) note: Option<String>,
    pub(super) moved: Option<String>,
    pub(super) waiting: Option<String>,
    pub(super) started_run: Option<i64>,
}

impl Engine {
    pub fn new(app: Arc<App>, runs: Arc<dyn Runs>, workflow: Workflow, limits: Limits) -> Self {
        Self {
            app,
            runs,
            workflow,
            limits: RwLock::new(limits),
        }
    }

    /// The limits as they are now.
    pub(super) fn limits(&self) -> Limits {
        self.limits
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Changes the limits scripts see (`moebius.run_limit`, `moebius.round_limit`, `moebius.day_budget`); the next
    /// script call reads them.
    pub fn set_limits(&self, run_limit: u32, round_limit: u32, day_budget: Option<i64>) {
        let mut limits = self.limits.write().unwrap_or_else(|e| e.into_inner());
        limits.run_limit = run_limit;
        limits.round_limit = round_limit;
        limits.day_budget = day_budget;
    }

    /// The script for `column`: the repository's override on its default branch, or the built-in one.
    fn script(&self, card: &Card, column: &Column) -> Result<String, BoardError> {
        let repository = self
            .app
            .store
            .repository(card.repository_id)
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::NotFound(format!("repository {}", card.repository_id)))?;
        if let Some(source) = repository_script(&repository, &column.script) {
            return Ok(source);
        }
        built_in_script(&column.script)
            .map(String::from)
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

    pub(super) fn run_script(
        &self,
        name: &str,
        source: &str,
        card: &Card,
        effects: Rc<RefCell<Effects>>,
    ) -> mlua::Result<()> {
        run_sandboxed(
            name,
            source,
            &self.limits(),
            |lua, deadline| self.api(lua, card, effects, deadline.clone()),
            |lua| self.card_table(lua, card),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::app::{PERSON, columns};
    use crate::board::testing::setup;
    use crate::repos::git;

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
        app.merge_change(&crate::decide::Actor::Person, change.id, "fine")
            .unwrap();
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
        app.approve_issue(&crate::decide::Actor::Person, card.id, "fine")
            .unwrap();
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

        app.approve_issue(&crate::decide::Actor::Person, card.id, "fine by me")
            .unwrap();
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
}
