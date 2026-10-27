//! Draws the shell headlessly with a fixed overview (no server) and checks what is on the screen. The pictures are
//! kept in `tests/snapshots/` to be compared with `docs/design/board.png`; a later run must draw the same pixels.

use std::collections::HashMap;

use iced::{Settings, Size};
use iced_test::Simulator;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{ClientError, Overview, Snapshot};
use moebius_desktop::theme;
use serde_json::json;

fn repository(id: i64, name: &str, github: Option<&str>) -> serde_json::Value {
    let forge = if github.is_some() { "github" } else { "none" };
    json!({"id": id, "name": name, "path": format!("/srv/repos/{name}"), "forge": forge,
           "default_branch": "main", "github": github, "trusted": []})
}

fn card(id: i64, repository: i64, column: &str, title: &str) -> serde_json::Value {
    json!({"id": id, "repository_id": repository, "number": id, "title": title, "body": "",
           "author": "person", "column": column, "created_at": 1790000000})
}

/// Three repositories, six inbox items, three runs working on the first and one on the second.
fn overview() -> Overview {
    let inbox: Vec<_> = (1..=6)
        .map(|id| json!({"id": id, "kind": "approve_issue", "card_id": id, "title": "An issue", "data": {}, "at": 1}))
        .collect();
    let snapshot: Snapshot = serde_json::from_value(json!({
        "boards": [
            {"repository": repository(1, "radiance", Some("Code000-211/Radiance")),
             "cards": [card(1, 1, "Ready", "Explain the coupled mode"), card(2, 1, "In progress", "Cap the tree depth")],
             "waiting": {"2": "needs a person"}, "changes": []},
            {"repository": repository(2, "moebius", None), "cards": [card(3, 2, "In progress", "Add the shell")],
             "waiting": {}, "changes": []},
            {"repository": repository(3, "dotfiles", None), "cards": [], "waiting": {}, "changes": []}
        ],
        "inbox": inbox
    }))
    .unwrap();
    Overview {
        snapshot,
        live_runs: HashMap::from([(1, 3), (2, 1), (3, 0)]),
    }
}

/// Draws the app at the size of the mockups, asserts that each of `texts` is on the screen, and compares the picture
/// with the stored one (the first run stores it).
fn draw(app: &App, name: &str, texts: &[&str]) {
    let settings = Settings {
        fonts: theme::fonts(),
        default_font: theme::MANROPE,
        ..Settings::default()
    };
    let mut ui = Simulator::with_size(settings, Size::new(1440.0, 900.0), app.view());
    for text in texts {
        assert!(
            ui.find(*text).is_ok(),
            "{name}: {text:?} is not on the screen"
        );
    }
    let snapshot = ui.snapshot(&app.theme()).unwrap();
    let path = format!("{}/tests/snapshots/{name}.png", env!("CARGO_MANIFEST_DIR"));
    assert!(
        snapshot.matches_image(&path).unwrap(),
        "{name} differs from {path}; delete the file to store the new picture"
    );
}

#[test]
fn the_expanded_sidebar_lists_the_repositories_and_the_board_is_named() {
    let app = App::with_overview(overview());
    draw(
        &app,
        "shell-expanded",
        &[
            "Online",
            "Inbox",
            "6",
            "Boards",
            "radiance",
            "3 live",
            "moebius",
            "1 live",
            "dotfiles",
            "Idle",
            "Build",
            "Agents",
            "Skills",
            "MCP servers",
            "Rules",
            "Improve",
            "Retro",
            "Experiments",
            "Decision model",
            "Usage",
            "Setup",
            "Models",
            "Plugins",
            "Settings",
            "GitHub · Code000-211/Radiance",
            "3 runs working · 1 card waits · workflow from ",
        ],
    );
}

#[test]
fn the_collapsed_sidebar_keeps_the_inbox_count() {
    let mut app = App::with_overview(overview());
    let _ = app.update(Message::ToggleSidebar);
    draw(
        &app,
        "shell-collapsed",
        &["6", "GitHub · Code000-211/Radiance"],
    );
}

#[test]
fn a_screen_that_is_not_built_says_which_milestone_brings_it() {
    let mut app = App::with_overview(overview());
    let _ = app.update(Message::Select(Page::Usage));
    draw(&app, "shell-soon", &["Usage", "This screen comes with M7."]);
}

#[test]
fn an_app_without_repositories_opens_on_the_inbox() {
    let app = App::with_overview(Overview::default());
    assert_eq!(app.page(), Page::Inbox);
}

#[test]
fn the_connect_screen_says_why_the_connection_failed() {
    let (mut app, _) = App::start(None);
    let _ = app.update(Message::Checked(Err(ClientError::Unauthorized)));
    draw(
        &app,
        "connect-token-refused",
        &[
            "Connect to a server",
            "The token is not accepted.",
            "Connect",
        ],
    );
    let address = "http://10.0.0.5:7171".to_string();
    let _ = app.update(Message::Checked(Err(ClientError::Unreachable(address))));
    let mut ui = Simulator::with_size(
        Settings {
            fonts: theme::fonts(),
            ..Settings::default()
        },
        Size::new(1440.0, 900.0),
        app.view(),
    );
    assert!(ui.find("Cannot reach http://10.0.0.5:7171.").is_ok());
}
