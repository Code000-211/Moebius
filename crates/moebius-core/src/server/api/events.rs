//! The event stream: every [`AppEvent`] the core publishes on its bus, as it happens.
//!
//! - `GET /v1/events`: a WebSocket on which every event arrives as one JSON text message; one that a slow client
//!   missed is answered with a `lagged` event that says how many.

use axum::Extension;
use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use tokio::sync::broadcast::error::RecvError;

use super::ApiState;
use crate::bus::{AppEvent, Event};
use crate::decide::Actor;

/// The route of the event stream.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/events", get(events))
}

async fn events(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let rx = state.app.bus.subscribe();
    let app = state.app.clone();
    upgrade.on_upgrade(move |socket| stream(socket, rx, app, by))
}

/// The events about a run that carry what a chat says: another person's chat that is not shared is not sent to
/// them (see [`crate::app::chat_access`]).
const OF_A_RUN: [&str; 4] = ["run_started", "run_event", "run_ended", "chat_renamed"];

/// Whether `by` may be told of `event`.
fn visible(app: &crate::app::App, by: &Actor, event: &AppEvent) -> bool {
    if !OF_A_RUN.contains(&event.kind.as_str()) {
        return true;
    }
    let Some(run) = event.data["run"].as_i64() else {
        return true;
    };
    match app.store.run(run) {
        Ok(Some(run)) => crate::app::may_use(by, &run, false),
        _ => true,
    }
}

async fn stream(
    mut socket: WebSocket,
    mut rx: tokio::sync::broadcast::Receiver<AppEvent>,
    app: std::sync::Arc<crate::app::App>,
    by: Actor,
) {
    loop {
        tokio::select! {
            event = rx.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(RecvError::Lagged(missed)) => AppEvent::from(Event::Lagged { missed }),
                    Err(RecvError::Closed) => return,
                };
                if !visible(&app, &by, &event) {
                    continue;
                }
                let Ok(text) = serde_json::to_string(&event) else { continue };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    return;
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    use super::*;
    use crate::server::api::testing::start;

    #[tokio::test]
    async fn the_events_of_a_chat_that_is_not_shared_do_not_reach_another_person() {
        let (_dir, state, _addr) = start().await;
        let store = &state.app.store;
        let lea = store
            .add_person("lea", "Lea", crate::store::Role::Member)
            .unwrap();
        let ben = store
            .add_person("ben", "Ben", crate::store::Role::Member)
            .unwrap();
        let run = store.add_run(None, "Moebius", "fake", "chat").unwrap();
        store.set_chat_owner(run.id, lea.id).unwrap();
        let of = |kind: Event| AppEvent::from(kind);
        let said = of(Event::RunEvent {
            run: run.id,
            seq: 1,
            kind: "said".into(),
            data: serde_json::json!({"message": "secret"}),
        });
        let (lea, ben) = (Actor::person(lea.id, "lea"), Actor::person(ben.id, "ben"));
        assert!(visible(&state.app, &lea, &said));
        assert!(!visible(&state.app, &ben, &said));
        store.set_chat_shared(run.id, true).unwrap();
        assert!(visible(&state.app, &ben, &said));
        store.set_chat_shared(run.id, false).unwrap();
        // What carries no chat's words goes to everyone, so that every list is read again.
        let shared = of(Event::ChatShared {
            run: run.id,
            shared: false,
        });
        assert!(visible(&state.app, &ben, &shared));
        assert!(!visible(
            &state.app,
            &ben,
            &of(Event::ChatRenamed {
                run: run.id,
                title: "t".into()
            })
        ));
    }

    #[tokio::test]
    async fn events_published_by_the_core_reach_the_client() {
        let (_dir, state, addr) = start().await;
        let mut request = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", state.token).parse().unwrap(),
        );
        let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        // The subscription exists once the upgrade is done; publish until the first event arrives.
        let event = Event::CardMoved {
            card: 1,
            from: "Backlog".into(),
            to: "Not started".into(),
        };
        let received = loop {
            state.app.bus.publish(event.clone());
            if let Ok(Some(message)) =
                tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await
            {
                break message.unwrap();
            }
        };
        let back: AppEvent = serde_json::from_str(received.to_text().unwrap()).unwrap();
        assert_eq!(back, AppEvent::from(event));

        let mut wrong = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        wrong
            .headers_mut()
            .insert("authorization", "Bearer nope".parse().unwrap());
        assert!(tokio_tungstenite::connect_async(wrong).await.is_err());
    }
}
