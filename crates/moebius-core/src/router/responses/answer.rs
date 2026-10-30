//! The answer of the Responses endpoint turned into chat completions: its server-sent events become chat completion
//! chunks as they arrive (`stream`), or one chat completion when the harness did not ask for a stream (`complete`).
//! Text, tool calls, the finish reason and the usage are carried over; the model's reasoning is not, since chat
//! completions have no place for it. An error event, or an answer that ends before its last event, becomes an `error`
//! object in the stream, or an error for the body.

use std::collections::VecDeque;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use futures_util::{Stream, StreamExt};
use serde_json::{Value, json};

/// A tool call being put together.
struct Call {
    /// Where the Responses output lists it, which its argument events name.
    output_index: i64,
    id: String,
    name: String,
    arguments: String,
    /// Whether any of its arguments came in events (otherwise the finished item has them whole).
    streamed: bool,
}

/// An answer being read, event by event.
pub(in crate::router) struct Answer {
    id: String,
    model: String,
    created: u64,
    /// Bytes of events not complete yet.
    pending: Vec<u8>,
    started: bool,
    text: String,
    calls: Vec<Call>,
    finish: &'static str,
    usage: Option<Value>,
    /// What went wrong, if the answer is an error.
    error: Option<Value>,
    done: bool,
}

impl Answer {
    /// An answer to a request for `model`.
    pub(in crate::router) fn new(model: &str) -> Self {
        Self {
            id: "chatcmpl-moebius".into(),
            model: model.to_string(),
            created: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            pending: Vec::new(),
            started: false,
            text: String::new(),
            calls: Vec::new(),
            finish: "stop",
            usage: None,
            error: None,
            done: false,
        }
    }

    /// Reads `bytes` of the answer and gives the chunks (as server-sent events) that complete events make.
    pub(in crate::router) fn feed(&mut self, bytes: &[u8]) -> Vec<Bytes> {
        self.pending
            .extend(bytes.iter().copied().filter(|&b| b != b'\r'));
        let mut out = Vec::new();
        while let Some(end) = self.pending.windows(2).position(|w| w == b"\n\n") {
            let raw: Vec<u8> = self.pending.drain(..end + 2).collect();
            let data: Vec<String> = String::from_utf8_lossy(&raw)
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(|data| data.trim_start().to_string())
                .collect();
            if let Ok(event) = serde_json::from_str::<Value>(&data.join("\n")) {
                out.extend(self.event(&event));
            }
        }
        out
    }

    /// The end of the answer: what is still owed to the harness when the upstream stopped before its last event.
    pub(in crate::router) fn finish(&mut self) -> Vec<Bytes> {
        if self.done {
            return Vec::new();
        }
        self.fail(json!({
            "type": "api_error",
            "message": "the upstream's answer ended before it was complete",
        }))
    }

    /// The chat completion the events read so far make, or the error the answer is.
    pub(in crate::router) fn complete(&self) -> Result<Value, Value> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if !self.done {
            return Err(json!({
                "type": "api_error",
                "message": "the upstream's answer ended before it was complete",
            }));
        }
        let mut message = json!({
            "role": "assistant",
            "content": if self.text.is_empty() { Value::Null } else { json!(self.text) },
        });
        if !self.calls.is_empty() {
            message["tool_calls"] = self
                .calls
                .iter()
                .map(|call| {
                    json!({ "id": call.id, "type": "function",
                        "function": { "name": call.name, "arguments": call.arguments } })
                })
                .collect();
        }
        let mut body = json!({
            "id": self.id, "object": "chat.completion", "created": self.created, "model": self.model,
            "choices": [{ "index": 0, "message": message, "finish_reason": self.finish }],
        });
        if let Some(usage) = &self.usage {
            body["usage"] = usage.clone();
        }
        Ok(body)
    }

    fn chunk(&self, delta: Value, finish: Option<&str>) -> Value {
        json!({
            "id": self.id, "object": "chat.completion.chunk", "created": self.created, "model": self.model,
            "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }],
        })
    }

    /// The chunks for one event: the first starts the message.
    fn event(&mut self, event: &Value) -> Vec<Bytes> {
        if self.done {
            return Vec::new();
        }
        let mut chunks = Vec::new();
        let open = |answer: &mut Self, chunks: &mut Vec<Value>| {
            if !answer.started {
                answer.started = true;
                chunks.push(answer.chunk(json!({ "role": "assistant", "content": "" }), None));
            }
        };
        match event["type"].as_str().unwrap_or_default() {
            "response.created" => {
                if let Some(id) = event["response"]["id"].as_str() {
                    self.id = id.to_string();
                }
                if let Some(model) = event["response"]["model"].as_str() {
                    self.model = model.to_string();
                }
                open(self, &mut chunks);
            }
            "response.output_text.delta" | "response.refusal.delta" => {
                if let Some(text) = event["delta"].as_str().filter(|t| !t.is_empty()) {
                    open(self, &mut chunks);
                    self.text.push_str(text);
                    chunks.push(self.chunk(json!({ "content": text }), None));
                }
            }
            "response.output_item.added" if event["item"]["type"] == "function_call" => {
                open(self, &mut chunks);
                let item = &event["item"];
                let at = self.start_call(event["output_index"].as_i64().unwrap_or(-1), item);
                let call = &self.calls[at];
                let delta = json!({ "tool_calls": [{ "index": at, "id": call.id, "type": "function",
                    "function": { "name": call.name, "arguments": "" } }] });
                chunks.push(self.chunk(delta, None));
            }
            "response.function_call_arguments.delta" => {
                let wanted = event["output_index"].as_i64().unwrap_or(-1);
                if let Some(at) = self.calls.iter().position(|c| c.output_index == wanted)
                    && let Some(part) = event["delta"].as_str()
                {
                    self.calls[at].arguments.push_str(part);
                    self.calls[at].streamed = true;
                    let delta = json!({ "tool_calls": [{ "index": at, "function": { "arguments": part } }] });
                    chunks.push(self.chunk(delta, None));
                }
            }
            "response.output_item.done" => {
                let item = &event["item"];
                let wanted = event["output_index"].as_i64().unwrap_or(-1);
                match item["type"].as_str() {
                    Some("function_call") => {
                        open(self, &mut chunks);
                        let at = match self.calls.iter().position(|c| c.output_index == wanted) {
                            Some(at) => at,
                            None => {
                                let at = self.start_call(wanted, item);
                                let call = &self.calls[at];
                                let delta = json!({ "tool_calls": [{ "index": at, "id": call.id,
                                    "type": "function",
                                    "function": { "name": call.name, "arguments": "" } }] });
                                chunks.push(self.chunk(delta, None));
                                at
                            }
                        };
                        let whole = item["arguments"].as_str().unwrap_or_default();
                        if !self.calls[at].streamed && !whole.is_empty() {
                            self.calls[at].arguments = whole.to_string();
                            self.calls[at].streamed = true;
                            let delta = json!({ "tool_calls": [{ "index": at, "function": { "arguments": whole } }] });
                            chunks.push(self.chunk(delta, None));
                        }
                    }
                    // A message whose text did not come in events.
                    Some("message") if self.text.is_empty() => {
                        let text: String = item["content"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|part| {
                                part["text"].as_str().or_else(|| part["refusal"].as_str())
                            })
                            .collect();
                        if !text.is_empty() {
                            open(self, &mut chunks);
                            self.text = text.clone();
                            chunks.push(self.chunk(json!({ "content": text }), None));
                        }
                    }
                    _ => {}
                }
            }
            "response.completed" | "response.done" | "response.incomplete" => {
                return self.end(&event["response"], chunks);
            }
            "error" => {
                let error = &event["error"];
                let message = event["message"]
                    .as_str()
                    .or_else(|| error["message"].as_str());
                let code = event["code"].as_str().or_else(|| error["code"].as_str());
                return self.fail(failure(message, code));
            }
            "response.failed" => {
                let error = &event["response"]["error"];
                return self.fail(failure(error["message"].as_str(), error["code"].as_str()));
            }
            _ => {}
        }
        chunks.iter().map(sse).collect()
    }

    fn start_call(&mut self, output_index: i64, item: &Value) -> usize {
        self.calls.push(Call {
            output_index,
            id: item["call_id"].as_str().unwrap_or_default().to_string(),
            name: item["name"].as_str().unwrap_or_default().to_string(),
            arguments: String::new(),
            streamed: false,
        });
        self.calls.len() - 1
    }

    /// The last event: the finish reason, the usage, and the end of the stream.
    fn end(&mut self, response: &Value, mut chunks: Vec<Value>) -> Vec<Bytes> {
        self.done = true;
        self.finish = if !self.calls.is_empty() {
            "tool_calls"
        } else if response["status"] == "incomplete" {
            match response["incomplete_details"]["reason"].as_str() {
                Some("content_filter") => "content_filter",
                _ => "length",
            }
        } else {
            "stop"
        };
        if !self.started {
            self.started = true;
            chunks.push(self.chunk(json!({ "role": "assistant", "content": "" }), None));
        }
        chunks.push(self.chunk(json!({}), Some(self.finish)));
        if let Some(usage) = response["usage"].as_object() {
            let number = |value: Option<&Value>| value.and_then(Value::as_i64).unwrap_or(0);
            let (input, output) = (
                number(usage.get("input_tokens")),
                number(usage.get("output_tokens")),
            );
            let usage = json!({
                "prompt_tokens": input,
                "completion_tokens": output,
                "total_tokens": usage.get("total_tokens").and_then(Value::as_i64).unwrap_or(input + output),
                "prompt_tokens_details": {
                    "cached_tokens": number(usage.get("input_tokens_details").and_then(|d| d.get("cached_tokens"))),
                },
            });
            let mut last = self.chunk(json!({}), None);
            last["choices"] = json!([]);
            last["usage"] = usage.clone();
            chunks.push(last);
            self.usage = Some(usage);
        }
        let mut out: Vec<Bytes> = chunks.iter().map(sse).collect();
        out.push(Bytes::from_static(b"data: [DONE]\n\n"));
        out
    }

    /// The answer is an error: the stream says so and ends.
    fn fail(&mut self, error: Value) -> Vec<Bytes> {
        self.done = true;
        self.error = Some(error.clone());
        vec![
            Bytes::from(format!("data: {}\n\n", json!({ "error": error }))),
            Bytes::from_static(b"data: [DONE]\n\n"),
        ]
    }
}

/// The `error` object for an error event.
fn failure(message: Option<&str>, code: Option<&str>) -> Value {
    json!({
        "type": "api_error",
        "message": message.or(code).unwrap_or("the upstream failed"),
        "code": code,
    })
}

fn sse(chunk: &Value) -> Bytes {
    Bytes::from(format!("data: {chunk}\n\n"))
}

/// The chat completion chunks for the Responses events of `upstream`, as a body to stream on.
pub(in crate::router) fn stream<S, E>(
    upstream: S,
    model: &str,
) -> impl Stream<Item = io::Result<Bytes>> + Send + 'static
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    let state = (
        Box::pin(upstream),
        Answer::new(model),
        VecDeque::<Bytes>::new(),
        false,
    );
    futures_util::stream::unfold(
        state,
        |(mut upstream, mut answer, mut queue, mut ended)| async move {
            loop {
                if let Some(bytes) = queue.pop_front() {
                    return Some((Ok(bytes), (upstream, answer, queue, ended)));
                }
                if ended {
                    return None;
                }
                match upstream.next().await {
                    Some(Ok(bytes)) => queue.extend(answer.feed(&bytes)),
                    Some(Err(e)) => {
                        ended = true;
                        return Some((Err(io::Error::other(e)), (upstream, answer, queue, ended)));
                    }
                    None => {
                        ended = true;
                        queue.extend(answer.finish());
                    }
                }
            }
        },
    )
}
