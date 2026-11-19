//! The cycles (ADR 0019): time boxes of one project, each holding cards of that project, with their progress, figures
//! and burn-up. A project that does not use cycles has none.
//!
//! - `GET /v1/projects/{id}/cycles`: the project's cycles with their progress, the latest start first (`CycleView`).
//! - `POST /v1/projects/{id}/cycles` `{name, starts_on, ends_on}`: makes an open cycle; dates are `YYYY-MM-DD`.
//! - `GET /v1/cycles/{id}`: one cycle with its figures, burn-up and cards (`CycleDetail`).
//! - `PATCH /v1/cycles/{id}` `{name, starts_on, ends_on}`: replaces the fields of an open cycle.
//! - `DELETE /v1/cycles/{id}`: deletes it and answers it; its cards stay.
//! - `POST /v1/cycles/{id}/close` `{carry: "next", to?}` or `{carry: "out"}`: closes it; the cards that are not done
//!   move to the open cycle `to` (default: the one that starts next) or to none (`Closed`).
//! - `GET /v1/cards/{id}/cycle`: the open cycle the card is in, or null.
//! - `PUT /v1/cards/{id}/cycle` `{cycle}`: puts the card into an open cycle of its project, or out of it with null.
//!
//! A person (a member or an owner) changes cycles, never an agent.

use axum::extract::{Path as UrlPath, State};
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::app::{Carry, Closed, CycleDetail, CycleFields, CycleView};
use crate::decide::Actor;
use crate::store::Cycle;

/// The routes of the cycles.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/projects/{id}/cycles", get(cycles).post(add_cycle))
        .route(
            "/v1/cycles/{id}",
            get(cycle).patch(edit_cycle).delete(delete_cycle),
        )
        .route("/v1/cycles/{id}/close", axum::routing::post(close_cycle))
        .route("/v1/cards/{id}/cycle", get(card_cycle).put(set_card_cycle))
}

/// What `PUT /v1/cards/{id}/cycle` takes.
#[derive(Debug, Deserialize)]
pub struct NewCycle {
    pub cycle: Option<i64>,
}

async fn cycles(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Vec<CycleView>>, ApiError> {
    blocking(&state.app, move |app| app.cycle_views(&actor, id)).await
}

async fn add_cycle(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(fields): Json<CycleFields>,
) -> Result<Json<Cycle>, ApiError> {
    blocking(&state.app, move |app| app.add_cycle(&actor, id, &fields)).await
}

async fn cycle(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<CycleDetail>, ApiError> {
    blocking(&state.app, move |app| app.cycle_detail(&actor, id)).await
}

async fn edit_cycle(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(fields): Json<CycleFields>,
) -> Result<Json<Cycle>, ApiError> {
    blocking(&state.app, move |app| app.edit_cycle(&actor, id, &fields)).await
}

async fn delete_cycle(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Cycle>, ApiError> {
    blocking(&state.app, move |app| app.delete_cycle(&actor, id)).await
}

async fn close_cycle(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(carry): Json<Carry>,
) -> Result<Json<Closed>, ApiError> {
    blocking(&state.app, move |app| app.close_cycle(&actor, id, &carry)).await
}

async fn card_cycle(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Option<Cycle>>, ApiError> {
    blocking(&state.app, move |app| app.card_cycle(&actor, id)).await
}

async fn set_card_cycle(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewCycle>,
) -> Result<Json<Option<Cycle>>, ApiError> {
    blocking(&state.app, move |app| {
        app.set_card_cycle(&actor, id, new.cycle)
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
    async fn a_cycle_is_planned_followed_and_closed() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let first = state.app.store.cards(1).unwrap()[0].clone();
        let project = first.project_id;
        let second = state
            .app
            .store
            .add_card(first.repository_id, "Two", "", "person", "Not started")
            .unwrap();

        let new = json!({"name": "Cycle 1", "starts_on": "2026-11-02", "ends_on": "2026-11-15"});
        let (status, one) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/projects/{project}/cycles"),
            Some(new.clone()),
        )
        .await;
        assert_eq!(status, Status::OK, "{one}");
        let (status, _) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/projects/{project}/cycles"),
            Some(new),
        )
        .await;
        assert_eq!(status, Status::CONFLICT);
        let (_, two) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/projects/{project}/cycles"),
            Some(json!({"name": "Cycle 2", "starts_on": "2026-11-16", "ends_on": "2026-11-29"})),
        )
        .await;
        let (one, two) = (one["id"].as_i64().unwrap(), two["id"].as_i64().unwrap());

        for (card, estimate) in [(first.id, 3), (second.id, 5)] {
            let (status, _) = call(
                "PUT",
                &addr,
                &token,
                &format!("/v1/cards/{card}/estimate"),
                Some(json!({"estimate": estimate})),
            )
            .await;
            assert_eq!(status, Status::OK);
            let (status, held) = call(
                "PUT",
                &addr,
                &token,
                &format!("/v1/cards/{card}/cycle"),
                Some(json!({"cycle": one})),
            )
            .await;
            assert_eq!(status, Status::OK, "{held}");
            assert_eq!(held["name"], "Cycle 1");
        }
        let (_, views) = call(
            "GET",
            &addr,
            &token,
            &format!("/v1/projects/{project}/cycles"),
            None,
        )
        .await;
        assert_eq!(views[0]["name"], "Cycle 2");
        assert_eq!(views[1]["points"], 8);

        // The progress follows the card: finishing one moves the numbers.
        state.app.store.move_card(second.id, "Done").unwrap();
        let (_, detail) = call("GET", &addr, &token, &format!("/v1/cycles/{one}"), None).await;
        assert_eq!(
            (
                detail["cards"].as_array().unwrap().len(),
                detail["points_done"].as_i64()
            ),
            (2, Some(5))
        );
        assert!(detail["figures"]["tokens_per_point"].is_number());

        let (status, closed) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/cycles/{one}/close"),
            Some(json!({"carry": "next"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{closed}");
        assert_eq!(closed["carried_to"]["id"], two);
        assert_eq!(closed["carried"], json!([first.id]));
        let (_, held) = call(
            "GET",
            &addr,
            &token,
            &format!("/v1/cards/{}/cycle", first.id),
            None,
        )
        .await;
        assert_eq!(held["id"], two);
        let (status, _) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/cycles/{one}/close"),
            Some(json!({"carry": "out"})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn a_card_of_another_project_is_refused_and_a_missing_cycle_is_not_found() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let first = state.app.store.cards(1).unwrap()[0].clone();
        let other = state
            .app
            .store
            .add_repository("storefront", "/s", "none", "main")
            .unwrap();
        let (status, made) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/projects/{}/cycles", other.home_project_id),
            Some(json!({"name": "Theirs", "starts_on": "2026-11-02", "ends_on": "2026-11-15"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{made}");
        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            &format!("/v1/cards/{}/cycle", first.id),
            Some(json!({"cycle": made["id"]})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        let (status, _) = call("GET", &addr, &token, "/v1/cycles/99", None).await;
        assert_eq!(status, Status::NOT_FOUND);
        let (status, _) = call(
            "POST",
            &addr,
            &token,
            "/v1/projects/999/cycles",
            Some(json!({"name": "X", "starts_on": "2026-11-02", "ends_on": "2026-11-15"})),
        )
        .await;
        assert_eq!(status, Status::NOT_FOUND);
        let (status, _) = call(
            "PATCH",
            &addr,
            &token,
            &format!("/v1/cycles/{}", made["id"]),
            Some(json!({"name": "Theirs", "starts_on": "2026-11-20", "ends_on": "2026-11-15"})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        let (status, _) = call(
            "DELETE",
            &addr,
            &token,
            &format!("/v1/cycles/{}", made["id"]),
            None,
        )
        .await;
        assert_eq!(status, Status::OK);
    }
}
