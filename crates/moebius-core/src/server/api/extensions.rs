//! Pi packages (see [`crate::harness::packages`] and [`crate::app`]'s `packages`): the catalog, what is installed, and
//! proposals to install, update and remove, each a change of the harness configuration repository that only a person
//! merges.
//!
//! - `GET /v1/extensions/search?q=&type=`: the packages of the catalog that match `q`, of the type `type` if given.
//! - `GET /v1/extensions/info?source=`: what the package a source names is, before it is installed.
//! - `GET /v1/extensions/packages`: the packages the repository vendors.
//! - `POST /v1/extensions/packages` `{source, agent?}`: proposes installing a package, and naming it in an agent.
//! - `POST /v1/extensions/packages/{name}/update` and `POST /v1/extensions/update`: propose updating a package, or all.
//! - `DELETE /v1/extensions/packages/{name}`: proposes removing a package.
//!
//! The run token of the Moebius agent may propose; merging a change that touches a package is refused to it (see
//! [`refuse_agent_merge`]), since an extension is code that runs in every Pi process of the agents that name it.

use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::app::{Offered, PackageInfo, PackageView, Updates};
use crate::decide::Actor;
use crate::harness::packages::SearchHit;

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/extensions/search", get(search))
        .route("/v1/extensions/info", get(info))
        .route("/v1/extensions/packages", get(packages).post(add))
        .route("/v1/extensions/packages/{name}/update", post(update))
        .route("/v1/extensions/packages/{name}", delete(remove))
        .route("/v1/extensions/update", post(update_all))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InfoQuery {
    source: String,
}

/// The body of `POST /v1/extensions/packages`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewPackage {
    source: String,
    agent: Option<String>,
}

async fn search(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> Result<axum::Json<Vec<SearchHit>>, ApiError> {
    blocking(&state.app, move |app| {
        app.search_packages(&query.q, query.kind.as_deref())
    })
    .await
}

async fn info(
    State(state): State<ApiState>,
    Query(query): Query<InfoQuery>,
) -> Result<axum::Json<PackageInfo>, ApiError> {
    blocking(&state.app, move |app| app.package_info(&query.source)).await
}

async fn packages(State(state): State<ApiState>) -> Result<axum::Json<Vec<PackageView>>, ApiError> {
    blocking(&state.app, |app| app.packages()).await
}

async fn add(
    State(state): State<ApiState>,
    axum::Json(new): axum::Json<NewPackage>,
) -> Result<axum::Json<Offered>, ApiError> {
    blocking(&state.app, move |app| {
        app.add_package(&new.source, new.agent.as_deref())
    })
    .await
}

async fn update(
    State(state): State<ApiState>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<Updates>, ApiError> {
    blocking(&state.app, move |app| app.update_packages(Some(&name))).await
}

async fn update_all(State(state): State<ApiState>) -> Result<axum::Json<Updates>, ApiError> {
    blocking(&state.app, |app| app.update_packages(None)).await
}

async fn remove(
    State(state): State<ApiState>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<Offered>, ApiError> {
    blocking(&state.app, move |app| app.remove_package(&name)).await
}

/// Refuses an agent the merge of the change `id` when the change installs, updates or removes a package.
pub(super) async fn refuse_agent_merge(
    state: &ApiState,
    by: &Actor,
    id: i64,
) -> Result<(), ApiError> {
    if matches!(by, Actor::Person) {
        return Ok(());
    }
    let touches = blocking(&state.app, move |app| app.change_touches_packages(id))
        .await?
        .0;
    refuse_if(touches)
}

/// Refuses an agent the merge of the proposal `id` of the Retro when its branch touches a package.
pub(super) async fn refuse_agent_proposal(
    state: &ApiState,
    by: &Actor,
    id: i64,
) -> Result<(), ApiError> {
    if matches!(by, Actor::Person) {
        return Ok(());
    }
    let repo = state.harness_repo.clone();
    let touches = blocking(&state.app, move |app| {
        let proposal = app.store.proposals(None)?.into_iter().find(|p| p.id == id);
        Ok(match (repo, proposal) {
            (Some(repo), Some(p)) if p.kind == "harness" => repo.changes_packages(&p.branch)?,
            _ => false,
        })
    })
    .await?
    .0;
    refuse_if(touches)
}

fn refuse_if(touches: bool) -> Result<(), ApiError> {
    if touches {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            crate::harness::repo::PERSON_ONLY.into(),
        ));
    }
    Ok(())
}
