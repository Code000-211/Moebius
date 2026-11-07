//! Blocking, unblocking and deleting a card (issue #183) against a harness that is a fake: its runs go on until they
//! are stopped, and end a moment after the stop, as a real agent's do. Blocking stops the runs and keeps the board
//! from working on the card; deleting takes the card with everything stored for it and what it left in the clone.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use moebius_core::app::{App, AppError, CardStopWaits, PERSON};
use moebius_core::board::{AgentInfo, CardRuns, Engine, Kanban, Limits, StartError, Variant};
use moebius_core::decide::Actor;
use moebius_core::harness::remote::RunnerInfo;
use moebius_core::repos::git;
use moebius_core::store::{Card, Run, Store};
use moebius_plugin_api::Host;

/// A harness whose runs wait in a worktree until they are stopped.
#[derive(Default)]
struct Waiting {
    live: Mutex<BTreeSet<i64>>,
    stopped: Mutex<Vec<i64>>,
    /// How many more stops are answered with "not running", as for a run its harness has not registered yet.
    refusals: Mutex<usize>,
    /// Whether a stop is accepted but the run goes on, as for an agent that does not settle.
    ignores_stop: AtomicBool,
    app: Mutex<Option<Arc<App>>>,
}

impl CardRuns for Waiting {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        (purpose, _): (&str, Option<&str>),
        _: Variant,
    ) -> Result<Run, StartError> {
        let store = &app.store;
        let run = store
            .add_run(Some(card.id), agent, "fake", purpose)
            .map_err(|e| StartError::Store(e.to_string()))?;
        let repository = store
            .repository(card.repository_id)
            .map_err(|e| StartError::Store(e.to_string()))?
            .ok_or(StartError::RepositoryGone)?;
        let worktree = app
            .repos
            .add_worktree(
                &repository.name,
                Path::new(&repository.path),
                run.id,
                &repository.base_ref(),
            )
            .map_err(|e| StartError::WorktreeFailed(e.to_string()))?;
        let stored = |e: moebius_core::store::StoreError| StartError::Store(e.to_string());
        store
            .set_run_worktree(run.id, &worktree.path.to_string_lossy())
            .map_err(stored)?;
        store.set_run_status(run.id, "running").map_err(stored)?;
        self.live.lock().unwrap().insert(run.id);
        *self.app.lock().unwrap() = Some(app.clone());
        Ok(store.run(run.id).unwrap().unwrap())
    }

    fn has_agent(&self, name: &str) -> bool {
        name == "Coder"
    }

    fn runners(&self) -> Vec<RunnerInfo> {
        Vec::new()
    }

    fn agents(&self) -> Vec<AgentInfo> {
        Vec::new()
    }

    fn token_budget(&self, _: &str) -> Option<i64> {
        None
    }

    fn steer(&self, _: i64, _: &str) -> bool {
        false
    }

    /// The run ends on another thread, as a harness's does once its agent has settled: the worktree goes first, then
    /// the status is set.
    fn stop(&self, run_id: i64) -> bool {
        {
            let mut refusals = self.refusals.lock().unwrap();
            if *refusals > 0 {
                *refusals -= 1;
                return false;
            }
        }
        if self.ignores_stop.load(Ordering::SeqCst) {
            self.stopped.lock().unwrap().push(run_id);
            return true;
        }
        if !self.live.lock().unwrap().remove(&run_id) {
            return false;
        }
        self.stopped.lock().unwrap().push(run_id);
        let app = self.app.lock().unwrap().clone().expect("a run was started");
        std::thread::spawn(move || {
            let run = app.store.run(run_id).unwrap().unwrap();
            let repository = app
                .store
                .repository(app.card(run.card_id.unwrap()).unwrap().repository_id)
                .unwrap()
                .unwrap();
            let worktree = PathBuf::from(run.worktree.unwrap());
            app.repos
                .remove_worktree(Path::new(&repository.path), &worktree)
                .unwrap();
            app.store.set_run_status(run_id, "stopped").unwrap();
        });
        true
    }
}

struct Setup {
    _dir: tempfile::TempDir,
    app: Arc<App>,
    runs: Arc<Waiting>,
    engine: Engine,
    repository: i64,
}

fn setup() -> Setup {
    setup_with(CardStopWaits::default())
}

fn setup_with(waits: CardStopWaits) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "hello\n").unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
    let host = Arc::new(Host::new(std::time::Duration::from_secs(1)));
    let mut app = App::new(store, host, &dir.path().join("data"));
    app.card_stop_waits = waits;
    let app = Arc::new(app);
    let repository = app
        .add_repository("demo", origin.to_str().unwrap())
        .unwrap()
        .id;
    let runs = Arc::new(Waiting::default());
    let engine = Engine::new(
        app.clone(),
        runs.clone(),
        Kanban::default(),
        Limits::default(),
    );
    Setup {
        _dir: dir,
        app,
        runs,
        engine,
        repository,
    }
}

fn judgement_kinds(app: &App, kind: &str) -> Vec<(String, Option<i64>)> {
    app.store
        .judgements(kind)
        .unwrap()
        .into_iter()
        .map(|j| (j.decider, j.card_id))
        .collect()
}

fn context(app: &App, kind: &str) -> serde_json::Value {
    let judgement = app.store.judgements(kind).unwrap().remove(0);
    serde_json::from_str(&app.store.blobs().get(&judgement.context[0]).unwrap()).unwrap()
}

fn kinds_published(
    events: &mut tokio::sync::broadcast::Receiver<moebius_core::bus::AppEvent>,
) -> Vec<String> {
    let mut kinds = Vec::new();
    while let Ok(event) = events.try_recv() {
        kinds.push(event.kind);
    }
    kinds
}

#[test]
fn blocking_stops_the_run_and_the_board_leaves_a_held_card_alone_until_it_is_released() {
    let s = setup();
    let app = &s.app;
    let card = app.create_issue(s.repository, "Work", "", PERSON).unwrap();
    s.engine.settle(card.id).unwrap();
    let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
    assert_eq!(run.status, "running");
    let column = app.card(card.id).unwrap().column;
    assert_eq!(column, "In progress");
    let mut events = app.bus.subscribe();

    let blocked = app
        .block_card(
            &Actor::Person,
            &*s.runs,
            card.id,
            Some("wait for the design"),
        )
        .unwrap();
    let held = blocked.held.unwrap();
    assert_eq!(
        (held.by.as_str(), held.reason.as_deref()),
        ("person", Some("wait for the design"))
    );
    // The run was stopped with the stop runs have, and it stays in its column.
    assert_eq!(*s.runs.stopped.lock().unwrap(), [run.id]);
    assert_eq!(app.card(card.id).unwrap().column, column);
    assert_eq!(
        judgement_kinds(app, "stop"),
        [("person".to_string(), Some(card.id))]
    );
    assert_eq!(
        judgement_kinds(app, "block_card"),
        [("person".to_string(), Some(card.id))]
    );
    assert_eq!(context(app, "block_card")["reason"], "wait for the design");
    assert!(kinds_published(&mut events).contains(&"card_updated".to_string()));

    // Wait for the run to be over, then tick: no script, no run, no inbox item, no move.
    while app.store.run(run.id).unwrap().unwrap().ended_at.is_none() {
        std::thread::yield_now();
    }
    for _ in 0..2 {
        let step = s.engine.settle(card.id).unwrap();
        assert_eq!(
            (step.moved, step.started_run, step.waiting),
            (None, None, None)
        );
    }
    assert_eq!(app.store.runs_for_card(card.id).unwrap().len(), 1);
    assert_eq!(app.card(card.id).unwrap().column, column);
    assert!(app.store.inbox().unwrap().is_empty());

    // Held already; and the Moebius agent may release what a person held, like editing a card.
    let again = app.block_card(&Actor::Person, &*s.runs, card.id, None);
    assert!(matches!(again, Err(AppError::Conflict(_))));
    let chat = app.store.add_run(None, "Moebius", "fake", "chat").unwrap();
    let agent = Actor::Agent {
        name: "Moebius".into(),
        run: chat.id,
    };
    let released = app
        .unblock_card(&agent, card.id, "the design is in")
        .unwrap();
    assert_eq!(released.held, None);
    assert_eq!(
        judgement_kinds(app, "unblock_card"),
        [("agent:Moebius".to_string(), Some(card.id))]
    );
    let unblocked = context(app, "unblock_card");
    assert_eq!(
        (unblocked["reason"].as_str(), unblocked["held_by"].as_str()),
        (Some("the design is in"), Some("person"))
    );
    let again = app.unblock_card(&Actor::Person, card.id, "");
    assert!(matches!(again, Err(AppError::Conflict(_))));
    assert!(kinds_published(&mut events).contains(&"card_updated".to_string()));

    // The next tick treats it like any other card: the run the block stopped is no reason to call the card stuck, so
    // the card works on, with no inbox item.
    assert!(!app.card(card.id).unwrap().held.is_some());
    s.engine.settle(card.id).unwrap();
    assert!(app.store.inbox().unwrap().is_empty());
    assert_eq!(app.store.runs_for_card(card.id).unwrap().len(), 2);
    assert_eq!(app.card(card.id).unwrap().retried_after, Some(run.id));
}

#[test]
fn a_run_stopped_before_the_block_still_waits_for_a_retry_after_an_unblock() {
    let s = setup();
    let app = &s.app;
    let card = app.create_issue(s.repository, "Work", "", PERSON).unwrap();
    s.engine.settle(card.id).unwrap();
    let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
    app.stop_run(&Actor::Person, &*s.runs, run.id, "").unwrap();
    while app.store.run(run.id).unwrap().unwrap().ended_at.is_none() {
        std::thread::yield_now();
    }
    // The hold must come in a later second than the stop (the clocks of the records are in seconds).
    let ended = app.store.run(run.id).unwrap().unwrap().ended_at.unwrap();
    while moebius_core::store::now() <= ended {
        std::thread::sleep(Duration::from_millis(20));
    }
    app.block_card(&Actor::Person, &*s.runs, card.id, None)
        .unwrap();
    app.unblock_card(&Actor::Person, card.id, "").unwrap();
    let step = s.engine.settle(card.id).unwrap();
    assert_eq!(step.waiting.as_deref(), Some("stopped by a person"));
}

#[test]
fn blocking_keeps_trying_to_stop_a_run_its_harness_has_not_registered_yet() {
    let s = setup();
    let app = &s.app;
    let card = app.create_issue(s.repository, "Work", "", PERSON).unwrap();
    s.engine.settle(card.id).unwrap();
    let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
    *s.runs.refusals.lock().unwrap() = 3;

    app.block_card(&Actor::Person, &*s.runs, card.id, None)
        .unwrap();
    // Stopped once, after the refusals; the stop was recorded once.
    assert_eq!(*s.runs.stopped.lock().unwrap(), [run.id]);
    assert_eq!(*s.runs.refusals.lock().unwrap(), 0);
    assert_eq!(judgement_kinds(app, "stop").len(), 1);
    assert_eq!(judgement_kinds(app, "block_card").len(), 1);
}

#[test]
fn a_block_that_cannot_stop_a_run_in_time_releases_the_card_and_records_nothing() {
    let s = setup_with(CardStopWaits {
        starting: Duration::from_millis(100),
        ..CardStopWaits::default()
    });
    let app = &s.app;
    let card = app.create_issue(s.repository, "Work", "", PERSON).unwrap();
    s.engine.settle(card.id).unwrap();
    *s.runs.refusals.lock().unwrap() = usize::MAX;

    let error = app
        .block_card(&Actor::Person, &*s.runs, card.id, None)
        .unwrap_err();
    assert!(matches!(error, AppError::Conflict(_)), "{error}");
    assert_eq!(app.card(card.id).unwrap().held, None);
    assert!(judgement_kinds(app, "block_card").is_empty());

    // Once the run can be stopped the block goes through.
    *s.runs.refusals.lock().unwrap() = 0;
    app.block_card(&Actor::Person, &*s.runs, card.id, None)
        .unwrap();
    assert!(app.card(card.id).unwrap().held.is_some());
}

#[test]
fn deleting_stops_a_run_that_was_still_starting_when_the_first_stop_came() {
    let s = setup();
    let app = &s.app;
    let card = app.create_issue(s.repository, "Work", "", PERSON).unwrap();
    s.engine.settle(card.id).unwrap();
    let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
    *s.runs.refusals.lock().unwrap() = 2;

    app.delete_card(&Actor::Person, &*s.runs, card.id, "")
        .unwrap();
    assert_eq!(*s.runs.stopped.lock().unwrap(), [run.id]);
    assert_eq!(app.store.card(card.id).unwrap(), None);
}

#[test]
fn a_delete_whose_runs_do_not_end_in_time_is_a_conflict_and_the_card_is_released() {
    let s = setup_with(CardStopWaits {
        ending: Duration::from_millis(150),
        ..CardStopWaits::default()
    });
    let app = &s.app;
    let card = app.create_issue(s.repository, "Work", "", PERSON).unwrap();
    s.engine.settle(card.id).unwrap();
    let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
    s.runs.ignores_stop.store(true, Ordering::SeqCst);

    let error = app
        .delete_card(&Actor::Person, &*s.runs, card.id, "")
        .unwrap_err();
    assert!(matches!(error, AppError::Conflict(_)), "{error}");
    // The run was asked to stop once, not again at every look; the card is still there and works again.
    assert_eq!(*s.runs.stopped.lock().unwrap(), [run.id]);
    assert_eq!(app.card(card.id).unwrap().held, None);
    assert!(judgement_kinds(app, "delete_card").is_empty());
}

#[test]
fn a_card_held_after_the_board_looked_gets_no_run_from_the_harness_that_starts_it() {
    let s = setup();
    let app = &s.app;
    let card = app.create_issue(s.repository, "Work", "", PERSON).unwrap();
    // The card the board holds is as it was before the hold.
    assert!(app.store.hold_card(card.id, PERSON, None).unwrap());
    let error = moebius_core::board::may_start(app, &card).unwrap_err();
    assert!(matches!(error, StartError::Held));
    app.store.release_card(card.id).unwrap();
    moebius_core::board::may_start(app, &card).unwrap();
}

#[test]
fn a_card_blocked_before_any_work_starts_none_until_it_is_released() {
    let s = setup();
    let app = &s.app;
    let card = app.create_issue(s.repository, "Work", "", PERSON).unwrap();
    app.block_card(&Actor::Person, &*s.runs, card.id, None)
        .unwrap();
    s.engine.settle(card.id).unwrap();
    assert_eq!(app.card(card.id).unwrap().column, "Backlog");
    assert!(app.store.runs_for_card(card.id).unwrap().is_empty());

    app.unblock_card(&Actor::Person, card.id, "").unwrap();
    s.engine.settle(card.id).unwrap();
    assert_eq!(app.card(card.id).unwrap().column, "In progress");
    assert_eq!(app.store.runs_for_card(card.id).unwrap().len(), 1);
}

#[test]
fn deleting_a_card_stops_its_run_and_takes_its_records_worktrees_and_branches() {
    let s = setup();
    let app = &s.app;
    let clone = PathBuf::from(&app.store.repository(s.repository).unwrap().unwrap().path);
    let branch_exists =
        |name: &str| git(&clone, &["rev-parse", "--verify", "--quiet", name]).is_ok();
    let doomed = app
        .create_issue(s.repository, "Doomed", "", PERSON)
        .unwrap();
    let kept = app.create_issue(s.repository, "Kept", "", PERSON).unwrap();
    s.engine.settle(doomed.id).unwrap();
    s.engine.settle(kept.id).unwrap();
    let running = app.store.runs_for_card(doomed.id).unwrap().pop().unwrap();
    let kept_run = app.store.runs_for_card(kept.id).unwrap().pop().unwrap();
    let running_worktree = PathBuf::from(running.worktree.clone().unwrap());
    assert!(running_worktree.is_dir());

    // What an earlier round left: a run that ended with its worktree still there, the change's branch, a trial's.
    let old = app
        .store
        .add_run(Some(doomed.id), "Coder", "fake", "code")
        .unwrap();
    let base = app
        .store
        .repository(s.repository)
        .unwrap()
        .unwrap()
        .base_ref();
    let leftover = app
        .repos
        .add_worktree("demo", &clone, old.id, &base)
        .unwrap();
    app.store
        .set_run_worktree(old.id, &leftover.path.to_string_lossy())
        .unwrap();
    app.store.set_run_status(old.id, "succeeded").unwrap();
    for branch in [
        format!("moebius/card-{}", doomed.number),
        format!("moebius/trial-4-card-{}", doomed.id),
        format!("moebius/trial-4-card-{}0", doomed.id),
        "moebius/chat-3".to_string(),
    ] {
        app.repos.point_branch(&clone, &branch, &base).unwrap();
    }
    app.store
        .add_change(
            s.repository,
            Some(doomed.id),
            &format!("moebius/card-{}", doomed.number),
            None,
            "abc",
        )
        .unwrap();
    app.store
        .add_inbox_item("stuck", Some(doomed.id), "Doomed", &serde_json::json!({}))
        .unwrap();
    assert!(branch_exists(&format!("moebius/run-{}", running.id)));
    let mut events = app.bus.subscribe();

    let deleted = app
        .delete_card(&Actor::Person, &*s.runs, doomed.id, "not wanted")
        .unwrap();
    assert_eq!(deleted.id, doomed.id);

    // The running run was stopped and had ended before anything was taken.
    assert_eq!(*s.runs.stopped.lock().unwrap(), [running.id]);
    assert_eq!(app.store.card(doomed.id).unwrap(), None);
    assert_eq!(app.store.runs_for_card(doomed.id).unwrap(), vec![]);
    assert_eq!(app.store.run(running.id).unwrap(), None);
    assert!(app.store.changes(s.repository).unwrap().is_empty());
    assert!(app.store.inbox().unwrap().is_empty());
    assert!(!running_worktree.exists());
    assert!(!leftover.path.exists());
    let worktrees = git(&clone, &["worktree", "list", "--porcelain"]).unwrap();
    assert!(
        !worktrees.contains(&format!("run-{}", old.id)),
        "{worktrees}"
    );
    for branch in [
        format!("moebius/run-{}", running.id),
        format!("moebius/run-{}", old.id),
        format!("moebius/card-{}", doomed.number),
        format!("moebius/trial-4-card-{}", doomed.id),
    ] {
        assert!(!branch_exists(&branch), "{branch}");
    }
    // What belongs to another card, or to nobody, stays: the number 10 of a card is not the number 1.
    for branch in [
        format!("moebius/run-{}", kept_run.id),
        format!("moebius/trial-4-card-{}0", doomed.id),
        "moebius/chat-3".to_string(),
    ] {
        assert!(branch_exists(&branch), "{branch}");
    }
    assert_eq!(app.store.cards(s.repository).unwrap().len(), 1);
    assert!(Path::new(&kept_run.worktree.unwrap()).is_dir());

    // Recorded as a decision that belongs to no card any more, and announced.
    assert_eq!(
        judgement_kinds(app, "delete_card"),
        [("person".to_string(), None)]
    );
    let context = context(app, "delete_card");
    assert_eq!(
        (
            context["title"].as_str(),
            context["number"].as_str(),
            context["repository"].as_str()
        ),
        (Some("Doomed"), Some("1"), Some("demo"))
    );
    assert!(kinds_published(&mut events).contains(&"card_deleted".to_string()));
    // Without a forge nothing is remembered, and the card is gone for good.
    assert!(
        !app.store
            .card_was_deleted(s.repository, doomed.number)
            .unwrap()
    );
    let again = app.delete_card(&Actor::Person, &*s.runs, doomed.id, "");
    assert!(matches!(again, Err(AppError::NotFound(_))));
}
