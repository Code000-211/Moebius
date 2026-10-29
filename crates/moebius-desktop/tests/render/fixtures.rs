//! What the render tests share: the snapshots they read as if from a server (repositories, cards, runs, the board
//! of the mockup), the simulator they draw with, `draw` that compares a picture with the stored one, and the clicks
//! that find a button and give the app what it sends.

use iced::{Settings, Size};
use iced_test::Simulator;
use moebius_desktop::app::{App, Message};
use moebius_desktop::client::Snapshot;
use moebius_desktop::screens::server_terminal;
use moebius_desktop::theme;
use serde_json::json;

pub fn repository(id: i64, name: &str, github: Option<&str>) -> serde_json::Value {
    let forge = if github.is_some() { "github" } else { "none" };
    json!({"id": id, "name": name, "path": format!("/srv/repos/{name}"), "forge": forge,
           "default_branch": "main", "github": github, "trusted": []})
}

pub fn card(id: i64, repository: i64, column: &str, title: &str) -> serde_json::Value {
    json!({"id": id, "repository_id": repository, "number": id, "title": title, "body": "",
           "author": "person", "column": column, "created_at": 1790000000})
}

/// A card that has a run: the card's fields are extended with what the snapshot says of its latest run.
pub fn working(
    id: i64,
    repository: i64,
    column: &str,
    title: &str,
    extra: serde_json::Value,
) -> serde_json::Value {
    let mut card = card(id, repository, column, title);
    card.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    card
}

pub fn run(id: i64, agent: &str, purpose: &str, status: &str) -> serde_json::Value {
    json!({"id": id, "agent": agent, "purpose": purpose, "status": status})
}

pub fn board(repository: serde_json::Value, cards: Vec<serde_json::Value>) -> serde_json::Value {
    json!({"repository": repository, "cards": cards, "round_limit": 5, "waiting": {}, "changes": []})
}

pub fn item(id: i64, kind: &str, card: i64) -> serde_json::Value {
    json!({"id": id, "kind": kind, "card_id": card, "title": "An issue", "data": {}, "at": 1})
}

pub fn snapshot(boards: Vec<serde_json::Value>, inbox: Vec<serde_json::Value>) -> Snapshot {
    serde_json::from_value(json!({"boards": boards, "inbox": inbox})).unwrap()
}

/// The board of the mockup: three runs working, one change waiting. Card 50 is in `column_of_50`.
pub fn radiance(column_of_50: &str) -> serde_json::Value {
    board(
        repository(1, "radiance", Some("Code000-211/Radiance")),
        vec![
            working(
                54,
                1,
                "Ready",
                "docs: explain the coupled MTP mode",
                json!({"blocked_by": [48]}),
            ),
            card(50, 1, column_of_50, "Retry model downloads on HTTP 5xx"),
            working(
                48,
                1,
                "In progress",
                "MTP drafter: cap the candidate tree depth",
                json!({
                "rounds": 2, "tokens": 412_000, "refusals": 1, "run": run(9, "Coder", "code", "running")}),
            ),
            working(
                52,
                1,
                "In progress",
                "Keep the HIP header out of the host build",
                json!({
                "rounds": 1, "tokens": 96_000, "run": run(10, "Coder", "code", "running")}),
            ),
            working(
                47,
                1,
                "In review",
                "sched: load adaptive draft profiles",
                json!({
                "rounds": 3, "run": run(11, "Reviewer", "review", "running"),
                "change": {"id": 1, "checks": "pending"}}),
            ),
            working(
                45,
                1,
                "Ready to merge",
                "place: release staged units on teardown",
                json!({
                "rounds": 2, "tokens": 590_000, "run": run(8, "Reviewer", "review", "succeeded"),
                "change": {"id": 2, "checks": "success"}}),
            ),
            card(43, 1, "Done", "tree: close the dispatch contract findings"),
        ],
    )
}

/// Three repositories, six inbox items, three runs working on the first and one on the second.
pub fn overview() -> Snapshot {
    let inbox: Vec<_> = (1..=5).map(|id| item(id, "approve_issue", id)).collect();
    let working_card = |id, repository| {
        working(
            id,
            repository,
            "In progress",
            "Add the shell",
            json!({"run": run(id, "Coder", "code", "running")}),
        )
    };
    snapshot(
        vec![
            board(
                repository(1, "radiance", Some("Code000-211/Radiance")),
                vec![working_card(1, 1), working_card(2, 1), working_card(3, 1)],
            ),
            board(repository(2, "moebius", None), vec![working_card(4, 2)]),
            board(repository(3, "dotfiles", None), vec![]),
        ],
        [inbox, vec![item(6, "ready_to_merge", 1)]].concat(),
    )
}

/// The app on the board of the mockup, with nothing in the inbox.
pub fn radiance_app() -> App {
    App::with_snapshot(snapshot(vec![radiance("Ready")], vec![]))
}

pub fn simulator(app: &App) -> Simulator<'_, Message> {
    simulator_sized(app, Size::new(1440.0, 900.0))
}

pub fn simulator_sized(app: &App, size: Size) -> Simulator<'_, Message> {
    let settings = Settings {
        fonts: theme::fonts(),
        default_font: theme::MANROPE,
        ..Settings::default()
    };
    Simulator::with_size(settings, size, app.view())
}

/// Draws the app at the size of the mockups, asserts that each of `texts` is on the screen, and compares the picture
/// with the stored one (the first run stores it).
pub fn draw(app: &App, name: &str, texts: &[&str]) {
    draw_sized(app, name, texts, Size::new(1440.0, 900.0));
}

/// Like `draw`, on a screen of another size, for pages longer than the mockups' 900 px.
pub fn draw_sized(app: &App, name: &str, texts: &[&str], size: Size) {
    let mut ui = simulator_sized(app, size);
    for text in texts {
        assert!(
            ui.find(*text).is_ok(),
            "{name}: {text:?} is not on the screen"
        );
    }
    let snapshot = ui.snapshot(&app.theme()).unwrap();
    let path = format!("{}/tests/snapshots/{name}.png", env!("CARGO_MANIFEST_DIR"));
    if !snapshot.matches_image(&path).unwrap() {
        // What was drawn is kept in target/snapshot-diffs (CI keeps the directory of a failed run), so that a
        // difference on another machine can be seen.
        let drawn = format!("{}/../../target/snapshot-diffs", env!("CARGO_MANIFEST_DIR"));
        let _ = std::fs::remove_file(format!("{drawn}/{name}-tiny-skia.png"));
        let _ = snapshot.matches_image(format!("{drawn}/{name}.png"));
        panic!("{name} differs from {path}; delete the file to store the new picture");
    }
}

pub fn clicked(app: &App, label: &str) -> Vec<Message> {
    clicked_sized(app, label, Size::new(1440.0, 900.0))
}

/// What a click on `label` sends, on a screen of `size`.
pub fn clicked_sized(app: &App, label: &str, size: Size) -> Vec<Message> {
    let mut ui = simulator_sized(app, size);
    ui.click(label)
        .unwrap_or_else(|_| panic!("{label:?} is not on the screen"));
    ui.into_messages().collect()
}

/// Clicks `label` and gives the app what the click sends.
pub fn click(app: &mut App, label: &str) {
    let messages = clicked(app, label);
    update_all(app, messages);
}

pub fn shows(app: &App, text: &str) -> bool {
    simulator(app).find(text).is_ok()
}

/// Like `clicked`, on a screen tall enough for a long list.
pub fn tall_click(app: &App, label: &str) -> Vec<Message> {
    clicked_sized(app, label, Size::new(1440.0, 1400.0))
}

pub fn settings() -> moebius_desktop::settings::Settings {
    moebius_desktop::settings::Settings {
        address: "http://127.0.0.1:1".into(),
        token: "not-a-real-token".into(),
    }
}

/// What pressing the button `label` sends, or none when the screen has no such button.
pub fn pressed(app: &App, label: &str) -> Option<String> {
    let mut ui = simulator(app);
    ui.click(label).ok()?;
    let sent: Vec<Message> = ui.into_messages().collect();
    Some(format!("{sent:?}"))
}

/// Gives the app each of `messages` in turn.
pub fn update_all(app: &mut App, messages: Vec<Message>) {
    for message in messages {
        let _ = app.update(message);
    }
}

pub fn key(text: &str) -> server_terminal::Message {
    let key = iced::keyboard::Key::Character(text.into());
    server_terminal::Message::Key(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::empty(),
        text: Some(text.into()),
        repeat: false,
    })
}

/// What a click on the element named `id` sends.
pub fn clicked_id(app: &App, id: &str) -> Vec<Message> {
    ids_sized(app, id, Size::new(1440.0, 900.0))
}

/// The messages a click on the widget named `id` sends, on a screen of `size`.
pub fn ids_sized(app: &App, id: &str, size: Size) -> Vec<Message> {
    let mut ui = simulator_sized(app, size);
    ui.click(iced::widget::Id::from(id.to_string()))
        .unwrap_or_else(|_| panic!("no {id} on the screen"));
    ui.into_messages().collect()
}
