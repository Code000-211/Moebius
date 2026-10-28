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

use moebius_plugin_api::{Call, DecideRequest, Decision, Kind, Manifest, Plugin, PluginError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};

use crate::app::{App, AppError, Result};
use crate::blobs::redact;
use crate::bus::AppEvent;
use crate::config::DecisionModelConfig;
use crate::decide::Question;
use crate::export::held_out;
use crate::store::{Judgement, NewShadow, ShadowStats, Takeover};

/// The name of the decision model as a decider plugin, and the `decider` of the judgements it makes.
pub const NAME: &str = "decision-model";

/// The kind whose decider stays when the model takes it over: the guard keeps its built-in patterns.
pub const KEPT_BY_ITS_DECIDER: &str = "tool_call";

/// The decider of a tool call the guard's teacher judged: the part of the guard a takeover replaces.
const TEACHER: &str = "guard/teacher";

/// The most characters of one field of a context the model is shown. What the model sees is cut from the full
/// context, so that the cut can change without touching the judgements.
const FIELD_LIMIT: usize = 4000;

/// How many shadow requests wait for the worker before further ones are dropped.
const QUEUE: usize = 64;

const SYSTEM_PROMPT: &str = "You are the decision model of Moebius, an orchestration layer for coding agents. \
You are given one decision: its kind, the options and its context. Choose one of the options. Answer with JSON only, \
no other text: {\"choice\": \"<one of the options>\", \"reason\": \"<one sentence>\"}.";

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

    fn add(&self, kind: &str) {
        self.kinds().insert(kind.to_string());
    }

    fn remove(&self, kind: &str) {
        self.kinds().remove(kind);
    }
}

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
        parse_answer(text, options)
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
fn parse_answer(text: &str, options: &[String]) -> std::result::Result<Answer, String> {
    let not_json = |e: serde_json::Error| format!("the answer is not the JSON asked for ({e})");
    let value: Value = serde_json::from_str(text.trim()).map_err(not_json)?;
    // A derived struct would also be read from a JSON array.
    if !value.is_object() {
        return Err("the answer is not a JSON object".into());
    }
    let answer: RawAnswer = serde_json::from_value(value).map_err(not_json)?;
    if !options.contains(&answer.choice) {
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

/// One judgement to ask the model about.
struct Job {
    judgement_id: i64,
    kind: String,
    context: BTreeMap<String, String>,
    options: Vec<String>,
}

/// The queue of judgements to ask the model about, and how many it dropped.
pub struct Shadowing {
    config: DecisionModelConfig,
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

/// Whether a kind with these `stats` is due a takeover proposal, given the kind's latest takeover. A kind with an
/// open or merged takeover is not. One that was dismissed or released is proposed again only once `min_judgements`
/// more shadows have been measured since.
fn due(config: &DecisionModelConfig, stats: &ShadowStats, latest: Option<&Takeover>) -> bool {
    let needed = match latest {
        None => config.min_judgements,
        Some(t) if matches!(t.status.as_str(), "open" | "merged") => return false,
        Some(t) => t.count + config.min_judgements,
    };
    stats.shadows >= needed && stats.agreement().is_some_and(|a| a >= config.agreement)
}

/// Proposes a takeover of `kind` if it is due: a row, an inbox item and an event. Kinds that only a person decides
/// get none.
fn propose(app: &App, config: &DecisionModelConfig, kind: &str) -> Result<Option<Takeover>> {
    if !app.decisions.decides(kind) {
        return Ok(None);
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
    let takeover = app.store.add_takeover(kind, agreement, stats.shadows)?;
    app.store.add_inbox_item(
        "takeover",
        None,
        &format!("The decision model can take over {kind}"),
        &json!({
            "takeover": takeover.id,
            "kind": kind,
            "agreement": agreement,
            "count": stats.shadows,
        }),
    )?;
    app.bus.publish(AppEvent::new(
        "takeover_proposed",
        json!({ "takeover": takeover.id, "kind": kind, "agreement": agreement }),
    ));
    Ok(Some(takeover))
}

/// What `GET /v1/decision-model` says about one kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KindView {
    pub kind: String,
    pub shadows: i64,
    pub agreement: Option<f64>,
    pub failed: i64,
    pub dropped: i64,
    pub routed: bool,
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
    Ok(View {
        configured: true,
        model: Some(shadowing.config.model.clone()),
        agreement: Some(shadowing.config.agreement),
        min_judgements: Some(shadowing.config.min_judgements),
        kinds: kinds.into_values().collect(),
        takeovers,
    })
}

fn publish_status(app: &App, takeover: i64, status: &str) {
    app.bus.publish(AppEvent::new(
        "takeover_status",
        json!({ "takeover": takeover, "status": status }),
    ));
}

/// Records the person's decision about a takeover, then moves it from `from` to `to`.
fn decide_takeover(
    app: &App,
    id: i64,
    from: &str,
    (choice, other): (&str, &str),
    reason: &str,
    to: &str,
) -> Result<Takeover> {
    let takeover = app
        .store
        .takeover(id)?
        .ok_or_else(|| AppError::NotFound(format!("takeover {id}")))?;
    if takeover.status != from {
        return Err(AppError::Conflict(format!(
            "takeover {id} is {}, not {from}",
            takeover.status
        )));
    }
    app.decisions.record_person(
        &Question {
            kind: "takeover".into(),
            context: BTreeMap::from([
                ("takeover".to_string(), id.to_string()),
                ("kind".to_string(), takeover.kind.clone()),
                ("agreement".to_string(), takeover.agreement.to_string()),
                ("count".to_string(), takeover.count.to_string()),
            ]),
            options: vec![choice.into(), other.into()],
            fail_closed: other.into(),
            ..Default::default()
        },
        choice,
        reason,
    )?;
    if !app.store.move_takeover(id, from, to)? {
        return Err(AppError::Conflict(format!(
            "takeover {id} is not {from} any more"
        )));
    }
    app.store.resolve_inbox_for_takeover(id)?;
    publish_status(app, id, to);
    Ok(Takeover {
        status: to.into(),
        ..takeover
    })
}

/// A person merges an open takeover: from now on the decision model decides the kind.
pub fn merge(app: &App, id: i64, reason: &str) -> Result<Takeover> {
    let takeover = decide_takeover(app, id, "open", ("merge", "dismiss"), reason, "merged")?;
    app.decisions.routed().add(&takeover.kind);
    Ok(takeover)
}

/// A person dismisses an open takeover.
pub fn dismiss(app: &App, id: i64, reason: &str) -> Result<Takeover> {
    decide_takeover(app, id, "open", ("dismiss", "merge"), reason, "dismissed")
}

/// A person hands a merged kind back to the decider it had before.
pub fn release(app: &App, id: i64, reason: &str) -> Result<Takeover> {
    let takeover = decide_takeover(app, id, "merged", ("release", "keep"), reason, "released")?;
    app.decisions.routed().remove(&takeover.kind);
    Ok(takeover)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn config() -> DecisionModelConfig {
        DecisionModelConfig {
            url: "http://model.test/v1".into(),
            model: "decider".into(),
            api_key_env: None,
            timeout: Duration::from_secs(5),
            shadow: None,
            agreement: 0.9,
            min_judgements: 10,
        }
    }

    fn options() -> Vec<String> {
        vec!["allow".into(), "refuse".into()]
    }

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
        let good = parse_answer(r#"{"choice": "allow", "reason": "fine"}"#, &options()).unwrap();
        assert_eq!(
            (good.choice.as_str(), good.reason.as_str()),
            ("allow", "fine")
        );
        // The reason is optional; padding around the JSON is not a problem.
        assert_eq!(
            parse_answer("\n {\"choice\": \"refuse\"} \n", &options())
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
            assert!(parse_answer(bad, &options()).is_err(), "{bad}");
        }
        assert!(
            parse_answer(r#"{"choice": "maybe"}"#, &options())
                .unwrap_err()
                .contains("not one of the options")
        );
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
