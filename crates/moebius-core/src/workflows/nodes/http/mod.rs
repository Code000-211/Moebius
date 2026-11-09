//! The `http` node: one HTTP request, made by the server, because the sandbox has no network. The URL must pass the
//! URL policy (only hosts named in the server configuration), a credential is named and never written in the
//! workflow, and a refusal or a failure fails the node closed. The parameters are read when the node runs, so the
//! URL, a header value and the body may hold expressions the run fills.
//!
//! What the node is given (`params`), what the URL policy allows (`policy`), and the request the node makes and the
//! answer it reads (`call`) are kept apart, each with the tests for its part.

mod call;
mod params;
mod policy;

use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde_json::Value as Json;

use super::{Metadata, Node, NodeError, Ports, Routed};
use crate::config::Config;
use crate::server::settings::Secrets;
use crate::workflows::decorate::{self, Scope};

/// The kind name a workflow file writes.
pub const KIND: &str = "http";

/// How long a request may take when the node does not say.
const DEFAULT_TIMEOUT_SECONDS: f64 = 30.0;
/// The longest a node may ask a request to take.
const MAX_TIMEOUT_SECONDS: f64 = 300.0;
/// The most bytes of a response body that are read; a longer one fails the node.
const MAX_BODY_BYTES: u64 = 10 * 1024 * 1024;
/// The text written before a named secret's value, in the `Authorization` header.
const SECRET_PREFIX: &str = "Bearer ";
const METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];

/// What the server configuration says about the node: the hosts it may call and where the named secrets are kept.
#[derive(Debug, Clone)]
struct Settings {
    hosts: Vec<String>,
    secrets: Secrets,
}

impl Settings {
    fn from_config(config: &Config) -> Self {
        Self {
            hosts: config.workflow_http_hosts.clone(),
            secrets: Secrets::new(&config.data_dir),
        }
    }
}

static SETTINGS: RwLock<Option<Arc<Settings>>> = RwLock::new(None);

/// Hands the node the server's configuration, at start. Until then, and for a server that names no host, every call is
/// refused.
pub fn configure(config: &Config) {
    *SETTINGS.write().unwrap_or_else(|p| p.into_inner()) =
        Some(Arc::new(Settings::from_config(config)));
}

fn configured() -> Option<Arc<Settings>> {
    SETTINGS.read().unwrap_or_else(|p| p.into_inner()).clone()
}

/// A node that makes one HTTP request and outputs `{status, headers, body}`. Its parameters are kept as the workflow
/// wrote them, so the expressions they carry are filled from the run when the node runs.
#[derive(Debug, Clone)]
pub struct HttpNode {
    params: Json,
    settings: Option<Arc<Settings>>,
}

/// The request the node makes: its parameters read after the run filled their expressions.
#[derive(Debug, Clone)]
struct Request {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Json>,
    timeout: Duration,
    secret: Option<String>,
    settings: Option<Arc<Settings>>,
}

impl HttpNode {
    /// Builds the node from the parameters as the workflow wrote them, with the server's settings.
    pub fn from_params(params: &Json) -> Result<Self, NodeError> {
        Self::with_settings(params, configured())
    }

    fn with_settings(params: &Json, settings: Option<Arc<Settings>>) -> Result<Self, NodeError> {
        // Read once here with the expressions left standing, so a file naming an unknown parameter, a header that is
        // not a string or a method the node does not have is refused when it is read. The node reads its parameters
        // again once the run has filled them, and refuses what only an expression could produce then.
        Request::parse(params, None, true)?;
        Ok(Self {
            params: params.clone(),
            settings,
        })
    }
}

impl Node for HttpNode {
    /// Makes the request with only the value that reached the node to fill its expressions; the executor runs the
    /// node through [`Node::run_routed`], which also has the trigger and the earlier nodes.
    fn run(&self, input: Json) -> Result<Json, NodeError> {
        let scope = Scope {
            trigger: Json::Null,
            input,
            nodes: Default::default(),
        };
        Ok(self.run_routed(&scope)?.value)
    }

    /// Fills the expressions the parameters hold from the run, reads them, and makes the request.
    fn run_routed(&self, scope: &Scope) -> Result<Routed, NodeError> {
        let filled = decorate::fill(KIND, &self.params, scope).map_err(|error| {
            NodeError::Failed(format!(
                "parameter {:?}: {}",
                error.parameter, error.message
            ))
        })?;
        let request = Request::parse(&filled, self.settings.clone(), false)?;
        // The request runs on a thread of its own, since the blocking client must not start inside a runtime.
        let value = std::thread::scope(|thread| {
            thread
                .spawn(|| request.call())
                .join()
                .unwrap_or_else(|_| Err(NodeError::Failed("the request panicked".to_string())))
        })?;
        Ok(Routed { value, port: None })
    }

    fn ports(&self) -> Ports {
        Ports::new(["in"], ["out"])
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            kind: KIND,
            title: "HTTP request",
            description: "Calls a URL on a host the server allows and outputs the status, headers and body.",
        }
    }
}

/// For the tests of every part: a local server on a port the system picks, and the settings and nodes they build.
#[cfg(test)]
mod testing {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use axum::Router;
    use serde_json::Value as Json;

    use super::{HttpNode, Node, NodeError, Settings};
    use crate::server::settings::Secrets;

    /// A local server on a port the system picks, on a runtime that lives as long as the value.
    pub(super) struct Fake {
        pub(super) addr: SocketAddr,
        _runtime: tokio::runtime::Runtime,
    }

    pub(super) fn serve(router: Router) -> Fake {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let addr = listener.local_addr().unwrap();
        runtime.spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Fake {
            addr,
            _runtime: runtime,
        }
    }

    pub(super) fn settings(dir: &std::path::Path, hosts: &[&str]) -> Option<Arc<Settings>> {
        Some(Arc::new(Settings {
            hosts: hosts.iter().map(|h| h.to_string()).collect(),
            secrets: Secrets::new(dir),
        }))
    }

    pub(super) fn node(params: Json, settings: Option<Arc<Settings>>) -> HttpNode {
        HttpNode::with_settings(&params, settings).unwrap()
    }

    pub(super) fn fails(node: &HttpNode) -> String {
        match node.run(Json::Null) {
            Err(NodeError::Failed(message)) => message,
            other => panic!("expected a failure, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::serve;
    use super::*;
    use crate::blobs::redact;
    use axum::Router;
    use axum::http::HeaderMap as AxumHeaders;
    use axum::routing::get;
    use serde_json::json;

    #[test]
    fn the_server_configuration_reaches_the_node_through_the_registry() {
        let secret = "registry-http-secret-5c2e9b8d71f4a6";
        let fake = serve(Router::new().route(
            "/echo",
            get(|headers: AxumHeaders| async move {
                let sent = headers["authorization"].to_str().unwrap().to_string();
                (
                    [("content-type", "application/json")],
                    json!({"sent": sent}).to_string(),
                )
            }),
        ));
        let dir = tempfile::tempdir().unwrap();
        let config = crate::config::Config::parse(
            "[workflows]\nhttp_hosts = [\"127.0.0.1\"]\n",
            dir.path(),
            std::path::Path::new("moebius.toml"),
        )
        .unwrap();
        Secrets::new(&config.data_dir)
            .set("service", secret)
            .unwrap();
        configure(&config);

        // Built the way the run builds it, the node has the hosts and the secrets file the server configured.
        let http = crate::workflows::nodes::node(
            KIND,
            &json!({"url": format!("http://{}/echo", fake.addr), "secret": "service"}),
        )
        .unwrap();
        let output = http.run(json!(null)).unwrap();
        assert_eq!(output["status"], 200);
        assert_eq!(output["body"]["sent"], format!("Bearer {secret}"));
        assert!(!redact(&output.to_string()).contains(secret));

        // And a host the configuration does not name is refused before any request is made.
        let refused = crate::workflows::nodes::node(
            KIND,
            &json!({"url": "https://not-configured.example.com/"}),
        )
        .unwrap();
        match refused.run(json!(null)) {
            Err(NodeError::Failed(message)) => assert!(
                message.contains(
                    "the host \"not-configured.example.com\" is not in workflows.http_hosts"
                ),
                "{message}"
            ),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }
}
