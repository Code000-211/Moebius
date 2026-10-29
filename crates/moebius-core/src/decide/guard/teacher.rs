//! The teacher model (spec 3.2), which judges a tool call that broke no built-in rule against the rules in prose:
//! what it is told, the request through the router, and how its answer is read. Anything that goes wrong is a
//! refusal.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use moebius_plugin_api::Decision;
use serde::Deserialize;
use serde_json::{Value, json};

use super::decision;
use crate::config::TeacherConfig;
use crate::decide::rules::{self, Rule};
use crate::router::{Grant, Protocol, Router};

/// What the teacher is told. It is a constant so that it can be proposed for change (spec 4, the Retro) without
/// touching the code around it.
const TEACHER_PROMPT: &str = "You judge one tool call of a coding agent against a list of rules. \
Decide whether the call breaks any rule. Judge only what the call itself does, with the card and the worktree as \
context, and refuse only when a rule clearly forbids the call. Answer with JSON only, no other text: \
{\"decision\": \"allow\" | \"refuse\", \"rule\": \"<name of the rule broken, or empty>\", \
\"reason\": \"<one sentence>\"}. A refusal must name one of the rules given.";

/// The teacher model (spec 3.2): judges the rules in prose by asking a model through the router's own OpenAI- or
/// Anthropic-compatible endpoint, without streaming. The request carries a token issued for the run being checked and
/// revoked after the call, so the tokens are counted in `usage` like any model call and provider keys stay in the
/// router.
pub struct Teacher {
    model: String,
    protocol: Protocol,
    timeout: Duration,
    router: Arc<Router>,
    /// Where the router listens, such as `http://127.0.0.1:7172`.
    router_url: String,
}

impl Teacher {
    pub fn new(config: &TeacherConfig, router: Arc<Router>, router_url: String) -> Self {
        Self {
            model: config.model.clone(),
            protocol: config.protocol,
            timeout: config.timeout,
            router,
            router_url,
        }
    }

    /// The teacher's decision about the call described by `context`, judged against `rules`. Anything that goes
    /// wrong is a refusal.
    pub(super) fn judge(&self, context: &BTreeMap<String, String>, rules: &[Rule]) -> Decision {
        let answer = self
            .ask(context, rules)
            .and_then(|text| parse_answer(&text, rules));
        let (choice, reason) = match answer {
            Ok(Answer::Allow(reason)) => ("allow", reason),
            Ok(Answer::Refuse { rule, reason }) => ("refuse", format!("{rule}: {reason}")),
            Err(why) => ("refuse", format!("teacher: {why}")),
        };
        decision(choice, reason, "teacher")
    }

    /// The teacher model's reply text.
    fn ask(&self, context: &BTreeMap<String, String>, rules: &[Rule]) -> Result<String, String> {
        let number = |key: &str| context.get(key).and_then(|v| v.parse::<i64>().ok());
        let run_id =
            number("run_id").ok_or("the question names no run to count its tokens against")?;
        let token = self.router.issue_token(Grant {
            run_id,
            card_id: number("card_id"),
            budget: None,
        });
        let reply = self.request(&token, &user_message(context, rules));
        self.router.revoke_token(&token);
        reply
    }

    /// One non-streaming request to the router.
    ///
    /// The blocking client is meant for this: the host runs a plugin on a thread of its own, so waiting here blocks
    /// nothing else, and a plain call reads better than a handle into the server's runtime. The client is built per
    /// call because it must be created and dropped outside of an async context, which this thread is.
    fn request(&self, token: &str, user: &str) -> Result<String, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| e.to_string())?;
        let base = self.router_url.trim_end_matches('/');
        let request = match self.protocol {
            Protocol::OpenAi => client
                .post(format!("{base}/v1/chat/completions"))
                .bearer_auth(token)
                .json(&json!({
                    "model": self.model,
                    "stream": false,
                    "messages": [
                        { "role": "system", "content": TEACHER_PROMPT },
                        { "role": "user", "content": user },
                    ],
                })),
            Protocol::Anthropic => client
                .post(format!("{base}/v1/messages"))
                .header("x-api-key", token)
                .header("anthropic-version", "2023-06-01")
                .json(&json!({
                    "model": self.model,
                    "stream": false,
                    "max_tokens": 1024,
                    "system": TEACHER_PROMPT,
                    "messages": [{ "role": "user", "content": user }],
                })),
        };
        let response = request.send().map_err(|e| {
            if e.is_timeout() {
                format!("no answer within {} s", self.timeout.as_secs())
            } else {
                format!("the request failed: {e}")
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("the router answered HTTP {status}"));
        }
        let body: Value = response
            .json()
            .map_err(|e| format!("the reply is not JSON: {e}"))?;
        let text = match self.protocol {
            Protocol::OpenAi => body.pointer("/choices/0/message/content"),
            Protocol::Anthropic => body.pointer("/content/0/text"),
        };
        text.and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "the reply has no text".to_string())
    }
}

/// What the teacher is asked about one call: the rules, then the call and what it happens in.
fn user_message(context: &BTreeMap<String, String>, rules: &[Rule]) -> String {
    let get = |key: &str| context.get(key).map_or("", String::as_str);
    format!(
        "Rules:\n\n{}\nTool: {}\nInput: {}\nWorktree: {}\nCard title: {}\nCard body:\n{}\n",
        rules::to_markdown(rules),
        get("tool"),
        get("input"),
        get("worktree"),
        get("card_title"),
        get("card_body"),
    )
}

/// The teacher's judgement of a call, once understood.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    Allow(String),
    Refuse { rule: String, reason: String },
}

#[derive(Deserialize)]
struct RawAnswer {
    decision: String,
    #[serde(default)]
    rule: String,
    #[serde(default)]
    reason: String,
}

/// Reads the teacher's reply: JSON, bare or in a ```json fence. A refusal must name one of `rules`.
fn parse_answer(text: &str, rules: &[Rule]) -> Result<Answer, String> {
    let json = text.trim();
    let json = json
        .strip_prefix("```json")
        .or_else(|| json.strip_prefix("```"))
        .and_then(|fenced| fenced.strip_suffix("```"))
        .unwrap_or(json);
    let answer: RawAnswer = serde_json::from_str(json.trim())
        .map_err(|e| format!("the answer is not the JSON asked for ({e})"))?;
    match answer.decision.as_str() {
        "allow" => Ok(Answer::Allow(if answer.reason.is_empty() {
            "the teacher found no rule broken".into()
        } else {
            answer.reason
        })),
        "refuse" if rules.iter().any(|rule| rule.name == answer.rule) => Ok(Answer::Refuse {
            rule: answer.rule,
            reason: answer.reason,
        }),
        "refuse" => Err(format!(
            "the answer refuses for {:?}, which is not one of the rules",
            answer.rule
        )),
        other => Err(format!("{other:?} is neither allow nor refuse")),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use moebius_plugin_api::Host;

    use super::*;
    use crate::decide::guard::{Guard, NAME, WORKTREE};
    use crate::decide::{Decisions, Question};
    use crate::router::Upstream;
    use crate::store::Store;

    const RULES: &str = "## no-billing\nNever change files under billing/.\n";

    /// What the fake upstream answers.
    #[derive(Clone)]
    enum Reply {
        /// A chat completion (or message) whose text is this.
        Text(String),
        Status(u16),
        /// Answers only after this long.
        Slow(Duration),
    }

    struct Fake {
        reply: Mutex<Reply>,
        /// The bodies of the requests that arrived, with their credentials.
        requests: Mutex<Vec<(String, Value)>>,
    }

    impl Fake {
        fn answers(&self, reply: Reply) {
            *self.reply.lock().unwrap() = reply;
        }

        fn answer_text(&self, decision: &str, rule: &str) {
            self.answers(Reply::Text(
                json!({ "decision": decision, "rule": rule, "reason": "it is under billing/" })
                    .to_string(),
            ));
        }

        fn request_count(&self) -> usize {
            self.requests.lock().unwrap().len()
        }
    }

    async fn serve(fake: Arc<Fake>, headers: HeaderMap, body: Value) -> axum::response::Response {
        let credential = ["authorization", "x-api-key"]
            .iter()
            .find_map(|name| headers.get(*name))
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let anthropic = body.get("max_tokens").is_some();
        fake.requests.lock().unwrap().push((credential, body));
        let reply = fake.reply.lock().unwrap().clone();
        let text = match reply {
            Reply::Status(code) => return StatusCode::from_u16(code).unwrap().into_response(),
            Reply::Slow(delay) => {
                tokio::time::sleep(delay).await;
                String::new()
            }
            Reply::Text(text) => text,
        };
        if anthropic {
            axum::Json(json!({
                "model": "teacher-model",
                "content": [{ "type": "text", "text": text }],
                "usage": { "input_tokens": 40, "output_tokens": 8 },
            }))
            .into_response()
        } else {
            axum::Json(json!({
                "model": "teacher-model",
                "choices": [{ "message": { "role": "assistant", "content": text } }],
                "usage": { "prompt_tokens": 40, "completion_tokens": 8 },
            }))
            .into_response()
        }
    }

    /// A guard with a teacher behind a real router, whose upstream is a fake.
    struct Teaching {
        _dir: tempfile::TempDir,
        store: Arc<Store>,
        decisions: Arc<Decisions>,
        fake: Arc<Fake>,
        router: Arc<Router>,
        router_url: String,
        run: i64,
        card: i64,
    }

    async fn teaching(protocol: Protocol, teacher: Option<Duration>) -> Teaching {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let repo = store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let card = store
            .add_card(
                repo.id,
                "Fix invoices",
                "Round the totals",
                "person",
                "In progress",
            )
            .unwrap();
        let run = store
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        let fake = Arc::new(Fake {
            reply: Mutex::new(Reply::Text(String::new())),
            requests: Mutex::new(Vec::new()),
        });
        let upstream = axum::Router::new()
            .route(
                "/v1/chat/completions",
                post(
                    |State(fake): State<Arc<Fake>>,
                     headers: HeaderMap,
                     axum::Json(body): axum::Json<Value>| {
                        serve(fake, headers, body)
                    },
                ),
            )
            .route(
                "/v1/messages",
                post(
                    |State(fake): State<Arc<Fake>>,
                     headers: HeaderMap,
                     axum::Json(body): axum::Json<Value>| {
                        serve(fake, headers, body)
                    },
                ),
            )
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, upstream).await });

        let router = Arc::new(Router::new(
            store.clone(),
            vec![Upstream {
                protocol,
                base_url: upstream_url,
                api_key: None,
            }],
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let router_url = format!("http://{}", listener.local_addr().unwrap());
        let routes = router.clone().routes();
        tokio::spawn(async move { axum::serve(listener, routes).await });

        let teacher = teacher.map(|timeout| {
            let config = TeacherConfig {
                model: "teacher-model".into(),
                protocol,
                timeout,
            };
            Teacher::new(&config, router.clone(), router_url.clone())
        });
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Guard::new(teacher));
        let decisions = Decisions::new(store.clone(), Arc::new(host));
        decisions.route("tool_call", NAME);
        Teaching {
            _dir: dir,
            store,
            decisions: Arc::new(decisions),
            fake,
            router,
            router_url,
            run: run.id,
            card: card.id,
        }
    }

    impl Teaching {
        /// Decides a call of the run through `Decisions`, as the check endpoint does.
        async fn decide(&self, tool: &str, input: Value, rules: &str) -> crate::decide::Decision {
            let question = Question {
                kind: "tool_call".into(),
                context: BTreeMap::from([
                    ("tool".to_string(), tool.to_string()),
                    ("input".to_string(), input.to_string()),
                    ("worktree".to_string(), WORKTREE.to_string()),
                    ("card_title".to_string(), "Fix invoices".to_string()),
                    ("card_body".to_string(), "Round the totals".to_string()),
                    ("run_id".to_string(), self.run.to_string()),
                    ("card_id".to_string(), self.card.to_string()),
                    ("rules".to_string(), rules.to_string()),
                ]),
                options: vec!["allow".into(), "refuse".into()],
                fail_closed: "refuse".into(),
                run_id: Some(self.run),
                card_id: Some(self.card),
            };
            let decisions = self.decisions.clone();
            tokio::task::spawn_blocking(move || decisions.decide(&question).unwrap())
                .await
                .unwrap()
        }

        async fn write_billing(&self) -> crate::decide::Decision {
            self.decide("write", json!({ "path": "billing/invoice.rs" }), RULES)
                .await
        }
    }

    const SECONDS: Duration = Duration::from_secs(20);

    #[tokio::test(flavor = "multi_thread")]
    async fn a_teacher_refuses_a_call_that_breaks_a_prose_rule_and_is_named_and_counted() {
        let t = teaching(Protocol::OpenAi, Some(SECONDS)).await;
        t.fake.answer_text("refuse", "no-billing");
        let decision = t.write_billing().await;
        assert_eq!(decision.choice, "refuse");
        assert!(!decision.refused);
        assert!(
            decision.reason.starts_with("no-billing: "),
            "{}",
            decision.reason
        );
        assert_eq!(decision.decider, "guard/teacher");
        assert_eq!(
            t.store.judgements("tool_call").unwrap()[0].decider,
            "guard/teacher"
        );

        let usage = t.store.usage_for_run(t.run).unwrap();
        assert_eq!(usage.len(), 1);
        assert_eq!((usage[0].input_tokens, usage[0].output_tokens), (40, 8));
        assert_eq!(usage[0].model, "teacher-model");

        // The teacher saw the rules and the call, and the token issued for it is gone.
        let (_, body) = t.fake.requests.lock().unwrap()[0].clone();
        assert_eq!(body["model"], "teacher-model");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["content"], TEACHER_PROMPT);
        let user = body["messages"][1]["content"].as_str().unwrap();
        for expected in [
            "## no-billing",
            "Never change files under billing/.",
            "Tool: write",
            "billing/invoice.rs",
            WORKTREE,
            "Fix invoices",
            "Round the totals",
        ] {
            assert!(user.contains(expected), "{expected} in {user}");
        }
        assert_eq!(t.router.token_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_teacher_that_allows_allows_even_in_a_json_fence() {
        let t = teaching(Protocol::OpenAi, Some(SECONDS)).await;
        t.fake.answer_text("allow", "");
        let decision = t.write_billing().await;
        assert_eq!(decision.choice, "allow");
        assert_eq!(decision.decider, "guard/teacher");

        t.fake.answers(Reply::Text(
            "```json\n{\"decision\": \"refuse\", \"rule\": \"no-billing\", \"reason\": \"no\"}\n```".into(),
        ));
        let decision = t.write_billing().await;
        assert_eq!(decision.choice, "refuse");
        assert_eq!(decision.reason, "no-billing: no");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_anthropic_teacher_is_asked_through_the_messages_endpoint() {
        let t = teaching(Protocol::Anthropic, Some(SECONDS)).await;
        t.fake.answer_text("refuse", "no-billing");
        let decision = t.write_billing().await;
        assert_eq!(decision.choice, "refuse");
        assert_eq!(decision.decider, "guard/teacher");
        let (_, body) = t.fake.requests.lock().unwrap()[0].clone();
        assert_eq!(body["system"], TEACHER_PROMPT);
        assert_eq!(t.store.usage_for_run(t.run).unwrap().len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_teacher_that_fails_or_makes_no_sense_refuses() {
        let t = teaching(Protocol::OpenAi, Some(Duration::from_secs(1))).await;
        let cases = [
            (Reply::Text("I think it is fine.".into()), "not the JSON"),
            (
                Reply::Text(r#"{"decision": "maybe", "rule": "", "reason": ""}"#.into()),
                "neither allow nor refuse",
            ),
            (
                Reply::Text(
                    r#"{"decision": "refuse", "rule": "no-such-rule", "reason": "x"}"#.into(),
                ),
                "not one of the rules",
            ),
            (
                Reply::Text(r#"{"decision": "refuse", "rule": "", "reason": "x"}"#.into()),
                "not one of the rules",
            ),
            (Reply::Status(500), "HTTP 500"),
            (Reply::Slow(Duration::from_secs(4)), "no answer within 1 s"),
        ];
        for (reply, expected) in cases {
            t.fake.answers(reply);
            let decision = t.write_billing().await;
            assert_eq!(decision.choice, "refuse", "{expected}");
            assert_eq!(decision.decider, "guard/teacher", "{expected}");
            assert!(
                decision.reason.starts_with("teacher: ") && decision.reason.contains(expected),
                "{expected}: {}",
                decision.reason
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn without_prose_rules_nobody_is_asked() {
        let t = teaching(Protocol::OpenAi, Some(SECONDS)).await;
        for rules in ["", "just words, no sections\n"] {
            let decision = t
                .decide("write", json!({ "path": "billing/invoice.rs" }), rules)
                .await;
            assert_eq!(decision.choice, "allow");
        }
        assert_eq!(t.fake.request_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn prose_rules_without_a_teacher_refuse() {
        let t = teaching(Protocol::OpenAi, None).await;
        let decision = t.write_billing().await;
        assert_eq!(decision.choice, "refuse");
        assert!(!decision.refused);
        assert!(decision.reason.contains("no teacher is configured"));
        assert_eq!(decision.decider, "guard/teacher");
        assert_eq!(t.fake.request_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_built_in_refusal_is_the_patterns_and_never_reaches_the_teacher() {
        let t = teaching(Protocol::OpenAi, Some(SECONDS)).await;
        t.fake.answer_text("allow", "");
        let decision = t
            .decide("bash", json!({ "command": "git push --force" }), RULES)
            .await;
        assert_eq!(decision.choice, "refuse");
        assert!(decision.reason.starts_with("no-force-push: "));
        assert_eq!(decision.decider, "guard/patterns");
        assert_eq!(t.fake.request_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_question_without_a_run_cannot_be_put_to_the_teacher() {
        let t = teaching(Protocol::OpenAi, Some(SECONDS)).await;
        let rules = rules::parse(RULES);
        let teacher = Teacher::new(
            &TeacherConfig {
                model: "m".into(),
                protocol: Protocol::OpenAi,
                timeout: SECONDS,
            },
            Arc::new(Router::new(t.store.clone(), vec![])),
            t.router_url.clone(),
        );
        let decision = tokio::task::spawn_blocking(move || teacher.judge(&BTreeMap::new(), &rules))
            .await
            .unwrap();
        assert_eq!(decision.choice, "refuse");
        assert!(decision.reason.contains("no run"));
    }
}
