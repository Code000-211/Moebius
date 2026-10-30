//! The models the upstreams serve, asked of all of them at once for the pickers of the desktop app and kept for a
//! while, without a key or a password in the answer, and the model classes the router serves besides them.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Protocol, Router, Upstream};
use crate::server::setup::without_credentials;

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
        let asked = futures_util::future::join_all(upstreams.iter().map(|u| self.list_models(u)));
        let mut answer = AvailableModels::default();
        for (upstream, result) in upstreams.iter().zip(asked.await) {
            match result {
                Ok(ids) => answer
                    .models
                    .extend(ids.into_iter().map(|id| AvailableModel {
                        id,
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
        self.list_models(&Upstream {
            protocol: Protocol::OpenAi,
            base_url: base_url.to_string(),
            api_key: key,
            oauth: None,
        })
        .await
    }

    /// The ids of the models one upstream lists, or a short message saying why not.
    pub(super) async fn list_models(&self, upstream: &Upstream) -> Result<Vec<String>, String> {
        if let Some(needs) = upstream.oauth.as_ref().and_then(|oauth| oauth.needs()) {
            return Err(format!("needs {needs}"));
        }
        if let Some(oauth) = upstream.oauth.as_ref().filter(|oauth| oauth.codex()) {
            // The backend has no list to ask for: the models are the ones its subscription serves.
            self.access_token(oauth).await?;
            return Ok(super::responses::MODELS
                .iter()
                .map(|id| id.to_string())
                .collect());
        }
        let base = upstream.base_url.trim_end_matches('/');
        let request = match upstream.protocol {
            Protocol::OpenAi => self.client.get(format!("{base}/models")),
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
            .filter_map(|m| m["id"].as_str().map(str::to_string))
            .collect())
    }
}
