//! Draws the shell headlessly with a fixed snapshot (no server) and checks what is on the screen. The pictures are
//! kept in `tests/snapshots/` to be compared with `docs/design/board.png`; a later run must draw the same pixels.

use iced::{Settings, Size};
use iced_test::Simulator;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{ClientError, Snapshot};
use moebius_desktop::live;
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

/// A card that has a run: the card's fields are extended with what the snapshot says of its latest run.
fn working(
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

fn run(id: i64, agent: &str, purpose: &str, status: &str) -> serde_json::Value {
    json!({"id": id, "agent": agent, "purpose": purpose, "status": status})
}

fn board(repository: serde_json::Value, cards: Vec<serde_json::Value>) -> serde_json::Value {
    json!({"repository": repository, "cards": cards, "round_limit": 5, "waiting": {}, "changes": []})
}

fn item(id: i64, kind: &str, card: i64) -> serde_json::Value {
    json!({"id": id, "kind": kind, "card_id": card, "title": "An issue", "data": {}, "at": 1})
}

fn snapshot(boards: Vec<serde_json::Value>, inbox: Vec<serde_json::Value>) -> Snapshot {
    serde_json::from_value(json!({"boards": boards, "inbox": inbox})).unwrap()
}

/// The board of the mockup: three runs working, one change waiting. Card 50 is in `column_of_50`.
fn radiance(column_of_50: &str) -> serde_json::Value {
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
fn overview() -> Snapshot {
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

fn simulator(app: &App) -> Simulator<'_, Message> {
    let settings = Settings {
        fonts: theme::fonts(),
        default_font: theme::MANROPE,
        ..Settings::default()
    };
    Simulator::with_size(settings, Size::new(1440.0, 900.0), app.view())
}

/// Draws the app at the size of the mockups, asserts that each of `texts` is on the screen, and compares the picture
/// with the stored one (the first run stores it).
fn draw(app: &App, name: &str, texts: &[&str]) {
    let mut ui = simulator(app);
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
    let app = App::with_snapshot(overview());
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
            "3 runs working · 1 change waits for you · default workflow",
        ],
    );
}

#[test]
fn the_collapsed_sidebar_keeps_the_inbox_count() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::ToggleSidebar);
    draw(
        &app,
        "shell-collapsed",
        &["6", "GitHub · Code000-211/Radiance"],
    );
}

#[test]
fn a_screen_that_is_not_built_says_which_milestone_brings_it() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Usage));
    draw(&app, "shell-soon", &["Usage", "This screen comes with M7."]);
}

#[test]
fn an_app_without_repositories_opens_on_the_inbox() {
    let app = App::with_snapshot(Snapshot::default());
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

/// The board of the mockup: every column, the cards and their tags.
#[test]
fn the_board_shows_its_columns_and_cards_like_the_mockup() {
    let snapshot = snapshot(vec![radiance("Ready")], vec![item(1, "ready_to_merge", 45)]);
    let app = App::with_snapshot(snapshot);
    draw(
        &app,
        "board",
        &[
            "radiance",
            "3 runs working · 1 change waits for you · default workflow",
            "Ready",
            "ready.luau",
            "In progress",
            "code.luau",
            "In review",
            "review.luau",
            "Ready to merge",
            "merge.luau",
            "Done",
            "done.luau",
            "#54",
            "docs: explain the coupled MTP mode",
            "Blocked by #48",
            "#50",
            "Next",
            "#48 · Coder · round 2 of 5",
            "Live",
            "412k tok",
            "1 rule broken",
            "#52 · Coder · round 1 of 5",
            "96k tok",
            "#47 · Reviewer · round 3 of 5",
            "CI running",
            "#45 · 2 rounds · 590k tok",
            "Green",
            "Waits for you",
            "#43",
        ],
    );
}

/// The states the mockup does not show, and titles that must stay inside their cards.
#[test]
fn every_tag_and_a_long_title_stay_inside_the_cards() {
    let long = "Supercalifragilisticexpialidocious_and_a_path_without_any_break/that/goes/on/and/on/and/on.rs";
    let snapshot = snapshot(
        vec![board(
            repository(1, "radiance", None),
            vec![
                working(1, 1, "Backlog", "An issue from outside", json!({})),
                working(2, 1, "Backlog", long, json!({"blocked_by": [7, 8]})),
                card(6, 1, "Backlog", "Another issue from outside"),
                card(7, 1, "Ready", "Ready to start"),
                working(
                    3,
                    1,
                    "In progress",
                    "Two broken rules",
                    json!({
                    "rounds": 5, "tokens": 1_250_000, "refusals": 2, "run": run(1, "Coder", "code", "failed"),
                    "change": {"id": 1, "checks": "failure"}}),
                ),
                working(
                    4,
                    1,
                    "In progress",
                    "Stuck at the round limit",
                    json!({
                    "rounds": 5, "run": run(2, "Coder", "code", "failed")}),
                ),
                working(
                    5,
                    1,
                    "In review",
                    "Waiting for a decision",
                    json!({"rounds": 2}),
                ),
            ],
        )],
        vec![item(1, "approve_issue", 1), item(2, "stuck", 4)],
    );
    let app = App::with_snapshot(snapshot);
    draw(
        &app,
        "board-states",
        &[
            "Backlog",
            "backlog.luau",
            "Waits for you",
            "Blocked by #7, #8",
            "2 rules broken",
            "1.2M tok",
            "CI failed",
            "Stuck",
            "#3 · Coder · round 5 of 5",
        ],
    );
    let mut ui = simulator(&app);
    let title = ui.find(long).unwrap();
    // The count of the Backlog, the only column of three cards, is at its right edge, inside the column's padding; the
    // title ends at or before it.
    let count = ui.find("3").unwrap();
    assert!(title.bounds().x + title.bounds().width <= count.bounds().x + count.bounds().width);
}

/// Whether the card's line is in the column of the script: it starts a little to the right of the script's name.
fn is_in_column(app: &App, card: &str, script: &str) -> bool {
    let mut ui = simulator(app);
    let card = ui.find(card).unwrap().bounds().x;
    let script = ui.find(script).unwrap().bounds().x;
    (0.0..20.0).contains(&(card - script))
}

/// A card moved by the server is in its new column once the snapshot is read again.
#[test]
fn a_card_that_moved_is_drawn_in_its_new_column() {
    let mut app = App::with_snapshot(snapshot(vec![radiance("Ready")], vec![]));
    assert!(is_in_column(&app, "#50", "ready.luau"));
    assert!(!is_in_column(&app, "#50", "review.luau"));

    let moved = snapshot(vec![radiance("In review")], vec![]);
    let _ = app.update(Message::Loaded(1, Ok(moved)));
    assert!(is_in_column(&app, "#50", "review.luau"));
    assert!(!is_in_column(&app, "#50", "ready.luau"));
    // Cards keep their order by number: #47 stays above #50.
    let mut ui = simulator(&app);
    assert!(
        ui.find("#47 · Reviewer · round 3 of 5").unwrap().bounds().y
            < ui.find("#50").unwrap().bounds().y
    );
}

/// The events of the mockup's run: text, an allowed read, a refused edit, an allowed edit with a diff, a bash that
/// still runs, and a steer.
fn mockup_events() -> Vec<moebius_desktop::client::RunEvent> {
    let delta = |text: &str| {
        json!({"type": "message_update",
               "assistantMessageEvent": {"type": "text_delta", "contentIndex": 0, "delta": text}})
    };
    let start = |id: &str, tool: &str, args: serde_json::Value| json!({"type": "tool_execution_start", "toolCallId": id, "toolName": tool, "args": args});
    let end = |id: &str, tool: &str, ms: Option<u64>| {
        json!({"type": "tool_execution_end", "toolCallId": id, "toolName": tool, "isError": false,
               "durationMs": ms, "result": {"content": [{"type": "text", "text": "ok"}]}})
    };
    let check = |id: &str, tool: &str, decision: &str, reason: &str| json!({"type": "tool_check", "tool": tool, "tool_call_id": id, "decision": decision, "reason": reason});
    let tree = "core/sched/mtp_tree.cpp";
    let old = "    const int max_depth = cfg.draft_len;";
    let new = "    const int max_depth = std::min(cfg.draft_len, kMaxVerifyDepth);\n    // the verify kernel handles trees up to depth 6";
    let output = "[ 92%] Linking CXX executable build/tests/test_tree\n[100%] Built target test_tree\n\
                  Test #7: tree.depth_cap ............   Passed    0.31 sec\nTest #8: tree.winner_walk ..........   running";
    let records = vec![
        delta("Review found that the tree can grow past what the verify kernel handles. "),
        delta("I'll find where the depth is set and cap it."),
        check("c1", "read", "allow", "no rule is broken"),
        start("c1", "read", json!({"path": tree})),
        end("c1", "read", Some(2)),
        check(
            "c2",
            "edit",
            "refuse",
            "no-vendor-edits: Never edit files under vendor/; change upstream code through patches/.",
        ),
        start(
            "c2",
            "edit",
            json!({"path": "vendor/hipcub/block_scan.hpp", "edits": [{"oldText": "a", "newText": "b"}]}),
        ),
        end("c2", "edit", None),
        delta("Right, vendor code is off limits. The cap belongs in our own tree builder."),
        check("c3", "edit", "allow", "no rule is broken"),
        start(
            "c3",
            "edit",
            json!({"path": tree, "edits": [{"oldText": old, "newText": new}]}),
        ),
        end("c3", "edit", Some(600)),
        check("c4", "bash", "allow", "no rule is broken"),
        start(
            "c4",
            "bash",
            json!({"command": "cmake --build build -j && ctest -R tree"}),
        ),
        json!({"type": "tool_execution_update", "toolCallId": "c4", "toolName": "bash",
               "partialResult": {"content": [{"type": "text", "text": output}]}}),
    ];
    let kinds = |record: &serde_json::Value| record["type"].as_str().unwrap().to_string();
    records
        .into_iter()
        .enumerate()
        .map(|(n, record)| moebius_desktop::client::RunEvent {
            seq: n as i64 + 1,
            kind: kinds(&record),
            data: record,
            at: 1_790_000_000,
        })
        .collect()
}

fn judgement(choice: &str, reason: &str, decider: &str) -> moebius_desktop::client::Judgement {
    moebius_desktop::client::Judgement {
        kind: "tool_call".into(),
        choice: choice.into(),
        reason: reason.into(),
        decider: decider.into(),
        run: Some(9),
    }
}

/// The app showing the live view of card 48, as if its run and events were read from a server.
fn watching(status: &str) -> App {
    let mut board = radiance("Ready");
    board["changes"] = json!([{"id": 1, "repository_id": 1, "card_id": 48,
                               "branch": "moebius/card-48", "state": "open"}]);
    let mut app = App::with_snapshot(snapshot(vec![board], vec![]));
    let _ = app.update(Message::OpenCard(48));
    let run = serde_json::from_value(json!({
        "id": 9, "card_id": 48, "agent": "Coder", "harness": "pi-bolt", "purpose": "code",
        "status": status, "started_at": 1_790_000_000 - 360}))
    .unwrap();
    let loaded = live::Loaded {
        run,
        events: mockup_events(),
        judgements: vec![
            judgement("allow", "no rule is broken", "patterns"),
            judgement(
                "refuse",
                "no-vendor-edits: Never edit files under vendor/; change upstream code through patches/.",
                "teacher",
            ),
            judgement("allow", "no rule is broken", "teacher"),
            judgement("allow", "no rule is broken", "patterns"),
        ],
        now: 1_790_000_000,
    };
    let _ = app.update(Message::Live(live::Message::Loaded(Ok(loaded))));
    app
}

/// A click on a card opens the live view of its run, and the breadcrumb leads back to the board.
#[test]
fn a_click_on_a_card_opens_its_live_view_and_the_breadcrumb_the_board() {
    let mut app = App::with_snapshot(snapshot(vec![radiance("Ready")], vec![]));
    let mut ui = simulator(&app);
    ui.click("MTP drafter: cap the candidate tree depth")
        .unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(matches!(messages[..], [Message::OpenCard(48)]));
    drop(app.update(messages.into_iter().next().unwrap()));
    assert!(simulator(&app).find("This run").is_ok());

    let mut app = watching("running");
    let mut ui = simulator(&app);
    ui.click("radiance").unwrap();
    for message in ui.into_messages().collect::<Vec<_>>() {
        let _ = app.update(message);
    }
    assert!(simulator(&app).find("ready.luau").is_ok());
}

/// A run like the mockup's, drawn: compare `tests/snapshots/live-view-tiny-skia.png` with `docs/design/live-view.png`.
#[test]
fn a_running_run_is_drawn_like_the_live_view_of_the_mockup() {
    draw(
        &watching("running"),
        "live-view",
        &[
            "MTP drafter: cap the candidate tree depth",
            "Live · round 2 of 5",
            "Coder on Pi-Bolt · started 6 min ago",
            "1 rule broken",
            "no-vendor-edits",
            "Never edit files under vendor/; change upstream code through patches/.",
            "Blocked",
            "Teacher",
            "Teacher · ran 600 ms",
            "Pattern · ran 2 ms",
            "Running",
            "-1 +2",
            "Stop run",
            "Steer",
            "The run keeps going while you type. Steering is saved as a judgement.",
            "This run",
            "2 of 5",
            "412k tokens",
            "moebius/card-48",
            "Right, vendor code is off limits. The cap belongs in our own tree builder.",
        ],
    );
}

/// A run that ended shows its status, and can no longer be steered or stopped.
#[test]
fn a_finished_run_shows_its_status_and_cannot_be_steered() {
    let app = watching("succeeded");
    let mut ui = simulator(&app);
    assert!(ui.find("Succeeded · round 2 of 5").is_ok());
    assert!(ui.find("Stop run").is_err());
    assert!(
        ui.find("This run is not running, so it cannot be steered.")
            .is_ok()
    );
}

/// The steer button waits for a message, and stopping asks first.
#[test]
fn steering_needs_a_message_and_stopping_a_confirmation() {
    let mut app = watching("running");
    let mut ui = simulator(&app);
    // With an empty field the button does nothing.
    let _ = ui.click("Steer");
    assert_eq!(ui.into_messages().count(), 0);

    let mut ui = simulator(&app);
    ui.click("Steer this run, for example: also add a test for depth 0")
        .unwrap();
    ui.typewrite("also test depth 0");
    let messages: Vec<_> = ui.into_messages().collect();
    for message in messages {
        let _ = app.update(message);
    }
    let mut ui = simulator(&app);
    ui.click("Steer").unwrap();
    assert!(matches!(
        ui.into_messages().collect::<Vec<_>>()[..],
        [Message::Live(live::Message::Steer)]
    ));

    let mut ui = simulator(&app);
    ui.click("Stop run").unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(matches!(
        messages[..],
        [Message::Live(live::Message::AskStop)]
    ));
    for message in messages {
        let _ = app.update(message);
    }
    let mut ui = simulator(&app);
    assert!(ui.find("Stop this run?").is_ok());
    ui.click("Keep going").unwrap();
    assert!(matches!(
        ui.into_messages().collect::<Vec<_>>()[..],
        [Message::Live(live::Message::KeepGoing)]
    ));
}

/// A card without a run shows its history.
#[test]
fn a_card_without_a_run_shows_its_history() {
    let mut app = App::with_snapshot(snapshot(vec![radiance("Ready")], vec![]));
    let _ = app.update(Message::OpenCard(50));
    let entries = serde_json::from_value(json!([
        {"at": 1_790_000_000, "kind": "moved",
         "data": {"from": "Backlog", "to": "Ready", "note": null}},
        {"at": 1_790_000_100, "kind": "decided",
         "data": {"judgement": 1, "kind": "approve_issue", "choice": "approve", "decider": "person",
                  "reason": "known author"}}]))
    .unwrap();
    let _ = app.update(Message::History(50, Ok(entries)));
    let mut ui = simulator(&app);
    for text in [
        "Retry model downloads on HTTP 5xx",
        "Moved from Backlog to Ready",
        "person decided approve: known author",
    ] {
        assert!(ui.find(text).is_ok(), "{text:?} is not on the screen");
    }
}
