//! The router's check of the tool calls in the answers to a harness without a bridge (Claude Code): every tool call of
//! an answer to a checked token goes through `decide("tool_call")` (see `check`) before the answer reaches the harness,
//! and a refused call is replaced by a text that names the rule, so the harness never sees it. Answers are read as
//! OpenAI chat completions (`tool_calls` of a message) or Anthropic messages (`tool_use` blocks), whole or streamed.
//! The tool results in such a run's requests lose their secrets before they go upstream, as the bridge's
//! `/moebius/result` removes them for Pi.
//!
//! This module reads and rewrites whole answers and requests; `stream` rewrites a streamed answer as it passes.

mod stream;

use serde_json::{Value, json};

use super::{Grant, Protocol};
use crate::blobs::redact;
use crate::decide::check::Checker;

pub(super) use stream::checked_stream;

/// One tool call of an answer: its id, the tool's name and its input.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Call {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) input: Value,
}

impl Call {
    /// The call of an OpenAI `tool_calls` entry, whose arguments are JSON text; arguments that are not JSON are given
    /// as the text.
    fn of_openai(id: &str, name: &str, arguments: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            input: serde_json::from_str(arguments).unwrap_or_else(|_| json!(arguments)),
        }
    }

    /// The text this call is replaced by when it is refused for `reason`.
    fn refusal(&self, reason: &str) -> String {
        format!(
            "Moebius refused the tool call {} ({}): {reason}",
            self.name, self.id
        )
    }
}

/// Decides the calls of the answers to one checked token, failing closed.
#[derive(Clone)]
pub(super) struct Decider {
    pub(super) checker: Option<Checker>,
    pub(super) grant: Grant,
}

impl Decider {
    /// Why `call` is refused, or none when it is allowed. Without a checker every call is refused.
    async fn refused(&self, call: &Call) -> Option<String> {
        let Some(checker) = self.checker.clone() else {
            return Some("no tool-call check is set up".into());
        };
        let (grant, call) = (self.grant.clone(), call.clone());
        let decided = tokio::task::spawn_blocking(move || {
            checker.decide(&grant, &call.name, &call.input, &call.id)
        })
        .await;
        match decided {
            Ok(Ok(())) => None,
            Ok(Err(reason)) => Some(reason),
            Err(_) => Some("the call cannot be decided".into()),
        }
    }
}

/// Checks every tool call of a whole answer `value` and rewrites it: a refused OpenAI call leaves `tool_calls` and its
/// refusal is added to the message's text (`finish_reason` becomes `stop` when no call is left), a refused Anthropic
/// `tool_use` block becomes a text block (`stop_reason` becomes `end_turn` when no call is left).
pub(super) async fn check_answer(value: &mut Value, protocol: Protocol, decider: &Decider) {
    match protocol {
        Protocol::OpenAi => {
            for choice in value["choices"].as_array_mut().into_iter().flatten() {
                let Some(calls) = choice["message"]["tool_calls"].as_array().cloned() else {
                    continue;
                };
                let (mut kept, mut refusals) = (Vec::new(), Vec::new());
                for entry in calls {
                    let function = &entry["function"];
                    let call = Call::of_openai(
                        entry["id"].as_str().unwrap_or_default(),
                        function["name"].as_str().unwrap_or_default(),
                        function["arguments"].as_str().unwrap_or_default(),
                    );
                    match decider.refused(&call).await {
                        None => kept.push(entry),
                        Some(reason) => refusals.push(call.refusal(&reason)),
                    }
                }
                if refusals.is_empty() {
                    continue;
                }
                let message = &mut choice["message"];
                let mut text: Vec<String> = message["content"]
                    .as_str()
                    .filter(|text| !text.is_empty())
                    .map(str::to_string)
                    .into_iter()
                    .collect();
                text.extend(refusals);
                message["content"] = json!(text.join("\n\n"));
                if kept.is_empty() {
                    if let Some(message) = message.as_object_mut() {
                        message.remove("tool_calls");
                    }
                    if choice["finish_reason"] == "tool_calls" {
                        choice["finish_reason"] = json!("stop");
                    }
                } else {
                    message["tool_calls"] = json!(kept);
                }
            }
        }
        Protocol::Anthropic => {
            let Some(blocks) = value["content"].as_array_mut() else {
                return;
            };
            let mut refused = false;
            for block in blocks.iter_mut().filter(|b| b["type"] == "tool_use") {
                let call = Call {
                    id: block["id"].as_str().unwrap_or_default().to_string(),
                    name: block["name"].as_str().unwrap_or_default().to_string(),
                    input: block["input"].clone(),
                };
                if let Some(reason) = decider.refused(&call).await {
                    *block = json!({ "type": "text", "text": call.refusal(&reason) });
                    refused = true;
                }
            }
            let left = blocks.iter().any(|b| b["type"] == "tool_use");
            if refused && !left && value["stop_reason"] == "tool_use" {
                value["stop_reason"] = json!("end_turn");
            }
        }
    }
}

/// Removes the secrets from the tool results in a request's messages: the `content` of OpenAI messages with the role
/// `tool`, and of Anthropic `tool_result` blocks, as text or as text blocks.
pub(super) fn redact_results(request: &mut Value, protocol: Protocol) {
    fn redact_content(content: &mut Value) {
        match content {
            Value::String(text) => *text = redact(text),
            Value::Array(blocks) => {
                for block in blocks.iter_mut().filter(|b| b["type"] == "text") {
                    if let Some(text) = block["text"].as_str() {
                        block["text"] = json!(redact(text));
                    }
                }
            }
            _ => {}
        }
    }
    for message in request["messages"].as_array_mut().into_iter().flatten() {
        match protocol {
            Protocol::OpenAi if message["role"] == "tool" => {
                redact_content(&mut message["content"])
            }
            Protocol::OpenAi => {}
            Protocol::Anthropic => {
                let blocks = message["content"].as_array_mut().into_iter().flatten();
                for block in blocks.filter(|b| b["type"] == "tool_result") {
                    redact_content(&mut block["content"]);
                }
            }
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::PluginError;
    use moebius_plugin_api::{Call as PluginCall, DecideRequest, Host, Kind, Manifest, Plugin};

    use super::*;
    use crate::app::App;
    use crate::store::Store;

    /// Allows every tool but `bash`, which it refuses naming a rule.
    struct NoBash;
    impl Plugin for NoBash {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: "no-bash".into(),
                version: "0".into(),
                kinds: vec![Kind::Decider],
            }
        }
        fn call(&self, call: PluginCall) -> Result<Value, PluginError> {
            let r: DecideRequest = serde_json::from_value(call.params).unwrap();
            Ok(if r.context["tool"] == "bash" {
                json!({"choice": "refuse", "reason": "rule no-bash: no shell"})
            } else {
                json!({"choice": "allow", "reason": "rule reads: harmless"})
            })
        }
    }

    /// A core whose `tool_call` decider refuses `bash`, a run in it, and a decider for that run.
    pub(in crate::router) fn decider() -> (tempfile::TempDir, Arc<App>, Decider) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let mut host = Host::new(Duration::from_secs(5));
        host.add(NoBash);
        let app = App::new(store, Arc::new(host), &dir.path().join("data"));
        app.decisions.route("tool_call", "no-bash");
        let app = Arc::new(app);
        let run = app
            .store
            .add_run(None, "Coder", "claude-code", "code")
            .unwrap();
        let checker = Checker::new(app.clone(), None::<PathBuf>);
        let grant = Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        };
        let decider = Decider {
            checker: Some(checker),
            grant,
        };
        (dir, app, decider)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_refused_openai_call_becomes_text_and_an_allowed_one_stays() {
        let (_dir, app, decider) = decider();
        let mut answer = json!({"choices": [{"finish_reason": "tool_calls", "message": {
            "role": "assistant", "content": "Let me look.", "tool_calls": [
                {"id": "a", "type": "function", "function": {"name": "read", "arguments": "{\"path\":\"x\"}"}},
                {"id": "b", "type": "function", "function": {"name": "bash", "arguments": "{\"command\":\"rm -rf /\"}"}}]}}]});
        check_answer(&mut answer, Protocol::OpenAi, &decider).await;
        let message = &answer["choices"][0]["message"];
        assert_eq!(message["tool_calls"].as_array().unwrap().len(), 1);
        assert_eq!(message["tool_calls"][0]["id"], "a");
        assert_eq!(
            message["content"],
            "Let me look.\n\nMoebius refused the tool call bash (b): rule no-bash: no shell"
        );
        assert_eq!(answer["choices"][0]["finish_reason"], "tool_calls");

        // With every call refused, the answer is text and the turn ends.
        let mut answer = json!({"choices": [{"finish_reason": "tool_calls", "message": {
            "role": "assistant", "content": null, "tool_calls": [
                {"id": "c", "type": "function", "function": {"name": "bash", "arguments": "{}"}}]}}]});
        check_answer(&mut answer, Protocol::OpenAi, &decider).await;
        let choice = &answer["choices"][0];
        assert!(choice["message"].get("tool_calls").is_none());
        assert_eq!(choice["finish_reason"], "stop");
        assert!(
            choice["message"]["content"]
                .as_str()
                .unwrap()
                .contains("rule no-bash")
        );

        // Each call was decided once and left its check on the run, with its parsed input.
        let checks: Vec<Value> = app
            .store
            .events(decider.grant.run_id)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "tool_check")
            .map(|e| e.data)
            .collect();
        let decisions: Vec<(&str, &str)> = checks
            .iter()
            .map(|c| {
                (
                    c["tool_call_id"].as_str().unwrap(),
                    c["decision"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            decisions,
            [("a", "allow"), ("b", "refuse"), ("c", "refuse")]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_refused_anthropic_call_becomes_a_text_block() {
        let (_dir, _app, decider) = decider();
        let mut answer = json!({"stop_reason": "tool_use", "content": [
            {"type": "text", "text": "Running it."},
            {"type": "tool_use", "id": "t1", "name": "bash", "input": {"command": "ls"}}]});
        check_answer(&mut answer, Protocol::Anthropic, &decider).await;
        assert_eq!(answer["stop_reason"], "end_turn");
        assert_eq!(
            answer["content"][1],
            json!({"type": "text", "text": "Moebius refused the tool call bash (t1): rule no-bash: no shell"})
        );
        let mut allowed = json!({"stop_reason": "tool_use", "content": [
            {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "a"}}]});
        let before = allowed.clone();
        check_answer(&mut allowed, Protocol::Anthropic, &decider).await;
        assert_eq!(allowed, before);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn without_a_checker_every_call_is_refused() {
        let (_dir, _app, mut decider) = decider();
        decider.checker = None;
        let mut answer = json!({"stop_reason": "tool_use", "content": [
            {"type": "tool_use", "id": "t", "name": "Read", "input": {}}]});
        check_answer(&mut answer, Protocol::Anthropic, &decider).await;
        assert_eq!(answer["content"][0]["type"], "text");
    }

    #[test]
    fn the_secrets_of_tool_results_are_removed_from_a_request() {
        let secret = "sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789";
        let mut request = json!({"messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t", "content": format!("key {secret}")},
                {"type": "tool_result", "tool_use_id": "u", "content": [{"type": "text", "text": secret}]},
                {"type": "text", "text": "keep this"}]}]});
        redact_results(&mut request, Protocol::Anthropic);
        let text = request.to_string();
        assert!(!text.contains(secret), "{text}");
        assert!(text.contains("keep this"));
        let mut request = json!({"messages": [
            {"role": "tool", "tool_call_id": "a", "content": format!("key {secret}")},
            {"role": "user", "content": "mine"}]});
        redact_results(&mut request, Protocol::OpenAi);
        assert!(!request.to_string().contains(secret));
        assert_eq!(request["messages"][1]["content"], "mine");
    }
}
