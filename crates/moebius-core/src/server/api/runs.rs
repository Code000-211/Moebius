//! The runs: one run and its stored events, and what a person does to a run that is going on.
//!
//! - `GET /v1/runs/{id}`: a run. `GET /v1/runs/{id}/events?after=&limit=`: its stored events, in pages.
//! - `POST /v1/runs/{id}/steer` `{message}`: a person sends a message to a running run.
//! - `POST /v1/runs/{id}/stop` `{reason}`: a person stops a running run; a chat ends as stopped.

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, Reason, blocking};
use crate::decide::Actor;
use crate::store::{Event, Run};

/// The routes of the runs.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/runs/{id}", get(get_run))
        .route("/v1/runs/{id}/events", get(run_events))
        .route("/v1/runs/{id}/steer", post(steer_run))
        .route("/v1/runs/{id}/stop", post(stop_run))
}

/// The body of a person's message to a running run or a chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Steer {
    pub message: String,
}

/// How many events of a run one request returns unless it asks for fewer or more.
const DEFAULT_EVENTS: i64 = 500;
/// The most events of a run one request returns.
const MAX_EVENTS: i64 = 5000;

/// Which of a run's events to return: those after `after`, at most `limit`.
#[derive(Debug, Deserialize)]
struct EventsQuery {
    #[serde(default)]
    after: i64,
    limit: Option<i64>,
}

async fn steer_run(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(steer): axum::Json<Steer>,
) -> Result<axum::Json<Run>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.steer_run(&by, &*runs, id, &steer.message)
    })
    .await
}

async fn stop_run(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(stop): axum::Json<Reason>,
) -> Result<axum::Json<Run>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.stop_run(&by, &*runs, id, &stop.reason)
    })
    .await
}

async fn get_run(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Run>, ApiError> {
    blocking(&state.app, move |app| app.run(id)).await
}

async fn run_events(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Query(query): Query<EventsQuery>,
) -> Result<axum::Json<Vec<Event>>, ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_EVENTS).clamp(1, MAX_EVENTS);
    blocking(&state.app, move |app| {
        app.run(id)?;
        Ok(app.store.events_after(id, query.after, limit)?)
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    use super::*;
    use crate::app::PERSON;
    use crate::board::{Engine, Limits, Workflow};
    use crate::bus::AppEvent;
    use crate::server::api::testing::{get, origin, post, start, until};

    #[tokio::test]
    async fn a_person_steers_and_stops_a_run_and_its_card_waits_in_the_inbox() {
        let (dir, state, addr) = start().await;
        let app = state.app.clone();
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let card = app
            .create_issue(repo.id, "Please [hang]", "", PERSON)
            .unwrap();
        let engine = Arc::new(Engine::new(
            app.clone(),
            state.runs.clone(),
            Workflow::default(),
            Limits::default(),
        ));
        let settle = || {
            let engine = engine.clone();
            async move {
                tokio::task::spawn_blocking(move || engine.settle(card.id).unwrap())
                    .await
                    .unwrap()
            }
        };
        let events = |run: i64| app.store.events(run).unwrap();

        // The board starts the Coder, which waits for a person.
        settle().await;
        let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        until("Pi takes the prompt", || events(run.id).len() >= 2).await;
        let steer = |message: &str| {
            post(
                &state,
                &addr,
                format!("/v1/runs/{}/steer", run.id),
                serde_json::json!({ "message": message }),
            )
        };
        assert_eq!(steer("  ").await.status(), 422);
        let steered: Run = steer("Use tabs.").await.json().await.unwrap();
        assert_eq!(steered.status, "running");
        until("Pi answers the steer", || {
            events(run.id)
                .iter()
                .any(|e| e.kind == "response" && e.data["echo"] == "Use tabs.")
        })
        .await;
        let steered = events(run.id).into_iter().find(|e| e.kind == "steered");
        assert_eq!(
            steered.unwrap().data,
            serde_json::json!({ "message": "Use tabs." })
        );

        let stop = |id: i64| {
            post(
                &state,
                &addr,
                format!("/v1/runs/{id}/stop"),
                serde_json::json!({ "reason": "wrong direction" }),
            )
        };
        assert_eq!(stop(run.id).await.status(), 200);
        until("the run ends", || {
            app.store.run(run.id).unwrap().unwrap().status == "stopped"
        })
        .await;

        // The card is not retried by itself; it waits in the inbox for a person.
        let step = settle().await;
        assert_eq!(step.waiting.as_deref(), Some("stopped by a person"));
        settle().await;
        assert_eq!(app.store.runs_for_card(card.id).unwrap().len(), 1);
        let inbox = app.store.inbox().unwrap();
        assert_eq!((inbox.len(), inbox[0].kind.as_str()), (1, "stuck"));

        // What a person did is recorded, and neither applies to a run that is over.
        for kind in ["steer", "stop"] {
            let judgements = app.store.judgements(kind).unwrap();
            assert_eq!(
                (
                    judgements.len(),
                    judgements[0].decider.as_str(),
                    judgements[0].run_id
                ),
                (1, "person", Some(run.id))
            );
        }
        assert_eq!(
            app.store.judgements("stop").unwrap()[0].reason,
            "wrong direction"
        );
        assert_eq!(steer("Too late.").await.status(), 422);
        assert_eq!(stop(run.id).await.status(), 422);
        assert_eq!(stop(99).await.status(), 404);
    }

    async fn connect(state: &ApiState, addr: &str) -> impl StreamExt<Item = AppEvent> + Unpin {
        let mut request = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", state.token).parse().unwrap(),
        );
        let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        ws.map(|message| serde_json::from_str(message.unwrap().to_text().unwrap()).unwrap())
    }

    #[tokio::test]
    async fn a_finished_runs_events_are_read_in_pages() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let run = app.store.add_run(Some(1), "Coder", "test", "code").unwrap();
        for n in 1..=7 {
            app.record_run_event(run.id, "step", &serde_json::json!({ "n": n }))
                .unwrap();
        }
        app.store.set_run_status(run.id, "succeeded").unwrap();

        let stored: Run = get(&state, &addr, &format!("/v1/runs/{}", run.id)).await;
        assert_eq!(stored.status, "succeeded");
        assert!(stored.ended_at.is_some());
        let all: Vec<Event> = get(&state, &addr, &format!("/v1/runs/{}/events", run.id)).await;
        assert_eq!(
            all.iter().map(|e| e.seq).collect::<Vec<_>>(),
            (1..=7).collect::<Vec<_>>()
        );
        assert_eq!(
            (all[2].kind.as_str(), &all[2].data),
            ("step", &serde_json::json!({ "n": 3 }))
        );

        // Pages of three follow on from the last seq without gaps or repeats.
        let mut paged = Vec::new();
        loop {
            let after = paged.last().map_or(0, |e: &Event| e.seq);
            let page: Vec<Event> = get(
                &state,
                &addr,
                &format!("/v1/runs/{}/events?after={after}&limit=3", run.id),
            )
            .await;
            assert!(page.len() <= 3);
            if page.is_empty() {
                break;
            }
            paged.extend(page);
        }
        assert_eq!(paged, all);
    }

    #[tokio::test]
    async fn the_events_of_a_running_run_arrive_on_the_stream_with_their_seq() {
        let (dir, state, addr) = start().await;
        let app = state.app.clone();
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let card = app
            .create_issue(repo.id, "Please [hang]", "", PERSON)
            .unwrap();
        let engine = Arc::new(Engine::new(
            app.clone(),
            state.runs.clone(),
            Workflow::default(),
            Limits::default(),
        ));
        let id = card.id;
        tokio::task::spawn_blocking(move || engine.settle(id).unwrap())
            .await
            .unwrap();
        let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        until("Pi takes the prompt", || {
            app.store.events(run.id).unwrap().len() >= 2
        })
        .await;

        // A client follows the stream, reads what is stored so far, and drops what it has already read.
        let mut stream = connect(&state, &addr).await;
        let path = format!("/v1/runs/{}/events", run.id);
        let mut seen: Vec<Event> = get(&state, &addr, &path).await;
        let read = seen.last().unwrap().seq;
        let steered = post(
            &state,
            &addr,
            format!("/v1/runs/{}/steer", run.id),
            serde_json::json!({ "message": "Use tabs." }),
        )
        .await;
        assert_eq!(steered.status(), 200);
        // Pi's answer may be stored just before or just after the steer itself.
        let steer_arrived = |seen: &[Event]| {
            let after_read = || seen.iter().filter(|e| e.seq > read);
            after_read().any(|e| e.kind == "steered")
                && after_read().any(|e| e.data["echo"] == "Use tabs.")
        };
        while !steer_arrived(&seen) {
            let event = tokio::time::timeout(std::time::Duration::from_secs(20), stream.next())
                .await
                .expect("a run event arrives")
                .unwrap();
            if event.kind == "run_event" && event.data["run"] == run.id {
                let seq = event.data["seq"].as_i64().unwrap();
                if seq > read {
                    seen.push(Event {
                        run_id: run.id,
                        seq,
                        kind: event.data["kind"].as_str().unwrap().into(),
                        data: event.data["data"].clone(),
                        at: 0,
                    });
                }
            }
        }

        // Each event is there exactly once, and what was stored is what the client saw.
        let stored: Vec<Event> = get(&state, &addr, &path).await;
        assert_eq!(
            stored
                .iter()
                .map(|e| (e.seq, &e.kind, &e.data))
                .collect::<Vec<_>>(),
            seen.iter()
                .map(|e| (e.seq, &e.kind, &e.data))
                .collect::<Vec<_>>()
        );
        assert!(stored.windows(2).all(|w| w[0].seq + 1 == w[1].seq));

        let stopped = post(
            &state,
            &addr,
            format!("/v1/runs/{}/stop", run.id),
            serde_json::json!({}),
        )
        .await;
        assert_eq!(stopped.status(), 200);
        until("the run ends", || {
            app.store.run(run.id).unwrap().unwrap().status == "stopped"
        })
        .await;
    }

    #[tokio::test]
    async fn an_unknown_run_or_card_is_not_found() {
        let (_dir, state, addr) = start().await;
        for path in [
            "/v1/runs/99",
            "/v1/runs/99/events",
            "/v1/cards/99/runs",
            "/v1/cards/99/history",
        ] {
            let response = reqwest::Client::new()
                .get(format!("http://{addr}{path}"))
                .bearer_auth(&*state.token)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 404, "{path}");
        }
    }
}
