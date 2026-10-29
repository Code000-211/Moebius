//! What `GET /v1/decision-model` shows: the model and its thresholds, and for each kind its shadows, agreement,
//! failures, dropped requests and whether the model decides it, with the takeovers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::localize as localize_model;
use crate::app::{App, Result};
use crate::decide::localize;
use crate::store::Takeover;

/// What `GET /v1/decision-model` says about one kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KindView {
    pub kind: String,
    pub shadows: i64,
    pub agreement: Option<f64>,
    pub failed: i64,
    pub dropped: i64,
    pub routed: bool,
    /// For `localize`, the cards whose change is known and the model's mean recall at ten and the baseline's over them
    /// (see `localize`); its `agreement` is none. Absent for the other kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measured: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recall: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_recall: Option<f64>,
}

/// What `GET /v1/decision-model` returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub configured: bool,
    pub model: Option<String>,
    /// The share of equal choices a takeover needs.
    pub agreement: Option<f64>,
    pub min_judgements: Option<i64>,
    pub kinds: Vec<KindView>,
    pub takeovers: Vec<Takeover>,
}

/// The view of `kind` in `kinds`, added when it is not there yet.
fn kind_view<'a>(
    kinds: &'a mut BTreeMap<String, KindView>,
    app: &App,
    kind: &str,
) -> &'a mut KindView {
    kinds.entry(kind.to_string()).or_insert_with(|| KindView {
        kind: kind.to_string(),
        shadows: 0,
        agreement: None,
        failed: 0,
        dropped: 0,
        routed: app.decisions.routed().contains(kind),
        measured: None,
        recall: None,
        baseline_recall: None,
    })
}

/// The shadowing as the app shows it: per kind with a shadow, a dropped request or a takeover, and the takeovers.
pub fn view(app: &App) -> Result<View> {
    let Some(shadowing) = app.decisions.shadowing() else {
        return Ok(View {
            configured: false,
            model: None,
            agreement: None,
            min_judgements: None,
            kinds: Vec::new(),
            takeovers: app.store.takeovers()?,
        });
    };
    let takeovers = app.store.takeovers()?;
    let takeovers_merged = takeovers
        .iter()
        .filter(|t| t.status == "merged")
        .map(|t| t.kind.clone());
    let mut kinds: BTreeMap<String, KindView> = BTreeMap::new();
    for stats in app.store.shadow_stats(None)? {
        let view = kind_view(&mut kinds, app, &stats.kind);
        (view.shadows, view.agreement, view.failed) =
            (stats.shadows, stats.agreement(), stats.failed);
    }
    for (kind, count) in shadowing.dropped() {
        kind_view(&mut kinds, app, &kind).dropped = count;
    }
    for takeover in takeovers_merged {
        kind_view(&mut kinds, app, &takeover);
    }
    // A ranked list of files is not measured by agreement but by the recall of its files against the change's.
    if kinds.contains_key(localize::KIND) {
        let measure = localize_model::measure(&app.store)?;
        let view = kind_view(&mut kinds, app, localize::KIND);
        view.agreement = None;
        view.measured = Some(measure.measured);
        view.recall = measure.recall;
        view.baseline_recall = measure.baseline_recall;
    }
    Ok(View {
        configured: true,
        model: Some(shadowing.config.model.clone()),
        agreement: Some(shadowing.config.agreement),
        min_judgements: Some(shadowing.config.min_judgements),
        kinds: kinds.into_values().collect(),
        takeovers,
    })
}
