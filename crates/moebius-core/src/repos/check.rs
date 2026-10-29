//! The tool-call check (spec 3.2, ADR 0007): the endpoints the harness's bridge calls on the router's listener. Every
//! tool call of a run is asked before it runs, and every tool result is reported before the model sees it. The run's
//! token (`Authorization: Bearer <token>`, the one the harness uses for the model) authenticates the bridge; an
//! unknown or missing token gets 401.
//!
//! - `POST /moebius/check` takes `{ "tool", "input", "tool_call_id" }` and answers
//!   `{ "decision": "allow" | "refuse", "reason" }`. The call goes through `decide("tool_call")` and leaves one
//!   judgement; a refusal's reason names the rule. Anything that fails while deciding answers refuse, and when even
//!   that cannot be answered the status is 500, which the bridge must treat as a refusal.
//! - `POST /moebius/result` takes `{ "content": [ { "type": "text", "text" }, .. ] }` (Pi's tool result blocks; the
//!   bridge may add `tool_call_id`, `tool` and `is_error`, which are not read) and answers `{ "content": [..] }` with
//!   the secrets removed from every text block and other blocks unchanged.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Json;
use axum::Router as Routes;
use axum::body::Bytes;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::app::App;
use crate::decide::Question;
use crate::decide::rules;
use crate::router::{Grant, Router};

#[derive(Clone)]
struct Check {
    app: Arc<App>,
    router: Arc<Router>,
    rules_file: Option<PathBuf>,
}

/// The check endpoints, authenticated by the tokens `router` issued. `rules_file` is the server's rules file.
pub fn routes(app: Arc<App>, router: Arc<Router>, rules_file: Option<PathBuf>) -> Routes {
    Routes::new()
        .route("/moebius/check", post(check))
        .route("/moebius/result", post(result))
        .with_state(Check {
            app,
            router,
            rules_file,
        })
}

#[derive(Debug, Deserialize)]
struct CheckRequest {
    tool: String,
    input: Value,
    tool_call_id: String,
}

#[derive(Debug, Serialize)]
struct CheckReply {
    decision: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct ResultRequest {
    content: Vec<Value>,
}

#[derive(Debug, Serialize)]
struct ResultReply {
    content: Vec<Value>,
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "not a run's token" })),
    )
        .into_response()
}

/// The body as JSON. It is read after the token is checked, so that a caller without one learns nothing about the
/// body; a body that is not what the endpoint takes is answered 400 or 422.
fn parse<T: DeserializeOwned>(body: &Bytes) -> Result<T, JsonRejection> {
    Json::<T>::from_bytes(body).map(|Json(value)| value)
}

async fn check(State(state): State<Check>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(grant) = state.router.grant(&headers) else {
        return unauthorized();
    };
    let request: CheckRequest = match parse(&body) {
        Ok(request) => request,
        Err(rejection) => return rejection.into_response(),
    };
    let reply = tokio::task::spawn_blocking(move || {
        ask(&state.app, state.rules_file.as_deref(), &grant, &request)
    })
    .await;
    match reply {
        Ok(Some(reply)) => Json(reply).into_response(),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "cannot decide" })),
        )
            .into_response(),
    }
}

/// Decides one tool call and stores and publishes the `tool_check` run event. None when the store fails.
fn ask(
    app: &App,
    rules_file: Option<&Path>,
    grant: &Grant,
    request: &CheckRequest,
) -> Option<CheckReply> {
    let question = tool_call_question(app, rules_file, grant, request)?;
    let decision = app.decisions.decide(&question).ok()?;
    let event = json!({
        "tool": request.tool,
        "tool_call_id": request.tool_call_id,
        "decision": decision.choice,
        "reason": decision.reason,
    });
    let _ = app.record_run_event(grant.run_id, "tool_check", &event);
    Some(CheckReply {
        decision: decision.choice,
        reason: decision.reason,
    })
}

/// The `tool_call` question for one call. Its context is what a decider can read, all as text:
///
/// - `tool`: the tool's name, such as `bash`.
/// - `input`: the call's arguments as JSON text.
/// - `tool_call_id`: the harness's id of the call.
/// - `run_id`, `card_id`: the run being checked and its card (when it has one), as numbers in text. The teacher model's
///   tokens are counted against them.
/// - `worktree`: the run's worktree path, when it has one.
/// - `card_title`, `card_body`: the card the run works on, when it has one.
/// - `rules`: the rules that apply, as Markdown (`## <name>` and the rule's text), the server's first (see `rules`).
/// - `rules_error`: instead of `rules`, why the server's rules file cannot be read. The guard refuses every call
///   then.
fn tool_call_question(
    app: &App,
    rules_file: Option<&Path>,
    grant: &Grant,
    request: &CheckRequest,
) -> Option<Question> {
    let mut context = BTreeMap::from([
        ("tool".to_string(), request.tool.clone()),
        ("input".to_string(), request.input.to_string()),
        ("tool_call_id".to_string(), request.tool_call_id.clone()),
        ("run_id".to_string(), grant.run_id.to_string()),
    ]);
    if let Some(card_id) = grant.card_id {
        context.insert("card_id".into(), card_id.to_string());
    }
    if let Some(worktree) = app.store.run(grant.run_id).ok()?.and_then(|r| r.worktree) {
        context.insert("worktree".into(), worktree);
    }
    let mut repository = None;
    if let Some(card_id) = grant.card_id
        && let Some(card) = app.store.card(card_id).ok()?
    {
        repository = app.store.repository(card.repository_id).ok()?;
        context.insert("card_title".into(), card.title);
        context.insert("card_body".into(), card.body);
    }
    match rules::applicable(rules_file, repository.as_ref()) {
        Ok(rules) => context.insert("rules".into(), rules::to_markdown(&rules)),
        Err(error) => context.insert("rules_error".into(), error.to_string()),
    };
    Some(Question {
        kind: "tool_call".into(),
        context,
        options: vec!["allow".into(), "refuse".into()],
        fail_closed: "refuse".into(),
        run_id: Some(grant.run_id),
        card_id: grant.card_id,
    })
}

async fn result(State(state): State<Check>, headers: HeaderMap, body: Bytes) -> Response {
    if state.router.grant(&headers).is_none() {
        return unauthorized();
    }
    let request: ResultRequest = match parse(&body) {
        Ok(request) => request,
        Err(rejection) => return rejection.into_response(),
    };
    let content = request
        .content
        .into_iter()
        .map(|mut block| {
            if block["type"] == "text"
                && let Some(text) = block["text"].as_str()
            {
                block["text"] = json!(crate::blobs::redact(text));
            }
            block
        })
        .collect();
    Json(ResultReply { content }).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use moebius_plugin_api::{Call, DecideRequest, Host, Kind, Manifest, Plugin, PluginError};
    use std::time::Duration;

    /// Allows `ls`, refuses everything else naming a rule.
    struct Rules;
    impl Plugin for Rules {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: "rules".into(),
                version: "0".into(),
                kinds: vec![Kind::Decider],
            }
        }
        fn call(&self, call: Call) -> Result<Value, PluginError> {
            let r: DecideRequest = serde_json::from_value(call.params).unwrap();
            Ok(if r.context["input"].contains("\"ls\"") {
                json!({"choice": "allow", "reason": "rule read-only: listing is harmless"})
            } else {
                json!({"choice": "refuse", "reason": "rule no-rm: rm is destructive"})
            })
        }
    }

    struct Setup {
        _dir: tempfile::TempDir,
        app: Arc<App>,
        url: String,
        token: String,
        run: i64,
    }

    async fn setup(route: bool) -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::store::Store::in_memory(dir.path()).unwrap());
        let repo = store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let card = store
            .add_card(repo.id, "Fix it", "It is broken", "person", "In progress")
            .unwrap();
        let run = store
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        store.set_run_worktree(run.id, "/srv/wt/1").unwrap();
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Rules);
        let app = App::new(store.clone(), Arc::new(host), dir.path());
        if route {
            app.decisions.route("tool_call", "rules");
        }
        let app = Arc::new(app);
        let router = Arc::new(Router::new(store, vec![]));
        let token = router.issue_token(Grant {
            run_id: run.id,
            card_id: Some(card.id),
            budget: None,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let served = routes(app.clone(), router, None);
        tokio::spawn(async move { axum::serve(listener, served).await });
        Setup {
            _dir: dir,
            app,
            url,
            token,
            run: run.id,
        }
    }

    async fn check_call(s: &Setup, command: &str) -> Value {
        reqwest::Client::new()
            .post(format!("{}/moebius/check", s.url))
            .bearer_auth(&s.token)
            .json(&json!({"tool": "bash", "input": {"command": command}, "tool_call_id": "call-1"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn an_allowed_call_leaves_one_judgement_with_its_context() {
        let s = setup(true).await;
        let reply = check_call(&s, "ls").await;
        assert_eq!(reply["decision"], "allow");
        let judgements = s.app.store.judgements("tool_call").unwrap();
        assert_eq!(judgements.len(), 1);
        assert_eq!(judgements[0].run_id, Some(s.run));
        let context: BTreeMap<String, String> =
            serde_json::from_str(&s.app.store.blobs().get(&judgements[0].context[0]).unwrap())
                .unwrap();
        assert_eq!(context["tool"], "bash");
        assert!(context["input"].contains("\"ls\""));
        assert_eq!(context["worktree"], "/srv/wt/1");
        assert_eq!(context["card_title"], "Fix it");
        assert_eq!(context["card_body"], "It is broken");
    }

    #[tokio::test]
    async fn a_refusal_names_its_rule_and_is_stored_as_a_run_event() {
        let s = setup(true).await;
        let mut bus = s.app.bus.subscribe();
        let reply = check_call(&s, "rm -rf /").await;
        assert_eq!(reply["decision"], "refuse");
        assert!(reply["reason"].as_str().unwrap().contains("rule no-rm"));
        let events = s.app.store.events(s.run).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "tool_check");
        assert_eq!(events[0].data["tool_call_id"], "call-1");
        assert_eq!(events[0].data["decision"], "refuse");
        let published = bus.recv().await.unwrap();
        assert_eq!(published.kind, "run_event");
        assert_eq!(published.data["kind"], "tool_check");
    }

    #[tokio::test]
    async fn without_a_decider_a_call_is_refused_and_still_judged() {
        let s = setup(false).await;
        let reply = check_call(&s, "ls").await;
        assert_eq!(reply["decision"], "refuse");
        assert!(reply["reason"].as_str().unwrap().starts_with("refused: "));
        assert_eq!(s.app.store.judgements("tool_call").unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_server_judges_calls_with_the_guard_and_hands_it_the_rules() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("rules.md"),
            "## be-kind\nBe kind in commit messages.\n",
        )
        .unwrap();
        let config = Config::parse(
            "rules_file = \"rules.md\"",
            dir.path(),
            &dir.path().join("moebius.toml"),
        )
        .unwrap();
        let server = crate::server::Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let repo = server
            .app
            .store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let card = server
            .app
            .store
            .add_card(repo.id, "Fix it", "", "person", "In progress")
            .unwrap();
        let run = server
            .app
            .store
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        server
            .app
            .store
            .set_run_worktree(run.id, "/srv/wt/1")
            .unwrap();
        let token = server.router.issue_token(Grant {
            run_id: run.id,
            card_id: Some(card.id),
            budget: None,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let served = routes(
            server.app.clone(),
            server.router.clone(),
            server.config.rules_file.clone(),
        );
        tokio::spawn(async move { axum::serve(listener, served).await });
        let s = Setup {
            _dir: dir,
            app: server.app.clone(),
            url,
            token,
            run: run.id,
        };

        let refused = check_call(&s, "rm -rf /").await;
        assert_eq!(refused["decision"], "refuse");
        assert!(
            refused["reason"]
                .as_str()
                .unwrap()
                .starts_with("no-destructive-commands: ")
        );
        // The rule in prose needs a teacher, and none is configured.
        let unchecked = check_call(&s, "ls").await;
        assert_eq!(unchecked["decision"], "refuse");
        assert!(unchecked["reason"].as_str().unwrap().contains("no teacher"));

        let judgements = s.app.store.judgements("tool_call").unwrap();
        assert_eq!(judgements.len(), 2);
        assert_eq!(judgements[0].decider, "guard/patterns");
        let context: BTreeMap<String, String> =
            serde_json::from_str(&s.app.store.blobs().get(&judgements[0].context[0]).unwrap())
                .unwrap();
        assert_eq!(
            context["rules"],
            "## be-kind\nBe kind in commit messages.\n"
        );
    }

    #[tokio::test]
    async fn a_result_comes_back_without_secrets() {
        let s = setup(true).await;
        let key = format!("sk-ant-{}", "k".repeat(30));
        let image = json!({"type": "image", "data": key, "mimeType": "image/png"});
        let reply: Value = reqwest::Client::new()
            .post(format!("{}/moebius/result", s.url))
            .bearer_auth(&s.token)
            .json(&json!({
                "tool_call_id": "call-1", "tool": "bash", "is_error": false,
                "content": [{"type": "text", "text": format!("KEY={key}")}, image],
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(reply["content"][0]["text"], "KEY=[redacted:anthropic-key]");
        assert_eq!(reply["content"][1], image);
    }

    #[tokio::test]
    async fn an_unknown_or_missing_token_is_refused() {
        let s = setup(true).await;
        let client = reqwest::Client::new();
        let bodies = [
            (
                "check",
                json!({"tool": "bash", "input": {}, "tool_call_id": "c"}),
            ),
            ("result", json!({"content": []})),
        ];
        for (endpoint, body) in bodies {
            let url = format!("{}/moebius/{endpoint}", s.url);
            let unknown = client
                .post(&url)
                .bearer_auth("moebius-run-nope")
                .json(&body);
            let missing = client.post(&url).json(&body);
            for request in [unknown, missing] {
                assert_eq!(request.send().await.unwrap().status(), 401, "{endpoint}");
            }
        }
        assert!(s.app.store.judgements("tool_call").unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_token_is_checked_before_the_body() {
        let s = setup(true).await;
        let client = reqwest::Client::new();
        for endpoint in ["check", "result"] {
            let url = format!("{}/moebius/{endpoint}", s.url);
            let bad_bodies = [
                client.post(&url).body("not json"),
                client.post(&url).json(&json!({"wrong": 1})),
                client
                    .post(&url)
                    .header("content-type", "text/plain")
                    .body("{}"),
            ];
            for request in bad_bodies {
                assert_eq!(request.send().await.unwrap().status(), 401, "{endpoint}");
            }
            let with_token = |body: &'static str| {
                client
                    .post(&url)
                    .bearer_auth(&s.token)
                    .header("content-type", "application/json")
                    .body(body)
            };
            let syntax = with_token("not json").send().await.unwrap().status();
            let shape = with_token("{\"wrong\": 1}").send().await.unwrap().status();
            assert_eq!((syntax.as_u16(), shape.as_u16()), (400, 422), "{endpoint}");
        }
    }

    #[tokio::test]
    async fn rules_that_cannot_be_read_refuse_every_call_through_the_guard() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where the rules file should be.
        std::fs::create_dir(dir.path().join("rules.md")).unwrap();
        let config = Config::parse(
            "rules_file = \"rules.md\"",
            dir.path(),
            &dir.path().join("moebius.toml"),
        )
        .unwrap();
        let server = crate::server::Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let repo = server
            .app
            .store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let run = server
            .app
            .store
            .add_run(None, "Coder", "pi-bolt", "code")
            .unwrap();
        let _ = repo;
        let grant = Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        };
        let request = CheckRequest {
            tool: "read".into(),
            input: json!({"path": "a"}),
            tool_call_id: "c".into(),
        };
        let reply = tokio::task::spawn_blocking(move || {
            ask(
                &server.app,
                server.config.rules_file.as_deref(),
                &grant,
                &request,
            )
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(reply.decision, "refuse");
        assert!(
            reply.reason.contains("rules cannot be read"),
            "{}",
            reply.reason
        );
        assert!(reply.reason.contains("cannot read the rules file"));
    }
}
