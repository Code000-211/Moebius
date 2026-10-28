//! The client against a real server (`moebius-core` as a dev-dependency): it reads the snapshot with the server's
//! token, receives an event on the stream, and is told when the token is wrong or the server cannot be reached.
#![cfg(unix)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::store::Usage;
use moebius_desktop::client::{
    Client, ClientError, RunEvent, StreamEvent, TerminalEvent, events, terminal,
};
use moebius_desktop::conversation::{Conversation, Entry, Said};
use moebius_desktop::settings::Settings;
use moebius_desktop::terminal::Shell;
use serde_json::json;

/// A server with no agents on a free port; returns the settings that connect to it and the server.
async fn start(dir: &Path) -> (Settings, Arc<Server>) {
    let config = Config::parse(
        "[router]\nlisten = \"127.0.0.1:0\"\n",
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
    let mut stream = Box::pin(terminal(settings.clone(), run.id));
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
    let mut stream = Box::pin(terminal(settings, run.id));
    match stream.next().await.unwrap() {
        TerminalEvent::Closed(reason) => assert!(reason.contains("422"), "{reason}"),
        other => panic!("{other:?}"),
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
