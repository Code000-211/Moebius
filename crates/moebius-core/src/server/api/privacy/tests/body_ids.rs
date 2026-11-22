//! The routes that take what they work on from their body, or answer with more than their path names: a Retro started
//! or retried, a replay deleted, an automation or research saved in a repository, and a workflow run started with
//! inputs. Each keeps a private project from someone who is no member of it, as the guarded routes do.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use super::*;
use crate::config::AgentSet;
use crate::harness::repo::{self, HarnessRepo};

async fn send(addr: &str, token: &str, method: &str, path: &str, body: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .request(method.parse().unwrap(), format!("http://{addr}{path}"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

/// Gives the test server a harness configuration repository, which a Retro's analysis and a workflow need.
fn with_harness(dir: &std::path::Path, state: &ApiState) -> Arc<HarnessRepo> {
    let origin = dir.join("harness-origin");
    repo::create(&origin, &BTreeMap::new(), None).unwrap();
    let harness = Arc::new(
        HarnessRepo::open(
            &state.app,
            origin.to_str().unwrap(),
            &dir.join("harness-clone"),
            vec![],
            Arc::new(RwLock::new(AgentSet::default())),
        )
        .unwrap(),
    );
    state.app.set_harness_repo(harness.clone());
    harness
}

/// The cards a Retro's view names among its errors and merged cards.
fn cards_of(view: &Value) -> Vec<i64> {
    let report = &view["report"];
    ["errors", "merged"]
        .iter()
        .flat_map(|list| report[list].as_array().cloned().unwrap_or_default())
        .filter_map(|entry| entry["card"].as_i64())
        .collect()
}

/// A Retro an owner who is no member starts or retries answers with what they would see of it, not the whole report.
#[tokio::test]
async fn a_retro_started_or_retried_answers_without_a_private_project_s_cards() {
    let (dir, state, addr) = start().await;
    let private = private(&state);
    let store = &state.app.store;
    store.set_run_status(private.run, "failed").unwrap();

    let (status, view) = send(&addr, &private.olga, "POST", "/v1/retros", json!({})).await;
    assert_eq!(status, 200, "{view}");
    assert!(!cards_of(&view).contains(&private.card), "{view}");
    // The report has the card, which its member sees.
    let today = format!("/v1/retros/{}", view["date"].as_str().unwrap());
    let (_, view) = call(&addr, &private.lea, "GET", &today).await;
    assert!(cards_of(&view).contains(&private.card), "{view}");

    // A retry analyses, so it needs a server with a harness configuration repository.
    let harness = with_harness(dir.path(), &state);
    let with = ApiState {
        harness_repo: Some(harness),
        ..state.clone()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second = listener.local_addr().unwrap().to_string();
    tokio::spawn(crate::server::api::serve(listener, with));
    store
        .finish_retro("2026-03-10", "failed", &["no analyst".into()])
        .unwrap();
    let path = "/v1/retros/2026-03-10/retry";
    let (status, view) = send(&second, &private.olga, "POST", path, json!({})).await;
    assert_eq!(status, 200, "{view}");
    assert!(!cards_of(&view).contains(&private.card), "{view}");
    assert!(!view.to_string().contains("secret"), "{view}");
}

/// A replay of a private project's run is not found by an owner who is no member, and stays.
#[tokio::test]
async fn a_replay_of_a_private_project_is_not_deleted_by_a_non_member() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let replay = state.app.store.replays(None, None, 10).unwrap()[0].id;
    let path = format!("/v1/replays/{replay}");
    let (status, body) = send(&addr, &private.olga, "DELETE", &path, json!({})).await;
    assert_eq!(status, 404, "{body}");
    assert!(!body.to_string().contains("secret output"), "{body}");
    assert_eq!(state.app.store.replays(None, None, 10).unwrap().len(), 1);
}

/// An automation or research is not saved in a repository of a private project by an owner who is no member of it.
#[tokio::test]
async fn an_automation_or_research_is_not_put_in_a_private_project_s_repository() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let automation = json!({
        "name": "Peek",
        "schedule": "0 3 * * *",
        "agent": "Coder",
        "prompt": "Read the secret files",
        "repository": private.repository,
        "harness": false,
        "enabled": false,
    });
    let (status, body) = send(
        &addr,
        &private.olga,
        "POST",
        "/v1/automations",
        automation.clone(),
    )
    .await;
    assert_eq!(status, 404, "{body}");
    // Nor moved there from an automation they see.
    let mut seen = automation.clone();
    seen["repository"] = Value::Null;
    let (status, body) = send(&addr, &private.olga, "POST", "/v1/automations", seen).await;
    assert_eq!(status, 200, "{body}");
    let path = format!("/v1/automations/{}", body["id"]);
    let (status, body) = send(&addr, &private.olga, "PUT", &path, automation.clone()).await;
    assert_eq!(status, 404, "{body}");
    // Its member may.
    let (status, body) = send(&addr, &private.lea, "PUT", &path, automation).await;
    assert_ne!(status, 404, "{body}");

    let research = json!({
        "repository": private.repository,
        "goal": "Find the secret",
        "measure": "make bench",
        "better": "lower",
        "token_budget": 1000,
    });
    let before = state.app.store.researches().unwrap().len();
    let (status, body) = send(&addr, &private.olga, "POST", "/v1/research", research).await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(state.app.store.researches().unwrap().len(), before);
}

/// A workflow run is not started in a private project's repository through an input of another name, or a default.
#[tokio::test]
async fn a_workflow_run_is_not_started_in_a_private_project_s_repository_by_any_input() {
    let (dir, state, addr) = start().await;
    let private = private(&state);
    let harness = with_harness(dir.path(), &state);
    let name = store_name(&state, private.repository);
    let workflow = |input: &str| {
        format!(
            r#"
name = "peek"
description = "Started by a person."

[[trigger]]
kind = "manual"
params = {{ inputs = [{input}] }}

[[nodes]]
id = "build"
kind = "test"

[[edges]]
from = "trigger"
to = "build"
port = "out"
"#
        )
    };
    // The workflow is a file of the private project's folder: the server's own person is no member of it, and a
    // person who does not see a project cannot save into it.
    let folder = harness.path().join(format!(
        "projects/{}/workflows",
        state
            .app
            .store
            .project(private.project)
            .unwrap()
            .unwrap()
            .name
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let save = |file: String| {
        let path = folder.join("peek.toml");
        async move { std::fs::write(path, file).unwrap() }
    };
    let runs = "/v1/workflows/peek/runs";

    save(workflow(
        r#"{ name = "target", type = "repository", required = true }"#,
    ))
    .await;
    let body = json!({"inputs": {"target": name}});
    let (status, answer) = send(&addr, &private.ben, "POST", runs, body.clone()).await;
    assert_eq!(status, 404, "{answer}");
    let (status, answer) = send(&addr, &private.lea, "POST", runs, body).await;
    assert_eq!(status, 200, "{answer}");

    save(workflow(&format!(
        r#"{{ name = "target", type = "repository", default = "{name}" }}"#
    )))
    .await;
    let (status, answer) = send(&addr, &private.ben, "POST", runs, json!({"inputs": {}})).await;
    assert_eq!(status, 404, "{answer}");
}
