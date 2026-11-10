//! The automations: chats a schedule starts with an agent and a prompt (see [`crate::app::automations`]).
//!
//! - `GET /v1/automations`: the automations, by name, each with its next firing and its latest one. `POST
//!   /v1/automations` `{name, schedule, agent, prompt, repository?, harness?, enabled?}`: adds one.
//! - `GET /v1/automations/{id}`, `PUT /v1/automations/{id}` (the same body as `POST`, which replaces it) and `DELETE
//!   /v1/automations/{id}`.
//! - `POST /v1/automations/{id}/run`: fires it now. `GET /v1/automations/{id}/firings`: its firings, newest first, each
//!   with its chat.

use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};

use super::{ApiError, ApiState, blocking};
use crate::app::AutomationView;
use crate::store::{Firing, NewAutomation};

/// The routes of the automations.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/automations", get(automations).post(add_automation))
        .route(
            "/v1/automations/{id}",
            get(automation)
                .put(save_automation)
                .delete(delete_automation),
        )
        .route("/v1/automations/{id}/run", post(run_automation))
        .route("/v1/automations/{id}/firings", get(firings))
}

async fn automations(
    State(state): State<ApiState>,
) -> Result<axum::Json<Vec<AutomationView>>, ApiError> {
    blocking(&state.app, |app| app.automations()).await
}

async fn add_automation(
    State(state): State<ApiState>,
    axum::Json(new): axum::Json<NewAutomation>,
) -> Result<axum::Json<AutomationView>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.save_automation(&*runs, None, &new)
    })
    .await
}

async fn automation(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<AutomationView>, ApiError> {
    blocking(&state.app, move |app| app.automation(id)).await
}

async fn save_automation(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(new): axum::Json<NewAutomation>,
) -> Result<axum::Json<AutomationView>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.save_automation(&*runs, Some(id), &new)
    })
    .await
}

async fn delete_automation(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    blocking(&state.app, move |app| {
        app.delete_automation(id).map(|()| serde_json::json!({}))
    })
    .await
}

async fn run_automation(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Firing>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| app.run_automation(&*runs, id)).await
}

async fn firings(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Vec<Firing>>, ApiError> {
    blocking(&state.app, move |app| app.firings(id)).await
}

#[cfg(test)]
// Some tests are Unix only; on Windows what only they use is unused.
#[cfg_attr(not(unix), allow(unused))]
mod tests;
