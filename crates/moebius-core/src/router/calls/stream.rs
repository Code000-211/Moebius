//! A streamed answer to a checked token, rewritten as it passes: its server-sent events go on as they come, except a
//! tool call's, which are held back until the call is complete and decided. An allowed call then goes on as it came
//! (Anthropic) or whole in one event (OpenAI); a refused one is replaced by text that names the rule, and an answer left
//! without calls ends its turn (`end_turn`, `stop`). A call the stream did not finish is never passed on.

use std::collections::BTreeMap;

use axum::body::{Body, Bytes};
use futures_util::{Stream, StreamExt};
use serde_json::{Value, json};

use super::{Call, Decider};
use crate::router::Protocol;

/// One server-sent event: its bytes as they came, with the blank line that ends it, and its `data` as JSON (`[DONE]`
/// and data that is not JSON are none).
struct Event {
    raw: Vec<u8>,
    data: Option<Value>,
    done: bool,
}

/// Splits bytes into events as they come.
#[derive(Default)]
struct Events {
    pending: Vec<u8>,
    /// How far `pending` has been searched for the end of an event.
    scanned: usize,
}

impl Events {
    /// The events `bytes` complete.
    fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        let mut line_start = self.scanned;
        while let Some(at) = self.pending[self.scanned..]
            .iter()
            .position(|&b| b == b'\n')
        {
            let end = self.scanned + at;
            let line = &self.pending[line_start..end];
            self.scanned = end + 1;
            if line.iter().all(|&b| b == b'\r') {
                let raw: Vec<u8> = self.pending.drain(..self.scanned).collect();
                self.scanned = 0;
                events.push(parse(raw));
            }
            line_start = self.scanned;
        }
        // The line being read starts again at its beginning next time.
        self.scanned = line_start;
        events
    }

    /// What the stream left that is no complete event.
    fn rest(self) -> Vec<u8> {
        self.pending
    }
}

fn parse(raw: Vec<u8>) -> Event {
    let text = String::from_utf8_lossy(&raw);
    let data: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .collect();
    let data = data.join("\n");
    Event {
        done: data == "[DONE]",
        data: serde_json::from_str(&data).ok(),
        raw,
    }
}

/// An Anthropic event as the stream writes it.
fn anthropic_event(data: Value) -> Vec<u8> {
    let kind = data["type"].as_str().unwrap_or_default().to_string();
    format!("event: {kind}\ndata: {data}\n\n").into_bytes()
}

/// An OpenAI event as the stream writes it.
fn openai_event(data: &Value) -> Vec<u8> {
    format!("data: {data}\n\n").into_bytes()
}

/// A `tool_use` block of an Anthropic stream, held back from its start to its stop.
struct HeldBlock {
    index: Value,
    id: String,
    name: String,
    /// The input as the start event gives it, which the deltas replace when they hold any.
    input: Value,
    json: String,
    /// The block's events, and every other event that came meanwhile, in order: whether it is the block's, and it.
    events: Vec<(bool, Event)>,
}

/// An Anthropic stream being checked.
#[derive(Default)]
struct AnthropicStream {
    held: Option<HeldBlock>,
    allowed: usize,
    refused: usize,
}

impl AnthropicStream {
    async fn event(&mut self, event: Event, decider: &Decider) -> Vec<Vec<u8>> {
        let data = event.data.clone().unwrap_or(Value::Null);
        let kind = data["type"].as_str().unwrap_or_default();
        if let Some(held) = &mut self.held {
            let own = data["index"] == held.index;
            if own && kind == "content_block_delta" && data["delta"]["type"] == "input_json_delta" {
                held.json
                    .push_str(data["delta"]["partial_json"].as_str().unwrap_or_default());
            }
            let stops = own && kind == "content_block_stop";
            held.events.push((own, event));
            if !stops {
                return vec![];
            }
            let held = self.held.take().expect("a block is held");
            return self.decide(held, decider).await;
        }
        if kind == "content_block_start" && data["content_block"]["type"] == "tool_use" {
            let block = &data["content_block"];
            self.held = Some(HeldBlock {
                index: data["index"].clone(),
                id: block["id"].as_str().unwrap_or_default().to_string(),
                name: block["name"].as_str().unwrap_or_default().to_string(),
                input: block["input"].clone(),
                json: String::new(),
                events: vec![(true, event)],
            });
            return vec![];
        }
        if kind == "message_delta"
            && data["delta"]["stop_reason"] == "tool_use"
            && self.refused > 0
            && self.allowed == 0
        {
            let mut data = data;
            data["delta"]["stop_reason"] = json!("end_turn");
            return vec![anthropic_event(data)];
        }
        vec![event.raw]
    }

    /// The events of a complete block: as they came when its call is allowed, else a text block with the refusal.
    async fn decide(&mut self, held: HeldBlock, decider: &Decider) -> Vec<Vec<u8>> {
        let input = if held.json.trim().is_empty() {
            held.input.clone()
        } else {
            serde_json::from_str(&held.json).unwrap_or_else(|_| json!(held.json))
        };
        let call = Call {
            id: held.id.clone(),
            name: held.name.clone(),
            input,
        };
        let Some(reason) = decider.refused(&call).await else {
            self.allowed += 1;
            return held
                .events
                .into_iter()
                .map(|(_, event)| event.raw)
                .collect();
        };
        self.refused += 1;
        let index = held.index;
        let mut out = vec![
            anthropic_event(json!({ "type": "content_block_start", "index": index,
                "content_block": { "type": "text", "text": "" } })),
            anthropic_event(json!({ "type": "content_block_delta", "index": index,
                "delta": { "type": "text_delta", "text": call.refusal(&reason) } })),
            anthropic_event(json!({ "type": "content_block_stop", "index": index })),
        ];
        out.extend(
            held.events
                .into_iter()
                .filter(|(own, _)| !own)
                .map(|(_, event)| event.raw),
        );
        out
    }

    /// The end of the stream: a block still held was never finished and is dropped with the events of its own.
    fn end(self) -> Vec<Vec<u8>> {
        let held = self.held.into_iter().flat_map(|held| held.events);
        held.filter(|(own, _)| !own)
            .map(|(_, event)| event.raw)
            .collect()
    }
}

/// An OpenAI tool call being put together from its deltas.
#[derive(Default)]
struct PendingCall {
    id: String,
    name: String,
    arguments: String,
}

/// An OpenAI stream being checked.
#[derive(Default)]
struct OpenAiStream {
    /// The calls being put together, by choice and by the call's index in it.
    calls: BTreeMap<(i64, i64), PendingCall>,
    /// The fields of the stream's chunks (`id`, `model`, ..) for the chunks that carry the decided calls.
    template: Value,
}

impl OpenAiStream {
    async fn event(&mut self, event: Event, decider: &Decider) -> Vec<Vec<u8>> {
        if event.done {
            let mut out = self.flush(None, decider).await.0;
            out.push(event.raw);
            return out;
        }
        let Some(mut chunk) = event.data.clone() else {
            return vec![event.raw];
        };
        let mut changed = false;
        for choice in chunk["choices"].as_array_mut().into_iter().flatten() {
            let Some(deltas) = choice["delta"]
                .as_object_mut()
                .and_then(|delta| delta.remove("tool_calls"))
            else {
                continue;
            };
            changed = true;
            let at = choice["index"].as_i64().unwrap_or(0);
            for delta in deltas.as_array().into_iter().flatten() {
                let call = self
                    .calls
                    .entry((at, delta["index"].as_i64().unwrap_or(0)))
                    .or_default();
                if let Some(id) = delta["id"].as_str() {
                    call.id = id.to_string();
                }
                let function = &delta["function"];
                call.name
                    .push_str(function["name"].as_str().unwrap_or_default());
                call.arguments
                    .push_str(function["arguments"].as_str().unwrap_or_default());
            }
        }
        if changed {
            let mut template = chunk.clone();
            if let Some(fields) = template.as_object_mut() {
                fields.remove("choices");
                fields.remove("usage");
            }
            self.template = template;
        }
        let mut out = Vec::new();
        let finishing: Vec<i64> = chunk["choices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|choice| !choice["finish_reason"].is_null())
            .filter_map(|choice| choice["index"].as_i64())
            .filter(|at| self.calls.keys().any(|(choice, _)| choice == at))
            .collect();
        for at in finishing {
            let (decided, allowed) = self.flush(Some(at), decider).await;
            out.extend(decided);
            changed = true;
            let choices = chunk["choices"].as_array_mut().into_iter().flatten();
            for choice in choices.filter(|choice| choice["index"] == at) {
                if allowed == 0 && choice["finish_reason"] == "tool_calls" {
                    choice["finish_reason"] = json!("stop");
                }
            }
        }
        if !changed {
            out.push(event.raw);
        } else if !empty(&chunk) {
            out.push(openai_event(&chunk));
        }
        out
    }

    /// Decides the calls put together for the choice `at`, or for every choice, and gives the chunks that carry them:
    /// an allowed call whole, a refused one as text. Also says how many were allowed.
    async fn flush(&mut self, at: Option<i64>, decider: &Decider) -> (Vec<Vec<u8>>, usize) {
        let keys: Vec<(i64, i64)> = self
            .calls
            .keys()
            .filter(|(choice, _)| at.is_none_or(|at| at == *choice))
            .copied()
            .collect();
        let (mut out, mut allowed) = (Vec::new(), BTreeMap::<i64, i64>::new());
        let mut count = 0;
        for key in keys {
            let pending = self.calls.remove(&key).expect("the call is pending");
            let call = Call::of_openai(&pending.id, &pending.name, &pending.arguments);
            let delta = match decider.refused(&call).await {
                None => {
                    let index = allowed.entry(key.0).or_default();
                    let delta = json!({ "tool_calls": [{ "index": *index, "id": pending.id,
                        "type": "function",
                        "function": { "name": pending.name, "arguments": pending.arguments } }] });
                    *index += 1;
                    count += 1;
                    delta
                }
                Some(reason) => json!({ "content": call.refusal(&reason) }),
            };
            let mut chunk = self.template.clone();
            chunk["choices"] = json!([{ "index": key.0, "delta": delta, "finish_reason": null }]);
            out.push(openai_event(&chunk));
        }
        (out, count)
    }

    /// The end of the stream: the calls it left are decided and passed on.
    async fn end(mut self, decider: &Decider) -> Vec<Vec<u8>> {
        self.flush(None, decider).await.0
    }
}

/// Whether an OpenAI chunk says nothing once its tool calls are taken out: no usage, and no choice with content,
/// another field of its delta, or a finish.
fn empty(chunk: &Value) -> bool {
    let says = |choice: &Value| {
        !choice["finish_reason"].is_null()
            || choice["delta"].as_object().is_some_and(|delta| {
                delta
                    .iter()
                    .any(|(key, value)| key != "role" && !value.is_null())
            })
    };
    chunk["usage"].is_null()
        && !chunk["choices"]
            .as_array()
            .is_some_and(|choices| choices.iter().any(says))
}

/// The answer `upstream` as a body, checked as it passes; `seen` is given every byte that came from upstream, so that
/// its usage is counted, and dropped when the stream ends.
pub(in crate::router) fn checked_stream<S, E, F>(
    upstream: S,
    protocol: Protocol,
    decider: Decider,
    mut seen: F,
) -> Body
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
    F: FnMut(&[u8]) + Send + 'static,
{
    let (sender, receiver) = tokio::sync::mpsc::channel::<std::io::Result<Bytes>>(16);
    tokio::spawn(async move {
        let mut upstream = Box::pin(upstream);
        let mut events = Events::default();
        let mut anthropic = AnthropicStream::default();
        let mut openai = OpenAiStream::default();
        let send = |bytes: Vec<u8>| sender.send(Ok(Bytes::from(bytes)));
        while let Some(chunk) = upstream.next().await {
            let bytes = match chunk {
                Ok(bytes) => bytes,
                Err(e) => {
                    let _ = sender.send(Err(std::io::Error::other(e))).await;
                    return;
                }
            };
            seen(&bytes);
            for event in events.feed(&bytes) {
                let out = match protocol {
                    Protocol::Anthropic => anthropic.event(event, &decider).await,
                    Protocol::OpenAi => openai.event(event, &decider).await,
                };
                for bytes in out {
                    if send(bytes).await.is_err() {
                        // The harness went away.
                        return;
                    }
                }
            }
        }
        let mut out = match protocol {
            Protocol::Anthropic => anthropic.end(),
            Protocol::OpenAi => openai.end(&decider).await,
        };
        out.push(events.rest());
        for bytes in out.into_iter().filter(|bytes| !bytes.is_empty()) {
            if send(bytes).await.is_err() {
                return;
            }
        }
    });
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|item| (item, receiver))
    });
    Body::from_stream(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::calls::tests::decider;

    /// The body `stream` becomes, fed in pieces of `piece` bytes, checked by a decider that refuses `bash`.
    async fn checked(stream: &str, protocol: Protocol, piece: usize) -> String {
        let (_dir, _app, decider) = decider();
        let pieces: Vec<reqwest::Result<Bytes>> = stream
            .as_bytes()
            .chunks(piece)
            .map(|piece| Ok(Bytes::copy_from_slice(piece)))
            .collect();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let saw = seen.clone();
        let body = checked_stream(
            futures_util::stream::iter(pieces),
            protocol,
            decider,
            move |bytes| saw.lock().unwrap().extend_from_slice(bytes),
        );
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        assert_eq!(seen.lock().unwrap().as_slice(), stream.as_bytes());
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn data(body: &str) -> Vec<Value> {
        body.lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter_map(|data| serde_json::from_str(data).ok())
            .collect()
    }

    const ANTHROPIC: &str = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":3}}}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Checking.\"}}\n\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"TOOL\",\"input\":{}}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"command\\\":\"}}\n\n\
event: ping\n\
data: {\"type\":\"ping\"}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"ls\\\"}\"}}\n\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":1}\n\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":9}}\n\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\n";

    #[tokio::test(flavor = "multi_thread")]
    async fn an_anthropic_stream_passes_an_allowed_call_as_it_came() {
        let stream = ANTHROPIC.replace("TOOL", "Read");
        for piece in [7, 64, 4096] {
            assert_eq!(checked(&stream, Protocol::Anthropic, piece).await, stream);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_anthropic_stream_replaces_a_refused_call_by_text_and_ends_its_turn() {
        let stream = ANTHROPIC.replace("TOOL", "bash");
        let body = checked(&stream, Protocol::Anthropic, 11).await;
        let events = data(&body);
        assert!(!body.contains("tool_use\",\"id"), "{body}");
        assert!(!body.contains("input_json_delta"), "{body}");
        let refusal = events
            .iter()
            .find(|e| e["index"] == 1 && e["delta"]["type"] == "text_delta")
            .unwrap();
        assert_eq!(
            refusal["delta"]["text"],
            "Moebius refused the tool call bash (t1): rule no-bash: no shell"
        );
        let kinds: Vec<&str> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            [
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "ping",
                "message_delta",
                "message_stop"
            ]
        );
        let end = events
            .iter()
            .find(|e| e["type"] == "message_delta")
            .unwrap();
        assert_eq!(end["delta"]["stop_reason"], "end_turn");
        assert_eq!(end["usage"]["output_tokens"], 9);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_anthropic_call_the_stream_did_not_finish_is_not_passed_on() {
        let stream = ANTHROPIC.replace("TOOL", "Read");
        let cut = &stream[..stream
            .find("event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}")
            .unwrap()];
        let body = checked(cut, Protocol::Anthropic, 64).await;
        assert!(!body.contains("tool_use"), "{body}");
        assert!(body.contains("ping"));
    }

    const OPENAI: &str = "data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Looking.\"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"type\":\"function\",\"function\":{\"name\":\"read\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\\\"x\\\"}\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"b\",\"type\":\"function\",\"function\":{\"name\":\"SECOND\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\n\
data: [DONE]\n\n";

    /// The tool calls the chunks of `body` carry, put together, and the finish reason.
    fn openai_calls(body: &str) -> (Vec<(String, String, String)>, String, String) {
        let mut calls: BTreeMap<i64, (String, String, String)> = BTreeMap::new();
        let (mut text, mut finish) = (String::new(), String::new());
        for chunk in data(body) {
            for choice in chunk["choices"].as_array().unwrap() {
                text.push_str(choice["delta"]["content"].as_str().unwrap_or_default());
                if let Some(reason) = choice["finish_reason"].as_str() {
                    finish = reason.to_string();
                }
                for call in choice["delta"]["tool_calls"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let entry = calls.entry(call["index"].as_i64().unwrap()).or_default();
                    if let Some(id) = call["id"].as_str() {
                        entry.0 = id.into();
                    }
                    entry
                        .1
                        .push_str(call["function"]["name"].as_str().unwrap_or_default());
                    entry
                        .2
                        .push_str(call["function"]["arguments"].as_str().unwrap_or_default());
                }
            }
        }
        (calls.into_values().collect(), text, finish)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_openai_stream_passes_allowed_calls_whole_and_replaces_a_refused_one() {
        let stream = OPENAI.replace("SECOND", "grep");
        let body = checked(&stream, Protocol::OpenAi, 13).await;
        let (calls, text, finish) = openai_calls(&body);
        assert_eq!(
            calls,
            [
                ("a".into(), "read".into(), "{\"path\":\"x\"}".into()),
                ("b".into(), "grep".into(), "{}".into())
            ]
        );
        assert_eq!((text.as_str(), finish.as_str()), ("Looking.", "tool_calls"));
        assert!(body.ends_with("data: [DONE]\n\n"));
        assert!(body.contains("\"completion_tokens\":2"));

        let stream = OPENAI.replace("SECOND", "bash");
        let body = checked(&stream, Protocol::OpenAi, 4096).await;
        let (calls, text, finish) = openai_calls(&body);
        assert_eq!(
            calls,
            [("a".into(), "read".into(), "{\"path\":\"x\"}".into())]
        );
        assert_eq!(
            text,
            "Looking.Moebius refused the tool call bash (b): rule no-bash: no shell"
        );
        assert_eq!(finish, "tool_calls");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_openai_stream_whose_calls_are_all_refused_stops() {
        let stream = OPENAI
            .replace("\"read\"", "\"bash\"")
            .replace("SECOND", "bash");
        let body = checked(&stream, Protocol::OpenAi, 64).await;
        let (calls, text, finish) = openai_calls(&body);
        assert!(calls.is_empty(), "{body}");
        assert!(text.contains("(a): rule no-bash") && text.contains("(b): rule no-bash"));
        assert_eq!(finish, "stop");
    }
}
