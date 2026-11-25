//! Tests of the custom columns of a project's board (ADR 0030) from the engine's side: which script runs for a card in
//! one, where `moebius.move` may send a card, and what a card does when its column goes away.

use crate::app::{App, OWNER_AUTHOR};
use crate::board::testing::setup;
use crate::harness::repo::testing::put;
use crate::store::Card;

const TESTING: &str =
    "[[column]]\nname = \"Ready for testing\"\nscript = \"testing.luau\"\nafter = \"In review\"\n";

/// A card of the repository, put into `column` by a person.
fn card_in(app: &App, repo: i64, column: &str) -> Card {
    let card = app
        .create_issue(repo, "Check it", "", OWNER_AUTHOR)
        .unwrap();
    app.store.move_card(card.id, column).unwrap();
    app.card(card.id).unwrap()
}

#[test]
fn a_custom_columns_script_runs_for_a_card_in_it_and_sees_its_class() {
    let (_dir, app, engine, repo) = setup();
    put(&app, "demo", "kanban.toml", TESTING);
    put(
        &app,
        "demo",
        "testing.luau",
        "function on_card(card)\n  if card.column == 'Ready for testing' and card.column_class == 'In review' then\n    moebius.move('Ready to merge', 'tested')\n  end\nend\n",
    );
    let card = card_in(&app, repo, "Ready for testing");
    assert_eq!(app.column_class(card.project_id, &card.column), "In review");
    let step = engine.tick(card.id).unwrap();
    assert_eq!(step.moved.as_deref(), Some("Ready to merge"));
    assert_eq!(app.card(card.id).unwrap().column, "Ready to merge");
}

#[test]
fn a_script_moves_a_card_to_a_custom_column_in_any_case_and_not_to_an_unknown_one() {
    let (_dir, app, engine, repo) = setup();
    put(&app, "demo", "kanban.toml", TESTING);
    put(
        &app,
        "demo",
        "backlog.luau",
        "function on_card(card) moebius.move('ready for TESTING') end\n",
    );
    let card = app.create_issue(repo, "Move", "", OWNER_AUTHOR).unwrap();
    engine.tick(card.id).unwrap();
    assert_eq!(app.card(card.id).unwrap().column, "Ready for testing");

    put(
        &app,
        "demo",
        "testing.luau",
        "function on_card(card) moebius.move('Nowhere') end\n",
    );
    let error = engine.tick(card.id).unwrap_err().to_string();
    assert!(error.contains("no column \"Nowhere\""), "{error}");
    assert_eq!(app.card(card.id).unwrap().column, "Ready for testing");
}

#[test]
fn the_ready_alias_still_names_not_started() {
    let (_dir, app, engine, repo) = setup();
    put(&app, "demo", "kanban.toml", TESTING);
    put(
        &app,
        "demo",
        "backlog.luau",
        "function on_card(card) moebius.move('Ready') end\n",
    );
    let card = app.create_issue(repo, "Alias", "", OWNER_AUTHOR).unwrap();
    engine.tick(card.id).unwrap();
    assert_eq!(app.card(card.id).unwrap().column, "Not started");
}

#[test]
fn a_column_the_file_lost_runs_the_script_of_its_class_and_says_why() {
    let (_dir, app, engine, repo) = setup();
    put(&app, "demo", "kanban.toml", TESTING);
    put(
        &app,
        "demo",
        "testing.luau",
        "function on_card(card) moebius.wait('own script') end\n",
    );
    put(&app, "demo", "review.luau", "function on_card(card) end\n");
    let card = card_in(&app, repo, "Ready for testing");
    let step = engine.tick(card.id).unwrap();
    assert_eq!(step.waiting.as_deref(), Some("own script"));

    // The file loses the column while the card is in it (a hand edit that did not go through the check).
    let project = app.project(card.project_id).unwrap();
    let path = app
        .harness_repo()
        .unwrap()
        .project_dir(&project.name)
        .join("kanban.toml");
    std::fs::write(path, "# no columns of its own any more\n").unwrap();
    let step = engine.tick(card.id).unwrap();
    let waiting = step.waiting.unwrap();
    assert!(waiting.contains("Ready for testing"), "{waiting}");
    assert!(waiting.contains("counted as In review"), "{waiting}");
    assert_eq!(
        app.waiting(&[app.card(card.id).unwrap()]).get(&card.id),
        Some(&waiting)
    );
    assert_eq!(
        app.column_class(card.project_id, "Ready for testing"),
        "In review"
    );
}

#[test]
fn a_custom_column_without_its_script_file_runs_the_script_of_its_class_with_a_notice() {
    let (_dir, app, engine, repo) = setup();
    put(&app, "demo", "kanban.toml", TESTING);
    put(&app, "demo", "review.luau", "function on_card(card) end\n");
    let card = card_in(&app, repo, "Ready for testing");
    let step = engine.tick(card.id).unwrap();
    let waiting = step.waiting.unwrap();
    assert!(waiting.contains("testing.luau"), "{waiting}");
    assert!(
        waiting.contains("running the In review script"),
        "{waiting}"
    );
    assert_eq!(
        app.waiting(&[app.card(card.id).unwrap()]).get(&card.id),
        Some(&waiting)
    );
}

#[test]
fn a_column_nobody_defined_counts_as_backlog_and_never_fails_the_card() {
    let (_dir, app, engine, repo) = setup();
    put(
        &app,
        "demo",
        "backlog.luau",
        "function on_card(card) moebius.wait('backlog ran: ' .. card.column_class) end\n",
    );
    let card = card_in(&app, repo, "Ready for testing");
    let step = engine.tick(card.id).unwrap();
    let waiting = step.waiting.unwrap();
    assert!(waiting.starts_with("backlog ran: Backlog"), "{waiting}");
}

#[test]
fn a_board_without_a_kanban_file_behaves_as_before() {
    let (_dir, app, engine, repo) = setup();
    let card = app.create_issue(repo, "Plain", "", OWNER_AUTHOR).unwrap();
    assert_eq!(app.column_class(card.project_id, "Done"), "Done");
    assert_eq!(card.column, "Backlog");
    engine.settle(card.id).unwrap();
    assert_ne!(app.card(card.id).unwrap().column, "Backlog");
}

#[test]
fn a_card_leaving_a_custom_ready_to_merge_column_no_longer_waits_in_the_inbox() {
    let (_dir, app, engine, repo) = setup();
    put(
        &app,
        "demo",
        "kanban.toml",
        "[[column]]\nname = \"Sign off\"\nscript = \"signoff.luau\"\ncounts_as = \"Ready to merge\"\nafter = \"Ready to merge\"\n",
    );
    for (script, column) in [
        ("merge.luau", "Ready to merge"),
        ("signoff.luau", "Sign off"),
    ] {
        put(
            &app,
            "demo",
            script,
            "function on_card(card) moebius.move('Backlog') end\n",
        );
        let card = card_in(&app, repo, column);
        app.store
            .add_inbox_item(
                "ready_to_merge",
                Some(card.id),
                "A change",
                &serde_json::json!({}),
            )
            .unwrap();
        assert!(!app.store.inbox_for_card(card.id).unwrap().is_empty());
        engine.tick(card.id).unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "Backlog", "{column}");
        assert!(
            app.store
                .inbox_for_card(card.id)
                .unwrap()
                .iter()
                .all(|item| item.resolved_at.is_some()),
            "{column}"
        );
    }
}
