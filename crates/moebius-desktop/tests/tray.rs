//! The tray's figures against a real server on port 0: they follow its events, and pausing from the tray pauses the
//! server (what pausing holds is tested with the server, in `moebius-core`).

use std::path::Path;
use std::time::Duration;

use moebius_core::bus::Event;
use moebius_core::config::Config;
use moebius_core::server::Server as MoebiusServer;
use moebius_core::server::api::{self, ApiState};
use moebius_core::store::Usage;
use moebius_desktop::server::Server;
use moebius_desktop::tray::{self, Figures};
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// A real server whose data is in `dir`, on port 0, signed in to with a person's access token.
async fn serve(dir: &Path) -> (MoebiusServer, Server) {
    let toml = "";
    let file = dir.join("moebius.toml");
    std::fs::write(&file, toml).unwrap();
    let config = Config::parse(toml, dir, &file).unwrap();
    let server = MoebiusServer::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
    let state = ApiState {
        codes: Default::default(),
        app: server.app.clone(),
        runs: server.harness.clone(),
        harness_repo: None,
        token: server.token.clone(),
        router: server.router.clone(),
        config: server.live.clone(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(api::serve(listener, state));
    let issued: Value = reqwest::Client::new()
        .post(format!("{base}/v1/tokens"))
        .bearer_auth(&*server.token)
        .json(&json!({ "name": "desktop" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let signed_in = Server::new(&base, issued["token"].as_str().unwrap());
    (server, signed_in)
}

/// Follows `server` and sends what the tray would show to the receiver.
fn following(server: &Server) -> UnboundedReceiver<Option<Figures>> {
    let (sender, receiver) = unbounded_channel();
    tokio::spawn(tray::follow(server.clone(), move |figures| {
        let _ = sender.send(figures);
    }));
    receiver
}

/// The next figures that satisfy `wanted`; the tray shows each reading it is told.
async fn until(
    shown: &mut UnboundedReceiver<Option<Figures>>,
    wanted: impl Fn(&Figures) -> bool,
) -> Figures {
    loop {
        let figures = tokio::time::timeout(Duration::from_secs(30), shown.recv())
            .await
            .expect("the figures never came")
            .expect("the tray stopped following");
        if let Some(figures) = figures.filter(|f| wanted(f)) {
            return figures;
        }
    }
}

#[tokio::test]
async fn the_figures_follow_the_events_of_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let (moebius, server) = serve(dir.path()).await;
    let mut shown = following(&server);
    assert_eq!(until(&mut shown, |_| true).await, Figures::default());

    // Something enters the inbox and the model is used: the next event makes the tray read them.
    let app = &moebius.app;
    app.store
        .add_inbox_item("stuck", None, "Needs a person", &json!({}))
        .unwrap();
    app.store
        .add_usage(&Usage {
            run_id: None,
            model: "m".into(),
            input_tokens: 1000,
            output_tokens: 500,
            cached_tokens: 0,
            duration_ms: 10,
            tools: Vec::new(),
        })
        .unwrap();
    app.bus.publish(Event::RunsResumed {});
    let figures = until(&mut shown, |f| f.inbox == 1).await;
    assert_eq!((figures.tokens, figures.budget), (1500, None));
    assert_eq!(figures.lines().inbox, "Inbox: 1");
    assert_eq!(figures.lines().tokens, "Tokens today: 1.5k");
}

#[tokio::test]
async fn pausing_from_the_tray_pauses_the_server_and_resuming_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let (moebius, server) = serve(dir.path()).await;
    let mut shown = following(&server);
    until(&mut shown, |f| !f.paused).await;

    tray::set_paused(&server, true).await.unwrap();
    assert!(moebius.app.runs_paused().unwrap().is_some());
    let paused = until(&mut shown, |f| f.paused).await;
    assert_eq!(paused.lines().pause, "Resume all agents");
    assert_eq!(tray::read(&server).await.unwrap(), paused);

    tray::set_paused(&server, false).await.unwrap();
    assert!(moebius.app.runs_paused().unwrap().is_none());
    let resumed = until(&mut shown, |f| !f.paused).await;
    assert_eq!(resumed.lines().pause, "Pause all agents");
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_shows_no_figures_and_is_no_error_of_the_tray() {
    // Nothing listens on port 0; the port of a listener that was dropped may be given to another test that answers.
    let gone = Server::new("http://127.0.0.1:0", "token");
    let mut shown = following(&gone);
    assert_eq!(shown.recv().await, Some(None));
    assert!(matches!(
        tray::read(&gone).await,
        Err(tray::Error::Unreachable(_))
    ));
    assert!(tray::set_paused(&gone, true).await.is_err());
}

/// A fake server that counts what the tray reads: `(status reads, snapshot reads)`, and sends to the event stream
/// whatever the test sends to the returned sender.
async fn counting() -> (
    Server,
    std::sync::Arc<(
        std::sync::atomic::AtomicUsize,
        std::sync::atomic::AtomicUsize,
    )>,
    tokio::sync::mpsc::UnboundedSender<String>,
) {
    use axum::extract::ws::{Message, WebSocketUpgrade};
    use axum::routing::get;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let counts = std::sync::Arc::new((AtomicUsize::new(0), AtomicUsize::new(0)));
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    let receiver = std::sync::Arc::new(tokio::sync::Mutex::new(receiver));
    let status = counts.clone();
    let snapshot = counts.clone();
    let app = axum::Router::new()
        .route(
            "/v1/status",
            get(move || async move {
                status.0.fetch_add(1, Ordering::SeqCst);
                axum::Json(
                    json!({"working": 0, "inbox": 0, "waiting_notices": 0, "tokens": 0,
                                  "day_budget": null, "paused": false}),
                )
            }),
        )
        .route(
            "/v1/snapshot",
            get(move || async move {
                snapshot.1.fetch_add(1, Ordering::SeqCst);
                axum::Json(json!({}))
            }),
        )
        .route(
            "/v1/events",
            get(move |upgrade: WebSocketUpgrade| async move {
                upgrade.on_upgrade(move |mut socket| async move {
                    let mut receiver = receiver.lock().await;
                    while let Some(text) = receiver.recv().await {
                        if socket.send(Message::text(text)).await.is_err() {
                            break;
                        }
                    }
                })
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (Server::new(&base, "token"), counts, sender)
}

#[tokio::test]
async fn the_steps_of_runs_do_not_make_the_tray_read_but_other_events_do_and_the_snapshot_is_never_read()
 {
    use std::sync::atomic::Ordering;
    let (server, counts, events) = counting().await;
    let mut shown = following(&server);
    until(&mut shown, |_| true).await;
    assert_eq!(counts.0.load(Ordering::SeqCst), 1);

    // A run taking many steps is read no more than once more, and not before its minimum wait.
    let step = json!({"kind": "run_event", "data": {}}).to_string();
    for _ in 0..200 {
        events.send(step.clone()).unwrap();
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        counts.0.load(Ordering::SeqCst),
        1,
        "a step is no reason to read now"
    );

    // An event that changes what the tray shows is read within about a second, steps or not.
    events
        .send(json!({"kind": "runs_paused", "data": {}}).to_string())
        .unwrap();
    until(&mut shown, |_| true).await;
    assert_eq!(counts.0.load(Ordering::SeqCst), 2);
    assert_eq!(
        counts.1.load(Ordering::SeqCst),
        0,
        "the snapshot is not the tray's to read"
    );
}
