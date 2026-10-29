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
    /// How the upstream logs in with OAuth, if it does instead of a key.
    #[serde(default)]
    pub oauth: Option<UpstreamOAuth>,
}

/// The OAuth login of an upstream, as `GET /v1/models` tells it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UpstreamOAuth {
    /// `device` or `code`.
    pub flow: String,
    /// Whether the server keeps a login for it.
    pub logged_in: bool,
    /// What the person confirms before a login starts.
    pub warning: String,
}

/// Where the OAuth login of an upstream stands, as `GET` and `POST /v1/models/upstreams/{place}/login` answer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Login {
    /// `logged_out`, `waiting`, `logged_in` or `failed`.
    pub state: String,
    pub flow: String,
    pub user_code: Option<String>,
    pub verification_uri: Option<String>,
    pub verification_uri_complete: Option<String>,
    pub authorize_url: Option<String>,
    pub message: String,
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

/// A model an upstream serves, as `GET /v1/models/available` lists it, or a model class of the router. It reads as its
/// id.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AvailableModel {
    pub id: String,
    pub protocol: String,
    pub upstream: String,
    /// Whether this is a model class (see [`AvailableClass`]) rather than a model.
    #[serde(default)]
    pub class: bool,
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

/// A model class of the router: a name for its models, tried in order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AvailableClass {
    pub name: String,
    pub models: Vec<String>,
    /// The protocols the upstreams list any of its models under.
    pub protocols: Vec<String>,
}

/// What `GET /v1/models/available` says: the models of all upstreams, the upstreams that could not be asked, and the
/// model classes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Available {
    pub models: Vec<AvailableModel>,
    pub errors: Vec<UpstreamError>,
    #[serde(default)]
    pub classes: Vec<AvailableClass>,
}

impl Available {
    /// What a Model field offers: the classes first, each with the protocol of its models when they share one, then
    /// the models.
    pub fn choices(&self) -> Vec<AvailableModel> {
        self.classes
            .iter()
            .map(|class| AvailableModel {
                id: class.name.clone(),
                protocol: match class.protocols.as_slice() {
                    [one] => one.clone(),
                    _ => String::new(),
                },
                upstream: String::new(),
                class: true,
            })
            .chain(self.models.iter().cloned())
            .collect()
    }
}

/// A plugin the server has loaded, as `GET /v1/plugins` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub kinds: Vec<String>,
    pub decides: Vec<String>,
    /// How it runs when it is a plugin in its own process; none for one compiled in.
    #[serde(default)]
    pub process: Option<PluginProcess>,
}

/// A plugin in its own process, as `GET /v1/plugins` tells it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PluginProcess {
    pub program: String,
    /// `starting`, `running`, `restarting` or `stopped`.
    pub state: String,
    pub restarts: u64,
    /// Why it stopped the last time.
    pub error: Option<String>,
    /// The file its standard error goes to.
    pub log: String,
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

/// A forge's webhook as `GET /v1/webhooks` tells it: where the forge sends deliveries and the secret it signs them with.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Webhook {
    /// `github`, `gitlab` or `forgejo`.
    pub forge: String,
    /// The path on the server that the forge is told to send to.
    pub path: String,
    /// The header in which the forge proves a delivery.
    pub header: String,
    /// The secret to set on the forge; none until one is made.
    pub secret: Option<String>,
}

// Not derived, so that the secret cannot be printed by accident.
impl std::fmt::Debug for Webhook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Webhook")
            .field("forge", &self.forge)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}
