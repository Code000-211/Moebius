//! How the decision model's answers to `localize` are compared. A ranked list of files has no right answer to be equal
//! to, so a shadow is measured against the outcome of its judgement instead: the share of the files the change touched
//! that are among the first ten the model named (recall at ten), set against the recall of the list that was recorded,
//! the baseline's. The outcome arrives when the change is known, so the takeover is considered again then.
//!
//! A takeover of `localize` is proposed when at least `min_judgements` cards have been measured and the model's mean
//! recall is at least [`MARGIN`] above the baseline's over them. The agreement a takeover of the kind records is the
//! model's mean recall.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::shadow::{needed, open_takeover};
use crate::app::{App, Result};
use crate::config::DecisionModelConfig;
use crate::decide::localize::{self, KIND, Outcome};
use crate::store::{Store, Takeover};

/// How much higher than the baseline's the model's mean recall must be for a takeover.
pub const MARGIN: f64 = 0.05;

/// What the shadows of `localize` say of the model against the baseline.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    /// The cards measured: those with a shadow whose change touched at least one file.
    pub measured: i64,
    /// The model's mean recall at ten over them; a failed shadow has recall 0.
    pub recall: Option<f64>,
    /// The baseline's mean recall at ten over the same cards.
    pub baseline_recall: Option<f64>,
}

/// Whether `choice`, the model's answer to a question of `localize`, is a list that can be recorded.
pub fn check_answer(choice: &str) -> std::result::Result<(), String> {
    localize::parse(choice).map(drop)
}

/// The recall at ten of the model's answer (none when it failed or was not a list, which counts as 0) for the change
/// that `outcome` describes; none when the change touched no file.
fn recall_of(answer: Option<&str>, outcome: &Outcome) -> Option<f64> {
    let touched: BTreeSet<&String> = outcome.touched.iter().collect();
    let hinted: Vec<String> = answer
        .and_then(|answer| localize::parse(answer).ok())
        .map(|hints| hints.into_iter().map(|hint| hint.path).collect())
        .unwrap_or_default();
    localize::recall(&hinted, &touched)
}

/// The model against the baseline over the shadows of `localize` whose change is known.
pub fn measure(store: &Store) -> crate::store::Result<Measure> {
    let (mut measured, mut model, mut baseline) = (0_i64, 0.0, 0.0);
    for shadow in store.shadows_with_outcome(KIND)? {
        let Ok(outcome) = serde_json::from_str::<Outcome>(&shadow.outcome) else {
            continue;
        };
        let Some(recall) = recall_of(shadow.shadow_choice.as_deref(), &outcome) else {
            continue;
        };
        measured += 1;
        model += recall;
        baseline += outcome.recall.unwrap_or(0.0);
    }
    let mean = |sum: f64| (measured > 0).then(|| sum / measured as f64);
    Ok(Measure {
        measured,
        recall: mean(model),
        baseline_recall: mean(baseline),
    })
}

/// Whether a takeover of `localize` is due, given the kind's latest takeover (see `shadow::needed`).
fn due(config: &DecisionModelConfig, measure: &Measure, latest: Option<&Takeover>) -> bool {
    let Some(needed) = needed(config, latest) else {
        return false;
    };
    match (measure.recall, measure.baseline_recall) {
        (Some(model), Some(baseline)) => measure.measured >= needed && model >= baseline + MARGIN,
        _ => false,
    }
}

/// Proposes a takeover of `localize` if it is due. Called when a shadow is stored and when an outcome is recorded.
pub(super) fn propose(app: &App, config: &DecisionModelConfig) -> Result<Option<Takeover>> {
    if !app.decisions.decides(KIND) {
        return Ok(None);
    }
    let measure = measure(&app.store)?;
    let latest = app
        .store
        .takeovers()?
        .into_iter()
        .rfind(|takeover| takeover.kind == KIND);
    if !due(config, &measure, latest.as_ref()) {
        return Ok(None);
    }
    open_takeover(
        app,
        KIND,
        measure.recall.unwrap_or_default(),
        measure.measured,
    )
    .map(Some)
}

/// Considers a takeover of `localize` once the outcome of a hint is known; nothing when no decision model is
/// configured.
pub fn reconsider(app: &App) {
    if let Some(shadowing) = app.decisions.shadowing() {
        let _ = propose(app, &shadowing.config);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::config;
    use super::*;
    use crate::store::{NewJudgement, NewShadow};

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        (dir, store)
    }

    /// A judgement of `localize` that named `hinted` for a change that touched `touched`, shadowed by a model that
    /// named `answer`.
    fn shadowed(s: &Store, hinted: &[&str], touched: &[&str], answer: Option<&str>) {
        let strings = |paths: &[&str]| paths.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        let hints: Vec<_> = hinted
            .iter()
            .map(|path| localize::Hint {
                path: path.to_string(),
                reason: String::new(),
            })
            .collect();
        let j = s
            .add_judgement(&NewJudgement {
                kind: KIND.into(),
                choice: localize::encode(&hints),
                decider: "localize".into(),
                ..Default::default()
            })
            .unwrap();
        s.set_outcome(
            j.id,
            &Outcome::of(&strings(hinted), &strings(touched)).json(),
        )
        .unwrap();
        s.add_shadow(&NewShadow {
            judgement_id: j.id,
            kind: KIND.into(),
            choice: answer.map(String::from),
            reason: String::new(),
            failed: answer.is_none(),
        })
        .unwrap();
    }

    fn list(paths: &[&str]) -> String {
        let hints: Vec<_> = paths
            .iter()
            .map(|path| serde_json::json!({ "path": path }))
            .collect();
        serde_json::Value::Array(hints).to_string()
    }

    #[test]
    fn the_model_is_measured_by_the_recall_of_its_list_against_the_baselines() {
        let (_dir, s) = store();
        // The baseline finds one of two touched files; the model both.
        shadowed(
            &s,
            &["a.rs", "x.rs"],
            &["a.rs", "b.rs"],
            Some(&list(&["a.rs", "b.rs"])),
        );
        // The baseline finds the file; a model that failed counts 0.
        shadowed(&s, &["a.rs"], &["a.rs"], None);
        // A change that touched nothing measures nothing.
        shadowed(&s, &["a.rs"], &[], Some(&list(&["a.rs"])));
        let measure = measure(&s).unwrap();
        assert_eq!(measure.measured, 2);
        assert_eq!(measure.recall, Some(0.5));
        assert_eq!(measure.baseline_recall, Some(0.75));
    }

    #[test]
    fn a_takeover_needs_enough_cards_and_a_recall_above_the_baselines_by_the_margin() {
        let config = config();
        let measure = |measured, recall, baseline_recall| Measure {
            measured,
            recall: Some(recall),
            baseline_recall: Some(baseline_recall),
        };
        assert!(!due(&config, &measure(9, 1.0, 0.1), None), "too few cards");
        assert!(
            !due(&config, &measure(10, 0.54, 0.5), None),
            "below the margin"
        );
        assert!(due(&config, &measure(10, 0.56, 0.5), None));
        assert!(!due(&config, &Measure::default(), None));
        let latest = Takeover {
            id: 1,
            kind: KIND.into(),
            agreement: 0.6,
            count: 10,
            status: "dismissed".into(),
            at: 0,
        };
        assert!(!due(&config, &measure(19, 1.0, 0.1), Some(&latest)));
        assert!(due(&config, &measure(20, 1.0, 0.1), Some(&latest)));
    }

    #[test]
    fn an_answer_must_be_a_list_of_at_most_ten_distinct_paths_inside_the_repository() {
        assert!(check_answer(&list(&["a.rs", "src/b.rs"])).is_ok());
        assert!(check_answer("[]").is_ok());
        let eleven: Vec<String> = (0..11).map(|i| format!("f{i}.rs")).collect();
        let eleven: Vec<&str> = eleven.iter().map(String::as_str).collect();
        for bad in [
            list(&eleven),
            list(&["a.rs", "a.rs"]),
            list(&["/etc/passwd"]),
            list(&["../a.rs"]),
            list(&[""]),
            "merge".to_string(),
            r#"{"path": "a.rs"}"#.to_string(),
            r#"[{"path": "a.rs", "extra": 1}]"#.to_string(),
        ] {
            assert!(check_answer(&bad).is_err(), "{bad}");
        }
    }
}
