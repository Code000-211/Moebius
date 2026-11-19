//! The repositories a card works in (ADR 0019).
//!
//! - `PUT /v1/cards/{id}/repositories` `{repositories, reason}`: names the repositories the card works in besides its
//!   own issue's, in the order its changes are to be merged; an empty list leaves it its own only. Answers the `Card`,
//!   whose `repositories` lists them all, its own first. A person or the Moebius agent does this.

use axum::extract::{Path as UrlPath, State};
use axum::routing::put;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking};
use crate::decide::Actor;
use crate::store::Card;

/// The routes of a card's repositories.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/cards/{id}/repositories", put(set_card_repositories))
}

/// The body of `PUT /v1/cards/{id}/repositories`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewRepositories {
    /// Names of repositories of the card's project, or of its files.
    pub repositories: Vec<String>,
    #[serde(default)]
    pub reason: String,
}

async fn set_card_repositories(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(body): Json<NewRepositories>,
) -> Result<Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.set_card_repositories(&by, id, &body.repositories, &body.reason)
    })
    .await
}
