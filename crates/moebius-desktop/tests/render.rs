//! Draws the shell headlessly with a fixed snapshot (no server) and checks what is on the screen. The pictures are
//! kept in `tests/snapshots/` to be compared with `docs/design/board.png`; a later run must draw the same pixels.

use iced::widget::text_editor;
use iced::{Settings, Size};
use iced_test::Simulator;
use moebius_desktop::add_repository;
use moebius_desktop::agents;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::chat;
use moebius_desktop::client::{self, ClientError, Snapshot, TerminalEvent};
use moebius_desktop::client::{Export, KindCounts, ShadowedKind, Shadowing, Takeover};
use moebius_desktop::decision_model;
use moebius_desktop::experiments;
use moebius_desktop::inbox::{self, Action};
use moebius_desktop::live;
use moebius_desktop::new_issue;
use moebius_desktop::retro;
use moebius_desktop::rules;
use moebius_desktop::search;
use moebius_desktop::setup;
use moebius_desktop::terminal::PaneEvent;
use moebius_desktop::theme;
use moebius_desktop::usage;
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
    simulator_sized(app, Size::new(1440.0, 900.0))
}

fn simulator_sized(app: &App, size: Size) -> Simulator<'_, Message> {
    let settings = Settings {
        fonts: theme::fonts(),
        default_font: theme::MANROPE,
        ..Settings::default()
    };
    Simulator::with_size(settings, size, app.view())
}

/// Draws the app at the size of the mockups, asserts that each of `texts` is on the screen, and compares the picture
/// with the stored one (the first run stores it).
fn draw(app: &App, name: &str, texts: &[&str]) {
    draw_sized(app, name, texts, Size::new(1440.0, 900.0));
}

/// Like `draw`, on a screen of another size, for pages longer than the mockups' 900 px.
fn draw_sized(app: &App, name: &str, texts: &[&str], size: Size) {
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
        outcome: None,
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

/// A refusal whose call was replayed in a copy of the worktree says what came of it, small and muted under the rule:
/// compare `tests/snapshots/live-view-replayed-tiny-skia.png`.
#[test]
fn a_replayed_refusal_shows_what_came_of_it() {
    let mut app = watching("running");
    assert!(!shows(&app, "Replayed: exit 0; deleted 3 files"));
    let replayed = moebius_desktop::client::Judgement {
        outcome: Some("replayed: exit 0; deleted 3 files".into()),
        ..judgement(
            "refuse",
            "no-vendor-edits: Never edit files under vendor/; change upstream code through patches/.",
            "teacher",
        )
    };
    let judgements = vec![
        judgement("allow", "no rule is broken", "patterns"),
        replayed,
        judgement("allow", "no rule is broken", "teacher"),
        judgement("allow", "no rule is broken", "patterns"),
    ];
    let _ = app.update(Message::Live(live::Message::Judgements(Ok(judgements))));
    draw(
        &app,
        "live-view-replayed",
        &[
            "Rule broken: ",
            "no-vendor-edits",
            "Replayed: exit 0; deleted 3 files",
            "Blocked",
        ],
    );
}

/// A run with its shell open and a few lines printed in it, drawn: compare `tests/snapshots/live-view-shell-tiny-skia.png`
/// with `docs/design/live-view.png`.
#[test]
fn a_shell_in_the_worktree_is_drawn_under_the_conversation() {
    let mut app = watching("running");
    let (input, _sent) = iced::futures::channel::mpsc::channel(16);
    let printed = "\x1b[1;32mcoder@moebius\x1b[0m:\x1b[1;34m~/worktrees/run-9\x1b[0m$ git status --short\r\n \
        \x1b[33mM\x1b[0m core/sched/mtp_tree.cpp\r\n\x1b[31m??\x1b[0m notes.txt\r\n\
        \x1b[1;32mcoder@moebius\x1b[0m:\x1b[1;34m~/worktrees/run-9\x1b[0m$ ls tests | head -3\r\n\
        test_tree.cpp  test_walk.cpp  \x1b[7mCMakeLists.txt\x1b[0m\r\n\
        \x1b[1;32mcoder@moebius\x1b[0m:\x1b[1;34m~/worktrees/run-9\x1b[0m$ ";
    for message in [
        live::Message::ToggleShell,
        live::Message::Shell(TerminalEvent::Connected(input)),
        live::Message::ShellPane(PaneEvent::Resized(Size::new(796.0, 250.0))),
        live::Message::Shell(TerminalEvent::Output(printed.as_bytes().to_vec())),
    ] {
        let _ = app.update(Message::Live(message));
    }
    draw(
        &app,
        "live-view-shell",
        &[
            "Close shell",
            "Stop run",
            "Shell in the worktree",
            "connected",
        ],
    );
}

/// Keys reach the shell only while its pane has focus, which a click on the pane gives and a click anywhere else takes
/// away; the focused pane has a teal border.
#[test]
fn keys_go_to_the_shell_only_while_its_pane_has_focus() {
    let mut app = watching("running");
    let (input, mut sent) = iced::futures::channel::mpsc::channel(16);
    for message in [
        live::Message::ToggleShell,
        live::Message::Shell(TerminalEvent::Connected(input)),
        live::Message::ShellPane(PaneEvent::Resized(Size::new(796.0, 250.0))),
    ] {
        let _ = app.update(Message::Live(message));
    }
    let _ = sent.try_recv(); // the size the connection starts with
    let _ = sent.try_recv();
    let key = || {
        let key = iced::keyboard::Key::Character("a".into());
        live::Message::Key(iced::keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: iced::keyboard::key::Physical::Unidentified(
                iced::keyboard::key::NativeCode::Unidentified,
            ),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: Some("a".into()),
            repeat: false,
        })
    };

    // A shell is focused when it is opened.
    let _ = app.update(Message::Live(key()));
    assert!(sent.try_recv().is_ok(), "the opened shell takes keys");

    // A click on the page, outside the pane, takes the focus away; the pane tells so.
    let mut ui = simulator(&app);
    let _ = ui.click("Stop run");
    let told: Vec<_> = ui.into_messages().collect();
    assert!(
        told.iter().any(|m| matches!(
            m,
            Message::Live(live::Message::ShellPane(PaneEvent::Focus(false)))
        )),
        "{told:?}"
    );
    let _ = app.update(Message::Live(live::Message::ShellPane(PaneEvent::Focus(
        false,
    ))));
    let _ = app.update(Message::Live(key()));
    assert!(
        sent.try_recv().is_err(),
        "typing on the page is not typed into the shell"
    );

    // A click on the pane gives it back.
    let mut ui = simulator(&app);
    let _ = ui.click("Shell in the worktree");
    let told: Vec<_> = ui.into_messages().collect();
    assert!(
        told.iter().any(|m| matches!(
            m,
            Message::Live(live::Message::ShellPane(PaneEvent::Focus(true)))
        )),
        "{told:?}"
    );
    let _ = app.update(Message::Live(live::Message::ShellPane(PaneEvent::Focus(
        true,
    ))));
    let _ = app.update(Message::Live(key()));
    assert!(sent.try_recv().is_ok());
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

/// The time the inbox tests stand at, Unix seconds.
const NOW: i64 = 1_790_100_000;

fn change(
    id: i64,
    card: Option<i64>,
    branch: &str,
    state: &str,
    checks: &str,
) -> serde_json::Value {
    json!({"id": id, "repository_id": 1, "card_id": card, "branch": branch, "state": state,
           "pull_request": 18, "checks": checks, "review": "approved"})
}

fn entry(
    id: i64,
    kind: &str,
    card: Option<i64>,
    title: &str,
    data: serde_json::Value,
    ago: i64,
) -> serde_json::Value {
    json!({"id": id, "kind": kind, "card_id": card, "title": title, "data": data, "at": NOW - ago})
}

/// One item of each kind: a stuck card, a change to merge, a change from a chat, an outside issue and a pull request
/// that was closed.
fn inbox_snapshot() -> Snapshot {
    let mut radiance = board(
        repository(1, "radiance", Some("Code000-211/Radiance")),
        vec![
            working(
                45,
                1,
                "Ready to merge",
                "place: release staged units on teardown",
                json!({"rounds": 2, "tokens": 590_000, "change": {"id": 2, "checks": "success"}}),
            ),
            working(
                46,
                1,
                "Ready to merge",
                "Retry model downloads on HTTP 5xx",
                json!({"change": {"id": 3, "checks": "success"}}),
            ),
            working(
                55,
                1,
                "Backlog",
                "Support gfx1100 (RDNA3) kernels",
                json!({"author": "lowfreq"}),
            ),
        ],
    );
    radiance["changes"] = json!([
        change(2, Some(45), "moebius/card-45", "open", "success"),
        change(3, Some(46), "moebius/card-46", "closed", "success"),
        change(5, None, "moebius/chat-9", "open", "pending"),
    ]);
    snapshot(
        vec![
            radiance,
            board(
                repository(2, "moebius", None),
                vec![card(
                    12,
                    2,
                    "In progress",
                    "Board: drag cards between columns",
                )],
            ),
        ],
        vec![
            entry(
                1,
                "stuck",
                Some(12),
                "Board: drag cards between columns",
                json!({"reason": "the round limit is reached"}),
                1200,
            ),
            entry(
                2,
                "ready_to_merge",
                Some(45),
                "place: release staged units on teardown",
                json!({"change": 2, "branch": "moebius/card-45"}),
                3600,
            ),
            entry(
                3,
                "ready_to_merge",
                None,
                "Add a retry helper to the downloader",
                json!({"change": 5, "branch": "moebius/chat-9"}),
                7200,
            ),
            entry(
                4,
                "approve_issue",
                Some(55),
                "Support gfx1100 (RDNA3) kernels",
                json!({"reason": "lowfreq is not a trusted author"}),
                10_800,
            ),
            entry(
                5,
                "change_closed",
                Some(46),
                "Retry model downloads on HTTP 5xx",
                json!({"change": 3, "pull_request": 18}),
                172_800,
            ),
        ],
    )
}

fn inbox_app() -> App {
    let mut app = App::with_snapshot(inbox_snapshot());
    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::Inbox(inbox::Message::Tick(NOW)));
    app
}

fn clicked(app: &App, label: &str) -> Vec<Message> {
    let mut ui = simulator(app);
    ui.click(label)
        .unwrap_or_else(|_| panic!("{label:?} is not on the screen"));
    ui.into_messages().collect()
}

fn shows(app: &App, text: &str) -> bool {
    simulator(app).find(text).is_ok()
}

#[test]
fn the_inbox_shows_every_kind_of_item_like_the_mockup() {
    draw_sized(
        &inbox_app(),
        "inbox",
        &[
            "Inbox",
            "5 things need you.",
            "All 5",
            "To merge 2",
            "Approvals 1",
            "Stuck 1",
            "Closed 1",
            "Every repository",
            "radiance 4",
            "moebius 1",
            "Stuck",
            "moebius · #12",
            "Board: drag cards between columns",
            "Stopped: the round limit is reached.",
            "20 min ago",
            "Ready to merge",
            "radiance · #45",
            "Rounds",
            "590k tok",
            "CI green · review approved",
            "moebius/card-45",
            "Merge",
            "Open live view",
            "Reject with a reason",
            "Change from a chat",
            "radiance · chat",
            "Open chat",
            "Outside issue",
            "radiance · #55 · by lowfreq",
            "Approve",
            "Closed without a merge",
            "Pull request #18 was closed without a merge.",
            "Retry",
            "2 d ago",
        ],
        Size::new(1440.0, 1500.0),
    );
}

/// A positive and an untried proposal of the Retro, waiting for a person.
fn proposals_app() -> App {
    let proposals = vec![
        entry(
            1,
            "proposal",
            None,
            "Tell the Coder to run the tests first",
            json!({"proposal": 1, "branch": "proposal/2026-10-28-small-commits", "trials": [
                {"card": 48, "outcome": "positive",
                 "reason": "the run succeeded, checks_failed did not happen again, the checks pass, and it used 41200 tokens against 38900"},
                {"card": 50, "outcome": "positive",
                 "reason": "the run succeeded, run_failed did not happen again, there were no checks, and it used 12000 tokens against 15100"}]}),
            3600,
        ),
        entry(
            2,
            "proposal",
            None,
            "Allow reading the docs directory",
            json!({"proposal": 2, "branch": "proposal/2026-10-28-allow-docs", "trials": [
                {"card": null, "outcome": "skipped",
                 "reason": "the proposal names no cards, so there is no task to try it on"}]}),
            7200,
        ),
    ];
    let mut app = App::with_snapshot(snapshot(vec![], proposals));
    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::Inbox(inbox::Message::Tick(NOW)));
    app
}

/// The inbox with proposals drawn: compare `tests/snapshots/inbox-proposals-tiny-skia.png`.
#[test]
fn the_inbox_offers_a_proposal_of_the_retro_to_merge_or_dismiss() {
    draw_sized(
        &proposals_app(),
        "inbox-proposals",
        &[
            "2 things need you.",
            "Proposals 2",
            "Retro proposal",
            "Retro",
            "Tell the Coder to run the tests first",
            "The Retro proposes this change to the harness configuration.",
            "Card 48",
            "Positive",
            "the run succeeded, checks_failed did not happen again, the checks pass, and it used 41200 tokens against 38900",
            "Allow reading the docs directory",
            "No card",
            "Skipped",
            "Merge",
            "Dismiss",
        ],
        Size::new(1440.0, 900.0),
    );
}

/// Like `clicked`, on a screen tall enough for a long list.
fn tall_click(app: &App, label: &str) -> Vec<Message> {
    let mut ui = simulator_sized(app, Size::new(1440.0, 1400.0));
    ui.click(label)
        .unwrap_or_else(|_| panic!("{label:?} is not on the screen"));
    ui.into_messages().collect()
}

/// The report of the grilling in `grilling_app`.
const REPORT: &str = "# Is a local cache worth it?\n\nThe downloads are rare and large, so a cache would save\nlittle. Measure the repeat rate before building one.";

/// The issues a grilling proposes, and a research report, waiting for a person.
fn grilling_app() -> App {
    let items = vec![
        entry(
            1,
            "grilling_issues",
            None,
            "3 issues from a grilling",
            json!({"grilling": 7, "repository": 1, "issues": [
                {"title": "Add the export button", "body": "A button on the report page that downloads the CSV.",
                 "blocked_by": []},
                {"title": "Write the CSV", "body": "The columns are the report's.", "blocked_by": [0]},
                {"title": "Document the export", "body": "A section in the user guide.", "blocked_by": [0, 1]}]}),
            1800,
        ),
        entry(
            2,
            "grilling_report",
            None,
            "A research report from a grilling",
            json!({"grilling": 8, "repository": 1,
                   "report": REPORT}),
            5400,
        ),
    ];
    let boards = vec![board(repository(1, "radiance", None), vec![])];
    let mut app = App::with_snapshot(snapshot(boards, items));
    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::Inbox(inbox::Message::Tick(NOW)));
    app
}

/// The inbox with the issues of a grilling: compare `tests/snapshots/inbox-grilling-issues-tiny-skia.png`.
#[test]
fn the_inbox_offers_the_issues_of_a_grilling_as_a_list_to_edit() {
    let mut app = grilling_app();
    let _ = app.update(Message::Inbox(inbox::Message::Kind(Some(
        "grilling_issues".into(),
    ))));
    draw_sized(
        &app,
        "inbox-grilling-issues",
        &[
            "2 things need you; 1 shown.",
            "Grilled issues 1",
            "Issues from a grilling",
            "radiance · Grilling",
            "3 issues from a grilling",
            "Issue 1",
            "Issue 3",
            "Blocked by issue 1",
            "Blocked by issue 1, 2",
            "Add the export button",
            "Document the export",
            "Remove",
            "Confirm",
            "Dismiss",
        ],
        Size::new(1440.0, 1100.0),
    );
}

/// The inbox with a report of a grilling: compare `tests/snapshots/inbox-grilling-report-tiny-skia.png`.
#[test]
fn the_inbox_shows_the_report_of_a_grilling_to_read_and_dismiss() {
    let mut app = grilling_app();
    let _ = app.update(Message::Inbox(inbox::Message::Kind(Some(
        "grilling_report".into(),
    ))));
    draw_sized(
        &app,
        "inbox-grilling-report",
        &[
            "Reports 1",
            "Report from a grilling",
            "radiance · Grilling",
            "A research report from a grilling",
            REPORT,
            "Dismiss",
        ],
        Size::new(1440.0, 600.0),
    );
    assert!(!shows(&app, "Confirm"));
    assert!(matches!(
        &clicked(&app, "Dismiss")[..],
        [Message::Inbox(inbox::Message::Act(
            2,
            Action::DismissGrilling { grilling: 8 }
        ))]
    ));
}

#[test]
fn confirming_or_dismissing_a_grilling_names_it_and_removing_an_issue_takes_it_off_the_list() {
    let mut app = grilling_app();
    let _ = app.update(Message::Inbox(inbox::Message::Kind(Some(
        "grilling_issues".into(),
    ))));
    assert!(matches!(
        &tall_click(&app, "Dismiss")[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::DismissGrilling { grilling: 7 }
        ))]
    ));
    // The first Remove is that of the first issue.
    for message in tall_click(&app, "Remove") {
        let _ = app.update(message);
    }
    assert!(!shows(&app, "Add the export button"));
    assert!(shows(&app, "Write the CSV"));
    assert!(shows(&app, "Issue 2"));
    assert!(!shows(&app, "Issue 3"));
    assert!(matches!(
        &tall_click(&app, "Confirm")[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::ConfirmGrilling { grilling: 7 }
        ))]
    ));
    for message in tall_click(&app, "Confirm") {
        let _ = app.update(message);
    }
    assert!(shows(&app, "Making the cards…"));

    // With every issue removed there is nothing to confirm.
    let mut empty = grilling_app();
    let _ = empty.update(Message::Inbox(inbox::Message::Kind(Some(
        "grilling_issues".into(),
    ))));
    for _ in 0..3 {
        let _ = empty.update(Message::Inbox(inbox::Message::Remove(1, 0)));
    }
    assert!(shows(
        &empty,
        "No issue is left. Dismiss the item, or ask the agent again."
    ));
    let mut ui = simulator(&empty);
    let _ = ui.click("Confirm");
    assert_eq!(ui.into_messages().count(), 0);
}

#[test]
fn a_grilling_chat_needs_a_repository_before_it_starts() {
    let mut app = chatting("waiting", false);
    for message in clicked(&app, "New chat") {
        let _ = app.update(message);
    }
    let agents = serde_json::from_value(json!([
        {"name": "Moebius", "model": "claude-fake", "protocol": "anthropic", "built_in": true},
        {"name": "Grilling-partner", "model": "claude-fake", "protocol": "anthropic", "built_in": true}
    ]))
    .unwrap();
    let _ = app.update(Message::Chat(chat::Message::Agents(Ok(agents))));
    let _ = app.update(Message::Chat(chat::Message::AgentChosen(
        "Grilling-partner".into(),
    )));
    let _ = app.update(Message::Chat(chat::Message::FirstEdited(
        "Reports should be exportable.".into(),
    )));
    assert!(shows(
        &app,
        "The Grilling partner questions your idea in a repository. Ending the chat puts the issues it proposes in the Inbox, to confirm."
    ));
    // Without a repository the chat does not start, with one it does.
    assert_eq!(app.update(Message::Chat(chat::Message::Start)).units(), 0);
    let mut ui = simulator(&app);
    let _ = ui.click("Start chat");
    assert_eq!(ui.into_messages().count(), 0);
    let _ = app.update(Message::Chat(chat::Message::RepositoryChosen(Some(1))));
    assert!(matches!(
        &clicked(&app, "Start chat")[..],
        [Message::Chat(chat::Message::Start)]
    ));
    assert_eq!(app.update(Message::Chat(chat::Message::Start)).units(), 1);
}

#[test]
fn merging_or_dismissing_a_proposal_names_it_and_shows_the_request_until_it_is_answered() {
    let mut app = proposals_app();
    // Both items have the buttons; the first of each is the newest item's.
    let mut ui = simulator(&app);
    ui.click("Dismiss").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(matches!(
        &messages[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::DismissProposal { proposal: 1 }
        ))]
    ));
    let mut ui = simulator(&app);
    ui.click("Merge").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(matches!(
        &messages[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::MergeProposal { proposal: 1 }
        ))]
    ));
    for message in messages {
        let _ = app.update(message);
    }
    assert!(shows(&app, "Merging…"));
    let refused = inbox::Message::Done(
        1,
        Err(ClientError::Server {
            status: 409,
            message: "the branch does not merge cleanly".into(),
        }),
    );
    let _ = app.update(Message::Inbox(refused));
    assert!(shows(&app, "the branch does not merge cleanly"));
}

#[test]
fn merging_sends_the_message_and_the_item_shows_the_request_until_it_is_answered() {
    let mut app = inbox_app();
    let messages = clicked(&app, "Merge");
    assert!(matches!(
        &messages[..],
        [Message::Inbox(inbox::Message::Act(
            2,
            Action::Merge { change: 2 }
        ))]
    ));
    for message in messages {
        let _ = app.update(message);
    }
    assert!(shows(&app, "Merging…"));

    let refused = inbox::Message::Done(
        2,
        Err(ClientError::Server {
            status: 409,
            message: "the branch does not merge cleanly".into(),
        }),
    );
    let _ = app.update(Message::Inbox(refused));
    assert!(shows(&app, "the branch does not merge cleanly"));
    assert!(!shows(&app, "Merging…"));
}

#[test]
fn approving_and_retrying_name_the_card() {
    let app = inbox_app();
    assert!(matches!(
        &clicked(&app, "Approve")[..],
        [Message::Inbox(inbox::Message::Act(
            4,
            Action::Approve { card: 55 }
        ))]
    ));
    let mut stuck = inbox_app();
    let _ = stuck.update(Message::Inbox(inbox::Message::Kind(Some("stuck".into()))));
    assert!(matches!(
        &clicked(&stuck, "Retry")[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::Retry { card: 12 }
        ))]
    ));
}

#[test]
fn rejecting_asks_for_a_reason_first() {
    let mut app = inbox_app();
    for message in clicked(&app, "Reject with a reason") {
        let _ = app.update(message);
    }
    assert!(!shows(&app, "Reject with a reason"));
    assert!(shows(&app, "Send back"));
    // Without a reason the button does nothing.
    let mut ui = simulator(&app);
    let _ = ui.click("Send back");
    assert_eq!(ui.into_messages().count(), 0);

    let _ = app.update(Message::Inbox(inbox::Message::Reason(
        2,
        "Handle the empty list".into(),
    )));
    assert!(matches!(
        &clicked(&app, "Send back")[..],
        [Message::Inbox(inbox::Message::Act(
            2,
            Action::Reject { change: 2 }
        ))]
    ));
    for message in clicked(&app, "Cancel") {
        let _ = app.update(message);
    }
    assert!(shows(&app, "Reject with a reason"));
}

#[test]
fn opening_the_live_view_from_the_inbox_goes_to_the_card() {
    let mut app = inbox_app();
    assert!(matches!(
        &clicked(&app, "Open live view")[..],
        [Message::OpenCard(45)]
    ));
    let _ = app.update(Message::OpenCard(45));
    assert_eq!(app.page(), Page::Board(1));
}

#[test]
fn filtering_by_kind_or_repository_hides_the_other_items() {
    let mut app = inbox_app();
    for message in clicked(&app, "To merge 2") {
        let _ = app.update(message);
    }
    assert!(shows(&app, "Merge"));
    assert!(!shows(&app, "Retry"));
    assert!(!shows(&app, "Approve"));
    assert!(shows(&app, "5 things need you; 2 shown."));
    // The sidebar keeps the total.
    assert!(shows(&app, "5"));

    let _ = app.update(Message::Inbox(inbox::Message::Kind(None)));
    for message in clicked(&app, "moebius 1") {
        let _ = app.update(message);
    }
    assert!(shows(&app, "Board: drag cards between columns"));
    assert!(!shows(&app, "Merge"));

    let _ = app.update(Message::Inbox(inbox::Message::Kind(Some(
        "approve_issue".into(),
    ))));
    assert!(shows(&app, "No item matches the filter."));
}

#[test]
fn an_empty_inbox_says_so() {
    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Select(Page::Inbox));
    assert!(shows(&app, "Nothing needs you."));
}

/// The chat of the chat screen's picture: a person asks for a script, the agent answers with Markdown and a code block,
/// writes `.moebius/ready.luau`, checks it, and closes; the chat ended and offered its branch as a change.
fn chat_events() -> Vec<moebius_desktop::client::RunEvent> {
    let delta = |text: &str| {
        json!({"type": "message_update",
               "assistantMessageEvent": {"type": "text_delta", "contentIndex": 0, "delta": text}})
    };
    let script = "if card.blocked_by_count == 0 then\n  moebius.start_run(\"Coder\")\nend\n";
    let output = "ok: .moebius/ready.luau is a valid column script\n";
    let records = vec![
        json!({"type": "said", "message": "Write a ready.luau that starts a card only when nothing blocks it."}),
        delta("A **ready script** runs when a card enters `Ready`. "),
        delta("This one starts the Coder only when no issue blocks the card:\n\n```lua\n"),
        delta("if card.blocked_by_count == 0 then\n  moebius.start_run(\"Coder\")\nend\n```\n\n"),
        delta("I will write it to the repository and check it.\n"),
        json!({"type": "tool_check", "tool": "write", "tool_call_id": "w1", "decision": "allow",
               "reason": "no rule is broken"}),
        json!({"type": "tool_execution_start", "toolCallId": "w1", "toolName": "write",
               "args": {"path": ".moebius/ready.luau", "content": script}}),
        json!({"type": "tool_execution_end", "toolCallId": "w1", "toolName": "write", "isError": false,
               "durationMs": 3, "result": {"content": [{"type": "text", "text": "written"}]}}),
        json!({"type": "tool_check", "tool": "bash", "tool_call_id": "b1", "decision": "allow",
               "reason": "no rule is broken"}),
        json!({"type": "tool_execution_start", "toolCallId": "b1", "toolName": "bash",
               "args": {"command": "moebius check-script .moebius/ready.luau"}}),
        json!({"type": "tool_execution_end", "toolCallId": "b1", "toolName": "bash", "isError": false,
               "durationMs": 140, "result": {"content": [{"type": "text", "text": output}]}}),
        json!({"type": "message_end"}),
        delta("The script passes. End the chat to offer it as a change."),
    ];
    records
        .into_iter()
        .enumerate()
        .map(|(n, record)| moebius_desktop::client::RunEvent {
            seq: n as i64 + 1,
            kind: record["type"].as_str().unwrap().to_string(),
            data: record,
            at: 1_790_000_000,
        })
        .collect()
}

fn chat_listing(status: &str) -> Vec<moebius_desktop::client::Chat> {
    serde_json::from_value(json!([
        {"id": 7, "agent": "Moebius", "status": status, "repository_id": 1, "started_at": 1_790_000_000 - 600,
         "ended_at": null, "title": "Write a ready.luau that starts a card only when nothing blocks it."},
        {"id": 6, "agent": "Coder", "status": "waiting", "repository_id": null, "started_at": 1_790_000_000 - 7200,
         "ended_at": null, "title": "Explain how the router chooses an upstream."},
        {"id": 5, "agent": "Moebius", "status": "succeeded", "repository_id": 1, "started_at": 1_790_000_000 - 90_000,
         "ended_at": 1_790_000_000 - 86_000, "title": "Add a rule against vendor edits"}
    ]))
    .unwrap()
}

/// The app on the chat screen with chat 7 open, as if its list and events were read from a server. Chat 7 offered
/// the branch `moebius/chat-7` when `offered`.
fn chatting(status: &str, offered: bool) -> App {
    let mut board = radiance("Ready");
    let mut inbox = vec![];
    if offered {
        board["changes"] = json!([{"id": 3, "repository_id": 1, "card_id": null,
                                   "branch": "moebius/chat-7", "state": "open"}]);
        inbox.push(json!({"id": 9, "kind": "ready_to_merge", "card_id": null,
                          "title": "Write a ready.luau that starts a card only when nothing blocks it.",
                          "data": {"change": 3, "branch": "moebius/chat-7"}, "at": 1_790_000_000}));
    }
    let mut app = App::with_snapshot(snapshot(vec![board], inbox));
    let _ = app.update(Message::Select(Page::Chat));
    let _ = app.update(Message::Chat(chat::Message::Tick(1_790_000_000)));
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(chat_listing(
        status,
    )))));
    let _ = app.update(Message::Chat(chat::Message::Select(7)));
    let _ = app.update(Message::Chat(chat::Message::Loaded(7, Ok(chat_events()))));
    app
}

/// A chat drawn: compare `tests/snapshots/chat-tiny-skia.png` with `docs/design/live-view.png`, whose conversation it
/// follows.
#[test]
fn a_chat_is_drawn_with_its_markdown_tool_calls_and_offered_change() {
    draw_sized(
        &chatting("succeeded", true),
        "chat",
        &[
            "Chat",
            "1 chat waits for your next message.",
            "Write a ready.luau that starts a card only when nothing blocks it.",
            "Moebius in radiance",
            "Ended",
            "Explain how the router chooses an upstream.",
            "Coder",
            "Add a rule against vendor edits",
            "New chat",
            "Write a ready.luau that starts a card only when nothing blocks it.",
            ".moebius/ready.luau",
            "Allowed",
            "moebius check-script .moebius/ready.luau",
            "ran 140 ms",
            "ok: .moebius/ready.luau is a valid column script",
            "Change offered",
            "moebius/chat-7",
            "Merge",
            "This chat has ended. What you send starts a new chat with the same agent, which is told this one.",
            "Send",
        ],
        Size::new(1440.0, 1100.0),
    );
}

/// An ended chat is written to like any other, and Send goes on in a new chat; a failed chat's tag is red and keeps its
/// size beside a long title.
#[test]
fn an_ended_chat_is_written_to_and_its_failed_tag_keeps_its_place() {
    let mut app = chatting("failed", false);
    let mut ui = simulator(&app);
    ui.click("Write to the agent").unwrap();
    ui.typewrite("Go on");
    let typed: Vec<Message> = ui.into_messages().collect();
    assert!(
        typed
            .iter()
            .any(|m| matches!(m, Message::Chat(chat::Message::Edited(_)))),
        "the field takes no text: {typed:?}"
    );
    let _ = app.update(Message::Chat(chat::Message::Edited("Go on".into())));
    draw(
        &app,
        "chat-failed",
        &[
            "Write a ready.luau that starts a card only when nothing blocks it.",
            "Failed",
            "Go on",
        ],
    );
    assert!(matches!(
        &clicked(&app, "Send")[..],
        [Message::Chat(chat::Message::Send)]
    ));
}

/// The judgements of a chat's tool calls name who decided each check, matched to the calls the way the live view does.
#[test]
fn a_chat_names_who_decided_its_tool_calls() {
    let mut app = chatting("succeeded", false);
    assert!(!shows(&app, "Pattern · ran 3 ms"));
    let judged = |decider: &str| moebius_desktop::client::Judgement {
        run: Some(7),
        ..judgement("allow", "no rule is broken", decider)
    };
    let _ = app.update(Message::Chat(chat::Message::Judged(
        7,
        Ok(vec![judged("patterns"), judged("teacher")]),
    )));
    assert!(shows(&app, "Pattern · ran 3 ms"));
    assert!(shows(&app, "Teacher · ran 140 ms"));
}

/// The new chat form offers the agents the server lists, the Moebius agent first and chosen.
#[test]
fn a_new_chat_offers_the_servers_agents_with_the_moebius_agent_chosen() {
    let mut app = chatting("waiting", false);
    for message in clicked(&app, "New chat") {
        let _ = app.update(message);
    }
    let agents = serde_json::from_value(json!([
        {"name": "Moebius", "model": "claude-fake", "protocol": "anthropic", "built_in": true},
        {"name": "Coder", "model": "qwen-fake", "protocol": "openai", "built_in": false}
    ]))
    .unwrap();
    let _ = app.update(Message::Chat(chat::Message::Agents(Ok(agents))));
    draw(
        &app,
        "chat-new",
        &["Moebius · claude-fake", "Coder · qwen-fake", "Start chat"],
    );
    assert!(matches!(
        &clicked(&app, "Coder · qwen-fake")[..],
        [Message::Chat(chat::Message::AgentChosen(agent))] if agent == "Coder"
    ));
}

#[test]
fn sending_needs_a_message_and_goes_to_the_open_chat() {
    let mut app = chatting("waiting", false);
    assert_eq!(app.update(Message::Chat(chat::Message::Send)).units(), 0);
    let mut ui = simulator(&app);
    let _ = ui.click("Send");
    assert_eq!(ui.into_messages().count(), 0);
    assert!(shows(&app, "The agent answers at once."));

    let mut ui = simulator(&app);
    ui.click("Write to the agent").unwrap();
    ui.typewrite("Now the budgets.");
    for message in ui.into_messages().collect::<Vec<_>>() {
        let _ = app.update(message);
    }
    assert!(matches!(
        &clicked(&app, "Send")[..],
        [Message::Chat(chat::Message::Send)]
    ));
    assert_eq!(app.update(Message::Chat(chat::Message::Send)).units(), 1);

    // While the agent works the message is queued, and the chat can be stopped after a question.
    let working = chatting("running", false);
    assert!(shows(
        &working,
        "The agent is working. Your message is queued and sent when it is done with this turn."
    ));
    let mut app = working;
    assert!(matches!(
        &clicked(&app, "End chat")[..],
        [Message::Chat(chat::Message::End)]
    ));
    for message in clicked(&app, "Stop") {
        let _ = app.update(message);
    }
    assert!(shows(&app, "Stop this chat?"));
    assert!(matches!(
        &clicked(&app, "Stop chat")[..],
        [Message::Chat(chat::Message::Stop)]
    ));
}

#[test]
fn a_new_chat_needs_a_first_message() {
    let mut app = chatting("waiting", false);
    for message in clicked(&app, "New chat") {
        let _ = app.update(message);
    }
    assert!(shows(&app, "First message"));
    assert!(shows(&app, "No repository"));
    assert!(shows(&app, "radiance"));
    assert_eq!(app.update(Message::Chat(chat::Message::Start)).units(), 0);

    let _ = app.update(Message::Chat(chat::Message::RepositoryChosen(Some(1))));
    let _ = app.update(Message::Chat(chat::Message::FirstEdited(
        "Explain the board".into(),
    )));
    assert_eq!(app.update(Message::Chat(chat::Message::Start)).units(), 1);
    assert!(matches!(
        &clicked(&app, "Start chat")[..],
        [Message::Chat(chat::Message::Start)]
    ));
    for message in clicked(&app, "Cancel") {
        let _ = app.update(message);
    }
    assert!(!shows(&app, "First message"));
}

#[test]
fn opening_the_chat_from_the_inbox_goes_to_the_chat_screen() {
    let mut app = inbox_app();
    assert!(matches!(
        &clicked(&app, "Open chat")[..],
        [Message::OpenChat(9)]
    ));
    let _ = app.update(Message::OpenChat(9));
    assert_eq!(app.page(), Page::Chat);
    assert!(shows(&app, "Reading the chat…"));
}

fn agent_listing() -> Vec<moebius_desktop::client::Agent> {
    serde_json::from_value(json!([
        {"name": "Moebius", "model": "claude-fake", "protocol": "anthropic", "built_in": true},
        {"name": "Coder", "model": "qwen-fake", "protocol": "openai", "built_in": false},
        {"name": "Typo", "model": "", "protocol": "openai", "built_in": false,
         "error": "agents/Typo.toml: unknown field `modle`"}
    ]))
    .unwrap()
}

fn skill_listing() -> Vec<moebius_desktop::client::Skill> {
    serde_json::from_value(json!([
        {"name": "moebius", "description": "Build on Moebius.", "built_in": true},
        {"name": "radiance-kernels", "description": "How the kernels are written.", "built_in": false},
        {"name": "house-style", "description": "Write plainly.", "built_in": false}
    ]))
    .unwrap()
}

fn mcp_listing() -> Vec<moebius_desktop::client::McpServer> {
    serde_json::from_value(json!([
        {"name": "docs-search", "kind": "remote"},
        {"name": "profiler", "kind": "stdio"},
        {"name": "leaky", "kind": "unknown", "error": "${OPENAI_API_KEY} is not in mcp_env"}
    ]))
    .unwrap()
}

fn coder() -> moebius_desktop::client::AgentDetail {
    serde_json::from_value(json!({
        "name": "Coder",
        "settings": {"model": "qwen-fake", "protocol": "openai", "thinking": "high",
                     "token_budget": 1_500_000, "skills": ["radiance-kernels"],
                     "extensions": ["extensions/log.ts"], "mcp": ["docs-search"]},
        "instructions": "# Coder\nYou work on one card at a time, in its own worktree.\n\n- Run the tests before you say you are done.\n"
    }))
    .unwrap()
}

/// The Agents page of a server with a harness repository: three agents and the Coder open in the editor.
fn agents_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Agents));
    let agents = |message| Message::Agents(message);
    let _ = app.update(agents(agents::Message::Listed(Ok(agent_listing()))));
    let _ = app.update(agents(agents::Message::SkillsListed(Ok(skill_listing()))));
    let _ = app.update(agents(agents::Message::McpListed(Ok(mcp_listing()))));
    let _ = app.update(agents(agents::Message::Select("Coder".into())));
    let _ = app.update(agents(agents::Message::Opened("Coder".into(), Ok(coder()))));
    app
}

/// The agents screen drawn: compare `tests/snapshots/agents-tiny-skia.png` with `docs/design/agents.png`.
#[test]
fn the_agents_screen_lists_the_agents_and_edits_one_like_the_mockup() {
    draw_sized(
        &agents_app(),
        "agents",
        &[
            "Agents",
            "New agent",
            "Moebius",
            "Built in",
            "claude-fake · anthropic",
            "Coder",
            "qwen-fake · openai",
            "Typo",
            "Broken",
            "agents/Typo.toml: unknown field `modle`",
            "Save",
            "Delete",
            "Model",
            "Protocol",
            "Thinking",
            "Token budget per card",
            "Instructions",
            "Skills",
            "1 of 2",
            "radiance-kernels",
            "How the kernels are written.",
            "house-style",
            "MCP servers",
            "docs-search",
            "remote",
            "leaky",
            "${OPENAI_API_KEY} is not in mcp_env",
            "Extensions",
            "extensions/log.ts",
            "Add extension",
            "From the Retro",
            "No proposal waiting for a decision changes this agent.",
        ],
        Size::new(1440.0, 1300.0),
    );
}

#[test]
fn a_server_without_a_harness_repository_edits_its_agents_in_moebius_toml() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Agents));
    let _ = app.update(Message::Agents(agents::Message::Listed(
        Ok(agent_listing()),
    )));
    let _ = app.update(Message::Agents(agents::Message::SkillsListed(Ok(vec![]))));
    assert!(shows(&app, "Agents are set in moebius.toml"));
    assert!(shows(
        &app,
        "What each run works with: its model, instructions, skills and extensions. An edit is saved in moebius.toml."
    ));
    assert_eq!(
        format!("{:?}", clicked(&app, "Coder")),
        "[Agents(Select(\"Coder\"))]"
    );
    assert_eq!(
        format!("{:?}", clicked(&app, "New agent")),
        "[Agents(NewAgent)]"
    );
    // The agent is read from the settings, and has skill paths instead of the repository's skills and MCP servers.
    let server = settings_answer(
        json!({"agents": {"Coder": {"model": "local", "instructions": "Be brief.", "skills": ["skills/house"],
                                    "extensions": ["extensions/log.ts"], "token_budget": 5000}}}),
        json!({}),
    );
    let _ = app.update(Message::Agents(agents::Message::Select("Coder".into())));
    let _ = app.update(Message::Agents(agents::Message::OpenedFromFile(
        "Coder".into(),
        Ok(serde_json::from_value(server).unwrap()),
    )));
    for text in [
        "Directories, from moebius.toml",
        "skills/house",
        "extensions/log.ts",
        "Add skill",
    ] {
        assert!(shows(&app, text), "{text}");
    }
    assert!(!shows(&app, "No server in mcp.toml of the repository."));
}

#[test]
fn skills_and_mcp_servers_have_pages_of_their_own() {
    let mut app = agents_app();
    let _ = app.update(Message::Select(Page::Skills));
    assert!(shows(&app, "Write plainly."));
    assert!(shows(&app, "Build on Moebius."));
    let _ = app.update(Message::Select(Page::McpServers));
    assert!(shows(&app, "profiler"));
    assert!(shows(&app, "stdio"));
}

fn settings() -> moebius_desktop::settings::Settings {
    moebius_desktop::settings::Settings {
        address: "http://127.0.0.1:1".into(),
        token: "not-a-real-token".into(),
    }
}

#[test]
fn saving_sends_the_edited_settings_and_a_wrong_field_is_said_instead() {
    let settings = settings();
    let mut agents = agents::Agents::default();
    let mut update = |message| agents.update(message, Some(&settings)).units();
    update(agents::Message::Opened("Coder".into(), Ok(coder())));
    // Nothing is open before an agent is chosen.
    assert_eq!(update(agents::Message::Select("Coder".into())), 1);
    update(agents::Message::Opened("Coder".into(), Ok(coder())));
    update(agents::Message::ModelEdited("better-model".into()));
    update(agents::Message::ProtocolChosen("anthropic".into()));
    update(agents::Message::ThinkingChosen("Not set".into()));
    update(agents::Message::BudgetEdited("2000".into()));
    update(agents::Message::ToggleSkill("house-style".into()));
    update(agents::Message::ToggleSkill("radiance-kernels".into()));
    update(agents::Message::ToggleMcp("docs-search".into()));
    update(agents::Message::ToggleMcp("profiler".into()));
    update(agents::Message::ExtensionEdited(
        " extensions/two.ts ".into(),
    ));
    update(agents::Message::AddExtension);
    update(agents::Message::RemoveExtension("extensions/log.ts".into()));
    assert_eq!(update(agents::Message::Save), 1);
    let (name, sent, instructions) = agents.save_request().unwrap();
    assert_eq!(name, "Coder");
    assert_eq!(
        serde_json::to_value(&sent).unwrap(),
        json!({"model": "better-model", "protocol": "anthropic", "token_budget": 2000,
               "skills": ["house-style"], "extensions": ["extensions/two.ts"], "mcp": ["profiler"]})
    );
    assert_eq!(
        instructions,
        "# Coder\nYou work on one card at a time, in its own worktree.\n\n- Run the tests before you say you are done."
    );

    let mut update = |message| agents.update(message, Some(&settings)).units();
    update(agents::Message::BudgetEdited("lots".into()));
    assert_eq!(update(agents::Message::Save), 0);
    assert!(agents.save_request().is_err());
}

#[test]
fn the_save_button_sends_the_save_message_and_the_answer_is_shown() {
    let mut app = agents_app();
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Agents(agents::Message::Save)]
    ));
    let mut saved = coder();
    saved.commit = Some("a1b2c3d".into());
    let _ = app.update(Message::Agents(agents::Message::Saved(Ok(saved))));
    assert!(shows(&app, "Saved as a commit: a1b2c3d"));
    let invalid = ClientError::Server {
        status: 422,
        message: "the skill \"nope\" is not in skills/".into(),
    };
    let _ = app.update(Message::Agents(agents::Message::Saved(Err(invalid))));
    assert!(shows(&app, "the skill \"nope\" is not in skills/"));
    assert!(!shows(&app, "Saved as a commit: a1b2c3d"));
}

#[test]
fn a_new_agent_asks_for_a_name_and_deleting_asks_first() {
    let mut app = agents_app();
    for message in clicked(&app, "New agent") {
        assert!(matches!(
            message,
            Message::Agents(agents::Message::NewAgent)
        ));
        let _ = app.update(message);
    }
    assert!(shows(&app, "Name of the new agent"));
    assert!(!shows(&app, "Delete"));
    let _ = app.update(Message::Agents(agents::Message::NameEdited(
        "Strong".into(),
    )));
    let _ = app.update(Message::Agents(agents::Message::ModelEdited("big".into())));
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Agents(agents::Message::Save)]
    ));

    let mut app = agents_app();
    assert!(matches!(
        &clicked(&app, "Delete")[..],
        [Message::Agents(agents::Message::AskDelete)]
    ));
    let _ = app.update(Message::Agents(agents::Message::AskDelete));
    assert!(shows(&app, "Delete Coder?"));
    assert!(matches!(
        &clicked(&app, "Delete agent")[..],
        [Message::Agents(agents::Message::Delete)]
    ));
    let _ = app.update(Message::Agents(agents::Message::KeepAgent));
    assert!(!shows(&app, "Delete Coder?"));
}

/// The day numbers of 1970-01-01 onwards as `YYYY-MM-DD` (Howard Hinnant's algorithm).
fn date_of(day: i64) -> String {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let of_era = z - era * 146_097;
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * of_year + 2) / 153;
    let day_of_month = of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day_of_month:02}")
}

/// The usage of the mockup: 135 days of tokens led by three models, four models today, five agents, four tools.
/// 2026-10-28 is the 20,389th day since 1970-01-01.
fn usage_report(day_budget: Option<i64>) -> client::UsageReport {
    const TODAY: i64 = 20_754;
    let leaders = ["Sonnet 5.5", "Opus 5.5", "Qwen3.8 Flash Next · local"];
    let days: Vec<_> = (0..135)
        .map(|i| {
            let day = TODAY - 134 + i;
            // Some days have no usage; the others get a size and a leading model from their number.
            let tokens = if i % 9 == 4 {
                0
            } else if day == TODAY {
                1_840_000
            } else {
                (80_000 + (i * 7_919 + i * i * 131) % 2_400_000).min(2_520_000)
            };
            let model = match i {
                0..3 => leaders[i as usize],
                _ => leaders[((i * 5 + i / 4) % 7 % 3) as usize],
            };
            json!({"date": date_of(day), "tokens": tokens, "model": model})
        })
        .filter(|day| day["tokens"] != 0)
        .collect();
    let model = |name: &str, tokens: i64, calls: i64, input: i64, cached: i64| json!({"model": name, "tokens": tokens, "calls": calls, "input_tokens": input, "cached_tokens": cached});
    let agent = |name: &str, calls: i64, tokens: i64, input: i64, cached: i64, ms: i64| {
        json!({"agent": name, "calls": calls, "tokens": tokens, "input_tokens": input,
               "cached_tokens": cached, "model_ms": ms})
    };
    serde_json::from_value(json!({
        "today": date_of(TODAY),
        "days_in_window": 1,
        "totals": {"tokens": 1_840_000, "calls": 1284, "cached_share": 0.61,
                   "model_ms": 11_520_000, "day_budget": day_budget},
        "days": days,
        "models": [
            model("Sonnet 5.5", 960_000, 690, 880_000, 563_000),
            model("Qwen3.8 Flash Next · local", 500_000, 246, 450_000, 247_500),
            model("Opus 5.5", 225_000, 142, 200_000, 132_000),
            model("Haiku 5.5", 155_000, 206, 150_000, 72_000),
        ],
        "agents": [
            agent("Coder", 702, 1_120_000, 1_000_000, 640_000, 7_080_000),
            agent("Autoresearcher", 188, 290_000, 260_000, 135_000, 2_280_000),
            agent("Reviewer", 214, 260_000, 230_000, 133_000, 1_320_000),
            agent("Grilling partner", 60, 50_000, 45_000, 20_000, 480_000),
            agent("No run", 120, 120_000, 100_000, 71_000, 360_000),
        ],
        "tool_waits": [
            {"tool": "ctest (radiance)", "calls": 31, "seconds": 2460},
            {"tool": "cmake --build", "calls": 22, "seconds": 1620},
            {"tool": "bench (gpu-bench)", "calls": 7, "seconds": 660},
            {"tool": "cargo test (moebius)", "calls": 12, "seconds": 300},
        ],
    }))
    .unwrap()
}

fn usage_app(day_budget: Option<i64>) -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Usage));
    let _ = app.update(Message::Usage(usage::Message::Loaded(Ok(usage_report(
        day_budget,
    )))));
    app
}

/// The usage screen drawn: compare `tests/snapshots/usage-tiny-skia.png` with `docs/design/usage.png`.
#[test]
fn the_usage_screen_shows_the_heatmap_models_and_agents_like_the_mockup() {
    draw_sized(
        &usage_app(Some(8_000_000)),
        "usage",
        &[
            "Usage",
            "Today",
            "7 days",
            "30 days",
            "Tokens",
            "1.84M",
            "23% of the 8M daily budget",
            "Model calls",
            "1,284",
            "1,433 tokens per call on average",
            "From cache",
            "61%",
            "Model time",
            "3 h 12 m",
            "Tokens per day",
            "Lighter means fewer tokens, darker means more",
            "Tokens by model",
            "1.84M today across 4 models",
            "Tokens per call",
            "Sonnet 5.5",
            "960k",
            "52%",
            "Where the model time goes",
            "Longest tool waits",
            "ctest (radiance)",
            "41 m",
            "By agent",
            "Autoresearcher",
            "Tokens today",
            "1.84M / 8M",
        ],
        Size::new(1440.0, 1500.0),
    );
}

#[test]
fn the_sidebar_meter_is_the_tokens_alone_without_a_budget_and_absent_before_the_usage_is_read() {
    let app = usage_app(None);
    assert!(shows(&app, "Tokens today"));
    assert!(shows(&app, "1.84M"));
    assert!(!shows(&app, "1.84M / 8M"));
    let unread = App::with_snapshot(overview());
    assert!(!shows(&unread, "Tokens today"));
}

#[test]
fn choosing_a_window_asks_for_it_and_an_answer_for_another_window_is_ignored() {
    let mut app = usage_app(Some(8_000_000));
    assert!(matches!(
        clicked(&app, "7 days").as_slice(),
        [Message::Usage(usage::Message::Choose(usage::Window::Week))]
    ));
    let _ = app.update(Message::Usage(usage::Message::Choose(usage::Window::Week)));
    // The answer to the window that was left does not replace what is shown.
    let _ = app.update(Message::Usage(usage::Message::Loaded(Ok(usage_report(
        None,
    )))));
    assert!(shows(&app, "1.84M / 8M"));
    let mut week = usage_report(None);
    week.days_in_window = 7;
    week.totals.tokens = 7_000_000;
    let _ = app.update(Message::Usage(usage::Message::Loaded(Ok(week))));
    assert!(shows(&app, "7M"));
    assert!(shows(&app, "1M tokens a day on average"));
    assert!(shows(&app, "1.84M in the last 7 days across 4 models"));
}

fn retro_listing() -> Vec<client::RetroSummary> {
    serde_json::from_value(json!([
        {"date": "2026-10-28", "status": "proposed", "run_id": 31, "errors": 6, "proposals": 3, "at": 1790000000},
        {"date": "2026-10-27", "status": "reported", "run_id": null, "errors": 0, "proposals": 0, "at": 1789900000},
        {"date": "2026-10-26", "status": "failed", "run_id": 20, "errors": 3, "proposals": 0, "at": 1789800000},
    ]))
    .unwrap()
}

fn proposal_json() -> serde_json::Value {
    json!([
        {"id": 1, "branch": "proposal/2026-10-28-small-commits", "title": "Tell the Coder to run the tests first",
         "body": "The Coder's changes failed the checks twice on 2026-10-28; its instructions now say to run the tests before it stops.",
         "agents": ["Coder"], "cards": [48, 50], "status": "positive", "trials": [
            {"card_id": 48, "outcome": "positive",
             "reason": "the run succeeded, checks_failed did not happen again, the checks pass, and it used 41200 tokens against 38900"},
            {"card_id": 50, "outcome": "positive",
             "reason": "the run succeeded, run_failed did not happen again, there were no checks, and it used 12000 tokens against 15100"}]},
        {"id": 2, "branch": "proposal/2026-10-28-allow-docs", "title": "Allow reading the docs directory",
         "body": "The rule no-reads-outside refused three reads of docs/.", "agents": [], "cards": [], "status": "untried",
         "trials": [{"card_id": null, "outcome": "skipped",
                     "reason": "the proposal names no cards, so there is no task to try it on"}]},
        {"id": 3, "branch": "proposal/2026-10-28-write-plainly", "title": "Ask the Coder to write plainly",
         "body": "The Reviewer found unclear names in card 48.", "agents": ["Coder"], "cards": [48], "status": "negative",
         "trials": [{"card_id": 48, "outcome": "negative",
                     "reason": "the run did not succeed; it still has run_failed"}]},
    ])
}

fn proposal_listing() -> Vec<client::Proposal> {
    serde_json::from_value(proposal_json()).unwrap()
}

fn retro_view() -> client::RetroView {
    serde_json::from_value(json!({
        "date": "2026-10-28", "status": "proposed",
        "warnings": ["proposal/2026-10-28-code is left out: it changes src/main.rs, which is not harness configuration"],
        "report": {"date": "2026-10-28", "errors": [
            {"kind": "tool_failed", "run": 12, "card": 48, "agent": "Coder", "text": "bash: cargo test exited with 101"},
            {"kind": "tool_refused", "run": 12, "card": 48, "agent": "Coder", "text": "bash: no-destructive-commands: rm -rf target"},
            {"kind": "run_failed", "run": 14, "card": 50, "agent": "Coder", "text": "the agent changed nothing"},
            {"kind": "steered", "run": 12, "card": 48, "agent": "Coder", "text": "use the other file"},
            {"kind": "checks_failed", "run": null, "card": 48, "agent": null, "text": "The checks failed:\ncargo test"},
            {"kind": "review_findings", "run": null, "card": 50, "agent": null, "text": "The test for the empty case is missing."},
        ]},
        "proposals": proposal_json(),
    }))
    .unwrap()
}

fn retro_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Retro));
    let retro = |message| Message::Retro(message);
    let _ = app.update(retro(retro::Message::Listed(Ok(retro_listing()))));
    let _ = app.update(retro(retro::Message::Select("2026-10-28".into())));
    let _ = app.update(retro(retro::Message::Opened(
        "2026-10-28".into(),
        Ok(retro_view()),
    )));
    app
}

#[test]
fn the_retro_screen_lists_the_days_and_shows_the_errors_and_proposals_of_one() {
    draw_sized(
        &retro_app(),
        "retro",
        &[
            "Retro",
            "Run the Retro now",
            "2026-10-28",
            "Proposed",
            "6 errors · 3 proposals",
            "2026-10-27",
            "Reported",
            "0 errors · 0 proposals",
            "Failed",
            "Errors of 2026-10-28",
            "Failed tool calls · 1",
            "bash: cargo test exited with 101",
            "Refused tool calls · 1",
            "Failed runs · 1",
            "Steering · 1",
            "Failed checks · 1",
            "Review findings · 1",
            "card 48 · run 12 · Coder",
            "Left out",
            "Proposals",
            "Tell the Coder to run the tests first",
            "proposal/2026-10-28-small-commits",
            "Positive",
            "Cards 48, 50",
            "the run succeeded, run_failed did not happen again, there were no checks, and it used 12000 tokens against 15100",
            "Allow reading the docs directory",
            "Untried",
            "Skipped",
            "the proposal names no cards, so there is no task to try it on",
            "Ask the Coder to write plainly",
            "Negative",
            "the run did not succeed; it still has run_failed",
        ],
        Size::new(1440.0, 1500.0),
    );
}

#[test]
fn a_retro_that_could_not_be_started_says_why_and_a_day_without_errors_says_so() {
    let mut app = retro_app();
    let _ = app.update(Message::Retro(retro::Message::Started(Err(
        ClientError::Server {
            status: 409,
            message: "the Retro of 2026-10-28 exists already".into(),
        },
    ))));
    assert!(shows(&app, "The Retro did not start"));
    let empty: client::RetroView = serde_json::from_value(json!({
        "date": "2026-10-27", "status": "reported", "report": {"errors": []}, "proposals": []
    }))
    .unwrap();
    let _ = app.update(Message::Retro(retro::Message::Select("2026-10-27".into())));
    let _ = app.update(Message::Retro(retro::Message::Opened(
        "2026-10-27".into(),
        Ok(empty),
    )));
    assert!(shows(&app, "No errors that day."));
    assert!(shows(&app, "The Retro proposed nothing."));
}

#[test]
fn the_agents_screen_offers_the_open_proposals_that_change_the_agent() {
    let mut app = agents_app();
    let _ = app.update(Message::Agents(agents::Message::ProposalsListed(Ok(
        proposal_listing(),
    ))));
    assert!(shows(&app, "Tell the Coder to run the tests first"));
    assert!(shows(&app, "proposal/2026-10-28-small-commits"));
    assert!(shows(&app, "Positive"));
    // The proposal that changes no agent is not offered here, nor one whose trials found it worse.
    assert!(!shows(&app, "Allow reading the docs directory"));
    assert!(!shows(&app, "Ask the Coder to write plainly"));
}

fn research_listing() -> Vec<client::Research> {
    serde_json::from_value(json!([
        {"id": 2, "repository_id": 1, "goal": "Make the host build faster", "measure": "cargo build --release 2>&1 | tail -1",
         "better": "lower", "token_budget": 200000, "status": "draft", "baseline": null, "reason": "", "tokens": 0,
         "experiments": []},
        {"id": 1, "repository_id": 1, "goal": "Cut the clean build time of the host", "measure": "./bench/build.sh",
         "better": "lower", "token_budget": 500000, "status": "done", "baseline": 372.0,
         "reason": "a round proposed no more levers", "tokens": 211400, "experiments": [
            {"id": 1, "research_id": 1, "branch": "research/1-split-crates", "title": "Split the host crate in two",
             "value": 301.0, "checks": true, "kept": true, "reason": "372 to 301 (-19.1 %), better than the baseline",
             "change_id": 4},
            {"id": 2, "research_id": 1, "branch": "research/1-more-jobs", "title": "Build with more jobs",
             "value": 375.0, "checks": null, "kept": false,
             "reason": "375 against the baseline 372: not better by more than 1 %", "change_id": null},
            {"id": 3, "research_id": 1, "branch": "research/1-lld", "title": "Link with lld",
             "value": null, "checks": null, "kept": false,
             "reason": "the measurement failed: the measure ended with exit status 127: lld: not found", "change_id": null}]},
    ]))
    .unwrap()
}

fn experiments_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Experiments));
    let _ = app.update(Message::Experiments(experiments::Message::Listed(Ok(
        research_listing(),
    ))));
    let _ = app.update(Message::Experiments(experiments::Message::Select(1)));
    app
}

#[test]
fn the_experiments_screen_lists_the_research_and_shows_the_baseline_and_levers_of_one() {
    draw_sized(
        &experiments_app(),
        "experiments",
        &[
            "Experiments",
            "New brief",
            "Draft",
            "Done",
            "3 experiments · 1 kept",
            "Cut the clean build time of the host",
            "./bench/build.sh",
            "radiance · Lower is better · 211400 of 500000 tokens used",
            "Baseline",
            "372",
            "Lever",
            "Branch",
            "Value",
            "Change",
            "Split the host crate in two",
            "research/1-split-crates",
            "301",
            "-19.1 %",
            "Kept",
            "372 to 301 (-19.1 %), better than the baseline",
            "Not kept",
            "Failed",
            "—",
        ],
        Size::new(1440.0, 1100.0),
    );
}

/// What pressing the button `label` sends, or none when the screen has no such button.
fn pressed(app: &App, label: &str) -> Option<String> {
    let mut ui = simulator(app);
    ui.click(label).ok()?;
    let sent: Vec<Message> = ui.into_messages().collect();
    Some(format!("{sent:?}"))
}

#[test]
fn a_draft_is_confirmed_or_dropped_a_confirmed_one_started_and_a_running_one_stopped() {
    let mut app = experiments_app();
    let _ = app.update(Message::Experiments(experiments::Message::Select(2)));
    assert_eq!(
        pressed(&app, "Confirm").as_deref(),
        Some("[Experiments(Act(2, \"confirm\"))]")
    );
    assert_eq!(
        pressed(&app, "Drop").as_deref(),
        Some("[Experiments(Act(2, \"stop\"))]")
    );
    assert_eq!(pressed(&app, "Start now"), None);
    assert_eq!(pressed(&app, "Stop"), None);
    let mut listing = research_listing();
    // A confirmed one is started or dropped, a running one stopped.
    for (status, available, missing) in [
        ("confirmed", ["Start now", "Drop"], "Confirm"),
        ("running", ["Stop", "Stop"], "Start now"),
    ] {
        listing[0].status = status.to_string();
        let _ = app.update(Message::Experiments(experiments::Message::Listed(Ok(
            listing.clone(),
        ))));
        for label in available {
            assert!(pressed(&app, label).is_some(), "{status}: {label}");
        }
        assert_eq!(pressed(&app, missing), None, "{status} has no {missing}");
    }
    listing[0].status = "stopped".to_string();
    let _ = app.update(Message::Experiments(experiments::Message::Listed(Ok(
        listing,
    ))));
    for label in ["Confirm", "Start now", "Stop", "Drop"] {
        assert_eq!(pressed(&app, label), None, "a stopped research has {label}");
    }
    assert!(shows(&app, "No lever has been tried yet."));
}

#[test]
fn a_brief_needs_a_repository_a_goal_a_measure_and_a_budget_before_it_is_stored() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Experiments));
    assert!(shows(&app, "No research yet"));
    let _ = app.update(Message::Experiments(experiments::Message::Compose));
    for text in [
        "radiance",
        "Lower is better",
        "Higher is better",
        "Token budget",
    ] {
        assert!(shows(&app, text), "{text}");
    }
    let stored = |app: &App| pressed(app, "Store the brief") == Some("[Experiments(Store)]".into());
    assert!(!stored(&app));
    let edit = |app: &mut App, message| {
        let _ = app.update(Message::Experiments(message));
    };
    edit(&mut app, experiments::Message::GoalEdited("Faster".into()));
    assert!(!stored(&app));
    edit(
        &mut app,
        experiments::Message::MeasureEdited("cat score".into()),
    );
    assert!(stored(&app));
    edit(&mut app, experiments::Message::BudgetEdited("0".into()));
    assert!(!stored(&app));
    edit(&mut app, experiments::Message::BudgetEdited("1000".into()));
    assert!(stored(&app));
}

fn counts(kind: &str, count: i64, with_outcome: i64, by_person: i64, held_out: i64) -> KindCounts {
    KindCounts {
        kind: kind.to_string(),
        count,
        with_outcome,
        by_person,
        held_out,
    }
}

fn decision_model_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::DecisionModel));
    let _ = app.update(Message::DecisionModel(decision_model::Message::Kinds(Ok(
        vec![
            counts("approve_issue", 14, 12, 14, 1),
            counts("merge", 9, 9, 9, 2),
            counts("tool_call", 1204, 311, 0, 118),
        ],
    ))));
    let _ = app.update(Message::DecisionModel(decision_model::Message::Exports(
        Ok(vec![
            Export {
                id: 2,
                at: 1_793_182_500,
                kinds: vec![
                    counts("merge", 9, 9, 9, 2),
                    counts("tool_call", 1100, 300, 0, 104),
                ],
            },
            Export {
                id: 1,
                at: 1_793_000_000,
                kinds: vec![counts("merge", 4, 4, 4, 0)],
            },
        ]),
    )));
    app
}

#[test]
fn the_decision_model_screen_counts_the_kinds_lists_the_exports_and_keeps_room_for_shadowing() {
    draw_sized(
        &decision_model_app(),
        "decision-model",
        &[
            "Decision model",
            "Export training set",
            "1227 judgements recorded in 3 kinds; they leave the server only in an export you start.",
            "Kinds of decision",
            "With an outcome",
            "By a person",
            "Held out",
            "approve_issue",
            "1204",
            "GET /v1/exports/2/tool_call.jsonl",
            "GET /v1/exports/2/merge.jsonl",
            "Not exported yet",
            "moebius export --out <directory>",
            "Exports",
            "#2",
            "2026-10-28 10:15 UTC",
            "merge, tool_call",
            "Shadowing",
            "No decision model is configured.",
        ],
        Size::new(1440.0, 1100.0),
    );
    assert_eq!(
        pressed(&decision_model_app(), "Export training set").as_deref(),
        Some("[DecisionModel(Export)]")
    );
}

#[test]
fn an_empty_decision_model_screen_and_a_refused_export_say_so() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::DecisionModel));
    for text in [
        "Nothing has been decided yet.",
        "No export has been made yet.",
    ] {
        assert!(shows(&app, text), "{text}");
    }
    let _ = app.update(Message::DecisionModel(decision_model::Message::Exported(
        Err(ClientError::Server {
            status: 422,
            message: "no judgements of kind \"x\"".into(),
        }),
    )));
    assert!(shows(&app, "The export was refused"));
}

fn shadowed(kind: &str, shadows: i64, agreement: Option<f64>, routed: bool) -> ShadowedKind {
    ShadowedKind {
        kind: kind.to_string(),
        shadows,
        agreement,
        failed: if kind == "merge" { 3 } else { 0 },
        dropped: if kind == "merge" { 2 } else { 0 },
        routed,
    }
}

fn takeover(id: i64, kind: &str, agreement: f64, count: i64, status: &str) -> Takeover {
    Takeover {
        id,
        kind: kind.to_string(),
        agreement,
        count,
        status: status.to_string(),
    }
}

/// The Decision model screen with a model that shadows three kinds and has taken one over.
fn shadowing_app() -> App {
    let mut app = decision_model_app();
    let _ = app.update(Message::DecisionModel(decision_model::Message::Shadowing(
        Ok(Shadowing {
            configured: true,
            model: Some("moebius-decider".into()),
            agreement: Some(0.95),
            min_judgements: Some(100),
            kinds: vec![
                shadowed("approve_issue", 0, None, false),
                shadowed("merge", 40, Some(0.85), false),
                shadowed("tool_call", 118, Some(0.975), true),
            ],
            takeovers: vec![
                takeover(2, "merge", 0.9, 80, "dismissed"),
                takeover(3, "tool_call", 0.97, 100, "merged"),
            ],
        }),
    )));
    app
}

#[test]
fn the_decision_model_screen_shows_the_agreement_per_kind_and_releases_a_taken_over_kind() {
    draw_sized(
        &shadowing_app(),
        "decision-model-shadowing",
        &[
            "Shadowing",
            "moebius-decider decides alongside whoever decides now on the judgements held back from training. A kind is proposed in the inbox once it has 100 of them and agrees on at least 95%.",
            "Shadows",
            "Agreement",
            "Decided by",
            "No shadow yet",
            "40, 3 failed, 2 dropped",
            "85% (needs 95%)",
            "97.5% (needs 95%)",
            "The decision model",
            "As before",
            "The decision model decides tool_call: it agreed on 97% of 100 judgements.",
            "Release",
        ],
        Size::new(1440.0, 1250.0),
    );
    assert_eq!(
        pressed(&shadowing_app(), "Release").as_deref(),
        Some("[DecisionModel(Release(3))]")
    );
}

/// A takeover proposal waits in the inbox to be merged or dismissed.
#[test]
fn the_inbox_offers_a_takeover_of_the_decision_model_to_merge_or_dismiss() {
    let items = vec![entry(
        1,
        "takeover",
        None,
        "The decision model can take over tool_call",
        json!({"takeover": 4, "kind": "tool_call", "agreement": 0.972, "count": 140}),
        600,
    )];
    let mut app = App::with_snapshot(snapshot(vec![], items));
    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::Inbox(inbox::Message::Tick(NOW)));
    for text in [
        "Takeovers 1",
        "Decision model",
        "The decision model can take over tool_call",
        "It chose what was recorded on 97% of 140 judgements held back from training. Merging lets it decide this kind from now on.",
        "Dismiss",
    ] {
        assert!(shows(&app, text), "{text}");
    }
    assert_eq!(
        tall_click(&app, "Merge")
            .iter()
            .map(|m| format!("{m:?}"))
            .collect::<Vec<_>>(),
        ["Inbox(Act(1, MergeTakeover { takeover: 4 }))"]
    );
    assert_eq!(
        tall_click(&app, "Dismiss")
            .iter()
            .map(|m| format!("{m:?}"))
            .collect::<Vec<_>>(),
        ["Inbox(Act(1, DismissTakeover { takeover: 4 }))"]
    );
}

/// The answer of `GET /v1/settings`: every key of the reference with its default, and `set` (keys of the file) on top.
fn settings_answer(set: serde_json::Value, more: serde_json::Value) -> serde_json::Value {
    let mut values = serde_json::Map::new();
    for (key, default) in [
        ("data_dir", json!(".moebius")),
        ("listen", json!("127.0.0.1:7171")),
        ("token_file", json!("token")),
        ("run_limit", json!(3)),
        ("retro_at", json!("23:30")),
        ("rules_file", json!(null)),
        ("harness.mcp_env", json!([])),
        ("harness.url", json!(null)),
        ("trial_window.start", json!("00:00")),
        ("trial_window.end", json!("06:00")),
        ("board.tick_ms", json!(2000)),
        ("board.round_limit", json!(5)),
        ("budgets.day_tokens", json!(null)),
        ("checks.timeout_seconds", json!(1800)),
        ("router.listen", json!("127.0.0.1:7172")),
        ("router.upstreams", json!([])),
        ("pi_bolt.program", json!("pi-bolt")),
        ("teacher.model", json!(null)),
        ("teacher.protocol", json!("openai")),
        ("teacher.timeout_seconds", json!(20)),
        ("decision_model.url", json!(null)),
        ("decision_model.model", json!(null)),
        ("decision_model.api_key_env", json!(null)),
        ("decision_model.api_key_secret", json!(null)),
        ("decision_model.timeout_seconds", json!(20)),
        ("decision_model.shadow", json!(null)),
        ("decision_model.agreement", json!(0.95)),
        ("decision_model.min_judgements", json!(100)),
        ("github.app_id", json!(null)),
        ("github.private_key_file", json!(null)),
        ("github.api_url", json!("https://api.github.com")),
        ("github.poll_seconds", json!(30)),
        ("agents", json!({})),
    ] {
        values.insert(key.into(), json!({"value": default, "default": true}));
    }
    for (key, value) in set.as_object().unwrap() {
        values.insert(key.clone(), json!({"value": value, "default": false}));
    }
    let mut answer = json!({"file": "/srv/moebius/moebius.toml", "values": values, "secrets": [],
                            "restart_needed": false, "restart_keys": []});
    answer
        .as_object_mut()
        .unwrap()
        .extend(more.as_object().unwrap().clone());
    answer
}

/// The settings of a server that is set up as the mockups are.
fn configured() -> serde_json::Value {
    settings_answer(
        json!({
            "data_dir": "/srv/moebius/.moebius", "board.round_limit": 7, "budgets.day_tokens": 20000000,
            "retro_at": "22:30", "harness.url": "https://github.com/acme/harness-configuration",
            "harness.mcp_env": ["DOCS_TOKEN"], "github.app_id": 123456,
            "github.private_key_file": "/srv/moebius/github-app.pem",
            "router.upstreams": [
                {"protocol": "openai", "base_url": "http://127.0.0.1:8080"},
                {"protocol": "anthropic", "base_url": "https://api.anthropic.com", "api_key_secret": "anthropic"},
                {"protocol": "openai", "base_url": "https://api.example.com/v1", "api_key_env": "EXAMPLE_API_KEY"},
            ],
            "teacher.model": "qwen3.8-flash-next",
            "decision_model.url": "http://127.0.0.1:8081/v1", "decision_model.model": "moebius-decider",
            "decision_model.api_key_env": "DECISION_MODEL_KEY",
        }),
        json!({"secrets": ["anthropic"]}),
    )
}

fn models_answer() -> serde_json::Value {
    json!({
        "upstreams": [
            {"protocol": "openai", "base_url": "http://127.0.0.1:8080", "api_key_set": false},
            {"protocol": "anthropic", "base_url": "https://api.anthropic.com", "api_key_set": true},
        ],
        "agents": [
            {"name": "Moebius", "model": "claude-fake", "protocol": "anthropic", "built_in": true},
            {"name": "Coder", "model": "qwen3.8-flash-next", "protocol": "openai", "thinking": "medium",
             "token_budget": 2000000, "built_in": false},
            {"name": "Reviewer", "model": "claude-fake", "protocol": "anthropic", "thinking": "high",
             "built_in": false},
        ],
        "teacher": {"model": "qwen3.8-flash-next", "protocol": "openai", "timeout_seconds": 20},
        "decision_model": {"url": "http://127.0.0.1:8081/v1", "model": "moebius-decider",
                           "timeout_seconds": 20, "api_key_set": true},
    })
}

fn setup_messages(models: serde_json::Value, server: serde_json::Value) -> [setup::Message; 3] {
    let plugins = json!({
        "time_limit_seconds": 30,
        "plugins": [
            {"name": "decision-model", "version": "0.1.0", "kinds": ["decider"], "decides": ["merge"]},
            {"name": "guard", "version": "0.1.0", "kinds": ["decider"], "decides": ["tool_call"]},
            {"name": "notes", "version": "0.2.1", "kinds": ["decider", "view"], "decides": []},
        ],
    });
    [
        setup::Message::Models(Ok(serde_json::from_value(models).unwrap())),
        setup::Message::Plugins(Ok(serde_json::from_value(plugins).unwrap())),
        setup::Message::Server(Ok(serde_json::from_value(server).unwrap())),
    ]
}

fn setup_with(page: Page, models: serde_json::Value, server: serde_json::Value) -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(page));
    for message in setup_messages(models, server) {
        let _ = app.update(Message::Setup(message));
    }
    app
}

fn setup_app(page: Page) -> App {
    setup_with(page, models_answer(), configured())
}

/// A page of the setup that has been told the same as `setup_app` and is not drawn: for the forms' requests.
fn setup_state(server: serde_json::Value) -> setup::Setup {
    let mut state = setup::Setup::default();
    for message in setup_messages(models_answer(), server) {
        let _ = state.update(message, None);
    }
    state
}

fn edit(state: &mut setup::Setup, key: &'static str, text: &str) {
    let _ = state.update(setup::Message::Edit(key, text.into()), None);
}

#[test]
fn the_models_page_has_the_upstreams_agents_the_teacher_and_the_decision_model_as_forms() {
    draw_sized(
        &setup_app(Page::Models),
        "models",
        &[
            "Models",
            "Upstreams",
            "Protocol",
            "Base URL",
            "Key",
            "Add upstream",
            "Remove",
            "Agents",
            "Coder",
            "qwen3.8-flash-next",
            "medium",
            "2.0M tok",
            "Agents are edited on the Agents page.",
            "Teacher",
            "Decision model",
            "Time limit (s)",
            "Turn off",
            "Save",
        ],
        Size::new(1440.0, 1500.0),
    );
}

#[test]
fn the_models_page_says_when_there_is_no_teacher_or_decision_model() {
    let models = json!({"upstreams": [], "agents": [], "teacher": null, "decision_model": null});
    let app = setup_with(Page::Models, models, settings_answer(json!({}), json!({})));
    let mut ui = simulator(&app);
    for text in [
        "No upstream is configured; every model request fails.",
        "The server has no agent.",
        "No teacher is configured; a call is refused while a rule in prose applies.",
        "No decision model is configured.",
        "Turn on",
        "Add upstream",
    ] {
        assert!(ui.find(text).is_ok(), "{text:?} is not on the screen");
    }
}

#[test]
fn the_plugins_page_lists_what_the_host_loaded() {
    draw(
        &setup_app(Page::Plugins),
        "plugins",
        &[
            "Plugins",
            "3 plugins loaded; a call into one is refused after 30 s.",
            "Loaded plugins",
            "decision-model",
            "guard",
            "tool_call",
            "0.2.1",
            "decider, view",
            "No kind of decision",
        ],
    );
}

#[test]
fn the_settings_page_has_every_key_in_a_box_with_its_default_and_this_app_with_the_token_masked() {
    let app = setup_app(Page::Settings);
    draw_sized(
        &app,
        "settings",
        &[
            "Settings",
            "The keys of /srv/moebius/moebius.toml. Save writes only what you changed.",
            "Server",
            "Data directory",
            "Desktop API listens on",
            "Default 127.0.0.1:7171",
            "Runs at a time",
            "Default 3",
            "Board and budgets",
            "Board tick (ms)",
            "Default 2000",
            "Coding rounds a card",
            "Checks time limit (s)",
            "Nightly schedule",
            "Retro at (HH:MM)",
            "Trial window starts",
            "Default 00:00",
            "Harness",
            "Repository URL",
            "MCP variables",
            "Pi-Bolt",
            "Program",
            "GitHub",
            "App id",
            "Private key file",
            "Poll every (s)",
            "This app",
            "http://127.0.0.1:7171",
            "••••oken",
            "Connect to another server",
            "Save",
        ],
        Size::new(1440.0, 1500.0),
    );
    assert!(simulator(&app).find("not-a-real-token").is_err());
    assert_eq!(
        format!("{:?}", tall_click(&app, "Connect to another server")),
        "[ChangeConnection]"
    );
    assert_eq!(
        pressed(&app, "Save").as_deref(),
        Some("[Setup(SaveSettings)]")
    );
}

#[test]
fn a_key_that_needs_a_restart_is_a_banner_with_a_restart_button_that_is_replaced_while_it_restarts()
{
    let server = settings_answer(
        json!({"listen": "127.0.0.1:9999", "teacher.model": "m"}),
        json!({"restart_needed": true, "restart_keys": ["listen", "teacher.model"]}),
    );
    let mut app = setup_with(Page::Settings, models_answer(), server.clone());
    draw_sized(
        &app,
        "settings-restart",
        &[
            "Restart the server to use: listen, teacher.model",
            "Restart",
        ],
        Size::new(1440.0, 1500.0),
    );
    assert_eq!(
        pressed(&app, "Restart").as_deref(),
        Some("[Setup(Restart)]")
    );
    // The banner is on the Models page too.
    let _ = app.update(Message::Select(Page::Models));
    assert!(shows(
        &app,
        "Restart the server to use: listen, teacher.model"
    ));
    let _ = app.update(Message::Setup(setup::Message::Restart));
    assert!(shows(
        &app,
        "Restarting the server. The app connects again by itself."
    ));
    assert_eq!(pressed(&app, "Restart"), None);
    // The server's answer after it is back says nothing is left to restart.
    let _ = app.update(Message::Setup(setup::Message::Server(Ok(
        serde_json::from_value(settings_answer(json!({}), json!({}))).unwrap(),
    ))));
    assert!(!shows(
        &app,
        "Restarting the server. The app connects again by itself."
    ));
    assert!(!shows(
        &app,
        "Restart the server to use: listen, teacher.model"
    ));
}

#[test]
fn a_save_says_saved_or_the_servers_message() {
    let mut app = setup_app(Page::Settings);
    assert!(!shows(&app, "Saved"));
    let saved =
        serde_json::from_value(json!({"restart_needed": false, "restart_keys": []})).unwrap();
    let _ = app.update(Message::Setup(setup::Message::Saved(Ok(Some(saved)))));
    assert!(shows(&app, "Saved"));
    let refused = client::ClientError::Server {
        status: 422,
        message: "run_limit must be at least 1".into(),
    };
    let _ = app.update(Message::Setup(setup::Message::Saved(Err(refused))));
    assert!(shows(&app, "run_limit must be at least 1"));
    // Nothing is sent when nothing was changed.
    let _ = app.update(Message::Setup(setup::Message::SaveSettings));
    assert!(shows(&app, "Nothing changed, so nothing was saved."));
    // A wrong number is said, not sent.
    let _ = app.update(Message::Setup(setup::Message::Edit(
        "run_limit",
        "many".into(),
    )));
    let _ = app.update(Message::Setup(setup::Message::SaveSettings));
    assert!(shows(&app, "Runs at a time is a whole number."));
}

#[test]
fn saving_the_settings_sends_exactly_the_keys_that_were_changed() {
    let mut state = setup_state(configured());
    assert_eq!(state.save_request(false), Ok(setup::Plan::default()));
    edit(&mut state, "board.round_limit", "9");
    edit(&mut state, "budgets.day_tokens", "5000");
    edit(&mut state, "listen", " 127.0.0.1:9000 ");
    edit(&mut state, "harness.mcp_env", "DOCS_TOKEN, OTHER");
    // A key put back to what the file says is not sent.
    edit(&mut state, "retro_at", "21:00");
    edit(&mut state, "retro_at", "22:30");
    let plan = state.save_request(false).unwrap();
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"board.round_limit": 9, "budgets.day_tokens": 5000, "listen": "127.0.0.1:9000",
               "harness.mcp_env": ["DOCS_TOKEN", "OTHER"]})
    );
    assert!(plan.remove.is_empty() && plan.store.is_empty());

    // A blank input takes a key out of the file, and a table whose inputs are all blank goes as a whole.
    let mut state = setup_state(configured());
    edit(&mut state, "budgets.day_tokens", "");
    edit(&mut state, "board.round_limit", "");
    edit(&mut state, "github.app_id", "");
    edit(&mut state, "github.private_key_file", "");
    let plan = state.save_request(false).unwrap();
    assert!(plan.set.is_empty());
    assert_eq!(
        plan.remove,
        ["board.round_limit", "budgets.day_tokens", "github"]
    );
    // One blank key of a table keeps the others, and the server says what is missing.
    let mut state = setup_state(configured());
    edit(&mut state, "github.app_id", "");
    assert_eq!(state.save_request(false).unwrap().remove, ["github.app_id"]);

    let mut state = setup_state(configured());
    edit(&mut state, "board.tick_ms", "fast");
    assert_eq!(
        state.save_request(false),
        Err("Board tick (ms) is a whole number.".into())
    );
}

#[test]
fn saving_the_models_sends_the_changed_rows_the_typed_key_and_the_tables_turned_off() {
    let mut state = setup_state(configured());
    let models = |state: &setup::Setup| state.save_request(true);
    assert_eq!(models(&state), Ok(setup::Plan::default()));

    // An upstream with a typed key is a row at the end, and the key is stored under a new name.
    let _ = state.update(setup::Message::AddUpstream, None);
    let _ = state.update(
        setup::Message::UpstreamUrl(3, "https://api.new.example".into()),
        None,
    );
    let _ = state.update(
        setup::Message::UpstreamProtocol(3, "anthropic".into()),
        None,
    );
    let _ = state.update(
        setup::Message::KeyMode(setup::Owner::Upstream(3), setup::KeyMode::Typed),
        None,
    );
    assert_eq!(
        models(&state),
        Err("Type the key of the upstream 4.".into())
    );
    let _ = state.update(
        setup::Message::KeyTyped(setup::Owner::Upstream(3), "sk-typed-secret".into()),
        None,
    );
    let plan = models(&state).unwrap();
    assert_eq!(
        plan.store,
        [("upstream-1".to_string(), "sk-typed-secret".to_string())]
    );
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"router.upstreams.3": {"protocol": "anthropic", "base_url": "https://api.new.example",
                                      "api_key_secret": "upstream-1"}})
    );
    assert!(plan.remove.is_empty());

    // A key stored before is replaced under its name, and nothing else of the row is sent again.
    let mut state = setup_state(configured());
    let _ = state.update(
        setup::Message::KeyTyped(setup::Owner::Upstream(1), "sk-new".into()),
        None,
    );
    let plan = models(&state).unwrap();
    assert_eq!(
        plan.store,
        [("anthropic".to_string(), "sk-new".to_string())]
    );
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"router.upstreams.1": {"protocol": "anthropic", "base_url": "https://api.anthropic.com",
                                      "api_key_secret": "anthropic"}})
    );

    // A removed row is removed by its place, the rows after it keep what they have, and a changed one is set at the
    // place it has then.
    let mut state = setup_state(configured());
    let _ = state.update(setup::Message::RemoveUpstream(0), None);
    let _ = state.update(
        setup::Message::KeyEnv(setup::Owner::Upstream(1), "NEW_KEY".into()),
        None,
    );
    let _ = state.update(
        setup::Message::KeyMode(setup::Owner::Upstream(1), setup::KeyMode::Environment),
        None,
    );
    let plan = models(&state).unwrap();
    assert_eq!(plan.remove, ["router.upstreams.0"]);
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"router.upstreams.1": {"protocol": "openai", "base_url": "https://api.example.com/v1",
                                      "api_key_env": "NEW_KEY"}})
    );
    assert!(plan.store.is_empty());

    // The teacher and the decision model are tables that are removed when they are turned off, and the decision
    // model's key can change from the environment to a typed one.
    let mut state = setup_state(configured());
    let _ = state.update(setup::Message::ToggleTeacher, None);
    edit(&mut state, "decision_model.timeout_seconds", "9");
    let _ = state.update(
        setup::Message::KeyMode(setup::Owner::DecisionModel, setup::KeyMode::Typed),
        None,
    );
    let _ = state.update(
        setup::Message::KeyTyped(setup::Owner::DecisionModel, "dm-key".into()),
        None,
    );
    let plan = models(&state).unwrap();
    assert_eq!(plan.remove, ["teacher", "decision_model.api_key_env"]);
    assert_eq!(
        plan.store,
        [("decision-model".to_string(), "dm-key".to_string())]
    );
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"decision_model.timeout_seconds": 9, "decision_model.api_key_secret": "decision-model"})
    );

    // A table that was off and is turned on is sent whole, and needs its model.
    let mut state = setup_state(settings_answer(json!({}), json!({})));
    let _ = state.update(setup::Message::ToggleTeacher, None);
    assert_eq!(
        models(&state),
        Err("Enter the model of the teacher.".into())
    );
    edit(&mut state, "teacher.model", "judge");
    assert_eq!(
        serde_json::Value::Object(models(&state).unwrap().set),
        json!({"teacher.model": "judge", "teacher.protocol": "openai"})
    );
}

/// What `GET /v1/rules` answers for a server with a harness repository, two rules in prose and a repository that adds one.
fn rules_view(editable: bool) -> moebius_desktop::client::RulesView {
    serde_json::from_value(json!({
        "patterns": [
            {"name": "no-destructive-commands", "description": "commands that delete recursively outside the worktree, format a disk, overwrite a device or exhaust the machine are never run"},
            {"name": "no-force-push", "description": "git push --force rewrites history on the remote"},
            {"name": "stay-in-the-worktree", "description": "files are written only inside the run's worktree"},
            {"name": "no-secrets-in-commands", "description": "a command must not contain a secret; read it from the environment instead"}
        ],
        "prose": {
            "source": if editable { json!("harness") } else { json!("file") },
            "editable": editable,
            "text": "Rules for every run.\n\n## leave-ci-alone\nDo not edit files under .github/ or any other CI configuration.\n\n## no-vendoring\nDo not copy dependencies into the tree.\n",
            "rules": [
                {"name": "leave-ci-alone", "text": "Do not edit files under .github/ or any other CI configuration."},
                {"name": "no-vendoring", "text": "Do not copy dependencies into the tree."}
            ],
            "error": null
        },
        "repositories": [{
            "repository": "radiance",
            "text": "## keep-migrations\nDo not edit a migration that exists.",
            "rules": [{"name": "keep-migrations", "text": "Do not edit a migration that exists."}],
            "error": null
        }],
        "refusals": {"no-force-push": 3, "leave-ci-alone": 1}
    }))
    .unwrap()
}

fn rules_app(editable: bool) -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Rules));
    let _ = app.update(Message::Rules(rules::Message::Loaded(Ok(rules_view(
        editable,
    )))));
    app
}

/// The Rules page drawn: `tests/snapshots/rules-tiny-skia.png`.
#[test]
fn the_rules_page_lists_the_patterns_the_rules_and_what_they_refused() {
    draw_sized(
        &rules_app(true),
        "rules",
        &[
            "Rules",
            "Save",
            "Built-in patterns",
            "no-force-push",
            "git push --force rewrites history on the remote",
            "3 refusals this week",
            "No refusals this week",
            "Rules in prose",
            "leave-ci-alone",
            "1 refusal this week",
            "no-vendoring",
            "radiance",
            "keep-migrations",
            "Read only: these rules change through a change to the repository (.moebius/rules.md on its default branch).",
        ],
        Size::new(1440.0, 1300.0),
    );
}

#[test]
fn without_a_harness_repository_the_rules_are_read_only_and_say_where_to_learn_more() {
    let app = rules_app(false);
    assert!(shows(&app, "leave-ci-alone"));
    assert!(shows(
        &app,
        "These rules are read only here: this server keeps them outside a harness configuration repository. To edit them in this app, set up [harness]; docs/guide/rules.md says how."
    ));
    assert!(!shows(&app, "Save"));
    let app = rules_app(true);
    assert!(!shows(
        &app,
        "These rules are read only here: this server keeps them outside a harness configuration repository. To edit them in this app, set up [harness]; docs/guide/rules.md says how."
    ));
}

#[test]
fn editing_the_rules_and_saving_sends_the_text_and_the_answer_is_shown() {
    let settings = settings();
    let mut rules = rules::Rules::default();
    let mut update = |message| rules.update(message, Some(&settings)).units();
    update(rules::Message::Loaded(Ok(rules_view(true))));
    update(rules::Message::Edited(text_editor::Action::Move(
        text_editor::Motion::DocumentEnd,
    )));
    update(rules::Message::Edited(text_editor::Action::Edit(
        text_editor::Edit::Paste(std::sync::Arc::new(
            "\n## ask-first\nAsk before deleting.".into(),
        )),
    )));
    assert_eq!(update(rules::Message::Save), 1);
    assert_eq!(
        rules.save_text(),
        "Rules for every run.\n\n## leave-ci-alone\nDo not edit files under .github/ or any other CI configuration.\n\n## no-vendoring\nDo not copy dependencies into the tree.\n\n## ask-first\nAsk before deleting."
    );

    let mut app = rules_app(true);
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Rules(rules::Message::Save)]
    ));
    let mut saved = rules_view(true);
    saved.commit = Some("a1b2c3d".into());
    let _ = app.update(Message::Rules(rules::Message::Saved(Ok(saved))));
    assert!(shows(&app, "Saved as a commit: a1b2c3d"));
    let invalid = ClientError::Server {
        status: 422,
        message: "line 4: the rule \"a\" is named twice".into(),
    };
    let _ = app.update(Message::Rules(rules::Message::Saved(Err(invalid))));
    assert!(shows(&app, "line 4: the rule \"a\" is named twice"));
    assert!(!shows(&app, "Saved as a commit: a1b2c3d"));
}

fn add_repository_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::AddRepository));
    app
}

fn typed(app: &mut App, messages: Vec<add_repository::Message>) {
    for message in messages {
        let _ = app.update(Message::AddRepository(message));
    }
}

#[test]
fn the_plus_of_the_sidebar_opens_the_page_that_adds_a_repository() {
    let app = App::with_snapshot(overview());
    assert_eq!(
        pressed(&app, "+").as_deref(),
        Some("[Select(AddRepository)]")
    );
    let app = add_repository_app();
    for text in ["Add a repository", "Name", "URL or path", "Forge", "None"] {
        assert!(shows(&app, text), "{text}");
    }
    assert!(!shows(&app, "Trusted authors"));
}

/// The page with the forge GitHub and a refusal: compare `tests/snapshots/add-repository-tiny-skia.png`.
#[test]
fn the_add_page_asks_for_the_github_repository_and_the_trusted_authors_and_shows_the_refusal() {
    let mut app = add_repository_app();
    typed(
        &mut app,
        vec![
            add_repository::Message::NameEdited("widgets".into()),
            add_repository::Message::UrlEdited("https://github.com/acme/widgets.git".into()),
            add_repository::Message::ForgeChosen(true),
            add_repository::Message::GithubEdited("acme/widgets".into()),
            add_repository::Message::TrustedEdited("leon, mona".into()),
            add_repository::Message::Answered(Err(ClientError::Server {
                status: 422,
                message: "the repository name \"widgets\" is taken".into(),
            })),
        ],
    );
    draw(
        &app,
        "add-repository",
        &[
            "Add a repository",
            "GitHub repository",
            "Trusted authors",
            "GitHub logins, separated by commas. Their issues need no approval.",
            "Add",
            "the server answered 422: the repository name \"widgets\" is taken",
        ],
    );
}

#[test]
fn adding_needs_a_name_and_a_url_and_with_github_the_repository_and_sends_what_the_api_takes() {
    let mut app = add_repository_app();
    let add = |app: &App| pressed(app, "Add") == Some("[AddRepository(Add)]".into());
    assert!(!add(&app));
    typed(
        &mut app,
        vec![
            add_repository::Message::NameEdited("demo".into()),
            add_repository::Message::UrlEdited("/srv/git/demo".into()),
        ],
    );
    assert!(add(&app));
    typed(&mut app, vec![add_repository::Message::ForgeChosen(true)]);
    assert!(!add(&app), "GitHub needs owner/name");
    typed(
        &mut app,
        vec![
            add_repository::Message::GithubEdited("acme/demo".into()),
            add_repository::Message::TrustedEdited(" leon, ,mona ".into()),
        ],
    );
    assert!(add(&app));

    let mut form = add_repository::AddRepository::default();
    for message in [
        add_repository::Message::NameEdited("demo".into()),
        add_repository::Message::UrlEdited("/srv/git/demo".into()),
    ] {
        let _ = form.update(message, None);
    }
    assert_eq!(
        serde_json::to_value(form.request().unwrap()).unwrap(),
        json!({"name": "demo", "url": "/srv/git/demo", "forge": "none"})
    );
    for message in [
        add_repository::Message::ForgeChosen(true),
        add_repository::Message::GithubEdited("acme/demo".into()),
        add_repository::Message::TrustedEdited(" leon, ,mona ".into()),
    ] {
        let _ = form.update(message, None);
    }
    assert_eq!(
        serde_json::to_value(form.request().unwrap()).unwrap(),
        json!({"name": "demo", "url": "/srv/git/demo", "forge": "github", "github": "acme/demo",
               "trusted": ["leon", "mona"]})
    );
}

fn paste(text: &str) -> new_issue::Message {
    new_issue::Message::BodyEdited(iced::widget::text_editor::Action::Edit(
        iced::widget::text_editor::Edit::Paste(std::sync::Arc::new(text.to_string())),
    ))
}

/// The board with the form open: compare `tests/snapshots/board-new-issue-tiny-skia.png`.
#[test]
fn a_boards_new_issue_button_opens_a_form_above_the_columns_that_creates_or_is_cancelled() {
    let mut app = App::with_snapshot(snapshot(vec![radiance("Ready")], vec![]));
    assert_eq!(
        pressed(&app, "New issue").as_deref(),
        Some("[NewIssue(Open(1))]")
    );
    let _ = app.update(Message::NewIssue(new_issue::Message::Open(1)));
    assert!(shows(&app, "Title"));
    assert_eq!(pressed(&app, "New issue").as_deref(), Some("[]"), "spent");
    assert_eq!(pressed(&app, "Create").as_deref(), Some("[]"), "no title");
    let _ = app.update(Message::NewIssue(new_issue::Message::TitleEdited(
        "  ".into(),
    )));
    assert_eq!(pressed(&app, "Create").as_deref(), Some("[]"), "blank");
    let _ = app.update(Message::NewIssue(new_issue::Message::TitleEdited(
        "Add a cache".into(),
    )));
    let _ = app.update(Message::NewIssue(paste(
        "Downloads are slow.\n\nKeep the files for a day.",
    )));
    assert_eq!(
        pressed(&app, "Create").as_deref(),
        Some("[NewIssue(Create)]")
    );
    draw(
        &app,
        "board-new-issue",
        &["New issue", "Describe the issue", "Create", "Cancel"],
    );
    // The server's refusal stays in the form, and the form is gone after Cancel.
    let _ = app.update(Message::NewIssue(new_issue::Message::Answered(Err(
        ClientError::Server {
            status: 422,
            message: "an issue needs a title".into(),
        },
    ))));
    assert!(shows(
        &app,
        "the server answered 422: an issue needs a title"
    ));
    assert_eq!(
        pressed(&app, "Cancel").as_deref(),
        Some("[NewIssue(Cancel)]")
    );
    let _ = app.update(Message::NewIssue(new_issue::Message::Cancel));
    assert!(!shows(&app, "Cancel"));
    assert!(shows(&app, "New issue"));
}

/// Two boards, chats and agents, with `text` typed into the search field.
fn searching(text: &str) -> App {
    let moebius = board(
        repository(2, "moebius", None),
        vec![
            card(60, 2, "Backlog", "Retry the usage report"),
            card(61, 2, "Ready", "Write the retro page"),
        ],
    );
    let mut app = App::with_snapshot(snapshot(vec![radiance("Ready"), moebius], vec![]));
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(chat_listing(
        "waiting",
    )))));
    let _ = app.update(Message::Agents(agents::Message::Listed(
        Ok(agent_listing()),
    )));
    let _ = app.update(Message::Search(search::Message::Edited(text.into())));
    app
}

fn key_pressed(key: iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> iced::Event {
    iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

/// The list of matches stands under the field, over the board: compare `tests/snapshots/search-tiny-skia.png`.
#[test]
fn the_list_of_matches_is_drawn_under_the_field_over_the_board() {
    let app = searching("retr");
    draw(
        &app,
        "search",
        &[
            "Cards",
            "#50 Retry model downloads on HTTP 5xx",
            "#60 Retry the usage report",
            "moebius · Backlog",
            "Pages",
            "Retro",
            "Ask the Moebius agent: retr",
        ],
    );
}

/// Typing a title of another board and pressing Enter opens that card, and the field is empty again.
#[test]
fn enter_opens_a_card_of_another_board_and_clears_the_field() {
    let mut app = searching("usage report");
    assert!(shows(&app, "#60 Retry the usage report"));
    assert!(matches!(app.page(), Page::Board(1)));
    let mut ui = simulator(&app);
    ui.click("usage report").unwrap();
    ui.tap_key(iced::keyboard::key::Named::Enter);
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(matches!(
        &messages[..],
        [Message::Search(search::Message::Submit)]
    ));
    for message in messages {
        let _ = app.update(message);
    }
    assert!(matches!(app.page(), Page::Board(2)));
    assert!(shows(&app, "Retry the usage report"));
    assert!(!shows(&app, "Ask the Moebius agent: usage report"));
    assert!(shows(&app, "Search or run…"));
}

/// `#48` finds the card by its number, and a click on the entry opens its live view because its run is working.
#[test]
fn a_number_finds_the_card_and_a_click_opens_its_live_view() {
    let mut app = searching("#48");
    let label = "#48 MTP drafter: cap the candidate tree depth";
    assert!(shows(&app, label));
    assert!(shows(&app, "radiance · In progress"));
    let messages = clicked(&app, label);
    assert!(matches!(
        &messages[..],
        [Message::Search(search::Message::Pick(0))]
    ));
    for message in messages {
        let _ = app.update(message);
    }
    assert!(shows(&app, "This run"));
}

/// The arrows move the selection; a chat, an agent and a page open what they are.
#[test]
fn the_arrows_move_the_selection_and_chats_agents_and_pages_open() {
    // Entries for "co": the agent Coder, then the pages with "co" in their title, then the ask entry.
    let mut app = searching("coder");
    let _ = app.update(Message::Search(search::Message::Move(
        search::Direction::Down,
    )));
    let _ = app.update(Message::Search(search::Message::Move(
        search::Direction::Up,
    )));
    let _ = app.update(Message::Search(search::Message::Submit));
    assert!(matches!(app.page(), Page::Agents));

    let mut app = searching("ready.luau");
    let _ = app.update(Message::Search(search::Message::Submit));
    assert!(matches!(app.page(), Page::Chat));

    let mut app = searching("usage");
    // The card of the moebius board, the chat of the same words in no title, then the page.
    for _ in 0..1 {
        let _ = app.update(Message::Search(search::Message::Move(
            search::Direction::Down,
        )));
    }
    let _ = app.update(Message::Search(search::Message::Submit));
    assert!(matches!(app.page(), Page::Usage));
}

/// The ask entry sends the first message to the Moebius agent, and the chat opens when the server has started it.
#[test]
fn the_ask_entry_starts_a_chat_and_a_failure_keeps_the_text() {
    let mut app = searching("how do I add a rule");
    let label = "Ask the Moebius agent: how do I add a rule";
    let messages = clicked(&app, label);
    assert!(matches!(
        &messages[..],
        [Message::Search(search::Message::Pick(_))]
    ));
    let mut sent = 0;
    for message in messages {
        sent += app.update(message).units();
    }
    assert_eq!(sent, 1, "one request starts the chat");
    assert!(!shows(&app, label), "the field is cleared");

    let run = serde_json::from_value(json!({"id": 8, "agent": "Moebius", "purpose": "chat",
                                            "status": "running"}))
    .unwrap();
    let _ = app.update(Message::Search(search::Message::Started(
        "how do I add a rule".into(),
        Ok(run),
    )));
    assert!(matches!(app.page(), Page::Chat));

    let mut app = searching("how");
    let _ = app.update(Message::Search(search::Message::Submit));
    let _ = app.update(Message::Search(search::Message::Started(
        "how".into(),
        Err(ClientError::Unreachable("http://x".into())),
    )));
    assert!(shows(&app, "Ask the Moebius agent: how"));
    assert!(shows(&app, "cannot reach http://x"));
}

/// Escape and an empty field close the list; Ctrl+K and the arrows are what the keys say.
#[test]
fn escape_or_clearing_the_field_closes_the_list() {
    let mut app = searching("usage");
    assert!(shows(&app, "Ask the Moebius agent: usage"));
    let escape = key_pressed(
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        iced::keyboard::Modifiers::empty(),
    );
    let message = search::key(
        escape,
        iced::event::Status::Ignored,
        iced::window::Id::unique(),
    );
    assert!(matches!(
        message,
        Some(Message::Search(search::Message::Close))
    ));
    let _ = app.update(message.unwrap());
    assert!(!shows(&app, "Ask the Moebius agent: usage"));

    let mut app = searching("usage");
    let _ = app.update(Message::Search(search::Message::Edited(String::new())));
    assert!(!shows(&app, "Ask the Moebius agent: "));

    let ctrl_k = key_pressed(
        iced::keyboard::Key::Character("k".into()),
        iced::keyboard::Modifiers::COMMAND,
    );
    assert!(matches!(
        search::key(
            ctrl_k,
            iced::event::Status::Captured,
            iced::window::Id::unique()
        ),
        Some(Message::Search(search::Message::Focus))
    ));
}

/// In the collapsed sidebar the search icon opens the sidebar, whose field shows.
#[test]
fn the_collapsed_sidebars_search_opens_the_sidebar() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::ToggleSidebar);
    assert!(!shows(&app, "Search or run…"));
    let _ = app.update(Message::Search(search::Message::Focus));
    assert!(shows(&app, "Search or run…"));
}
