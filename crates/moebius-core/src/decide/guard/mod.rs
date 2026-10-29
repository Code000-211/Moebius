//! The `guard` decider (spec 3.2, ADR 0007): a compiled-in plugin that judges `tool_call` questions in two steps.
//!
//! 1. The built-in rules are fixed patterns over the call (see `patterns`). A call that breaks one is refused with
//!    the rule's name, and the judgement's decider is `guard/patterns`.
//! 2. The rules in prose (`context["rules"]`, see `check`) are judged by the [`Teacher`]. Without prose rules the call
//!    is allowed and nobody is asked. With them and a teacher configured (`[teacher]` in the configuration) the
//!    teacher is asked and the judgement's decider is `guard/teacher`; with them and no teacher the call is refused
//!    (fail closed). The teacher also refuses when it fails, is too slow or answers anything but the JSON asked for.
//!    A refusal's reason starts with the rule's name. Once the decision model has taken `tool_call` over (see
//!    `decision_model`), it is asked instead of the teacher, the judgement's decider is `guard/decision-model`, and it
//!    too refuses when it fails. The built-in patterns of step 1 always apply first.
//!
//! A question whose context has a `rules_error` (the rules files could not be read) is refused outright, by
//! `guard/patterns`.
//!
//! The question's context also carries `run_id` and `card_id` (when the run has a card), so that the teacher's tokens
//! are counted against the run being checked.
//!
//! `shell` reads a call's command line, `patterns` holds the built-in rules, `teacher` asks the teacher model, and
//! `reversible` says whether a call can be replayed (see `replay`). This module is the decider that puts them together.

mod patterns;
mod reversible;
mod shell;
mod teacher;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use moebius_plugin_api::{Call, DecideRequest, Decision, Kind, Manifest, Plugin, PluginError};
use serde_json::Value;

use crate::decide::model::{self, DecisionModel, Routed};
use crate::decide::rules;
use patterns::broken_rule;
use shell::ToolCall;

pub use patterns::patterns;
pub use reversible::{relative_to, reversible};
pub use teacher::Teacher;

/// The plugin's name, which `Decisions` routes `tool_call` to.
pub const NAME: &str = "guard";

fn decision(choice: &str, reason: String, by: &str) -> Decision {
    Decision {
        choice: choice.into(),
        reason,
        by: Some(by.into()),
    }
}

/// The compiled-in decider for `tool_call` questions. Without a teacher, prose rules cannot be checked, so calls are
/// refused while any applies.
pub struct Guard {
    teacher: Option<Teacher>,
    /// The decision model, and the kinds it has taken over, which says whether it answers instead of the teacher.
    model: Option<(DecisionModel, Arc<Routed>)>,
}

impl Guard {
    pub fn new(teacher: Option<Teacher>) -> Self {
        Self {
            teacher,
            model: None,
        }
    }

    /// Lets `model` judge the rules in prose instead of the teacher once `routed` has `tool_call`.
    pub fn with_decision_model(mut self, model: DecisionModel, routed: Arc<Routed>) -> Self {
        self.model = Some((model, routed));
        self
    }

    /// The decision model's decision about the call, which refuses when the model cannot decide.
    fn ask_model(model: &DecisionModel, context: &BTreeMap<String, String>) -> Decision {
        let options = ["allow".to_string(), "refuse".to_string()];
        match model.ask(model::KEPT_BY_ITS_DECIDER, context, &options) {
            Ok(answer) => decision(&answer.choice, answer.reason, model::NAME),
            Err(why) => decision("refuse", format!("decision model: {why}"), model::NAME),
        }
    }
}

impl Plugin for Guard {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: NAME.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kinds: vec![Kind::Decider],
        }
    }

    fn call(&self, call: Call) -> Result<Value, PluginError> {
        let request: DecideRequest = serde_json::from_value(call.params)
            .map_err(|e| PluginError::BadParams(e.to_string()))?;
        if request.kind != "tool_call" {
            return Err(PluginError::Failed(format!(
                "guard decides tool calls, not {:?}",
                request.kind
            )));
        }
        let context = &request.context;
        let tool_call = ToolCall {
            tool: context.get("tool").map_or("", String::as_str),
            input: context
                .get("input")
                .and_then(|input| serde_json::from_str(input).ok())
                .unwrap_or_default(),
            worktree: context.get("worktree").map(Path::new),
        };
        let decision = if let Some(error) = context.get("rules_error") {
            decision(
                "refuse",
                format!("the rules cannot be read, so no call is allowed: {error}"),
                "patterns",
            )
        } else if let Some(reason) = broken_rule(&tool_call) {
            decision("refuse", reason, "patterns")
        } else {
            let prose = rules::parse(context.get("rules").map_or("", String::as_str));
            let model = self
                .model
                .as_ref()
                .filter(|(_, routed)| routed.contains(model::KEPT_BY_ITS_DECIDER));
            match (&self.teacher, model, prose.is_empty()) {
                (_, _, true) => decision("allow", "no rule is broken".into(), "patterns"),
                (_, Some((model, _)), false) => Self::ask_model(model, context),
                (Some(teacher), None, false) => teacher.judge(context, &prose),
                (None, None, false) => decision(
                    "refuse",
                    "no teacher is configured to check the rules in prose".into(),
                    "teacher",
                ),
            }
        };
        serde_json::to_value(decision).map_err(|e| PluginError::Failed(e.to_string()))
    }
}

/// The worktree the calls of the tests are made in.
#[cfg(test)]
const WORKTREE: &str = "/srv/wt/1";

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use moebius_plugin_api::Host;
    use serde_json::json;

    use super::*;

    fn decide(tool: &str, input: Value) -> moebius_plugin_api::Decision {
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Guard::new(None));
        let request = DecideRequest {
            kind: "tool_call".into(),
            context: BTreeMap::from([
                ("tool".to_string(), tool.to_string()),
                ("input".to_string(), input.to_string()),
                ("worktree".to_string(), WORKTREE.to_string()),
            ]),
            options: vec!["allow".into(), "refuse".into()],
        };
        host.decide(NAME, &request).unwrap()
    }

    #[test]
    fn the_plugin_refuses_with_the_rules_name_and_allows_the_rest() {
        let refused = decide("bash", json!({ "command": "git push --force" }));
        assert_eq!(refused.choice, "refuse");
        assert_eq!(
            refused.reason,
            "no-force-push: git push --force rewrites history on the remote"
        );
        let refused = decide("write", json!({ "path": "/etc/passwd" }));
        assert!(refused.reason.starts_with("stay-in-the-worktree: "));
        assert_eq!(
            decide("bash", json!({ "command": "cargo test" })).choice,
            "allow"
        );
        assert_eq!(
            decide("read", json!({ "path": "/etc/hosts" })).choice,
            "allow"
        );
    }

    #[test]
    fn unreadable_rules_refuse_every_call() {
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Guard::new(None));
        let request = DecideRequest {
            kind: "tool_call".into(),
            context: BTreeMap::from([
                ("tool".to_string(), "read".to_string()),
                ("input".to_string(), "{}".to_string()),
                (
                    "rules_error".to_string(),
                    "cannot read rules.md".to_string(),
                ),
            ]),
            options: vec!["allow".into(), "refuse".into()],
        };
        let refused = host.decide(NAME, &request).unwrap();
        assert_eq!(refused.choice, "refuse");
        assert!(refused.reason.contains("rules cannot be read"));
        assert!(refused.reason.contains("cannot read rules.md"));
    }

    #[test]
    fn other_questions_are_not_the_guards() {
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Guard::new(None));
        let request = DecideRequest {
            kind: "merge".into(),
            context: BTreeMap::new(),
            options: vec!["allow".into(), "refuse".into()],
        };
        assert!(host.decide(NAME, &request).is_err());
    }
}
