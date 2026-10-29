//! The result endpoint, `POST /moebius/result` (see the folder's doc): each text of a tool's result cleaned of secrets
//! and, with the agent's `tool_output`, made smaller where it is made (`harness::output`), recorded as a `tool_output`
//! run event.

use std::path::PathBuf;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Check, parse, unauthorized};
use crate::app::App;
use crate::config::ToolOutput;
use crate::router::Grant;

#[derive(Debug, Deserialize)]
struct ResultRequest {
    content: Vec<Value>,
    #[serde(default)]
    tool_call_id: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    input: Value,
    /// How small to make the result; without it, it is only cleaned of secrets.
    #[serde(default)]
    tool_output: Option<ToolOutput>,
}

#[derive(Debug, Serialize)]
struct ResultReply {
    content: Vec<Value>,
}

pub(super) async fn result(
    State(state): State<Check>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(grant) = state.router.grant(&headers) else {
        return unauthorized();
    };
    let request: ResultRequest = match parse(&body) {
        Ok(request) => request,
        Err(rejection) => return rejection.into_response(),
    };
    let reply =
        tokio::task::spawn_blocking(move || clean_result(&state.app, &grant, request)).await;
    match reply {
        Ok(content) => Json(ResultReply { content }).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "cannot clean the result" })),
        )
            .into_response(),
    }
}

/// The result's blocks with the secrets removed from each text and, with `tool_output`, each text made smaller in
/// the run's worktree; every text made smaller leaves a `tool_output` run event with what that saved.
fn clean_result(app: &App, grant: &Grant, request: ResultRequest) -> Vec<Value> {
    let worktree = request
        .tool_output
        .and_then(|_| app.store.run(grant.run_id).ok().flatten())
        .and_then(|run| run.worktree)
        .map(PathBuf::from);
    let command = (request.tool.as_deref() == Some("bash"))
        .then(|| request.input["command"].as_str())
        .flatten();
    request
        .content
        .into_iter()
        .map(|mut block| {
            if block["type"] == "text"
                && let Some(text) = block["text"].as_str()
            {
                let text = crate::blobs::redact(text);
                let shrunk = request.tool_output.and_then(|limits| {
                    crate::harness::output::shrink(&text, command, &limits, worktree.as_deref())
                });
                match shrunk {
                    Some(shrunk) => {
                        let event = json!({
                            "tool": request.tool,
                            "tool_call_id": request.tool_call_id,
                            "steps": shrunk.steps,
                            "bytes_before": shrunk.bytes_before,
                            "bytes_after": shrunk.bytes_after,
                            "lines_before": shrunk.lines_before,
                            "lines_after": shrunk.lines_after,
                            "saved": shrunk.saved,
                        });
                        let _ = app.record_run_event(grant.run_id, "tool_output", &event);
                        block["text"] = json!(shrunk.text);
                    }
                    None => block["text"] = json!(text),
                }
            }
            block
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::decide::check::tests::setup;

    #[tokio::test]
    async fn a_result_comes_back_without_secrets() {
        let s = setup(true).await;
        let key = format!("sk-ant-{}", "k".repeat(30));
        let image = json!({"type": "image", "data": key, "mimeType": "image/png"});
        let reply: Value = reqwest::Client::new()
            .post(format!("{}/moebius/result", s.url))
            .bearer_auth(&s.token)
            .json(&json!({
                "tool_call_id": "call-1", "tool": "bash", "is_error": false,
                "content": [{"type": "text", "text": format!("KEY={key}")}, image],
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(reply["content"][0]["text"], "KEY=[redacted:anthropic-key]");
        assert_eq!(reply["content"][1], image);
    }

    #[tokio::test]
    async fn a_result_over_the_budget_is_cut_saved_whole_and_recorded() {
        let s = setup(true).await;
        let worktree = s._dir.path().join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        s.app
            .store
            .set_run_worktree(s.run, &worktree.to_string_lossy())
            .unwrap();
        let key = format!("sk-ant-{}", "k".repeat(30));
        let text: String = (1..=500).map(|i| format!("line {i} {key}\n")).collect();
        let post = |body: Value| {
            let request = reqwest::Client::new()
                .post(format!("{}/moebius/result", s.url))
                .bearer_auth(&s.token)
                .json(&body);
            async move { request.send().await.unwrap().json::<Value>().await.unwrap() }
        };
        let content = json!([{"type": "text", "text": text}]);
        let reply = post(json!({
            "tool_call_id": "call-1", "tool": "bash", "input": {"command": "seq 500"}, "is_error": false,
            "content": content,
            "tool_output": {"hide_after_turns": 10, "max_lines": 200, "max_bytes": 16384, "compress": true},
        }))
        .await;
        let shown = reply["content"][0]["text"].as_str().unwrap();
        assert_eq!(shown.lines().count(), 201);
        assert!(shown.contains("lines left out; the full output is in .moebius/out/1.txt]"));
        assert!(!shown.contains(&key));
        let saved = std::fs::read_to_string(worktree.join(".moebius/out/1.txt")).unwrap();
        assert_eq!(saved.lines().count(), 500);
        assert!(
            !saved.contains(&key),
            "the saved output has no secrets either"
        );
        let events = s.app.store.events(s.run).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "tool_output");
        assert_eq!(events[0].data["tool_call_id"], "call-1");
        assert_eq!(events[0].data["steps"], json!(["budget"]));
        assert_eq!(events[0].data["lines_before"], 500);
        assert_eq!(events[0].data["bytes_after"], shown.len());
        assert_eq!(events[0].data["saved"], ".moebius/out/1.txt");
        // Without a budget the result is only cleaned of secrets.
        let reply = post(json!({ "content": content })).await;
        assert_eq!(
            reply["content"][0]["text"]
                .as_str()
                .unwrap()
                .lines()
                .count(),
            500
        );
        assert_eq!(s.app.store.events(s.run).unwrap().len(), 1);
    }
}
