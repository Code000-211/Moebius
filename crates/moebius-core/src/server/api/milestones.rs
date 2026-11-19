//! The milestones (ADR 0019): goals of the organisation with a target date, holding cards of any projects, with their
//! progress. What a person gets is counted and listed within the projects they may see (`App::may_see_project`).
//!
//! - `GET /v1/milestones`: every milestone with its progress, the nearest target date first (`MilestoneView`).
//! - `POST /v1/milestones` `{name, target_date, description}`: makes one; `target_date` is `YYYY-MM-DD`.
//! - `GET /v1/milestones/{id}`: one milestone with the cards the asker may see (`MilestoneDetail`).
//! - `PATCH /v1/milestones/{id}` `{name, target_date, description}`: replaces its fields.
//! - `DELETE /v1/milestones/{id}`: deletes it and answers it; its cards stay.
//! - `GET /v1/cards/{id}/milestones`: the milestones a card is in, the nearest target date first.
//! - `PUT /v1/cards/{id}/milestones` `{milestones}`: replaces the milestones a card is in (ids); answers them.
//!
//! A person (a member or an owner) changes milestones, never an agent.

use axum::extract::{Path as UrlPath, State};
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::app::{MilestoneDetail, MilestoneFields, MilestoneView};
use crate::decide::Actor;
use crate::store::Milestone;

/// The routes of the milestones.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/milestones", get(milestones).post(add_milestone))
        .route(
            "/v1/milestones/{id}",
            get(milestone)
                .patch(edit_milestone)
                .delete(delete_milestone),
        )
        .route(
            "/v1/cards/{id}/milestones",
            get(card_milestones).put(set_card_milestones),
        )
}

/// What `PUT /v1/cards/{id}/milestones` takes.
#[derive(Debug, Deserialize)]
pub struct NewMilestones {
    pub milestones: Vec<i64>,
}

async fn milestones(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
) -> Result<Json<Vec<MilestoneView>>, ApiError> {
    blocking(&state.app, move |app| app.milestone_views(&actor)).await
}

async fn milestone(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<MilestoneDetail>, ApiError> {
    blocking(&state.app, move |app| app.milestone_detail(&actor, id)).await
}

async fn add_milestone(
    State(state): State<ApiState>,
    Json(fields): Json<MilestoneFields>,
) -> Result<Json<Milestone>, ApiError> {
    blocking(&state.app, move |app| app.add_milestone(&fields)).await
}

async fn edit_milestone(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(fields): Json<MilestoneFields>,
) -> Result<Json<Milestone>, ApiError> {
    blocking(&state.app, move |app| app.edit_milestone(id, &fields)).await
}

async fn delete_milestone(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Milestone>, ApiError> {
    blocking(&state.app, move |app| app.delete_milestone(id)).await
}

async fn card_milestones(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Vec<Milestone>>, ApiError> {
    blocking(&state.app, move |app| app.card_milestones(&actor, id)).await
}

async fn set_card_milestones(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewMilestones>,
) -> Result<Json<Vec<Milestone>>, ApiError> {
    blocking(&state.app, move |app| {
        app.set_card_milestones(&actor, id, &new.milestones)
    })
    .await
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode as Status;
    use serde_json::{Value, json};

    use crate::server::api::testing::start;

    async fn call(
        method: &str,
        addr: &str,
        token: &str,
        path: &str,
        body: Option<Value>,
    ) -> (Status, Value) {
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), format!("http://{addr}{path}"))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    #[tokio::test]
    async fn a_milestone_shows_cards_of_two_projects_with_its_progress() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let first = state.app.store.cards(1).unwrap()[0].clone();
        let other = state
            .app
            .store
            .add_repository("storefront", "/s", "none", "main")
            .unwrap();
        let done = state
            .app
            .store
            .add_card(other.id, "Pay", "", "person", "Done")
            .unwrap();

        let (status, made) = call(
            "POST",
            &addr,
            &token,
            "/v1/milestones",
            Some(json!({"name": "Launch", "target_date": "2027-01-31", "description": "Ship"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{made}");
        let id = made["id"].as_i64().unwrap();
        for card in [first.id, done.id] {
            let (status, held) = call(
                "PUT",
                &addr,
                &token,
                &format!("/v1/cards/{card}/milestones"),
                Some(json!({"milestones": [id]})),
            )
            .await;
            assert_eq!(status, Status::OK, "{held}");
            assert_eq!(held[0]["name"], "Launch");
        }

        let (_, all) = call("GET", &addr, &token, "/v1/milestones", None).await;
        assert_eq!(
            (all[0]["cards"].as_i64(), all[0]["done"].as_i64()),
            (Some(2), Some(1))
        );
        assert_eq!(all[0]["projects"].as_array().unwrap().len(), 2);
        let (_, detail) = call("GET", &addr, &token, &format!("/v1/milestones/{id}"), None).await;
        assert_eq!(detail["cards"].as_array().unwrap().len(), 2);
        assert_eq!(detail["target_date"], "2027-01-31");
        let (_, held) = call(
            "GET",
            &addr,
            &token,
            &format!("/v1/cards/{}/milestones", first.id),
            None,
        )
        .await;
        assert_eq!(held[0]["id"], id);
    }

    #[tokio::test]
    async fn edits_and_refusals_have_their_statuses() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let new = json!({"name": "Launch", "target_date": "2027-01-31"});
        let (status, made) = call("POST", &addr, &token, "/v1/milestones", Some(new.clone())).await;
        assert_eq!(status, Status::OK, "{made}");
        let id = made["id"].as_i64().unwrap();
        let (status, _) = call("POST", &addr, &token, "/v1/milestones", Some(new)).await;
        assert_eq!(status, Status::CONFLICT);
        let (status, _) = call(
            "POST",
            &addr,
            &token,
            "/v1/milestones",
            Some(json!({"name": "Late", "target_date": "2027-13-01"})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        let path = format!("/v1/milestones/{id}");
        let (status, edited) = call(
            "PATCH",
            &addr,
            &token,
            &path,
            Some(json!({"name": "Launch 2", "target_date": "2027-02-28", "description": "More"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{edited}");
        assert_eq!(edited["target_date"], "2027-02-28");
        let card = state.app.store.cards(1).unwrap()[0].id;
        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            &format!("/v1/cards/{card}/milestones"),
            Some(json!({"milestones": [99]})),
        )
        .await;
        assert_eq!(status, Status::NOT_FOUND);
        let (status, _) = call("DELETE", &addr, &token, &path, None).await;
        assert_eq!(status, Status::OK);
        let (status, _) = call("GET", &addr, &token, &path, None).await;
        assert_eq!(status, Status::NOT_FOUND);
    }
}
