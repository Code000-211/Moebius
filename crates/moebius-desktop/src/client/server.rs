//! What the server runs with, as the desktop API has it: its health, models and upstreams, plugins, the steps to a
//! first card, and the settings of `moebius.toml`.

use serde::Deserialize;

use super::harness::Agent;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Health {
    pub version: String,
}

/// An upstream of the router, as `GET /v1/models` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Upstream {
    pub protocol: String,
    pub base_url: String,
    pub api_key_set: bool,
}

/// The teacher model, as `GET /v1/models` tells it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Teacher {
    pub model: String,
    pub protocol: String,
    pub timeout_seconds: u64,
}

/// The decision model's endpoint, as `GET /v1/models` tells it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DecisionModelEndpoint {
    pub url: String,
    pub model: String,
    pub timeout_seconds: u64,
    pub api_key_set: bool,
}

/// What `GET /v1/models` says: the models the server uses and where it reaches them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Models {
    pub upstreams: Vec<Upstream>,
    pub agents: Vec<Agent>,
    pub teacher: Option<Teacher>,
    pub decision_model: Option<DecisionModelEndpoint>,
}

/// A model an upstream serves, as `GET /v1/models/available` lists it. It reads as its id.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AvailableModel {
    pub id: String,
    pub protocol: String,
    pub upstream: String,
}

impl std::fmt::Display for AvailableModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.id)
    }
}

/// An upstream that could not say which models it serves.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UpstreamError {
    pub upstream: String,
    pub message: String,
}

/// What `GET /v1/models/available` says: the models of all upstreams and the upstreams that could not be asked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Available {
    pub models: Vec<AvailableModel>,
    pub errors: Vec<UpstreamError>,
}

/// A plugin the server has loaded, as `GET /v1/plugins` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub kinds: Vec<String>,
    pub decides: Vec<String>,
}

/// What `GET /v1/plugins` says.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Plugins {
    pub time_limit_seconds: u64,
    pub plugins: Vec<Plugin>,
}

/// One thing to set up before the first card, as `GET /v1/start` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StartStep {
    /// `upstream`, `pi_bolt`, `harness`, `coder` or `repository`.
    pub key: String,
    pub title: String,
    pub done: bool,
    pub detail: String,
}

/// What `GET /v1/start` says.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Start {
    pub done: bool,
    pub steps: Vec<StartStep>,
}

/// One key of `moebius.toml` as `GET /v1/settings` tells it: what the file says, or the default.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Setting {
    pub value: serde_json::Value,
    /// True when the file does not set the key and `value` is the default.
    pub default: bool,
}

/// What `GET /v1/settings` says: every key of the configuration, the stored keys' names and what needs a restart.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ServerSettings {
    /// The configuration file on the server.
    pub file: String,
    pub values: std::collections::BTreeMap<String, Setting>,
    /// The names of the stored API keys, never the keys.
    #[serde(default)]
    pub secrets: Vec<String>,
    #[serde(default)]
    pub restart_needed: bool,
    #[serde(default)]
    pub restart_keys: Vec<String>,
}

impl ServerSettings {
    /// The setting `key`, or null when the server did not tell it.
    pub fn value(&self, key: &str) -> &serde_json::Value {
        self.values
            .get(key)
            .map_or(&serde_json::Value::Null, |setting| &setting.value)
    }

    /// Whether the file sets `key`; the default is in use when it does not.
    pub fn is_set(&self, key: &str) -> bool {
        self.values.get(key).is_some_and(|setting| !setting.default)
    }
}

/// What `PUT /v1/settings` and `POST /v1/harness/setup` answer: whether some changed key is used only after a restart.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SavedSettings {
    pub restart_needed: bool,
    #[serde(default)]
    pub restart_keys: Vec<String>,
}
