//! The door a `webhook` trigger answers on (see [`crate::workflows::trigger::webhook`]):
//!
//! - `POST /api/hooks/{path}`: a delivery to the workflow whose webhook trigger names this path, whose run belongs to
//!   the repository the trigger names.
//! - `POST /api/hooks/{repository}/{path}`: the same, for a delivery that names which repository the run belongs to.
//!
//! Neither needs the server's token: a hook is a door an outside service knocks on, and what opens it is the trigger's
//! own secret, which a workflow file only names. A delivery that reaches no hook answers one way whether its path is
//! unknown, its credential missing, its credential wrong, or the secret the trigger names not in the server's store,
//! so nothing here says which paths have hooks or what they ask for. A JSON body becomes the run's value and the id of
//! the queued run comes back at once, its nodes running on a thread of their own; a body that is not JSON is refused
//! saying so, and so is a delivery past the workflow's ceiling of runs going at once.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::Serialize;
use serde_json::Value;

use super::{ApiState, same};
use crate::app::{App, AppError};
use crate::server::settings::Secrets;
use crate::store::Repository;
use crate::workflows::trigger::webhook::{self, Hook, MAX_ACTIVE_RUNS};

/// What a delivery gets when it reaches no hook, whether for its path or for its credential.
const NO_HOOK: &str = "no hook on this path";

/// The routes that need no token: an outside service knows a hook's path and, at most, its secret.
pub(super) fn public_routes() -> Router<ApiState> {
    Router::new()
        .route("/api/hooks/{path}", post(deliver))
        .route(
            "/api/hooks/{repository}/{path}",
            post(deliver_in_repository),
        )
}

/// What a hook answers when it starts a run.
#[derive(Serialize)]
struct Started {
    /// The stored run, queued and already running on a thread of its own.
    run: i64,
    /// The workflow whose trigger answered on the path.
    workflow: String,
}

/// Why a delivery did not start a run, and how it is answered.
enum Refusal {
    /// Nothing on this path answers to this delivery. The one answer for an unknown path, a credential that is
    /// missing or wrong, and a secret the server does not have.
    Unknown,
    /// The body is not a JSON value, which is what a run starts with.
    Body(String),
    /// The run would belong to a repository the server does not have, or to none.
    Repository(String),
    /// The workflow already has as many runs going as its hook lets have.
    Ceiling(String),
    /// The server could not say: its own configuration or store failed.
    Failed(AppError),
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Refusal::Unknown => (StatusCode::NOT_FOUND, NO_HOOK.to_string()),
            Refusal::Body(message) => (StatusCode::BAD_REQUEST, message),
            Refusal::Repository(message) => (StatusCode::UNPROCESSABLE_ENTITY, message),
            Refusal::Ceiling(message) => (StatusCode::TOO_MANY_REQUESTS, message),
            Refusal::Failed(error) => return super::ApiError::from(error).into_response(),
        };
        (status, message).into_response()
    }
}

impl From<AppError> for Refusal {
    fn from(error: AppError) -> Self {
        Refusal::Failed(error)
    }
}

impl From<crate::store::StoreError> for Refusal {
    fn from(error: crate::store::StoreError) -> Self {
        Refusal::Failed(error.into())
    }
}

async fn deliver(
    State(state): State<ApiState>,
    UrlPath(path): UrlPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    answer(state, &path, None, &headers, &body).await
}

async fn deliver_in_repository(
    State(state): State<ApiState>,
    UrlPath((repository, path)): UrlPath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    answer(state, &path, Some(&repository), &headers, &body).await
}

/// The one door both forms go through. Which workflow a path names, what it asks for, and which repository its run
/// belongs to are all read from the harness configuration repository and the store, so they are asked off the runtime.
async fn answer(
    state: ApiState,
    path: &str,
    repository: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    let credential = credential(headers).map(str::to_string);
    let path = path.to_string();
    let repository = repository.map(str::to_string);
    let body = body.to_vec();
    let outcome = tokio::task::spawn_blocking(move || -> Result<Started, Refusal> {
        start(
            &state.app,
            state.config.secrets(),
            &path,
            repository.as_deref(),
            credential.as_deref(),
            &body,
        )
    })
    .await;
    match outcome {
        Ok(Ok(started)) => (StatusCode::ACCEPTED, axum::Json(started)).into_response(),
        Ok(Err(refusal)) => refusal.into_response(),
        Err(error) => super::internal(format!("the hook thread died: {error}")).into_response(),
    }
}

/// Start the run one delivery asks for: the hook its path names, the credential it carries, the value its body holds.
/// The workflows are read from the same harness configuration repository the run itself is started from, so a hook
/// never answers on a workflow the server would then fail to find.
fn start(
    app: &Arc<App>,
    secrets: &Secrets,
    path: &str,
    url_repository: Option<&str>,
    credential: Option<&str>,
    body: &[u8],
) -> Result<Started, Refusal> {
    let workflows = app.harness_repo()?.workflows()?;
    let hooks = webhook::matching(&workflows, path);
    if hooks.is_empty() {
        return Err(Refusal::Unknown);
    }
    // Open every hook that claims the path before saying anything about it, so a delivery that cannot open them all
    // learns nothing a path with no hook on it would not tell it.
    for hook in &hooks {
        if !opens(hook, secrets, credential)? {
            return Err(Refusal::Unknown);
        }
    }
    let hook = webhook::only(hooks)
        .map_err(|message| Refusal::Failed(AppError::Invalid(message)))?
        .ok_or(Refusal::Unknown)?;
    let definition = workflows
        .iter()
        .find(|(file, _)| file == &hook.workflow)
        .map(|(_, definition)| definition)
        .ok_or_else(|| {
            Refusal::Failed(AppError::Invalid(format!(
                "workflow file {:?} disappeared while its hook was answered",
                hook.workflow
            )))
        })?;
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| Refusal::Body(format!("the body is not a JSON value: {error}")))?;
    let repository = repository_of(app, &hook, url_repository)?;
    // Runs are stored under the definition's own name, not its file's stem, so the ceiling counts those.
    let going = app.store.active_workflow_runs(&definition.name)?;
    if going >= MAX_ACTIVE_RUNS {
        return Err(Refusal::Ceiling(format!(
            "workflow {:?} already has {} runs queued or running, which is all its hook lets it have; try again later",
            definition.name, MAX_ACTIVE_RUNS
        )));
    }
    let run = app.start_workflow_run(definition.clone(), repository.id, value)?;
    Ok(Started {
        run: run.id,
        workflow: run.workflow,
    })
}

/// Whether a delivery carries what a hook asks for. A hook that names no secret is open to anyone who knows its path;
/// one whose named key the server does not have is open to no one, and answers no differently from no hook at all.
fn opens(hook: &Hook, secrets: &Secrets, credential: Option<&str>) -> Result<bool, AppError> {
    let Some(name) = hook.trigger.secret.as_deref() else {
        return Ok(true);
    };
    let wanted = secrets.get(name)?;
    Ok(match (wanted, credential) {
        (Some(wanted), Some(given)) => same(given.as_bytes(), wanted.as_bytes()),
        _ => false,
    })
}

/// The repository the run belongs to: the one the address names, else the one the trigger names. A webhook trigger
/// that names none leaves the answer to say what to name.
fn repository_of(
    app: &Arc<App>,
    hook: &Hook,
    url_repository: Option<&str>,
) -> Result<Repository, Refusal> {
    let Some(wanted) = url_repository.or(hook.trigger.repository.as_deref()) else {
        return Err(Refusal::Repository(format!(
            "workflow {:?} names no repository: set its trigger's `repository`, or post to /api/hooks/<repository>/{path}",
            hook.workflow,
            path = hook.trigger.path
        )));
    };
    let found = app
        .store
        .repositories()?
        .into_iter()
        .find(|repository| repository.name == wanted)
        .ok_or_else(|| Refusal::Repository(format!("no repository named {wanted:?}")))?;
    Ok(found)
}

/// The credential a delivery carries, which is its `Authorization: Bearer` key.
fn credential(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|given| !given.is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, RwLock};

    use serde_json::json;
    use tempfile::TempDir;

    use super::super::testing;
    use super::*;
    use crate::config::AgentSet;
    use crate::harness::repo::{self, HarnessRepo};
    use crate::store::WorkflowRun;

    /// A workflow file: a `webhook` trigger on `path` with `more` as its other settings, and no node behind it, so
    /// its runs start and end at once.
    fn workflow(name: &str, path: &str, more: &str) -> String {
        format!(
            "name = \"{name}\"\ndescription = \"One.\"\n\n[[trigger]]\nkind = \"webhook\"\n[trigger.params]\npath = \"{path}\"\n{more}\n"
        )
    }

    /// A server whose harness configuration repository holds one workflow file per `(file, text)`, and the repository
    /// `radiance` to run in.
    async fn holding(workflows: &[(&str, &str)]) -> (TempDir, ApiState, String) {
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
        let folder = harness.path().join("workflows");
        std::fs::create_dir_all(&folder).unwrap();
        for (name, text) in workflows {
            std::fs::write(folder.join(format!("{name}.toml")), text).unwrap();
        }
        (dir, state, addr)
    }

    /// A server whose workflows are webhook triggers on a path of their own name, with the settings given.
    async fn hooked(workflows: &[(&str, &str)]) -> (TempDir, ApiState, String) {
        let files: Vec<(String, String)> = workflows
            .iter()
            .map(|(name, more)| ((*name).to_string(), workflow(name, name, more)))
            .collect();
        let named: Vec<(&str, &str)> = files
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect();
        holding(&named).await
    }

    /// Delivers to a hook, carrying `key` as its credential when one is given.
    async fn deliver(addr: &str, path: &str, body: &str, key: Option<&str>) -> reqwest::Response {
        let mut request = reqwest::Client::new()
            .post(format!("http://{addr}{path}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
        if let Some(key) = key {
            request = request.bearer_auth(key);
        }
        request.send().await.unwrap()
    }

    /// The run a delivery started, as the store holds it.
    fn stored(state: &ApiState, id: i64) -> WorkflowRun {
        state.app.store.workflow_run(id).unwrap().unwrap()
    }

    #[tokio::test]
    async fn a_json_body_becomes_the_runs_value() {
        let (_dir, state, addr) = hooked(&[("build", "repository = \"radiance\"")]).await;
        let response = deliver(&addr, "/api/hooks/build", r#"{"ref": "abc123"}"#, None).await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let answer: Value = response.json().await.unwrap();
        assert_eq!(answer["workflow"], "build");

        let run = stored(&state, answer["run"].as_i64().unwrap());
        assert_eq!(run.value, json!({"ref": "abc123"}));
        assert_eq!(run.trigger, "webhook");
        assert_eq!(
            run.repository_id, 1,
            "the run belongs to the repository the trigger names"
        );
    }

    #[tokio::test]
    async fn a_delivery_names_the_repository_it_runs_for() {
        let (_dir, state, addr) = hooked(&[("build", "")]).await;
        let response = deliver(&addr, "/api/hooks/radiance/build", "{}", None).await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let answer: Value = response.json().await.unwrap();
        assert_eq!(
            stored(&state, answer["run"].as_i64().unwrap()).repository_id,
            1
        );
    }

    #[tokio::test]
    async fn the_repository_in_the_address_wins_over_the_one_the_trigger_names() {
        let (_dir, state, addr) = hooked(&[("build", "repository = \"radiance\"")]).await;
        let other = state
            .app
            .store
            .add_repository("other", "/srv/other", "none", "main")
            .unwrap();
        assert_ne!(other.id, 1);
        let response = deliver(&addr, "/api/hooks/other/build", "{}", None).await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let answer: Value = response.json().await.unwrap();
        let run = stored(&state, answer["run"].as_i64().unwrap());
        assert_eq!(
            run.repository_id, other.id,
            "the repository in the address wins over the one the trigger names"
        );
    }

    #[tokio::test]
    async fn a_run_with_no_repository_to_belong_to_is_refused() {
        let (_dir, _state, addr) = hooked(&[("build", "")]).await;
        let response = deliver(&addr, "/api/hooks/build", "{}", None).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            response
                .text()
                .await
                .unwrap()
                .contains("names no repository"),
            "the refusal says what to name"
        );
    }

    #[tokio::test]
    async fn a_repository_the_server_does_not_have_is_refused() {
        let (_dir, _state, addr) = hooked(&[("build", "")]).await;
        let response = deliver(&addr, "/api/hooks/nobody/build", "{}", None).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            response
                .text()
                .await
                .unwrap()
                .contains("no repository named"),
            "the refusal names the repository it looked for"
        );
    }

    #[tokio::test]
    async fn a_hook_wants_the_secret_its_trigger_names() {
        let (_dir, state, addr) = hooked(&[(
            "build",
            "repository = \"radiance\"\nsecret = \"build-hook\"",
        )])
        .await;
        state.config.secrets().set("build-hook", "s3cret").unwrap();

        let right = deliver(&addr, "/api/hooks/build", "{}", Some("s3cret")).await;
        assert_eq!(right.status(), StatusCode::ACCEPTED);
        let wrong = deliver(&addr, "/api/hooks/build", "{}", Some("guess")).await;
        assert_eq!(wrong.status(), StatusCode::NOT_FOUND);
        let missing = deliver(&addr, "/api/hooks/build", "{}", None).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            wrong.text().await.unwrap(),
            missing.text().await.unwrap(),
            "a wrong credential and no credential at all answer alike"
        );
    }

    #[tokio::test]
    async fn an_unknown_path_and_a_wrong_secret_answer_alike() {
        let (_dir, state, addr) = hooked(&[(
            "build",
            "repository = \"radiance\"\nsecret = \"build-hook\"",
        )])
        .await;
        state.config.secrets().set("build-hook", "s3cret").unwrap();

        let unknown = deliver(&addr, "/api/hooks/nowhere", "{}", Some("s3cret")).await;
        let wrong = deliver(&addr, "/api/hooks/build", "{}", Some("guess")).await;
        assert_eq!(unknown.status(), wrong.status());
        assert_eq!(unknown.text().await.unwrap(), wrong.text().await.unwrap());
    }

    #[tokio::test]
    async fn a_secret_nobody_has_set_keeps_the_hook_shut() {
        let (_dir, _state, addr) = hooked(&[(
            "build",
            "repository = \"radiance\"\nsecret = \"build-hook\"",
        )])
        .await;
        let response = deliver(&addr, "/api/hooks/build", "{}", Some("anything")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_delivery_that_cannot_open_the_hook_is_refused_whatever_its_body() {
        let (_dir, state, addr) = hooked(&[(
            "build",
            "repository = \"radiance\"\nsecret = \"build-hook\"",
        )])
        .await;
        state.config.secrets().set("build-hook", "s3cret").unwrap();
        let response = deliver(&addr, "/api/hooks/build", "not json at all", None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.text().await.unwrap(), NO_HOOK);
    }

    #[tokio::test]
    async fn a_body_that_is_not_json_is_refused_saying_so() {
        let (_dir, _state, addr) = hooked(&[("build", "repository = \"radiance\"")]).await;
        let response = deliver(&addr, "/api/hooks/build", "a note, not JSON", None).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            response
                .text()
                .await
                .unwrap()
                .contains("is not a JSON value"),
            "the refusal says what is wrong with the body"
        );
    }

    #[tokio::test]
    async fn a_flood_past_the_ceiling_is_refused() {
        let (_dir, state, addr) = hooked(&[("build", "repository = \"radiance\"")]).await;
        for _ in 0..MAX_ACTIVE_RUNS {
            state
                .app
                .store
                .add_workflow_run("build", 1, "webhook", &json!({}))
                .unwrap();
        }
        let response = deliver(&addr, "/api/hooks/build", "{}", None).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(
            response.text().await.unwrap().contains("try again later"),
            "the refusal says when to come back"
        );
        assert_eq!(
            state.app.store.active_workflow_runs("build").unwrap(),
            MAX_ACTIVE_RUNS,
            "a refused delivery starts no run of its own"
        );
    }

    #[tokio::test]
    async fn the_ceiling_counts_runs_under_the_name_they_are_stored_with() {
        let text = workflow("Release build", "build", "repository = \"radiance\"");
        let (_dir, state, addr) = holding(&[("release", &text)]).await;
        for _ in 0..MAX_ACTIVE_RUNS {
            state
                .app
                .store
                .add_workflow_run("Release build", 1, "webhook", &json!({}))
                .unwrap();
        }
        let response = deliver(&addr, "/api/hooks/build", "{}", None).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            state
                .app
                .store
                .active_workflow_runs("Release build")
                .unwrap(),
            MAX_ACTIVE_RUNS,
            "the ceiling counts the name a run is stored under, not its file's stem"
        );
    }

    #[tokio::test]
    async fn a_workflow_triggered_otherwise_answers_on_no_path() {
        let (_dir, _state, addr) = holding(&[(
            "plan",
            "name = \"plan\"\ndescription = \"One.\"\n[[trigger]]\nkind = \"test\"\n",
        )])
        .await;
        let response = deliver(&addr, "/api/hooks/plan", "{}", None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.text().await.unwrap(), NO_HOOK);
    }

    #[tokio::test]
    async fn two_workflows_on_one_path_start_nothing() {
        let (_dir, state, addr) = holding(&[
            ("a", &workflow("a", "build", "repository = \"radiance\"")),
            ("b", &workflow("b", "build", "repository = \"radiance\"")),
        ])
        .await;
        let response = deliver(&addr, "/api/hooks/build", "{}", None).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let refusal = response.text().await.unwrap();
        assert!(
            refusal.contains("\"a\"") && refusal.contains("\"b\""),
            "the refusal names both workflows that claim the path: {refusal}"
        );
        assert_eq!(state.app.store.active_workflow_runs("a").unwrap(), 0);
    }

    #[tokio::test]
    async fn two_workflows_on_one_path_say_nothing_to_a_delivery_that_cannot_open_them() {
        let (_dir, state, addr) = holding(&[
            (
                "a",
                &workflow(
                    "a",
                    "build",
                    "repository = \"radiance\"\nsecret = \"build-hook\"",
                ),
            ),
            (
                "b",
                &workflow(
                    "b",
                    "build",
                    "repository = \"radiance\"\nsecret = \"build-hook\"",
                ),
            ),
        ])
        .await;
        state.config.secrets().set("build-hook", "s3cret").unwrap();

        let unknown = deliver(&addr, "/api/hooks/nowhere", "{}", None).await;
        let unknown = (unknown.status(), unknown.text().await.unwrap());
        let missing = deliver(&addr, "/api/hooks/build", "{}", None).await;
        assert_eq!(
            (missing.status(), missing.text().await.unwrap()),
            unknown,
            "a path with two hooks answers a delivery with no credential as an unknown path does"
        );
        let wrong = deliver(&addr, "/api/hooks/build", "{}", Some("guess")).await;
        assert_eq!(wrong.status(), StatusCode::NOT_FOUND);
        assert_eq!(wrong.text().await.unwrap(), NO_HOOK);

        let right = deliver(&addr, "/api/hooks/build", "{}", Some("s3cret")).await;
        assert_eq!(right.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let refusal = right.text().await.unwrap();
        assert!(
            refusal.contains("\"a\"") && refusal.contains("\"b\""),
            "a delivery that opens every hook is told both workflows claim the path: {refusal}"
        );
        assert_eq!(state.app.store.active_workflow_runs("a").unwrap(), 0);
        assert_eq!(state.app.store.active_workflow_runs("b").unwrap(), 0);
    }
}
