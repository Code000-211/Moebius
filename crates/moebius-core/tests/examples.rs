//! The examples in `examples/` work: the column script runs as a repository override and the rules parse.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use moebius_core::app::{App, PERSON};
use moebius_core::board::{Engine, Limits, Runs, Workflow};
use moebius_core::repos::git;
use moebius_core::rules;
use moebius_core::store::{Card, Run, Store};
use moebius_plugin_api::Host;

fn example(path: &str) -> String {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(path);
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

/// Starts runs that never end.
struct IdleRuns;

impl Runs for IdleRuns {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
    ) -> Result<Run, String> {
        app.store
            .add_run(Some(card.id), agent, "idle", purpose)
            .map_err(|e| e.to_string())
    }
}

/// A repository whose default branch carries `script` as `.moebius/ready.luau`, and an engine over it.
fn engine_with_ready_script(script: &str) -> (tempfile::TempDir, Arc<App>, Engine, i64) {
    let dir = tempfile::tempdir().unwrap();
    let origin: PathBuf = dir.path().join("origin");
    std::fs::create_dir_all(origin.join(".moebius")).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join(".moebius/ready.luau"), script).unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "scripts"]).unwrap();
    let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
    let host = Arc::new(Host::new(Duration::from_secs(1)));
    let app = Arc::new(App::new(store, host, &dir.path().join("data")));
    let repository = app
        .add_repository("demo", origin.to_str().unwrap())
        .unwrap();
    let limits = Limits {
        run_limit: 3,
        ..Limits::default()
    };
    let engine = Engine::new(app.clone(), Arc::new(IdleRuns), Workflow::default(), limits);
    (dir, app, engine, repository.id)
}

/// A new card, moved to Ready by hand.
fn ready_card(app: &App, repository: i64, title: &str) -> Card {
    let card = app.create_issue(repository, title, "", PERSON).unwrap();
    app.move_card(card.id, "Ready").unwrap()
}

#[test]
fn the_large_cards_script_gives_a_large_card_the_machine_to_itself() {
    let (_dir, app, engine, repository) =
        engine_with_ready_script(&example("scripts/ready-large-cards-alone.luau"));
    let column = |card: &Card| app.card(card.id).unwrap().column;

    // An ordinary card starts while fewer than run_limit (3) runs are going.
    let busy = app.create_issue(repository, "Busy", "", PERSON).unwrap();
    app.store
        .add_run(Some(busy.id), "Coder", "idle", "code")
        .unwrap();
    let ordinary = ready_card(&app, repository, "A small fix");
    let step = engine.tick(ordinary.id).unwrap();
    assert_eq!(column(&ordinary), "In progress");
    assert_eq!(step.moved.as_deref(), Some("In progress"));

    // A large card waits while any run is going, and says why.
    let large = ready_card(&app, repository, "[large] Rewrite the parser");
    let step = engine.tick(large.id).unwrap();
    assert_eq!(column(&large), "Ready");
    assert_eq!(
        step.waiting.as_deref(),
        Some("waiting for a free slot (limit 1)")
    );

    // It starts once nothing runs.
    for run in app.store.active_runs().unwrap() {
        app.store.set_run_status(run.id, "succeeded").unwrap();
    }
    let ended = app.store.active_runs().unwrap();
    assert!(ended.is_empty(), "set_run_status ends the run: {ended:?}");
    engine.tick(large.id).unwrap();
    assert_eq!(column(&large), "In progress");
}

#[test]
fn the_large_cards_script_still_waits_for_blockers() {
    let (_dir, app, engine, repository) =
        engine_with_ready_script(&example("scripts/ready-large-cards-alone.luau"));
    let blocker = app.create_issue(repository, "First", "", PERSON).unwrap();
    let card = ready_card(&app, repository, "Second");
    app.store
        .set_dependencies(card.id, &[blocker.number].into())
        .unwrap();
    let step = engine.tick(card.id).unwrap();
    assert_eq!(
        step.waiting,
        Some(format!("waiting for #{}", blocker.number))
    );
    assert_eq!(app.card(card.id).unwrap().column, "Ready");
}

#[test]
fn the_example_rules_are_three_named_rules() {
    let names: Vec<String> = rules::parse(&example("rules.md"))
        .into_iter()
        .map(|rule| rule.name)
        .collect();
    assert_eq!(
        names,
        ["no-new-dependencies", "tests-with-code", "leave-ci-alone"]
    );
}
