//! Decision points (spec 3.2, ADR 0010): every decision, whoever makes it, goes through [`Decisions::decide`] or
//! [`Decisions::record_by`] and leaves exactly one judgement with its full context.

pub mod check;
pub mod guard;
pub mod localize;
pub mod model;
pub mod rules;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use moebius_plugin_api::{DecideRequest, Host};
use serde::{Deserialize, Serialize};

use crate::decide::model::{Routed, Shadowing};
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

/// Who acts through the desktop API: a person, or a run of the Moebius agent operating Moebius with its own token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    Person,
    Agent { name: String, run: i64 },
}

impl Actor {
    /// The decider a judgement records for this actor: `person`, or `agent:<name>`.
    pub fn decider(&self) -> String {
        match self {
            Actor::Person => crate::app::PERSON.to_string(),
            Actor::Agent { name, .. } => format!("agent:{name}"),
        }
    }
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
    by_kind: Mutex<BTreeMap<String, String>>,
    /// The kinds the decision model has taken over (see `decision_model`).
    routed: Arc<Routed>,
    /// Set once, when a decision model is configured.
    shadowing: OnceLock<Shadowing>,
}

impl Decisions {
    pub fn new(store: Arc<Store>, host: Arc<Host>) -> Self {
        Self {
            store,
            host,
            by_kind: Mutex::default(),
            routed: Arc::default(),
            shadowing: OnceLock::new(),
        }
    }

    /// Lets the decider plugin `plugin` answer decisions of `kind`.
    pub fn route(&self, kind: &str, plugin: &str) -> &Self {
        self.by_kind
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(kind.to_string(), plugin.to_string());
        self
    }

    /// Whether a decider plugin answers decisions of `kind`, as opposed to a kind only a person decides.
    pub fn decides(&self, kind: &str) -> bool {
        self.by_kind
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains_key(kind)
    }

    /// The plugin that answers each kind that has one, the decision model's takeovers included.
    pub fn routes(&self) -> BTreeMap<String, String> {
        let mut kinds: Vec<String> = self
            .by_kind
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .keys()
            .cloned()
            .collect();
        kinds.extend(self.routed.list());
        kinds
            .into_iter()
            .filter_map(|kind| self.decider_for(&kind).map(|plugin| (kind, plugin)))
            .collect()
    }

    /// The plugins the decisions go through, and how long a call into one may take.
    pub fn host(&self) -> &Host {
        &self.host
    }

    /// Uses `routed`, which the guard shares, as the kinds the decision model has taken over.
    pub fn share_routed(&mut self, routed: Arc<Routed>) {
        self.routed = routed;
    }

    pub fn routed(&self) -> &Routed {
        &self.routed
    }

    /// Starts shadowing; false when it was started already.
    pub fn start_shadowing(&self, shadowing: Shadowing) -> bool {
        self.shadowing.set(shadowing).is_ok()
    }

    pub fn shadowing(&self) -> Option<&Shadowing> {
        self.shadowing.get()
    }

    /// The plugin that answers decisions of `kind`: the decision model once it has taken the kind over, except for
    /// the kind whose built-in rules stay with their decider.
    fn decider_for(&self, kind: &str) -> Option<String> {
        if self.routed.contains(kind) && kind != model::KEPT_BY_ITS_DECIDER {
            return Some(model::NAME.to_string());
        }
        self.by_kind
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(kind)
            .cloned()
    }

    /// Asks the decider for the question's kind and records the judgement. A decider that is missing, times out,
    /// panics, fails or answers outside the options gives the fail-closed option, and the judgement says why.
    pub fn decide(&self, question: &Question) -> Result<Decision> {
        let context = self.keep_context(question)?;
        let decider = self.decider_for(&question.kind);
        let answer = match &decider {
            None => Err(format!("no decider answers {:?}", question.kind)),
            Some(plugin) => self
                .host
                .decide(
                    plugin,
                    &DecideRequest {
                        kind: question.kind.clone(),
                        context: context.text.clone(),
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
        self.record(question, context, choice, reason, decider, refused)
    }

    /// Records a decision made through the desktop API (merge, reject, approve, steer, stop): by a person, or by a run
    /// of the Moebius agent, whose id is the judgement's run unless the decision is about another run, and is then in
    /// the context as `acting_run`.
    pub fn record_by(
        &self,
        actor: &Actor,
        question: &Question,
        choice: &str,
        reason: &str,
    ) -> Result<Decision> {
        let mut question = question.clone();
        if let Actor::Agent { run, .. } = actor {
            question.run_id.get_or_insert(*run);
            question
                .context
                .insert("acting_run".to_string(), run.to_string());
        }
        let context = self.keep_context(&question)?;
        self.record(
            &question,
            context,
            choice.to_string(),
            reason.to_string(),
            actor.decider(),
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
        context: Kept,
        choice: String,
        reason: String,
        decider: String,
        refused: bool,
    ) -> Result<Decision> {
        let j = self.store.add_judgement(&NewJudgement {
            kind: question.kind.clone(),
            context: vec![context.blob],
            options: question.options.clone(),
            choice,
            reason,
            decider,
            run_id: question.run_id,
            card_id: question.card_id,
        })?;
        if let Some(shadowing) = self.shadowing.get() {
            shadowing.offer(&j, &context.text);
        }
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
        let d = Decisions::new(store.clone(), Arc::new(host));
        d.route("tool_call", "refuser");
        d.route("broken_kind", "broken");
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
            .record_by(&Actor::Person, &question("merge"), "allow", "looks good")
            .unwrap();
        assert_eq!(decision.decider, "person");
        assert_eq!(store.judgements("merge").unwrap().len(), 1);
    }
}
