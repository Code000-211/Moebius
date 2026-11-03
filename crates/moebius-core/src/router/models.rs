//! The models the upstreams serve, asked of all of them at once for the pickers of the desktop app and kept for a
//! while, without a key or a password in the answer, the model classes the router serves besides them, and the
//! reasoning level names of each model: the names its endpoint says it takes, or the ones `[router.model_thinking]`
//! gives it when it does not say. A run's `models.json` is given the name to send for every level whose own name the
//! endpoint does not take, so that its harness can ask for a level the endpoint would not take: `xhigh` of an endpoint
//! whose hardest level is named so, or the hardest an endpoint has when it has no name for a level at all.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Protocol, Router, Upstream};
use crate::config::THINKING_LEVELS;
use crate::server::setup::without_credentials;

/// The reasoning level names by model: for each reasoning level of a model, the value its endpoint takes.
pub type ModelThinking = BTreeMap<String, BTreeMap<String, String>>;

/// How long one upstream has to list its models.
const MODELS_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a list of models is kept before the upstreams are asked again.
const MODELS_KEPT: Duration = Duration::from_secs(5 * 60);

/// One model an upstream serves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableModel {
    pub id: String,
    pub protocol: Protocol,
    /// The base URL of the upstream that serves it.
    pub upstream: String,
    /// The reasoning level names its endpoint said it takes, easiest first; empty when the upstream does not say,
    /// which is what an Anthropic endpoint and most OpenAI ones do.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<String>,
}

/// A model an upstream listed, with the reasoning level names its endpoint takes.
struct Listed {
    id: String,
    levels: Vec<String>,
}

/// An upstream that could not say which models it serves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamError {
    pub upstream: String,
    pub message: String,
}

/// A model class of `[router.classes]`, which a request may name as a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableClass {
    pub name: String,
    /// Its models, in the order they are tried.
    pub models: Vec<String>,
    /// The protocols the upstreams list any of its models under.
    pub protocols: Vec<Protocol>,
}

/// The models of all upstreams, sorted by protocol and then id, the upstreams that could not be asked, and the model
/// classes by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableModels {
    pub models: Vec<AvailableModel>,
    pub errors: Vec<UpstreamError>,
    #[serde(default)]
    pub classes: Vec<AvailableClass>,
    /// The models the decision model's endpoint lists, when they were asked for.
    #[serde(default)]
    pub decision_models: Vec<String>,
}

impl Router {
    /// Takes the reasoning level names of the models that need them, for a router being built.
    pub fn with_model_thinking(self, maps: ModelThinking) -> Self {
        self.set_model_thinking(maps);
        self
    }

    /// Replaces the reasoning level names; the next run of a model is written with the new ones.
    pub fn set_model_thinking(&self, maps: ModelThinking) {
        *self
            .model_thinking
            .write()
            .unwrap_or_else(|p| p.into_inner()) = maps;
    }

    /// The reasoning level names `model`'s endpoint was last heard taking, from the list kept for the pickers; empty
    /// before the upstreams have been asked, and for an endpoint that does not name them.
    pub fn model_levels(&self, model: &str) -> Vec<String> {
        let kept = self
            .available
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        kept.and_then(|(_, listed)| {
            listed
                .models
                .into_iter()
                .find(|m| m.id == model)
                .map(|m| m.levels)
        })
        .unwrap_or_default()
    }

    /// The value to send for each reasoning level of `model`: what `[router.model_thinking]` gives it when it names
    /// the model, or the names its own endpoint lists; none when neither is known, or when it takes the levels' own
    /// names.
    pub fn thinking_map(&self, model: &str) -> Option<BTreeMap<String, String>> {
        let given = self
            .model_thinking
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(model)
            .cloned();
        given.or_else(|| {
            let map = thinking_map_of(&self.model_levels(model));
            (!map.is_empty()).then_some(map)
        })
    }

    /// The models the upstreams serve, asked of all of them at once. The answer is kept for five minutes unless
    /// `refresh` is set. An upstream that fails is listed in `errors`; no key is ever in the answer. The classes are
    /// the router's at the time of asking.
    pub async fn available_models(&self, refresh: bool) -> AvailableModels {
        let mut answer = self.listed_models(refresh).await;
        answer.classes = self
            .classes()
            .into_iter()
            .map(|(name, models)| {
                let models: Vec<String> = models.into_iter().map(|m| m.model).collect();
                let mut protocols = Vec::new();
                for listed in answer.models.iter().filter(|m| models.contains(&m.id)) {
                    if !protocols.contains(&listed.protocol) {
                        protocols.push(listed.protocol);
                    }
                }
                AvailableClass {
                    name,
                    models,
                    protocols,
                }
            })
            .collect();
        answer
    }

    /// The models the upstreams list, kept for five minutes unless `refresh` is set.
    async fn listed_models(&self, refresh: bool) -> AvailableModels {
        if !refresh {
            let kept = self.available.lock().unwrap_or_else(|p| p.into_inner());
            if let Some((at, models)) = kept.as_ref()
                && at.elapsed() < MODELS_KEPT
            {
                return models.clone();
            }
        }
        let mut upstreams = self
            .upstreams
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let mut seen = Vec::new();
        upstreams.retain(|u| {
            let new = !seen.contains(u);
            seen.push(u.clone());
            new
        });
        let asked = futures_util::future::join_all(
            upstreams.iter().map(|u| self.listed_at(u, "/v1/models")),
        );
        let mut answer = AvailableModels::default();
        for (upstream, result) in upstreams.iter().zip(asked.await) {
            match result {
                Ok(listed) => answer
                    .models
                    .extend(listed.into_iter().map(|m| AvailableModel {
                        id: m.id,
                        levels: m.levels,
                        protocol: upstream.protocol,
                        upstream: without_credentials(&upstream.base_url),
                    })),
                Err(message) => answer.errors.push(UpstreamError {
                    upstream: without_credentials(&upstream.base_url),
                    message,
                }),
            }
        }
        answer.models.sort_by(|a, b| {
            (a.protocol.name(), &a.id, &a.upstream).cmp(&(b.protocol.name(), &b.id, &b.upstream))
        });
        answer.models.dedup();
        answer.errors.dedup();
        *self.available.lock().unwrap_or_else(|p| p.into_inner()) =
            Some((Instant::now(), answer.clone()));
        answer
    }

    /// The ids of the models an OpenAI-compatible endpoint outside the router lists, such as the decision model's,
    /// asked with `key` and never kept.
    pub async fn endpoint_models(
        &self,
        base_url: &str,
        key: Option<String>,
    ) -> Result<Vec<String>, String> {
        Ok(self
            .listed_at(
                &Upstream {
                    protocol: Protocol::OpenAi,
                    base_url: base_url.to_string(),
                    api_key: key,
                    oauth: None,
                },
                "/models",
            )
            .await?
            .into_iter()
            .map(|listed| listed.id)
            .collect())
    }

    /// The ids of the models one upstream lists, or a short message saying why not.
    pub(super) async fn list_models(&self, upstream: &Upstream) -> Result<Vec<String>, String> {
        Ok(self
            .listed_at(upstream, "/v1/models")
            .await?
            .into_iter()
            .map(|listed| listed.id)
            .collect())
    }

    /// [`Router::list_models`], where an `openai` upstream's list is at `openai_path` after its base url: a router
    /// upstream's base has no `/v1`, the decision model's url is asked at `/models` whatever it ends in.
    async fn listed_at(
        &self,
        upstream: &Upstream,
        openai_path: &str,
    ) -> Result<Vec<Listed>, String> {
        if let Some(needs) = upstream.oauth.as_ref().and_then(|oauth| oauth.needs()) {
            return Err(format!("needs {needs}"));
        }
        if let Some(oauth) = upstream.oauth.as_ref().filter(|oauth| oauth.codex()) {
            // The backend has no list to ask for: the models are the ones its subscription serves.
            self.access_token(oauth).await?;
            return Ok(super::responses::MODELS
                .iter()
                .map(|id| Listed {
                    id: (*id).to_string(),
                    // The subscription's own models: Pi-Bolt knows their levels, and the backend is not asked.
                    levels: Vec::new(),
                })
                .collect());
        }
        let base = upstream.base_url.trim_end_matches('/');
        let request = match upstream.protocol {
            Protocol::OpenAi => self.client.get(format!("{base}{openai_path}")),
            // The default page of Anthropic's list is twenty models.
            Protocol::Anthropic => self
                .client
                .get(format!("{base}/v1/models?limit=1000"))
                .header("anthropic-version", "2023-06-01"),
        }
        .timeout(MODELS_TIMEOUT);
        let request = self.authenticate(upstream, request).await?;
        let request = match upstream.oauth.as_ref().and_then(|oauth| oauth.preset()) {
            Some(preset) => preset.dress(request, None),
            None => request,
        };
        // The messages are fixed words, so that neither the key nor the address can be in them.
        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                "timed out".to_string()
            } else if e.is_connect() {
                "could not connect".to_string()
            } else {
                "request failed".to_string()
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(status.as_u16().to_string());
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "the answer is not JSON".to_string())?;
        let data = body["data"]
            .as_array()
            .ok_or_else(|| "the answer has no model list".to_string())?;
        Ok(data
            .iter()
            .filter_map(|model| {
                let id = model["id"].as_str()?.to_string();
                // An endpoint of this server's own kind names the reasoning levels its chat template takes.
                let levels = model["capabilities"]["reasoning_efforts"]
                    .as_array()
                    .map(|named| {
                        named
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                Some(Listed { id, levels })
            })
            .collect())
    }
}

/// The name to send to an endpoint that takes `levels`, for each reasoning level whose own name the endpoint does not
/// take: the name at that level's place in the endpoint's list, or the last of the list when it is shorter, so that a
/// level harder than any the endpoint names asks for the hardest it has. Nothing is given for a level the endpoint
/// takes by its own name, nor for any level of an endpoint that lists none; the harness sends those names themselves.
fn thinking_map_of(levels: &[String]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Some(last) = levels.last() else {
        return map;
    };
    for (place, level) in THINKING_LEVELS.iter().enumerate() {
        if levels.iter().any(|named| named == level) {
            continue;
        }
        let takes = levels.get(place).unwrap_or(last);
        map.insert((*level).to_string(), takes.clone());
    }
    map
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::store::Store;

    /// An OpenAI upstream whose one model names the reasoning levels its endpoint takes.
    async fn upstream_that(levels: Value) -> (Router, Arc<Store>, tempfile::TempDir) {
        let answers = json!({
            "data": [{ "id": "one-model", "capabilities": { "reasoning_efforts": levels } }]
        });
        let app = axum::Router::new().route(
            "/v1/models",
            axum::routing::get(move || {
                let answers = answers.clone();
                async move { axum::Json(answers) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let router = Router::new(
            store.clone(),
            vec![Upstream {
                protocol: Protocol::OpenAi,
                base_url: base,
                api_key: None,
                oauth: None,
            }],
        );
        (router, store, dir)
    }

    #[tokio::test]
    async fn the_levels_an_endpoint_lists_are_kept_and_send_their_names() {
        let (router, _store, _dir) =
            upstream_that(json!(["none", "minimal", "low", "medium", "xhigh"])).await;
        let listed = router.available_models(true).await;
        assert_eq!(
            listed.models[0].levels,
            ["none", "minimal", "low", "medium", "xhigh"]
        );
        assert_eq!(router.model_levels("one-model"), listed.models[0].levels);
        // The endpoint has no name of its own for these three, so each takes the name at its place, or the hardest.
        assert_eq!(
            router.thinking_map("one-model").unwrap(),
            [
                ("off".to_string(), "none".to_string()),
                ("high".to_string(), "xhigh".to_string()),
                ("max".to_string(), "xhigh".to_string()),
            ]
            .into_iter()
            .collect::<BTreeMap<String, String>>()
        );
        assert_eq!(router.thinking_map("another-model"), None);
    }

    #[tokio::test]
    async fn an_endpoint_that_takes_the_level_names_is_given_no_map() {
        let (router, _store, _dir) = upstream_that(json!([
            "off", "minimal", "low", "medium", "high", "xhigh", "max"
        ]))
        .await;
        router.available_models(true).await;
        assert_eq!(router.thinking_map("one-model"), None);
    }

    #[tokio::test]
    async fn an_endpoint_that_does_not_say_is_given_no_map() {
        let (router, _store, _dir) = upstream_that(Value::Null).await;
        router.available_models(true).await;
        assert!(router.model_levels("one-model").is_empty());
        assert_eq!(router.thinking_map("one-model"), None);
    }

    #[tokio::test]
    async fn the_names_written_in_the_configuration_win() {
        let (router, _store, _dir) = upstream_that(json!(["low", "medium"])).await;
        router.set_model_thinking(
            [(
                "one-model".to_string(),
                [("high".to_string(), "medium".to_string())]
                    .into_iter()
                    .collect(),
            )]
            .into_iter()
            .collect(),
        );
        router.available_models(true).await;
        assert_eq!(
            router.thinking_map("one-model").unwrap(),
            [("high".to_string(), "medium".to_string())]
                .into_iter()
                .collect::<BTreeMap<String, String>>()
        );
    }

    #[test]
    fn a_shorter_list_than_the_levels_sends_its_hardest_for_everything_above() {
        let levels = ["low", "high"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<String>>();
        let map = thinking_map_of(&levels);
        assert_eq!(map["off"], "low");
        assert_eq!(map["minimal"], "high");
        assert!(!map.contains_key("high"), "it takes the name itself");
        assert_eq!(map["xhigh"], "high");
        assert_eq!(map["max"], "high");
    }

    #[tokio::test]
    async fn the_decision_models_url_is_asked_at_models_whatever_it_ends_in() {
        let app = axum::Router::new().route(
            "/openai/models",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!({"data": [{"id": "m1"}]}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/openai", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let dir = tempfile::tempdir().unwrap();
        let router = Router::new(Arc::new(Store::in_memory(dir.path()).unwrap()), Vec::new());
        assert_eq!(router.endpoint_models(&url, None).await.unwrap(), ["m1"]);
    }
}
