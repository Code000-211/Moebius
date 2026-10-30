//! Starting a chat: New chat with its pickers of agent, place, model and reasoning, the draft the first message
//! starts, chats in the harness configuration, and the chats the Moebius agent starts to write an issue or an agent's
//! instructions.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{self, ClientError, Snapshot};
use moebius_desktop::screens::agents;
use moebius_desktop::screens::chat;
use moebius_desktop::screens::inbox;
use moebius_desktop::widgets::new_issue;
use serde_json::json;

use crate::agents::agents_app;
use crate::board::paste;
use crate::chat::{chat_events, chat_listing, chatting};
use crate::fixtures::{
    click, clicked, draw, draw_sized, pressed, radiance, radiance_app, run, shows, simulator,
    simulator_sized, update_all,
};
use crate::inbox::inbox_app;

fn available_listing() -> moebius_desktop::client::Available {
    serde_json::from_value(json!({
        "models": [
            {"id": "claude-fake", "protocol": "anthropic", "upstream": "https://api.anthropic.com"},
            {"id": "claude-large", "protocol": "anthropic", "upstream": "https://api.anthropic.com"},
            {"id": "qwen-fake", "protocol": "openai", "upstream": "http://127.0.0.1:8080"}
        ],
        "errors": []
    }))
    .unwrap()
}

/// New chat opens the empty chat at once: a hint where the conversation will be, the field with Send and Terminal, and
/// under it the pickers of the agent (the Moebius agent first and chosen), the place, the model and the reasoning, the
/// agent's until others are chosen: compare `tests/snapshots/chat-new-tiny-skia.png`.
#[test]
fn a_new_chat_opens_the_empty_chat_with_its_pickers() {
    let mut app = chatting("waiting", false);
    click(&mut app, "New chat");
    let agents = serde_json::from_value(json!([
        {"name": "Moebius", "model": "claude-fake", "protocol": "anthropic", "thinking": "medium", "built_in": true},
        {"name": "Coder", "model": "qwen-fake", "protocol": "openai", "built_in": false}
    ]))
    .unwrap();
    let _ = app.update(Message::Chat(chat::Message::Agents(Ok(agents))));
    let _ = app.update(Message::Chat(chat::Message::Models(
        Ok(available_listing()),
    )));
    // The pickers draw what they hold (Moebius, No repository, claude-fake, medium), which the picture shows.
    draw(
        &app,
        "chat-new",
        &[
            "New chat",
            "Moebius, no repository",
            "Send",
            "Terminal",
            "Agent",
            "Place",
            "Model",
            "Reasoning",
        ],
    );
    let _ = app.update(Message::Chat(chat::Message::AgentChosen("Coder".into())));
    assert!(shows(&app, "Coder, no repository"));
    // Terminal starts the chat without a message; Send needs one.
    assert_eq!(app.update(Message::Chat(chat::Message::Send)).units(), 0);
    assert_eq!(
        app.update(Message::Chat(chat::Message::StartInTerminal))
            .units(),
        1
    );
}

/// Terminal starts the draft without a message, and the chat opens in its terminal as soon as it waits.
#[test]
fn a_chat_started_from_its_terminal_button_opens_in_its_terminal() {
    let mut app = chatting("waiting", false);
    let _ = app.update(Message::Chat(chat::Message::NewChat));
    assert_eq!(
        app.update(Message::Chat(chat::Message::StartInTerminal))
            .units(),
        1
    );
    // While the server starts it, nothing starts it twice.
    assert_eq!(
        app.update(Message::Chat(chat::Message::StartInTerminal))
            .units(),
        0
    );
    let run: moebius_desktop::client::Run = serde_json::from_value(json!({
        "id": 8, "agent": "Moebius", "purpose": "chat", "status": "waiting"
    }))
    .unwrap();
    let _ = app.update(Message::Chat(chat::Message::Started(Ok(run))));
    let mut listing = chat_listing("waiting");
    listing.insert(
        0,
        serde_json::from_value(
            json!({"id": 8, "agent": "Moebius", "status": "waiting", "repository_id": null,
                                      "started_at": 1_790_000_000, "ended_at": null, "title": ""}),
        )
        .unwrap(),
    );
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(listing))));
    assert!(shows(&app, "Pi-Bolt in the chat's worktree"));
    assert!(!shows(&app, "Write to the agent"));

    // Sent with a message, the chat opens on its conversation.
    let _ = app.update(Message::Chat(chat::Message::NewChat));
    let _ = app.update(Message::Chat(chat::Message::Edited("Hello".into())));
    assert_eq!(app.update(Message::Chat(chat::Message::Send)).units(), 1);
    let run: moebius_desktop::client::Run = serde_json::from_value(json!({
        "id": 6, "agent": "Coder", "purpose": "chat", "status": "running"
    }))
    .unwrap();
    let _ = app.update(Message::Chat(chat::Message::Started(Ok(run))));
    assert!(!shows(&app, "Pi-Bolt in the chat's worktree"));
}

/// A started chat shows its model and reasoning under the field and changes them while it waits; while the agent works
/// they only show: compare `tests/snapshots/chat-settings-tiny-skia.png`.
#[test]
fn a_started_chat_shows_its_model_and_reasoning_and_changes_them_while_it_waits() {
    let mut app = chatting("waiting", false);
    let mut listing = chat_listing("waiting");
    listing[0].model = Some("claude-large".into());
    listing[0].thinking = Some("high".into());
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(listing.clone()))));
    let agents = serde_json::from_value(json!([
        {"name": "Moebius", "model": "claude-fake", "protocol": "anthropic", "built_in": true}
    ]))
    .unwrap();
    let _ = app.update(Message::Chat(chat::Message::Agents(Ok(agents))));
    let _ = app.update(Message::Chat(chat::Message::Models(
        Ok(available_listing()),
    )));
    // The pickers hold claude-large and high, which the picture shows.
    draw_sized(
        &app,
        "chat-settings",
        &["Model", "Reasoning", "Send"],
        Size::new(1440.0, 1100.0),
    );
    // A change while it waits asks the server; a refusal shows.
    assert_eq!(
        app.update(Message::Chat(chat::Message::ThinkingChosen("low".into())))
            .units(),
        1
    );
    let refusal = ClientError::Server {
        status: 409,
        message: "chat 7 is running".into(),
    };
    let _ = app.update(Message::Chat(chat::Message::SettingsChanged(Err(refusal))));
    assert!(shows(&app, "chat 7 is running"));

    // While the agent works the pickers only show what the chat runs with.
    listing[0].status = "running".into();
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(listing))));
    assert!(shows(&app, "claude-large"));
    assert!(shows(&app, "high"));
    assert_eq!(
        app.update(Message::Chat(chat::Message::ModelChosen(
            "claude-fake".into()
        )))
        .units(),
        0
    );
}

/// The app on the chat screen with chat 7 open, a chat in the harness configuration repository that offered the branch
/// `moebius/chat-7` as a change of that repository.
fn harness_chatting() -> App {
    let snapshot: Snapshot = serde_json::from_value(json!({
        "boards": [radiance("Ready")],
        "harness_changes": [{"id": 3, "repository_id": null, "harness_repository": true, "card_id": null,
                             "branch": "moebius/chat-7", "state": "open"}],
        "inbox": [{"id": 9, "kind": "ready_to_merge", "card_id": null,
                   "title": "Add a skill for testing habits",
                   "data": {"change": 3, "branch": "moebius/chat-7"}, "at": 1_790_000_000}]
    }))
    .unwrap();
    let chats = serde_json::from_value(json!([
        {"id": 7, "agent": "Moebius", "status": "succeeded", "repository_id": null, "harness_repository": true,
         "started_at": 1_790_000_000 - 600, "ended_at": null, "title": "Add a skill for testing habits"}
    ]))
    .unwrap();
    let mut app = App::with_snapshot(snapshot);
    let _ = app.update(Message::Select(Page::Chat));
    let _ = app.update(Message::Chat(chat::Message::Tick(1_790_000_000)));
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(chats))));
    let _ = app.update(Message::Chat(chat::Message::Select(7)));
    let _ = app.update(Message::Chat(chat::Message::Loaded(7, Ok(chat_events()))));
    app
}

/// A chat in the harness configuration is headed by its place, and its offered change shows and merges like a
/// repository chat's.
#[test]
fn a_harness_chat_is_drawn_with_its_offered_change_and_merged_from_there() {
    let app = harness_chatting();
    draw_sized(
        &app,
        "chat-harness",
        &[
            "Moebius in the harness configuration",
            "Add a skill for testing habits",
            "Change offered",
            "moebius/chat-7",
            "Merge",
        ],
        Size::new(1440.0, 1100.0),
    );
    assert!(matches!(
        &clicked(&app, "Merge")[..],
        [Message::Inbox(inbox::Message::Act(
            9,
            inbox::Action::Merge { change: 3 }
        ))]
    ));
}

/// The place picker of a new chat offers the harness configuration only when the server has one.
#[test]
fn a_new_chat_offers_the_harness_configuration_only_when_the_server_has_one() {
    let mut app = chatting("waiting", false);
    click(&mut app, "New chat");
    let _ = app.update(Message::Chat(chat::Message::HarnessChosen));
    assert!(!shows(&app, "Moebius in the harness configuration"));
    let skills = serde_json::from_value(json!([
        {"name": "moebius", "description": "Builds on Moebius.", "built_in": true}
    ]))
    .unwrap();
    let _ = app.update(Message::Chat(chat::Message::Skills(Ok(skills))));
    let _ = app.update(Message::Chat(chat::Message::HarnessChosen));
    assert!(shows(
        &app,
        "The agent works on a branch of the harness configuration repository: agents, skills, extensions, MCP servers and rules. \"Offer changes\" offers what it built as a change, if every agent still works with it."
    ));
    assert!(shows(&app, "Moebius in the harness configuration"));
    // New chat again drops the draft; the harness is offered only while the server has one.
    click(&mut app, "New chat");
    // A server without a harness configuration repository lists no skill.
    let _ = app.update(Message::Chat(chat::Message::Skills(Ok(vec![]))));
    let _ = app.update(Message::Chat(chat::Message::HarnessChosen));
    assert!(!shows(&app, "Moebius in the harness configuration"));
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
    let messages = ui.into_messages().collect();
    update_all(&mut app, messages);
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
    assert!(!shows(&app, "End chat"));
    click(&mut app, "Stop");
    assert!(shows(&app, "Stop this chat?"));
    assert!(matches!(
        &clicked(&app, "Stop chat")[..],
        [Message::Chat(chat::Message::Stop)]
    ));
}

#[test]
fn a_new_chat_opens_an_empty_draft_that_the_first_message_starts() {
    let mut app = chatting("waiting", false);
    click(&mut app, "New chat");
    assert!(shows(&app, "Moebius, no repository"));
    let _ = app.update(Message::Chat(chat::Message::RepositoryChosen(Some(1))));
    // The draft is empty and not in the list; the first message is what sends.
    let hint = "Write the first message to start the chat, or open it in the terminal.";
    assert!(shows(&app, "Moebius in radiance"));
    assert!(shows(&app, hint));
    assert_eq!(app.update(Message::Chat(chat::Message::Send)).units(), 0);
    let mut ui = simulator(&app);
    let _ = ui.click("Send");
    assert_eq!(ui.into_messages().count(), 0);

    let _ = app.update(Message::Chat(chat::Message::Edited(
        "Explain the board".into(),
    )));
    assert!(matches!(
        &clicked(&app, "Send")[..],
        [Message::Chat(chat::Message::Send)]
    ));
    assert_eq!(app.update(Message::Chat(chat::Message::Send)).units(), 1);
    // While the server starts it, a second Send does nothing.
    assert_eq!(app.update(Message::Chat(chat::Message::Send)).units(), 0);

    // A refusal shows and the text stays.
    let refusal = ClientError::Server {
        status: 422,
        message: "the agent cannot start".into(),
    };
    let _ = app.update(Message::Chat(chat::Message::Started(Err(refusal))));
    assert!(shows(&app, "the agent cannot start"));
    assert!(!shows(&app, hint));
    assert!(shows(&app, "Explain the board"));
    assert_eq!(app.update(Message::Chat(chat::Message::Send)).units(), 1);

    // Leaving drops the draft: another chat, a new chat, or another page.
    let _ = app.update(Message::Chat(chat::Message::Select(7)));
    assert!(!shows(&app, hint));
    let _ = app.update(Message::Chat(chat::Message::NewChat));
    assert!(shows(&app, hint));
    assert!(!shows(&app, "Explain the board"));
    let _ = app.update(Message::Select(Page::Fleet));
    let _ = app.update(Message::Select(Page::Chat));
    assert!(!shows(&app, hint));
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

pub fn moebius_chat(id: i64) -> client::Run {
    serde_json::from_value(run(id, "Moebius", "chat", "running")).unwrap()
}

#[test]
fn the_moebius_agent_can_write_a_new_issue_from_a_few_words_in_a_chat() {
    let mut app = radiance_app();
    let _ = app.update(Message::NewIssue(new_issue::Message::Open(1)));
    assert_eq!(
        pressed(&app, "Let the Moebius agent write it").as_deref(),
        Some("[NewIssue(ByAgent(true))]")
    );
    let _ = app.update(Message::NewIssue(new_issue::Message::ByAgent(true)));
    assert!(!shows(&app, "Title"), "the agent writes the title");
    assert_eq!(
        pressed(&app, "Ask the agent").as_deref(),
        Some("[]"),
        "nothing said"
    );
    let _ = app.update(Message::NewIssue(paste("cache the model downloads")));
    assert_eq!(
        pressed(&app, "Ask the agent").as_deref(),
        Some("[NewIssue(Create)]")
    );
    draw(
        &app,
        "board-new-issue-by-agent",
        &[
            "Write it",
            "Let the Moebius agent write it",
            "Ask the agent",
        ],
    );
    // The chat the server started opens in the floating window over the board, and the form is gone.
    let _ = app.update(Message::NewIssue(new_issue::Message::Asked(Ok(
        moebius_chat(9),
    ))));
    assert_eq!(app.page(), Page::Board(1));
    assert!(shows(&app, "Works in radiance"));
    assert!(!shows(&app, "Let the Moebius agent write it"));
}

#[test]
fn the_moebius_agent_can_write_an_agents_instructions_in_a_chat() {
    let mut app = agents_app();
    let size = Size::new(1440.0, 1300.0);
    let clicked_on = |app: &App, label: &str| {
        let mut ui = simulator_sized(app, size);
        ui.click(label).unwrap();
        format!("{:?}", ui.into_messages().collect::<Vec<_>>())
    };
    assert_eq!(
        clicked_on(&app, "Let the Moebius agent write them"),
        "[Agents(InstructionsByAgent(true))]"
    );
    let _ = app.update(Message::Agents(agents::Message::InstructionsByAgent(true)));
    let _ = app.update(Message::Agents(agents::Message::WishEdited(
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(
            std::sync::Arc::new("review Rust changes for unsafe code".to_string()),
        )),
    )));
    assert_eq!(clicked_on(&app, "Ask the agent"), "[Agents(AskAgent)]");
    draw_sized(
        &app,
        "agents-instructions-by-agent",
        &[
            "Write them",
            "Let the Moebius agent write them",
            "Ask the agent",
        ],
        size,
    );
    let _ = app.update(Message::Agents(agents::Message::Asked(Ok(moebius_chat(9)))));
    assert_eq!(app.page(), Page::Agents);
    assert!(shows(&app, "Moebius agent"));
}

/// A server without any agent configured lists the built-in agents without a model: the Moebius agent is offered all
/// the same, says that it needs a model, and starts once one is chosen for the chat.
#[test]
fn a_built_in_agent_without_a_model_is_offered_and_starts_on_the_model_chosen() {
    let mut app = chatting("waiting", false);
    click(&mut app, "New chat");
    let agents = serde_json::from_value(json!([
        {"name": "Moebius", "model": "", "protocol": "openai", "built_in": true},
        {"name": "Grilling-partner", "model": "", "protocol": "openai", "built_in": true},
    ]))
    .unwrap();
    let _ = app.update(Message::Chat(chat::Message::Agents(Ok(agents))));
    let _ = app.update(Message::Chat(chat::Message::Models(
        Ok(available_listing()),
    )));
    assert!(shows(&app, "Moebius, no repository"));
    assert!(shows(
        &app,
        "This agent has no model yet: choose one below, or give the Coder one on the Agents page."
    ));
    let _ = app.update(Message::Chat(chat::Message::Edited("Hello".into())));
    assert_eq!(
        app.update(Message::Chat(chat::Message::Send)).units(),
        0,
        "no model, no chat"
    );

    let _ = app.update(Message::Chat(chat::Message::ModelChosen(
        "qwen-fake".into(),
    )));
    assert!(!shows(&app, "This agent has no model yet"));
    assert_eq!(app.update(Message::Chat(chat::Message::Send)).units(), 1);
}
