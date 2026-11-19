//! Intake (ADR 0019): a project that turns it on takes requests, which a grilling turns into draft issues that a member
//! accepts, edits or declines (see [`crate::app::App::submit_request`]).
//!
//! - `PUT /v1/projects/{id}/intake` `{enabled}`: an owner turns the project's intake on or off; answers the project.
//! - `POST /v1/projects/{id}/requests` `{text}`: a person, a viewer too, writes a request in their own words; a
//!   project without intake answers `422`. Starts the grilling and answers the request, `grilling` until it has a draft.
//! - `GET /v1/projects/{id}/requests`: a viewer's own requests, or everyone's for a member or an owner, newest first.
//! - `GET /v1/requests/{id}`: one request, which a viewer reads only when it is theirs.
//! - `PUT /v1/requests/{id}` `{title, body}`: a member edits the draft issue (and gives a request still being grilled
//!   its draft at once).
//! - `POST /v1/requests/{id}/accept` `{title?, body?}`: a member makes a card of the draft, as it is or as edited; the
//!   requester follows it and is told.
//! - `POST /v1/requests/{id}/decline` `{reason}`: a member declines it; the requester is told why.

use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post, put};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::app::ProjectView;
use crate::decide::Actor;
use crate::store::IntakeRequest;

/// The routes of intake.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/projects/{id}/intake", put(set_intake))
        .route("/v1/projects/{id}/requests", get(requests).post(submit))
        .route("/v1/requests/{id}", get(request).put(edit))
        .route("/v1/requests/{id}/accept", post(accept))
        .route("/v1/requests/{id}/decline", post(decline))
}

/// The body of `PUT /v1/projects/{id}/intake`.
#[derive(Debug, Deserialize)]
pub struct NewIntake {
    pub enabled: bool,
}

/// The body of `POST /v1/projects/{id}/requests`.
#[derive(Debug, Deserialize)]
pub struct NewRequest {
    pub text: String,
}

/// The body of `PUT /v1/requests/{id}`: the draft issue.
#[derive(Debug, Deserialize)]
pub struct Draft {
    pub title: String,
    #[serde(default)]
    pub body: String,
}

/// The body of `POST /v1/requests/{id}/accept`: the draft as edited, or nothing to accept it as it is.
#[derive(Debug, Default, Deserialize)]
pub struct Acceptance {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

/// The body of `POST /v1/requests/{id}/decline`.
#[derive(Debug, Deserialize)]
pub struct Declining {
    pub reason: String,
}

async fn set_intake(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(intake): Json<NewIntake>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| app.set_intake(id, intake.enabled)).await
}

async fn submit(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewRequest>,
) -> Result<Json<IntakeRequest>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.submit_request(&*runs, &by, id, &new.text)
    })
    .await
}

async fn requests(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Vec<IntakeRequest>>, ApiError> {
    blocking(&state.app, move |app| app.requests(&by, id)).await
}

async fn request(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<IntakeRequest>, ApiError> {
    blocking(&state.app, move |app| app.request(&by, id)).await
}

async fn edit(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(draft): Json<Draft>,
) -> Result<Json<IntakeRequest>, ApiError> {
    blocking(&state.app, move |app| {
        app.edit_request(&by, id, &draft.title, &draft.body)
    })
    .await
}

async fn accept(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<Json<Acceptance>>,
) -> Result<Json<IntakeRequest>, ApiError> {
    let runs = state.runs.clone();
    let accepted = body.map(|body| body.0).unwrap_or_default();
    blocking(&state.app, move |app| {
        let edited = match (&accepted.title, &accepted.body) {
            (None, None) => None,
            (title, body) => {
                let current = app.request(&by, id)?;
                Some((
                    title.clone().unwrap_or(current.title),
                    body.clone().unwrap_or(current.body),
                ))
            }
        };
        app.accept_request(
            &*runs,
            &by,
            id,
            edited.as_ref().map(|(t, b)| (t.as_str(), b.as_str())),
        )
    })
    .await
}

async fn decline(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(declining): Json<Declining>,
) -> Result<Json<IntakeRequest>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.decline_request(&*runs, &by, id, &declining.reason)
    })
    .await
}
