//! Model classes: `[router.classes]` names a class, such as `fast` or `strong`, for an ordered list of models, and an
//! agent, the teacher or a chat may name the class where it names a model. The router serves a class as a model name:
//! it sends the request to the first model of the list that is within its day's tokens, on the first upstream of the
//! request's protocol, and falls back to the next upstream, then the next model, when an upstream fails before it
//! answers. Once an answer is passed on to the harness nothing falls back any more.
//!
//! A request that names a model rather than a class falls back over the upstreams of its protocol the same way.

use std::collections::BTreeMap;
use std::time::Instant;

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Protocol, Router, Upstream, error};

/// One model of a class, and the most tokens it may use in a day of the server's local time, if limited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Entry")]
pub struct ClassModel {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_tokens: Option<i64>,
}

/// A model of a class as the file writes it: a name, or a table with the name and its day's tokens.
#[derive(Deserialize)]
#[serde(untagged)]
enum Entry {
    Name(String),
    Table(Table),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    model: String,
    day_tokens: Option<i64>,
}

impl From<Entry> for ClassModel {
    fn from(entry: Entry) -> Self {
        match entry {
            Entry::Name(model) => ClassModel {
                model,
                day_tokens: None,
            },
            Entry::Table(table) => ClassModel {
                model: table.model,
                day_tokens: table.day_tokens,
            },
        }
    }
}

/// The classes by name.
pub type Classes = BTreeMap<String, Vec<ClassModel>>;

/// What is wrong with a class, if anything: its name must be letters, digits, `-` and `_`, it must list a model, every
/// model must have a name, and a day's tokens must be at least 1.
pub fn check_class(name: &str, models: &[ClassModel]) -> Result<(), String> {
    if !crate::config::valid_agent_name(name) {
        return Err(format!("{name:?} is not a name (letters, digits, - and _)"));
    }
    if models.is_empty() {
        return Err(format!("{name} lists no model"));
    }
    for model in models {
        if model.model.trim().is_empty() {
            return Err(format!("{name} has a model without a name"));
        }
        if model.day_tokens.is_some_and(|tokens| tokens < 1) {
            return Err(format!(
                "{name}: day_tokens of {} must be at least 1",
                model.model
            ));
        }
    }
    Ok(())
}

/// Whether an upstream that answered with `status` has failed before answering, so that the next one is tried:
/// a server error, too many requests, or not found (the upstream does not serve the model).
fn falls_back(status: StatusCode) -> bool {
    status.is_server_error()
        || status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::NOT_FOUND
}

/// The answer of the upstream that took a request, and the model it was asked for.
pub(super) struct Answered {
    pub(super) response: reqwest::Response,
    pub(super) model: String,
    /// When the request went to the upstream that answered.
    pub(super) started: Instant,
}

impl Router {
    /// Takes the classes, for a router being built.
    pub fn with_classes(self, classes: Classes) -> Self {
        self.set_classes(classes);
        self
    }

    /// Replaces the classes; the next request is served with the new ones.
    pub fn set_classes(&self, classes: Classes) {
        *self.classes.write().unwrap_or_else(|p| p.into_inner()) = classes;
    }

    /// The classes the router serves.
    pub fn classes(&self) -> Classes {
        self.classes
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// The upstreams of `protocol`, in the order of the file.
    pub(super) fn upstreams_of(&self, protocol: Protocol) -> Vec<Upstream> {
        self.upstreams
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|u| u.protocol == protocol)
            .cloned()
            .collect()
    }

    /// The models to ask for a request that names `model`: when it names a class, the class and those of its models
    /// that have tokens left today, in order; otherwise no class and `model` itself. A class whose every model has
    /// spent its day's tokens is refused with the message to give. Usage that cannot be read counts as spent.
    pub(super) fn candidates(&self, model: &str) -> Result<(Option<String>, Vec<String>), String> {
        let Some(models) = self
            .classes
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(model)
            .cloned()
        else {
            return Ok((None, vec![model.to_string()]));
        };
        let within: Vec<String> = models
            .into_iter()
            .filter(|m| match m.day_tokens {
                None => true,
                Some(limit) => self
                    .store
                    .model_tokens_today(&m.model)
                    .is_ok_and(|used| used < limit),
            })
            .map(|m| m.model)
            .collect();
        if within.is_empty() {
            return Err(format!(
                "every model of the class {model} has spent its tokens for today"
            ));
        }
        Ok((Some(model.to_string()), within))
    }

    /// Sends `request` to the first of `models` on the first of `upstreams` that answers, the model name set to the
    /// one asked when `rename` is set. An upstream that cannot be reached, is not logged in, or answers with a status
    /// that [`falls_back`] gives the request to the next upstream, then to the next model. The last one's failure is
    /// what the harness gets.
    pub(super) async fn send_first(
        &self,
        protocol: Protocol,
        path: &str,
        headers: &HeaderMap,
        request: &mut Value,
        upstreams: &[Upstream],
        (models, rename): (&[String], bool),
    ) -> Result<Answered, Box<Response>> {
        let attempts: Vec<(&String, &Upstream)> = models
            .iter()
            .flat_map(|model| upstreams.iter().map(move |upstream| (model, upstream)))
            .collect();
        let mut failure = error(
            protocol,
            StatusCode::BAD_GATEWAY,
            "api_error",
            "no upstream for this protocol",
        );
        for (n, (model, upstream)) in attempts.iter().enumerate() {
            let last = n + 1 == attempts.len();
            if rename {
                request["model"] = Value::String((*model).clone());
            }
            let mut outgoing = self
                .client
                .post(format!(
                    "{}{}",
                    upstream.base_url.trim_end_matches('/'),
                    path
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(request.to_string());
            for name in ["anthropic-version", "anthropic-beta"] {
                if let Some(value) = headers.get(name) {
                    outgoing = outgoing.header(name, value);
                }
            }
            let outgoing = match self.authenticate(upstream, outgoing).await {
                Ok(outgoing) => outgoing,
                Err(message) => {
                    failure = error(
                        protocol,
                        StatusCode::BAD_GATEWAY,
                        "authentication_error",
                        &message,
                    );
                    continue;
                }
            };
            let started = Instant::now();
            match outgoing.send().await {
                Ok(response) if last || !falls_back(response.status()) => {
                    return Ok(Answered {
                        response,
                        model: (*model).clone(),
                        started,
                    });
                }
                // The failed answer is dropped unread; nothing of it reached the harness.
                Ok(_) => {}
                Err(e) => {
                    failure = error(
                        protocol,
                        StatusCode::BAD_GATEWAY,
                        "api_error",
                        &format!("upstream: {e}"),
                    );
                }
            }
        }
        Err(Box::new(failure))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::Router as Routes;
    use axum::response::IntoResponse;
    use axum::routing::post;
    use serde_json::json;

    use super::*;
    use crate::router::Grant;
    use crate::store::Store;

    /// A fake upstream that answers with `status` (and a stream for a streamed request that succeeds), noting the
    /// model of each request it gets.
    async fn upstream(status: u16, seen: Arc<Mutex<Vec<String>>>) -> String {
        let app = Routes::new().route(
            "/v1/chat/completions",
            post(move |axum::Json(body): axum::Json<Value>| async move {
                let model = body["model"].as_str().unwrap_or_default().to_string();
                seen.lock().unwrap().push(model.clone());
                let status = StatusCode::from_u16(status).unwrap();
                if !status.is_success() {
                    return (status, axum::Json(json!({"error": {"message": "down"}})))
                        .into_response();
                }
                axum::Json(json!({
                    "model": format!("{model}-2026"),
                    "choices": [{"message": {"role": "assistant", "content": "Hi"}}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 5}
                }))
                .into_response()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}")
    }

    fn openai(base_url: &str) -> Upstream {
        Upstream {
            protocol: Protocol::OpenAi,
            base_url: base_url.to_string(),
            api_key: None,
            oauth: None,
        }
    }

    struct Setup {
        _dir: tempfile::TempDir,
        store: Arc<Store>,
        router: Arc<Router>,
        url: String,
        token: String,
        run: i64,
    }

    async fn setup(upstreams: Vec<Upstream>, classes: Classes) -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let run = store.add_run(None, "Coder", "pi-bolt", "code").unwrap().id;
        let router = Arc::new(Router::new(store.clone(), upstreams).with_classes(classes));
        let token = router.issue_token(Grant {
            run_id: run,
            card_id: None,
            budget: None,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let routes = router.clone().routes();
        tokio::spawn(async move { axum::serve(listener, routes).await });
        Setup {
            _dir: dir,
            store,
            router,
            url,
            token,
            run,
        }
    }

    impl Setup {
        async fn ask(&self, model: &str) -> reqwest::Response {
            reqwest::Client::new()
                .post(format!("{}/v1/chat/completions", self.url))
                .bearer_auth(&self.token)
                .json(&json!({"model": model, "messages": []}))
                .send()
                .await
                .unwrap()
        }

        /// The model and tokens of each usage row of the run, and the class it was asked by, if any.
        fn usage(&self) -> Vec<(String, Option<String>, i64)> {
            let classes = self.store.usage_by_class(1).unwrap();
            self.store
                .usage_for_run(self.run)
                .unwrap()
                .into_iter()
                .map(|u| {
                    let class = classes
                        .iter()
                        .find(|c| c.model == u.model)
                        .map(|c| c.class.clone());
                    (u.model.clone(), class, u.total())
                })
                .collect()
        }
    }

    fn class(models: &[(&str, Option<i64>)]) -> Vec<ClassModel> {
        models
            .iter()
            .map(|(model, day_tokens)| ClassModel {
                model: model.to_string(),
                day_tokens: *day_tokens,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_class_falls_back_from_a_failing_upstream_and_is_recorded_under_the_model_that_answered()
     {
        let (failing_seen, working_seen) = (Arc::default(), Arc::default());
        let failing = upstream(503, Arc::clone(&failing_seen)).await;
        let working = upstream(200, Arc::clone(&working_seen)).await;
        // The first upstream fails for every model; the second serves them.
        let s = setup(
            vec![openai(&failing), openai(&working)],
            Classes::from([(
                "strong".to_string(),
                class(&[("big", None), ("small", None)]),
            )]),
        )
        .await;

        let reply = s.ask("strong").await;
        assert_eq!(reply.status(), 200);
        let body: Value = reply.json().await.unwrap();
        assert_eq!(body["choices"][0]["message"]["content"], "Hi");
        assert_eq!(*failing_seen.lock().unwrap(), ["big"]);
        assert_eq!(*working_seen.lock().unwrap(), ["big"]);
        // Recorded under the class's model, not the name the upstream answers with, with the class beside it.
        assert_eq!(
            s.usage(),
            [("big".to_string(), Some("strong".to_string()), 15)]
        );

        // A model named directly falls back over the upstreams too, and keeps the name the upstream answers with.
        assert_eq!(s.ask("small").await.status(), 200);
        assert_eq!(s.usage()[1], ("small-2026".to_string(), None, 15));
    }

    #[tokio::test]
    async fn the_next_model_is_asked_when_every_upstream_fails_and_the_last_failure_is_passed_on() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let refusing = upstream(429, Arc::clone(&seen)).await;
        let s = setup(
            vec![openai(&refusing)],
            Classes::from([("fast".to_string(), class(&[("a", None), ("b", None)]))]),
        )
        .await;
        let reply = s.ask("fast").await;
        // Both models were asked, in order; the harness gets the last upstream's answer.
        assert_eq!(*seen.lock().unwrap(), ["a", "b"]);
        assert_eq!(reply.status(), 429);
        assert!(s.usage().is_empty());

        // An upstream that cannot be reached fails before it answers too.
        let seen = Arc::new(Mutex::new(Vec::new()));
        let working = upstream(200, Arc::clone(&seen)).await;
        let closed = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", listener.local_addr().unwrap())
        };
        s.router
            .set_upstreams(vec![openai(&closed), openai(&working)]);
        assert_eq!(s.ask("fast").await.status(), 200);
        assert_eq!(*seen.lock().unwrap(), ["a"]);
        s.router.set_upstreams(vec![openai(&closed)]);
        let reply = s.ask("fast").await;
        assert_eq!(reply.status(), 502);
    }

    #[tokio::test]
    async fn a_model_whose_tokens_for_today_are_spent_is_passed_over() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let working = upstream(200, Arc::clone(&seen)).await;
        let s = setup(
            vec![openai(&working)],
            Classes::from([(
                "strong".to_string(),
                class(&[("big", Some(20)), ("small", None)]),
            )]),
        )
        .await;
        // 15 of big's 20 tokens are used; then 30; then small takes the requests.
        for _ in 0..3 {
            assert_eq!(s.ask("strong").await.status(), 200);
        }
        assert_eq!(*seen.lock().unwrap(), ["big", "big", "small"]);

        s.router.set_classes(Classes::from([(
            "strong".to_string(),
            class(&[("big", Some(20))]),
        )]));
        let refused = s.ask("strong").await;
        assert_eq!(refused.status(), 429);
        let body: Value = refused.json().await.unwrap();
        assert_eq!(body["error"]["type"], "budget_exceeded");
        assert_eq!(seen.lock().unwrap().len(), 3);
    }

    #[test]
    fn a_class_model_is_a_name_or_a_table() {
        let classes: Classes =
            toml::from_str("fast = [\"qwen-local\", { model = \"haiku\", day_tokens = 500 }]\n")
                .unwrap();
        assert_eq!(
            classes["fast"],
            class(&[("qwen-local", None), ("haiku", Some(500))])
        );
        assert!(check_class("fast", &classes["fast"]).is_ok());
        assert!(toml::from_str::<Classes>("fast = [{ model = \"a\", tokens = 1 }]").is_err());
        assert!(check_class("fast", &[]).is_err());
        assert!(check_class("a b", &classes["fast"]).is_err());
        assert!(check_class("fast", &class(&[("a", Some(0))])).is_err());
        assert!(check_class("fast", &class(&[(" ", None)])).is_err());
    }
}
