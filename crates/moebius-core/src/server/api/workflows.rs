//! The workflow door the canvas, the command line and the tests use (see [`crate::workflows::definition`]).
//!
//! - `GET /v1/workflows`: every workflow of the harness configuration repository, named, described, with its trigger
//!   kind and how many nodes it has.
//! - `GET /v1/workflows/{name}`: one workflow's file and its parsed form.
//! - `PUT /v1/workflows/{name}` `{file}` or `{definition}`: validates and commits the workflow. An invalid definition
//!   is refused with the validation message and the file is untouched.
//! - `DELETE /v1/workflows/{name}`: commits the workflow's deletion, refused while a run of it is queued or running.

use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::routing::get;
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking};
use crate::app::AppError;
use crate::workflows::definition::{self, WorkflowDefinition};

/// The routes of workflows.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/workflows", get(list))
        .route("/v1/workflows/{name}", get(show).put(save).delete(delete))
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

    fn log(harness: &HarnessRepo) -> String {
        let output = std::process::Command::new("git")
            .args(["log", "--format=%s"])
            .current_dir(harness.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
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
}
