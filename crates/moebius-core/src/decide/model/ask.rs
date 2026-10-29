//! Asking the decision model: the request to its OpenAI-compatible endpoint with every field of the context cut,
//! reading its answer, which must be exactly the JSON asked for, and the model as the decider plugin [`NAME`].

use std::collections::BTreeMap;

use moebius_plugin_api::{Call, DecideRequest, Decision, Kind, Manifest, Plugin, PluginError};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{NAME, localize};
use crate::config::DecisionModelConfig;

/// The most characters of one field of a context the model is shown. What the model sees is cut from the full
/// context, so that the cut can change without touching the judgements.
const FIELD_LIMIT: usize = 4000;

const SYSTEM_PROMPT: &str = "You are the decision model of Moebius, an orchestration layer for coding agents. \
You are given one decision: its kind, the options and its context. Choose one of the options. Answer with JSON only, \
no other text: {\"choice\": \"<one of the options>\", \"reason\": \"<one sentence>\"}.";

/// What the model chose and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub choice: String,
    pub reason: String,
}

/// The model's endpoint and what it is asked to do. It is a decider plugin, so the host gives its calls a time limit.
#[derive(Clone)]
pub struct DecisionModel {
    pub config: DecisionModelConfig,
    /// The key, read from the environment at start. Never stored or logged.
    key: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAnswer {
    choice: String,
    #[serde(default)]
    reason: String,
}

impl DecisionModel {
    pub fn new(config: &DecisionModelConfig, key: Option<String>) -> Self {
        Self {
            config: config.clone(),
            key,
        }
    }

    /// Asks the model to decide. A timeout, a failed request, an answer that is not exactly the JSON asked for and a
    /// choice that is not among the options are all failures.
    ///
    /// The blocking client is meant for this: callers are the shadow worker and the plugin host's threads, which
    /// wait for the answer anyway. It is built per call because it must be created and dropped outside of an async
    /// context.
    pub fn ask(
        &self,
        kind: &str,
        context: &BTreeMap<String, String>,
        options: &[String],
    ) -> std::result::Result<Answer, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(self.config.timeout)
            .build()
            .map_err(|e| e.to_string())?;
        let mut request = client
            .post(format!("{}/chat/completions", self.config.url))
            .json(&self.request_body(kind, context, options));
        if let Some(key) = &self.key {
            request = request.bearer_auth(key);
        }
        let response = request.send().map_err(|e| {
            if e.is_timeout() {
                format!("no answer within {} s", self.config.timeout.as_secs())
            } else {
                format!("the request failed: {}", e.without_url())
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("the decision model answered HTTP {status}"));
        }
        let body: Value = response
            .json()
            .map_err(|e| format!("the reply is not JSON: {e}"))?;
        let text = body
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or("the reply has no text")?;
        parse_answer(kind, text, options)
    }

    fn request_body(
        &self,
        kind: &str,
        context: &BTreeMap<String, String>,
        options: &[String],
    ) -> Value {
        json!({
            "model": self.config.model,
            "stream": false,
            "temperature": 0,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": user_message(kind, context, options) },
            ],
        })
    }
}

impl Plugin for DecisionModel {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: NAME.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kinds: vec![Kind::Decider],
        }
    }

    fn call(&self, call: Call) -> std::result::Result<Value, PluginError> {
        let request: DecideRequest = serde_json::from_value(call.params)
            .map_err(|e| PluginError::BadParams(e.to_string()))?;
        let answer = self
            .ask(&request.kind, &request.context, &request.options)
            .map_err(PluginError::Failed)?;
        serde_json::to_value(Decision {
            choice: answer.choice,
            reason: answer.reason,
            by: None,
        })
        .map_err(|e| PluginError::Failed(e.to_string()))
    }
}

/// `text` cut to [`FIELD_LIMIT`] characters, with a mark where it was cut.
fn cut(text: &str) -> String {
    match text.char_indices().nth(FIELD_LIMIT) {
        Some((end, _)) => format!("{} [cut]", &text[..end]),
        None => text.to_string(),
    }
}

/// What the model is asked: the kind, the options and the context, each field cut.
fn user_message(kind: &str, context: &BTreeMap<String, String>, options: &[String]) -> String {
    let context: BTreeMap<&str, String> = context.iter().map(|(k, v)| (&**k, cut(v))).collect();
    json!({ "kind": kind, "options": options, "context": context }).to_string()
}

/// Reads the model's reply: exactly the JSON asked for, bare. A reply in a code fence is a failure, since the model
/// is trained to answer without one.
fn parse_answer(kind: &str, text: &str, options: &[String]) -> std::result::Result<Answer, String> {
    let not_json = |e: serde_json::Error| format!("the answer is not the JSON asked for ({e})");
    let value: Value = serde_json::from_str(text.trim()).map_err(not_json)?;
    // A derived struct would also be read from a JSON array.
    if !value.is_object() {
        return Err("the answer is not a JSON object".into());
    }
    let answer: RawAnswer = serde_json::from_value(value).map_err(not_json)?;
    if kind == crate::decide::localize::KIND {
        // A ranked list of files is no option; it must be a list that can be recorded.
        localize::check_answer(&answer.choice)?;
    } else if !options.contains(&answer.choice) {
        return Err(format!(
            "the answer {:?} is not one of the options",
            answer.choice
        ));
    }
    Ok(Answer {
        choice: answer.choice,
        reason: answer.reason,
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::config;
    use super::*;

    fn options() -> Vec<String> {
        vec!["allow".into(), "refuse".into()]
    }

    #[test]
    fn the_request_carries_the_kind_the_options_and_the_cut_context() {
        let model = DecisionModel::new(&config(), Some("secret-key".into()));
        let context = BTreeMap::from([
            ("command".to_string(), "ls".to_string()),
            ("diff".to_string(), "x".repeat(FIELD_LIMIT + 50)),
        ]);
        let body = model.request_body("tool_call", &context, &options());
        assert_eq!(body["model"], "decider");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["role"], "system");
        assert!(
            body["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("\"choice\"")
        );
        let user: Value =
            serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(user["kind"], "tool_call");
        assert_eq!(user["options"], json!(["allow", "refuse"]));
        assert_eq!(user["context"]["command"], "ls");
        let diff = user["context"]["diff"].as_str().unwrap();
        assert_eq!(diff.chars().count(), FIELD_LIMIT + " [cut]".len());
        assert!(diff.ends_with(" [cut]"));
        assert!(!body.to_string().contains("secret-key"));
    }

    #[test]
    fn cutting_keeps_whole_characters_and_short_text_as_it_is() {
        assert_eq!(cut("short"), "short");
        let long = "é".repeat(FIELD_LIMIT + 1);
        let cut = cut(&long);
        assert_eq!(cut.chars().filter(|c| *c == 'é').count(), FIELD_LIMIT);
        assert!(cut.ends_with(" [cut]"));
        assert_eq!(super::cut(&"a".repeat(FIELD_LIMIT)).len(), FIELD_LIMIT);
    }

    #[test]
    fn an_answer_must_be_exactly_the_json_asked_for() {
        let good = parse_answer(
            "merge",
            r#"{"choice": "allow", "reason": "fine"}"#,
            &options(),
        )
        .unwrap();
        assert_eq!(
            (good.choice.as_str(), good.reason.as_str()),
            ("allow", "fine")
        );
        // The reason is optional; padding around the JSON is not a problem.
        assert_eq!(
            parse_answer("merge", "\n {\"choice\": \"refuse\"} \n", &options())
                .unwrap()
                .reason,
            ""
        );
        for bad in [
            r#"{"choice": "maybe", "reason": "x"}"#,
            "I would allow it.",
            "",
            r#"{"reason": "no choice"}"#,
            r#"{"choice": "allow", "extra": 1}"#,
            r#"{"choice": 1}"#,
            r#"["allow"]"#,
            "```json\n{\"choice\": \"allow\", \"reason\": \"x\"}\n```",
        ] {
            assert!(parse_answer("merge", bad, &options()).is_err(), "{bad}");
        }
        assert!(
            parse_answer("merge", r#"{"choice": "maybe"}"#, &options())
                .unwrap_err()
                .contains("not one of the options")
        );
    }
}
