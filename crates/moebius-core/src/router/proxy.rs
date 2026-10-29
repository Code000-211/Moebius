//! Forwarding a run's request to an upstream of its protocol (chosen by `classes`), with the upstream's key or login in
//! place of the run's token, and the answer back, streamed or not, its usage counted on the way. The answer to a checked
//! token has its tool calls checked on the way (see `calls`).

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use serde_json::{Value, json};

use super::calls::{self, Decider};
use super::usage::{Counted, Meter, SseUsage, Tally};
use super::{Protocol, Router, error};

impl Router {
    pub(super) async fn forward(
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
        let upstreams = self.upstreams_of(protocol);
        if upstreams.is_empty() {
            return error(
                protocol,
                StatusCode::BAD_GATEWAY,
                "api_error",
                "no upstream for this protocol",
            );
        }
        let Ok(mut request) = serde_json::from_slice::<Value>(&body) else {
            return error(
                protocol,
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "the body is not JSON",
            );
        };
        let checked = self.checks_answers(&headers).then(|| Decider {
            checker: self.checker.get().cloned(),
            grant: grant.clone(),
        });
        if checked.is_some() {
            calls::redact_results(&mut request, protocol);
        }
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

        let (class, models) = match self.candidates(&model) {
            Ok(candidates) => candidates,
            Err(message) => {
                return error(
                    protocol,
                    StatusCode::TOO_MANY_REQUESTS,
                    "budget_exceeded",
                    &message,
                );
            }
        };
        let answered = match self
            .send_first(
                protocol,
                path,
                &headers,
                &mut request,
                &upstreams,
                (&models, class.is_some()),
            )
            .await
        {
            Ok(answered) => answered,
            Err(failure) => return *failure,
        };
        let (upstream_response, started) = (answered.response, answered.started);
        let status = upstream_response.status();
        let content_type = upstream_response
            .headers()
            .get(header::CONTENT_TYPE)
            .cloned();
        let meter = Meter {
            store: self.store.clone(),
            run_id: grant.run_id,
            model: answered.model,
            class,
            started,
        };

        let mut response = if streaming && status.is_success() {
            let mut tally = Tally {
                scanner: SseUsage::default(),
                meter,
            };
            let upstream = upstream_response.bytes_stream();
            match checked {
                Some(decider) => {
                    let seen = move |bytes: &[u8]| tally.scanner.feed(bytes, protocol);
                    calls::checked_stream(upstream, protocol, decider, seen).into_response()
                }
                None => {
                    let stream = upstream.map(move |chunk| {
                        if let Ok(bytes) = &chunk {
                            tally.scanner.feed(bytes, protocol);
                        }
                        chunk
                    });
                    Body::from_stream(stream).into_response()
                }
            }
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
            let mut bytes = bytes;
            if status.is_success()
                && let Ok(mut value) = serde_json::from_slice::<Value>(&bytes)
            {
                let mut usage = Counted::default();
                usage.read(&value, protocol);
                if let Some(m) = value.get("model").and_then(Value::as_str) {
                    usage.model = Some(m.to_string());
                }
                meter.record(usage);
                if let Some(decider) = &checked {
                    calls::check_answer(&mut value, protocol, decider).await;
                    bytes = Bytes::from(value.to_string());
                }
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
#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::Router as Routes;
    use axum::routing::post;

    use super::*;
    use crate::router::{Grant, Upstream};
    use crate::store::Store;

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

    /// How long the fake upstream takes to answer a request that is not streamed.
    const UPSTREAM_DELAY: std::time::Duration = std::time::Duration::from_millis(30);
    const OPENAI_STREAM: &str = "data: {\"model\":\"qwen-local\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n\
data: {\"model\":\"qwen-local\",\"choices\":[],\"usage\":{\"prompt_tokens\":30,\"completion_tokens\":3}}\n\n\
data: [DONE]\n\n";

    /// The model a request names to have the fake upstream answer it with a tool call (OpenAI, not streamed).
    const TOOL_MODEL: &str = "tool-model";

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
                        let asks_for_a_tool = body["model"] == TOOL_MODEL;
                        seen.lock().unwrap().auth.push(auth);
                        seen.lock().unwrap().bodies.push(body);
                        if streaming {
                            ([(header::CONTENT_TYPE, "text/event-stream")], OPENAI_STREAM)
                                .into_response()
                        } else {
                            tokio::time::sleep(UPSTREAM_DELAY).await;
                            let message = if asks_for_a_tool {
                                json!({"role": "assistant", "content": null, "tool_calls": [
                                    {"id": "c1", "type": "function",
                                     "function": {"name": "bash", "arguments": "{}"}}]})
                            } else {
                                json!({"role": "assistant", "content": "Hi"})
                            };
                            axum::Json(json!({
                                "model": "qwen-local",
                                "choices": [{"message": message}],
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
        setup_with_day_budget(None).await
    }

    async fn setup_with_day_budget(day_budget: Option<i64>) -> Setup {
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
        let router = Arc::new(
            Router::new(
                store.clone(),
                vec![
                    Upstream {
                        protocol: Protocol::OpenAi,
                        base_url: upstream.clone(),
                        api_key: None,
                        oauth: None,
                    },
                    Upstream {
                        protocol: Protocol::Anthropic,
                        base_url: upstream,
                        api_key: Some("sk-provider-key".into()),
                        oauth: None,
                    },
                ],
            )
            .with_day_budget(day_budget),
        );
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
        // The call took at least as long as the upstream took to answer.
        assert!(usage[0].duration_ms >= UPSTREAM_DELAY.as_millis() as i64);
    }

    #[tokio::test]
    async fn a_call_answered_with_a_tool_call_stores_the_tools_name() {
        let s = setup().await;
        let token = s.router.issue_token(Grant {
            run_id: s.run,
            card_id: Some(s.card),
            budget: None,
        });
        let client = reqwest::Client::new();
        let send = |model: &str| {
            client
                .post(format!("{}/v1/chat/completions", s.url))
                .bearer_auth(&token)
                .json(&json!({"model": model, "messages": []}))
                .send()
        };
        send(TOOL_MODEL).await.unwrap();
        send("local").await.unwrap();
        let usage = s.store.usage_for_run(s.run).unwrap();
        let tools: Vec<_> = usage.iter().map(|u| u.tools.clone()).collect();
        assert_eq!(tools, [vec!["bash".to_string()], Vec::<String>::new()]);
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
    async fn the_attempts_of_a_card_spend_one_budget_together() {
        let s = setup().await;
        let sibling = s
            .store
            .add_run(Some(s.card), "Coder", "pi-bolt", "attempt")
            .unwrap();
        let token = |run: i64| {
            s.router.issue_token(Grant {
                run_id: run,
                card_id: Some(s.card),
                budget: Some(15),
            })
        };
        let (first, second) = (token(s.run), token(sibling.id));
        let client = reqwest::Client::new();
        let send = |token: String| {
            client
                .post(format!("{}/v1/chat/completions", s.url))
                .bearer_auth(token)
                .json(&json!({"model": "local", "messages": []}))
                .send()
        };
        // One attempt spends the 15 tokens of the card's budget; the other is refused though it used none.
        assert_eq!(send(first).await.unwrap().status(), 200);
        assert_eq!(send(second).await.unwrap().status(), 429);
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

    #[tokio::test]
    async fn requests_are_refused_once_the_days_tokens_are_spent() {
        let s = setup_with_day_budget(Some(20)).await;
        // A run without a card or budget of its own is still stopped by the day's budget.
        let token = s.router.issue_token(Grant {
            run_id: s.run,
            card_id: None,
            budget: None,
        });
        let send = || {
            reqwest::Client::new()
                .post(format!("{}/v1/chat/completions", s.url))
                .bearer_auth(&token)
                .json(&json!({"model": "local", "messages": []}))
                .send()
        };
        assert_eq!(send().await.unwrap().status(), 200);
        // 15 of the 20 tokens are used.
        assert_eq!(send().await.unwrap().status(), 200);
        let refused = send().await.unwrap();
        assert_eq!(refused.status(), 429);
        let body: Value = refused.json().await.unwrap();
        assert_eq!(body["error"]["type"], "budget_exceeded");
        assert_eq!(
            body["error"]["message"],
            "the daily token budget of 20 is spent (30 used today)"
        );
        assert_eq!(s.seen.lock().unwrap().bodies.len(), 2);
    }
}
