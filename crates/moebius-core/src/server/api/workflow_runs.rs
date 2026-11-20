//! The runs of the workflows: reading them, and starting them by hand — the `manual` trigger names its inputs, and a
//! person starts each run giving them, through this door or the `moebius workflow run` command.
//!
//! - `GET /v1/workflow-runs?workflow=&limit=`: the runs, newest first (of one workflow when it is named), leaving out
//!   the runs of a repository the person does not see.
//! - `GET /v1/workflow-runs/{id}`: one run with what each of its nodes was given, answered and failed with.
//! - `GET /v1/workflows/{name}/run-inputs`: the inputs of the workflow's manual trigger, so a form can be built from
//!   them; a conflict when the trigger is not manual.
//! - `POST /v1/workflows/{name}/runs` `{repository?, inputs?}`: starts a run of the workflow, checked against the
//!   trigger's inputs — a refusal names the input it refused, a default stands in for an input that was not given,
//!   and an input the trigger does not name is refused. The run belongs to the repository `repository` names, or to
//!   the one the trigger's `repository` input names when it has one.

use std::sync::Arc;

use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{ApiError, ApiState, blocking};
use crate::app::AppError;
use crate::decide::Actor;
use crate::store::WorkflowRun;
use crate::workflows::trigger::manual;

/// The routes that start runs of workflows by hand.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
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
    repository_id: i64,
    trigger: String,
    status: String,
    value: Value,
    created_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    error: Option<String>,
}

impl From<WorkflowRun> for RunView {
    fn from(run: WorkflowRun) -> Self {
        Self {
            id: run.id,
            workflow: run.workflow,
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
        Ok(visible_runs(app, &sight, query.workflow.as_deref(), limit)?
            .into_iter()
            .map(RunView::from)
            .collect())
    })
    .await
}

/// The newest runs of `workflow` (of all with none) in repositories `sight` sees, at most `limit`.
pub(super) fn visible_runs(
    app: &crate::app::App,
    sight: &crate::app::Sight,
    workflow: Option<&str>,
    limit: i64,
) -> Result<Vec<WorkflowRun>, AppError> {
    // A run of a repository the person does not see is skipped, so the store is read page by page until `limit`
    // visible runs are found or the runs are all read.
    let wanted = limit as usize;
    let mut found = Vec::new();
    let mut before = None;
    while found.len() < wanted {
        let page = app.store.workflow_runs(workflow, before, MOST_RUNS)?;
        let Some(last) = page.last() else { break };
        before = Some(last.id);
        let full = page.len() as i64 == MOST_RUNS;
        found.extend(
            page.into_iter()
                .filter(|run| sight.repository(run.repository_id)),
        );
        if !full {
            break;
        }
    }
    found.truncate(wanted);
    Ok(found)
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
            .filter(|run| sight.repository(run.repository_id))
            .ok_or_else(|| AppError::NotFound(format!("workflow run {id}")))?;
        let kinds: std::collections::HashMap<String, String> = repo
            .and_then(|repo| repo.workflow(&run.workflow).ok())
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
            run: run.into(),
            nodes,
            waiting,
        })
    })
    .await
}

/// What `GET /v1/workflows/{name}/run-inputs` answers: the inputs a form builds itself from.
#[derive(Debug, Serialize)]
struct RunInputs {
    /// Always `manual`, the trigger of the workflow.
    trigger_kind: String,
    /// The inputs in the order the trigger's file names them.
    inputs: Vec<manual::Input>,
}

/// What `POST /v1/workflows/{name}/runs` takes.
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
    /// The name of the repository the run belongs to.
    repository: String,
    /// The kind of the trigger that started the run, `manual` through this door.
    trigger: String,
    status: String,
    /// The value the run starts with: the inputs given, with the defaults standing in.
    value: Value,
}

/// The inputs a person gave, checked against the trigger of the workflow, the repository the run belongs to, and the
/// run as it was recorded.
fn start_manual(
    app: &std::sync::Arc<crate::app::App>,
    repo: &crate::harness::repo::HarnessRepo,
    name: &str,
    request: &StartRequest,
    sight: &crate::app::Sight,
) -> Result<Started, AppError> {
    let definition = repo.workflow(name)?;
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

    // The run belongs to the repository the person named, or to the one the trigger's `repository` input named.
    let (repository_name, via_input) = match manual
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
            let repository_name = value
                .get(&input.name)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| AppError::Invalid(format!(
                    "the trigger's input {:?} names the run's repository, but no value was given for it",
                    input.name
                )))?;
            (repository_name, Some(input.name.clone()))
        }
        None => (
            request
                .repository
                .clone()
                .filter(|repository| !repository.is_empty())
                .ok_or_else(|| AppError::Invalid(
                    "the trigger has no repository input, so \"repository\" names the run's repository".to_string(),
                ))?,
            None,
        ),
    };
    let repository = repositories
        .iter()
        .find(|repository| repository.name == repository_name)
        .cloned()
        .ok_or_else(|| {
            AppError::Invalid(match &via_input {
                Some(input) => format!("input {input:?}: no repository named {repository_name:?}"),
                None => format!("no repository named {repository_name:?}"),
            })
        })?;

    let run = app.start_workflow_run(name, repository.id, value)?;
    Ok(Started {
        id: run.id,
        workflow: run.workflow,
        repository: repository.name,
        trigger: run.trigger,
        status: run.status,
        value: run.value,
    })
}

async fn run_inputs(
    State(state): State<ApiState>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<RunInputs>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |_| {
        let definition = repo.workflow(&name)?;
        let trigger =
            definition.trigger.first().cloned().ok_or_else(|| {
                AppError::Conflict(format!("the workflow {name:?} has no trigger"))
            })?;
        if trigger.kind != manual::KIND {
            return Err(AppError::Conflict(format!(
                "the workflow's trigger is {:?}, not manual",
                trigger.kind
            )));
        }
        let manual = manual::ManualTrigger::parse(&trigger.params)
            .map_err(|message| AppError::Invalid(format!("trigger: {message}")))?;
        Ok(RunInputs {
            trigger_kind: trigger.kind,
            inputs: manual.inputs,
        })
    })
    .await
}

async fn start(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
    axum::Json(request): axum::Json<StartRequest>,
) -> Result<axum::Json<Started>, ApiError> {
    let repo = harness(&state);
    blocking(&state.app, move |app| {
        // A run works only in a repository the person sees (see `app::projects::sight`): the one the request names
        // is checked first, and every one the workflow's inputs name once they are read (`start_manual`).
        let sight = app.sight(&by)?;
        let named = [
            request.repository.as_deref(),
            request.inputs.get("repository").and_then(Value::as_str),
        ];
        for name in named.into_iter().flatten() {
            if app
                .store
                .repositories()?
                .iter()
                .any(|r| r.name == name && !sight.repository(r.id))
            {
                return Err(AppError::NotFound(format!("repository {name}")));
            }
        }
        let repo = repo?;
        start_manual(app, &repo, &name, &request, &sight)
    })
    .await
}

fn harness(state: &ApiState) -> Result<Arc<crate::harness::repo::HarnessRepo>, AppError> {
    if let Some(repo) = state.harness_repo.clone() {
        return Ok(repo);
    }
    state.app.harness_repo().cloned().map_err(|_| {
        AppError::Conflict("the server has no harness configuration repository".into())
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

        // Without a repository, when the trigger has no input of its kind.
        let (status, body) =
            start_run(&state, &addr, json!({ "inputs": { "version": "1.2.3" } })).await;
        assert_eq!(status, 422, "{body}");
        assert!(body.contains("repository"), "{body}");

        // With a repository the server does not know.
        let (status, body) = start_run(
            &state,
            &addr,
            json!({ "repository": "nobody", "inputs": { "version": "1.2.3" } }),
        )
        .await;
        assert_eq!(status, 422, "{body}");
        assert!(body.contains("nobody"), "{body}");
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
        assert!(body.contains("\"repository\""), "{body}");
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
}
