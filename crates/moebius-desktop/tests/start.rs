//! The Get started page, drawn headlessly: its steps, the buttons that open the page fixing each, when the app opens on
//! it, and when the sidebar lists it. The picture is kept in `tests/snapshots/`.

use iced::{Settings, Size};
use iced_test::Simulator;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{ClientError, Snapshot};
use moebius_desktop::settings::Settings as Connection;
use moebius_desktop::{start, theme};
use serde_json::json;

fn answer(missing: &[&str]) -> moebius_desktop::client::Start {
    let step = |key: &str, title: &str, ok: &str, bad: &str| {
        let done = !missing.contains(&key);
        json!({"key": key, "title": title, "done": done, "detail": if done { ok } else { bad }})
    };
    serde_json::from_value(json!({
        "done": missing.is_empty(),
        "steps": [
            step("upstream", "Connect a model provider", "3 models are available.",
                 "Could not ask https://api.example.com/v1: 401."),
            step("pi_bolt", "Install Pi-Bolt", "/usr/local/bin/pi-bolt runs.",
                 "Could not run pi-bolt: it was not found. Set the program on the Settings page."),
            step("coder", "Choose the Coder's model", "The Coder uses qwen3.8-flash-next.",
                 "Give the Coder a model on the Agents page."),
            step("repository", "Add a repository", "1 repository is added.", "No repository is added yet."),
        ],
    }))
    .unwrap()
}

fn loaded(missing: &[&str]) -> Message {
    Message::Start(start::Message::Loaded(Ok(answer(missing))))
}

fn simulator(app: &App) -> Simulator<'_, Message> {
    let settings = Settings {
        fonts: theme::fonts(),
        default_font: theme::MANROPE,
        ..Settings::default()
    };
    Simulator::with_size(settings, Size::new(1440.0, 900.0), app.view())
}

fn shows(app: &App, text: &str) -> bool {
    simulator(app).find(text).is_ok()
}

fn clicked(app: &App, label: &str) -> Vec<Message> {
    let mut ui = simulator(app);
    ui.click(label)
        .unwrap_or_else(|_| panic!("{label:?} is not on the screen"));
    ui.into_messages().collect()
}

/// An app connected to a server that has no repository, with the steps `missing` still to do.
fn app(missing: &[&str]) -> App {
    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(loaded(missing));
    app
}

#[test]
fn the_page_lists_the_steps_with_a_button_for_each_that_is_missing() {
    let mut app = app(&["pi_bolt", "repository"]);
    let _ = app.update(Message::Select(Page::Start));
    assert_eq!(app.page(), Page::Start);
    let mut ui = simulator(&app);
    for text in [
        "Get started",
        "A few things have to be set up before the first card can run.",
        "Before the first card",
        "Connect a model provider",
        "3 models are available.",
        "Install Pi-Bolt",
        "Could not run pi-bolt: it was not found. Set the program on the Settings page.",
        "Choose the Coder's model",
        "Add a repository",
        "Done",
        "To do",
        "Open Settings",
        "Refresh",
    ] {
        assert!(ui.find(text).is_ok(), "{text:?} is not on the screen");
    }
    // Only a missing step has a button.
    for text in ["Open Models", "Open Agents"] {
        assert!(ui.find(text).is_err(), "{text:?} is on the screen");
    }
    let snapshot = ui.snapshot(&app.theme()).unwrap();
    let path = format!(
        "{}/tests/snapshots/get-started.png",
        env!("CARGO_MANIFEST_DIR")
    );
    if !snapshot.matches_image(&path).unwrap() {
        let drawn = format!("{}/../../target/snapshot-diffs", env!("CARGO_MANIFEST_DIR"));
        let _ = std::fs::remove_file(format!("{drawn}/get-started-tiny-skia.png"));
        let _ = snapshot.matches_image(format!("{drawn}/get-started.png"));
        panic!("get-started differs from {path}; delete the file to store the new picture");
    }
}

#[test]
fn each_missing_step_opens_the_page_that_fixes_it() {
    let mut app = app(&["upstream", "pi_bolt", "coder", "repository"]);
    let _ = app.update(Message::Select(Page::Start));
    for (label, page) in [
        ("Open Models", Page::Models),
        ("Open Settings", Page::Settings),
        ("Open Agents", Page::Agents),
        ("Add repository", Page::AddRepository),
    ] {
        let messages = clicked(&app, label);
        assert!(
            matches!(messages.as_slice(), [Message::Select(p)] if *p == page),
            "{label}: {messages:?}"
        );
    }
}

#[test]
fn the_refresh_button_reads_the_steps_again() {
    let mut app = app(&["repository"]);
    let _ = app.update(Message::Select(Page::Start));
    let messages = clicked(&app, "Refresh");
    assert!(matches!(
        messages.as_slice(),
        [Message::Start(start::Message::Refresh)]
    ));
    // A failed read says so and keeps the steps.
    let _ = app.update(Message::Start(start::Message::Loaded(Err(
        ClientError::Unreachable("http://127.0.0.1:1".into()),
    ))));
    assert!(shows(&app, "cannot reach http://127.0.0.1:1"));
    assert!(shows(&app, "Install Pi-Bolt"));
}

#[test]
fn the_sidebar_lists_the_page_only_while_a_step_is_missing() {
    let mut app = App::with_snapshot(Snapshot::default());
    // Before the server has said anything, and once everything is done, there is no entry.
    assert!(!shows(&app, "Get started"));
    let _ = app.update(loaded(&["coder"]));
    assert!(shows(&app, "Get started"));
    let _ = app.update(loaded(&[]));
    assert!(!shows(&app, "Get started"));
    let _ = app.update(Message::Select(Page::Start));
    assert!(shows(&app, "Everything is set up. Cards can run."));
}

fn connect(app: &mut App) {
    let settings = Connection {
        address: "http://127.0.0.1:1".into(),
        token: "not-a-real-token".into(),
    };
    let _ = app.update(Message::Checked(Ok((settings, Snapshot::default()))));
}

#[test]
fn the_app_opens_on_get_started_after_connecting_while_a_step_is_missing() {
    let (mut app, _) = App::start(None);
    connect(&mut app);
    assert_eq!(app.page(), Page::Inbox);
    let _ = app.update(loaded(&["upstream"]));
    assert_eq!(app.page(), Page::Start);

    // Done: it opens as it did.
    let (mut app, _) = App::start(None);
    connect(&mut app);
    let _ = app.update(loaded(&[]));
    assert_eq!(app.page(), Page::Inbox);

    // A page the person chose before the answer came stays.
    let (mut app, _) = App::start(None);
    connect(&mut app);
    let _ = app.update(Message::Select(Page::Usage));
    let _ = app.update(loaded(&["upstream"]));
    assert_eq!(app.page(), Page::Usage);

    // Only the answer after connecting opens the page, not a later one.
    let (mut app, _) = App::start(None);
    connect(&mut app);
    let _ = app.update(loaded(&[]));
    let _ = app.update(loaded(&["upstream"]));
    assert_eq!(app.page(), Page::Inbox);
}
