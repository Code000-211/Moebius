//! Chats against a real server: started, written to and followed through the stream, the floating window's chats,
//! changes offered, the ask entry of the search, harness chats, drafts, and the model and reasoning of a chat.

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use iced_test::Simulator;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{Client, NewChat, RunEvent, Snapshot, StreamEvent, events};
use moebius_desktop::screens::chat;
use moebius_desktop::search;
use moebius_desktop::widgets::conversation::{Conversation, Entry, Said};

use crate::fixtures::{
    add_demo, harness_origin, next, settle, shown, snapshot_where, start_with, start_with_fake_pi,
};

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_is_started_written_to_and_followed_through_the_stream() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let client = Client::new(settings.clone());
    let mut app = App::with_snapshot(Snapshot::default());
    let mut stream = Box::pin(events(settings.clone()));
    assert_eq!(next(&mut stream).await, StreamEvent::Connected);

    // The first message starts a chat with the Moebius agent, which the fake Pi answers.
    let run = client
        .start_chat(&NewChat {
            message: Some("first question".into()),
            ..NewChat::default()
        })
        .await
        .unwrap();
    assert_eq!(
        (run.purpose.as_str(), run.agent.as_str()),
        ("chat", "Moebius")
    );
    // The form offers the agents the server can run, the Moebius agent first; a chat has no judgements yet.
    let agents = client.agents().await.unwrap();
    let names: Vec<_> = agents.iter().map(|agent| agent.name.as_str()).collect();
    assert_eq!(names, ["Moebius", "Grilling-partner", "Coder"]);
    assert_eq!(agents[0].model, "fake-coder");
    assert!(client.tool_judgements(run.id).await.unwrap().is_empty());
    let chats = client.chats().await.unwrap();
    assert_eq!(chats[0].id, run.id);
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(chats))));
    let _ = app.update(Message::OpenChat(run.id));

    // Both answers arrive through the stream into the chat's conversation: the app is fed what the stream says.
    let mut conversation = Conversation::default();
    for message in ["", "second question"] {
        if !message.is_empty() {
            client.say(run.id, message, None).await.unwrap();
        }
        let mut settled = false;
        while !settled {
            match next(&mut stream).await {
                StreamEvent::Event(event) if event.kind == "run_event" => {
                    let _ = app.update(Message::Stream(StreamEvent::Event(event.clone())));
                    if event.data["run"] == run.id {
                        let event: RunEvent = serde_json::from_value(event.data).unwrap();
                        settled |= event.kind == "agent_settled";
                        conversation.push(event);
                    }
                }
                _ => {}
            }
        }
    }
    let texts: Vec<_> = conversation
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Text(text) => Some(text.as_str()),
            Entry::Person(Said::Said, message) => Some(message.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        [
            "first question",
            "You said: first question",
            "second question",
            "You said: second question"
        ]
    );

    // The chat waits for the next message, and the app draws what was said.
    let waiting = client.chats().await.unwrap();
    assert_eq!(waiting[0].status, "waiting");
    let mut ui = Simulator::with_size(
        iced::Settings {
            fonts: moebius_desktop::theme::fonts(),
            ..iced::Settings::default()
        },
        iced::Size::new(1440.0, 900.0),
        app.view(),
    );
    assert!(ui.find("second question").is_ok());

    // An ended chat goes on in a new chat with the same agent.
    client.end_chat(run.id).await.unwrap();
    let ended = loop {
        let chats = client.chats().await.unwrap();
        if !chats[0].is_open() {
            break chats;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    assert_eq!(ended[0].id, run.id);
    let next_chat = client.continue_chat(run.id, "go on").await.unwrap();
    assert_ne!(next_chat.id, run.id);
    assert_eq!(
        (next_chat.purpose.as_str(), next_chat.agent.as_str()),
        ("chat", "Moebius")
    );
}

/// A chat the floating window starts is stored as the window's: the server lists it only when asked for all, which
/// the app does, and the Chat page leaves it out.
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_of_the_floating_window_is_kept_out_of_the_chat_list() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let client = Client::new(settings.clone());
    let page = client
        .start_chat(&NewChat {
            message: Some("on the page".into()),
            ..NewChat::default()
        })
        .await
        .unwrap();
    let window = client.ask("in the window", None, "").await.unwrap();

    let listed: Vec<i64> = client.chats().await.unwrap().iter().map(|c| c.id).collect();
    assert_eq!(listed, [window.id, page.id]);
    let chats = client.chats().await.unwrap();
    assert!(chats[0].is_assistant() && !chats[1].is_assistant());
    let http = reqwest::Client::new();
    let default: serde_json::Value = http
        .get(format!("{}/v1/chats", settings.address))
        .bearer_auth(&settings.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(default.as_array().unwrap().len(), 1);
    assert_eq!(default[0]["id"], page.id);

    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(chats))));
    let _ = app.update(Message::Select(Page::Chat));
    let mut ui = Simulator::with_size(
        iced::Settings {
            fonts: moebius_desktop::theme::fonts(),
            ..iced::Settings::default()
        },
        iced::Size::new(1440.0, 900.0),
        app.view(),
    );
    assert!(ui.find("on the page").is_ok());
    assert!(ui.find("in the window").is_err());
}

/// A chat in a repository offers what its worktree changes through the app's client: the change waits in the inbox and
/// the chat goes on.
#[tokio::test(flavor = "multi_thread")]
async fn a_chats_changes_are_offered_and_the_chat_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let repository = add_demo(&settings, dir.path()).await;
    let client = Client::new(settings);
    let run = client
        .start_chat(&NewChat {
            agent: Some("Coder".into()),
            message: Some("Make notes [write notes.txt]".into()),
            repository: repository["id"].as_i64(),
            ..NewChat::default()
        })
        .await
        .unwrap();
    let chat = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let chats = client.chats().await.unwrap();
            if chats[0].status == "waiting" {
                return chats[0].clone();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the chat did not answer in a minute");
    assert_eq!((chat.id, chat.changed_files), (run.id, 1));

    let change = client.offer_chat(run.id).await.unwrap();
    assert_eq!(change.branch, chat.branch());
    let snapshot = snapshot_where(&client, |s| {
        s.inbox.iter().any(|item| item.change() == Some(change.id))
    })
    .await;
    assert!(
        snapshot
            .inbox
            .iter()
            .any(|item| item.kind == "ready_to_merge")
    );
    assert!(client.chats().await.unwrap()[0].is_open());
    client.say(run.id, "Thanks", None).await.unwrap();
}

/// The ask entry of the search starts a real chat with the Moebius agent, which the fake Pi answers, and it opens.
#[tokio::test(flavor = "multi_thread")]
async fn the_ask_entry_of_the_search_starts_a_chat_that_opens() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let client = Client::new(settings.clone());
    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));

    let _ = app.update(Message::Search(search::Message::Edited(
        "what is a column script".into(),
    )));
    let task = app.update(Message::Search(search::Message::Submit));
    let mut stream = iced_runtime::task::into_stream(task).expect("the ask starts a request");
    let started = loop {
        match stream.next().await.expect("the request answers") {
            iced_runtime::Action::Output(message) => break message,
            _ => continue,
        }
    };
    assert!(matches!(
        started,
        Message::Search(search::Message::Started(_, Ok(_)))
    ));
    let _ = app.update(started);
    assert_eq!(app.page(), Page::Chat);

    let chats = client.chats().await.unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].agent, "Moebius");
    assert_eq!(chats[0].title, "what is a column script");
}

/// A chat in the harness configuration repository, started from the client: its skill is offered as a change of that
/// repository, which the snapshot lists and the client merges.
#[tokio::test(flavor = "multi_thread")]
async fn a_harness_chat_offers_its_change_in_the_snapshot_and_the_client_merges_it() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("../moebius-core/testdata/fake-pi");
    let more = format!(
        "[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\n",
        fake_pi.display().to_string(),
        origin.display().to_string()
    );
    let (settings, _server) = start_with(dir.path(), &more).await;
    let client = Client::new(settings.clone());
    // The server has a harness configuration repository, which the skills say.
    assert!(!client.skills().await.unwrap().is_empty());

    let message = "Add a skill.\n[put skills/testing/SKILL.md]\n---\nname: testing\ndescription: Test first.\n---\n[/put]";
    let run = client
        .start_chat(&NewChat {
            agent: Some("Coder".into()),
            message: Some(message.into()),
            harness: true,
            ..NewChat::default()
        })
        .await
        .unwrap();
    assert!(run.harness_repository);
    let waiting = loop {
        let chats = client.chats().await.unwrap();
        if chats[0].status == "waiting" {
            break chats;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(waiting[0].harness_repository);
    client.end_chat(run.id).await.unwrap();

    let snapshot = snapshot_where(&client, |s| !s.harness_changes.is_empty()).await;
    let change = &snapshot.harness_changes[0];
    assert_eq!(
        (change.repository_id, change.harness_repository),
        (None, true)
    );
    assert_eq!(change.branch, format!("moebius/chat-{}", run.id));
    assert_eq!(snapshot.inbox[0].change(), Some(change.id));

    // The app draws the chat as one in the harness configuration with its change.
    let mut app = App::with_snapshot(snapshot.clone());
    let _ = app.update(Message::Select(Page::Chat));
    let _ = app.update(Message::Chat(chat::Message::Listed(client.chats().await)));
    let _ = app.update(Message::OpenChat(run.id));
    assert!(shown(&app, "Coder in the harness configuration"));
    assert!(shown(&app, "Change offered"));

    client.merge(change.id).await.unwrap();
    let snapshot = snapshot_where(&client, |s| {
        s.harness_changes.iter().all(|c| c.state == "merged") && s.inbox.is_empty()
    })
    .await;
    assert_eq!(snapshot.harness_changes[0].state, "merged");
    let skills = client.skills().await.unwrap();
    assert!(skills.iter().any(|skill| skill.name == "testing"));
}

/// New chat opens an empty draft that starts nothing on the server; the first Send creates the chat, which is then the
/// open one.
#[tokio::test(flavor = "multi_thread")]
async fn the_first_send_of_a_draft_creates_the_chat() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let client = Client::new(settings.clone());
    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));
    let task = app.update(Message::Select(Page::Chat));
    settle(&mut app, task).await;
    let task = app.update(Message::Chat(chat::Message::NewChat));
    settle(&mut app, task).await;
    assert!(shown(
        &app,
        "Write the first message to start the chat, or open it in the terminal."
    ));
    assert!(client.chats().await.unwrap().is_empty());

    let task = app.update(Message::Chat(chat::Message::Edited("Hello there".into())));
    settle(&mut app, task).await;
    let task = app.update(Message::Chat(chat::Message::Send));
    settle(&mut app, task).await;
    let chats = client.chats().await.unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].agent, "Moebius");
    assert!(!shown(
        &app,
        "Write the first message to start the chat, or open it in the terminal."
    ));
    assert!(shown(&app, "Hello there"));
}

/// A chat started from the pickers under the field runs on the model and reasoning chosen, which the list says; its
/// Terminal button starts it without a message and shows its terminal at once, and its reasoning changes while it waits.
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_started_with_a_model_and_reasoning_lists_them_and_changes_them_while_it_waits() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let client = Client::new(settings.clone());
    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));
    let task = app.update(Message::Select(Page::Chat));
    settle(&mut app, task).await;
    let task = app.update(Message::Chat(chat::Message::NewChat));
    settle(&mut app, task).await;
    for message in [
        chat::Message::ModelChosen("big-model".into()),
        chat::Message::ThinkingChosen("high".into()),
        chat::Message::StartInTerminal,
    ] {
        let task = app.update(Message::Chat(message));
        settle(&mut app, task).await;
    }
    let chats = client.chats().await.unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].status, "waiting");
    assert_eq!(chats[0].title, "");
    assert_eq!(
        (chats[0].model.as_deref(), chats[0].thinking.as_deref()),
        (Some("big-model"), Some("high"))
    );
    // The terminal takes the conversation's place.
    assert!(!shown(&app, "Write to the agent"));

    let task = app.update(Message::Chat(chat::Message::ShowTerminal(false)));
    settle(&mut app, task).await;
    assert!(shown(&app, "Write to the agent"));
    let task = app.update(Message::Chat(chat::Message::ThinkingChosen("low".into())));
    settle(&mut app, task).await;
    let chats = client.chats().await.unwrap();
    assert_eq!(chats[0].thinking.as_deref(), Some("low"));
    assert_eq!(chats[0].model.as_deref(), Some("big-model"));
}
