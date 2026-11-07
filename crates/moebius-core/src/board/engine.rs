//! The engine: runs the script of a card's column once (`tick`, which skips a held card) or until the card stays where
//! it is (`settle`), and applies what the script did: the column it moves the card to, with the note it gives, and why
//! the card waits. The script of a column is the repository's override, or the built-in one (see `kanban`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use super::sandbox::run_sandboxed;
use super::{CardRuns, Column, Kanban, Limits, built_in_script, repository_script};
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
    #[error("no column {0:?} in the Kanban")]
    NoColumn(String),
    #[error("{script}: {message}")]
    Script { script: String, message: String },
}

/// Runs the scripts of the Kanban's columns, with the harness they start runs on and the limits they see.
pub struct Engine {
    pub(super) app: Arc<App>,
    pub(super) runs: Arc<dyn CardRuns>,
    pub(super) kanban: Kanban,
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
    /// Whether the script closed the card's issue.
    pub(super) close: bool,
}

impl Engine {
    pub fn new(app: Arc<App>, runs: Arc<dyn CardRuns>, kanban: Kanban, limits: Limits) -> Self {
        Self {
            app,
            runs,
            kanban,
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

    /// The script for `column`: the project's override, or the built-in one.
    fn script(&self, card: &Card, column: &Column) -> Result<String, BoardError> {
        let repository = self
            .app
            .store
            .repository(card.repository_id)
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::NotFound(format!("repository {}", card.repository_id)))?;
        if let Some(source) = repository_script(&self.app, &repository, &column.script) {
            return Ok(source);
        }
        built_in_script(&column.script)
            .map(String::from)
            .ok_or_else(|| BoardError::Script {
                script: column.script.clone(),
                message: "no such script".into(),
            })
    }

    /// Runs the script of the card's column once and applies what it did. A held card has no script run: it stays
    /// where it is, and nothing starts for it, until a person or the Moebius agent releases it.
    pub fn tick(&self, card_id: i64) -> Result<Step, BoardError> {
        let card = self.app.card(card_id)?;
        if card.held.is_some() {
            return Ok(Step::default());
        }
        let column = self
            .kanban
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
        self.apply(&card, effects.take())
    }

    /// Applies what a script did to `card`, which is the card as the script saw it.
    fn apply(&self, card: &Card, mut effects: Effects) -> Result<Step, BoardError> {
        let card_id = card.id;
        if effects.close {
            // Closed on the forge first; the card then goes to Done, whatever else the script asked for. A forge that
            // refuses leaves the card where it is, waiting with the reason, and is not asked at every tick.
            if let Some(reason) = self.app.close_for_script(card_id)? {
                self.app.set_waiting(card_id, Some(reason.clone()));
                return Ok(Step {
                    moved: None,
                    waiting: Some(reason),
                    started_run: effects.started_run,
                });
            }
            self.app.set_waiting(card_id, None);
            return Ok(Step {
                moved: (card.column != columns::DONE).then(|| columns::DONE.into()),
                waiting: None,
                started_run: effects.started_run,
            });
        }
        if let Some(to) = effects.move_to.take()
            && to != card.column
        {
            // Only if the card is still where the script saw it: a person or the forge poll may have moved it since.
            let moved = self
                .app
                .move_card_from(card_id, &card.column, &to, effects.note.as_deref())?
                .is_some();
            if !moved {
                // What the script worked out, why the card waits included, was for a column it has left.
                return Ok(Step {
                    moved: None,
                    waiting: None,
                    started_run: effects.started_run,
                });
            }
            if card.column == columns::READY_TO_MERGE {
                // The change is no longer waiting for a person.
                self.app
                    .store
                    .resolve_inbox_for_card(card_id)
                    .map_err(AppError::from)?;
            }
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
        for _ in 0..self.kanban.columns.len() * 2 {
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

    #[test]
    fn the_default_kanban_moves_a_card_from_backlog_to_done() {
        let (_dir, app, engine, repo) = setup();
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();

        // Backlog -> Not started -> In progress, where the Coder run starts.
        let step = engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "In progress");
        assert!(step.started_run.is_some());
        assert_eq!(step.waiting.as_deref(), Some("coding"));

        // The run has succeeded: the built-in Reviewer reviews the change, with no setting to make.
        let step = engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "In review");
        assert!(step.started_run.is_some());

        // Both axes approve (one run each), so its change is offered and waits for a person.
        engine.settle(card.id).unwrap();
        let step = engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Ready to merge");
        assert_eq!(
            step.waiting.as_deref(),
            Some("waiting for a person to merge")
        );
        let runs = app.store.runs_for_card(card.id).unwrap();
        let agents: Vec<_> = runs.iter().map(|r| r.agent.as_str()).collect();
        assert_eq!(agents, ["Coder", "Reviewer", "Reviewer"]);
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
    fn a_move_worked_out_from_a_stale_card_is_not_applied() {
        use super::Effects;
        let (_dir, app, engine, repo) = setup();
        let card = app.create_issue(repo, "Add", "", PERSON).unwrap();
        // The script saw the card in the backlog; meanwhile a person or the forge poll finished it.
        let seen = app.card(card.id).unwrap();
        app.move_card(card.id, columns::DONE).unwrap();
        let moves = app.store.moves_for_card(card.id).unwrap().len();

        let step = engine
            .apply(
                &seen,
                Effects {
                    move_to: Some("Not started".into()),
                    note: Some("stale".into()),
                    waiting: Some("stale".into()),
                    ..Effects::default()
                },
            )
            .unwrap();
        assert_eq!(step.moved, None);
        assert_eq!(step.waiting, None);
        assert!(app.waiting(&[app.card(card.id).unwrap()]).is_empty());
        let card = app.card(card.id).unwrap();
        assert_eq!(card.column, columns::DONE);
        assert_ne!(card.note.as_deref(), Some("stale"));
        assert_eq!(app.store.moves_for_card(card.id).unwrap().len(), moves);
    }

    #[test]
    fn an_offer_is_not_made_for_a_card_moved_while_the_change_was_published() {
        let (_dir, app, _engine, repo) = setup();
        let card = app.create_issue(repo, "Add", "", PERSON).unwrap();
        let seen = app.card(card.id).unwrap().column;
        let change = app
            .store
            .add_change(repo, Some(card.id), "moebius/card-1", None, "abc")
            .unwrap();
        // While the change was published, a person finished the card.
        app.move_card(card.id, columns::DONE).unwrap();

        let (_, moved) = app.finish_offer(card.id, &seen, change, None).unwrap();
        assert!(!moved);
        assert_eq!(app.card(card.id).unwrap().column, columns::DONE);
        assert!(app.store.inbox_for_card(card.id).unwrap().is_empty());
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
        assert_eq!(app.card(card.id).unwrap().column, "Not started");
        assert!(app.store.inbox().unwrap().is_empty());
        assert_eq!(
            app.store.judgements("approve_issue").unwrap()[0].decider,
            "person"
        );
    }

    #[test]
    fn a_repository_script_that_moves_a_card_to_ready_moves_it_to_not_started() {
        let (_dir, app, engine, repo) = setup();
        let put =
            |file: &str, text: &str| crate::harness::repo::testing::put(&app, "demo", file, text);
        put(
            "backlog.luau",
            "function on_card(card) moebius.move('Ready') end",
        );
        put(
            "ready.luau",
            "function on_card(card) moebius.wait('held') end",
        );
        let card = app.create_issue(repo, "Old script", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Not started");
    }

    #[test]
    fn a_project_overrides_a_script_in_its_settings() {
        let (_dir, app, engine, repo) = setup();
        crate::harness::repo::testing::put(
            &app,
            "demo",
            "ready.luau",
            "function on_card(card) moebius.wait('held by the repository') end",
        );
        let card = app.create_issue(repo, "Held", "", PERSON).unwrap();
        let step = engine.settle(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Not started");
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
            assert_eq!(app.card(card.id).unwrap().column, "Not started");
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
        assert_eq!(app.card(card.id).unwrap().column, "Not started");
        assert_eq!(step.waiting.as_deref(), Some("waiting for #99"));
    }

    #[test]
    fn a_held_card_is_skipped_before_its_script_is_called_and_ticks_again_once_released() {
        let (_dir, app, engine, repo) = setup();
        // A script that fails whenever it is called, and would move the card on if it did not.
        crate::harness::repo::testing::put(
            &app,
            "demo",
            "backlog.luau",
            "function on_card(card) moebius.move('Done') error('the script was called') end",
        );
        let card = app.create_issue(repo, "Held", "", PERSON).unwrap();
        assert!(app.store.hold_card(card.id, PERSON, Some("later")).unwrap());

        for _ in 0..2 {
            assert_eq!(engine.tick(card.id).unwrap(), super::Step::default());
            assert_eq!(engine.settle(card.id).unwrap(), super::Step::default());
        }
        assert_eq!(app.card(card.id).unwrap().column, "Backlog");
        assert!(app.store.runs_for_card(card.id).unwrap().is_empty());

        assert!(app.store.release_card(card.id).unwrap());
        let error = engine.tick(card.id).unwrap_err().to_string();
        assert!(error.contains("the script was called"), "{error}");
    }

    #[test]
    fn a_script_reads_who_holds_a_card_and_why() {
        let (_dir, app, engine, repo) = setup();
        let card = app.create_issue(repo, "Held", "", PERSON).unwrap();
        let lua = mlua::Lua::new();
        assert!(
            !engine
                .card_table(&lua, &card)
                .unwrap()
                .contains_key("held")
                .unwrap()
        );
        app.store
            .hold_card(card.id, "agent:Moebius", Some("needs a design"))
            .unwrap();
        let card = app.card(card.id).unwrap();
        let held: mlua::Table = engine.card_table(&lua, &card).unwrap().get("held").unwrap();
        assert_eq!(held.get::<String>("by").unwrap(), "agent:Moebius");
        assert_eq!(held.get::<String>("reason").unwrap(), "needs a design");
        assert_eq!(held.get::<i64>("at").unwrap(), card.held.unwrap().at);
    }

    #[test]
    fn a_script_reads_the_parent_the_children_the_labels_and_the_claim_of_a_card() {
        let (_dir, app, engine, repo) = setup();
        let map = app.create_issue(repo, "Map", "", PERSON).unwrap();
        let ticket = app.create_issue(repo, "Ticket", "", PERSON).unwrap();
        app.set_parent(ticket.id, Some(map.number)).unwrap();
        app.set_labels(ticket.id, &["wayfinder:spike".into()])
            .unwrap();
        app.claim_issue(&crate::decide::Actor::Person, ticket.id, Some("session 1"))
            .unwrap();
        let lua = mlua::Lua::new();
        let table = |card: &crate::store::Card| engine.card_table(&lua, card).unwrap();

        let below = table(&app.card(ticket.id).unwrap());
        assert_eq!(below.get::<i64>("parent").unwrap(), map.number);
        assert_eq!(
            below.get::<Vec<String>>("labels").unwrap(),
            ["wayfinder:spike"]
        );
        assert_eq!(below.get::<String>("claimed_by").unwrap(), "session 1");
        assert_eq!(
            below.get::<mlua::Table>("children").unwrap().len().unwrap(),
            0
        );

        let above = table(&app.card(map.id).unwrap());
        assert_eq!(above.get::<Option<i64>>("parent").unwrap(), None);
        let children: mlua::Table = above.get("children").unwrap();
        let child: mlua::Table = children.get(1).unwrap();
        assert_eq!(child.get::<i64>("number").unwrap(), ticket.number);
        assert_eq!(child.get::<String>("title").unwrap(), "Ticket");
        assert_eq!(child.get::<String>("state").unwrap(), "open");
        assert_eq!(child.get::<String>("column").unwrap(), "Backlog");
    }

    #[test]
    fn the_default_kanban_starts_no_work_on_a_parent_and_closes_it_with_its_last_sub_issue() {
        let (_dir, app, engine, repo) = setup();
        let map = app.create_issue(repo, "Map", "", PERSON).unwrap();
        let (a, b) = (
            app.create_issue(repo, "A", "", PERSON).unwrap(),
            app.create_issue(repo, "B", "", PERSON).unwrap(),
        );
        for ticket in [&a, &b] {
            app.set_parent(ticket.id, Some(map.number)).unwrap();
        }
        // The parent is ready, and waits for its open sub-issues instead of starting a run.
        app.move_card(map.id, columns::NOT_STARTED).unwrap();
        let step = engine.settle(map.id).unwrap();
        assert_eq!(
            step.waiting.as_deref(),
            Some(&*format!(
                "waiting for sub-issues #{}, #{}",
                a.number, b.number
            ))
        );
        assert!(app.store.runs_for_card(map.id).unwrap().is_empty());

        app.close_card(a.id).unwrap();
        let step = engine.settle(map.id).unwrap();
        assert_eq!(
            step.waiting.as_deref(),
            Some(&*format!("waiting for sub-issues #{}", b.number))
        );

        // The last one closes it, and the issue is closed with it.
        app.close_card(b.id).unwrap();
        engine.settle(map.id).unwrap();
        let map = app.card(map.id).unwrap();
        assert_eq!((map.column.as_str(), map.closed), ("Done", true));
        assert!(app.store.issue_closed(map.id).unwrap());
    }

    #[test]
    fn an_outside_issue_that_names_a_parent_does_not_hold_or_close_it() {
        let (_dir, app, engine, repo) = setup();
        let map = app.create_issue(repo, "Map", "", PERSON).unwrap();
        // Written by anyone, its description says "Part of #1"; nobody approved it.
        let outside = app
            .store
            .add_card(repo, "Outside", "Part of #1", "mallory", columns::BACKLOG)
            .unwrap();
        app.store.set_parent(outside.id, Some(map.number)).unwrap();
        app.move_card(map.id, columns::NOT_STARTED).unwrap();

        // It is not a sub-issue, so the parent is a card like any other and starts its work, and closing the outside
        // issue closes nothing.
        let step = engine.settle(map.id).unwrap();
        assert_eq!(app.card(map.id).unwrap().column, "In progress");
        assert!(step.started_run.is_some());
        app.close_card(outside.id).unwrap();
        assert!(!app.card(map.id).unwrap().closed);
    }

    #[test]
    fn a_parent_that_has_work_of_its_own_keeps_its_normal_flow() {
        let (_dir, app, engine, repo) = setup();
        let map = app.create_issue(repo, "Map", "", PERSON).unwrap();
        let ticket = app.create_issue(repo, "Ticket", "", PERSON).unwrap();
        app.set_parent(ticket.id, Some(map.number)).unwrap();
        // It was coded before the ticket was put under it.
        let run = app
            .store
            .add_run(Some(map.id), "Coder", "test", "code")
            .unwrap();
        app.store.set_run_status(run.id, "succeeded").unwrap();
        app.move_card(map.id, columns::NOT_STARTED).unwrap();
        let step = engine.tick(map.id).unwrap();
        assert_eq!(step.moved.as_deref(), Some("In progress"));
        assert!(!app.card(map.id).unwrap().closed);
    }

    #[test]
    fn a_wayfinder_map_and_its_tickets_are_questions_that_the_default_kanban_never_starts() {
        let (_dir, app, engine, repo) = setup();
        let map = app.create_issue(repo, "Map", "", PERSON).unwrap();
        let ticket = app.create_issue(repo, "Ticket", "", PERSON).unwrap();
        let task = app.create_issue(repo, "Task", "", PERSON).unwrap();
        app.set_parent(ticket.id, Some(map.number)).unwrap();
        app.set_labels(map.id, &["wayfinder:map".to_string()])
            .unwrap();
        app.set_labels(ticket.id, &["bug".into(), "Wayfinder:Research".into()])
            .unwrap();
        // The prefix is matched in any case (the ticket above); a label that only starts like it is no wayfinder label.
        app.set_labels(task.id, &["wayfinder".to_string()]).unwrap();
        for card in [&map, &ticket, &task] {
            app.move_card(card.id, columns::NOT_STARTED).unwrap();
        }

        for card in [&map, &ticket] {
            let step = engine.settle(card.id).unwrap();
            assert!(
                step.waiting.as_deref().unwrap().contains("wayfinder"),
                "{:?}",
                step.waiting
            );
            assert!(step.started_run.is_none());
            assert_eq!(app.card(card.id).unwrap().column, "Not started");
            assert!(app.store.runs_for_card(card.id).unwrap().is_empty());
        }

        // Even when every ticket is closed the map is not closed for them: the wayfinder session closes it.
        app.close_card(ticket.id).unwrap();
        engine.settle(map.id).unwrap();
        let map_now = app.card(map.id).unwrap();
        assert_eq!(
            (map_now.column.as_str(), map_now.closed),
            ("Not started", false)
        );

        // A task that is no question starts as any card does.
        engine.settle(task.id).unwrap();
        assert_eq!(app.card(task.id).unwrap().column, "In progress");
    }

    #[test]
    fn done_records_merged_for_a_card_whose_change_was_merged_and_closed_for_one_that_was_closed() {
        let (_dir, app, engine, repo) = setup();
        let card = app
            .create_issue(repo, "Closed unmerged", "", PERSON)
            .unwrap();
        let judgement = |card_id| {
            app.store
                .add_judgement(&crate::store::NewJudgement {
                    kind: "approve_issue".into(),
                    choice: "approve".into(),
                    decider: "person".into(),
                    card_id: Some(card_id),
                    ..Default::default()
                })
                .unwrap()
        };
        let made = judgement(card.id);
        app.close_card(card.id).unwrap();
        engine.settle(card.id).unwrap();
        let outcome = |id| app.store.judgement(id).unwrap().unwrap().outcome;
        assert_eq!(outcome(made.id).as_deref(), Some("closed"));
    }
}
