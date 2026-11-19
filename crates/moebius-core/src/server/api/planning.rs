//! Priorities and due dates of cards.
//!
//! - `PUT /v1/cards/{id}/priority` `{priority}`: `urgent`, `high`, `medium`, `low` or `none`; answers the `Card`.
//! - `PUT /v1/cards/{id}/due` `{due}`: a date as `YYYY-MM-DD`, or null for none; answers the `Card`.
//!
//! Both are kept as labels of the card (`priority:high`, `due:2026-12-01`), so they are written to the forge and
//! announced as `card_labels_changed`. A person does this, never an agent.

use axum::extract::{Path as UrlPath, State};
use axum::routing::put;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking};
use crate::store::Card;

/// The routes of priorities and due dates.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/cards/{id}/priority", put(set_priority))
        .route("/v1/cards/{id}/due", put(set_due))
}

/// The body of `PUT /v1/cards/{id}/priority`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewPriority {
    pub priority: String,
}

/// The body of `PUT /v1/cards/{id}/due`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewDue {
    pub due: Option<String>,
}

async fn set_priority(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewPriority>,
) -> Result<Json<Card>, ApiError> {
    blocking(&state.app, move |app| app.set_priority(id, &new.priority)).await
}

async fn set_due(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewDue>,
) -> Result<Json<Card>, ApiError> {
    blocking(&state.app, move |app| app.set_due(id, new.due.as_deref())).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::server::api::testing::start;
    use crate::store::Priority;

    #[tokio::test]
    async fn a_person_sets_and_clears_the_priority_and_the_due_date_of_a_card() {
        let (_dir, state, addr) = start().await;
        let repo = state
            .app
            .store
            .add_repository("r", "/r", "none", "main")
            .unwrap();
        let card = state
            .app
            .store
            .add_card(repo.id, "t", "b", "person", "Not started")
            .unwrap();
        let put = |path: String, body: serde_json::Value| {
            let url = format!("http://{addr}{path}");
            let token = state.token.clone();
            async move {
                reqwest::Client::new()
                    .put(url)
                    .bearer_auth(&*token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap()
            }
        };
        let mut events = state.app.bus.subscribe();

        let path = format!("/v1/cards/{}/priority", card.id);
        let answer = put(path.clone(), json!({"priority": "Urgent"})).await;
        assert_eq!(answer.status(), 200);
        let body: serde_json::Value = answer.json().await.unwrap();
        assert_eq!(body["priority"], "urgent");
        assert_eq!(body["labels"], json!(["priority:urgent"]));
        assert_eq!(events.recv().await.unwrap().kind, "card_labels_changed");

        let path_due = format!("/v1/cards/{}/due", card.id);
        let answer = put(path_due.clone(), json!({"due": "2026-12-01"})).await;
        assert_eq!(answer.status(), 200);
        let got = answer.json::<Card>().await.unwrap();
        assert_eq!(got.due.as_deref(), Some("2026-12-01"));
        assert_eq!(got.priority, Priority::Urgent);

        let answer = put(path_due.clone(), json!({"due": null})).await;
        assert_eq!(answer.json::<Card>().await.unwrap().due, None);
        let answer = put(path, json!({"priority": "none"})).await;
        let got = answer.json::<Card>().await.unwrap();
        assert_eq!(got.priority, Priority::None);
        assert!(got.labels.is_empty());

        for (path, body) in [
            (
                format!("/v1/cards/{}/priority", card.id),
                json!({"priority": "asap"}),
            ),
            (path_due, json!({"due": "2026-02-30"})),
            (
                "/v1/cards/9999/priority".to_string(),
                json!({"priority": "low"}),
            ),
            ("/v1/cards/9999/due".to_string(), json!({"due": null})),
        ] {
            let answer = put(path.clone(), body).await;
            assert!(
                answer.status().is_client_error(),
                "{path}: {}",
                answer.status()
            );
        }
    }
}
