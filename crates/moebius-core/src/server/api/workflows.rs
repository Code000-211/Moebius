//! The workflow door the canvas, the command line and the tests use (see [`crate::workflows::definition`]). A workflow
//! belongs to one project (ADR 0025) and is addressed as (project, name):
//!
//! - `GET /v1/projects/{id}/workflows`: the project's workflows, named, described, with their trigger kind and how many
//!   nodes each has.
//! - `GET /v1/projects/{id}/workflows/{name}`: one workflow's file and its parsed form.
//! - `PUT /v1/projects/{id}/workflows/{name}` `{file}` or `{definition}`: validates and commits the workflow (the
//!   canvas's save). An invalid definition is refused with the validation message and the file is untouched.
//! - `DELETE /v1/projects/{id}/workflows/{name}`: commits the workflow's deletion, refused while a run of it is queued,
//!   running or waiting.
//! - `POST /v1/workflows/runs/{id}/cancel`: cancels a run of a workflow and ends it `cancelled`.
//! - `POST /v1/workflows/runs/{run_id}/resume` `{decision, value?, reason?}`: records a person's decision on a run that
//!   waits for one (`workflow_approval` in the inbox); a second decision on a finished wait is refused.
//!
//! Deprecated, until the Workflows tab of a project replaces what still uses them: `GET /v1/workflows` (every workflow
//! of every project the person sees, each with its project) and `GET`, `PUT` and `DELETE /v1/workflows/{name}`, which
//! name a workflow by its name alone. A name is resolved when exactly one project the person sees has it, is not found
//! when none does and is answered `409` with the choices when several do ([`owner_of`]). A workflow no project has is
//! saved in the project the body's `project` names, else in the oldest project that is not private.

use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking};
use crate::app::workflows::DecisionRequest;
use crate::app::workflows::stopping::CancelWorkflowRunError;
use crate::app::{App, AppError, Sight};
use crate::harness::repo::HarnessRepo;
use crate::store::{Project, WorkflowRun};
use crate::workflows::definition::{self, WorkflowDefinition};

/// The routes of workflows.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/projects/{id}/workflows", get(project_list))
        .route(
            "/v1/projects/{id}/workflows/{name}",
            get(project_show).put(project_save).delete(project_delete),
        )
        .route("/v1/workflows", get(list))
        .route("/v1/workflows/{name}", get(show).put(save).delete(delete))
        .route("/v1/workflows/runs/{id}/cancel", post(cancel_run))
        .route("/v1/workflows/runs/{run_id}/resume", post(resume))
}

/// One workflow as the list names it.
#[derive(Debug, Serialize)]
struct Summary {
    /// The project that owns the workflow.
    project: i64,
    project_name: String,
    name: String,
    description: String,
    trigger_kind: String,
    node_count: usize,
    /// The newest run of the workflow the person sees; none when it never ran.
    last_run: Option<LastRun>,
}

/// The newest run of a workflow, as its row in the list tells it.
#[derive(Debug, Serialize)]
struct LastRun {
    id: i64,
    status: String,
    created_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
}

/// One workflow's file and parsed form.
#[derive(Debug, Serialize)]
struct View {
    project: i64,
    project_name: String,
    name: String,
    file: String,
    definition: WorkflowDefinition,
}

/// What `PUT` takes: either the file's text or its parsed form, and for the deprecated route the project a new
/// workflow goes to.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveRequest {
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    definition: Option<WorkflowDefinition>,
    /// The project the workflow belongs to; on a route that names the project it must be that one.
    #[serde(default)]
    project: Option<i64>,
}

/// What saving a workflow answers: the saved workflow and the commit it made.
#[derive(Debug, Serialize)]
struct Saved {
    #[serde(flatten)]
    view: View,
    /// The short id of the commit; left out when the definition changed nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
}

/// What deleting a workflow answers: the project, the name and the commit it made.
#[derive(Debug, Serialize)]
struct Deleted {
    project: i64,
    name: String,
    /// The short id of the commit; left out when there was nothing to commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
}

/// The workflows of `projects`, in project order and then name order, as the list names them.
fn summaries(
    app: &App,
    sight: &Sight,
    repo: &HarnessRepo,
    projects: &[Project],
) -> Result<Vec<Summary>, AppError> {
    let mut summaries = Vec::new();
    for project in projects {
        for (name, definition) in repo.project_workflows(&project.name)? {
            let last_run =
                super::workflow_runs::visible_runs(app, sight, Some(project.id), Some(&name), 1)?
                    .into_iter()
                    .next()
                    .map(|run| LastRun {
                        id: run.id,
                        status: run.status,
                        created_at: run.created_at,
                        started_at: run.started_at,
                        finished_at: run.finished_at,
                    });
            summaries.push(Summary {
                project: project.id,
                project_name: project.name.clone(),
                name,
                description: definition.description,
                trigger_kind: definition
                    .trigger
                    .first()
                    .map(|trigger| trigger.kind.clone())
                    .unwrap_or_default(),
                node_count: definition.nodes.len(),
                last_run,
            });
        }
    }
    Ok(summaries)
}

async fn project_list(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath(project): UrlPath<i64>,
) -> Result<axum::Json<Vec<Summary>>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        let project = app.project(project)?;
        summaries(app, &sight, &repo, &[project])
    })
    .await
}

/// Deprecated: the workflows of every project the person sees.
async fn list(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
) -> Result<axum::Json<Vec<Summary>>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        let projects = visible_projects(app, &sight)?;
        summaries(app, &sight, &repo, &projects)
    })
    .await
}

async fn project_show(
    State(state): State<ApiState>,
    UrlPath((project, name)): UrlPath<(i64, String)>,
) -> Result<axum::Json<View>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |app| {
        view(&repo, &app.project(project)?, &name)
    })
    .await
}

/// Deprecated: the workflow `name` of the one project that has it.
async fn show(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<View>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |app| {
        let project = owner_of(app, &app.sight(&by)?, &repo, &name)?;
        view(&repo, &project, &name)
    })
    .await
}

/// The definition a save carries, and the project it names, if it names one.
fn definition_of(
    name: &str,
    request: SaveRequest,
) -> Result<(WorkflowDefinition, Option<i64>), AppError> {
    let definition = match (request.file, request.definition) {
        (Some(file), None) => {
            definition::parse(name, &file).map_err(|error| AppError::Invalid(error.to_string()))?
        }
        (None, Some(definition)) => definition,
        _ => {
            return Err(AppError::Invalid(
                "send exactly one of file and definition".into(),
            ));
        }
    };
    Ok((definition, request.project))
}

fn save_in(
    app: &App,
    repo: &HarnessRepo,
    by: &crate::decide::Actor,
    project: &Project,
    name: &str,
    definition: &WorkflowDefinition,
) -> Result<Saved, AppError> {
    let commit = repo.save_project_workflow(app, by, &project.name, name, definition)?;
    Ok(Saved {
        view: view(repo, project, name)?,
        commit,
    })
}

async fn project_save(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath((project, name)): UrlPath<(i64, String)>,
    axum::Json(request): axum::Json<SaveRequest>,
) -> Result<axum::Json<Saved>, ApiError> {
    let repo = harness(&state)?;
    let (definition, named) = definition_of(&name, request)?;
    if named.is_some_and(|named| named != project) {
        return Err(AppError::Invalid(format!(
            "the workflow is saved in the project {project}, which the address names, not in the project {}",
            named.unwrap_or_default()
        ))
        .into());
    }
    blocking(&state.app, move |app| {
        save_in(app, &repo, &by, &app.project(project)?, &name, &definition)
    })
    .await
}

/// Deprecated: saves the workflow `name` where it is, or, when no project has it, in the project the body names.
async fn save(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath(name): UrlPath<String>,
    axum::Json(request): axum::Json<SaveRequest>,
) -> Result<axum::Json<Saved>, ApiError> {
    let repo = harness(&state)?;
    let (definition, named) = definition_of(&name, request)?;
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        let project = match named {
            // A project the person does not see is answered as if it did not exist, whether it does or not.
            Some(project) if !sight.project(project) => {
                return Err(AppError::NotFound(format!("project {project}")));
            }
            Some(project) => app.project(project)?,
            None => match owner_of(app, &sight, &repo, &name) {
                Ok(project) => project,
                Err(AppError::NotFound(_)) => oldest_open_project(app, &sight)?,
                Err(other) => return Err(other),
            },
        };
        save_in(app, &repo, &by, &project, &name, &definition)
    })
    .await
}

/// Cancels a run of a workflow and ends it `cancelled`: a queued run never starts, a run going stops at its current
/// node, and a run waiting on a person leaves its inbox item withdrawn. A run that has already ended answers 409.
async fn cancel_run(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<WorkflowRun>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.cancel_workflow_run(&by, &*runs, id)
            .map_err(|error| match error {
                CancelWorkflowRunError::NotFound => {
                    AppError::NotFound(format!("workflow run {id}"))
                }
                CancelWorkflowRunError::AlreadyFinished { status } => {
                    AppError::Conflict(format!("workflow run {id} has already ended as {status}"))
                }
                CancelWorkflowRunError::Store(error) => AppError::Store(error),
                CancelWorkflowRunError::App(error) => error,
            })
    })
    .await
}

async fn project_delete(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath((project, name)): UrlPath<(i64, String)>,
) -> Result<axum::Json<Deleted>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |app| {
        delete_in(app, &repo, &by, &app.project(project)?, name)
    })
    .await
}

fn delete_in(
    app: &App,
    repo: &HarnessRepo,
    by: &crate::decide::Actor,
    project: &Project,
    name: String,
) -> Result<Deleted, AppError> {
    let commit = repo.delete_project_workflow(app, by, &project.name, &name)?;
    // A workflow file that holds an automation is the automation: its row goes with it, so that nothing is left on the
    // page that no file fires.
    if let Some(automation) = app
        .store
        .automation_with_project_workflow_key(project.id, &name)?
    {
        app.store.delete_automation(automation.id)?;
    }
    Ok(Deleted {
        project: project.id,
        name,
        commit,
    })
}

/// Deprecated: deletes the workflow `name` of the one project that has it.
async fn delete(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<Deleted>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |app| {
        let project = owner_of(app, &app.sight(&by)?, &repo, &name)?;
        delete_in(app, &repo, &by, &project, name)
    })
    .await
}

/// What resuming a run answers: the run the decision was recorded against, and the status it went to.
#[derive(Debug, Serialize)]
struct Resumed {
    run: i64,
    status: String,
}

/// Records a person's decision on a workflow run that waits for one (see `app::workflows::approval`): the run runs
/// on with the value, the person's, or fails with the reason.
async fn resume(
    State(state): State<ApiState>,
    UrlPath(run_id): UrlPath<i64>,
    axum::Json(request): axum::Json<DecisionRequest>,
) -> Result<axum::Json<Resumed>, ApiError> {
    let run = blocking(&state.app, move |app| {
        app.resume_workflow_run(run_id, &request)
    })
    .await?;
    Ok(axum::Json(Resumed {
        run: run.0.id,
        status: run.0.status,
    }))
}

/// The harness configuration repository, or the conflict that the server has none.
pub(super) fn harness(state: &ApiState) -> Result<std::sync::Arc<HarnessRepo>, ApiError> {
    if let Some(repo) = state.harness_repo.clone() {
        return Ok(repo);
    }
    state.app.harness_repo().cloned().map_err(|_| {
        AppError::Conflict("the server has no harness configuration repository".into()).into()
    })
}

/// The projects `sight` sees, the oldest first.
pub(super) fn visible_projects(app: &App, sight: &Sight) -> Result<Vec<Project>, AppError> {
    let mut projects: Vec<Project> = app
        .store
        .projects()?
        .into_iter()
        .filter(|project| sight.project(project.id))
        .collect();
    projects.sort_by_key(|project| (project.created_at, project.id));
    Ok(projects)
}

/// The oldest project that is not private among those `sight` sees, where a workflow goes when nothing says which
/// project it is for (the rule the migration of the old files uses).
fn oldest_open_project(app: &App, sight: &Sight) -> Result<Project, AppError> {
    visible_projects(app, sight)?
        .into_iter()
        .find(|project| !project.private)
        .ok_or_else(|| {
            AppError::Invalid(
                "no project can take the workflow: create a project, or name one in `project`"
                    .into(),
            )
        })
}

/// The project whose workflow is called `name`, for a route that names the workflow only: the one project of those
/// `sight` sees that has it. None is not found; several are a conflict that names them, so that the caller can pick one
/// and use the route that names it.
pub(super) fn owner_of(
    app: &App,
    sight: &Sight,
    repo: &HarnessRepo,
    name: &str,
) -> Result<Project, AppError> {
    let mut owners = Vec::new();
    for project in visible_projects(app, sight)? {
        let names = definition::names_in_project(repo.path(), &project.name)
            .map_err(|error| AppError::Invalid(error.to_string()))?;
        if names.iter().any(|candidate| candidate == name) {
            owners.push(project);
        }
    }
    match owners.len() {
        0 => Err(AppError::NotFound(format!("workflow {name}"))),
        1 => Ok(owners.remove(0)),
        _ => Err(AppError::Conflict(format!(
            "the workflow {name} exists in more than one project: {}; use /v1/projects/{{id}}/workflows/{name}",
            owners
                .iter()
                .map(|project| format!("{} (id {})", project.name, project.id))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// One workflow's file and parsed form.
fn view(repo: &HarnessRepo, project: &Project, name: &str) -> Result<View, AppError> {
    // A name that is no workflow is not found, rather than a file that cannot be read.
    let file = definition::project_file_for(&project.name, name)
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    if !repo.path().join(file).is_file() {
        return Err(AppError::NotFound(format!("workflow {name}")));
    }
    let definition = repo.project_workflow(&project.name, name)?;
    let file = definition::read_project_text(repo.path(), &project.name, name)
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    Ok(View {
        project: project.id,
        project_name: project.name.clone(),
        name: name.to_string(),
        file,
        definition,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, RwLock};

    use serde_json::{Value, json};

    use super::*;
    use crate::config::AgentSet;
    use crate::harness::repo::{self, HarnessRepo};
    use crate::server::api::testing;

    const APPROVAL: &str = r#"
name = "asking"
description = "Ask a person before the change ships."

[[trigger]]
kind = "test"

[[nodes]]
id = "ask"
kind = "approval"
params = { prompt = "Ship the change?" }

[[nodes]]
id = "after"
kind = "test"

[[edges]]
from = "trigger"
port = "out"
to = "ask"

[[edges]]
from = "ask"
port = "out"
to = "after"
"#;

    const WORKFLOW: &str = r#"
name = "release"
description = "Build the change and run its tests."

[[trigger]]
kind = "test"
params = { repository = "moebius" }

[[nodes]]
id = "build"
kind = "test"
params = { command = "cargo build" }
position = { x = 12.5, y = 34.0 }

[[nodes]]
id = "test"
kind = "test"
position = { x = 80.0, y = 4.5 }

[[edges]]
from = "trigger"
to = "build"
port = "out"

[[edges]]
from = "build"
to = "test"
port = "out"
"#;

    async fn started() -> (tempfile::TempDir, ApiState, String, Arc<HarnessRepo>) {
        let (dir, state, addr) = testing::start().await;
        let origin = dir.path().join("harness-origin");
        repo::create(&origin, &BTreeMap::new(), None).unwrap();
        let clone = dir.path().join("harness-clone");
        let harness = Arc::new(
            HarnessRepo::open(
                &state.app,
                origin.to_str().unwrap(),
                &clone,
                vec![],
                Arc::new(RwLock::new(AgentSet::default())),
            )
            .unwrap(),
        );
        state.app.set_harness_repo(harness.clone());
        (dir, state, addr, harness)
    }

    async fn put(state: &ApiState, addr: &str, name: &str, body: Value) -> reqwest::Response {
        reqwest::Client::new()
            .put(format!("http://{addr}/v1/workflows/{name}"))
            .bearer_auth(&*state.token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn delete(state: &ApiState, addr: &str, name: &str) -> reqwest::Response {
        reqwest::Client::new()
            .delete(format!("http://{addr}/v1/workflows/{name}"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap()
    }

    async fn get_response(state: &ApiState, addr: &str, path: &str) -> reqwest::Response {
        reqwest::Client::new()
            .get(format!("http://{addr}{path}"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap()
    }

    async fn post(state: &ApiState, addr: &str, path: &str) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("http://{addr}{path}"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap()
    }

    fn log(harness: &HarnessRepo) -> String {
        let output = std::process::Command::new("git")
            .args(["log", "--format=%s"])
            .current_dir(harness.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    }

    async fn resume(state: &ApiState, addr: &str, run: i64, body: Value) -> reqwest::Response {
        testing::post(
            state,
            addr,
            format!("/v1/workflows/runs/{run}/resume"),
            body,
        )
        .await
    }

    /// Starts a workflow run that waits for a person, and returns its id once it does.
    async fn waiting(state: &ApiState, harness: &HarnessRepo, value: Value) -> i64 {
        let folder = harness.path().join("projects/radiance/workflows");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("asking.toml"), APPROVAL).unwrap();
        let project = state
            .app
            .store
            .project_by_name("radiance")
            .unwrap()
            .unwrap();
        let run = state
            .app
            .start_project_workflow_run(project.id, "asking", None, value)
            .unwrap();
        testing::until("the run waits for a decision", || {
            state
                .app
                .store
                .workflow_run(run.id)
                .unwrap()
                .unwrap()
                .status
                == "waiting"
        })
        .await;
        run.id
    }

    /// Waits for the run to end, then returns it as stored.
    async fn ended(state: &ApiState, run: i64) -> crate::store::WorkflowRun {
        testing::until("the run ends", || {
            matches!(
                state
                    .app
                    .store
                    .workflow_run(run)
                    .unwrap()
                    .unwrap()
                    .status
                    .as_str(),
                "succeeded" | "failed"
            )
        })
        .await;
        state.app.store.workflow_run(run).unwrap().unwrap()
    }

    /// The value the node after the wait was given.
    fn left_the_wait(state: &ApiState, run: i64) -> Value {
        state
            .app
            .store
            .workflow_node_runs(run)
            .unwrap()
            .into_iter()
            .find(|row| row.node_id == "after")
            .expect("the run ran on past the wait")
            .input
    }

    /// The run and the status the resume route answered for it.
    async fn answered(response: reqwest::Response, run: i64) -> Value {
        assert_eq!(response.status(), 200);
        let answered: Value = response.json().await.unwrap();
        assert_eq!(answered["run"], json!(run));
        assert!(
            matches!(
                answered["status"].as_str(),
                Some("running" | "succeeded" | "failed")
            ),
            "{answered}"
        );
        answered
    }

    #[tokio::test]
    async fn an_invalid_definition_is_refused_and_the_file_is_untouched() {
        let (_dir, state, addr, harness) = started().await;
        let response = put(&state, &addr, "release", json!({ "file": WORKFLOW })).await;
        assert_eq!(response.status(), 200);
        let before: Value = testing::get(&state, &addr, "/v1/workflows/release").await;

        let response = put(
            &state,
            &addr,
            "release",
            json!({ "file": "name = \"release\"\ndescription = \"broken\"\n" }),
        )
        .await;
        assert_eq!(response.status(), 422);
        let refusal = response.text().await.unwrap();
        assert!(
            refusal.contains("trigger"),
            "the refusal says why: {refusal}"
        );
        assert!(
            harness
                .path()
                .join("projects/radiance/workflows/release.toml")
                .is_file(),
            "the file is not removed"
        );

        let after: Value = testing::get(&state, &addr, "/v1/workflows/release").await;
        assert_eq!(after["file"], before["file"]);
        assert_eq!(after["definition"], before["definition"]);
    }

    #[tokio::test]
    async fn save_then_read_returns_the_same_definition_with_its_positions() {
        let (_dir, state, addr, harness) = started().await;
        let response = put(&state, &addr, "release", json!({ "file": WORKFLOW })).await;
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(status, 200, "{body}");
        let saved: Value = serde_json::from_str(&body).unwrap();
        assert!(saved["commit"].is_string(), "{body}");

        let shown: Value = testing::get(&state, &addr, "/v1/workflows/release").await;
        assert_eq!(shown["name"], "release");
        assert_eq!(shown["definition"]["name"], "release");
        assert_eq!(shown["definition"]["nodes"][0]["position"]["x"], 12.5);
        assert_eq!(shown["definition"]["nodes"][0]["position"]["y"], 34.0);

        let expected =
            definition::WorkflowDefinition::parse(WORKFLOW, "workflows/release.toml").unwrap();
        assert_eq!(shown["definition"], serde_json::to_value(expected).unwrap());

        let listed: Value = testing::get(&state, &addr, "/v1/workflows").await;
        assert_eq!(
            listed,
            json!([{
                "project": 1,
                "project_name": "radiance",
                "name": "release",
                "description": "Build the change and run its tests.",
                "trigger_kind": "test",
                "node_count": 2,
                "last_run": null,
            }])
        );

        assert!(
            log(&harness).contains("Add the workflow release"),
            "{}",
            log(&harness)
        );
    }

    #[tokio::test]
    async fn a_definition_can_be_saved_as_its_parsed_form() {
        let (_dir, state, addr, _harness) = started().await;
        let definition =
            definition::WorkflowDefinition::parse(WORKFLOW, "workflows/release.toml").unwrap();
        let response = put(
            &state,
            &addr,
            "release",
            json!({ "definition": definition }),
        )
        .await;
        assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
        let shown: Value = testing::get(&state, &addr, "/v1/workflows/release").await;
        assert_eq!(shown["definition"]["nodes"][1]["position"]["y"], 4.5);
    }

    #[tokio::test]
    async fn the_resume_route_continues_a_run_with_the_value_it_waited_on() {
        let (_dir, state, addr, harness) = started().await;
        let value = json!({"change": 7});
        let run = waiting(&state, &harness, value.clone()).await;

        let response = resume(&state, &addr, run, json!({ "decision": "approve" })).await;
        answered(response, run).await;
        let ended = ended(&state, run).await;

        assert_eq!(ended.status, "succeeded");
        assert_eq!(left_the_wait(&state, run), value);
        assert_eq!(
            state.app.store.inbox_for_workflow_run(run).unwrap(),
            None,
            "the decision resolves the inbox item"
        );
    }

    #[tokio::test]
    async fn the_resume_route_continues_with_the_persons_value_after_an_edit() {
        let (_dir, state, addr, harness) = started().await;
        let run = waiting(&state, &harness, json!({"change": 7})).await;
        let shipped = json!({"change": 8});

        let response = resume(
            &state,
            &addr,
            run,
            json!({ "decision": "edit", "value": shipped }),
        )
        .await;
        answered(response, run).await;
        let ended = ended(&state, run).await;

        assert_eq!(ended.status, "succeeded");
        assert_eq!(left_the_wait(&state, run), shipped);
    }

    #[tokio::test]
    async fn the_resume_route_fails_the_run_with_the_reason_of_a_rejection() {
        let (_dir, state, addr, harness) = started().await;
        let run = waiting(&state, &harness, json!({"change": 7})).await;

        let response = resume(
            &state,
            &addr,
            run,
            json!({ "decision": "reject", "reason": "not this week" }),
        )
        .await;
        answered(response, run).await;
        let ended = ended(&state, run).await;

        assert_eq!(ended.status, "failed");
        assert_eq!(ended.error.as_deref(), Some("not this week"));
    }

    #[tokio::test]
    async fn the_resume_route_refuses_a_request_it_cannot_take_or_a_second_decision() {
        let (_dir, state, addr, harness) = started().await;
        let run = waiting(&state, &harness, json!({"change": 7})).await;

        let refused = resume(
            &state,
            &addr,
            run,
            json!({ "decision": "approve", "unexpected": true }),
        )
        .await;
        assert_eq!(refused.status(), 422, "unknown keys are refused");

        let refused = resume(&state, &addr, run, json!({ "decision": "maybe" })).await;
        assert_eq!(refused.status(), 422, "a decision is one of the three");

        let refused = resume(&state, &addr, run, json!({ "decision": "edit" })).await;
        assert_eq!(refused.status(), 422, "an edit needs a value");

        let response = resume(&state, &addr, run, json!({ "decision": "approve" })).await;
        answered(response, run).await;
        assert_eq!(ended(&state, run).await.status, "succeeded");

        let refused = resume(
            &state,
            &addr,
            run,
            json!({ "decision": "reject", "reason": "too late" }),
        )
        .await;
        assert_eq!(
            refused.status(),
            409,
            "a second decision on a finished wait is refused"
        );

        let refused = resume(
            &state,
            &addr,
            run + 10_000,
            json!({ "decision": "approve" }),
        )
        .await;
        assert_eq!(refused.status(), 404, "no such run");
    }

    #[tokio::test]
    async fn delete_is_refused_while_a_run_is_queued_or_running_and_leaves_a_commit_after() {
        let (_dir, state, addr, harness) = started().await;
        put(&state, &addr, "release", json!({ "file": WORKFLOW })).await;
        let repository = state.app.store.repositories().unwrap().first().unwrap().id;
        let run = state
            .app
            .store
            .add_project_workflow_run(1, "release", repository, "test", &json!({}))
            .unwrap();

        let response = delete(&state, &addr, "release").await;
        assert_eq!(response.status(), 409);
        assert!(
            harness
                .path()
                .join("projects/radiance/workflows/release.toml")
                .is_file()
        );

        state
            .app
            .store
            .set_workflow_run_status(run.id, "running")
            .unwrap();
        let response = delete(&state, &addr, "release").await;
        assert_eq!(response.status(), 409);

        state
            .app
            .store
            .finish_workflow_run(run.id, "succeeded", None)
            .unwrap();
        let response = delete(&state, &addr, "release").await;
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(status, 200, "{body}");
        let deleted: Value = serde_json::from_str(&body).unwrap();
        assert!(deleted["commit"].is_string(), "{body}");

        let shown = get_response(&state, &addr, "/v1/workflows/release").await;
        assert_eq!(shown.status(), 404);
        assert!(
            !harness
                .path()
                .join("projects/radiance/workflows/release.toml")
                .is_file()
        );
        assert!(
            log(&harness).contains("Delete the workflow release"),
            "{}",
            log(&harness)
        );
    }

    #[tokio::test]
    async fn a_queued_run_is_cancelled_by_the_route_and_refuses_a_second_cancel() {
        let (_dir, state, addr, _harness) = started().await;
        let repository = state.app.store.repositories().unwrap().first().unwrap().id;
        let run = state
            .app
            .store
            .add_workflow_run("release", repository, "test", &json!({}))
            .unwrap();

        let response = post(
            &state,
            &addr,
            &format!("/v1/workflows/runs/{}/cancel", run.id),
        )
        .await;
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(status, 200, "{body}");
        let cancelled: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(cancelled["id"], run.id);
        assert_eq!(cancelled["status"], "cancelled");
        assert!(cancelled["started_at"].is_null(), "{body}");
        assert_eq!(cancelled["error"], "cancelled by a person");
        assert_eq!(
            state
                .app
                .store
                .workflow_run(run.id)
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );

        let response = post(
            &state,
            &addr,
            &format!("/v1/workflows/runs/{}/cancel", run.id),
        )
        .await;
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(status, 409, "{body}");
        assert!(
            body.contains("has already ended as cancelled"),
            "the refusal says what ended: {body}"
        );
    }

    #[tokio::test]
    async fn a_finished_run_and_an_unknown_one_answer_the_cancel_with_an_error() {
        let (_dir, state, addr, _harness) = started().await;
        let repository = state.app.store.repositories().unwrap().first().unwrap().id;
        let run = state
            .app
            .store
            .add_workflow_run("release", repository, "test", &json!({}))
            .unwrap();
        state
            .app
            .store
            .finish_workflow_run(run.id, "succeeded", None)
            .unwrap();

        let response = post(
            &state,
            &addr,
            &format!("/v1/workflows/runs/{}/cancel", run.id),
        )
        .await;
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(status, 409, "{body}");
        assert!(
            body.contains("has already ended as succeeded"),
            "the refusal says what ended: {body}"
        );

        let response = post(&state, &addr, "/v1/workflows/runs/9999/cancel").await;
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn a_waiting_run_leaves_its_inbox_item_withdrawn_when_the_route_cancels_it() {
        let (_dir, state, addr, _harness) = started().await;
        let repository = state.app.store.repositories().unwrap().first().unwrap().id;
        let run = state
            .app
            .store
            .add_workflow_run("release", repository, "test", &json!({}))
            .unwrap();
        state
            .app
            .store
            .set_workflow_run_status(run.id, "waiting")
            .unwrap();
        let item = state
            .app
            .store
            .add_inbox_item(
                "workflow_approval",
                None,
                "The run waits for a person",
                &json!({ "run": run.id }),
            )
            .unwrap();

        let response = post(
            &state,
            &addr,
            &format!("/v1/workflows/runs/{}/cancel", run.id),
        )
        .await;
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(status, 200, "{body}");

        let open = state.app.store.inbox().unwrap();
        let open_ids = open.iter().map(|item| item.id).collect::<Vec<_>>();
        assert!(
            !open_ids.contains(&item.id),
            "the run's inbox item is still open: {open_ids:?}"
        );
    }

    /// The keys of `real` are the keys of `fixture`, all the way down; a null on either side stands for anything, and
    /// the keys that hold a value of the workflow's own making are not looked into.
    fn same_keys(real: &Value, fixture: &Value, at: &str) {
        const FREE: [&str; 5] = ["value", "input", "output", "asks", "params"];
        match (real, fixture) {
            (Value::Null, _) | (_, Value::Null) => {}
            (Value::Object(real), Value::Object(fixture)) => {
                let keys = |map: &serde_json::Map<String, Value>| {
                    map.keys()
                        .cloned()
                        .collect::<std::collections::BTreeSet<_>>()
                };
                assert_eq!(keys(real), keys(fixture), "the keys of {at}");
                for (key, value) in real {
                    if !FREE.contains(&key.as_str()) {
                        same_keys(value, &fixture[key], &format!("{at}.{key}"));
                    }
                }
            }
            (Value::Array(real), Value::Array(fixture)) => {
                if let (Some(real), Some(fixture)) = (real.first(), fixture.first()) {
                    same_keys(real, fixture, &format!("{at}[]"));
                }
            }
            (real, fixture) => assert_eq!(
                std::mem::discriminant(real),
                std::mem::discriminant(fixture),
                "the type of {at}"
            ),
        }
    }

    #[tokio::test]
    async fn the_answers_have_the_shape_of_the_fixtures_the_web_app_is_tested_with() {
        let fixtures: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/src/api/fixtures/workflows.json"
        )))
        .unwrap();
        let (_dir, state, addr, harness) = started().await;
        assert_eq!(
            put(&state, &addr, "release", json!({ "file": WORKFLOW }))
                .await
                .status(),
            200
        );
        let run = waiting(&state, &harness, json!({ "version": "1.0.0" })).await;

        let list: Value = testing::get(&state, &addr, "/v1/workflows").await;
        let release = list
            .as_array()
            .unwrap()
            .iter()
            .find(|each| each["name"] == "release")
            .unwrap();
        same_keys(release, &fixtures["list"][0], "list");
        let asking = list
            .as_array()
            .unwrap()
            .iter()
            .find(|each| each["name"] == "asking")
            .unwrap();
        assert_eq!(asking["last_run"]["id"], run);
        assert_eq!(asking["last_run"]["status"], "waiting");
        same_keys(asking, &fixtures["list"][0], "list with a run");
        assert_eq!(
            release["last_run"],
            Value::Null,
            "a workflow that never ran"
        );

        let shown: Value = testing::get(&state, &addr, "/v1/workflows/release").await;
        same_keys(&shown, &fixtures["show"], "show");

        let runs: Value = testing::get(&state, &addr, "/v1/workflow-runs?workflow=asking").await;
        assert_eq!(runs.as_array().unwrap().len(), 1);
        same_keys(&runs[0], &fixtures["runs"][0], "runs");
        let none: Value = testing::get(&state, &addr, "/v1/workflow-runs?workflow=release").await;
        assert_eq!(none, json!([]));

        let detail: Value = testing::get(&state, &addr, &format!("/v1/workflow-runs/{run}")).await;
        same_keys(&detail, &fixtures["run"], "run");
        assert_eq!(detail["waiting"]["node"], "ask");
        assert_eq!(detail["nodes"][0]["kind"], "approval");
        assert_eq!(detail["trigger"], "test");
    }

    #[tokio::test]
    async fn a_workflow_or_run_that_does_not_exist_is_not_found_without_the_operating_systems_words()
     {
        let (_dir, state, addr, _harness) = started().await;
        for path in ["/v1/workflows/nothing", "/v1/workflow-runs/999"] {
            let response = get_response(&state, &addr, path).await;
            assert_eq!(response.status(), 404, "{path}");
            let body = response.text().await.unwrap();
            assert!(!body.contains("os error"), "{body}");
            assert!(!body.contains("cannot be read"), "{body}");
        }
    }

    async fn send(
        state: &ApiState,
        addr: &str,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> reqwest::Response {
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), format!("http://{addr}{path}"))
            .bearer_auth(&*state.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        request.send().await.unwrap()
    }

    /// The text of the release workflow with another description, so that two projects' files differ.
    fn release_saying(description: &str) -> Value {
        json!({ "file": WORKFLOW.replace("Build the change and run its tests.", description) })
    }

    #[tokio::test]
    async fn a_workflow_is_saved_read_listed_and_deleted_in_its_project_and_names_repeat() {
        let (_dir, state, addr, harness) = started().await;
        let radiance = state
            .app
            .store
            .project_by_name("radiance")
            .unwrap()
            .unwrap()
            .id;
        let other = state.app.add_project("Other", "OTH").unwrap().id;

        let saved = send(
            &state,
            &addr,
            "PUT",
            &format!("/v1/projects/{radiance}/workflows/release"),
            Some(release_saying("In radiance.")),
        )
        .await;
        assert_eq!(saved.status(), 200);
        let saved: Value = saved.json().await.unwrap();
        assert_eq!(
            (saved["project"].as_i64(), saved["project_name"].as_str()),
            (Some(radiance), Some("radiance"))
        );
        assert!(saved["commit"].is_string());
        let saved = send(
            &state,
            &addr,
            "PUT",
            &format!("/v1/projects/{other}/workflows/release"),
            Some(release_saying("In other.")),
        )
        .await;
        assert_eq!(saved.status(), 200);
        assert!(
            harness
                .path()
                .join("projects/Other/workflows/release.toml")
                .is_file()
        );

        // Each project sees its own file of that name.
        for (project, says) in [(radiance, "In radiance."), (other, "In other.")] {
            let shown: Value = testing::get(
                &state,
                &addr,
                &format!("/v1/projects/{project}/workflows/release"),
            )
            .await;
            assert_eq!(shown["definition"]["description"], says);
            assert_eq!(shown["project"], project);
            let listed: Value =
                testing::get(&state, &addr, &format!("/v1/projects/{project}/workflows")).await;
            assert_eq!(listed.as_array().unwrap().len(), 1);
            assert_eq!(listed[0]["description"], says);
            assert_eq!(listed[0]["project"], project);
        }
        // The deprecated aggregate lists both, each with its project.
        let all: Value = testing::get(&state, &addr, "/v1/workflows").await;
        assert_eq!(all.as_array().unwrap().len(), 2);

        // A body that names another project than the address is refused; nothing is written.
        let mismatch = send(
            &state,
            &addr,
            "PUT",
            &format!("/v1/projects/{radiance}/workflows/extra"),
            Some(json!({ "file": WORKFLOW, "project": other })),
        )
        .await;
        assert_eq!(mismatch.status(), 422);
        assert!(
            !harness
                .path()
                .join("projects/radiance/workflows/extra.toml")
                .exists()
        );

        // Deleting one leaves the other, and a project that has none of that name does not find it.
        let deleted = send(
            &state,
            &addr,
            "DELETE",
            &format!("/v1/projects/{other}/workflows/release"),
            None,
        )
        .await;
        assert_eq!(deleted.status(), 200);
        let deleted: Value = deleted.json().await.unwrap();
        assert_eq!(deleted["project"], other);
        assert_eq!(
            send(
                &state,
                &addr,
                "GET",
                &format!("/v1/projects/{other}/workflows/release"),
                None
            )
            .await
            .status(),
            404
        );
        assert_eq!(
            send(
                &state,
                &addr,
                "GET",
                &format!("/v1/projects/{radiance}/workflows/release"),
                None
            )
            .await
            .status(),
            200
        );
        // A project the server does not have is not found.
        assert_eq!(
            send(&state, &addr, "GET", "/v1/projects/999/workflows", None)
                .await
                .status(),
            404
        );
    }

    #[tokio::test]
    async fn the_deprecated_routes_find_the_one_project_that_has_the_name_and_name_the_choices_otherwise()
     {
        let (_dir, state, addr, harness) = started().await;
        let radiance = state
            .app
            .store
            .project_by_name("radiance")
            .unwrap()
            .unwrap()
            .id;
        let other = state.app.add_project("Other", "OTH").unwrap().id;

        // A name no project has goes to the oldest project that is not private, or to the project the body names.
        assert_eq!(
            put(&state, &addr, "solo", json!({ "file": WORKFLOW }))
                .await
                .status(),
            200
        );
        assert!(
            harness
                .path()
                .join("projects/radiance/workflows/solo.toml")
                .is_file()
        );
        let named = put(
            &state,
            &addr,
            "named",
            json!({ "file": WORKFLOW, "project": other }),
        )
        .await;
        assert_eq!(named.status(), 200);
        assert!(
            harness
                .path()
                .join("projects/Other/workflows/named.toml")
                .is_file()
        );

        // A unique name resolves and says its project.
        let shown: Value = testing::get(&state, &addr, "/v1/workflows/solo").await;
        assert_eq!(shown["project"], radiance);
        let shown: Value = testing::get(&state, &addr, "/v1/workflows/named").await;
        assert_eq!(shown["project"], other);
        assert_eq!(
            get_response(&state, &addr, "/v1/workflows/nothing")
                .await
                .status(),
            404
        );

        // Once both projects have `solo`, every route that names it alone answers 409 with the choices.
        assert_eq!(
            send(
                &state,
                &addr,
                "PUT",
                &format!("/v1/projects/{other}/workflows/solo"),
                Some(json!({ "file": WORKFLOW }))
            )
            .await
            .status(),
            200
        );
        for (method, path, body) in [
            ("GET", "/v1/workflows/solo", None),
            (
                "PUT",
                "/v1/workflows/solo",
                Some(json!({ "file": WORKFLOW })),
            ),
            ("DELETE", "/v1/workflows/solo", None),
            ("GET", "/v1/workflows/solo/run-inputs", None),
            ("POST", "/v1/workflows/solo/runs", Some(json!({}))),
        ] {
            let response = send(&state, &addr, method, path, body).await;
            assert_eq!(response.status(), 409, "{method} {path}");
            let message = response.text().await.unwrap();
            assert!(
                message.contains(&format!("radiance (id {radiance})"))
                    && message.contains(&format!("Other (id {other})"))
                    && message.contains("/v1/projects/{id}/workflows/solo"),
                "{message}"
            );
        }
        // Nothing was deleted by the ambiguous delete.
        assert!(
            harness
                .path()
                .join("projects/radiance/workflows/solo.toml")
                .is_file()
        );
        assert!(
            harness
                .path()
                .join("projects/Other/workflows/solo.toml")
                .is_file()
        );
    }

    /// A member of the organisation, a viewer and an owner who is not the first one, with their tokens.
    fn people(state: &ApiState) -> [String; 2] {
        use crate::server::api::TOKEN_PREFIX;
        let make = |handle: &str, role: crate::store::Role, n: usize| {
            let person = state.app.store.add_person(handle, handle, role).unwrap();
            let token = format!("{TOKEN_PREFIX}{n:064}");
            state
                .app
                .store
                .add_access_token(person.id, "test", &token)
                .unwrap();
            token
        };
        [
            make("mia", crate::store::Role::Member, 11),
            make("vic", crate::store::Role::Viewer, 12),
        ]
    }

    #[tokio::test]
    async fn only_an_owner_changes_a_projects_workflows_and_a_member_starts_runs() {
        let (_dir, state, addr, _harness) = started().await;
        let [member, viewer] = people(&state);
        let radiance = state
            .app
            .store
            .project_by_name("radiance")
            .unwrap()
            .unwrap()
            .id;
        let path = format!("/v1/projects/{radiance}/workflows/release");
        let call = |token: String, method: &'static str, path: String, body: Value| {
            let addr = addr.clone();
            async move {
                reqwest::Client::new()
                    .request(method.parse().unwrap(), format!("http://{addr}{path}"))
                    .bearer_auth(token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap()
                    .status()
                    .as_u16()
            }
        };
        let owner = state.token.to_string();
        // The owner saves it; a member and a viewer may not save or delete it.
        assert_eq!(
            call(
                owner.clone(),
                "PUT",
                path.clone(),
                json!({ "file": WORKFLOW })
            )
            .await,
            200
        );
        for token in [&member, &viewer] {
            assert_eq!(
                call(
                    token.clone(),
                    "PUT",
                    path.clone(),
                    json!({ "file": WORKFLOW })
                )
                .await,
                403
            );
            assert_eq!(
                call(token.clone(), "DELETE", path.clone(), json!({})).await,
                403
            );
            // Everyone reads.
            assert_eq!(
                call(token.clone(), "GET", path.clone(), json!({})).await,
                200
            );
            assert_eq!(
                call(
                    token.clone(),
                    "GET",
                    format!("/v1/projects/{radiance}/workflows"),
                    json!({})
                )
                .await,
                200
            );
        }
        // Starting a run needs a member: a viewer is refused, a member gets as far as the workflow's trigger.
        let runs = format!("{path}/runs");
        assert_eq!(
            call(viewer.clone(), "POST", runs.clone(), json!({})).await,
            403
        );
        assert_ne!(call(member.clone(), "POST", runs, json!({})).await, 403);
    }
}
