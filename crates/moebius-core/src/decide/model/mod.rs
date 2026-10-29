//! The decision model (ADR 0010): a model Moebius trains on its own judgements and that is asked through an
//! OpenAI-compatible endpoint (`[decision_model]` in the configuration).
//!
//! It first runs in shadow. Whenever a judgement that is held out from training (see `export::held_out`) is recorded
//! for a shadowed kind, the model is asked the same question in the background, on one worker thread behind a
//! bounded queue, so that no decision and no run ever waits for it; a request that finds the queue full is dropped
//! and counted. Each answer is stored as a shadow, and a failed one counts as disagreement.
//!
//! After each shadow, a kind that a decider plugin answers and that has enough shadows with enough agreement gets a
//! takeover proposal in the inbox. Merging it routes the kind to the model, from then on and after a restart:
//! [`crate::decide::Decisions`] asks the model, registered as the decider plugin [`NAME`], instead of the kind's
//! decider. `tool_call` is the exception: the guard keeps its built-in patterns and only asks the model instead of
//! the teacher about the rules in prose. Releasing a takeover hands the kind back. The rules are documented in
//! `docs/reference/decision-model.md`.
//!
//! This module holds the names and the kinds routed to the model; `ask` has the request to the model and reading its
//! answer, `shadow` the shadowing and the takeover proposals, `view` what the API shows of it, and `takeovers` a
//! person's merge, dismissal and release of a takeover. `localize` is how the answers to the kind `localize` are
//! compared, since a ranked list of files has no choice to be equal to.

mod ask;
pub mod localize;
mod shadow;
mod takeovers;
mod view;

use std::collections::BTreeSet;
use std::sync::Mutex;

pub use ask::{Answer, DecisionModel};
pub use shadow::{Shadowing, start};
pub use takeovers::{dismiss, merge, release};
pub use view::{KindView, View, view};

/// The name of the decision model as a decider plugin, and the `decider` of the judgements it makes.
pub const NAME: &str = "decision-model";

/// The kind whose decider stays when the model takes it over: the guard keeps its built-in patterns.
pub const KEPT_BY_ITS_DECIDER: &str = "tool_call";

/// The decider of a tool call the guard's teacher judged: the part of the guard a takeover replaces.
const TEACHER: &str = "guard/teacher";

/// The kinds the decision model has taken over. Shared by the decisions and the guard.
#[derive(Default)]
pub struct Routed {
    kinds: Mutex<BTreeSet<String>>,
}

impl Routed {
    fn kinds(&self) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
        self.kinds
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn contains(&self, kind: &str) -> bool {
        self.kinds().contains(kind)
    }

    pub fn list(&self) -> Vec<String> {
        self.kinds().iter().cloned().collect()
    }

    fn add(&self, kind: &str) {
        self.kinds().insert(kind.to_string());
    }

    fn remove(&self, kind: &str) {
        self.kinds().remove(kind);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::config::DecisionModelConfig;

    /// A decision model's configuration that takes a kind over at 90 % agreement in 10 shadows.
    pub(super) fn config() -> DecisionModelConfig {
        DecisionModelConfig {
            url: "http://model.test/v1".into(),
            model: "decider".into(),
            api_key_env: None,
            api_key_secret: None,
            timeout: Duration::from_secs(5),
            shadow: None,
            agreement: 0.9,
            min_judgements: 10,
        }
    }
}
