//! The decision model: it shadows held-out judgements without delaying anything, its agreement is measured on them
//! alone, a takeover proposal reaches the inbox, and merging it routes the kind to the model, which survives a
//! restart, fails closed and can be released. The model is a fake OpenAI-compatible server whose answer each question
//! scripts through the `model_says` field of its context.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use moebius_core::api::{self, ApiState};
use moebius_core::app::App;
use moebius_core::config::Config;
use moebius_core::decide::{Decision, Question};
use moebius_core::decision_model::{self, DecisionModel};
use moebius_core::export::held_out;
use moebius_core::server::Server;
use moebius_core::store::Store;
use moebius_plugin_api::{Call, DecideRequest, Host, Kind, Manifest, Plugin, PluginError};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

/// How the fake model answers.
#[derive(Clone, Copy)]
enum Reply {
    /// The choice named by `model_says` in the question's context, `allow` without one.
    Scripted,
    /// An HTTP 500.
    Fails,
}

struct FakeState {
    reply: Mutex<Reply>,
    /// Each request holds a permit while it is answered; none are available while the gate is closed.
    gate: Semaphore,
    /// The authorization header and the body of every request.
    requests: Mutex<Vec<(Option<String>, Value)>>,
}

async fn chat(State(state): State<Arc<FakeState>>, headers: HeaderMap, body: String) -> Response {
    let body: Value = serde_json::from_str(&body).unwrap();
    let authorization = headers
        .get("authorization")
        .map(|v| v.to_str().unwrap().to_string());
    state
        .requests
        .lock()
        .unwrap()
        .push((authorization, body.clone()));
    state.gate.acquire().await.unwrap().forget();
    if matches!(*state.reply.lock().unwrap(), Reply::Fails) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let user: Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    let says = user["context"]["model_says"].as_str().unwrap_or("allow");
    let content = json!({ "choice": says, "reason": "scripted" }).to_string();
    axum::Json(json!({ "choices": [{ "message": { "role": "assistant", "content": content } }] }))
        .into_response()
}

/// A runtime on its own threads, so that the tests can block on the core's synchronous calls.
struct Fake {
    runtime: tokio::runtime::Runtime,
    url: String,
    state: Arc<FakeState>,
}

impl Fake {
    fn start(open: bool) -> Self {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let state = Arc::new(FakeState {
            reply: Mutex::new(Reply::Scripted),
            gate: Semaphore::new(if open { 1_000_000 } else { 0 }),
            requests: Mutex::default(),
        });
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let app = axum::Router::new()
            .route("/v1/chat/completions", post(chat))
            .with_state(state.clone());
        runtime.spawn(async move { axum::serve(listener, app).await });
        Self {
            runtime,
            url,
            state,
        }
    }

    fn open_gate(&self) {
        self.state.gate.add_permits(1_000_000);
    }

    fn reply(&self, reply: Reply) {
        *self.state.reply.lock().unwrap() = reply;
    }

    fn requests(&self) -> usize {
        self.state.requests.lock().unwrap().len()
    }
}

/// A decider that allows everything.
struct Fixed;

impl Plugin for Fixed {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "fixed".into(),
            version: "0".into(),
            kinds: vec![Kind::Decider],
        }
    }

    fn call(&self, call: Call) -> Result<Value, PluginError> {
        let _: DecideRequest = serde_json::from_value(call.params).unwrap();
        Ok(json!({ "choice": "allow", "reason": "always" }))
    }
}

fn config(dir: &Path, fake: &Fake, min_judgements: i64) -> Config {
    let text = format!(
        "[decision_model]\nurl = {:?}\nmodel = \"decider\"\nagreement = 0.9\nmin_judgements = {min_judgements}\n",
        fake.url
    );
    Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap()
}

/// An app over the store in `dir`, with the kind `triage` decided by `fixed`, and the decision model started.
fn start_app(dir: &Path, fake: &Fake, min_judgements: i64) -> Arc<App> {
    let config = config(dir, fake, min_judgements);
    let model = DecisionModel::new(
        config.decision_model.as_ref().unwrap(),
        Some("test-key".into()),
    );
    let data = dir.join("data");
    let store = Arc::new(Store::open(&data).unwrap());
    let mut host = Host::new(Duration::from_secs(30));
    host.add(Fixed).add(model.clone());
    let app = App::new(store, Arc::new(host), &data);
    app.decisions.route("triage", "fixed");
    let app = Arc::new(app);
    decision_model::start(&app, model).unwrap();
    app
}

fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn question(kind: &str, says: &str, n: usize) -> Question {
    Question {
        kind: kind.into(),
        context: BTreeMap::from([
            ("model_says".to_string(), says.to_string()),
            ("n".to_string(), n.to_string()),
        ]),
        options: vec!["allow".into(), "refuse".into()],
        fail_closed: "refuse".into(),
        ..Default::default()
    }
}

/// Decides `triage` until `count` of the judgements are held out; returns their ids and the other ids.
fn decide_until_held_out(
    app: &App,
    count: usize,
    says: impl Fn(usize) -> &'static str,
) -> (Vec<i64>, Vec<i64>) {
    let (mut held, mut trained) = (Vec::new(), Vec::new());
    for n in 0.. {
        assert!(n < 5000, "no judgement was held out");
        if held.len() == count {
            break;
        }
        let decision = app
            .decisions
            .decide(&question("triage", says(n), n))
            .unwrap();
        if held_out(decision.judgement_id) {
            held.push(decision.judgement_id);
        } else {
            trained.push(decision.judgement_id);
        }
    }
    (held, trained)
}

fn shadows(app: &App, kind: &str) -> i64 {
    app.store
        .shadow_stats(Some(kind))
        .unwrap()
        .first()
        .map_or(0, |s| s.shadows)
}

#[test]
fn shadowing_never_delays_a_decision() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::start(false);
    let app = start_app(dir.path(), &fake, 1000);
    // The model is stuck, yet every decision returns and is recorded; the shadows wait.
    let (held, trained) = decide_until_held_out(&app, 1, |_| "allow");
    assert_eq!(
        app.store.judgements("triage").unwrap().len(),
        held.len() + trained.len()
    );
    assert!(app.store.shadow_stats(None).unwrap().is_empty());
    // Once the model answers, the shadow is stored.
    fake.open_gate();
    wait_for("the shadow", || shadows(&app, "triage") == 1);
    let stats = app.store.shadow_stats(None).unwrap();
    assert_eq!((stats[0].shadows, stats[0].agreed), (1, 1));
    let (authorization, body) = fake.state.requests.lock().unwrap()[0].clone();
    assert_eq!(authorization.as_deref(), Some("Bearer test-key"));
    assert_eq!(body["model"], "decider");
    assert!(app.decisions.shadowing().unwrap().dropped().is_empty());
}

#[test]
fn agreement_is_measured_on_held_out_judgements_only() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::start(true);
    let app = start_app(dir.path(), &fake, 1000);
    // The model agrees (allow) on every other question and disagrees (refuse) on the rest.
    let says = |n: usize| {
        if n.is_multiple_of(2) {
            "allow"
        } else {
            "refuse"
        }
    };
    let mut all = (Vec::new(), Vec::new());
    for n in 0..400 {
        let decision = app
            .decisions
            .decide(&question("triage", says(n), n))
            .unwrap();
        if held_out(decision.judgement_id) {
            all.0.push((decision.judgement_id, n));
        } else {
            all.1.push(decision.judgement_id);
        }
    }
    let (held, trained) = all;
    assert!(!held.is_empty() && !trained.is_empty());
    let agreeing = held.iter().filter(|(_, n)| n.is_multiple_of(2)).count() as i64;
    assert!(
        agreeing > 0 && agreeing < held.len() as i64,
        "the sample has both"
    );
    wait_for("every held-out shadow", || {
        shadows(&app, "triage") == held.len() as i64
    });
    let stats = &app.store.shadow_stats(None).unwrap()[0];
    assert_eq!(
        (stats.shadows, stats.agreed, stats.failed),
        (held.len() as i64, agreeing, 0)
    );
    assert_eq!(stats.agreement(), Some(agreeing as f64 / held.len() as f64));
    // The model was never asked about the judgements that are trained on.
    assert_eq!(fake.requests(), held.len());
}

#[test]
fn a_model_that_fails_disagrees_and_earns_no_takeover() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::start(true);
    fake.reply(Reply::Fails);
    let app = start_app(dir.path(), &fake, 2);
    let (held, _) = decide_until_held_out(&app, 3, |_| "allow");
    wait_for("the shadows", || {
        shadows(&app, "triage") == held.len() as i64
    });
    let stats = &app.store.shadow_stats(None).unwrap()[0];
    assert_eq!(
        (stats.agreed, stats.failed, stats.agreement()),
        (0, 3, Some(0.0))
    );
    assert!(app.store.takeovers().unwrap().is_empty());
    assert!(app.store.inbox().unwrap().is_empty());
}

/// Gets `triage` taken over: enough agreeing held-out judgements, then the person merges the proposal.
fn merged(app: &Arc<App>) -> i64 {
    decide_until_held_out(app, 3, |_| "allow");
    wait_for("the proposal in the inbox", || {
        !app.store.inbox().unwrap().is_empty()
    });
    let takeover = app.store.takeovers().unwrap().remove(0);
    decision_model::merge(
        app,
        &moebius_core::decide::Actor::Person,
        takeover.id,
        "it agrees",
    )
    .unwrap();
    takeover.id
}

#[test]
fn a_takeover_is_proposed_in_the_inbox_and_merging_it_routes_the_kind() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::start(true);
    let app = start_app(dir.path(), &fake, 3);

    // A kind only a person decides is shadowed and measured, but never proposed. The worker handles the shadows in
    // order, so those of this kind are done before the proposal below.
    let mut n = 0;
    while shadows(&app, "approve") < 3 {
        n += 1;
        assert!(n < 5000);
        app.decisions
            .record_by(
                &moebius_core::decide::Actor::Person,
                &question("approve", "allow", n),
                "allow",
                "ok",
            )
            .unwrap();
    }
    assert!(app.store.takeovers().unwrap().is_empty());

    let mut events = app.bus.subscribe();
    let (held, _) = decide_until_held_out(&app, 3, |_| "allow");
    // The event comes last, after the takeover and its inbox item are stored.
    let proposed = loop {
        let event = events.blocking_recv().unwrap();
        if event.kind == "takeover_proposed" {
            break event.data;
        }
    };
    let takeovers = app.store.takeovers().unwrap();
    assert_eq!(takeovers.len(), 1, "one proposal, for triage only");
    let takeover = &takeovers[0];
    assert_eq!(
        (
            takeover.kind.as_str(),
            takeover.status.as_str(),
            takeover.agreement
        ),
        ("triage", "open", 1.0)
    );
    assert!(takeover.count >= held.len() as i64);
    let inbox = app.store.inbox().unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(
        (inbox[0].kind.as_str(), inbox[0].title.as_str()),
        ("takeover", "The decision model can take over triage")
    );
    assert_eq!(inbox[0].data["takeover"], takeover.id);
    assert_eq!(inbox[0].data["agreement"], 1.0);
    assert_eq!(
        proposed,
        json!({ "takeover": takeover.id, "kind": "triage", "agreement": 1.0 })
    );
    // Before the merge the fixed decider answers.
    let before = app
        .decisions
        .decide(&question("triage", "refuse", 0))
        .unwrap();
    assert_eq!(
        (before.choice.as_str(), before.decider.as_str()),
        ("allow", "fixed")
    );

    // The person merges: the decision is recorded, the item is resolved, and the model decides from now on.
    let merged = decision_model::merge(
        &app,
        &moebius_core::decide::Actor::Person,
        takeover.id,
        "it agrees often enough",
    )
    .unwrap();
    assert_eq!(merged.status, "merged");
    assert_eq!(
        app.decisions.routes().get("triage").map(String::as_str),
        Some("decision-model")
    );
    assert!(app.store.inbox().unwrap().is_empty());
    let recorded = app.store.judgements("takeover").unwrap();
    assert_eq!(
        (
            recorded[0].decider.as_str(),
            recorded[0].choice.as_str(),
            recorded[0].reason.as_str()
        ),
        ("person", "merge", "it agrees often enough")
    );
    let after = app
        .decisions
        .decide(&question("triage", "refuse", 1))
        .unwrap();
    assert_eq!(
        (after.choice.as_str(), after.decider.as_str(), after.refused),
        ("refuse", "decision-model", false)
    );
    assert_eq!(
        after.judgement_id,
        app.store.judgements("triage").unwrap().last().unwrap().id
    );
    assert!(
        decision_model::merge(&app, &moebius_core::decide::Actor::Person, takeover.id, "").is_err(),
        "a takeover is merged once"
    );
    let view = decision_model::view(&app).unwrap();
    let triage = view.kinds.iter().find(|k| k.kind == "triage").unwrap();
    assert!(triage.routed && triage.shadows >= 3);
    assert!(
        !view
            .kinds
            .iter()
            .find(|k| k.kind == "approve")
            .unwrap()
            .routed
    );
}

#[test]
fn the_route_survives_a_restart_and_a_release_hands_the_kind_back() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::start(true);
    let id = {
        let app = start_app(dir.path(), &fake, 3);
        merged(&app)
    };

    // A new app over the same store reads the route from it.
    let app = start_app(dir.path(), &fake, 3);
    let decision = app
        .decisions
        .decide(&question("triage", "refuse", 7))
        .unwrap();
    assert_eq!(
        (decision.choice.as_str(), decision.decider.as_str()),
        ("refuse", "decision-model")
    );

    // A model that fails makes the decision fail closed, and says why.
    fake.reply(Reply::Fails);
    let failed: Decision = app
        .decisions
        .decide(&question("triage", "allow", 8))
        .unwrap();
    assert!(failed.refused);
    assert_eq!(
        (failed.choice.as_str(), failed.decider.as_str()),
        ("refuse", "decision-model")
    );
    let reason = &app
        .store
        .judgement(failed.judgement_id)
        .unwrap()
        .unwrap()
        .reason;
    assert!(
        reason.starts_with("refused: ") && reason.contains("HTTP 500"),
        "{reason}"
    );
    fake.reply(Reply::Scripted);

    let released = decision_model::release(
        &app,
        &moebius_core::decide::Actor::Person,
        id,
        "back to the rule",
    )
    .unwrap();
    assert_eq!(released.status, "released");
    assert_eq!(
        app.decisions.routes().get("triage").map(String::as_str),
        Some("fixed")
    );
    let decision = app
        .decisions
        .decide(&question("triage", "refuse", 9))
        .unwrap();
    assert_eq!(
        (decision.choice.as_str(), decision.decider.as_str()),
        ("allow", "fixed")
    );
    assert!(decision_model::release(&app, &moebius_core::decide::Actor::Person, id, "").is_err());
    // Released, it is not routed after a restart either.
    drop(app);
    let app = start_app(dir.path(), &fake, 3);
    let decision = app
        .decisions
        .decide(&question("triage", "refuse", 10))
        .unwrap();
    assert_eq!(decision.decider, "fixed");
}

#[test]
fn a_dismissed_takeover_leaves_the_kind_with_its_decider() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::start(true);
    let app = start_app(dir.path(), &fake, 3);
    decide_until_held_out(&app, 3, |_| "allow");
    wait_for("the proposal in the inbox", || {
        !app.store.inbox().unwrap().is_empty()
    });
    let id = app.store.takeovers().unwrap()[0].id;
    let dismissed =
        decision_model::dismiss(&app, &moebius_core::decide::Actor::Person, id, "not yet").unwrap();
    assert_eq!(dismissed.status, "dismissed");
    assert!(app.store.inbox().unwrap().is_empty());
    let decision = app
        .decisions
        .decide(&question("triage", "refuse", 0))
        .unwrap();
    assert_eq!(decision.decider, "fixed");
}

/// The API of a server, on the fake's runtime.
struct Api {
    base: String,
    token: String,
    client: reqwest::blocking::Client,
}

impl Api {
    fn start(fake: &Fake, server: &Server) -> Self {
        let listener = fake
            .runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            app: server.app.clone(),
            runs: server.harness.clone(),
            harness_repo: None,
            token: server.token.clone(),
            router: server.router.clone(),
            config: server.live.clone(),
        };
        fake.runtime.spawn(api::serve(listener, state));
        Self {
            base,
            token: server.token.to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }

    fn get(&self, path: &str) -> (u16, Value) {
        let response = self
            .client
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .send()
            .unwrap();
        (response.status().as_u16(), response.json().unwrap())
    }

    fn post(&self, path: &str) -> (u16, Value) {
        let response = self
            .client
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .json(&json!({ "reason": "because" }))
            .send()
            .unwrap();
        let status = response.status().as_u16();
        (status, response.json().unwrap_or(Value::Null))
    }
}

/// A tool call under a rule in prose, with nobody to judge the rule but the guard's teacher, which is not set up.
fn tool_call(server: &Server, command: &str, says: &str, n: usize) -> Decision {
    let mut q = question("tool_call", says, n);
    q.context.extend([
        ("tool".to_string(), "bash".to_string()),
        (
            "input".to_string(),
            json!({ "command": command }).to_string(),
        ),
        ("worktree".to_string(), "/srv/worktrees/1".to_string()),
        (
            "rules".to_string(),
            "## keep-src\nNever touch src.".to_string(),
        ),
    ]);
    server.app.decisions.decide(&q).unwrap()
}

#[test]
fn for_tool_calls_the_model_answers_the_rules_in_prose_and_the_patterns_stay() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::start(true);
    let config = config(dir.path(), &fake, 2);
    let _runtime = fake.runtime.enter();
    let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
    let api = Api::start(&fake, &server);

    // The patterns' own refusals are not what a takeover replaces, so they are not shadowed: had they been, the model
    // (which would allow them) would disagree on them and the agreement below would not be whole.
    let mut n = 0;
    while !held_out(tool_call(&server, "rm -rf /", "allow", n).judgement_id) {
        n += 1;
        assert!(n < 5000, "no call was held out");
    }
    // Without the model, the rule in prose cannot be judged, so the guard refuses; the model agrees.
    assert_eq!(api.get("/v1/decision-model").1["configured"], true);
    while server.app.store.inbox().unwrap().is_empty() {
        n += 1;
        assert!(n < 5000, "no takeover was proposed");
        let decision = tool_call(&server, "echo hi", "refuse", n);
        assert_eq!(
            (decision.choice.as_str(), decision.decider.as_str()),
            ("refuse", "guard/teacher")
        );
    }
    let (status, takeovers) = api.get("/v1/takeovers");
    assert_eq!(status, 200);
    assert_eq!(takeovers[0]["kind"], "tool_call");
    assert_eq!(takeovers[0]["status"], "open");
    let id = takeovers[0]["id"].as_i64().unwrap();

    let (status, view) = api.get("/v1/decision-model");
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["model"], "decider");
    assert_eq!(view["min_judgements"], 2);
    assert_eq!(view["kinds"][0]["kind"], "tool_call");
    assert_eq!(view["kinds"][0]["routed"], false);
    assert_eq!(view["kinds"][0]["agreement"], 1.0);
    assert_eq!(view["kinds"][0]["dropped"], 0);

    let (status, merged) = api.post(&format!("/v1/takeovers/{id}/merge"));
    assert_eq!((status, merged["status"].as_str()), (200, Some("merged")));
    assert_eq!(api.post(&format!("/v1/takeovers/{id}/merge")).0, 409);
    assert_eq!(api.post("/v1/takeovers/999/merge").0, 404);
    assert_eq!(api.get("/v1/decision-model").1["kinds"][0]["routed"], true);

    // The model, not the teacher, answers the rule in prose.
    let allowed = tool_call(&server, "echo hi", "allow", 1);
    assert_eq!(
        (allowed.choice.as_str(), allowed.decider.as_str()),
        ("allow", "guard/decision-model")
    );
    // The built-in patterns still refuse, whatever the model would say.
    let wiped = tool_call(&server, "rm -rf /", "allow", 2);
    assert_eq!(
        (wiped.choice.as_str(), wiped.decider.as_str()),
        ("refuse", "guard/patterns")
    );
    // A model that fails refuses the call.
    fake.reply(Reply::Fails);
    let failed = tool_call(&server, "echo hi", "allow", 3);
    assert_eq!(
        (failed.choice.as_str(), failed.decider.as_str()),
        ("refuse", "guard/decision-model")
    );
    assert!(failed.reason_of(&server).starts_with("decision model: "));
    fake.reply(Reply::Scripted);

    // Released, the teacher (which is not set up) is the one again.
    let (status, released) = api.post(&format!("/v1/takeovers/{id}/release"));
    assert_eq!(
        (status, released["status"].as_str()),
        (200, Some("released"))
    );
    let back = tool_call(&server, "echo hi", "allow", 4);
    assert_eq!(back.decider, "guard/teacher");
}

trait ReasonOf {
    fn reason_of(&self, server: &Server) -> String;
}

impl ReasonOf for Decision {
    fn reason_of(&self, server: &Server) -> String {
        server
            .app
            .store
            .judgement(self.judgement_id)
            .unwrap()
            .unwrap()
            .reason
    }
}
