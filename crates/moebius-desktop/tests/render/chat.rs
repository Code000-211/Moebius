//! The Chat page with a chat open: its Markdown and tool calls, the steps folded under the working header, the offered
//! change, the terminal with Pi-Bolt's screen, and the badge of unread answers.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{self, TerminalEvent};
use moebius_desktop::screens::chat;
use moebius_desktop::widgets::chat_terminal;
use moebius_desktop::widgets::terminal::PaneEvent;
use serde_json::json;

use crate::fixtures::{
    click, clicked, draw, draw_sized, pressed, radiance, shows, simulator, snapshot,
};
use crate::live::judgement;

/// The chat of the chat screen's picture: a person asks for a script, the agent answers with Markdown and a code block,
/// writes `.moebius/ready.luau`, checks it, and closes; the chat ended and offered its branch as a change.
pub fn chat_events() -> Vec<moebius_desktop::client::RunEvent> {
    let delta = |text: &str| {
        json!({"type": "message_update",
               "assistantMessageEvent": {"type": "text_delta", "contentIndex": 0, "delta": text}})
    };
    let thinking = |text: &str| {
        json!({"type": "message_update",
               "assistantMessageEvent": {"type": "thinking_delta", "contentIndex": 0, "delta": text}})
    };
    let script = "if card.blocked_by_count == 0 then\n  moebius.start_run(\"Coder\")\nend\n";
    let output = "ok: .moebius/ready.luau is a valid column script\n";
    let records = vec![
        json!({"type": "said", "message": "Write a ready.luau that starts a card only when nothing blocks it."}),
        delta("A **ready script** runs when a card enters `Ready`. "),
        delta("This one starts the Coder only when no issue blocks the card:\n\n```lua\n"),
        delta("if card.blocked_by_count == 0 then\n  moebius.start_run(\"Coder\")\nend\n```\n\n"),
        delta("I will write it to the repository and check it.\n"),
        json!({"type": "message_end"}),
        thinking("Looking at how the other column scripts decide whether a card may start..."),
        json!({"type": "tool_check", "tool": "write", "tool_call_id": "w1", "decision": "allow",
               "reason": "no rule is broken"}),
        json!({"type": "tool_execution_start", "toolCallId": "w1", "toolName": "write",
               "args": {"path": ".moebius/ready.luau", "content": script}}),
        json!({"type": "tool_execution_end", "toolCallId": "w1", "toolName": "write", "isError": false,
               "durationMs": 3, "result": {"content": [{"type": "text", "text": "written"}]}}),
        json!({"type": "tool_check", "tool": "edit", "tool_call_id": "e1", "decision": "refuse",
               "reason": "no-vendor-edits: Never edit files under vendor/."}),
        json!({"type": "tool_execution_start", "toolCallId": "e1", "toolName": "edit",
               "args": {"path": "vendor/ready.luau", "edits": [{"oldText": "end", "newText": "end -- done"}]}}),
        json!({"type": "tool_execution_end", "toolCallId": "e1", "toolName": "edit", "isError": true,
               "durationMs": 1, "result": {"content": [{"type": "text", "text": "blocked"}]}}),
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
            // The chat began ten minutes before the picture's time and took its steps a few seconds apart.
            at: 1_790_000_000 - 600 + n as i64 * 20,
        })
        .collect()
}

pub fn chat_listing(status: &str) -> Vec<moebius_desktop::client::Chat> {
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
pub fn chatting(status: &str, offered: bool) -> App {
    chatting_with(status, offered, chat_events())
}

/// Like `chatting`, with the events of the chat given.
fn chatting_with(
    status: &str,
    offered: bool,
    events: Vec<moebius_desktop::client::RunEvent>,
) -> App {
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
    let _ = app.update(Message::Chat(chat::Message::Loaded(7, Ok(events))));
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
            "Talk to an agent. What it builds in a repository is offered as a change.",
            "Write a ready.luau that starts a card only when nothing blocks it.",
            "Moebius in radiance",
            "Ended",
            "Explain how the router chooses an upstream.",
            "Coder",
            "Add a rule against vendor edits",
            "New chat",
            "Write a ready.luau that starts a card only when nothing blocks it.",
            "3 tool calls",
            "Write",
            "Edit",
            "refused",
            "Bash",
            "moebius check-script .moebius/ready.luau",
            "result:  ok: .moebius/ready.luau is a valid column script",
            "Change offered",
            "moebius/chat-7",
            "Merge",
            "This chat has ended. What you send starts a new chat with the same agent, which is told this one.",
            "Send",
        ],
        Size::new(1440.0, 1100.0),
    );
}

/// A waiting chat whose worktree changes files offers them with "Offer changes (3 files)" in its head, which asks the
/// server to offer them; it has no "End chat", and no "Stop" while the agent waits. Compare
/// `tests/snapshots/chat-offer-tiny-skia.png`.
#[test]
fn a_chat_with_changed_files_offers_them_and_has_no_end() {
    let mut app = chatting("waiting", false);
    let mut listing = chat_listing("waiting");
    listing[0].changed_files = 3;
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(listing.clone()))));
    draw_sized(
        &app,
        "chat-offer",
        &["Offer changes (3 files)", "Send", "Terminal"],
        Size::new(1440.0, 1100.0),
    );
    // A chat whose agent has answered carries no mark while the answer asks nothing.
    assert!(!shows(&app, "Waiting"));
    assert!(!shows(&app, "Asks you"));
    assert!(!shows(&app, "End chat"));
    assert!(!shows(&app, "Stop"));
    assert!(matches!(
        &clicked(&app, "Offer changes (3 files)")[..],
        [Message::Chat(chat::Message::Offer)]
    ));
    assert_eq!(app.update(Message::Chat(chat::Message::Offer)).units(), 1);

    // The answer shows under the field: the change in the Inbox, or why the server refused.
    let offered: moebius_desktop::client::Change = serde_json::from_value(json!({
        "id": 3, "branch": "moebius/chat-7", "state": "open"
    }))
    .unwrap();
    let _ = app.update(Message::Chat(chat::Message::Offered(Ok(offered))));
    assert!(shows(
        &app,
        "Offered as a change on moebius/chat-7: it waits in the Inbox, ready to merge."
    ));
    let refused = moebius_desktop::client::ClientError::Server {
        status: 422,
        message: "chat 7 has nothing to offer".into(),
    };
    let _ = app.update(Message::Chat(chat::Message::Offered(Err(refused))));
    assert!(shows(&app, "chat 7 has nothing to offer"));

    // While the agent works the button is there but does nothing; Stop is offered then.
    listing[0].status = "running".into();
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(listing.clone()))));
    assert_eq!(
        pressed(&app, "Offer changes (3 files)").as_deref(),
        Some("[]")
    );
    assert!(shows(&app, "Stop"));

    // A chat that changes nothing has no button.
    listing[0].changed_files = 0;
    listing[0].status = "waiting".into();
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(listing))));
    assert!(!shows(&app, "Offer changes"));
}

/// A chat whose agent is still at work shows its steps under "Agent is working" with the time and the tool calls so
/// far, one folded line each: compare `tests/snapshots/chat-working-tiny-skia.png`.
#[test]
fn a_working_chat_shows_its_steps_folded_under_the_working_header() {
    // The chat's events up to the last tool call: the agent has not answered yet.
    let mut events = chat_events();
    events.truncate(events.len() - 2);
    let app = chatting_with("running", false, events);
    assert!(!shows(&app, "Worked for"));
    draw_sized(
        &app,
        "chat-working",
        &[
            "Agent is working",
            "3 tool calls",
            "Looking at how the other column scripts decide whether a card may start...",
            "Write",
            "Edit",
            "refused",
            "Bash",
            "result:  ok: .moebius/ready.luau is a valid column script",
        ],
        Size::new(1440.0, 1100.0),
    );
    // Folded, a step does not show what only its unfolding does.
    assert!(!shows(&app, "moebius start_run"));
}

/// A click on a step unfolds it to the whole of it, highlighted, and another click folds it again; the other steps
/// stay as they were. Compare `tests/snapshots/chat-unfolded-tiny-skia.png`.
#[test]
fn a_click_on_a_step_unfolds_it_and_another_folds_it_again() {
    let mut app = chatting("succeeded", false);
    let fold = clicked(&app, "Write");
    let [Message::Chat(chat::Message::Fold(step))] = &fold[..] else {
        panic!("clicking a step should fold or unfold it: {fold:?}");
    };
    let step = *step;
    assert!(shows(&app, "result:  written"));
    let _ = app.update(Message::Chat(chat::Message::Fold(step)));
    // Unfolded, the step's result line gives way to the whole of it: the file, drawn in lines of highlighted spans.
    assert!(!shows(&app, "result:  written"));
    // The thought and the other tool calls stay folded: their lines are still there, and their contents are not.
    assert!(!shows(
        &app,
        "ok: .moebius/ready.luau is a valid column script"
    ));
    draw_sized(
        &app,
        "chat-unfolded",
        &[
            ".moebius/ready.luau",
            "result:  ok: .moebius/ready.luau is a valid column script",
        ],
        Size::new(1440.0, 1100.0),
    );
    let _ = app.update(Message::Chat(chat::Message::Fold(step)));
    assert!(shows(&app, "result:  written"));
}

/// A refused call keeps its tag on the folded line, and its reason shows when it is unfolded.
#[test]
fn a_refused_step_says_so_folded_and_why_unfolded() {
    let mut app = chatting("succeeded", false);
    assert!(shows(&app, "refused"));
    assert!(!shows(
        &app,
        "Rule broken: no-vendor-edits. Never edit files under vendor/."
    ));
    let fold = clicked(&app, "Edit");
    let [Message::Chat(chat::Message::Fold(step))] = &fold[..] else {
        panic!("clicking a step should fold or unfold it: {fold:?}");
    };
    let _ = app.update(Message::Chat(chat::Message::Fold(*step)));
    assert!(shows(
        &app,
        "Rule broken: no-vendor-edits. Never edit files under vendor/."
    ));
}

/// A waiting chat in its terminal, connected and sized, with Pi-Bolt's screen printed in it.
fn chat_in_terminal() -> App {
    let mut app = chatting("waiting", false);
    let (input, _sent) = iced::futures::channel::mpsc::channel(16);
    let printed = "\x1b[1;36mPi-Bolt\x1b[0m v1.4.0  \x1b[2mescape interrupt · ctrl+c/ctrl+d clear/exit\x1b[0m\r\n\r\n\
        \x1b[1mYou\x1b[0m  Write a ready.luau that starts a card only when nothing blocks it.\r\n\r\n\
        \x1b[1;32mPi-Bolt\x1b[0m  I wrote .moebius/ready.luau and checked it with moebius check-script.\r\n\r\n\
        \x1b[2m> \x1b[0m\x1b[7m \x1b[0m";
    for message in [
        chat::Message::ShowTerminal(true),
        chat::Message::Terminal(chat_terminal::Message::Shell(TerminalEvent::Connected(
            input,
        ))),
        chat::Message::Terminal(chat_terminal::Message::Pane(PaneEvent::Resized(Size::new(
            1000.0, 700.0,
        )))),
        chat::Message::Terminal(chat_terminal::Message::Shell(TerminalEvent::Output(
            printed.as_bytes().to_vec(),
        ))),
    ] {
        let _ = app.update(Message::Chat(message));
    }
    app
}

/// A chat switched to its terminal shows Pi-Bolt's own screen instead of the conversation and the field to write in:
/// compare `tests/snapshots/chat-terminal-tiny-skia.png`.
#[test]
fn a_chat_in_its_terminal_is_drawn_with_pi_bolts_screen() {
    draw_sized(
        &chat_in_terminal(),
        "chat-terminal",
        &["Chat", "Pi-Bolt in the chat's worktree", "connected"],
        Size::new(1440.0, 1100.0),
    );
    assert!(!shows(&chat_in_terminal(), "Send"));
}

/// Terminal beside Send opens the chat's terminal once the agent has answered, and Chat under it goes back; an ended chat
/// has none. Switching back shows the conversation again.
#[test]
fn the_terminal_beside_send_opens_once_the_agent_has_answered() {
    let mut app = chatting("waiting", false);
    assert!(shows(&app, "Send"));

    // Terminal beside Send opens the terminal once the agent has answered; while it works the button does nothing.
    let working = chatting("running", false);
    assert!(!shows(&working, "Pi-Bolt in the chat's worktree"));

    let _ = app.update(Message::Chat(chat::Message::ShowTerminal(true)));
    assert!(shows(&app, "Pi-Bolt in the chat's worktree"));
    assert!(!shows(&app, "Send"));
    let _ = app.update(Message::Chat(chat::Message::ShowTerminal(false)));
    assert!(shows(&app, "Send"));
    assert!(!shows(&app, "Pi-Bolt in the chat's worktree"));

    // Choosing another chat, leaving the page, or the chat ending leaves the terminal too.
    for leave in [
        Message::Chat(chat::Message::Select(6)),
        Message::Select(Page::Inbox),
    ] {
        let _ = app.update(Message::Chat(chat::Message::ShowTerminal(true)));
        let _ = app.update(leave);
        let _ = app.update(Message::Select(Page::Chat));
        assert!(!shows(&app, "Pi-Bolt in the chat's worktree"));
    }
}

/// Keys typed while the terminal has focus reach Pi-Bolt as bytes.
#[test]
fn keys_typed_in_a_chats_terminal_go_to_pi_bolt() {
    let mut app = chatting("waiting", false);
    let (input, mut sent) = iced::futures::channel::mpsc::channel(16);
    for message in [
        chat::Message::ShowTerminal(true),
        chat::Message::Terminal(chat_terminal::Message::Shell(TerminalEvent::Connected(
            input,
        ))),
    ] {
        let _ = app.update(Message::Chat(message));
    }
    let _ = sent.try_recv(); // the size the connection starts with
    let key = iced::keyboard::Key::Character("a".into());
    let _ = app.update(Message::Chat(chat::Message::Terminal(
        chat_terminal::Message::Key(iced::keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: iced::keyboard::key::Physical::Unidentified(
                iced::keyboard::key::NativeCode::Unidentified,
            ),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: Some("a".into()),
            repeat: false,
        }),
    )));
    assert_eq!(
        sent.try_recv().unwrap(),
        client::TerminalInput::Bytes(b"a".to_vec())
    );
}

/// The Chat entry counts the chats with an answer the person has not read; a chat open on the Chat page is read.
#[test]
fn the_chat_badge_counts_unread_answers_and_reading_one_clears_it() {
    let mut app = chatting("waiting", false);
    let mut listing = chat_listing("waiting");
    listing[1].answered = 4;
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(listing))));
    assert!(shows(&app, "1"), "chat 6 is unread");
    let _ = app.update(Message::Chat(chat::Message::Select(6)));
    let _ = app.update(Message::Chat(chat::Message::Loaded(6, Ok(chat_events()))));
    assert!(!shows(&app, "1"), "chat 6 was read");
}

/// A chat of the floating window is read there: the Chat page does not list it and the Chat entry does not count its
/// answer, but "Open in Chat" shows it on the page.
#[test]
fn a_chat_of_the_floating_window_is_not_listed_nor_unread_until_opened_in_chat() {
    let mut app = chatting("waiting", false);
    let mut listing = chat_listing("waiting");
    listing.insert(
        0,
        serde_json::from_value(json!({"id": 8, "agent": "Moebius", "status": "waiting", "repository_id": null,
            "started_at": 1_790_000_000 - 60, "title": "Asked in the window", "answered": 4, "seen": 0,
            "origin": "assistant"}))
        .unwrap(),
    );
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(listing))));
    assert!(!shows(&app, "Asked in the window"));
    assert!(!shows(&app, "1"), "the window's chat counts unread");

    let _ = app.update(Message::OpenChat(8));
    assert!(shows(&app, "Asked in the window"));
    assert!(!shows(&app, "1"), "the window's chat counts unread");
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
    let judged = |choice: &str, reason: &str, decider: &str| moebius_desktop::client::Judgement {
        run: Some(7),
        ..judgement(choice, reason, decider)
    };
    let refusal = "no-vendor-edits: Never edit files under vendor/.";
    let _ = app.update(Message::Chat(chat::Message::Judged(
        7,
        Ok(vec![
            judged("allow", "no rule is broken", "patterns"),
            judged("refuse", refusal, "teacher"),
            judged("allow", "no rule is broken", "teacher"),
        ]),
    )));
    // Folded, a step says only that it was refused; unfolded it says who checked it and what the tool took.
    assert!(!shows(&app, "Pattern · ran 3 ms"));
    for name in ["Write", "Bash"] {
        click(&mut app, name);
    }
    assert!(shows(&app, "Pattern · ran 3 ms"));
    assert!(shows(&app, "Teacher · ran 140 ms"));
}

/// A chat whose agent's answer asks the person a question is marked "Asks you" in the list and its head, and the page
/// counts it; one that only answered is not.
#[test]
fn only_a_chat_whose_agent_asks_a_question_is_marked() {
    let mut app = chatting("waiting", false);
    let mut listing = chat_listing("waiting");
    listing[0].asks = true;
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(listing))));
    assert!(shows(&app, "Asks you"));
    assert!(shows(&app, "1 chat asks you something."));
}

/// The open chat follows what the agent writes to its end, unless the person scrolled up; what they send is followed
/// again.
#[test]
fn a_chat_follows_new_answers_unless_scrolled_up() {
    let mut app = chatting("working", false);
    let more = || Message::Chat(chat::Message::Loaded(7, Ok(vec![])));
    let followed = app.update(more()).units();
    assert!(followed > 0, "new events scroll a followed chat to its end");

    let _ = app.update(Message::Chat(chat::Message::Scrolled(false)));
    assert_eq!(
        app.update(more()).units(),
        0,
        "a chat scrolled up stays put"
    );

    let _ = app.update(Message::Chat(chat::Message::Send));
    assert_eq!(
        app.update(more()).units(),
        followed,
        "sending follows again"
    );
}

/// An answer is marked read once, not at every streamed piece of it, which made a long answer slow to show.
#[test]
fn an_answer_is_marked_read_once() {
    let mut app = chatting("working", false);
    let mut listing = chat_listing("working");
    listing[0].answered = 4;
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(listing))));
    // Scrolled up, so that a piece of the answer has nothing to follow either.
    let _ = app.update(Message::Chat(chat::Message::Scrolled(false)));
    for seq in 30..33 {
        let piece = client::ServerEvent {
            kind: "run_event".into(),
            data: json!({"run": 7, "seq": seq, "kind": "message_update", "at": 1_790_000_000,
                         "data": {"type": "message_update", "assistantMessageEvent":
                                  {"type": "text_delta", "contentIndex": 0, "delta": "more "}}}),
        };
        let task = app.update(Message::Stream(client::StreamEvent::Event(piece)));
        assert_eq!(task.units(), 0, "a piece of the answer is not marked read");
    }
}
