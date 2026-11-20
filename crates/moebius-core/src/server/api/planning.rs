//! Priorities, due dates and estimates of cards.
//!
//! - `PUT /v1/cards/{id}/priority` `{priority}`: `urgent`, `high`, `medium`, `low` or `none`; answers the `Card`.
//! - `PUT /v1/cards/{id}/due` `{due}`: a date as `YYYY-MM-DD`, or null for none; answers the `Card`.
//! - `PUT /v1/cards/{id}/estimate` `{estimate}`: points, a whole number from 1 to 1000, or null for none; answers the
//!   `Card`.
//! - `GET /v1/cards/{id}/estimate/proposal`: the estimate the Moebius agent proposes, from the cards that look like
//!   this one and have an estimate, with the cards it compared. Nothing is set by it.
//!
//! All are kept as labels of the card (`priority:high`, `due:2026-12-01`, `estimate:5`), so they are written to the forge and
//! announced as `card_labels_changed`. A person does this, never an agent.

use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking};
use crate::app::{AppError, EstimateProposal};
use crate::store::{Card, ESTIMATE_LIMIT};

/// The routes of priorities, due dates and estimates.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/cards/{id}/priority", put(set_priority))
        .route("/v1/cards/{id}/due", put(set_due))
        .route("/v1/cards/{id}/estimate", put(set_estimate))
        .route("/v1/cards/{id}/estimate/proposal", get(propose_estimate))
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

/// The body of `PUT /v1/cards/{id}/estimate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewEstimate {
    /// Any number, so that a negative or fractional one reaches the check that says what an estimate is.
    pub estimate: Option<serde_json::Number>,
}

async fn set_estimate(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewEstimate>,
) -> Result<Json<Card>, ApiError> {
    let points = match new.estimate {
        None => None,
        Some(number) => match number.as_u64().and_then(|n| u32::try_from(n).ok()) {
            Some(points) => Some(points),
            None => {
                return Err(AppError::Invalid(format!(
                    "{number} is not an estimate (1 to {ESTIMATE_LIMIT} points)"
                ))
                .into());
            }
        },
    };
    blocking(&state.app, move |app| app.set_estimate(id, points)).await
}

async fn propose_estimate(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<EstimateProposal>, ApiError> {
    blocking(&state.app, move |app| app.propose_estimate(id)).await
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

    #[tokio::test]
    async fn a_person_sets_an_estimate_and_asks_for_a_proposal_that_names_the_cards_compared() {
        let (_dir, state, addr) = start().await;
        let repo = state
            .app
            .store
            .add_repository("r", "/r", "none", "main")
            .unwrap();
        let add = |title: &str| {
            state
                .app
                .store
                .add_card(repo.id, title, "", "person", "Not started")
                .unwrap()
        };
        let (old, new) = (
            add("Cache model downloads"),
            add("Cache the model downloads"),
        );
        let client = reqwest::Client::new();
        let put = |id: i64, estimate: serde_json::Value| {
            client
                .put(format!("http://{addr}/v1/cards/{id}/estimate"))
                .bearer_auth(&*state.token)
                .json(&json!({ "estimate": estimate }))
                .send()
        };
        let got = put(old.id, json!(8))
            .await
            .unwrap()
            .json::<Card>()
            .await
            .unwrap();
        assert_eq!(
            (got.estimate, got.labels),
            (Some(8), vec!["estimate:8".to_string()])
        );
        for bad in [
            json!(0),
            json!(5000),
            json!(-3),
            json!(2.5),
            json!(99999999999u64),
        ] {
            let answer = put(new.id, bad.clone()).await.unwrap();
            assert_eq!(answer.status(), 422, "{bad}");
            let said = answer.text().await.unwrap();
            assert!(
                said.contains("is not an estimate (1 to 1000 points)"),
                "{bad}: {said}"
            );
        }
        assert!(
            put(new.id, json!("big"))
                .await
                .unwrap()
                .status()
                .is_client_error()
        );

        let proposal: serde_json::Value = client
            .get(format!(
                "http://{addr}/v1/cards/{}/estimate/proposal",
                new.id
            ))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(proposal["estimate"], 8);
        assert_eq!(proposal["compared"][0]["id"], old.id);
        assert_eq!(proposal["compared"][0]["estimate"], 8);

        assert_eq!(
            put(old.id, json!(null))
                .await
                .unwrap()
                .json::<Card>()
                .await
                .unwrap()
                .estimate,
            None
        );
    }
}
