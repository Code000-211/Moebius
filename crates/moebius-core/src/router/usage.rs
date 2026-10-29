//! What an answer used: its tokens, read from a body or from a stream as it passes through, and the tools it asks for,
//! recorded against the run.

use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use super::Protocol;
use crate::store::{Store, Usage};

/// Tokens counted from one response.
#[derive(Debug, Default, Clone)]
pub(super) struct Counted {
    input: i64,
    output: i64,
    cached: i64,
    pub(super) model: Option<String>,
    /// The tools the answer asks for, in order.
    tools: Vec<String>,
    seen: bool,
}

/// The names of the tools a response body or a stream event asks for, in order:
/// OpenAI's `tool_calls[].function.name` of a body's message or a stream's delta, and Anthropic's `tool_use` blocks of
/// a body's content or the `content_block_start` event of a stream.
fn tool_names(value: &Value, protocol: Protocol) -> Vec<String> {
    let mut names = Vec::new();
    match protocol {
        Protocol::OpenAi => {
            for choice in value["choices"].as_array().into_iter().flatten() {
                for call in choice["message"]["tool_calls"]
                    .as_array()
                    .into_iter()
                    .chain(choice["delta"]["tool_calls"].as_array())
                    .flatten()
                {
                    names.extend(call["function"]["name"].as_str().map(str::to_string));
                }
            }
        }
        Protocol::Anthropic => {
            let blocks = value["content"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(value.get("content_block"));
            for block in blocks.filter(|b| b["type"] == "tool_use") {
                names.extend(block["name"].as_str().map(str::to_string));
            }
        }
    }
    names
}

impl Counted {
    /// Reads the usage of a response body or a stream event; later numbers replace earlier ones of the same kind.
    pub(super) fn read(&mut self, value: &Value, protocol: Protocol) {
        self.tools.extend(tool_names(value, protocol));
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
pub(super) struct SseUsage {
    pending: Vec<u8>,
    usage: Counted,
}

impl SseUsage {
    pub(super) fn feed(&mut self, bytes: &[u8], protocol: Protocol) {
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
pub(super) struct Tally {
    pub(super) scanner: SseUsage,
    pub(super) meter: Meter,
}

impl Drop for Tally {
    fn drop(&mut self) {
        self.meter.record(std::mem::take(&mut self.scanner.usage));
    }
}

/// Records counted usage against a run.
pub(super) struct Meter {
    pub(super) store: Arc<Store>,
    pub(super) run_id: i64,
    pub(super) model: String,
    /// When the request went upstream, which is where the model's time is counted from.
    pub(super) started: Instant,
}

impl Meter {
    pub(super) fn record(&self, counted: Counted) {
        if !counted.seen {
            return;
        }
        let _ = self.store.add_usage(&Usage {
            run_id: Some(self.run_id),
            model: counted.model.unwrap_or_else(|| self.model.clone()),
            input_tokens: counted.input,
            output_tokens: counted.output,
            cached_tokens: counted.cached,
            duration_ms: i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX),
            tools: counted.tools,
        });
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_tools_an_answer_asks_for_are_read_for_both_protocols_streamed_and_not() {
        let names = |value: Value, protocol| tool_names(&value, protocol);
        let strings =
            |names: &[&str]| -> Vec<String> { names.iter().map(|n| n.to_string()).collect() };

        // OpenAI, not streamed: two tool calls in one message.
        let body = json!({"choices": [{"message": {"content": null, "tool_calls": [
            {"id": "a", "type": "function", "function": {"name": "read", "arguments": "{}"}},
            {"id": "b", "type": "function", "function": {"name": "bash", "arguments": "{}"}}]}}]});
        assert_eq!(names(body, Protocol::OpenAi), strings(&["read", "bash"]));
        // OpenAI, streamed: the name comes with a call's first delta, its arguments after.
        let first = json!({"choices": [{"delta": {"tool_calls": [
            {"index": 0, "id": "a", "function": {"name": "edit", "arguments": ""}}]}}]});
        let rest = json!({"choices": [{"delta": {"tool_calls": [
            {"index": 0, "function": {"arguments": "{\"path\":"}}]}}]});
        assert_eq!(names(first, Protocol::OpenAi), strings(&["edit"]));
        assert!(names(rest, Protocol::OpenAi).is_empty());
        // Anthropic, not streamed: text and a tool_use block.
        let body = json!({"content": [{"type": "text", "text": "Looking."},
            {"type": "tool_use", "id": "t", "name": "grep", "input": {}}]});
        assert_eq!(names(body, Protocol::Anthropic), strings(&["grep"]));
        // Anthropic, streamed: the block start carries the name; its deltas do not.
        let start = json!({"type": "content_block_start", "index": 1,
            "content_block": {"type": "tool_use", "id": "t", "name": "write", "input": {}}});
        let text = json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "text", "text": ""}});
        let delta = json!({"type": "content_block_delta", "index": 1,
            "delta": {"type": "input_json_delta", "partial_json": "{}"}});
        assert_eq!(names(start, Protocol::Anthropic), strings(&["write"]));
        assert!(names(text, Protocol::Anthropic).is_empty());
        assert!(names(delta, Protocol::Anthropic).is_empty());
        // An answer with no tool call names none.
        let answer = json!({"choices": [{"message": {"content": "Hi"}}]});
        assert!(names(answer, Protocol::OpenAi).is_empty());
        let answer = json!({"content": [{"type": "text", "text": "Hi"}]});
        assert!(names(answer, Protocol::Anthropic).is_empty());
    }

    #[test]
    fn a_stream_collects_the_tools_of_all_its_events() {
        let mut scanner = SseUsage::default();
        let stream = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"name\":\"read\"}}\n\n\
data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"name\":\"bash\"}}\n\n";
        // Fed in pieces that split an event.
        let (a, b) = stream.split_at(40);
        scanner.feed(a.as_bytes(), Protocol::Anthropic);
        scanner.feed(b.as_bytes(), Protocol::Anthropic);
        assert_eq!(scanner.usage.tools, ["read", "bash"]);
    }
}
