//! The lens's two decision points (issue 114) next to the decision model: `lint_report` and `deep_check` are decided
//! through `decide`, shadowed by the model, listed by `GET /v1/decision-model`, proposed for a takeover and, once the
//! takeover is merged, answered by the model, and a model that fails leaves the safe side: the finding is shown.
//! The model is a fake OpenAI-compatible server that always picks the first option.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use moebius_core::config::Config;
use moebius_core::harness::lens;
use moebius_core::improve::export::held_out;
use moebius_core::router::Grant;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use serde_json::{Value, json};

struct Fake {
    fails: Mutex<bool>,
}

async fn chat(State(fake): State<Arc<Fake>>, body: String) -> Response {
    if *fake.fails.lock().unwrap() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let body: Value = serde_json::from_str(&body).unwrap();
    let user: Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    let content = json!({ "choice": user["options"][0], "reason": "first" }).to_string();
    axum::Json(json!({ "choices": [{ "message": { "role": "assistant", "content": content } }] }))
        .into_response()
}

async fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn serve(app: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    url
}

fn report(n: usize) -> Value {
    json!({
        "file": "src/a.py",
        "edit": { "tool": "edit", "first_line": 1, "last_line": 5, "excerpt": format!("x = {n}") },
        "max_lines": 10,
        "findings": [{ "tool": "ruff", "severity": "warning", "line": 2, "rule": "F401",
                       "message": format!("unused {n}"), "touched": true, "key": format!("ruff|F401|unused {n}") }],
        "present": [format!("ruff|F401|unused {n}")],
        "checks": [{ "phase": "lint", "tool": "ruff", "ms": 12, "findings": 1 }],
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn both_kinds_are_recorded_shadowed_proposed_and_taken_over() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake {
        fails: Mutex::new(false),
    });
    let model_url = serve(
        axum::Router::new()
            .route("/v1/chat/completions", post(chat))
            .with_state(fake.clone()),
    )
    .await;
    let config = Config::parse(
        &format!(
            "[decision_model]\nurl = \"{model_url}/v1\"\nmodel = \"decider\"\nagreement = 0.9\nmin_judgements = 2\n"
        ),
        dir.path(),
        &dir.path().join("moebius.toml"),
    )
    .unwrap();
    let server = Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap());
    let app = server.app.clone();
    let repo = app
        .store
        .add_repository("demo", "/srv/demo", "none", "main")
        .unwrap();
    let card = app
        .store
        .add_card(repo.id, "Fix", "", "person", "In progress")
        .unwrap();
    let run = app
        .store
        .add_run(Some(card.id), "Coder", "pi-bolt", "code")
        .unwrap();
    let token = server.router.issue_token(Grant {
        run_id: run.id,
        card_id: Some(card.id),
        budget: None,
    });
    let lens_url = serve(lens::routes(app.clone(), server.router.clone())).await;
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_url = format!("http://{}", api_listener.local_addr().unwrap());
    tokio::spawn(api::serve(
        api_listener,
        ApiState {
            app: app.clone(),
            runs: server.harness.clone(),
            harness_repo: None,
            token: server.token.clone(),
            router: server.router.clone(),
            config: server.live.clone(),
        },
    ));
    let client = reqwest::Client::new();
    let lens_post = |path: &str, body: Value| {
        client
            .post(format!("{lens_url}/moebius/lens/{path}"))
            .bearer_auth(&token)
            .json(&body)
            .send()
    };

    // Findings are decided until two of their judgements are held out from training, which the model shadows.
    let mut n = 0;
    while app
        .store
        .judgements("lint_report")
        .unwrap()
        .iter()
        .filter(|j| held_out(j.id))
        .count()
        < 2
    {
        n += 1;
        assert!(n < 2000, "no judgement was held out");
        let reply: Value = lens_post("report", report(n))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(reply["shown"], json!([0]));
    }
    let mut m = 0;
    while app
        .store
        .judgements("deep_check")
        .unwrap()
        .iter()
        .filter(|j| held_out(j.id))
        .count()
        < 2
    {
        m += 1;
        assert!(m < 2000, "no judgement was held out");
        let body = json!({ "files": [{ "path": format!("src/a{m}.rs"), "comment_only": false }], "round": 1, "tools": ["cargo"] });
        let reply: Value = lens_post("deep", body).await.unwrap().json().await.unwrap();
        assert_eq!(reply["run"], true);
    }
    for kind in ["lint_report", "deep_check"] {
        let judgements = app.store.judgements(kind).unwrap();
        assert!(
            judgements.iter().all(|j| j.decider == "lens/rules"),
            "{kind}"
        );
        assert!(
            judgements.iter().all(|j| j.run_id == Some(run.id)),
            "{kind}"
        );
        assert!(app.decisions.decides(kind));
    }

    // The model shadows both kinds, agrees with the rule, and a takeover of each is proposed.
    let shadowed = |kind: &str| {
        app.store
            .shadow_stats(Some(kind))
            .unwrap()
            .first()
            .map_or(0, |s| s.shadows)
    };
    wait_for("the shadows", || {
        shadowed("lint_report") >= 2 && shadowed("deep_check") >= 2
    })
    .await;
    wait_for("the proposals", || {
        app.store.takeovers().unwrap().len() == 2
    })
    .await;
    let view: Value = client
        .get(format!("{api_url}/v1/decision-model"))
        .bearer_auth(&*server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let kinds: Vec<&str> = view["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["deep_check", "lint_report"], "{view}");
    assert_eq!(view["kinds"][0]["agreement"], 1.0);
    assert_eq!(view["kinds"][0]["routed"], false);

    // After the takeover the model answers; a model that fails leaves the safe side, and the finding is shown.
    let takeover = app
        .store
        .takeovers()
        .unwrap()
        .into_iter()
        .find(|t| t.kind == "lint_report")
        .unwrap();
    moebius_core::decide::model::merge(
        &app,
        &moebius_core::decide::Actor::Person,
        takeover.id,
        "ok",
    )
    .unwrap();
    *fake.fails.lock().unwrap() = true;
    let mut hidden = report(9999);
    hidden["findings"][0]["touched"] = json!(false);
    let reply: Value = lens_post("report", hidden)
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reply["shown"], json!([0]), "the rule would have hidden it");
    let last = app.store.judgements("lint_report").unwrap().pop().unwrap();
    assert_eq!(last.decider, "decision-model");
    assert_eq!(last.choice, "show");
    assert!(last.reason.starts_with("refused: "), "{}", last.reason);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_endpoints_want_the_runs_token_and_a_judgement_of_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::parse("", dir.path(), &dir.path().join("moebius.toml")).unwrap();
    let server = Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap());
    let app = server.app.clone();
    let run = app.store.add_run(None, "Coder", "pi-bolt", "code").unwrap();
    let other = app.store.add_run(None, "Coder", "pi-bolt", "code").unwrap();
    let grant = |run_id| Grant {
        run_id,
        card_id: None,
        budget: None,
    };
    let token = server.router.issue_token(grant(run.id));
    let url = serve(lens::routes(app.clone(), server.router.clone())).await;
    let client = reqwest::Client::new();
    for endpoint in ["report", "deep", "deep-result"] {
        let url = format!("{url}/moebius/lens/{endpoint}");
        let missing = client.post(&url).json(&json!({})).send().await.unwrap();
        assert_eq!(missing.status(), 401, "{endpoint}");
        let bad = client
            .post(&url)
            .bearer_auth("moebius-run-nope")
            .body("not json")
            .send()
            .await
            .unwrap();
        assert_eq!(bad.status(), 401, "{endpoint}");
        let shape = client
            .post(&url)
            .bearer_auth(&token)
            .json(&json!({ "wrong": 1 }))
            .send()
            .await
            .unwrap();
        assert_eq!(shape.status(), 422, "{endpoint}");
    }

    // The outcome of a deep check is set only on the run's own judgement.
    let ask = |token: &str| {
        client
            .post(format!("{url}/moebius/lens/deep"))
            .bearer_auth(token.to_string())
            .json(&json!({ "files": [{ "path": "README.md" }], "tools": ["cargo"] }))
            .send()
    };
    // No decider is routed in this app's lens... the server routes it: documentation alone is skipped.
    let reply: Value = ask(&token).await.unwrap().json().await.unwrap();
    assert_eq!(reply["run"], false);
    let id = reply["judgement_id"].as_i64().unwrap();
    let theirs = server.router.issue_token(grant(other.id));
    let result = |token: &str| {
        client
            .post(format!("{url}/moebius/lens/deep-result"))
            .bearer_auth(token.to_string())
            .json(&json!({ "judgement_id": id, "new_findings": 1,
                           "checks": [{ "phase": "deep", "tool": "cargo", "ms": 5, "findings": 1, "shown": 1 }] }))
            .send()
    };
    assert_eq!(result(&theirs).await.unwrap().status(), 200);
    assert_eq!(app.store.judgement(id).unwrap().unwrap().outcome, None);
    assert_eq!(result(&token).await.unwrap().status(), 200);
    assert_eq!(
        app.store.judgement(id).unwrap().unwrap().outcome.as_deref(),
        Some("found something new")
    );
    // Each check is a run event of the run that sent it.
    let events = app.store.events(run.id).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "lens_check");
    assert_eq!(events[0].data["phase"], "deep");
    assert_eq!(events[0].data["shown"], 1);
}
