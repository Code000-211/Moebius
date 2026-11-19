//! The notices of a person on a fake server: one event gives exactly one notification, and reading a notice lowers the
//! count.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use moebius_desktop::notices::{Effect, watch};
use moebius_desktop::server::Server;
use serde_json::{Value, json};
use tokio::sync::broadcast;

fn notice(id: u64, read: bool) -> Value {
    json!({"id": id, "kind": "assigned", "by": "tom", "card_id": 7, "inbox_item_id": null,
        "title": "Card seven", "body": "", "at": 5, "read": read})
}

struct Fake {
    events: broadcast::Sender<String>,
}

/// A server that has one unread notice, answers the stream with what `events` is sent, and wants the token `secret`.
async fn serve() -> (String, Arc<Fake>) {
    let fake = Arc::new(Fake {
        events: broadcast::channel(16).0,
    });
    async fn notices(headers: axum::http::HeaderMap) -> axum::response::Response {
        if headers["authorization"] != "Bearer secret" {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        axum::Json(json!([notice(1, false)])).into_response()
    }
    async fn events(
        State(fake): State<Arc<Fake>>,
        headers: axum::http::HeaderMap,
        upgrade: WebSocketUpgrade,
    ) -> axum::response::Response {
        if headers["authorization"] != "Bearer secret" {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        let mut events = fake.events.subscribe();
        upgrade.on_upgrade(move |mut socket| async move {
            while let Ok(event) = events.recv().await {
                if socket.send(Message::text(event)).await.is_err() {
                    break;
                }
            }
        })
    }
    let router = Router::new()
        .route("/v1/notices", get(notices))
        .route("/v1/events", get(events))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await });
    (base, fake)
}

fn event(kind: &str, notice: Value) -> String {
    json!({"kind": kind, "data": {"person": "lea", "notice": notice}}).to_string()
}

async fn next(effects: &mut tokio::sync::mpsc::UnboundedReceiver<Effect>) -> Effect {
    tokio::time::timeout(Duration::from_secs(60), effects.recv())
        .await
        .expect("an effect comes")
        .expect("the watch is running")
}

#[tokio::test]
async fn an_event_gives_one_notification_and_reading_lowers_the_count() {
    let (base, fake) = serve().await;
    let (_watch, mut effects) = watch(Server::new(&base, "secret"));
    // The notice that was there before is the baseline: counted, not announced.
    assert_eq!(next(&mut effects).await, Effect::Count(1));

    // The stream is subscribed once the baseline is read, as the first send may otherwise have no listener.
    while fake.events.receiver_count() == 0 {
        tokio::task::yield_now().await;
    }
    let added = notice(2, false);
    fake.events
        .send(event("notice_added", added.clone()))
        .unwrap();
    fake.events.send(event("notice_added", added)).unwrap();
    let Effect::Notify(notified) = next(&mut effects).await else {
        panic!("a notification comes first");
    };
    assert_eq!((notified.id, notified.title.as_str()), (2, "Card seven"));
    assert_eq!(next(&mut effects).await, Effect::Count(2));

    fake.events
        .send(event("notice_read", notice(2, true)))
        .unwrap();
    assert_eq!(next(&mut effects).await, Effect::Count(1));
    // The second `notice_added` of the same notice gave nothing: the next effect is of what follows.
    fake.events
        .send(event("notice_read", notice(1, true)))
        .unwrap();
    assert_eq!(next(&mut effects).await, Effect::Count(0));
}

#[tokio::test]
async fn a_server_that_does_not_accept_the_token_gives_nothing() {
    let (base, _fake) = serve().await;
    let (_watch, mut effects) = watch(Server::new(&base, "wrong"));
    assert!(
        tokio::time::timeout(Duration::from_millis(800), effects.recv())
            .await
            .is_err()
    );
}
