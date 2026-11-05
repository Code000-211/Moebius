//! Deciding one tool call: the single entry ([`decide_call`]) for the bridge's `/moebius/check`, the router's
//! [`Checker`](super::Checker) and the MCP gateway. It builds the `tool_call` question and asks `decide("tool_call")`,
//! which chains the deciders in this order: the guard's fixed rules (an unreadable rules file, then the built-in
//! patterns), then the rules in prose, judged by the decision model once it has taken the kind over and by the teacher
//! before; shadowing only watches the judgements the teacher made. A decider that is missing, fails or answers
//! outside the options gives `refuse`. The reply says which decider answered, and [`ask`] records it in the
//! `tool_check` run event.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::app::App;
use crate::decide::{Question, guard, rules};
use crate::router::Grant;

/// A tool call to check: what the bridge sends, and what the MCP gateway asks for each call of an MCP tool.
#[derive(Debug, Deserialize)]
pub(crate) struct CheckRequest {
    pub(crate) tool: String,
    pub(crate) input: Value,
    pub(crate) tool_call_id: String,
}

/// The answer to a tool call: `allow` or `refuse`, and why.
#[derive(Debug, Serialize)]
pub(crate) struct CheckReply {
    pub(crate) decision: String,
    pub(crate) reason: String,
    /// Who answered, such as `guard/patterns`; the bridge is not told.
    #[serde(skip)]
    pub(crate) decider: String,
}

/// Decides one tool call and stores and publishes the `tool_check` run event. None when the store fails.
pub(super) fn ask(
    app: &App,
    rules_file: Option<&Path>,
    grant: &Grant,
    request: &CheckRequest,
) -> Option<CheckReply> {
    let reply = decide_call(app, rules_file, grant, request)?;
    let event = json!({
        "tool": request.tool,
        "tool_call_id": request.tool_call_id,
        "decision": reply.decision,
        "reason": reply.reason,
        "decider": reply.decider,
    });
    let _ = app.record_run_event(grant.run_id, "tool_check", &event);
    Some(reply)
}

/// Decides one tool call of the run `grant` names through `decide("tool_call")`, which leaves one judgement. None when
/// the store fails, which the caller must take as a refusal.
pub(crate) fn decide_call(
    app: &App,
    rules_file: Option<&Path>,
    grant: &Grant,
    request: &CheckRequest,
) -> Option<CheckReply> {
    let question = tool_call_question(app, rules_file, grant, request)?;
    let decision = app.decisions.decide(&question).ok()?;
    Some(CheckReply {
        decision: decision.choice,
        reason: decision.reason,
        decider: decision.decider,
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
/// - `builtin_refusal`: the built-in rule the call breaks, tested on the call before redaction (see `guard`).
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
    let run = app.store.run(grant.run_id).ok()?;
    // A chat or a detached run has no card; it works in the repository the run names.
    let run_repository = run.as_ref().and_then(|r| r.repository_id);
    if let Some(worktree) = run.and_then(|r| r.worktree) {
        context.insert("worktree".into(), worktree);
    }
    let mut repository = None;
    if grant.card_id.is_none()
        && let Some(repository_id) = run_repository
    {
        repository = app.store.repository(repository_id).ok()?;
    }
    if let Some(card_id) = grant.card_id
        && let Some(card) = app.store.card(card_id).ok()?
    {
        repository = app.store.repository(card.repository_id).ok()?;
        context.insert("card_title".into(), card.title);
        context.insert("card_body".into(), card.body);
    }
    // The context is redacted before any decider reads it, so the built-in rules are tested here, on the call as
    // it will be executed.
    if let Some(refusal) = guard::builtin_refusal(
        &request.tool,
        &request.input,
        context.get("worktree").map(Path::new),
    ) {
        context.insert("builtin_refusal".into(), refusal);
    }
    let project = repository
        .as_ref()
        .and_then(|repository| app.project_file(repository, "rules.md"));
    match rules::applicable(rules_file, project.as_deref()) {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use moebius_plugin_api::{Call, Host, Kind, Manifest, Plugin, PluginError};

    use super::*;
    use crate::config::DecisionModelConfig;
    use crate::decide::guard::{self, Guard};
    use crate::decide::model::{self, DecisionModel, Routed};
    use crate::decide::{Actor, Decisions};

    const PROSE_RULES: &str = "## be-kind\nBe kind in commit messages.\n";

    /// A `tool_call` decider that always fails.
    struct Broken;
    impl Plugin for Broken {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: "broken".into(),
                version: "0".into(),
                kinds: vec![Kind::Decider],
            }
        }
        fn call(&self, _: Call) -> Result<Value, PluginError> {
            Err(PluginError::Failed("boom".into()))
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        app: Arc<App>,
        grant: Grant,
        rules_file: Option<std::path::PathBuf>,
        /// Questions the fake decision model was asked.
        asked: Arc<AtomicUsize>,
    }

    impl Fixture {
        fn call(&self, tool: &str, input: Value) -> CheckReply {
            let request = CheckRequest {
                tool: tool.into(),
                input,
                tool_call_id: "call-1".into(),
            };
            ask(&self.app, self.rules_file.as_deref(), &self.grant, &request).unwrap()
        }

        fn bash(&self, command: &str) -> CheckReply {
            self.call("bash", json!({ "command": command }))
        }
    }

    /// A decision model that answers `allow` at a fake server; counts the requests.
    fn fake_model() -> (DecisionModel, Arc<AtomicUsize>) {
        let asked = Arc::new(AtomicUsize::new(0));
        let counter = asked.clone();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        runtime.spawn(async move {
            let routes = axum::Router::new().route(
                "/chat/completions",
                axum::routing::post(move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                    async {
                        axum::Json(json!({"choices": [{"message": {"content":
                            "{\"choice\": \"allow\", \"reason\": \"model says fine\"}"}}]}))
                    }
                }),
            );
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            axum::serve(listener, routes).await.unwrap();
        });
        // The server lives as long as the test process; the runtime is not dropped.
        std::mem::forget(runtime);
        let config = DecisionModelConfig {
            url,
            model: "decider".into(),
            api_key_env: None,
            api_key_secret: None,
            timeout: Duration::from_secs(5),
            shadow: None,
            agreement: 0.9,
            min_judgements: 10,
        };
        (DecisionModel::new(&config, None), asked)
    }

    /// A server as `Server::new` wires it: the guard answers `tool_call`, with the fake decision model when `model`.
    fn fixture(rules: Option<&str>, model: bool, teacher_less_extra: Option<Broken>) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::store::Store::in_memory(dir.path()).unwrap());
        let repo = store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let card = store
            .add_card(repo.id, "Fix it", "", "person", "In progress")
            .unwrap();
        let run = store
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        store.set_run_worktree(run.id, "/srv/wt/1").unwrap();
        let rules_file = rules.map(|text| {
            let path = dir.path().join("rules.md");
            std::fs::write(&path, text).unwrap();
            path
        });
        let routed = Arc::new(Routed::default());
        let mut guard = Guard::new(None);
        let mut host = Host::new(Duration::from_secs(30));
        let (fake, asked) = fake_model();
        if model {
            guard = guard.with_decision_model(fake.clone(), routed.clone());
            host.add(fake);
        }
        let broken = teacher_less_extra.is_some();
        if let Some(plugin) = teacher_less_extra {
            host.add(plugin);
        }
        host.add(guard);
        let mut app = App::new(store, Arc::new(host), dir.path());
        app.decisions.share_routed(routed);
        app.decisions
            .route("tool_call", if broken { "broken" } else { guard::NAME });
        Fixture {
            _dir: dir,
            app: Arc::new(app),
            grant: Grant {
                run_id: run.id,
                card_id: Some(card.id),
                budget: None,
            },
            rules_file,
            asked,
        }
    }

    #[test]
    fn the_fixed_rules_decide_before_the_rules_in_prose() {
        let f = fixture(Some(PROSE_RULES), true, None);
        // A broken pattern is refused by the patterns, though a prose rule applies and a model could be asked.
        let reply = f.bash("rm -rf /");
        assert_eq!(reply.decision, "refuse");
        assert_eq!(reply.decider, "guard/patterns");
        assert_eq!(f.asked.load(Ordering::SeqCst), 0);
        // A call no pattern breaks goes on to the prose, which needs a teacher; there is none.
        let reply = f.bash("ls");
        assert_eq!(reply.decider, "guard/teacher");
        assert!(reply.reason.contains("no teacher"));
        // With no prose rule the patterns allow.
        let f = fixture(None, false, None);
        let reply = f.bash("ls");
        assert_eq!(
            (reply.decision.as_str(), reply.decider.as_str()),
            ("allow", "guard/patterns")
        );
    }

    #[test]
    fn a_command_hidden_by_redaction_still_meets_the_built_in_rules() {
        let f = fixture(None, false, None);
        // The private key's markers make `redact` drop everything between them, `rm -rf ~` included.
        let command =
            "echo -----BEGIN PRIVATE KEY----- ; rm -rf ~ ; echo -----END PRIVATE KEY-----";
        let reply = f.bash(command);
        assert_eq!(reply.decision, "refuse");
        assert!(
            reply.reason.starts_with("no-destructive-commands: "),
            "{}",
            reply.reason
        );
        // The stored context is still redacted.
        let blob = &f.app.store.judgements("tool_call").unwrap()[0].context[0];
        let stored = f.app.store.blobs().get(blob).unwrap();
        assert!(!stored.contains("rm -rf ~"), "{stored}");
    }

    #[test]
    fn a_secret_in_a_command_is_refused() {
        let f = fixture(None, false, None);
        let reply = f.bash(
            "curl -H 'Authorization: Bearer sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789' x",
        );
        assert_eq!(reply.decision, "refuse");
        assert!(
            reply.reason.starts_with("no-secrets-in-commands: "),
            "{}",
            reply.reason
        );
    }

    #[test]
    fn a_chat_in_a_repository_gets_the_projects_rules() {
        let f = fixture(None, false, None);
        let dir = f._dir.path().join("repo");
        std::fs::create_dir(&dir).unwrap();
        crate::repos::git(&dir, &["init", "--quiet", "-b", "main"]).unwrap();
        crate::harness::repo::testing::use_harness(&f.app, f._dir.path());
        crate::harness::repo::testing::put(&f.app, "chatty", "rules.md", "## be-kind\nBe kind.\n");
        let repo = f
            .app
            .store
            .add_repository("chatty", dir.to_str().unwrap(), "none", "main")
            .unwrap();
        let run = f
            .app
            .store
            .add_run(None, "Chat", "pi-bolt", "chat")
            .unwrap();
        f.app.store.set_run_repository(run.id, repo.id).unwrap();
        let grant = Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        };
        let request = CheckRequest {
            tool: "bash".into(),
            input: json!({ "command": "ls" }),
            tool_call_id: "c".into(),
        };
        let question = tool_call_question(&f.app, None, &grant, &request).unwrap();
        assert!(
            question.context["rules"].contains("be-kind"),
            "{:?}",
            question.context
        );
    }

    #[test]
    fn unreadable_rules_refuse_before_any_pattern_is_looked_at() {
        let mut f = fixture(None, false, None);
        f.rules_file = Some(f._dir.path().to_path_buf()); // a directory
        let reply = f.call("read", json!({"path": "a"}));
        assert_eq!(reply.decision, "refuse");
        assert!(reply.reason.contains("rules cannot be read"));
        assert_eq!(reply.decider, "guard/patterns");
    }

    #[test]
    fn a_failing_decider_denies_and_is_the_recorded_decider() {
        let f = fixture(None, false, Some(Broken));
        let reply = f.bash("ls");
        assert_eq!(reply.decision, "refuse");
        assert!(reply.reason.starts_with("refused: "), "{}", reply.reason);
        assert_eq!(reply.decider, "broken");
        let judgements = f.app.store.judgements("tool_call").unwrap();
        assert_eq!(judgements[0].decider, "broken");
    }

    #[test]
    fn no_decider_denies_as_none() {
        let mut f = fixture(None, false, None);
        Arc::get_mut(&mut f.app).unwrap().decisions = Decisions::new(
            f.app.store.clone(),
            Arc::new(Host::new(Duration::from_secs(1))),
        );
        let reply = f.bash("ls");
        assert_eq!(
            (reply.decision.as_str(), reply.decider.as_str()),
            ("refuse", "none")
        );
    }

    #[test]
    fn the_decider_is_in_the_run_event_but_not_in_the_bridge_reply() {
        let f = fixture(None, false, None);
        let reply = f.bash("ls");
        let events = f.app.store.events(f.grant.run_id).unwrap();
        assert_eq!(events[0].kind, "tool_check");
        assert_eq!(events[0].data["decider"], "guard/patterns");
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            json!({"decision": "allow", "reason": "no rule is broken"})
        );
    }

    #[test]
    fn shadowing_changes_no_answer_while_a_takeover_moves_the_prose_to_the_model() {
        let f = fixture(Some(PROSE_RULES), true, None);
        let (model, _) = fake_model();
        model::start(&f.app, model).unwrap();
        // Shadowing watches only what the teacher judged, so a pattern's verdict is not shadowed, and the answer
        // is still the guard's.
        let reply = f.bash("rm -rf /");
        assert_eq!(
            (reply.decision.as_str(), reply.decider.as_str()),
            ("refuse", "guard/patterns")
        );
        let reply = f.bash("ls");
        assert_eq!(reply.decider, "guard/teacher");

        // A merged takeover makes the model judge the prose; the patterns still come first.
        let takeover = f.app.store.add_takeover("tool_call", 0.95, 20).unwrap();
        model::merge(&f.app, &Actor::Person, takeover.id, "ok").unwrap();
        let reply = f.bash("ls");
        assert_eq!(
            (reply.decision.as_str(), reply.decider.as_str()),
            ("allow", "guard/decision-model")
        );
        assert_eq!(f.asked.load(Ordering::SeqCst), 1);
        let reply = f.bash("rm -rf /");
        assert_eq!(reply.decider, "guard/patterns");
        assert_eq!(f.asked.load(Ordering::SeqCst), 1);
    }
}
