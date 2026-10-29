//! The event stream: every [`AppEvent`] the core publishes on its bus, as it happens.
//!
//! - `GET /v1/events`: a WebSocket on which every event arrives as one JSON text message; one that a slow client
//!   missed is answered with a `lagged` event that says how many.

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use tokio::sync::broadcast::error::RecvError;

use super::ApiState;
use crate::bus::AppEvent;

/// The route of the event stream.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/events", get(events))
}

async fn events(State(state): State<ApiState>, upgrade: WebSocketUpgrade) -> Response {
    let rx = state.app.bus.subscribe();
    upgrade.on_upgrade(move |socket| stream(socket, rx))
}

async fn stream(mut socket: WebSocket, mut rx: tokio::sync::broadcast::Receiver<AppEvent>) {
    loop {
        tokio::select! {
            event = rx.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(RecvError::Lagged(missed)) => AppEvent::new("lagged", serde_json::json!({ "missed": missed })),
                    Err(RecvError::Closed) => return,
                };
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
        let event = AppEvent::new("card_moved", serde_json::json!({"card": 1, "to": "Ready"}));
        let received = loop {
            state.app.bus.publish(event.clone());
            if let Ok(Some(message)) =
                tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await
            {
                break message.unwrap();
            }
        };
        let back: AppEvent = serde_json::from_str(received.to_text().unwrap()).unwrap();
        assert_eq!(back, event);

        let mut wrong = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        wrong
            .headers_mut()
            .insert("authorization", "Bearer nope".parse().unwrap());
        assert!(tokio_tungstenite::connect_async(wrong).await.is_err());
    }
}
