//! The live view of a card's run (its conversation, the shell in its worktree, steering and stopping) and the history
//! of a card without a run.

use iced::Size;
use moebius_desktop::app::{App, Message};
use moebius_desktop::client::TerminalEvent;
use moebius_desktop::screens::live;
use moebius_desktop::widgets::terminal::PaneEvent;
use serde_json::json;

use crate::fixtures::{draw, radiance, radiance_app, shows, simulator, snapshot, update_all};

/// The events of the mockup's run: text, an allowed read, a refused edit, an allowed edit with a diff, a bash that
/// still runs, and a steer.
pub fn mockup_events() -> Vec<moebius_desktop::client::RunEvent> {
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

pub fn judgement(choice: &str, reason: &str, decider: &str) -> moebius_desktop::client::Judgement {
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
    let mut app = radiance_app();
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
    let messages = ui.into_messages().collect();
    update_all(&mut app, messages);
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

/// A run that uses MCP servers, drawn: a call folded into Pi's own, a refused one the gateway refused, and a server
/// that could not be reached. Compare `tests/snapshots/live-view-mcp-tiny-skia.png`.
#[test]
fn mcp_calls_and_errors_are_drawn_in_the_conversation() {
    let mut board = radiance("Ready");
    board["changes"] = json!([{"id": 1, "repository_id": 1, "card_id": 48,
                               "branch": "moebius/card-48", "state": "open"}]);
    let mut app = App::with_snapshot(snapshot(vec![board], vec![]));
    let _ = app.update(Message::OpenCard(48));
    let run = serde_json::from_value(json!({
        "id": 9, "card_id": 48, "agent": "Coder", "harness": "pi-bolt", "purpose": "code",
        "status": "running", "started_at": 1_790_000_000 - 360}))
    .unwrap();
    let records = vec![
        (
            "mcp_error",
            json!({"server": "sentry", "message": "could not start: no such executable"}),
        ),
        (
            "message_update",
            json!({"assistantMessageEvent": {"type": "text_delta", "contentIndex": 0,
            "delta": "I'll look for the open issues about the tree depth."}}),
        ),
        (
            "tool_check",
            json!({"tool": "mcp__moebius__github__search_issues", "tool_call_id": "c1",
            "decision": "allow", "reason": "no rule is broken"}),
        ),
        (
            "tool_execution_start",
            json!({"toolCallId": "c1", "toolName": "mcp__moebius__github__search_issues",
            "args": {"query": "tree depth"}}),
        ),
        (
            "mcp_call",
            json!({"server": "github", "tool": "search_issues", "tool_call_id": "mcp-3",
            "input": {"query": "tree depth"}, "decision": "allow", "reason": "no rule is broken",
            "result": {"content": [{"type": "text", "text": "2 open issues"}]}}),
        ),
        (
            "tool_execution_end",
            json!({"toolCallId": "c1", "toolName": "mcp__moebius__github__search_issues",
            "isError": false, "durationMs": 840,
            "result": {"content": [{"type": "text", "text": "2 open issues"}]}}),
        ),
        (
            "mcp_call",
            json!({"server": "github", "tool": "create_issue", "tool_call_id": "mcp-4",
            "input": {"title": "Cap the tree depth"}, "decision": "refuse",
            "reason": "no-mcp-writes: Do not write to GitHub; the card is the place for it.",
            "result": {"content": [{"type": "text", "text": "Moebius refused this call"}], "isError": true}}),
        ),
    ];
    let events = records
        .into_iter()
        .enumerate()
        .map(|(n, (kind, data))| moebius_desktop::client::RunEvent {
            seq: n as i64 + 1,
            kind: kind.to_string(),
            data,
            at: 1_790_000_000,
        })
        .collect();
    let loaded = live::Loaded {
        run,
        events,
        judgements: vec![],
        now: 1_790_000_000,
    };
    let _ = app.update(Message::Live(live::Message::Loaded(Ok(loaded))));
    draw(
        &app,
        "live-view-mcp",
        &[
            "MCP server sentry could not be reached",
            "could not start: no such executable",
            "github · search_issues",
            "github · create_issue",
            "1 rule broken",
            "no-mcp-writes",
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
    update_all(&mut app, messages);
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
    update_all(&mut app, messages);
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
    let mut app = radiance_app();
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
