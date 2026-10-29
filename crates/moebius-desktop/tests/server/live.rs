//! Runs against a real server: the live view that loads, follows, steers and stops a run, the shell in its worktree,
//! the Terminal page's shell, and the Fleet.

use std::time::Duration;

use futures_util::{Stream, StreamExt};
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{
    Client, ClientError, RunEvent, Snapshot, StreamEvent, TerminalEvent, events, terminal,
};
use moebius_desktop::widgets::conversation::{Conversation, Entry, Said};
use moebius_desktop::widgets::terminal::Shell;
use serde_json::json;

use crate::fixtures::{add_demo, next, post, settle, shown, start, start_with_fake_pi};

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
    let repository = add_demo(&settings, dir.path()).await;
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
    let repository = add_demo(&settings, dir.path()).await;
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

/// Opening the Fleet reads the events of every working run, and a run that is stopped leaves it.
#[tokio::test(flavor = "multi_thread")]
async fn opening_the_fleet_reads_the_events_of_the_working_runs_and_a_stopped_run_leaves_it() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let repository = add_demo(&settings, dir.path()).await;
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
