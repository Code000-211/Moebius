//! Starting runs of the workflows by hand: the `manual` trigger names its inputs, and a person starts each run giving
//! them — through this door or the `moebius workflow run` command.
//!
//! - `GET /v1/workflows/{name}/run-inputs`: the inputs of the workflow's manual trigger, so a form can be built from
//!   them; a conflict when the trigger is not manual.
//! - `POST /v1/workflows/{name}/runs` `{repository?, inputs?}`: starts a run of the workflow, checked against the
//!   trigger's inputs — a refusal names the input it refused, a default stands in for an input that was not given,
//!   and an input the trigger does not name is refused. The run belongs to the repository `repository` names, or to
//!   the one the trigger's `repository` input names when it has one.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{ApiError, ApiState, blocking};
use crate::app::AppError;
use crate::workflows::trigger::manual;

/// The routes that start runs of workflows by hand.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/workflows/{name}/run-inputs", get(run_inputs))
        .route("/v1/workflows/{name}/runs", post(start))
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
    app: &crate::app::App,
    repo: &crate::harness::repo::HarnessRepo,
    name: &str,
    request: &StartRequest,
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
    let value = manual.resolve(&request.inputs)?;

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
    let repositories = app.store.repositories()?;
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
        Ok(RunInputs {
            trigger_kind: trigger.kind,
            inputs: manual.inputs,
        })
    })
    .await
}

async fn start(
    State(state): State<ApiState>,
    UrlPath(name): UrlPath<String>,
    axum::Json(request): axum::Json<StartRequest>,
) -> Result<axum::Json<Started>, ApiError> {
    let repo = harness(&state)?;
    blocking(&state.app, move |app| start_manual(app, &repo, &name, &request)).await
}

fn harness(
    state: &ApiState,
) -> Result<Arc<crate::harness::repo::HarnessRepo>, ApiError> {
    if let Some(repo) = state.harness_repo.clone() {
        return Ok(repo);
    }
    state.app.harness_repo().cloned().map_err(|_| {
        AppError::Conflict("the server has no harness configuration repository".into()).into()
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

    async fn start_run(
        state: &ApiState,
        addr: &str,
        body: Value,
    ) -> (reqwest::StatusCode, String) {
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

        let stored = state.app.store.workflow_run(started["id"].as_i64().unwrap()).unwrap().unwrap();
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
            assert!(body.contains(input), "the refusal names the input it refused: {body}");
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
        let (status, body) = start_run(
            &state,
            &addr,
            json!({ "inputs": { "version": "1.2.3" } }),
        )
        .await;
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
