//! The router (ADR 0006): Moebius is every harness's only model provider. It offers OpenAI- and Anthropic-compatible
//! endpoints on localhost, knows each request's run by its per-run token, forwards it to an upstream (a provider with an
//! API key, or a local server such as Radiance), records the tokens used against the run, and refuses requests once the
//! card's or run's token budget, or the day's, is spent. Provider keys stay in the router; harnesses only ever see their run's token.
//!
//! - `POST /v1/chat/completions`: OpenAI chat completions, streamed or not.
//! - `POST /v1/messages`: Anthropic messages, streamed or not.
//!
//! While it reads an answer to count its tokens, the router also notes the names of the tools the answer asks for, so
//! that the Usage page can say what the model's time went on.
//!
//! The router can also ask its upstreams which models they serve ([`Router::available_models`]), for the pickers of the
//! desktop app.
//!
//! The server serves these routes together with the tool-call check (`check`) and the MCP gateway (`gateway`) on one
//! listener. A run's token also names the MCP servers the gateway gives the run.
//!
//! This module holds the router, its upstreams and its routes; `proxy` forwards a request upstream, `usage` counts
//! the tokens and tools of what comes back, `grants` are the runs' tokens and the budgets they are checked against,
//! and `models` asks the upstreams which models they serve.

mod grants;
mod models;
mod proxy;
mod usage;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use axum::Router as Routes;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::harness::mcp::McpServer;
use crate::store::Store;

pub use grants::{Grant, token};
pub use models::{AvailableModel, AvailableModels, UpstreamError};

/// The protocol an endpoint or an upstream speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[serde(rename = "openai")]
    OpenAi,
    Anthropic,
}

impl Protocol {
    /// The name the configuration and the API give it.
    fn name(self) -> &'static str {
        match self {
            Protocol::OpenAi => "openai",
            Protocol::Anthropic => "anthropic",
        }
    }
}

/// Where requests of one protocol go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upstream {
    pub protocol: Protocol,
    /// The base URL the endpoint's path is appended to, such as `https://api.anthropic.com` or `http://127.0.0.1:8080`.
    pub base_url: String,
    /// The provider's key; none for a local server.
    pub api_key: Option<String>,
}

pub struct Router {
    store: Arc<Store>,
    /// Replaced when the upstreams are edited while the server runs.
    upstreams: RwLock<Vec<Upstream>>,
    /// The most tokens all runs may use in a day, if limited; replaced when the budget is edited.
    day_budget: RwLock<Option<i64>>,
    grants: RwLock<HashMap<String, Grant>>,
    /// The MCP servers each token's run may use through the MCP gateway, with their values filled in.
    mcp: RwLock<HashMap<String, BTreeMap<String, McpServer>>>,
    client: reqwest::Client,
    /// The last list of available models and when it was made.
    available: Mutex<Option<(Instant, AvailableModels)>>,
}

impl Router {
    pub fn new(store: Arc<Store>, upstreams: Vec<Upstream>) -> Self {
        Self {
            store,
            upstreams: RwLock::new(upstreams),
            day_budget: RwLock::new(None),
            grants: RwLock::new(HashMap::new()),
            mcp: RwLock::new(HashMap::new()),
            client: reqwest::Client::new(),
            available: Mutex::new(None),
        }
    }

    /// Replaces the upstreams; the next request goes to the new ones.
    pub fn set_upstreams(&self, upstreams: Vec<Upstream>) {
        *self.upstreams.write().unwrap_or_else(|p| p.into_inner()) = upstreams;
        *self.available.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    pub fn routes(self: Arc<Self>) -> Routes {
        Routes::new()
            .route(
                "/v1/chat/completions",
                post(
                    |State(r): State<Arc<Router>>, headers: HeaderMap, body: Bytes| async move {
                        r.forward(Protocol::OpenAi, "/v1/chat/completions", headers, body)
                            .await
                    },
                ),
            )
            .route(
                "/v1/messages",
                post(
                    |State(r): State<Arc<Router>>, headers: HeaderMap, body: Bytes| async move {
                        r.forward(Protocol::Anthropic, "/v1/messages", headers, body)
                            .await
                    },
                ),
            )
            .with_state(self)
    }
}

/// An error in the shape the protocol's clients expect.
fn error(protocol: Protocol, status: StatusCode, kind: &str, message: &str) -> Response {
    let body = match protocol {
        Protocol::OpenAi => json!({ "error": { "type": kind, "message": message } }),
        Protocol::Anthropic => {
            json!({ "type": "error", "error": { "type": kind, "message": message } })
        }
    };
    (status, axum::Json(body)).into_response()
}
