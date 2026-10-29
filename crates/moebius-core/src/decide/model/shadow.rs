//! Shadowing: the held-out judgements of the shadowed kinds asked of the model in the background, on one worker
//! thread behind a bounded queue, each answer stored as a shadow, and a takeover proposed for a kind that has earned
//! one.

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};

use serde_json::json;

use super::localize as localize_takeover;
use super::{DecisionModel, KEPT_BY_ITS_DECIDER, NAME, TEACHER};
use crate::app::{App, AppError, Result};
use crate::blobs::redact;
use crate::bus::AppEvent;
use crate::config::DecisionModelConfig;
use crate::decide::localize;
use crate::improve::export::held_out;
use crate::store::{Judgement, NewShadow, ShadowStats, Takeover};

/// How many shadow requests wait for the worker before further ones are dropped.
const QUEUE: usize = 64;

/// One judgement to ask the model about.
struct Job {
    judgement_id: i64,
    kind: String,
    context: BTreeMap<String, String>,
    options: Vec<String>,
}

/// The queue of judgements to ask the model about, and how many it dropped.
pub struct Shadowing {
    pub(super) config: DecisionModelConfig,
    queue: SyncSender<Job>,
    dropped: Mutex<BTreeMap<String, i64>>,
}

impl Shadowing {
    /// Queues the judgement if it is held out from training, of a shadowed kind and not made by the model itself. Of
    /// the kind whose decider keeps it ([`KEPT_BY_ITS_DECIDER`]), only the judgements of the part a takeover replaces,
    /// the teacher, are shadowed: those its patterns decide would make the agreement look better than it is. It never
    /// waits: with the queue full the request is dropped and counted.
    pub fn offer(&self, judgement: &Judgement, context: &BTreeMap<String, String>) {
        let by_model = judgement.decider == NAME || judgement.decider.ends_with("/decision-model");
        let shadowed = self
            .config
            .shadow
            .as_ref()
            .is_none_or(|kinds| kinds.contains(&judgement.kind))
            && (judgement.kind != KEPT_BY_ITS_DECIDER || judgement.decider == TEACHER);
        if by_model || !shadowed || !held_out(judgement.id) {
            return;
        }
        let job = Job {
            judgement_id: judgement.id,
            kind: judgement.kind.clone(),
            context: context.clone(),
            options: judgement.options.clone(),
        };
        if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) = self.queue.try_send(job)
        {
            *self
                .dropped
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .entry(judgement.kind.clone())
                .or_default() += 1;
        }
    }

    /// How many requests were dropped for each kind since the server started.
    pub fn dropped(&self) -> BTreeMap<String, i64> {
        self.dropped
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

/// Starts shadowing with `model`, and routes the kinds whose takeovers were merged before. The worker thread ends
/// when the app is dropped.
pub fn start(app: &Arc<App>, model: DecisionModel) -> Result<()> {
    for takeover in app.store.takeovers()? {
        if takeover.status == "merged" {
            app.decisions.routed().add(&takeover.kind);
        }
    }
    let (queue, jobs) = sync_channel(QUEUE);
    let shadowing = Shadowing {
        config: model.config.clone(),
        queue,
        dropped: Mutex::default(),
    };
    if !app.decisions.start_shadowing(shadowing) {
        return Err(AppError::Conflict("shadowing has started already".into()));
    }
    let app = Arc::downgrade(app);
    std::thread::Builder::new()
        .name("decision model shadowing".into())
        .spawn(move || work(&app, &model, jobs))?;
    Ok(())
}

fn work(app: &std::sync::Weak<App>, model: &DecisionModel, jobs: Receiver<Job>) {
    while let Ok(job) = jobs.recv() {
        let Some(app) = app.upgrade() else { return };
        let _ = shadow(&app, model, job);
    }
}

/// Asks the model, stores its answer and proposes a takeover if the kind has earned one.
fn shadow(app: &App, model: &DecisionModel, job: Job) -> Result<()> {
    let (choice, reason, failed) = match model.ask(&job.kind, &job.context, &job.options) {
        Ok(answer) => (Some(answer.choice), answer.reason, false),
        Err(why) => (None, why, true),
    };
    app.store.add_shadow(&NewShadow {
        judgement_id: job.judgement_id,
        kind: job.kind.clone(),
        choice,
        reason: redact(&reason),
        failed,
    })?;
    propose(app, &model.config, &job.kind).map(drop)
}

/// How many shadows a kind needs for a takeover proposal, given the kind's latest takeover: none when it has an open
/// or merged one, and when it was dismissed or released `min_judgements` more than were measured then.
pub(super) fn needed(config: &DecisionModelConfig, latest: Option<&Takeover>) -> Option<i64> {
    match latest {
        None => Some(config.min_judgements),
        Some(t) if matches!(t.status.as_str(), "open" | "merged") => None,
        Some(t) => Some(t.count + config.min_judgements),
    }
}

/// Whether a kind with these `stats` is due a takeover proposal, given the kind's latest takeover (see [`needed`]).
fn due(config: &DecisionModelConfig, stats: &ShadowStats, latest: Option<&Takeover>) -> bool {
    let Some(needed) = needed(config, latest) else {
        return false;
    };
    stats.shadows >= needed && stats.agreement().is_some_and(|a| a >= config.agreement)
}

/// Proposes a takeover of `kind` if it is due: a row, an inbox item and an event. Kinds that only a person decides
/// get none.
fn propose(app: &App, config: &DecisionModelConfig, kind: &str) -> Result<Option<Takeover>> {
    if !app.decisions.decides(kind) {
        return Ok(None);
    }
    if kind == localize::KIND {
        return localize_takeover::propose(app, config);
    }
    let Some(stats) = app.store.shadow_stats(Some(kind))?.pop() else {
        return Ok(None);
    };
    let latest = app
        .store
        .takeovers()?
        .into_iter()
        .rfind(|takeover| takeover.kind == kind);
    if !due(config, &stats, latest.as_ref()) {
        return Ok(None);
    }
    let agreement = stats.agreement().unwrap_or_default();
    open_takeover(app, kind, agreement, stats.shadows).map(Some)
}

/// Proposes the takeover of `kind`: a row, an inbox item and an event.
pub(super) fn open_takeover(app: &App, kind: &str, agreement: f64, count: i64) -> Result<Takeover> {
    let takeover = app.store.add_takeover(kind, agreement, count)?;
    app.store.add_inbox_item(
        "takeover",
        None,
        &format!("The decision model can take over {kind}"),
        &json!({
            "takeover": takeover.id,
            "kind": kind,
            "agreement": agreement,
            "count": count,
        }),
    )?;
    app.bus.publish(AppEvent::new(
        "takeover_proposed",
        json!({ "takeover": takeover.id, "kind": kind, "agreement": agreement }),
    ));
    Ok(takeover)
}

#[cfg(test)]
mod tests {
    use super::super::tests::config;
    use super::*;

    fn stats(shadows: i64, agreed: i64) -> ShadowStats {
        ShadowStats {
            kind: "tool_call".into(),
            shadows,
            agreed,
            failed: 0,
        }
    }

    fn takeover(status: &str, count: i64) -> Takeover {
        Takeover {
            id: 1,
            kind: "tool_call".into(),
            agreement: 1.0,
            count,
            status: status.into(),
            at: 0,
        }
    }

    #[test]
    fn agreement_is_the_share_of_equal_choices_and_a_failure_disagrees() {
        assert_eq!(stats(0, 0).agreement(), None);
        assert_eq!(stats(20, 19).agreement(), Some(0.95));
        let failing = ShadowStats {
            failed: 5,
            ..stats(10, 5)
        };
        assert_eq!(failing.agreement(), Some(0.5));
    }

    #[test]
    fn a_takeover_is_due_with_enough_shadows_and_agreement() {
        let config = config();
        assert!(!due(&config, &stats(9, 9), None), "too few shadows");
        assert!(
            !due(&config, &stats(10, 8), None),
            "agreement 0.8 is below 0.9"
        );
        assert!(
            due(&config, &stats(10, 9), None),
            "exactly the share needed"
        );
        assert!(due(&config, &stats(100, 100), None));
    }

    #[test]
    fn an_open_or_merged_takeover_is_not_proposed_again() {
        let config = config();
        for status in ["open", "merged"] {
            assert!(!due(&config, &stats(500, 500), Some(&takeover(status, 10))));
        }
    }

    #[test]
    fn a_dismissed_or_released_kind_needs_min_judgements_more_shadows() {
        let config = config();
        for status in ["dismissed", "released"] {
            let latest = takeover(status, 10);
            assert!(!due(&config, &stats(19, 19), Some(&latest)), "{status}");
            assert!(!due(&config, &stats(20, 17), Some(&latest)), "{status}");
            assert!(due(&config, &stats(20, 19), Some(&latest)), "{status}");
        }
    }
}
