//! The client and the server: the snapshot read with the server's token, an event on the stream, a wrong token, a
//! server that is not there, and a zip stored on the server.

use moebius_core::store::Usage;
use moebius_desktop::client::{Client, ClientError, StreamEvent, events};
use moebius_desktop::settings::Settings;
use serde_json::json;

use crate::fixtures::{add_demo, next, post, start};

#[tokio::test(flavor = "multi_thread")]
async fn the_client_reads_the_snapshot_and_follows_the_events() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start(dir.path()).await;
    let repository = add_demo(&settings, dir.path()).await;

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
    let repository = add_demo(&settings, dir.path()).await;
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

#[tokio::test(flavor = "multi_thread")]
async fn the_app_reports_where_it_runs_when_it_connects_and_again_when_that_changes() {
    use crate::fixtures::settle;
    use moebius_desktop::app::{App, Message};
    use moebius_desktop::client::Snapshot;

    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start(dir.path()).await;
    assert!(server.app.client_info().is_none());

    let client = Client::new(settings.clone());
    let mut app = App::with_snapshot(Snapshot::default());
    let task = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));
    settle(&mut app, task).await;
    let reported = server.app.client_info().expect("the app reported itself");
    assert_eq!(reported.server_address, settings.address);
    assert_eq!(reported.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(reported.os, std::env::consts::OS);
    assert_eq!(reported.ssh_host, None);
    assert!(!reported.program.is_empty());
    assert!(!reported.host.is_empty());
    // No token in what the server keeps or tells the agent.
    assert!(!format!("{reported:?}").contains(&settings.token));
    assert!(!server.app.where_moebius_runs().contains(&settings.token));
    assert!(server.app.where_moebius_runs().contains(&reported.host));

    // The same report, read back through the route; a later one replaces it.
    assert_eq!(
        client.reported_client().await.unwrap().server_address,
        settings.address
    );
    let mut over_ssh =
        moebius_desktop::app::report::this_app("http://127.0.0.1:50123", Some("build-box"));
    over_ssh.host = "laptop".into();
    client.report_client(&over_ssh).await.unwrap();
    let latest = server.app.client_info().unwrap();
    assert_eq!(latest.server_address, "http://127.0.0.1:50123");
    assert_eq!(latest.ssh_host.as_deref(), Some("build-box"));
    assert!(
        server
            .app
            .where_moebius_runs()
            .contains("over SSH through build-box")
    );
}
