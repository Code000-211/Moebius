//! The Fleet page: a tile for every working run with its last steps, and how the tiles follow the stream.

use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{self, Snapshot};
use moebius_desktop::screens::chat;
use moebius_desktop::screens::fleet;
use serde_json::json;

use crate::chat::chat_events;
use crate::fixtures::{board, card, clicked, draw, repository, run, shows, snapshot, working};
use crate::live::mockup_events;

/// The chat of the Fleet: running now, in the repository radiance.
fn fleet_chats() -> Vec<client::Chat> {
    serde_json::from_value(json!([
        {"id": 7, "agent": "Moebius", "status": "running", "repository_id": 1, "started_at": 1_790_000_000 - 600,
         "ended_at": null, "title": "Write a ready.luau that starts a card only when nothing blocks it."},
        {"id": 5, "agent": "Moebius", "status": "succeeded", "repository_id": 1, "started_at": 1_790_000_000 - 90_000,
         "ended_at": 1_790_000_000 - 86_000, "title": "Add a rule against vendor edits"}
    ]))
    .unwrap()
}

/// Two card runs (one coding, one waiting for a review) and a chat are working; a card is done.
fn fleet_snapshot() -> Snapshot {
    snapshot(
        vec![board(
            repository(1, "radiance", Some("Code000-211/Radiance")),
            vec![
                working(
                    48,
                    1,
                    "In progress",
                    "MTP drafter: cap the candidate tree depth",
                    json!({"tokens": 412_000, "run": run(9, "Coder", "code", "running")}),
                ),
                working(
                    47,
                    1,
                    "In review",
                    "sched: load adaptive draft profiles",
                    json!({"run": run(11, "Reviewer", "review", "waiting")}),
                ),
                working(
                    45,
                    1,
                    "Done",
                    "place: release staged units on teardown",
                    json!({"run": run(8, "Reviewer", "review", "succeeded")}),
                ),
            ],
        )],
        vec![],
    )
}

fn fleet_events(run: i64) -> fleet::Loaded {
    // The review has taken a few steps and waits; the coder is in the middle of its work.
    let mut events = match run {
        7 => chat_events(),
        11 => mockup_events().into_iter().take(8).collect(),
        _ => mockup_events(),
    };
    if run != 7 {
        for (n, event) in events.iter_mut().enumerate() {
            event.at = 1_790_000_000 - 240 + n as i64 * 15;
        }
    }
    fleet::Loaded {
        started_at: 1_790_000_000 - 360,
        events,
    }
}

/// The app on the Fleet page with its three tiles read, as if from a server.
fn fleet_app() -> App {
    let mut app = App::with_snapshot(fleet_snapshot());
    let _ = app.update(Message::Select(Page::Fleet));
    let _ = app.update(Message::Fleet(fleet::Message::Tick(1_790_000_000)));
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(fleet_chats()))));
    for run in [9, 11, 7] {
        let _ = app.update(Message::Fleet(fleet::Message::Loaded(
            run,
            Ok(fleet_events(run)),
        )));
    }
    app
}

fn run_event(run: i64, seq: i64, record: serde_json::Value) -> Message {
    Message::Stream(client::StreamEvent::Event(client::ServerEvent {
        kind: "run_event".into(),
        data: json!({"run": run, "seq": seq, "kind": record["type"], "data": record, "at": 1_790_000_000}),
    }))
}

/// Every working run is a tile: its agent, what for, the card or chat, the repository, the tokens and the last
/// steps. Compare `tests/snapshots/fleet-tiny-skia.png`.
#[test]
fn the_fleet_shows_a_tile_for_every_working_card_and_chat() {
    let app = fleet_app();
    assert_eq!(app.page(), Page::Fleet);
    draw(
        &app,
        "fleet",
        &[
            "Fleet",
            "3 agents working",
            "Coder",
            "code",
            "#48 MTP drafter: cap the candidate tree depth",
            "radiance · 412k tokens",
            "Reviewer",
            "review",
            "waiting",
            "#47 sched: load adaptive draft profiles",
            "Moebius",
            "chat",
            "Write a ready.luau that starts a card only when nothing blocks it.",
            "Agent is working",
            "Bash",
            "Worked for 1m 15s",
        ],
    );
    // The card that is done is not there.
    assert!(!shows(&app, "#45 place: release staged units on teardown"));
}

#[test]
fn an_empty_fleet_says_nothing_is_working() {
    let idle = board(
        repository(1, "radiance", None),
        vec![card(
            43,
            1,
            "Done",
            "tree: close the dispatch contract findings",
        )],
    );
    let mut app = App::with_snapshot(snapshot(vec![idle], vec![]));
    let _ = app.update(Message::Select(Page::Fleet));
    draw(
        &app,
        "fleet-empty",
        &["Fleet", "Nothing is working.", "Nothing is working."],
    );
}

#[test]
fn a_run_event_appends_a_step_to_the_tile_of_its_run() {
    let mut app = fleet_app();
    assert!(!shows(&app, "cargo test --workspace"));
    let start = json!({"type": "tool_execution_start", "toolCallId": "z1", "toolName": "bash",
                       "args": {"command": "cargo test --workspace"}});
    // An event of a run that has no tile changes nothing; one of the coder's run appends to its tile.
    let _ = app.update(run_event(99, 1, start.clone()));
    assert!(!shows(&app, "cargo test --workspace"));
    let _ = app.update(run_event(9, 100, start));
    assert!(shows(&app, "cargo test --workspace"));
}

#[test]
fn a_run_that_ends_leaves_the_fleet_and_a_chat_that_ends_does_too() {
    let mut app = fleet_app();
    assert!(shows(&app, "3 agents working"));
    // The chat ends: the stream says so.
    let _ = app.update(Message::Stream(client::StreamEvent::Event(
        client::ServerEvent {
            kind: "run_ended".into(),
            data: json!({"run": 7, "status": "succeeded"}),
        },
    )));
    assert!(shows(&app, "2 agents working"));
    assert!(!shows(
        &app,
        "Write a ready.luau that starts a card only when nothing blocks it."
    ));
    // The card's run ends: the snapshot says so.
    let mut done = fleet_snapshot();
    done.boards[0].cards[0].run.as_mut().unwrap().status = "succeeded".into();
    let _ = app.update(Message::Loaded(1, Ok(done)));
    assert!(shows(&app, "1 agent working"));
    assert!(!shows(
        &app,
        "#48 MTP drafter: cap the candidate tree depth"
    ));
    assert!(shows(&app, "#47 sched: load adaptive draft profiles"));
}

#[test]
fn a_click_on_a_tile_opens_the_live_view_of_the_card_or_the_chat() {
    let mut app = fleet_app();
    let opened = clicked(&app, "#48 MTP drafter: cap the candidate tree depth");
    assert!(matches!(opened[..], [Message::OpenCard(48)]));
    for message in opened {
        let _ = app.update(message);
    }
    assert_eq!(app.page(), Page::Board(1));
    assert!(shows(&app, "This run"));

    let mut app = fleet_app();
    let opened = clicked(
        &app,
        "Write a ready.luau that starts a card only when nothing blocks it.",
    );
    assert!(matches!(opened[..], [Message::OpenChat(7)]));
    for message in opened {
        let _ = app.update(message);
    }
    assert_eq!(app.page(), Page::Chat);
}

#[test]
fn the_sidebar_lists_the_fleet_after_chat_with_the_number_working() {
    let app = fleet_app();
    assert!(shows(&app, "Fleet"));
    assert!(shows(&app, "3 live"));
}
