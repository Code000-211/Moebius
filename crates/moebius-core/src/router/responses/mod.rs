//! The Codex Responses protocol of the ChatGPT backend, which a ChatGPT Plus/Pro subscription (the `openai` preset of
//! `oauth`) is served in. A harness sends the router chat completions; for such an upstream the router turns the
//! request into a Responses request (`POST {base_url}/codex/responses`, always streamed, `store: false`) and the answer,
//! streamed or not, back into chat completions (`answer`), so that everything after the translation (the check of
//! tool calls, the counting of tokens, the budgets) is the path of every other upstream.
//!
//! This follows the `openai-codex` provider of Pi (commit `f09d01b85e0384647fcab6779d35ab9dabe04be3` of
//! `badlogic/pi-mono`, `packages/ai/src/api/openai-codex-responses.ts`, and the Responses conversion it shares in
//! `openai-responses-shared.ts`): the URL, the body, and the headers `chatgpt-account-id` (read from the claims of the
//! access token), `originator`, `OpenAI-Beta: responses=experimental`, `session-id` and `x-client-request-id`. The
//! models are the ones its catalog (`packages/ai/catalog/providers/openai-codex.ts`) lists for a ChatGPT account.
//! Pi's WebSocket transport, reasoning replay and tool search are not used.
//!
//! This module holds the request, its headers and the model list; `answer` reads the answer.

pub(super) mod answer;
#[cfg(test)]
mod tests;

use axum::http::HeaderMap;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use reqwest::RequestBuilder;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::{OAuthConfig, Router, Upstream};

/// The models the ChatGPT backend serves to a subscription, as Pi lists them.
pub(super) const MODELS: &[&str] = &[
    "gpt-6.1-sol",
    "gpt-6.1-terra",
    "gpt-6-astra",
    "gpt-6-sol",
    "gpt-6-luna",
    "gpt-5.6-luna",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
];

/// Where the token's claims name the account.
const CLAIMS: &str = "https://api.openai.com/auth";

/// The instructions of a request that has no system message; the backend refuses a request without any.
const NO_INSTRUCTIONS: &str = "You are a helpful assistant.";

/// The ChatGPT account an access token (a JWT) is for, from its claims.
pub(super) fn account_id(token: &str) -> Option<String> {
    let mut parts = token.split('.');
    let (_, payload, _, None) = (parts.next()?, parts.next()?, parts.next()?, parts.next()) else {
        return None;
    };
    let payload = payload.trim_end_matches('=');
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| STANDARD_NO_PAD.decode(payload))
        .ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims[CLAIMS]["chatgpt_account_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// The URL of the Responses endpoint for an upstream's base URL, which may already name it.
fn endpoint(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/codex/responses") {
        base.to_string()
    } else if base.ends_with("/codex") {
        format!("{base}/responses")
    } else {
        format!("{base}/codex/responses")
    }
}

/// What names a run to the backend's prompt cache: a hash of the run's token, which is never sent.
fn cache_key(harness: &HeaderMap) -> String {
    let token = super::grants::token(harness).unwrap_or_default();
    hex::encode(Sha256::digest(token.as_bytes()))[..32].to_string()
}

/// The text of a message's content: a string, or the text parts of an array.
fn text_of(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The input parts of a user message's content.
fn user_parts(content: &Value) -> Vec<Value> {
    let text = |text: &str| json!({ "type": "input_text", "text": text });
    match content {
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| match part["type"].as_str() {
                Some("text") => Some(text(part["text"].as_str().unwrap_or_default())),
                Some("image_url") => {
                    let url = part["image_url"]["url"]
                        .as_str()
                        .or_else(|| part["image_url"].as_str())?;
                    Some(json!({ "type": "input_image", "image_url": url }))
                }
                _ => None,
            })
            .collect(),
        other => vec![text(&text_of(other))],
    }
}

/// The Responses input items and instructions of a chat's messages: the system messages before the first other one are
/// the instructions, a later one is a developer message.
fn conversation(messages: &[Value]) -> (String, Vec<Value>) {
    let (mut instructions, mut input) = (Vec::<String>::new(), Vec::<Value>::new());
    for message in messages {
        let content = &message["content"];
        match message["role"].as_str().unwrap_or_default() {
            "system" | "developer" if input.is_empty() => instructions.push(text_of(content)),
            "system" | "developer" => input.push(json!({
                "role": "developer",
                "content": [{ "type": "input_text", "text": text_of(content) }],
            })),
            "user" => input.push(json!({ "role": "user", "content": user_parts(content) })),
            "assistant" => {
                let text = text_of(content);
                if !text.is_empty() {
                    input.push(json!({
                        "type": "message", "role": "assistant", "status": "completed",
                        "id": format!("msg_{}", input.len()),
                        "content": [{ "type": "output_text", "text": text, "annotations": [] }],
                    }));
                }
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": call["id"],
                        "name": call["function"]["name"],
                        "arguments": call["function"]["arguments"],
                    }));
                }
            }
            "tool" => input.push(json!({
                "type": "function_call_output",
                "call_id": message["tool_call_id"],
                "output": text_of(content),
            })),
            _ => {}
        }
    }
    (instructions.join("\n\n"), input)
}

/// The Responses request for a chat completions request. `cache_key` names the run to the prompt cache.
pub(super) fn request(chat: &Value, cache_key: &str) -> Value {
    let (instructions, input) =
        conversation(chat["messages"].as_array().map_or(&[], Vec::as_slice));
    let tool_choice = match &chat["tool_choice"] {
        Value::String(choice) => json!(choice),
        Value::Object(_) => {
            json!({ "type": "function", "name": chat["tool_choice"]["function"]["name"] })
        }
        _ => json!("auto"),
    };
    let mut body = Map::new();
    body.insert("model".into(), chat["model"].clone());
    body.insert("store".into(), json!(false));
    body.insert("stream".into(), json!(true));
    body.insert(
        "instructions".into(),
        json!(if instructions.is_empty() {
            NO_INSTRUCTIONS
        } else {
            &instructions
        }),
    );
    body.insert("input".into(), Value::Array(input));
    body.insert("text".into(), json!({ "verbosity": "low" }));
    body.insert("prompt_cache_key".into(), json!(cache_key));
    body.insert("tool_choice".into(), tool_choice);
    body.insert("parallel_tool_calls".into(), json!(true));
    if let Some(tools) = chat["tools"].as_array().filter(|tools| !tools.is_empty()) {
        let tools: Vec<Value> = tools
            .iter()
            .map(|tool| {
                let function = &tool["function"];
                json!({
                    "type": "function",
                    "name": function["name"],
                    "description": function["description"],
                    "parameters": function["parameters"],
                    "strict": null,
                })
            })
            .collect();
        body.insert("tools".into(), Value::Array(tools));
    }
    if let Some(temperature) = chat.get("temperature").filter(|t| t.is_number()) {
        body.insert("temperature".into(), temperature.clone());
    }
    if let Some(effort) = chat["reasoning_effort"].as_str() {
        body.insert(
            "reasoning".into(),
            json!({ "effort": effort, "summary": "auto" }),
        );
    }
    Value::Object(body)
}

impl Router {
    /// The request to the Responses endpoint of `upstream` for the chat completions request `chat` of a harness,
    /// with the upstream's login. The message of a failure says nothing secret.
    pub(super) async fn codex_request(
        &self,
        upstream: &Upstream,
        oauth: &OAuthConfig,
        chat: &Value,
        harness: &HeaderMap,
    ) -> Result<RequestBuilder, String> {
        let token = self.access_token(oauth).await?;
        let account = account_id(&token)
            .ok_or("the login's token names no ChatGPT account: log in again on the Models page")?;
        let key = cache_key(harness);
        Ok(self
            .client
            .post(endpoint(&upstream.base_url))
            .bearer_auth(token)
            .header("chatgpt-account-id", account)
            .header("originator", "moebius")
            .header("OpenAI-Beta", "responses=experimental")
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("session-id", &key)
            .header("x-client-request-id", &key)
            .body(request(chat, &key).to_string()))
    }
}

/// The body to pass on for an error the backend answered with: its own when it has the `error` object clients read,
/// otherwise its `detail` or `message` (or the text) in that shape.
pub(super) fn error_body(raw: &[u8]) -> Vec<u8> {
    let parsed: Option<Value> = serde_json::from_slice(raw).ok();
    if parsed.as_ref().is_some_and(|v| v["error"].is_object()) {
        return raw.to_vec();
    }
    let message = parsed
        .as_ref()
        .and_then(|v| {
            v["detail"]
                .as_str()
                .or_else(|| v["message"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| {
            String::from_utf8_lossy(raw)
                .trim()
                .chars()
                .take(500)
                .collect()
        });
    json!({ "error": { "type": "api_error", "message": message } })
        .to_string()
        .into_bytes()
}
