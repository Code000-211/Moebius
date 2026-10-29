//! The floating chat: the round button that grows into the window, the chat it starts and keeps across pages, and
//! what the agent is told of where the person is.

use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{self, ClientError};
use moebius_desktop::widgets::float_chat;
use moebius_desktop::widgets::script_editor;
use serde_json::json;

use crate::board::{HOLD, script, script_app};
use crate::chat::chat_events;
use crate::fixtures::{clicked, clicked_id, draw, radiance_app, shows, simulator};
use crate::new_chat::moebius_chat;
use crate::rules::rules_app;

/// The board of radiance with the floating chat open on chat 7, started from it, whose events were read.
fn floating_app() -> App {
    let mut app = radiance_app();
    let _ = app.update(Message::Float(float_chat::Message::Show(true)));
    let _ = app.update(Message::Float(float_chat::Message::Tick(1_790_000_000)));
    let _ = app.update(Message::Float(float_chat::Message::Edited(
        "Write a ready.luau that starts a card only when nothing blocks it.".into(),
    )));
    let _ = app.update(Message::Float(float_chat::Message::Send));
    let _ = app.update(Message::Float(float_chat::Message::Started(Ok(
        moebius_chat(7),
    ))));
    let _ = app.update(Message::Float(float_chat::Message::Loaded(
        7,
        Ok(chat_events()),
    )));
    app
}

/// The round button stands at the bottom right of every page; a click grows it into the window, and minimising turns it
/// back. Compare `tests/snapshots/floating-chat-button-tiny-skia.png`.
#[test]
fn the_floating_button_grows_into_the_chat_window_and_minimises_back() {
    let mut app = radiance_app();
    draw(&app, "floating-chat-button", &["radiance"]);
    assert!(!shows(&app, "Moebius agent"));
    let messages = clicked_id(&app, float_chat::BUTTON);
    assert!(
        matches!(
            &messages[..],
            [Message::Float(float_chat::Message::Show(true))]
        ),
        "{messages:?}"
    );
    let _ = app.update(Message::Float(float_chat::Message::Show(true)));
    assert!(shows(&app, "Moebius agent"));
    assert!(shows(&app, "Works in radiance"));
    assert!(shows(&app, "Your first message starts the chat."));
    // Nothing is written yet, so nothing is sent.
    assert_eq!(
        app.update(Message::Float(float_chat::Message::Send))
            .units(),
        0
    );

    let messages = clicked_id(&app, float_chat::MINIMISE);
    assert!(
        matches!(
            &messages[..],
            [Message::Float(float_chat::Message::Show(false))]
        ),
        "{messages:?}"
    );
    let _ = app.update(Message::Float(float_chat::Message::Show(false)));
    assert!(!shows(&app, "Moebius agent"));

    // The button is on the other pages too, and away from a repository's page the chat works in none.
    let _ = app.update(Message::Select(Page::Rules));
    let _ = app.update(Message::Float(float_chat::Message::Show(true)));
    assert!(shows(&app, "No repository"));
}

/// The first Send starts the chat, which then stays across pages until a new conversation. Compare
/// `tests/snapshots/floating-chat-open-tiny-skia.png`.
#[test]
fn the_floating_chat_starts_a_chat_and_keeps_it_across_pages() {
    let mut app = radiance_app();
    let _ = app.update(Message::Float(float_chat::Message::Show(true)));
    let _ = app.update(Message::Float(float_chat::Message::Edited(
        "What is blocked?".into(),
    )));
    let mut ui = simulator(&app);
    ui.click("Send").unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(
        matches!(&messages[..], [Message::Float(float_chat::Message::Send)]),
        "{messages:?}"
    );
    assert_eq!(
        app.update(Message::Float(float_chat::Message::Send))
            .units(),
        1
    );
    // While the server starts it, a second Send waits; a refusal shows and the text stays.
    assert_eq!(
        app.update(Message::Float(float_chat::Message::Send))
            .units(),
        0
    );
    let refusal = ClientError::Server {
        status: 422,
        message: "the Moebius agent has no model".into(),
    };
    let _ = app.update(Message::Float(float_chat::Message::Started(Err(refusal))));
    assert!(shows(&app, "the Moebius agent has no model"));
    assert!(shows(&app, "What is blocked?"));

    let mut app = floating_app();
    draw(
        &app,
        "floating-chat-open",
        &[
            "Moebius agent",
            "Works in radiance",
            "Open in Chat",
            "New conversation",
        ],
    );
    // A later message goes to the same chat.
    let _ = app.update(Message::Float(float_chat::Message::Edited(
        "And now?".into(),
    )));
    assert_eq!(
        app.update(Message::Float(float_chat::Message::Send))
            .units(),
        1
    );
    let _ = app.update(Message::Float(float_chat::Message::Sent(Ok(moebius_chat(
        7,
    )))));
    assert!(!shows(&app, "And now?"));

    // Another page keeps the chat, which still works in its repository.
    let _ = app.update(Message::Select(Page::Rules));
    assert_eq!(app.page(), Page::Rules);
    assert!(shows(&app, "Works in radiance"));
    assert!(shows(
        &app,
        "Write a ready.luau that starts a card only when nothing blocks it."
    ));
    // Minimised and opened again, it is still there.
    let _ = app.update(Message::Float(float_chat::Message::Show(false)));
    let _ = app.update(Message::Float(float_chat::Message::Show(true)));
    assert!(shows(&app, "Open in Chat"));

    // "Open in Chat" shows the chat on the Chat page, and the window gives way.
    assert!(matches!(
        &clicked(&app, "Open in Chat")[..],
        [Message::Float(float_chat::Message::OpenInChat)]
    ));
    let _ = app.update(Message::Float(float_chat::Message::OpenInChat));
    assert_eq!(app.page(), Page::Chat);
    assert!(!shows(&app, "Moebius agent"));
    let _ = app.update(Message::Select(Page::Board(1)));
    assert!(!shows(&app, "Moebius agent"), "minimised");

    // A new conversation starts over.
    let _ = app.update(Message::Float(float_chat::Message::Show(true)));
    let _ = app.update(Message::Float(float_chat::Message::NewConversation));
    assert!(shows(&app, "Your first message starts the chat."));
    assert!(!shows(&app, "Open in Chat"));
}

/// The floating chat follows its chat's events from the stream, as the chat page does.
#[test]
fn the_floating_chat_follows_its_chats_events() {
    let mut app = floating_app();
    assert!(shows(
        &app,
        "The agent is working. Your message is sent when it is done with this turn."
    ));
    let settled = json!({"run": 7, "seq": 100, "kind": "agent_settled", "data": {"type": "agent_settled"}, "at": 1_790_000_000});
    let _ = app.update(Message::Stream(client::StreamEvent::Event(
        client::ServerEvent {
            kind: "run_event".into(),
            data: settled,
        },
    )));
    assert!(shows(&app, "The agent answers at once."));
}

/// What the agent is told of where the person is: the page, the repository, and the script being edited.
#[test]
fn the_page_context_names_the_script_being_edited_and_its_text() {
    let app = script_app("repository");
    let context = app.page_context();
    assert!(context.contains("Page: Projects > Board"), "{context}");
    assert!(context.contains("Repository: radiance (id 1)"), "{context}");
    assert!(
        context.contains("Editing the script of the Ready column (.moebius/ready.luau)"),
        "{context}"
    );
    assert!(context.contains(HOLD.trim_end()), "{context}");

    let rules = rules_app(true).page_context();
    assert!(rules.starts_with("Page: Build > Rules"), "{rules}");
    assert!(rules.contains("## leave-ci-alone"), "{rules}");

    // A long text is cut, and says so.
    let mut app = script_app("repository");
    let long = "-- x\n".repeat(2_000);
    let _ = app.update(Message::Script(script_editor::Message::Opened(Ok(script(
        "repository",
        &long,
    )))));
    let context = app.page_context();
    assert!(
        context.contains("(the first 2500 of 9999 characters)"),
        "{context}"
    );
    assert!(context.chars().count() < 4_000);
}
