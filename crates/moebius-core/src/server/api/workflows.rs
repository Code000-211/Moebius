//! The workflow door the canvas, the command line and the tests use (see [`crate::workflows::definition`]).
//!
//! - `GET /v1/workflows`: every workflow of the harness configuration repository, named, described, with its trigger
//!   kind and how many nodes it has.
//! - `GET /v1/workflows/{name}`: one workflow's file and its parsed form.
//! - `PUT /v1/workflows/{name}` `{file}` or `{definition}`: validates and commits the workflow. An invalid definition
//!   is refused with the validation message and the file is untouched.
//! - `DELETE /v1/workflows/{name}`: commits the workflow's deletion, refused while a run of it is queued, running or
//!   waiting.
//! - `POST /v1/workflows/runs/{id}/cancel`: cancels a run of a workflow and ends it `cancelled`.
//! - `POST /v1/workflows/runs/{run_id}/resume` `{decision, value?, reason?}`: records a person's decision on a run that
//!   waits for one (`workflow_approval` in the inbox); a second decision on a finished wait is refused.

use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking};
use crate::app::AppError;
use crate::app::workflows::DecisionRequest;
use crate::app::workflows::stopping::CancelWorkflowRunError;
use crate::store::WorkflowRun;
use crate::workflows::definition::{self, WorkflowDefinition};

/// The routes of workflows.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/workflows", get(list))
        .route("/v1/workflows/{name}", get(show).put(save).delete(delete))
        .route("/v1/workflows/runs/{id}/cancel", post(cancel_run))
        .route("/v1/workflows/runs/{run_id}/resume", post(resume))
}

/// One workflow as the list names it.
#[derive(Debug, Serialize)]
struct Summary {
    name: String,
    description: String,
    trigger_kind: String,
    node_count: usize,
}

/// One workflow's file and parsed form.
#[derive(Debug, Serialize)]
struct View {
    name: String,
    file: String,
    definition: WorkflowDefinition,
}

/// What `PUT /v1/workflows/{name}` takes: either the file's text or its parsed form.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveRequest {
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    definition: Option<WorkflowDefinition>,
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

/// What deleting a workflow answers: the name and the commit it made.
#[derive(Debug, Serialize)]
struct Deleted {
    name: String,
    /// The short id of the commit; left out when there was nothing to commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
}

async fn list(State(state): State<ApiState>) -> Result<axum::Json<Vec<Summary>>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |_| {
        let workflows = repo.workflows()?;
        Ok(workflows
            .into_iter()
            .map(|(name, definition)| Summary {
                name,
                description: definition.description,
                trigger_kind: definition
                    .trigger
                    .first()
                    .map(|trigger| trigger.kind.clone())
                    .unwrap_or_default(),
                node_count: definition.nodes.len(),
            })
            .collect())
    })
    .await
}

async fn show(
    State(state): State<ApiState>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<View>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |_| view(&repo, &name)).await
}

async fn save(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath(name): UrlPath<String>,
    axum::Json(request): axum::Json<SaveRequest>,
) -> Result<axum::Json<Saved>, ApiError> {
    let repo = harness(&state)?;
    let definition = match (request.file, request.definition) {
        (Some(file), None) => {
            definition::parse(&name, &file).map_err(|error| AppError::Invalid(error.to_string()))?
        }
        (None, Some(definition)) => definition,
        _ => {
            return Err(AppError::Invalid("send exactly one of file and definition".into()).into());
        }
    };
    blocking(&state.app, move |app| {
        let commit = repo.save_workflow(app, &by, &name, &definition)?;
        Ok(Saved {
            view: view(&repo, &name)?,
            commit,
        })
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

async fn delete(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<Deleted>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |app| {
        let commit = repo.delete_workflow(app, &by, &name)?;
        Ok(Deleted { name, commit })
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
fn harness(
    state: &ApiState,
) -> Result<std::sync::Arc<crate::harness::repo::HarnessRepo>, ApiError> {
    if let Some(repo) = state.harness_repo.clone() {
        return Ok(repo);
    }
    state.app.harness_repo().cloned().map_err(|_| {
        AppError::Conflict("the server has no harness configuration repository".into()).into()
    })
}

/// One workflow's file and parsed form.
fn view(repo: &crate::harness::repo::HarnessRepo, name: &str) -> crate::app::Result<View> {
    let definition = repo.workflow(name)?;
    let file = definition::read_text(repo.path(), name)
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    Ok(View {
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
        let folder = harness.path().join("workflows");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("asking.toml"), APPROVAL).unwrap();
        let repository = state.app.store.repositories().unwrap().first().unwrap().id;
        let run = state
            .app
            .start_workflow_run("asking", repository, value)
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
            harness.path().join("workflows/release.toml").is_file(),
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
                "name": "release",
                "description": "Build the change and run its tests.",
                "trigger_kind": "test",
                "node_count": 2,
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
            .add_workflow_run("release", repository, "test", &json!({}))
            .unwrap();

        let response = delete(&state, &addr, "release").await;
        assert_eq!(response.status(), 409);
        assert!(harness.path().join("workflows/release.toml").is_file());

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
        assert_eq!(shown.status(), 422);
        assert!(!harness.path().join("workflows/release.toml").is_file());
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
}
