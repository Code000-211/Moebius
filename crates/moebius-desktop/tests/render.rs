//! Draws the shell headlessly with a fixed snapshot (no server) and checks what is on the screen. The pictures are
//! kept in `tests/snapshots/` to be compared with `docs/design/board.png`; a later run must draw the same pixels.

use iced::{Settings, Size};
use iced_test::Simulator;
use moebius_desktop::agents;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::chat;
use moebius_desktop::client::{ClientError, Snapshot, TerminalEvent};
use moebius_desktop::inbox::{self, Action};
use moebius_desktop::live;
use moebius_desktop::terminal::PaneEvent;
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
            "This chat has ended.",
            "Send",
        ],
        Size::new(1440.0, 1100.0),
    );
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
            "No proposals yet — the Retro proposes changes from M8 on.",
        ],
        Size::new(1440.0, 1300.0),
    );
}

#[test]
fn a_server_without_a_harness_repository_lists_its_agents_read_only() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Agents));
    let _ = app.update(Message::Agents(agents::Message::Listed(
        Ok(agent_listing()),
    )));
    let _ = app.update(Message::Agents(agents::Message::SkillsListed(Ok(vec![]))));
    assert!(shows(&app, "Agents are set in moebius.toml"));
    assert!(shows(
        &app,
        "To edit them in this app, move them into a harness configuration repository (the [harness] table). The guide page \"Set up agents\" (docs/guide/agents.md) says how."
    ));
    assert!(shows(&app, "Coder"));
    assert!(clicked(&app, "Coder").is_empty());
    assert!(clicked(&app, "New agent").is_empty());
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
