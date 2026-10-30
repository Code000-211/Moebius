//! The tests of the Codex Responses translation against a fake Codex endpoint and a fake provider that gives the
//! login's tokens: a plain answer, a streamed one, tool calls (allowed, and refused by the check), an error passed
//! on, and the usage counted against the run.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Router as Routes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};

use super::*;
use crate::decide::check::Checker;
use crate::router::oauth::testing::{ACCOUNT, FakeProvider, preset_config};
use crate::router::{Grant, Protocol};
use crate::server::settings::Secrets;
use crate::store::Store;

const TEXT: &str = "event: response.created\n\
data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-6-sol\"}}\n\n\
event: response.reasoning_summary_text.delta\n\
data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"thinking\"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hel\"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"lo\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":12,\"output_tokens\":5,\"total_tokens\":17,\"input_tokens_details\":{\"cached_tokens\":4}}}}\n\n";

/// A call to `read` whose arguments come in events, and one to `bash` whose arguments come whole.
const TOOLS: &str = "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_2\"}}\n\n\
data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_read\",\"name\":\"read\",\"arguments\":\"\"}}\n\n\
data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"delta\":\"{\\\"path\\\":\"}\n\n\
data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"delta\":\"\\\"a.rs\\\"}\"}\n\n\
data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_read\",\"name\":\"read\",\"arguments\":\"{\\\"path\\\":\\\"a.rs\\\"}\"}}\n\n\
data: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_bash\",\"name\":\"bash\",\"arguments\":\"{\\\"command\\\":\\\"ls\\\"}\"}}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":30,\"output_tokens\":9}}}\n\n";

/// What the fake Codex endpoint saw and what it answers.
struct Codex {
    status: StatusCode,
    content_type: &'static str,
    answer: String,
    seen: Vec<(HashMap<String, String>, Value)>,
}

async fn codex(
    State(state): State<Arc<Mutex<Codex>>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let mut codex = state.lock().unwrap();
    let headers = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap().to_string()))
        .collect();
    codex
        .seen
        .push((headers, serde_json::from_str(&body).unwrap()));
    (
        codex.status,
        [(header::CONTENT_TYPE, codex.content_type)],
        codex.answer.clone(),
    )
        .into_response()
}

struct Setup {
    _dir: tempfile::TempDir,
    provider: FakeProvider,
    codex: Arc<Mutex<Codex>>,
    store: Arc<Store>,
    router: Arc<Router>,
    url: String,
    token: String,
    run: i64,
}

/// A router whose only upstream is the `openai` preset logged in at the fake provider and served by the fake Codex
/// endpoint; `checked` makes the token one whose answers are checked, by a decider that refuses `bash`.
async fn setup(answer: &str, checked: bool) -> Setup {
    let provider = FakeProvider::start(3600).await;
    let state = Arc::new(Mutex::new(Codex {
        status: StatusCode::OK,
        content_type: "text/event-stream",
        answer: answer.to_string(),
        seen: Vec::new(),
    }));
    let app = Routes::new()
        .route("/codex/responses", post(codex))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });

    let (dir, store, run, checker) = if checked {
        let (dir, app, decider) = crate::router::calls::tests::decider();
        let checker = Checker::new(app.clone(), None::<PathBuf>);
        (dir, app.store.clone(), decider.grant.run_id, Some(checker))
    } else {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let run = store.add_run(None, "Coder", "pi-bolt", "code").unwrap().id;
        (dir, store, run, None)
    };
    Secrets::new(dir.path())
        .set("oauth-openai", "refresh-token-0")
        .unwrap();
    let upstream = Upstream {
        protocol: Protocol::OpenAi,
        base_url,
        api_key: None,
        oauth: Some(preset_config(&provider.url, "openai")),
    };
    let router = Router::new(store.clone(), vec![upstream]).with_secrets(Secrets::new(dir.path()));
    let router = Arc::new(router);
    let grant = Grant {
        run_id: run,
        card_id: None,
        budget: None,
    };
    let token = match checker {
        Some(checker) => {
            router.set_checker(checker);
            router.issue_checked_token(grant)
        }
        None => router.issue_token(grant),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let routes = router.clone().routes();
    tokio::spawn(async move { axum::serve(listener, routes).await });
    Setup {
        _dir: dir,
        provider,
        codex: state,
        store,
        router,
        url,
        token,
        run,
    }
}

impl Setup {
    async fn ask(&self, stream: bool) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/v1/chat/completions", self.url))
            .bearer_auth(&self.token)
            .json(&json!({
                "model": "gpt-6-sol",
                "stream": stream,
                "messages": [
                    {"role": "system", "content": "Be brief."},
                    {"role": "user", "content": "Read a.rs"},
                    {"role": "assistant", "content": null, "tool_calls": [
                        {"id": "call_1", "type": "function",
                         "function": {"name": "read", "arguments": "{\"path\":\"a.rs\"}"}}]},
                    {"role": "tool", "tool_call_id": "call_1", "content": "fn main() {}"},
                    {"role": "user", "content": [
                        {"type": "text", "text": "And?"},
                        {"type": "image_url", "image_url": {"url": "data:image/png;base64,AA"}}]},
                ],
                "tools": [{"type": "function", "function": {
                    "name": "read", "description": "Reads a file",
                    "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}}}],
                "tool_choice": {"type": "function", "function": {"name": "read"}},
                "reasoning_effort": "low",
                "max_tokens": 100,
            }))
            .send()
            .await
            .unwrap()
    }

    fn seen(&self) -> Vec<(HashMap<String, String>, Value)> {
        self.codex.lock().unwrap().seen.clone()
    }

    /// The tokens recorded against the run, with the model.
    fn usage(&self) -> Vec<(String, i64)> {
        self.store
            .usage_for_run(self.run)
            .unwrap()
            .into_iter()
            .map(|u| (u.model.clone(), u.total()))
            .collect()
    }
}

/// The JSON of each `data:` line of a stream, and whether it ended with `[DONE]`.
fn events(body: &str) -> (Vec<Value>, bool) {
    let lines: Vec<&str> = body
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .collect();
    let done = lines.last() == Some(&"[DONE]");
    let values = lines
        .into_iter()
        .filter(|data| *data != "[DONE]")
        .map(|data| serde_json::from_str(data).unwrap())
        .collect();
    (values, done)
}

#[tokio::test]
async fn a_plain_request_is_translated_to_responses_and_the_answer_back() {
    let s = setup(TEXT, false).await;
    let reply = s.ask(false).await;
    assert_eq!(reply.status(), 200);
    assert_eq!(reply.headers()["content-type"], "application/json");
    let body: Value = reply.json().await.unwrap();
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["choices"][0]["message"]["content"], "Hello");
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
    assert_eq!(body["usage"]["prompt_tokens"], 12);
    assert_eq!(body["usage"]["completion_tokens"], 5);
    assert_eq!(body["usage"]["prompt_tokens_details"]["cached_tokens"], 4);

    let seen = s.seen();
    assert_eq!(seen.len(), 1);
    let (headers, request) = &seen[0];
    assert_eq!(
        headers["authorization"],
        format!("Bearer {}", s.provider.access_token(1))
    );
    assert_eq!(headers["chatgpt-account-id"], ACCOUNT);
    assert_eq!(headers["originator"], "moebius");
    assert_eq!(headers["openai-beta"], "responses=experimental");
    assert_eq!(headers["accept"], "text/event-stream");
    // The run's token is not sent; a hash of it names the run to the cache.
    assert!(!headers.values().any(|v| v.contains(&s.token)));
    assert_eq!(
        headers["session-id"],
        request["prompt_cache_key"].as_str().unwrap()
    );
    assert_eq!(request["model"], "gpt-6-sol");
    assert_eq!(request["stream"], true);
    assert_eq!(request["store"], false);
    assert_eq!(request["instructions"], "Be brief.");
    assert_eq!(
        request["tool_choice"],
        json!({"type": "function", "name": "read"})
    );
    assert_eq!(request["reasoning"]["effort"], "low");
    assert!(request.get("max_tokens").is_none() && request.get("messages").is_none());
    assert_eq!(
        request["tools"],
        json!([{"type": "function", "name": "read", "description": "Reads a file", "strict": null,
            "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}}])
    );
    let input = request["input"].as_array().unwrap();
    assert_eq!(
        input[0],
        json!({"role": "user", "content": [{"type": "input_text", "text": "Read a.rs"}]})
    );
    assert_eq!(
        input[1],
        json!({"type": "function_call", "call_id": "call_1", "name": "read", "arguments": "{\"path\":\"a.rs\"}"})
    );
    assert_eq!(
        input[2],
        json!({"type": "function_call_output", "call_id": "call_1", "output": "fn main() {}"})
    );
    assert_eq!(
        input[3]["content"],
        json!([{"type": "input_text", "text": "And?"},
            {"type": "input_image", "image_url": "data:image/png;base64,AA"}])
    );
    // The tokens are counted against the run, as for any upstream.
    assert_eq!(s.usage(), [("gpt-6-sol".to_string(), 17)]);
}

#[tokio::test]
async fn a_streamed_request_gets_chat_completion_chunks_and_its_usage_counted() {
    let s = setup(TEXT, false).await;
    let reply = s.ask(true).await;
    assert_eq!(reply.status(), 200);
    let (chunks, done) = events(&reply.text().await.unwrap());
    assert!(done);
    assert_eq!(chunks[0]["choices"][0]["delta"]["role"], "assistant");
    let text: String = chunks
        .iter()
        .filter_map(|c| c["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(text, "Hello");
    assert!(chunks.iter().all(|c| c["model"] == "gpt-6-sol"));
    let finish = chunks
        .iter()
        .find(|c| c["choices"][0]["finish_reason"] == "stop")
        .expect("a chunk finishes the message");
    assert_eq!(finish["choices"][0]["delta"], json!({}));
    let last = chunks.last().unwrap();
    assert_eq!(last["choices"], json!([]));
    assert_eq!(last["usage"]["prompt_tokens"], 12);
    assert_eq!(s.usage(), [("gpt-6-sol".to_string(), 17)]);
}

#[tokio::test]
async fn tool_calls_are_carried_over_whole_and_in_pieces_with_the_finish_reason() {
    let s = setup(TOOLS, false).await;
    let body: Value = s.ask(false).await.json().await.unwrap();
    let calls = &body["choices"][0]["message"]["tool_calls"];
    assert_eq!(calls[0]["id"], "call_read");
    assert_eq!(calls[0]["function"]["name"], "read");
    assert_eq!(calls[0]["function"]["arguments"], "{\"path\":\"a.rs\"}");
    assert_eq!(calls[1]["id"], "call_bash");
    assert_eq!(calls[1]["function"]["arguments"], "{\"command\":\"ls\"}");
    assert_eq!(body["choices"][0]["finish_reason"], "tool_calls");
    assert!(body["choices"][0]["message"]["content"].is_null());

    let (chunks, done) = events(&s.ask(true).await.text().await.unwrap());
    assert!(done);
    let mut arguments = [String::new(), String::new()];
    for call in chunks.iter().flat_map(|c| {
        c["choices"][0]["delta"]["tool_calls"]
            .as_array()
            .into_iter()
            .flatten()
    }) {
        arguments[call["index"].as_u64().unwrap() as usize]
            .push_str(call["function"]["arguments"].as_str().unwrap_or_default());
    }
    assert_eq!(arguments, ["{\"path\":\"a.rs\"}", "{\"command\":\"ls\"}"]);
    assert!(
        chunks
            .iter()
            .any(|c| c["choices"][0]["finish_reason"] == "tool_calls")
    );
    assert_eq!(s.usage().iter().map(|(_, n)| n).sum::<i64>(), 78);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_tool_call_is_replaced_by_the_check_whole_and_streamed() {
    let s = setup(TOOLS, true).await;
    let body: Value = s.ask(false).await.json().await.unwrap();
    let message = &body["choices"][0]["message"];
    let calls = message["tool_calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["function"]["name"], "read");
    assert!(
        message["content"]
            .as_str()
            .unwrap()
            .contains("rule no-bash"),
        "{message}"
    );

    let (chunks, done) = events(&s.ask(true).await.text().await.unwrap());
    assert!(done);
    let names: Vec<&str> = chunks
        .iter()
        .flat_map(|c| {
            c["choices"][0]["delta"]["tool_calls"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(|call| call["function"]["name"].as_str())
        .collect();
    assert_eq!(names, ["read"]);
    let text: String = chunks
        .iter()
        .filter_map(|c| c["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert!(
        text.contains("Moebius refused the tool call bash"),
        "{text}"
    );
}

#[tokio::test]
async fn an_error_of_the_backend_is_passed_on_with_its_status() {
    let s = setup(TEXT, false).await;
    {
        let mut codex = s.codex.lock().unwrap();
        codex.status = StatusCode::BAD_REQUEST;
        codex.content_type = "application/json";
        codex.answer =
            json!({"error": {"message": "not supported", "type": "invalid_request_error"}})
                .to_string();
    }
    for stream in [false, true] {
        let reply = s.ask(stream).await;
        assert_eq!(reply.status(), 400);
        let body: Value = reply.json().await.unwrap();
        assert_eq!(body["error"]["message"], "not supported");
    }
    // An answer in the backend's own shape is given the shape clients read.
    s.codex.lock().unwrap().answer = json!({"detail": "The model is not supported"}).to_string();
    let body: Value = s.ask(false).await.json().await.unwrap();
    assert_eq!(body["error"]["message"], "The model is not supported");
    assert!(s.usage().is_empty());
}

#[tokio::test]
async fn an_error_event_or_a_cut_off_answer_ends_the_stream_with_an_error() {
    let s = setup(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\n\
         data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"server_error\",\"message\":\"overloaded\"}}}\n\n",
        false,
    )
    .await;
    let (chunks, done) = events(&s.ask(true).await.text().await.unwrap());
    assert!(done);
    assert_eq!(chunks.last().unwrap()["error"]["message"], "overloaded");
    let reply = s.ask(false).await;
    assert_eq!(reply.status(), 502);
    let body: Value = reply.json().await.unwrap();
    assert_eq!(body["error"]["message"], "overloaded");

    s.codex.lock().unwrap().answer =
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\n".into();
    let (chunks, done) = events(&s.ask(true).await.text().await.unwrap());
    assert!(done);
    assert!(
        chunks.last().unwrap()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("ended before")
    );
}

#[tokio::test]
async fn a_cut_off_message_and_a_text_that_came_whole_finish_as_length_and_stop() {
    let s = setup(
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"Whole\"}]}}\n\n\
         data: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\",\"incomplete_details\":{\"reason\":\"max_output_tokens\"}}}\n\n",
        false,
    )
    .await;
    let body: Value = s.ask(false).await.json().await.unwrap();
    assert_eq!(body["choices"][0]["message"]["content"], "Whole");
    assert_eq!(body["choices"][0]["finish_reason"], "length");
}

#[tokio::test]
async fn the_models_are_the_subscriptions_and_need_a_login() {
    let s = setup(TEXT, false).await;
    let upstream = s.router.upstreams_of(Protocol::OpenAi).remove(0);
    let listed = s.router.list_models(&upstream).await.unwrap();
    assert_eq!(listed.len(), MODELS.len());
    assert_eq!(listed[0], "gpt-6.1-sol");
    // Without a refresh token there is no login.
    let dir = tempfile::tempdir().unwrap();
    let fresh =
        Router::new(s.store.clone(), vec![upstream.clone()]).with_secrets(Secrets::new(dir.path()));
    assert!(fresh.list_models(&upstream).await.is_err());
}

#[test]
fn the_account_comes_from_the_claims_of_the_token() {
    let token = FakeProvider::token_for_tests("acct-1");
    assert_eq!(account_id(&token).as_deref(), Some("acct-1"));
    assert_eq!(account_id("access-token"), None);
    assert_eq!(account_id("a.b.c"), None);
    assert_eq!(account_id(&format!("{token}.extra")), None);
    assert_eq!(account_id(&FakeProvider::token_for_tests("")), None);
}

#[test]
fn the_endpoint_is_appended_to_a_base_url_that_does_not_name_it() {
    for base in [
        "https://chatgpt.com/backend-api",
        "https://chatgpt.com/backend-api/",
        "https://chatgpt.com/backend-api/codex",
        "https://chatgpt.com/backend-api/codex/responses",
    ] {
        assert_eq!(
            endpoint(base),
            "https://chatgpt.com/backend-api/codex/responses"
        );
    }
}

#[test]
fn system_messages_after_the_first_turn_become_developer_messages() {
    let chat = json!({"model": "m", "messages": [
        {"role": "system", "content": "A"}, {"role": "developer", "content": [{"type": "text", "text": "B"}]},
        {"role": "user", "content": "Hi"}, {"role": "system", "content": "C"}]});
    let body = request(&chat, "key");
    assert_eq!(body["instructions"], "A\n\nB");
    assert_eq!(body["input"][1]["role"], "developer");
    assert_eq!(body["input"][1]["content"][0]["text"], "C");
    assert_eq!(body["tool_choice"], "auto");
    assert!(body.get("tools").is_none() && body.get("reasoning").is_none());
    let bare = request(&json!({"model": "m", "messages": []}), "key");
    assert_eq!(bare["instructions"], NO_INSTRUCTIONS);
}
