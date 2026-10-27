//! The router (ADR 0006): Moebius is every harness's only model provider. It offers OpenAI- and Anthropic-compatible
//! endpoints on localhost, knows each request's run by its per-run token, forwards it to an upstream (a provider with an
//! API key, or a local server such as Radiance), records the tokens used against the run, and refuses requests once the
//! card's or run's token budget is spent. Provider keys stay in the router; harnesses only ever see their run's token.
//!
//! - `POST /v1/chat/completions`: OpenAI chat completions, streamed or not.
//! - `POST /v1/messages`: Anthropic messages, streamed or not.
//!
//! The server serves these routes together with the tool-call check (`check`) on one listener.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use axum::Router as Routes;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::store::{Store, Usage};

/// The protocol an endpoint or an upstream speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[serde(rename = "openai")]
    OpenAi,
    Anthropic,
}

/// Where requests of one protocol go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upstream {
    pub protocol: Protocol,
    /// The base URL the endpoint's path is appended to, such as `https://api.anthropic.com` or `http://127.0.0.1:8080`.
    pub base_url: String,
    /// The provider's key; none for a local server.
    pub api_key: Option<String>,
}

/// What a run's token allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub run_id: i64,
    pub card_id: Option<i64>,
    /// The card's token budget, or the run's when it has no card.
    pub budget: Option<i64>,
}

pub struct Router {
    store: Arc<Store>,
    upstreams: Vec<Upstream>,
    grants: RwLock<HashMap<String, Grant>>,
    client: reqwest::Client,
}

impl Router {
    pub fn new(store: Arc<Store>, upstreams: Vec<Upstream>) -> Self {
        Self {
            store,
            upstreams,
            grants: RwLock::new(HashMap::new()),
            client: reqwest::Client::new(),
        }
    }

    /// Issues a new token for a run; requests carrying it are counted against the run.
    pub fn issue_token(&self, grant: Grant) -> String {
        let mut bytes = [0u8; 24];
        getrandom::fill(&mut bytes).expect("the system has randomness");
        let token = format!("moebius-run-{}", hex::encode(bytes));
        self.grants
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .insert(token.clone(), grant);
        token
    }

    /// Withdraws a run's token when the run ends.
    pub fn revoke_token(&self, token: &str) {
        self.grants
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .remove(token);
    }

    /// How many tokens are valid, for tests that check a token is withdrawn.
    #[cfg(test)]
    pub fn token_count(&self) -> usize {
        self.grants.read().unwrap_or_else(|p| p.into_inner()).len()
    }

    pub fn routes(self: Arc<Self>) -> Routes {
        Routes::new()
            .route(
                "/v1/chat/completions",
                post(
                    |State(r): State<Arc<Router>>, headers: HeaderMap, body: Bytes| async move {
                        r.forward(Protocol::OpenAi, "/v1/chat/completions", headers, body)
                            .await
                    },
                ),
            )
            .route(
                "/v1/messages",
                post(
                    |State(r): State<Arc<Router>>, headers: HeaderMap, body: Bytes| async move {
                        r.forward(Protocol::Anthropic, "/v1/messages", headers, body)
                            .await
                    },
                ),
            )
            .with_state(self)
    }

    /// What the token in the request's headers allows, or none for an unknown or missing token.
    pub fn grant(&self, headers: &HeaderMap) -> Option<Grant> {
        let token = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()))?;
        self.grants
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(token)
            .cloned()
    }

    /// Tokens already used against the grant's budget.
    fn spent(&self, grant: &Grant) -> crate::store::Result<i64> {
        match grant.card_id {
            Some(card) => self.store.tokens_for_card(card),
            None => Ok(self
                .store
                .usage_for_run(grant.run_id)?
                .iter()
                .map(Usage::total)
                .sum()),
        }
    }

    /// A refusal when the grant's budget is spent or its usage cannot be read.
    async fn check_budget(self: Arc<Self>, protocol: Protocol, grant: &Grant) -> Option<Response> {
        let budget = grant.budget?;
        let g = grant.clone();
        match tokio::task::spawn_blocking(move || self.spent(&g)).await {
            Ok(Ok(spent)) if spent >= budget => Some(error(
                protocol,
                StatusCode::TOO_MANY_REQUESTS,
                "budget_exceeded",
                &format!("the token budget of {budget} is spent ({spent} used)"),
            )),
            Ok(Ok(_)) => None,
            _ => Some(error(
                protocol,
                StatusCode::INTERNAL_SERVER_ERROR,
                "api_error",
                "cannot read usage",
            )),
        }
    }

    async fn forward(
        self: Arc<Self>,
        protocol: Protocol,
        path: &str,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        let Some(grant) = self.grant(&headers) else {
            return error(
                protocol,
                StatusCode::UNAUTHORIZED,
                "authentication_error",
                "not a run's token",
            );
        };
        if let Some(refusal) = self.clone().check_budget(protocol, &grant).await {
            return refusal;
        }
        let Some(upstream) = self
            .upstreams
            .iter()
            .find(|u| u.protocol == protocol)
            .cloned()
        else {
            return error(
                protocol,
                StatusCode::BAD_GATEWAY,
                "api_error",
                "no upstream for this protocol",
            );
        };
        let Ok(mut request) = serde_json::from_slice::<Value>(&body) else {
            return error(
                protocol,
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "the body is not JSON",
            );
        };
        let streaming = request
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if streaming && protocol == Protocol::OpenAi {
            // Without this, an OpenAI stream does not say how many tokens it used.
            request["stream_options"]["include_usage"] = json!(true);
        }
        let model = request
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let mut outgoing = self
            .client
            .post(format!(
                "{}{}",
                upstream.base_url.trim_end_matches('/'),
                path
            ))
            .header(header::CONTENT_TYPE, "application/json")
            .body(request.to_string());
        for name in ["anthropic-version", "anthropic-beta"] {
            if let Some(value) = headers.get(name) {
                outgoing = outgoing.header(name, value);
            }
        }
        if let Some(key) = &upstream.api_key {
            outgoing = match protocol {
                Protocol::OpenAi => outgoing.bearer_auth(key),
                Protocol::Anthropic => outgoing.header("x-api-key", key),
            };
        }
        let upstream_response = match outgoing.send().await {
            Ok(r) => r,
            Err(e) => {
                return error(
                    protocol,
                    StatusCode::BAD_GATEWAY,
                    "api_error",
                    &format!("upstream: {e}"),
                );
            }
        };
        let status = upstream_response.status();
        let content_type = upstream_response
            .headers()
            .get(header::CONTENT_TYPE)
            .cloned();
        let meter = Meter {
            store: self.store.clone(),
            run_id: grant.run_id,
            model,
        };

        let mut response = if streaming && status.is_success() {
            let mut tally = Tally {
                scanner: SseUsage::default(),
                meter,
            };
            let stream = upstream_response.bytes_stream().map(move |chunk| {
                if let Ok(bytes) = &chunk {
                    tally.scanner.feed(bytes, protocol);
                }
                chunk
            });
            Body::from_stream(stream).into_response()
        } else {
            let bytes = match upstream_response.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    return error(
                        protocol,
                        StatusCode::BAD_GATEWAY,
                        "api_error",
                        &format!("upstream: {e}"),
                    );
                }
            };
            if status.is_success()
                && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
            {
                let mut usage = Counted::default();
                usage.read(&value, protocol);
                if let Some(m) = value.get("model").and_then(Value::as_str) {
                    usage.model = Some(m.to_string());
                }
                meter.record(usage);
            }
            Body::from(bytes).into_response()
        };
        *response.status_mut() = status;
        if let Some(ct) = content_type {
            response.headers_mut().insert(header::CONTENT_TYPE, ct);
        }
        response
    }
}

/// Tokens counted from one response.
#[derive(Debug, Default, Clone)]
struct Counted {
    input: i64,
    output: i64,
    cached: i64,
    model: Option<String>,
    seen: bool,
}

impl Counted {
    /// Reads the usage of a response body or a stream event; later numbers replace earlier ones of the same kind.
    fn read(&mut self, value: &Value, protocol: Protocol) {
        let usage = match protocol {
            Protocol::OpenAi => value.get("usage"),
            // Streams carry usage in message_start (as message.usage) and in message_delta (as usage).
            Protocol::Anthropic => value
                .get("usage")
                .or_else(|| value.get("message").and_then(|m| m.get("usage"))),
        };
        let Some(usage) = usage.filter(|u| u.is_object()) else {
            return;
        };
        let int = |key: &str| usage.get(key).and_then(Value::as_i64);
        match protocol {
            Protocol::OpenAi => {
                if let Some(n) = int("prompt_tokens") {
                    self.input = n;
                }
                if let Some(n) = int("completion_tokens") {
                    self.output = n;
                }
                if let Some(n) = usage
                    .get("prompt_tokens_details")
                    .and_then(|d| d.get("cached_tokens"))
                    .and_then(Value::as_i64)
                {
                    self.cached = n;
                }
            }
            Protocol::Anthropic => {
                if let Some(n) = int("input_tokens") {
                    self.input = n;
                }
                if let Some(n) = int("output_tokens") {
                    self.output = n;
                }
                if let Some(n) = int("cache_read_input_tokens") {
                    self.cached = n;
                }
            }
        }
        self.seen = true;
    }
}

/// Finds the usage in a server-sent event stream as it passes through.
#[derive(Default)]
struct SseUsage {
    pending: Vec<u8>,
    usage: Counted,
}

impl SseUsage {
    fn feed(&mut self, bytes: &[u8], protocol: Protocol) {
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(data) = line.trim_end().strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(data) {
                if let Some(m) = value
                    .get("model")
                    .or_else(|| value.get("message").and_then(|m| m.get("model")))
                    .and_then(Value::as_str)
                {
                    self.usage.model = Some(m.to_string());
                }
                self.usage.read(&value, protocol);
            }
        }
    }
}

/// Counts a stream's usage as it passes and records it when the stream ends, however it ends.
struct Tally {
    scanner: SseUsage,
    meter: Meter,
}

impl Drop for Tally {
    fn drop(&mut self) {
        self.meter.record(std::mem::take(&mut self.scanner.usage));
    }
}

/// Records counted usage against a run.
struct Meter {
    store: Arc<Store>,
    run_id: i64,
    model: String,
}

impl Meter {
    fn record(&self, counted: Counted) {
        if !counted.seen {
            return;
        }
        let _ = self.store.add_usage(&Usage {
            run_id: Some(self.run_id),
            model: counted.model.unwrap_or_else(|| self.model.clone()),
            input_tokens: counted.input,
            output_tokens: counted.output,
            cached_tokens: counted.cached,
        });
    }
}

/// An error in the shape the protocol's clients expect.
fn error(protocol: Protocol, status: StatusCode, kind: &str, message: &str) -> Response {
    let body = match protocol {
        Protocol::OpenAi => json!({ "error": { "type": kind, "message": message } }),
        Protocol::Anthropic => {
            json!({ "type": "error", "error": { "type": kind, "message": message } })
        }
    };
    (status, axum::Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// What the fake upstream saw.
    #[derive(Default)]
    struct Seen {
        auth: Vec<String>,
        bodies: Vec<Value>,
    }

    const ANTHROPIC_STREAM: &str = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-sonnet-5-5\",\"usage\":{\"input_tokens\":20,\"output_tokens\":1,\"cache_read_input_tokens\":4}}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":7}}\n\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\n";

    const OPENAI_STREAM: &str = "data: {\"model\":\"qwen-local\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n\
data: {\"model\":\"qwen-local\",\"choices\":[],\"usage\":{\"prompt_tokens\":30,\"completion_tokens\":3}}\n\n\
data: [DONE]\n\n";

    async fn fake_upstream(seen: Arc<Mutex<Seen>>) -> String {
        let app = Routes::new()
            .route(
                "/v1/chat/completions",
                post({
                    let seen = seen.clone();
                    move |headers: HeaderMap, axum::Json(body): axum::Json<Value>| async move {
                        let auth = headers
                            .get("authorization")
                            .map(|v| v.to_str().unwrap().to_string())
                            .unwrap_or_default();
                        let streaming = body["stream"] == json!(true);
                        seen.lock().unwrap().auth.push(auth);
                        seen.lock().unwrap().bodies.push(body);
                        if streaming {
                            ([(header::CONTENT_TYPE, "text/event-stream")], OPENAI_STREAM)
                                .into_response()
                        } else {
                            axum::Json(json!({
                                "model": "qwen-local",
                                "choices": [{"message": {"role": "assistant", "content": "Hi"}}],
                                "usage": {"prompt_tokens": 10, "completion_tokens": 5,
                                          "prompt_tokens_details": {"cached_tokens": 2}}
                            }))
                            .into_response()
                        }
                    }
                }),
            )
            .route(
                "/v1/messages",
                post(
                    move |headers: HeaderMap, axum::Json(body): axum::Json<Value>| async move {
                        let key = headers
                            .get("x-api-key")
                            .map(|v| v.to_str().unwrap().to_string())
                            .unwrap_or_default();
                        seen.lock().unwrap().auth.push(key);
                        seen.lock().unwrap().bodies.push(body);
                        (
                            [(header::CONTENT_TYPE, "text/event-stream")],
                            ANTHROPIC_STREAM,
                        )
                    },
                ),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}")
    }

    struct Setup {
        _dir: tempfile::TempDir,
        store: Arc<Store>,
        router: Arc<Router>,
        url: String,
        seen: Arc<Mutex<Seen>>,
        card: i64,
        run: i64,
    }

    async fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let repo = store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let card = store
            .add_card(repo.id, "Card", "", "person", "In progress")
            .unwrap();
        let run = store
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        let seen = Arc::new(Mutex::new(Seen::default()));
        let upstream = fake_upstream(seen.clone()).await;
        let router = Arc::new(Router::new(
            store.clone(),
            vec![
                Upstream {
                    protocol: Protocol::OpenAi,
                    base_url: upstream.clone(),
                    api_key: None,
                },
                Upstream {
                    protocol: Protocol::Anthropic,
                    base_url: upstream,
                    api_key: Some("sk-provider-key".into()),
                },
            ],
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let routes = router.clone().routes();
        tokio::spawn(async move { axum::serve(listener, routes).await });
        Setup {
            _dir: dir,
            store,
            router,
            url,
            seen,
            card: card.id,
            run: run.id,
        }
    }

    #[tokio::test]
    async fn a_request_goes_upstream_and_its_usage_is_stored_against_the_run() {
        let s = setup().await;
        let token = s.router.issue_token(Grant {
            run_id: s.run,
            card_id: Some(s.card),
            budget: None,
        });
        let client = reqwest::Client::new();
        let reply: Value = client
            .post(format!("{}/v1/chat/completions", s.url))
            .bearer_auth(&token)
            .json(&json!({"model": "local", "messages": [{"role": "user", "content": "hi"}]}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(reply["choices"][0]["message"]["content"], "Hi");
        let usage = s.store.usage_for_run(s.run).unwrap();
        assert_eq!(usage.len(), 1);
        assert_eq!(
            (
                usage[0].model.as_str(),
                usage[0].input_tokens,
                usage[0].output_tokens,
                usage[0].cached_tokens
            ),
            ("qwen-local", 10, 5, 2)
        );
        // The upstream never sees the run's token.
        assert_eq!(s.seen.lock().unwrap().auth, vec![String::new()]);
    }

    #[tokio::test]
    async fn streamed_requests_are_counted_for_both_protocols() {
        let s = setup().await;
        let token = s.router.issue_token(Grant {
            run_id: s.run,
            card_id: Some(s.card),
            budget: None,
        });
        let client = reqwest::Client::new();
        let text = client
            .post(format!("{}/v1/messages", s.url))
            .header("x-api-key", &token)
            .header("anthropic-version", "2023-06-01")
            .json(&json!({"model": "claude-sonnet-5-5", "stream": true, "max_tokens": 10, "messages": []}))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(text, ANTHROPIC_STREAM);
        let text = client
            .post(format!("{}/v1/chat/completions", s.url))
            .bearer_auth(&token)
            .json(&json!({"model": "local", "stream": true, "messages": []}))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(text, OPENAI_STREAM);

        // A stream's usage is recorded when the server drops it, which may be just after the client has read it.
        for _ in 0..100 {
            if s.store.usage_for_run(s.run).unwrap().len() == 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let seen = s.seen.lock().unwrap();
        assert_eq!(seen.auth[0], "sk-provider-key");
        assert_eq!(
            seen.bodies[1]["stream_options"]["include_usage"],
            json!(true)
        );
        let usage = s.store.usage_for_run(s.run).unwrap();
        let counted: Vec<_> = usage
            .iter()
            .map(|u| {
                (
                    u.model.as_str(),
                    u.input_tokens,
                    u.output_tokens,
                    u.cached_tokens,
                )
            })
            .collect();
        assert_eq!(
            counted,
            [("claude-sonnet-5-5", 20, 7, 4), ("qwen-local", 30, 3, 0)]
        );
        assert_eq!(s.store.tokens_for_card(s.card).unwrap(), 60);
    }

    #[tokio::test]
    async fn requests_over_budget_or_without_a_run_token_are_refused() {
        let s = setup().await;
        let token = s.router.issue_token(Grant {
            run_id: s.run,
            card_id: Some(s.card),
            budget: Some(15),
        });
        let client = reqwest::Client::new();
        let send = |token: String| {
            client
                .post(format!("{}/v1/chat/completions", s.url))
                .bearer_auth(token)
                .json(&json!({"model": "local", "messages": []}))
                .send()
        };
        assert_eq!(send(token.clone()).await.unwrap().status(), 200);
        // 15 tokens are now spent; the budget is 15.
        let refused = send(token.clone()).await.unwrap();
        assert_eq!(refused.status(), 429);
        let body: Value = refused.json().await.unwrap();
        assert_eq!(body["error"]["type"], "budget_exceeded");
        assert_eq!(s.seen.lock().unwrap().bodies.len(), 1);

        assert_eq!(send("wrong".into()).await.unwrap().status(), 401);
        s.router.revoke_token(&token);
        assert_eq!(send(token).await.unwrap().status(), 401);
    }
}
