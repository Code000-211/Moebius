//! The client against a real server (`moebius-core` as a dev-dependency): it reads the snapshot with the server's
//! token, receives an event on the stream, and is told when the token is wrong or the server cannot be reached.
#![cfg(unix)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced_test::Simulator;
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::store::Usage;
use moebius_desktop::agents;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::chat;
use moebius_desktop::client::{
    Client, ClientError, NewRepository, RunEvent, Snapshot, StreamEvent, TerminalEvent, events,
    terminal,
};
use moebius_desktop::conversation::{Conversation, Entry, Said};
use moebius_desktop::search;
use moebius_desktop::settings::Settings;
use moebius_desktop::setup;
use moebius_desktop::terminal::Shell;
use serde_json::json;

/// A server with no agents on a free port; returns the settings that connect to it and the server.
async fn start(dir: &Path) -> (Settings, Arc<Server>) {
    start_with(dir, "").await
}

/// Like `start`, with `more` added to `moebius.toml`.
async fn start_with(dir: &Path, more: &str) -> (Settings, Arc<Server>) {
    let text = format!("[router]\nlisten = \"127.0.0.1:0\"\n{more}");
    std::fs::write(dir.join("moebius.toml"), &text).unwrap();
    let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", api_listener.local_addr().unwrap());
    let router_listener = tokio::net::TcpListener::bind(config.router_listen)
        .await
        .unwrap();
    let router_addr = router_listener.local_addr().unwrap();
    let server = Arc::new(Server::new(config, router_addr).unwrap());
    let token = server.token.to_string();
    tokio::spawn(server.clone().serve(api_listener, router_listener));
    (Settings { address, token }, server)
}

fn origin(dir: &Path) -> std::path::PathBuf {
    let origin = dir.join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "A demo repository.\n").unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    origin
}

async fn post(settings: &Settings, path: &str, body: serde_json::Value) -> serde_json::Value {
    reqwest::Client::new()
        .post(format!("{}{path}", settings.address))
        .bearer_auth(&settings.token)
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// The next event of the stream; the limit only stops a test that would otherwise hang.
async fn next(stream: &mut (impl Stream<Item = StreamEvent> + Unpin)) -> StreamEvent {
    tokio::time::timeout(Duration::from_secs(60), stream.next())
        .await
        .expect("no event in a minute")
        .expect("the stream ended")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_client_reads_the_snapshot_and_follows_the_events() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start(dir.path()).await;
    let origin = origin(dir.path());
    let repository = post(
        &settings,
        "/v1/repositories",
        json!({"name": "demo", "url": origin.to_str().unwrap()}),
    )
    .await;

    let client = Client::new(settings.clone());
    assert!(!client.health().await.unwrap().version.is_empty());
    let snapshot = client.snapshot().await.unwrap();
    let board = &snapshot.boards[0];
    assert_eq!(board.repository.name, "demo");
    assert_eq!(board.repository.default_branch, "main");
    assert_eq!(board.live_runs(), 0);

    let mut stream = Box::pin(events(settings.clone()));
    assert_eq!(next(&mut stream).await, StreamEvent::Connected);
    post(
        &settings,
        &format!("/v1/repositories/{}/issues", repository["id"]),
        json!({"title": "Add a feature"}),
    )
    .await;
    loop {
        if let StreamEvent::Event(event) = next(&mut stream).await
            && event.kind == "card_added"
        {
            assert_eq!(event.data["title"], "Add a feature");
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_snapshot_of_a_real_server_has_what_the_board_shows_of_a_card_in_progress() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start(dir.path()).await;
    let origin = origin(dir.path());
    let repository = post(
        &settings,
        "/v1/repositories",
        json!({"name": "demo", "url": origin.to_str().unwrap()}),
    )
    .await;
    let id = repository["id"].as_i64().unwrap();
    let store = &server.app.store;
    let card = store
        .add_card(id, "Add a feature", "", "person", "In progress")
        .unwrap();
    store.set_dependencies(card.id, &[41].into()).unwrap();
    let run = store
        .add_run(Some(card.id), "Coder", "test", "code")
        .unwrap();
    store.set_run_status(run.id, "running").unwrap();
    let usage = Usage {
        run_id: Some(run.id),
        model: "fake".into(),
        input_tokens: 1200,
        output_tokens: 300,
        cached_tokens: 0,
        duration_ms: 0,
        tools: Vec::new(),
    };
    store.add_usage(&usage).unwrap();

    let snapshot = Client::new(settings).snapshot().await.unwrap();
    let board = &snapshot.boards[0];
    assert_eq!(board.round_limit, 5);
    assert_eq!(board.live_runs(), 1);
    let shown = &board.cards[0];
    assert_eq!((shown.rounds, shown.tokens), (1, 1500));
    assert_eq!(shown.blocked_by, [41]);
    let run = shown.run.as_ref().unwrap();
    assert_eq!(
        (run.agent.as_str(), run.status.as_str()),
        ("Coder", "running")
    );
    assert!(shown.is_live());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_token_is_not_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let (mut settings, _) = start(dir.path()).await;
    settings.token = "not the token".to_string();

    let client = Client::new(settings.clone());
    assert_eq!(
        client.health().await.unwrap_err(),
        ClientError::Unauthorized
    );
    assert_eq!(
        client.snapshot().await.unwrap_err(),
        ClientError::Unauthorized
    );
    let mut stream = Box::pin(events(settings));
    assert_eq!(next(&mut stream).await, StreamEvent::Unauthorized);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_zip_sent_to_the_server_is_stored_where_the_answer_says() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _server) = start(dir.path()).await;
    let upload = Client::new(settings)
        .upload(b"PK\x03\x04 skills".to_vec())
        .await
        .unwrap();
    assert!(upload.path.ends_with(&format!("uploads/{}.zip", upload.id)));
    assert_eq!(std::fs::read(&upload.path).unwrap(), b"PK\x03\x04 skills");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_is_not_there_cannot_be_reached() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = Client::new(Settings {
        address: address.clone(),
        token: "token".to_string(),
    });
    assert_eq!(
        client.health().await.unwrap_err(),
        ClientError::Unreachable(address)
    );
}

/// A server whose Coder is the fake Pi of `moebius-core`, which keeps a run going on a prompt that says `[hang]`.
async fn start_with_fake_pi(dir: &Path) -> (Settings, Arc<Server>) {
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("../moebius-core/testdata/fake-pi");
    let config = Config::parse(
        &format!(
            "[board]\ntick_ms = 100\n[router]\nlisten = \"127.0.0.1:0\"\n[pi_bolt]\nprogram = {fake_pi:?}\n\
             [agents.Coder]\nmodel = \"fake-coder\"\ntoken_budget = 100000\n"
        ),
        dir,
        &dir.join("moebius.toml"),
    )
    .unwrap();
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", api_listener.local_addr().unwrap());
    let router_listener = tokio::net::TcpListener::bind(config.router_listen)
        .await
        .unwrap();
    let router_addr = router_listener.local_addr().unwrap();
    let server = Arc::new(Server::new(config, router_addr).unwrap());
    let token = server.token.to_string();
    tokio::spawn(server.clone().serve(api_listener, router_listener));
    (Settings { address, token }, server)
}

/// The next `run_event` of `run` on the stream.
async fn next_run_event(
    stream: &mut (impl Stream<Item = StreamEvent> + Unpin),
    run: i64,
) -> RunEvent {
    loop {
        if let StreamEvent::Event(event) = next(stream).await
            && event.kind == "run_event"
            && event.data["run"] == run
        {
            return serde_json::from_value(event.data).unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_live_view_loads_follows_steers_and_stops_a_run() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let origin = origin(dir.path());
    let repository = post(
        &settings,
        "/v1/repositories",
        json!({"name": "demo", "url": origin.to_str().unwrap()}),
    )
    .await;
    post(
        &settings,
        &format!("/v1/repositories/{}/issues", repository["id"]),
        json!({"title": "Add a feature", "body": "[hang]"}),
    )
    .await;

    // The board starts the card; its run keeps going.
    let client = Client::new(settings.clone());
    let run = loop {
        let snapshot = client.snapshot().await.unwrap();
        if let Some(run) = snapshot.boards[0].cards[0].run.clone()
            && run.status == "running"
        {
            break run;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    // The events stored so far are loaded as a page.
    let mut conversation = Conversation::default();
    let mut stream = Box::pin(events(settings.clone()));
    assert_eq!(next(&mut stream).await, StreamEvent::Connected);
    // The agent has started once its first record is stored.
    loop {
        let page = client.run_events(run.id, 0).await.unwrap();
        if page.iter().any(|event| event.kind == "agent_start") {
            page.into_iter().for_each(|event| conversation.push(event));
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(client.run(run.id).await.unwrap().status, "running");

    // A steer through the client is stored and echoed by the fake Pi, and both arrive on the stream.
    client.steer(run.id, "also test depth 0").await.unwrap();
    let (mut steered, mut echoed) = (false, false);
    while !(steered && echoed) {
        let event = next_run_event(&mut stream, run.id).await;
        steered |= event.kind == "steered";
        echoed |= event.kind == "response" && event.data["echo"] == "also test depth 0";
        conversation.push(event);
    }
    assert!(conversation.entries().iter().any(|entry| {
        matches!(entry, Entry::Person(Said::Steered, message) if message == "also test depth 0")
    }));

    // The page and the stream together are the whole stored run.
    let mut stored = Conversation::default();
    for event in client.run_events(run.id, 0).await.unwrap() {
        stored.push(event);
    }
    assert_eq!(stored.entries(), conversation.entries());
    assert_eq!(stored.last_seq(), conversation.last_seq());

    // Stopping ends the run; a steer after that is refused with the server's message.
    client.stop(run.id).await.unwrap();
    loop {
        if let StreamEvent::Event(event) = next(&mut stream).await
            && event.kind == "run_ended"
        {
            assert_eq!(event.data["status"], "stopped");
            break;
        }
    }
    match client.steer(run.id, "too late").await.unwrap_err() {
        ClientError::Server { status, message } => {
            assert_eq!(status, 422);
            assert!(message.contains("not running"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_apps_shell_echoes_a_command_run_in_a_real_servers_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let origin = origin(dir.path());
    let repository = post(
        &settings,
        "/v1/repositories",
        json!({"name": "demo", "url": origin.to_str().unwrap()}),
    )
    .await;
    post(
        &settings,
        &format!("/v1/repositories/{}/issues", repository["id"]),
        json!({"title": "Add a feature", "body": "[hang]"}),
    )
    .await;
    let client = Client::new(settings.clone());
    let run = loop {
        let snapshot = client.snapshot().await.unwrap();
        if let Some(run) = snapshot.boards[0].cards[0].run.clone()
            && run.status == "running"
        {
            break run;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    // The shell starts in the run's worktree; what it prints reaches the terminal state.
    let mut shell = Shell::new(1);
    let mut stream = Box::pin(terminal(
        settings.clone(),
        format!("/v1/runs/{}/terminal", run.id),
    ));
    let limit = Duration::from_secs(60);
    let mut sent = false;
    while !shell.lines().iter().any(|line| line == "moebius-42") {
        let event = tokio::time::timeout(limit, stream.next())
            .await
            .expect("nothing was printed in a minute")
            .expect("the shell's stream ended");
        assert!(!matches!(event, TerminalEvent::Closed(_)), "{event:?}");
        shell.on_event(event);
        if shell.is_open() && !sent {
            sent = true;
            let enter = Key::Named(Named::Enter);
            for c in "echo moebius-$((40+2))".chars() {
                let text = c.to_string();
                shell.key(
                    &Key::Character(text.as_str().into()),
                    Modifiers::empty(),
                    Some(&text),
                );
            }
            shell.key(&enter, Modifiers::empty(), Some("\r"));
        }
    }

    // A run that is over has no shell: the stream says why and ends.
    client.stop(run.id).await.unwrap();
    loop {
        if client.run(run.id).await.unwrap().status == "stopped" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut stream = Box::pin(terminal(settings, format!("/v1/runs/{}/terminal", run.id)));
    match stream.next().await.unwrap() {
        TerminalEvent::Closed(reason) => assert!(reason.contains("422"), "{reason}"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_terminal_page_opens_a_shell_in_the_directory_of_the_configuration_file() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start(dir.path()).await;
    let mut shell = Shell::on_server(1);
    let mut stream = Box::pin(terminal(settings, "/v1/terminal".to_string()));
    let home = std::fs::canonicalize(dir.path()).unwrap();
    let wanted = format!("where-{}-end", home.display());
    let limit = Duration::from_secs(60);
    let mut sent = false;
    while !shell.lines().contains(&wanted) {
        let event = tokio::time::timeout(limit, stream.next())
            .await
            .expect("nothing was printed in a minute")
            .expect("the shell's stream ended");
        assert!(!matches!(event, TerminalEvent::Closed(_)), "{event:?}");
        shell.on_event(event);
        if shell.is_open() && !sent {
            sent = true;
            for c in "echo where-$(pwd -P)-end".chars() {
                let text = c.to_string();
                shell.key(
                    &Key::Character(text.as_str().into()),
                    Modifiers::empty(),
                    Some(&text),
                );
            }
            shell.key(&Key::Named(Named::Enter), Modifiers::empty(), Some("\r"));
        }
    }
}

/// Reads the snapshot until `ready` holds for it; the limit only stops a test that would otherwise hang.
async fn snapshot_where(
    client: &Client,
    ready: impl Fn(&moebius_desktop::client::Snapshot) -> bool,
) -> moebius_desktop::client::Snapshot {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let snapshot = client.snapshot().await.unwrap();
            if ready(&snapshot) {
                return snapshot;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the snapshot did not change in a minute")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_inbox_merges_a_change_and_approves_an_outside_issue() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start_with_fake_pi(dir.path()).await;
    let origin = origin(dir.path());
    let repository = post(
        &settings,
        "/v1/repositories",
        json!({"name": "demo", "url": origin.to_str().unwrap()}),
    )
    .await;
    let repository = repository["id"].as_i64().unwrap();
    post(
        &settings,
        &format!("/v1/repositories/{repository}/issues"),
        json!({"title": "Add a feature"}),
    )
    .await;
    let outside = server
        .app
        .store
        .add_card(repository, "An outside issue", "", "stranger", "Backlog")
        .unwrap();

    // The first card is coded by the fake Pi and waits to be merged; the outside issue waits to be approved.
    let client = Client::new(settings);
    let snapshot = snapshot_where(&client, |snapshot| {
        ["ready_to_merge", "approve_issue"]
            .iter()
            .all(|kind| snapshot.inbox.iter().any(|item| item.kind == *kind))
    })
    .await;
    let item = |kind: &str| {
        snapshot
            .inbox
            .iter()
            .find(|item| item.kind == kind)
            .unwrap()
            .clone()
    };

    let to_merge = item("ready_to_merge");
    let card = to_merge.card_id.unwrap();
    client.merge(to_merge.change().unwrap()).await.unwrap();
    let column_of = |snapshot: &moebius_desktop::client::Snapshot, id: i64| {
        snapshot.boards[0]
            .cards
            .iter()
            .find(|card| card.id == id)
            .map(|card| card.column.clone())
    };
    let merged = snapshot_where(&client, |s| column_of(s, card).as_deref() == Some("Done")).await;
    assert!(merged.inbox.iter().all(|item| item.id != to_merge.id));

    let to_approve = item("approve_issue");
    assert_eq!(to_approve.card_id, Some(outside.id));
    client.approve(outside.id).await.unwrap();
    let approved = snapshot_where(&client, |s| {
        column_of(s, outside.id).as_deref() != Some("Backlog")
    })
    .await;
    assert!(approved.inbox.iter().all(|item| item.id != to_approve.id));

    // A request the server refuses comes back with its message: the change is merged already.
    match client.merge(to_merge.change().unwrap()).await.unwrap_err() {
        ClientError::Server { status, .. } => assert_eq!(status, 422),
        other => panic!("{other:?}"),
    }
}

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
        .start_chat("", "first question", None, false)
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

/// A chat in a repository offers what its worktree changes through the app's client: the change waits in the inbox and
/// the chat goes on.
#[tokio::test(flavor = "multi_thread")]
async fn a_chats_changes_are_offered_and_the_chat_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let origin = origin(dir.path());
    let repository = post(
        &settings,
        "/v1/repositories",
        json!({"name": "demo", "url": origin.to_str().unwrap()}),
    )
    .await;
    let client = Client::new(settings);
    let run = client
        .start_chat(
            "Coder",
            "Make notes [write notes.txt]",
            repository["id"].as_i64(),
            false,
        )
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

/// A bare harness configuration repository with the agent Coder, a skill and an MCP server.
fn harness_origin(dir: &Path) -> std::path::PathBuf {
    let work = dir.join("harness-work");
    std::fs::create_dir_all(work.join("agents")).unwrap();
    std::fs::create_dir_all(work.join("skills/house-style")).unwrap();
    git(&work, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(
        work.join("agents/Coder.toml"),
        "model = \"coder-model\"\nskills = [\"house-style\"]\n",
    )
    .unwrap();
    std::fs::write(work.join("agents/Coder.md"), "Keep changes small.\n").unwrap();
    std::fs::write(
        work.join("skills/house-style/SKILL.md"),
        "---\nname: house-style\ndescription: Write plainly.\n---\n",
    )
    .unwrap();
    std::fs::write(work.join("mcp.toml"), "[docs]\ncommand = \"docs-server\"\n").unwrap();
    git(&work, &["add", "."]).unwrap();
    git(&work, &["commit", "--quiet", "-m", "Start"]).unwrap();
    let bare = dir.join("harness.git");
    git(
        dir,
        &["init", "--quiet", "--bare", "-b", "main", "harness.git"],
    )
    .unwrap();
    git(&work, &["push", "--quiet", bare.to_str().unwrap(), "main"]).unwrap();
    bare
}

/// Editing an agent in the app is a commit in the harness configuration repository, and an invalid edit is refused with
/// the server's message.
#[tokio::test(flavor = "multi_thread")]
async fn editing_an_agent_is_a_commit_on_the_harness_repository_and_a_bad_edit_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let more = format!("[harness]\nurl = {:?}\n", origin.display().to_string());
    let (settings, _server) = start_with(dir.path(), &more).await;
    let client = Client::new(settings);

    let agents = client.agents().await.unwrap();
    assert!(
        agents
            .iter()
            .any(|agent| agent.name == "Coder" && agent.model == "coder-model")
    );
    let skills = client.skills().await.unwrap();
    assert_eq!(skills[0].name, "moebius");
    assert!(skills[0].built_in);
    assert!(skills[1..6].iter().all(|skill| skill.built_in));
    assert_eq!(skills[6].name, "house-style");
    assert_eq!(skills[6].description, "Write plainly.");
    let servers = client.mcp_servers().await.unwrap();
    assert_eq!(
        (servers[0].name.as_str(), servers[0].kind.as_str()),
        ("docs", "stdio")
    );

    let coder = client.agent("Coder").await.unwrap();
    assert_eq!(coder.instructions, "Keep changes small.\n");
    assert!(coder.built_in.is_none());
    let moebius = client.agent("Moebius").await.unwrap();
    assert!(moebius.settings.is_none());
    let built_in = moebius.built_in.expect("the Moebius agent is built in");
    assert!(
        built_in
            .instructions
            .starts_with("You are the Moebius agent.")
    );
    assert_eq!(built_in.skills.len(), 6);
    assert_eq!(built_in.skills[1].name, "moebius-operate");
    let mut coder = coder.settings.expect("the Coder has files");
    coder.model = "better-model".into();
    coder.thinking = Some("high".into());
    coder.mcp = vec!["docs".into()];
    let saved = client
        .save_agent("Coder", &coder, "Be careful.")
        .await
        .unwrap();
    let commit = saved.commit.expect("the edit is a commit");
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Update the agent Coder"
    );
    assert!(
        git(&origin, &["rev-parse", "main"])
            .unwrap()
            .starts_with(&commit)
    );
    let file = git(&origin, &["show", "main:agents/Coder.toml"]).unwrap();
    assert!(file.contains("better-model") && file.contains("thinking = \"high\""));
    assert_eq!(
        client.agent("Coder").await.unwrap().instructions,
        "Be careful."
    );

    // A new agent is a commit too, and an agent that names what is not there is refused with the reason.
    let mut strong = coder.clone();
    strong.model = "big".into();
    client.save_agent("Strong", &strong, "").await.unwrap();
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Add the agent Strong"
    );
    let head = git(&origin, &["rev-parse", "main"]).unwrap();
    strong.skills = vec!["missing".into()];
    let Err(ClientError::Server { status, message }) =
        client.save_agent("Strong", &strong, "").await
    else {
        panic!("the edit was accepted");
    };
    assert_eq!(status, 422);
    assert!(
        message.contains("\"missing\" is not in skills/"),
        "{message}"
    );
    assert_eq!(git(&origin, &["rev-parse", "main"]).unwrap(), head);

    client.delete_agent("Strong").await.unwrap();
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Delete the agent Strong"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_client_reads_the_usage_the_server_stored() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start_with(dir.path(), "[budgets]\nday_tokens = 10000\n").await;
    let store = &server.app.store;
    let run = store.add_run(None, "Coder", "test", "code").unwrap();
    for (model, input, output, cached, duration_ms, tools) in [
        ("big", 1000, 100, 600, 4000, vec!["bash", "read"]),
        ("big", 500, 50, 0, 2000, vec!["bash"]),
        ("small", 200, 20, 100, 1000, vec![]),
    ] {
        store
            .add_usage(&Usage {
                run_id: Some(run.id),
                model: model.into(),
                input_tokens: input,
                output_tokens: output,
                cached_tokens: cached,
                duration_ms,
                tools: tools.into_iter().map(String::from).collect(),
            })
            .unwrap();
    }
    let report = Client::new(settings).usage(1).await.unwrap();
    assert_eq!(report.days_in_window, 1);
    assert_eq!(report.totals.tokens, 1870);
    assert_eq!(report.totals.calls, 3);
    assert_eq!(report.totals.model_ms, 7000);
    assert_eq!(report.totals.day_budget, Some(10_000));
    assert!((report.totals.cached_share - 700.0 / 1700.0).abs() < 1e-9);
    assert_eq!(report.days.len(), 1);
    assert_eq!(
        (report.days[0].tokens, report.days[0].model.as_str()),
        (1870, "big")
    );
    assert_eq!(report.models[0].model, "big");
    assert_eq!((report.models[0].tokens, report.models[0].calls), (1650, 2));
    assert_eq!(report.agents[0].agent, "Coder");
    assert_eq!(report.agents[0].model_ms, 7000);
    let activities: Vec<_> = report
        .activities
        .iter()
        .map(|a| (a.activity.as_str(), a.model_ms, a.calls, a.output_tokens))
        .collect();
    assert_eq!(
        activities,
        [
            ("bash", 4000, 2, 100),
            ("read", 2000, 1, 50),
            ("answering", 1000, 1, 20)
        ]
    );
}

/// A day with a failed run becomes a Retro through the client, the fake Pi analyst proposes a branch for the Coder, and
/// the app's screens read the report and the proposal.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_runs_the_retro_and_reads_its_report_and_proposals() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("../moebius-core/testdata/fake-pi");
    let more = format!(
        "[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\n",
        fake_pi.display().to_string(),
        origin.display().to_string()
    );
    let (settings, server) = start_with(dir.path(), &more).await;
    let day = server.app.store.today().unwrap();
    let branch = format!("proposal/{day}-careful");
    let store = &server.app.store;
    let failed = store.add_run(None, "Coder", "test", "code").unwrap();
    let why = format!("stalled [propose {branch} agents/Coder.md]");
    store
        .append_event(failed.id, "moebius_error", &json!({ "message": why }))
        .unwrap();
    store.set_run_status(failed.id, "failed").unwrap();

    let client = Client::new(settings.clone());
    assert!(client.retros().await.unwrap().is_empty());
    let mut stream = Box::pin(events(settings.clone()));
    assert_eq!(next(&mut stream).await, StreamEvent::Connected);
    let started = client.start_retro().await.unwrap();
    assert_eq!(started.date, day);
    assert_eq!(started.report.errors[0].kind, "run_failed");
    assert_eq!(started.report.errors[0].agent.as_deref(), Some("Coder"));
    loop {
        if let StreamEvent::Event(event) = next(&mut stream).await
            && event.kind == "retro_done"
        {
            assert_eq!(event.data["status"], "proposed");
            break;
        }
    }
    let retro = client.retro(&day).await.unwrap();
    assert_eq!(retro.status, "proposed");
    assert_eq!(retro.proposals.len(), 1);
    assert_eq!(retro.proposals[0].branch, branch);
    assert_eq!(retro.proposals[0].agents, ["Coder"]);
    let list = client.retros().await.unwrap();
    assert_eq!((list[0].errors, list[0].proposals), (1, 1));
    assert_eq!(client.proposals().await.unwrap(), retro.proposals);
    // A day has one Retro: the server says so, and the client passes the message on.
    let again = client.start_retro().await.unwrap_err();
    assert!(
        matches!(again, ClientError::Server { status: 409, .. }),
        "{again:?}"
    );
}

/// A proposal without cards is tried (and so offered as it is) by the server; the client lists it with its trial and
/// merges it, and the inbox item goes.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_lists_proposals_with_their_trials_and_merges_one() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("../moebius-core/testdata/fake-pi");
    let more = format!(
        "[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\n",
        fake_pi.display().to_string(),
        origin.display().to_string()
    );
    let (settings, server) = start_with(dir.path(), &more).await;
    let store = &server.app.store;
    let day = "2026-10-01";
    store
        .add_retro(day, &json!({ "date": day, "errors": [] }), "proposed")
        .unwrap();
    let clone = dir.path().join(".moebius/harness");
    let branch = format!("proposal/{day}-plain");
    git(&clone, &["checkout", "--quiet", "-b", &branch]).unwrap();
    std::fs::write(
        clone.join("agents/Coder.md"),
        "Keep changes small.\nWrite plainly.\n",
    )
    .unwrap();
    git(&clone, &["commit", "--quiet", "-am", "Write plainly"]).unwrap();
    git(&clone, &["push", "--quiet", "origin", &branch]).unwrap();
    git(&clone, &["checkout", "--quiet", "main"]).unwrap();
    store
        .add_proposal(
            day,
            &branch,
            "Write plainly",
            "",
            &["Coder".to_string()],
            &[],
        )
        .unwrap();

    let runs: Arc<dyn moebius_core::board::Runs> = server.harness.clone();
    let repo = server.harness_repo.clone().unwrap();
    let app = server.app.clone();
    tokio::task::spawn_blocking(move || moebius_core::trial::start(&app, &runs, &repo, 1))
        .await
        .unwrap()
        .unwrap();
    let client = Client::new(settings.clone());
    let proposal = loop {
        let proposals = client.proposals().await.unwrap();
        if proposals[0].status != "trying" {
            break proposals.into_iter().next().unwrap();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(proposal.status, "untried");
    assert_eq!(proposal.trials.len(), 1);
    assert_eq!(proposal.trials[0].outcome, "skipped");
    let snapshot = client.snapshot().await.unwrap();
    assert_eq!(snapshot.inbox[0].proposal(), Some(1));

    client.merge_proposal(1).await.unwrap();
    assert_eq!(client.proposals().await.unwrap()[0].status, "merged");
    assert!(client.snapshot().await.unwrap().inbox.is_empty());
    let text = git(&origin, &["show", "main:agents/Coder.md"]).unwrap();
    assert!(text.contains("Write plainly."), "{text}");
    // Merged is final.
    let again = client.merge_proposal(1).await.unwrap_err();
    assert!(
        matches!(again, ClientError::Server { status: 409, .. }),
        "{again:?}"
    );
}

/// A brief becomes a research through the client: stored as a draft, confirmed, started and, as its measure fails,
/// ended as failed, which the stream reports; the list shows it.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_stores_confirms_and_starts_a_research() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with(dir.path(), "[agents.Coder]\nmodel = \"m\"\n").await;
    let origin = origin(dir.path());
    let repository = post(
        &settings,
        "/v1/repositories",
        json!({"name": "demo", "url": origin.to_str().unwrap()}),
    )
    .await;
    let repository = repository["id"].as_i64().unwrap();
    let client = Client::new(settings.clone());
    assert!(client.researches().await.unwrap().is_empty());

    let draft = client
        .create_research(repository, "Be faster", "exit 1", "lower", 1000)
        .await
        .unwrap();
    assert_eq!((draft.status.as_str(), draft.baseline), ("draft", None));
    let refused = client
        .create_research(repository, "Be faster", "exit 1", "smaller", 1000)
        .await
        .unwrap_err();
    assert!(matches!(refused, ClientError::Server { status: 422, .. }));

    let mut stream = Box::pin(events(settings.clone()));
    assert_eq!(next(&mut stream).await, StreamEvent::Connected);
    let confirmed = client.act_on_research(draft.id, "confirm").await.unwrap();
    assert_eq!(confirmed.status, "confirmed");
    client.act_on_research(draft.id, "start").await.unwrap();
    loop {
        if let StreamEvent::Event(event) = next(&mut stream).await
            && event.kind == "research_status"
            && event.data["status"] == "failed"
        {
            break;
        }
    }
    let list = client.researches().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "failed");
    assert!(
        list[0].reason.contains("the baseline failed"),
        "{}",
        list[0].reason
    );
    assert_eq!(list[0].token_budget, 1000);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_grilling_ends_in_an_inbox_item_whose_issues_the_client_confirms_into_cards() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let origin = origin(dir.path());
    let repository = post(
        &settings,
        "/v1/repositories",
        json!({"name": "demo", "url": origin.to_str().unwrap()}),
    )
    .await;
    let client = Client::new(settings.clone());

    // The chat is started in the repository, answered by the fake Pi, which leaves the issues, and ended.
    let message = "Export the reports.\n[put .moebius/grilling/issues.json]\n\
        [{\"title\": \"Add the button\", \"body\": \"On the report page.\", \"blocked_by\": []}, \
         {\"title\": \"Write the CSV\", \"body\": \"Its columns.\", \"blocked_by\": [0]}]\n[/put]";
    let run = client
        .start_chat(
            "Grilling-partner",
            message,
            repository["id"].as_i64(),
            false,
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(60), async {
        while client.chats().await.unwrap()[0].status != "waiting" {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the agent did not answer");
    client.end_chat(run.id).await.unwrap();

    let snapshot = snapshot_where(&client, |s| {
        s.inbox.iter().any(|item| item.kind == "grilling_issues")
    })
    .await;
    let item = snapshot
        .inbox
        .iter()
        .find(|i| i.kind == "grilling_issues")
        .unwrap();
    assert_eq!(item.title, "2 issues from a grilling");
    assert_eq!(item.repository(), repository["id"].as_i64());
    let issues = item.issues();
    assert_eq!(issues[1].blocked_by, [0]);

    // The inbox page draws the list to edit, with the buttons that act on it.
    let mut app = App::with_snapshot(snapshot.clone());
    let _ = app.update(Message::Select(Page::Inbox));
    let mut ui = Simulator::with_size(
        iced::Settings {
            fonts: moebius_desktop::theme::fonts(),
            ..iced::Settings::default()
        },
        iced::Size::new(1440.0, 900.0),
        app.view(),
    );
    assert!(ui.find("Add the button").is_ok());

    // Confirming the list the person left makes the cards, and the item leaves the inbox.
    client
        .confirm_grilling(item.grilling().unwrap(), &issues)
        .await
        .unwrap();
    let confirmed = snapshot_where(&client, |s| {
        s.inbox.iter().all(|item| item.kind != "grilling_issues") && s.boards[0].cards.len() == 2
    })
    .await;
    let cards = &confirmed.boards[0].cards;
    assert_eq!(cards[0].title, "Add the button");
    assert_eq!(cards[1].blocked_by, [cards[0].number]);
    // Confirming again is refused with the server's message.
    match client
        .confirm_grilling(item.grilling().unwrap(), &issues)
        .await
        .unwrap_err()
    {
        ClientError::Server { status, .. } => assert_eq!(status, 422),
        other => panic!("{other:?}"),
    }
}

/// The client counts the judgements the server stored, makes an export of them and lists it.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_counts_judgements_and_starts_an_export() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start(dir.path()).await;
    let client = Client::new(settings);
    assert!(client.judgement_kinds().await.unwrap().is_empty());
    assert!(client.exports().await.unwrap().is_empty());

    let question = moebius_core::decide::Question {
        kind: "merge".into(),
        context: std::collections::BTreeMap::from([("change".to_string(), "1".to_string())]),
        options: vec!["merge".into()],
        ..Default::default()
    };
    let id = server
        .app
        .decisions
        .record_by(
            &moebius_core::decide::Actor::Person,
            &question,
            "merge",
            "fine",
        )
        .unwrap()
        .judgement_id;
    server.app.store.set_outcome(id, "merged").unwrap();

    let kinds = client.judgement_kinds().await.unwrap();
    assert_eq!(kinds.len(), 1);
    assert_eq!(
        (
            kinds[0].kind.as_str(),
            kinds[0].count,
            kinds[0].with_outcome,
            kinds[0].by_person
        ),
        ("merge", 1, 1, 1)
    );
    let made = client.start_export().await.unwrap();
    assert_eq!((made.id, made.kinds.clone()), (1, kinds));
    assert_eq!(client.exports().await.unwrap(), vec![made]);
    // The person's decision to export is now a kind of its own.
    let kinds = client.judgement_kinds().await.unwrap();
    assert_eq!(
        kinds.iter().map(|k| k.kind.as_str()).collect::<Vec<_>>(),
        ["export", "merge"]
    );
}

/// Without a `[decision_model]` the client reads that none is configured; with one it reads an empty shadowing, and a
/// takeover is merged and released through the client.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_reads_the_shadowing_and_merges_and_releases_a_takeover() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _server) = start(dir.path()).await;
    let shadowing = Client::new(settings).shadowing().await.unwrap();
    assert!(!shadowing.configured && shadowing.kinds.is_empty() && shadowing.model.is_none());

    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start_with(
        dir.path(),
        "[decision_model]\nurl = \"http://127.0.0.1:1/v1\"\nmodel = \"decider\"\nmin_judgements = 7\n",
    )
    .await;
    let client = Client::new(settings);
    let shadowing = client.shadowing().await.unwrap();
    assert!(shadowing.configured && shadowing.kinds.is_empty());
    assert_eq!(
        (
            shadowing.model.as_deref(),
            shadowing.agreement,
            shadowing.min_judgements
        ),
        (Some("decider"), Some(0.95), Some(7))
    );

    let store = &server.app.store;
    let proposed = store.add_takeover("tool_call", 0.97, 120).unwrap();
    store
        .add_inbox_item(
            "takeover",
            None,
            "The decision model can take over tool_call",
            &serde_json::json!({"takeover": proposed.id, "kind": "tool_call", "agreement": 0.97, "count": 120}),
        )
        .unwrap();
    let snapshot = client.snapshot().await.unwrap();
    assert_eq!(snapshot.inbox[0].takeover(), Some(proposed.id));

    client.merge_takeover(proposed.id).await.unwrap();
    assert!(server.app.decisions.routed().contains("tool_call"));
    assert!(client.snapshot().await.unwrap().inbox.is_empty());
    let shadowing = client.shadowing().await.unwrap();
    assert_eq!(shadowing.takeovers[0].status, "merged");
    assert_eq!(shadowing.kinds[0].kind, "tool_call");
    assert!(shadowing.kinds[0].routed);

    client.release_takeover(proposed.id).await.unwrap();
    assert!(!server.app.decisions.routed().contains("tool_call"));
    assert_eq!(
        client.shadowing().await.unwrap().takeovers[0].status,
        "released"
    );
    // A second release is refused with the server's message.
    assert!(matches!(
        client.release_takeover(proposed.id).await.unwrap_err(),
        ClientError::Server { status: 409, .. }
    ));

    let second = store.add_takeover("merge", 0.99, 90).unwrap();
    client.dismiss_takeover(second.id).await.unwrap();
    assert_eq!(
        client.shadowing().await.unwrap().takeovers[1].status,
        "dismissed"
    );
}

#[tokio::test]
async fn the_client_reads_the_models_plugins_and_settings_of_a_real_server() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _server) = start_with(
        dir.path(),
        "[board]\nround_limit = 4\n\
         [[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"http://127.0.0.1:8080\"\n\
         [teacher]\nmodel = \"teacher-model\"\n\
         [agents.Coder]\nmodel = \"coder-model\"\nthinking = \"low\"\ntoken_budget = 1000\n",
    )
    .await;
    let client = Client::new(settings);

    let models = client.models().await.unwrap();
    assert_eq!(models.upstreams.len(), 1);
    assert_eq!(models.upstreams[0].base_url, "http://127.0.0.1:8080");
    assert!(!models.upstreams[0].api_key_set);
    let coder = models.agents.iter().find(|a| a.name == "Coder").unwrap();
    assert_eq!(
        (
            coder.model.as_str(),
            coder.thinking.as_deref(),
            coder.token_budget
        ),
        ("coder-model", Some("low"), Some(1000))
    );
    assert_eq!(models.teacher.unwrap().model, "teacher-model");
    assert!(models.decision_model.is_none());

    let plugins = client.plugins().await.unwrap();
    assert_eq!(plugins.time_limit_seconds, 30);
    let guard = plugins.plugins.iter().find(|p| p.name == "guard").unwrap();
    assert_eq!(guard.decides, ["tool_call"]);

    let server = client.server_settings().await.unwrap();
    assert_eq!(server.value("board.round_limit"), &json!(4));
    assert!(server.is_set("board.round_limit"));
    assert_eq!(server.value("listen"), &json!("127.0.0.1:7171"));
    assert!(!server.is_set("listen"));
    assert!(server.value("github.app_id").is_null());
    assert!(server.value("harness.url").is_null());
    assert!(!server.restart_needed);
}

/// Runs a task and every task its messages start, feeding the messages to the app.
async fn settle(app: &mut App, task: iced::Task<Message>) {
    let mut pending = vec![task];
    while let Some(task) = pending.pop() {
        let Some(mut stream) = iced_runtime::task::into_stream(task) else {
            continue;
        };
        while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Output(message) = action {
                pending.push(app.update(message));
            }
        }
    }
}

fn shown(app: &App, text: &str) -> bool {
    Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1440.0, 1800.0),
        app.view(),
    )
    .find(text)
    .is_ok()
}

/// Saving in the Settings and Models pages writes `moebius.toml` of a real server and the pages read it back; a key
/// that needs a restart shows the banner, and an agent of the file is edited in the Agents page.
#[tokio::test(flavor = "multi_thread")]
async fn the_apps_settings_models_and_agents_are_saved_to_the_servers_file_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start_with(
        dir.path(),
        "# kept\n[board]\nround_limit = 4\n[agents.Coder]\nmodel = \"coder-model\"\n",
    )
    .await;
    let client = Client::new(settings.clone());
    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));
    let file = dir.path().join("moebius.toml");

    let task = app.update(Message::Select(Page::Settings));
    settle(&mut app, task).await;
    assert!(shown(&app, "Coding rounds a card"));
    for (key, text) in [
        ("board.round_limit", "9"),
        ("budgets.day_tokens", "5000"),
        ("listen", "127.0.0.1:9"),
    ] {
        let _ = app.update(Message::Setup(setup::Message::Edit(key, text.into())));
    }
    let task = app.update(Message::Setup(setup::Message::SaveSettings));
    settle(&mut app, task).await;
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.starts_with("[router]") || text.contains("# kept"),
        "{text}"
    );
    assert!(
        text.contains("round_limit = 9") && text.contains("day_tokens = 5000"),
        "{text}"
    );
    assert!(text.contains("listen = \"127.0.0.1:9\""), "{text}");
    let read = client.server_settings().await.unwrap();
    assert_eq!(read.value("board.round_limit"), &json!(9));
    assert_eq!(read.value("budgets.day_tokens"), &json!(5000));
    assert_eq!(read.restart_keys, ["listen"]);
    assert!(shown(&app, "Saved"));
    assert!(shown(&app, "Restart the server to use: listen"));
    assert_eq!(server.live.get().round_limit, 9);

    // A refused value is the server's message, and the file is as it was.
    let _ = app.update(Message::Setup(setup::Message::Edit(
        "run_limit",
        "0".into(),
    )));
    let task = app.update(Message::Setup(setup::Message::SaveSettings));
    settle(&mut app, task).await;
    assert!(shown(&app, "run_limit must be at least 1"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);

    // An upstream with a typed key: the key is stored by the server, the file names it.
    let task = app.update(Message::Select(Page::Models));
    settle(&mut app, task).await;
    let _ = app.update(Message::Setup(setup::Message::AddUpstream));
    let _ = app.update(Message::Setup(setup::Message::UpstreamUrl(
        0,
        "http://127.0.0.1:1".into(),
    )));
    let owner = setup::Owner::Upstream(0);
    let _ = app.update(Message::Setup(setup::Message::KeyMode(
        owner,
        setup::KeyMode::Typed,
    )));
    let _ = app.update(Message::Setup(setup::Message::KeyTyped(
        owner,
        "sk-typed-by-hand-0123456789".into(),
    )));
    let task = app.update(Message::Setup(setup::Message::SaveModels));
    settle(&mut app, task).await;
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("api_key_secret = \"upstream-1\""), "{text}");
    assert!(!text.contains("sk-typed-by-hand"));
    let read = client.server_settings().await.unwrap();
    assert_eq!(read.secrets, ["upstream-1"]);
    assert_eq!(
        read.value("router.upstreams")[0]["base_url"],
        "http://127.0.0.1:1"
    );
    let stored = std::fs::read_to_string(dir.path().join(".moebius/secrets.toml")).unwrap();
    assert!(stored.contains("sk-typed-by-hand-0123456789"));

    // The agent of the file is opened, changed and saved from the Agents page.
    let task = app.update(Message::Select(Page::Agents));
    settle(&mut app, task).await;
    let task = app.update(Message::Agents(agents::Message::Select("Coder".into())));
    settle(&mut app, task).await;
    let _ = app.update(Message::Agents(agents::Message::ModelEdited(
        "better-model".into(),
    )));
    let _ = app.update(Message::Agents(agents::Message::BudgetEdited("777".into())));
    let task = app.update(Message::Agents(agents::Message::Save));
    settle(&mut app, task).await;
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("model = \"better-model\"") && text.contains("token_budget = 777"),
        "{text}"
    );
    let agents = client.agents().await.unwrap();
    let coder = agents.iter().find(|a| a.name == "Coder").unwrap();
    assert_eq!(
        (coder.model.as_str(), coder.token_budget),
        ("better-model", Some(777))
    );
    assert!(shown(&app, "Saved in moebius.toml."));

    // A restart is asked for with the button; the app then waits for the server (which this test does not start again).
    let task = app.update(Message::Select(Page::Settings));
    settle(&mut app, task).await;
    let task = app.update(Message::Setup(setup::Message::Restart));
    settle(&mut app, task).await;
    tokio::time::timeout(Duration::from_secs(60), server.live.restart_asked())
        .await
        .expect("the server is asked to restart");
    assert!(shown(
        &app,
        "Restarting the server. The app connects again by itself."
    ));
}

/// Saving the rules in the app is a commit in the harness configuration repository, and text that does not parse is
/// refused with the server's message.
#[tokio::test(flavor = "multi_thread")]
async fn saving_the_rules_is_a_commit_on_the_harness_repository_and_bad_text_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let more = format!("[harness]\nurl = {:?}\n", origin.display().to_string());
    let (settings, _server) = start_with(dir.path(), &more).await;
    let client = Client::new(settings);

    let view = client.rules().await.unwrap();
    assert_eq!(view.patterns.len(), 4);
    assert!(view.prose.editable && view.prose.rules.is_empty());

    let text = "## leave-ci-alone\nDo not edit .github/.";
    let saved = client.save_rules(text).await.unwrap();
    let commit = saved.commit.expect("the save is a commit");
    assert_eq!(saved.prose.rules[0].name, "leave-ci-alone");
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Update the rules"
    );
    assert!(
        git(&origin, &["rev-parse", "main"])
            .unwrap()
            .starts_with(&commit)
    );
    assert_eq!(git(&origin, &["show", "main:rules.md"]).unwrap(), text);
    assert!(client.save_rules(text).await.unwrap().commit.is_none());

    let head = git(&origin, &["rev-parse", "main"]).unwrap();
    let Err(ClientError::Server { status, message }) = client.save_rules("## a\n## a\n").await
    else {
        panic!("the text was accepted");
    };
    assert_eq!(status, 422);
    assert!(message.contains("named twice"), "{message}");
    assert_eq!(git(&origin, &["rev-parse", "main"]).unwrap(), head);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_client_adds_a_repository_from_a_bare_clone_and_creates_an_issue_on_it() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start(dir.path()).await;
    let bare = dir.path().join("bare.git");
    git(
        dir.path(),
        &[
            "clone",
            "--quiet",
            "--bare",
            origin(dir.path()).to_str().unwrap(),
            "bare.git",
        ],
    )
    .unwrap();
    let client = Client::new(settings);
    let request = NewRepository {
        name: "demo".into(),
        url: bare.display().to_string(),
        forge: "none",
        github: None,
        trusted: Vec::new(),
    };
    let repository = client.add_repository(&request).await.unwrap();
    assert_eq!(
        (repository.name.as_str(), repository.forge.as_str()),
        ("demo", "none")
    );
    let again = client.add_repository(&request).await.unwrap_err();
    assert!(matches!(again, ClientError::Server { status: 422, .. }));

    let card = client
        .create_issue(repository.id, "Add a cache", "Keep the files.")
        .await
        .unwrap();
    assert_eq!((card.number, card.column.as_str()), (1, "Backlog"));
    let snapshot = client.snapshot().await.unwrap();
    let board = snapshot
        .boards
        .iter()
        .find(|board| board.repository.id == repository.id)
        .unwrap();
    assert!(
        board
            .cards
            .iter()
            .any(|c| c.title == "Add a cache" && c.body == "Keep the files.")
    );
    let blank = client
        .create_issue(repository.id, " ", "")
        .await
        .unwrap_err();
    assert!(matches!(blank, ClientError::Server { status: 422, .. }));
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
        .start_chat("Coder", message, None, true)
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

/// Editing a skill and an MCP server in the app is a commit in the harness configuration repository; one an agent names
/// is not deleted, and an invalid server is refused with the server's message.
#[tokio::test(flavor = "multi_thread")]
async fn editing_a_skill_and_an_mcp_server_is_a_commit_and_a_named_one_is_not_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let more = format!(
        "[harness]\nurl = {:?}\nmcp_env = [\"DOCS_TOKEN\"]\n",
        origin.display().to_string()
    );
    let (settings, _server) = start_with(dir.path(), &more).await;
    let client = Client::new(settings);
    let subject = || git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap();

    let saved = client
        .save_skill("greeting", "Say hello.", "# Greeting\n\nBe kind.")
        .await
        .unwrap();
    assert!(saved.commit.is_some());
    assert_eq!(subject(), "Add the skill greeting");
    let read = client.skill("greeting").await.unwrap();
    assert_eq!(
        (read.description.as_str(), read.text.as_str()),
        ("Say hello.", "# Greeting\n\nBe kind.\n")
    );
    let Err(ClientError::Server { status, .. }) = client.save_skill("moebius", "x", "").await
    else {
        panic!("the built-in skill was changed");
    };
    assert_eq!(status, 409);

    let definition = moebius_desktop::client::McpDefinition {
        command: Some("search".into()),
        args: vec!["--fast".into()],
        env: [("KEY".to_string(), "${DOCS_TOKEN}".to_string())].into(),
        ..Default::default()
    };
    client.save_mcp_server("search", &definition).await.unwrap();
    assert_eq!(subject(), "Add the MCP server search");
    let read = client.mcp_server("search").await.unwrap();
    assert_eq!(read.definition, definition);
    let leaking = moebius_desktop::client::McpDefinition {
        env: [("KEY".to_string(), "${OPENAI_API_KEY}".to_string())].into(),
        ..definition
    };
    let Err(ClientError::Server { status, message }) =
        client.save_mcp_server("search", &leaking).await
    else {
        panic!("the edit was accepted");
    };
    assert_eq!(status, 422);
    assert!(message.contains("is not in mcp_env"), "{message}");
    assert_eq!(subject(), "Add the MCP server search");

    // house-style and docs are named by the Coder.
    let mut coder = client.agent("Coder").await.unwrap().settings.unwrap();
    coder.mcp = vec!["docs".into()];
    client.save_agent("Coder", &coder, "").await.unwrap();
    for result in [
        client.delete_skill("house-style").await,
        client.delete_mcp_server("docs").await,
    ] {
        let Err(ClientError::Server { status, message }) = result else {
            panic!("a named one was deleted");
        };
        assert_eq!(status, 409);
        assert!(message.contains("Coder"), "{message}");
    }
    client.delete_skill("greeting").await.unwrap();
    assert_eq!(subject(), "Delete the skill greeting");
    client.delete_mcp_server("search").await.unwrap();
    assert_eq!(subject(), "Delete the MCP server search");
}

/// An upstream that answers every request with a model list, as plain HTTP.
async fn fake_upstream() -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buffer = [0u8; 4096];
                let _ = stream.read(&mut buffer).await;
                let body = r#"{"data": [{"id": "fake-small"}, {"id": "fake-large"}]}"#;
                let answer = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(answer.as_bytes()).await;
            });
        }
    });
    url
}

#[tokio::test(flavor = "multi_thread")]
async fn the_client_reads_the_models_the_upstreams_serve_from_a_real_server() {
    let dir = tempfile::tempdir().unwrap();
    let upstream = fake_upstream().await;
    let (settings, _server) = start_with(
        dir.path(),
        &format!("[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"{upstream}\"\n"),
    )
    .await;
    let client = Client::new(settings);
    for refresh in [false, true] {
        let available = client.available_models(refresh).await.unwrap();
        let ids: Vec<&str> = available.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["fake-large", "fake-small"]);
        assert_eq!(available.models[0].protocol, "openai");
        assert_eq!(available.models[0].upstream, upstream);
        assert!(available.errors.is_empty());
    }
}

#[tokio::test]
async fn the_client_reads_the_steps_to_a_first_card_of_a_real_server() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _server) = start_with(
        dir.path(),
        "[pi_bolt]\nprogram = \"/no/such/pi-bolt\"\n[agents.Coder]\nmodel = \"coder-model\"\n",
    )
    .await;
    let client = Client::new(settings);
    let start = client.start().await.unwrap();
    assert!(!start.done);
    let keys: Vec<(&str, bool)> = start.steps.iter().map(|s| (&*s.key, s.done)).collect();
    assert_eq!(
        keys,
        [
            ("upstream", false),
            ("pi_bolt", false),
            ("coder", true),
            ("repository", false)
        ]
    );
    assert_eq!(start.steps[0].detail, "Add an upstream on the Models page.");
}

/// The pencil's editor against a real server: it reads the built-in script, saves a script as a commit, shows the
/// server's refusal of a bad one, and gives the script back. The speech bubble starts a draft that is not sent until
/// "Send" is pressed, and the chat then works in the repository.
#[tokio::test(flavor = "multi_thread")]
async fn the_script_editor_reads_saves_refuses_and_gives_back_and_the_bubble_asks_the_floating_chat()
 {
    use iced::widget::text_editor::{Action, Edit};
    use moebius_desktop::script_editor;

    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let client = Client::new(settings.clone());
    let repository = client
        .add_repository(&NewRepository {
            name: "demo".into(),
            url: origin(dir.path()).display().to_string(),
            forge: "none",
            github: None,
            trusted: Vec::new(),
        })
        .await
        .unwrap();
    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));
    let clone = std::path::PathBuf::from(&repository.path);
    let edit = |app: &mut App, text: &str| {
        let _ = app.update(Message::Script(script_editor::Message::Text(
            Action::SelectAll,
        )));
        let paste = Edit::Paste(Arc::new(text.to_string()));
        let _ = app.update(Message::Script(script_editor::Message::Text(Action::Edit(
            paste,
        ))));
    };

    // The pencil of the Ready column reads the built-in script.
    let mut ui = Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1440.0, 900.0),
        app.view(),
    );
    ui.click(iced::widget::Id::from("edit-script-Ready"))
        .unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    let task = app.update(messages.into_iter().next().unwrap());
    settle(&mut app, task).await;
    assert!(shown(&app, ".moebius/ready.luau"));
    assert!(shown(&app, "Built in"));

    // A script that fails the check is refused with the server's message and nothing is committed.
    edit(&mut app, "this is not luau");
    let task = app.update(Message::Script(script_editor::Message::Save));
    settle(&mut app, task).await;
    let refusal = client
        .save_script(repository.id, "Ready", "this is not luau")
        .await
        .unwrap_err();
    let ClientError::Server {
        status: 422,
        message,
    } = refusal
    else {
        panic!("{refusal:?}")
    };
    assert!(shown(&app, &message), "{message}");
    assert!(!clone.join(".moebius").exists());

    // A good one is committed and the editor says the script is the repository's.
    edit(&mut app, "function on_card(card) moebius.wait('held') end");
    let task = app.update(Message::Script(script_editor::Message::Save));
    settle(&mut app, task).await;
    assert!(shown(&app, "Repository"));
    assert!(shown(&app, "Saved as a commit on the default branch."));
    assert_eq!(
        std::fs::read_to_string(clone.join(".moebius/ready.luau")).unwrap(),
        "function on_card(card) moebius.wait('held') end\n"
    );
    assert_eq!(
        client.script(repository.id, "Ready").await.unwrap().source,
        "repository"
    );

    // The bubble opens the floating chat over the editor with a message begun; nothing is sent until the person sends
    // it, which starts a chat in this repository.
    let _ = app.update(Message::Script(script_editor::Message::AskAgent));
    assert_ne!(app.page(), Page::Chat);
    assert!(client.chats().await.unwrap().is_empty());
    let mut ui = Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1440.0, 900.0),
        app.view(),
    );
    ui.click("Send").unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    let task = app.update(messages.into_iter().next().unwrap());
    settle(&mut app, task).await;
    let chats = client.chats().await.unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(
        (chats[0].agent.as_str(), chats[0].repository_id),
        ("Moebius", Some(repository.id))
    );
    assert_eq!(
        chats[0].title,
        "About the Ready column's script (.moebius/ready.luau):"
    );
    // The agent was told where the person is (the fake Pi answers with its prompt); the chat shows only the message.
    let answer = loop {
        let events = client.run_events(chats[0].id, 0).await.unwrap();
        let answered: String = events
            .iter()
            .filter(|event| event.kind == "message_update")
            .filter_map(|event| event.data["assistantMessageEvent"]["delta"].as_str())
            .collect();
        if events.iter().any(|event| event.kind == "agent_end") {
            let said: Vec<_> = events.iter().filter(|event| event.kind == "said").collect();
            assert_eq!(said.len(), 1);
            assert_eq!(
                said[0].data["message"],
                "About the Ready column's script (.moebius/ready.luau):"
            );
            break answered;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    assert!(
        answer.contains("[Where the person is in the Moebius app.")
            && answer.contains("Editing the script of the Ready column (.moebius/ready.luau)")
            && answer.contains("moebius.wait('held')")
            && answer.ends_with("About the Ready column's script (.moebius/ready.luau):"),
        "{answer}"
    );

    // Back in the editor, the built-in script is used again.
    let task = app.update(Message::EditScript(repository.id, "Ready".into()));
    settle(&mut app, task).await;
    let task = app.update(Message::Script(script_editor::Message::UseBuiltIn));
    settle(&mut app, task).await;
    assert!(!clone.join(".moebius/ready.luau").exists());
    assert_eq!(
        client.script(repository.id, "Ready").await.unwrap().source,
        "built-in"
    );
}

/// The New chat form opens an empty draft that makes no request; the first Send creates the chat, which is then the
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
    let task = app.update(Message::Chat(chat::Message::NewChat(true)));
    settle(&mut app, task).await;
    let task = app.update(Message::Chat(chat::Message::Start));
    settle(&mut app, task).await;
    assert!(shown(&app, "Write the first message to start the chat."));
    assert!(client.chats().await.unwrap().is_empty());

    let task = app.update(Message::Chat(chat::Message::Edited("Hello there".into())));
    settle(&mut app, task).await;
    let task = app.update(Message::Chat(chat::Message::Send));
    settle(&mut app, task).await;
    let chats = client.chats().await.unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].agent, "Moebius");
    assert!(!shown(&app, "Write the first message to start the chat."));
    assert!(shown(&app, "Hello there"));
}

/// Opening the Fleet reads the events of every working run, and a run that is stopped leaves it.
#[tokio::test(flavor = "multi_thread")]
async fn opening_the_fleet_reads_the_events_of_the_working_runs_and_a_stopped_run_leaves_it() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let origin = origin(dir.path());
    let repository = post(
        &settings,
        "/v1/repositories",
        json!({"name": "demo", "url": origin.to_str().unwrap()}),
    )
    .await;
    post(
        &settings,
        &format!("/v1/repositories/{}/issues", repository["id"]),
        json!({"title": "Add a feature", "body": "[hang]"}),
    )
    .await;
    let client = Client::new(settings.clone());
    let run = loop {
        let snapshot = client.snapshot().await.unwrap();
        if let Some(run) = snapshot.boards[0].cards[0].run.clone()
            && run.status == "running"
        {
            break run;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };

    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));
    let task = app.update(Message::Select(Page::Fleet));
    settle(&mut app, task).await;
    assert!(shown(&app, "1 agent working"));
    assert!(shown(&app, "#1 Add a feature"));
    // The hanging agent has taken no step, which the tile says once its events are read.
    assert!(shown(&app, "No steps yet."));

    client.stop(run.id).await.unwrap();
    loop {
        let snapshot = client.snapshot().await.unwrap();
        if snapshot.boards[0].cards[0]
            .run
            .as_ref()
            .is_some_and(|run| run.status != "running")
        {
            let _ = app.update(Message::Loaded(1, Ok(snapshot)));
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(shown(&app, "Nothing is working."));
}
