//! The automations: chats a schedule starts with an agent and a prompt (see [`crate::app::automations`]).
//!
//! - `GET /v1/automations?project=`: the automations, by name, each with its project, its next firing and its latest
//!   one; of one project when it is named. `POST /v1/automations` `{name, schedule, agent, prompt, repository?,
//!   harness?, enabled?, project?}`: adds one to the project `project` names (ADR 0025), else to the home project of its
//!   repository, else to the oldest project that is not private.
//! - `GET /v1/automations/{id}`, `PUT /v1/automations/{id}` (the same body as `POST`, which replaces it, in the project
//!   it has; naming another is refused) and `DELETE /v1/automations/{id}`.
//! - `POST /v1/automations/{id}/run`: fires it now. `GET /v1/automations/{id}/firings`: its firings, newest first, each
//!   with its chat.

use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{get, post};
use axum::{Extension, Router};

use super::{ApiError, ApiState, blocking};
use crate::app::{App, AppError, AutomationView, Sight};
use crate::decide::Actor;
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

/// What the list may be narrowed by.
#[derive(Debug, serde::Deserialize)]
struct ListQuery {
    #[serde(default)]
    project: Option<i64>,
}

/// What an automation is saved with: what it is made of, and the project it is for.
#[derive(Debug, serde::Deserialize)]
struct Body {
    #[serde(flatten)]
    new: NewAutomation,
    #[serde(default)]
    project: Option<i64>,
}

async fn automations(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    Query(query): Query<ListQuery>,
) -> Result<axum::Json<Vec<AutomationView>>, ApiError> {
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        let mut views = app.automations()?;
        views.retain(|view| {
            view.automation
                .repository_id
                .is_none_or(|r| sight.repository(r))
                && view
                    .automation
                    .project_id
                    .is_none_or(|project| sight.project(project))
                && query
                    .project
                    .is_none_or(|project| view.automation.project_id == Some(project))
        });
        Ok(views)
    })
    .await
}

/// The project a new automation is for: the one named, which the person must see, else the home project of its
/// repository when they see that, else the oldest project that is not private. A server with nowhere to keep a file
/// has no project to name.
fn project_for(
    app: &App,
    sight: &Sight,
    named: Option<i64>,
    new: &NewAutomation,
) -> Result<Option<i64>, AppError> {
    if app.harness_repo().is_err() {
        return Ok(None);
    }
    if let Some(project) = named {
        // A project the person does not see is answered as if it did not exist.
        return if sight.project(project) {
            Ok(Some(app.project(project)?.id))
        } else {
            Err(AppError::NotFound(format!("project {project}")))
        };
    }
    if let Some(repository) = new.repository
        && let Ok(repository) = app.repository(repository)
        && sight.project(repository.home_project_id)
    {
        return Ok(Some(repository.home_project_id));
    }
    let mut projects = app.store.projects()?;
    projects.sort_by_key(|project| (project.created_at, project.id));
    projects
        .into_iter()
        .find(|project| !project.private && sight.project(project.id))
        .map(|project| Some(project.id))
        .ok_or_else(|| {
            AppError::Invalid("an automation belongs to a project: name its project".into())
        })
}

/// Refuses an automation that would work in a repository `by` does not see, as if it did not exist, so that no one
/// starts an agent in the files of a private project they are no member of.
fn check_place(app: &crate::app::App, by: &Actor, new: &NewAutomation) -> crate::app::Result<()> {
    match new.repository {
        Some(id) if !app.sight(by)?.repository(id) => {
            Err(crate::app::AppError::NotFound(format!("repository {id}")))
        }
        _ => Ok(()),
    }
}

async fn add_automation(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    axum::Json(Body { new, project }): axum::Json<Body>,
) -> Result<axum::Json<AutomationView>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        check_place(app, &by, &new)?;
        let project = project_for(app, &app.sight(&by)?, project, &new)?;
        app.save_automation(&*runs, None, project, &new)
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
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(Body { new, project }): axum::Json<Body>,
) -> Result<axum::Json<AutomationView>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        check_place(app, &by, &new)?;
        app.save_automation(&*runs, Some(id), project, &new)
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
