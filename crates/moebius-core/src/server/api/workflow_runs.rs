//! The runs of the workflows: reading them, and starting them by hand — the `manual` trigger names its inputs, and a
//! person starts each run giving them, through this door or the `moebius workflow run` command.
//!
//! - `GET /v1/workflow-runs?project=&workflow=&limit=`: the runs, newest first (of one project and one workflow when
//!   they are named; without `project`, of every project the person sees), leaving out the runs of a repository or a
//!   project the person does not see. A run says which
//!   project it belongs to (`project`, `project_name`).
//! - `GET /v1/workflow-runs/{id}`: one run with what each of its nodes was given, answered and failed with.
//! - `GET /v1/projects/{id}/workflows/{name}/run-inputs`: the inputs of the workflow's manual trigger, so a form can be
//!   built from them; a conflict when the trigger is not manual.
//! - `POST /v1/projects/{id}/workflows/{name}/runs` `{repository?, inputs?}`: starts a run of the workflow, checked
//!   against the trigger's inputs — a refusal names the input it refused, a default stands in for an input that was not
//!   given, and an input the trigger does not name is refused. The run belongs to the repository `repository` names, or
//!   to the one the trigger's `repository` input names when it has one; it must be one of the project's repositories
//!   and, left out, is the project's only one (see `App::workflow_run_repository`).
//!
//! The same two routes without the project, `GET /v1/workflows/{name}/run-inputs` and `POST
//! /v1/workflows/{name}/runs`, are deprecated: they find the one project that has a workflow of that name (`409` with
//! the choices when several do, see `workflows::owner_of`).

use std::collections::HashMap;

use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::workflows::{harness, owner_of};
use super::{ApiError, ApiState, blocking};
use crate::app::{App, AppError, Sight};
use crate::decide::Actor;
use crate::harness::repo::HarnessRepo;
use crate::store::{Project, WorkflowRun};
use crate::workflows::trigger::manual;

/// The routes that start runs of workflows by hand.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/v1/projects/{id}/workflows/{name}/run-inputs",
            get(project_run_inputs),
        )
        .route(
            "/v1/projects/{id}/workflows/{name}/runs",
            post(project_start),
        )
        .route("/v1/workflows/{name}/run-inputs", get(run_inputs))
        .route("/v1/workflows/{name}/runs", post(start))
        .route("/v1/workflow-runs", get(list_runs))
        .route("/v1/workflow-runs/{id}", get(show_run))
}

/// The most runs one list answers when the request names no limit.
const DEFAULT_RUNS: i64 = 50;

/// The most runs one list answers at all.
const MOST_RUNS: i64 = 200;

/// A run as the web app lists it: its place in the queue and its ends, without the worktree of the server's disk.
#[derive(Debug, Serialize)]
pub(super) struct RunView {
    id: i64,
    workflow: String,
    /// The project that owns the workflow; none for a run from before workflows belonged to projects.
    project: Option<i64>,
    /// Its name; none when the run has no project or the project has been deleted.
    project_name: Option<String>,
    repository_id: i64,
    trigger: String,
    status: String,
    value: Value,
    created_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    error: Option<String>,
}

impl RunView {
    /// `run`, with the name of its project taken from `names`, the projects by id.
    fn new(run: WorkflowRun, names: &HashMap<i64, String>) -> Self {
        Self {
            id: run.id,
            workflow: run.workflow,
            project_name: run.project_id.and_then(|id| names.get(&id).cloned()),
            project: run.project_id,
            repository_id: run.repository_id,
            trigger: run.trigger,
            status: run.status,
            value: run.value,
            created_at: run.created_at,
            started_at: run.started_at,
            finished_at: run.finished_at,
            error: run.error,
        }
    }
}

/// The names of the projects the server has, by id.
fn project_names(app: &App) -> Result<HashMap<i64, String>, AppError> {
    Ok(app
        .store
        .projects()?
        .into_iter()
        .map(|project| (project.id, project.name))
        .collect())
}

/// One node of a run at one attempt.
#[derive(Debug, Serialize)]
struct NodeView {
    /// The id the node has in the workflow.
    node: String,
    /// What the node does; empty when the workflow no longer has the node.
    kind: String,
    attempt: i64,
    status: String,
    input: Value,
    output: Option<Value>,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    error: Option<String>,
}

/// A run that stopped on a node and asks for a person.
#[derive(Debug, Serialize)]
struct Waiting {
    node: String,
    /// What the node was given, which is what it asks about.
    asks: Value,
}

/// What `GET /v1/workflow-runs/{id}` answers.
#[derive(Debug, Serialize)]
struct RunDetail {
    #[serde(flatten)]
    run: RunView,
    nodes: Vec<NodeView>,
    /// Where and on what the run waits, when its status is `waiting`.
    waiting: Option<Waiting>,
}

#[derive(Debug, Deserialize)]
struct RunsQuery {
    #[serde(default)]
    project: Option<i64>,
    #[serde(default)]
    workflow: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
}

async fn list_runs(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    axum::extract::Query(query): axum::extract::Query<RunsQuery>,
) -> Result<axum::Json<Vec<RunView>>, ApiError> {
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        let limit = query.limit.unwrap_or(DEFAULT_RUNS).clamp(1, MOST_RUNS);
        let names = project_names(app)?;
        Ok(
            visible_runs(app, &sight, query.project, query.workflow.as_deref(), limit)?
                .into_iter()
                .map(|run| RunView::new(run, &names))
                .collect(),
        )
    })
    .await
}

/// The newest runs of `workflow` of `project` (of all with none) in repositories and projects `sight` sees, at most
/// `limit`.
pub(super) fn visible_runs(
    app: &App,
    sight: &Sight,
    project: Option<i64>,
    workflow: Option<&str>,
    limit: i64,
) -> Result<Vec<WorkflowRun>, AppError> {
    // A run of a repository or a project the person does not see is skipped, so the store is read page by page until
    // `limit` visible runs are found or the runs are all read.
    let wanted = limit as usize;
    let mut found = Vec::new();
    let mut before = None;
    while found.len() < wanted {
        let page = app
            .store
            .project_workflow_runs(project, workflow, before, MOST_RUNS)?;
        let Some(last) = page.last() else { break };
        before = Some(last.id);
        let full = page.len() as i64 == MOST_RUNS;
        found.extend(page.into_iter().filter(|run| sees_run(sight, run)));
        if !full {
            break;
        }
    }
    found.truncate(wanted);
    Ok(found)
}

/// Whether `sight` sees `run`: its repository, and its project when it has one.
pub(super) fn sees_run(sight: &Sight, run: &WorkflowRun) -> bool {
    sight.repository(run.repository_id)
        && run.project_id.is_none_or(|project| sight.project(project))
}

async fn show_run(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<RunDetail>, ApiError> {
    let repo = harness(&state).ok();
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        let run = app
            .store
            .workflow_run(id)?
            .filter(|run| sees_run(&sight, run))
            .ok_or_else(|| AppError::NotFound(format!("workflow run {id}")))?;
        // The definition is the one in the folder of the run's own project; a run from before the migration has none.
        let kinds: HashMap<String, String> = repo
            .zip(
                run.project_id
                    .and_then(|project| app.store.project(project).ok().flatten()),
            )
            .and_then(|(repo, project)| repo.project_workflow(&project.name, &run.workflow).ok())
            .map(|definition| {
                definition
                    .nodes
                    .into_iter()
                    .map(|node| (node.id, node.kind))
                    .collect()
            })
            .unwrap_or_default();
        let rows = app.store.workflow_node_runs(id)?;
        let waiting = if run.status == "waiting" {
            rows.iter()
                .rev()
                .find(|row| row.status == "waiting")
                .map(|row| Waiting {
                    node: row.node_id.clone(),
                    asks: row.input.clone(),
                })
        } else {
            None
        };
        let nodes = rows
            .into_iter()
            .map(|row| NodeView {
                kind: kinds.get(&row.node_id).cloned().unwrap_or_default(),
                node: row.node_id,
                attempt: row.attempt,
                status: row.status,
                input: row.input,
                output: row.output,
                started_at: row.started_at,
                finished_at: row.finished_at,
                error: row.error,
            })
            .collect();
        Ok(RunDetail {
            run: RunView::new(run, &project_names(app)?),
            nodes,
            waiting,
        })
    })
    .await
}

/// What `GET .../run-inputs` answers: the inputs a form builds itself from.
#[derive(Debug, Serialize)]
struct RunInputs {
    /// Always `manual`, the trigger of the workflow.
    trigger_kind: String,
    /// The inputs in the order the trigger's file names them.
    inputs: Vec<manual::Input>,
}

/// What `POST .../runs` takes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartRequest {
    /// The repository the run belongs to, by name: not taken when the trigger names one of its inputs `repository`.
    #[serde(default)]
    repository: Option<String>,
    /// The inputs a person gave, by the names the trigger names them.
    #[serde(default)]
    inputs: Map<String, Value>,
}

/// What starting a run answers: the run as it was recorded, with its trigger and the value it starts with.
#[derive(Debug, Serialize)]
struct Started {
    id: i64,
    workflow: String,
    project: i64,
    project_name: String,
    /// The name of the repository the run belongs to.
    repository: String,
    /// The kind of the trigger that started the run, `manual` through this door.
    trigger: String,
    status: String,
    /// The value the run starts with: the inputs given, with the defaults standing in.
    value: Value,
}

/// The manual trigger of the workflow `name` of `project`, read from its file.
fn manual_trigger(
    repo: &HarnessRepo,
    project: &Project,
    name: &str,
) -> Result<(String, manual::ManualTrigger), AppError> {
    let definition = repo.project_workflow(&project.name, name)?;
    let trigger = definition
        .trigger
        .first()
        .cloned()
        .ok_or_else(|| AppError::Conflict(format!("the workflow {name:?} has no trigger")))?;
    if trigger.kind != manual::KIND {
        return Err(AppError::Conflict(format!(
            "the workflow's trigger is {:?}, not manual",
            trigger.kind
        )));
    }
    let manual = manual::ManualTrigger::parse(&trigger.params)
        .map_err(|message| AppError::Invalid(format!("trigger: {message}")))?;
    Ok((trigger.kind, manual))
}

/// The inputs a person gave, checked against the trigger of the workflow, the repository the run belongs to, and the
/// run as it was recorded.
fn start_manual(
    app: &std::sync::Arc<App>,
    repo: &HarnessRepo,
    project: &Project,
    name: &str,
    request: &StartRequest,
    sight: &Sight,
) -> Result<Started, AppError> {
    let (_, manual) = manual_trigger(repo, project, name)?;
    let value = manual.resolve(&request.inputs).map_err(AppError::Invalid)?;
    // A run works only in repositories the person sees (see `app::projects::sight`): neither the one it belongs to nor
    // one any other input names, with its default, may be of a private project they are no member of. Such a
    // repository is answered as if it did not exist.
    let repositories = app.store.repositories()?;
    let named = manual
        .inputs
        .iter()
        .filter(|input| input.ty == manual::InputType::Repository)
        .filter_map(|input| value.get(&input.name).and_then(Value::as_str))
        .chain(request.repository.as_deref());
    for named in named {
        if repositories
            .iter()
            .any(|r| r.name == named && !sight.repository(r.id))
        {
            return Err(AppError::NotFound(format!("repository {named}")));
        }
    }

    // The run belongs to the repository the person named, or to the one the trigger's `repository` input named; left
    // out, to the project's only repository (see `App::workflow_run_repository`).
    let requested = match manual
        .inputs
        .iter()
        .find(|input| input.ty == manual::InputType::Repository)
    {
        Some(input) => {
            if request.repository.is_some() {
                return Err(AppError::Invalid(format!(
                    "the trigger's input {:?} names the run's repository, so \"repository\" is not taken",
                    input.name
                )));
            }
            value
                .get(&input.name)
                .and_then(Value::as_str)
                .map(str::to_string)
        }
        None => request.repository.clone(),
    };
    let run = app.start_project_workflow_run(project.id, name, requested.as_deref(), value)?;
    let repository = app.repository(run.repository_id)?;
    Ok(Started {
        id: run.id,
        workflow: run.workflow,
        project: project.id,
        project_name: project.name.clone(),
        repository: repository.name,
        trigger: run.trigger,
        status: run.status,
        value: run.value,
    })
}

fn inputs_of(repo: &HarnessRepo, project: &Project, name: &str) -> Result<RunInputs, AppError> {
    let (trigger_kind, manual) = manual_trigger(repo, project, name)?;
    Ok(RunInputs {
        trigger_kind,
        inputs: manual.inputs,
    })
}

async fn project_run_inputs(
    State(state): State<ApiState>,
    UrlPath((project, name)): UrlPath<(i64, String)>,
) -> Result<axum::Json<RunInputs>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |app| {
        inputs_of(&repo, &app.project(project)?, &name)
    })
    .await
}

/// Deprecated: the inputs of the workflow `name` of the one project that has it.
async fn run_inputs(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<RunInputs>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |app| {
        let project = owner_of(app, &app.sight(&by)?, &repo, &name)?;
        inputs_of(&repo, &project, &name)
    })
    .await
}

/// Refuses, as if it did not exist, a repository the request names that `sight` does not see. It is asked before
/// anything about the workflow is, so that nothing says whether a workflow of a private project exists.
fn check_named(app: &App, sight: &Sight, request: &StartRequest) -> Result<(), AppError> {
    let named = [
        request.repository.as_deref(),
        request.inputs.get("repository").and_then(Value::as_str),
    ];
    for named in named.into_iter().flatten() {
        if app
            .store
            .repositories()?
            .iter()
            .any(|r| r.name == named && !sight.repository(r.id))
        {
            return Err(AppError::NotFound(format!("repository {named}")));
        }
    }
    Ok(())
}

/// Starts the run a person asked for in `project`; every repository an input names is checked once the inputs are
/// read (`start_manual`).
fn start_in(
    app: &std::sync::Arc<App>,
    repo: &HarnessRepo,
    sight: &Sight,
    project: &Project,
    name: &str,
    request: &StartRequest,
) -> Result<Started, AppError> {
    start_manual(app, repo, project, name, request, sight)
}

async fn project_start(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((project, name)): UrlPath<(i64, String)>,
    axum::Json(request): axum::Json<StartRequest>,
) -> Result<axum::Json<Started>, ApiError> {
    let repo = harness(&state);
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        check_named(app, &sight, &request)?;
        let project = app.project(project)?;
        let repo = repo.map_err(|error| AppError::Conflict(error.1))?;
        start_in(app, &repo, &sight, &project, &name, &request)
    })
    .await
}

/// Deprecated: starts a run of the workflow `name` of the one project that has it.
async fn start(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
    axum::Json(request): axum::Json<StartRequest>,
) -> Result<axum::Json<Started>, ApiError> {
    let repo = harness(&state);
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        check_named(app, &sight, &request)?;
        let repo = repo.map_err(|error| AppError::Conflict(error.1))?;
        let project = owner_of(app, &sight, &repo, &name)?;
        start_in(app, &repo, &sight, &project, &name, &request)
    })
    .await
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

    const WORKFLOW: &str = r#"
name = "release"
description = "Started by a person."

[[trigger]]
kind = "manual"
params = { inputs = [
    { name = "version", type = "text", required = true },
    { name = "count", type = "number", default = 3 },
    { name = "dry_run", type = "boolean" },
    { name = "environment", type = "choice", choices = ["main", "staging"] },
] }

[[nodes]]
id = "build"
kind = "test"

[[edges]]
from = "trigger"
to = "build"
port = "out"
"#;

    const REPOSITORY_WORKFLOW: &str = r#"
name = "release"
description = "Started by a person, in the repository it names."

[[trigger]]
kind = "manual"
params = { inputs = [{ name = "repository", type = "repository", required = true }] }

[[nodes]]
id = "build"
kind = "test"

[[edges]]
from = "trigger"
to = "build"
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

    async fn save(state: &ApiState, addr: &str, file: &str) {
        let response = reqwest::Client::new()
            .put(format!("http://{addr}/v1/workflows/release"))
            .bearer_auth(&*state.token)
            .json(&json!({ "file": file }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
    }

    async fn start_run(state: &ApiState, addr: &str, body: Value) -> (reqwest::StatusCode, String) {
        let response = reqwest::Client::new()
            .post(format!("http://{addr}/v1/workflows/release/runs"))
            .bearer_auth(&*state.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        (response.status(), response.text().await.unwrap())
    }

    #[tokio::test]
    async fn the_run_inputs_route_reports_the_inputs_a_form_builds_from() {
        let (_dir, state, addr, _harness) = started().await;
        save(&state, &addr, WORKFLOW).await;

        let inputs: Value = testing::get(&state, &addr, "/v1/workflows/release/run-inputs").await;
        assert_eq!(inputs["trigger_kind"], "manual");
        assert_eq!(
            inputs["inputs"],
            json!([
                { "name": "version", "type": "text", "required": true },
                { "name": "count", "type": "number", "required": false, "default": 3 },
                { "name": "dry_run", "type": "boolean", "required": false },
                { "name": "environment", "type": "choice", "required": false, "choices": ["main", "staging"] },
            ])
        );
    }

    #[tokio::test]
    async fn a_run_starts_with_the_inputs_given_and_is_recorded_with_manual_as_its_trigger() {
        let (_dir, state, addr, _harness) = started().await;
        save(&state, &addr, WORKFLOW).await;

        let (status, body) = start_run(
            &state,
            &addr,
            json!({
                "repository": "radiance",
                "inputs": {
                    "version": "1.2.3",
                    "dry_run": false,
                    "environment": "main",
                },
            }),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        let started: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(started["trigger"], "manual");
        assert_eq!(started["repository"], "radiance");
        assert_eq!(
            started["value"],
            json!({
                "version": "1.2.3",
                "count": 3,
                "dry_run": false,
                "environment": "main",
            }),
            "the default stands in for the input not given"
        );

        let stored = state
            .app
            .store
            .workflow_run(started["id"].as_i64().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(stored.workflow, "release");
        assert_eq!(stored.trigger, "manual");
        assert_eq!(
            stored.value,
            json!({
                "version": "1.2.3",
                "count": 3,
                "dry_run": false,
                "environment": "main",
            })
        );
    }

    #[tokio::test]
    async fn a_value_that_is_not_the_type_of_its_input_is_refused_naming_the_input() {
        let (_dir, state, addr, _harness) = started().await;
        save(&state, &addr, WORKFLOW).await;
        for (input, value) in [
            ("version", json!(3)),
            ("count", json!("3")),
            ("dry_run", json!("false")),
            ("environment", json!("staged")),
        ] {
            let (status, body) = start_run(
                &state,
                &addr,
                json!({ "repository": "radiance", "inputs": { input: value } }),
            )
            .await;
            assert_eq!(status, 422, "{input}: {body}");
            assert!(
                body.contains(input),
                "the refusal names the input it refused: {body}"
            );
        }
    }

    #[tokio::test]
    async fn a_missing_required_input_is_refused_naming_it() {
        let (_dir, state, addr, _harness) = started().await;
        save(&state, &addr, WORKFLOW).await;

        let (status, body) = start_run(&state, &addr, json!({ "repository": "radiance" })).await;
        assert_eq!(status, 422, "{body}");
        assert!(body.contains("\"version\""), "{body}");
        assert!(body.contains("required"), "{body}");
    }

    #[tokio::test]
    async fn a_default_stands_in_for_an_input_that_was_not_given() {
        let (_dir, state, addr, _harness) = started().await;
        save(&state, &addr, WORKFLOW).await;

        let (status, body) = start_run(
            &state,
            &addr,
            json!({ "repository": "radiance", "inputs": { "version": "1.2.3" } }),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        let started: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(started["value"]["count"], 3);
    }

    #[tokio::test]
    async fn an_input_the_trigger_does_not_name_is_refused_naming_it() {
        let (_dir, state, addr, _harness) = started().await;
        save(&state, &addr, WORKFLOW).await;

        let (status, body) = start_run(
            &state,
            &addr,
            json!({ "repository": "radiance", "inputs": { "version": "1.2.3", "bogus": 1 } }),
        )
        .await;
        assert_eq!(status, 422, "{body}");
        assert!(body.contains("\"bogus\""), "{body}");
    }

    #[tokio::test]
    async fn the_repository_of_the_run_is_checked_and_refused_naming_it() {
        let (_dir, state, addr, _harness) = started().await;
        save(&state, &addr, WORKFLOW).await;

        // Without a repository, the run belongs to the only repository of its project.
        let (status, body) =
            start_run(&state, &addr, json!({ "inputs": { "version": "1.2.3" } })).await;
        assert_eq!(status, 200, "{body}");
        let started: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(started["repository"], "radiance");
        assert_eq!(started["project_name"], "radiance");

        // With a repository that is not one of the project's, the refusal lists the choices.
        let (status, body) = start_run(
            &state,
            &addr,
            json!({ "repository": "nobody", "inputs": { "version": "1.2.3" } }),
        )
        .await;
        assert_eq!(status, 422, "{body}");
        assert!(
            body.contains("\"nobody\" is not one of the repositories of the project radiance")
                && body.contains("one of: radiance"),
            "{body}"
        );
    }

    #[tokio::test]
    async fn a_repository_input_names_the_repository_the_run_belongs_to() {
        let (_dir, state, addr, _harness) = started().await;
        save(&state, &addr, REPOSITORY_WORKFLOW).await;

        // The input alone names the repository, and the run is recorded in it.
        let (status, body) = start_run(
            &state,
            &addr,
            json!({ "inputs": { "repository": "radiance" } }),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        let started: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(started["repository"], "radiance");
        assert_eq!(started["value"], json!({ "repository": "radiance" }));

        // A repository the server does not know is refused, naming the input.
        let (status, body) = start_run(
            &state,
            &addr,
            json!({ "inputs": { "repository": "nobody" } }),
        )
        .await;
        assert_eq!(status, 422, "{body}");
        assert!(body.contains("nobody"), "{body}");

        // And the "repository" key is not taken at the same time.
        let (status, body) = start_run(
            &state,
            &addr,
            json!({ "repository": "radiance", "inputs": { "repository": "radiance" } }),
        )
        .await;
        assert_eq!(status, 422, "{body}");
        assert!(body.contains("input"), "{body}");
    }

    #[tokio::test]
    async fn a_trigger_that_is_not_manual_is_a_conflict_not_a_refusal() {
        let (_dir, state, addr, _harness) = started().await;
        save(
            &state,
            &addr,
            "name = \"release\"\ndescription = \"Nothing to do yet.\"\n\n[[trigger]]\nkind = \"test\"\n",
        )
        .await;

        let (status, body) = start_run(&state, &addr, json!({})).await;
        assert_eq!(status, 409, "{body}");
        assert!(body.contains("not manual"), "{body}");

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/v1/workflows/release/run-inputs"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 409, "{}", response.text().await.unwrap());
    }

    #[tokio::test]
    async fn a_run_is_started_in_its_project_and_its_history_keeps_the_definition() {
        let (_dir, state, addr, _harness) = started().await;
        save(&state, &addr, WORKFLOW).await;
        let radiance = state
            .app
            .store
            .project_by_name("radiance")
            .unwrap()
            .unwrap()
            .id;
        let other = state.app.add_project("Other", "OTH").unwrap().id;
        let client = reqwest::Client::new();
        let call = |method: &str, path: String, body: Value| {
            client
                .request(method.parse().unwrap(), format!("http://{addr}{path}"))
                .bearer_auth(&*state.token)
                .json(&body)
                .send()
        };

        // The inputs are those of the workflow of the project that is named; another project has none of that name.
        let inputs = call(
            "GET",
            format!("/v1/projects/{radiance}/workflows/release/run-inputs"),
            json!({}),
        )
        .await
        .unwrap();
        assert_eq!(inputs.status(), 200);
        let missing = call(
            "GET",
            format!("/v1/projects/{other}/workflows/release/run-inputs"),
            json!({}),
        )
        .await
        .unwrap();
        assert_eq!(
            missing.status(),
            422,
            "a workflow the project does not have cannot be read"
        );

        let started = call(
            "POST",
            format!("/v1/projects/{radiance}/workflows/release/runs"),
            json!({ "inputs": { "version": "1.0.0" } }),
        )
        .await
        .unwrap();
        assert_eq!(started.status(), 200);
        let started: Value = started.json().await.unwrap();
        assert_eq!(
            (
                started["project"].as_i64(),
                started["project_name"].as_str()
            ),
            (Some(radiance), Some("radiance"))
        );
        let run = started["id"].as_i64().unwrap();

        // Its detail names the kind of each node of the definition in its project, and the list filters by project.
        testing::until("the run ends", || {
            state
                .app
                .store
                .workflow_run(run)
                .unwrap()
                .unwrap()
                .finished_at
                .is_some()
        })
        .await;
        let detail: Value = testing::get(&state, &addr, &format!("/v1/workflow-runs/{run}")).await;
        assert_eq!(detail["project"], radiance);
        assert_eq!(detail["project_name"], "radiance");
        assert_eq!(detail["nodes"][0]["kind"], "test");
        let listed: Value = testing::get(
            &state,
            &addr,
            &format!("/v1/workflow-runs?project={radiance}&workflow=release"),
        )
        .await;
        assert_eq!(listed.as_array().unwrap().len(), 1);
        assert_eq!(listed[0]["project"], radiance);
        let none: Value =
            testing::get(&state, &addr, &format!("/v1/workflow-runs?project={other}")).await;
        assert!(none.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_old_run_route_resolves_the_project_and_refuses_a_name_two_projects_have() {
        let (_dir, state, addr, _harness) = started().await;
        save(&state, &addr, WORKFLOW).await;
        let (status, body) =
            start_run(&state, &addr, json!({ "inputs": { "version": "1.0.0" } })).await;
        assert_eq!(status, 200, "{body}");
        let started: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(started["project_name"], "radiance");

        let other = state.app.add_project("Other", "OTH").unwrap().id;
        let saved = reqwest::Client::new()
            .put(format!(
                "http://{addr}/v1/projects/{other}/workflows/release"
            ))
            .bearer_auth(&*state.token)
            .json(&json!({ "file": WORKFLOW }))
            .send()
            .await
            .unwrap();
        assert_eq!(saved.status(), 200);
        let (status, body) = start_run(&state, &addr, json!({})).await;
        assert_eq!(status, 409, "{body}");
        assert!(body.contains("Other (id"), "{body}");
    }

    #[tokio::test]
    async fn the_list_without_a_project_holds_the_runs_of_the_projects_the_caller_sees() {
        let (_dir, state, addr) = testing::start().await;
        let store = &state.app.store;
        let repository = store.repositories().unwrap()[0].id;
        let open = state.app.add_project("Open", "OPN").unwrap().id;
        let secret = state.app.add_project("Secret", "SEC").unwrap().id;
        let shown = store
            .add_project_workflow_run(open, "release", repository, "manual", &json!({}))
            .unwrap()
            .id;
        let hidden = store
            .add_project_workflow_run(secret, "release", repository, "manual", &json!({}))
            .unwrap()
            .id;
        // A person who is no member of the private project: the caller is not the server's own token.
        let lea = store
            .add_person("lea", "Lea", crate::store::Role::Member)
            .unwrap();
        let token = format!("{}{:064}", crate::server::api::TOKEN_PREFIX, 31);
        store.add_access_token(lea.id, "test", &token).unwrap();
        store.set_project_access(secret, true, &[]).unwrap();

        let get = |path: String, token: String| {
            let addr = addr.clone();
            async move {
                reqwest::Client::new()
                    .get(format!("http://{addr}{path}"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap()
            }
        };
        let ids = |list: &Value| -> Vec<i64> {
            list.as_array()
                .unwrap()
                .iter()
                .map(|run| run["id"].as_i64().unwrap())
                .collect()
        };

        let list = get("/v1/workflow-runs".to_string(), token.clone()).await;
        assert_eq!(list.status(), 200);
        let list: Value = list.json().await.unwrap();
        assert_eq!(ids(&list), [shown], "the private project's run is left out");
        assert_eq!(list[0]["project_name"], "Open");
        // Named, the private project's runs are as absent as when it is not named; its run is no run for her.
        let named = get(format!("/v1/workflow-runs?project={secret}"), token.clone()).await;
        let named: Value = named.json().await.unwrap();
        assert!(named.as_array().unwrap().is_empty());
        let detail = get(format!("/v1/workflow-runs/{hidden}"), token.clone()).await;
        assert_eq!(detail.status(), 404);

        // Owners included, someone who is no member does not see a private project's runs (ADR 0018); a member does,
        // newest first.
        let owner: Value = testing::get(&state, &addr, "/v1/workflow-runs").await;
        assert_eq!(ids(&owner), [shown]);
        state
            .app
            .store
            .set_project_access(secret, true, &[lea.id])
            .unwrap();
        let list = get("/v1/workflow-runs".to_string(), token).await;
        let list: Value = list.json().await.unwrap();
        assert_eq!(ids(&list), [hidden, shown]);
    }
}
