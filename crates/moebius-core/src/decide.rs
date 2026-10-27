//! Decision points (spec 3.2, ADR 0010): every decision, whoever makes it, goes through [`Decisions::decide`] or
//! [`Decisions::record_person`] and leaves exactly one judgement with its full context.

use std::collections::BTreeMap;
use std::sync::Arc;

use moebius_plugin_api::{DecideRequest, Host};
use serde::{Deserialize, Serialize};

use crate::store::{NewJudgement, Result, Store};

/// A decision to be made.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    /// The kind of decision, such as `tool_call` or `approve_issue`; it picks the decider.
    pub kind: String,
    /// Everything the decision is about, by name. Kept in full (secrets removed) as a blob.
    pub context: BTreeMap<String, String>,
    pub options: Vec<String>,
    /// The option taken when the decider cannot answer; it must be the safe one (fail closed).
    pub fail_closed: String,
    pub run_id: Option<i64>,
    pub card_id: Option<i64>,
}

/// The answer to a [`Question`], as recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub choice: String,
    pub reason: String,
    /// Who decided: a plugin's name (`<plugin>/<part>` when the plugin says which of its parts decided), or `person`.
    pub decider: String,
    /// True when the decider failed and the fail-closed option was taken.
    pub refused: bool,
    pub judgement_id: i64,
}

/// The decision points: which decider answers which kind, and where judgements are recorded.
pub struct Decisions {
    store: Arc<Store>,
    host: Arc<Host>,
    by_kind: BTreeMap<String, String>,
}

impl Decisions {
    pub fn new(store: Arc<Store>, host: Arc<Host>) -> Self {
        Self {
            store,
            host,
            by_kind: BTreeMap::new(),
        }
    }

    /// Lets the decider plugin `plugin` answer decisions of `kind`.
    pub fn route(&mut self, kind: &str, plugin: &str) -> &mut Self {
        self.by_kind.insert(kind.to_string(), plugin.to_string());
        self
    }

    /// Asks the decider for the question's kind and records the judgement. A decider that is missing, times out,
    /// panics, fails or answers outside the options gives the fail-closed option, and the judgement says why.
    pub fn decide(&self, question: &Question) -> Result<Decision> {
        let context = self.keep_context(question)?;
        let decider = self.by_kind.get(&question.kind).cloned();
        let answer = match &decider {
            None => Err(format!("no decider answers {:?}", question.kind)),
            Some(plugin) => self
                .host
                .decide(
                    plugin,
                    &DecideRequest {
                        kind: question.kind.clone(),
                        context: context.text,
                        options: question.options.clone(),
                    },
                )
                .map_err(|refusal| refusal.to_string()),
        };
        let mut decider = decider.unwrap_or_else(|| "none".to_string());
        let (choice, reason, refused) = match answer {
            Ok(d) => {
                if let Some(by) = d.by {
                    decider = format!("{decider}/{by}");
                }
                (d.choice, d.reason, false)
            }
            Err(why) => (
                question.fail_closed.clone(),
                format!("refused: {why}"),
                true,
            ),
        };
        self.record(question, context.blob, choice, reason, decider, refused)
    }

    /// Records a decision a person made in the desktop app (merge, reject, approve, steer, stop).
    pub fn record_person(
        &self,
        question: &Question,
        choice: &str,
        reason: &str,
    ) -> Result<Decision> {
        let context = self.keep_context(question)?;
        self.record(
            question,
            context.blob,
            choice.to_string(),
            reason.to_string(),
            crate::app::PERSON.to_string(),
            false,
        )
    }

    fn keep_context(&self, question: &Question) -> Result<Kept> {
        let text: BTreeMap<String, String> = question
            .context
            .iter()
            .map(|(k, v)| (k.clone(), crate::blobs::redact(v)))
            .collect();
        let blob = self.store.blobs().put(&serde_json::to_string(&text)?)?;
        Ok(Kept { text, blob })
    }

    fn record(
        &self,
        question: &Question,
        blob: crate::blobs::BlobId,
        choice: String,
        reason: String,
        decider: String,
        refused: bool,
    ) -> Result<Decision> {
        let j = self.store.add_judgement(&NewJudgement {
            kind: question.kind.clone(),
            context: vec![blob],
            options: question.options.clone(),
            choice,
            reason,
            decider,
            run_id: question.run_id,
            card_id: question.card_id,
        })?;
        Ok(Decision {
            choice: j.choice,
            reason: j.reason,
            decider: j.decider,
            refused,
            judgement_id: j.id,
        })
    }
}

struct Kept {
    text: BTreeMap<String, String>,
    blob: crate::blobs::BlobId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use moebius_plugin_api::{Call, Kind, Manifest, Plugin, PluginError};
    use serde_json::Value;
    use std::time::Duration;

    struct Refuser;
    impl Plugin for Refuser {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: "refuser".into(),
                version: "0".into(),
                kinds: vec![Kind::Decider],
            }
        }
        fn call(&self, call: Call) -> std::result::Result<Value, PluginError> {
            let r: DecideRequest = serde_json::from_value(call.params).unwrap();
            assert!(
                !r.context["command"].contains("sk-ant-"),
                "deciders never see secrets"
            );
            Ok(serde_json::json!({"choice": "refuse", "reason": "rm -rf is destructive"}))
        }
    }

    struct Broken;
    impl Plugin for Broken {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: "broken".into(),
                version: "0".into(),
                kinds: vec![Kind::Decider],
            }
        }
        fn call(&self, _: Call) -> std::result::Result<Value, PluginError> {
            panic!("decider crashed")
        }
    }

    fn setup() -> (tempfile::TempDir, Arc<Store>, Decisions) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Refuser).add(Broken);
        let mut d = Decisions::new(store.clone(), Arc::new(host));
        d.route("tool_call", "refuser")
            .route("broken_kind", "broken");
        (dir, store, d)
    }

    fn question(kind: &str) -> Question {
        Question {
            kind: kind.into(),
            context: BTreeMap::from([(
                "command".into(),
                format!("rm -rf / # sk-ant-{}", "k".repeat(30)),
            )]),
            options: vec!["allow".into(), "refuse".into()],
            fail_closed: "refuse".into(),
            ..Default::default()
        }
    }

    #[test]
    fn every_decision_leaves_exactly_one_judgement() {
        let (_dir, store, d) = setup();
        let decision = d.decide(&question("tool_call")).unwrap();
        assert_eq!(
            (decision.choice.as_str(), decision.refused),
            ("refuse", false)
        );
        let all = store.judgements("tool_call").unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].decider, "refuser");
        let context = store.blobs().get(&all[0].context[0]).unwrap();
        assert!(context.contains("rm -rf /") && context.contains("[redacted:anthropic-key]"));
    }

    #[test]
    fn a_failing_or_missing_decider_refuses_and_says_so() {
        let (_dir, store, d) = setup();
        for kind in ["broken_kind", "unrouted"] {
            let decision = d.decide(&question(kind)).unwrap();
            assert!(decision.refused);
            assert_eq!(decision.choice, "refuse");
            let all = store.judgements(kind).unwrap();
            assert_eq!(all.len(), 1);
            assert!(all[0].reason.starts_with("refused: "), "{}", all[0].reason);
        }
        assert!(
            store.judgements("broken_kind").unwrap()[0]
                .reason
                .contains("decider crashed")
        );
    }

    #[test]
    fn a_persons_decision_takes_the_same_path() {
        let (_dir, store, d) = setup();
        let decision = d
            .record_person(&question("merge"), "allow", "looks good")
            .unwrap();
        assert_eq!(decision.decider, "person");
        assert_eq!(store.judgements("merge").unwrap().len(), 1);
    }
}
