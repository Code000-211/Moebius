//! The `http` node: one HTTP request, made by the server, because the sandbox has no network. The URL must pass the
//! URL policy (only hosts named in the server configuration), a credential is named and never written in the
//! workflow, and a refusal or a failure fails the node closed. The parameters are read when the node runs, so the
//! URL, a header value and the body may hold expressions the run fills.

use std::io::Read;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::Url;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value as Json};

use super::{Metadata, Node, NodeError, Ports, Routed};
use crate::blobs::{redact, remember_secret};
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

impl Request {
    /// Reads parameters into a request. `allow_expressions` leaves a value that still holds an expression unchecked,
    /// since its value is only known when the node runs; the node reads the filled parameters with it off.
    fn parse(
        params: &Json,
        settings: Option<Arc<Settings>>,
        allow_expressions: bool,
    ) -> Result<Self, NodeError> {
        // A string that still holds `{{` is a value the run has yet to fill.
        let pending = |text: &str| allow_expressions && text.contains("{{");
        let invalid = |message: String| NodeError::Parameters {
            kind: KIND.to_string(),
            message,
        };
        let Some(map) = params.as_object() else {
            return Err(invalid("the parameters must be a table".to_string()));
        };
        const KNOWN: [&str; 6] = [
            "method",
            "url",
            "headers",
            "body",
            "timeout_seconds",
            "secret",
        ];
        if let Some(unknown) = map.keys().find(|key| !KNOWN.contains(&key.as_str())) {
            return Err(invalid(format!("unknown parameter \"{unknown}\"")));
        }
        let text = |key: &str| -> Result<Option<String>, NodeError> {
            match map.get(key) {
                None => Ok(None),
                Some(Json::String(text)) => Ok(Some(text.clone())),
                Some(_) => Err(invalid(format!("\"{key}\" must be a string"))),
            }
        };
        let method = text("method")?.unwrap_or_else(|| "GET".to_string());
        // A method the run will fill is left as written: its text is an expression, not a name yet.
        let method = if pending(&method) {
            method
        } else {
            method.to_ascii_uppercase()
        };
        if !METHODS.contains(&method.as_str()) && !pending(&method) {
            return Err(invalid(format!(
                "\"method\" must be one of {}",
                METHODS.join(", ")
            )));
        }
        let url = match text("url")? {
            Some(url) if !url.trim().is_empty() => url,
            Some(_) => return Err(invalid("\"url\" is empty".to_string())),
            None => return Err(invalid("\"url\" is required".to_string())),
        };
        let mut headers = Vec::new();
        match map.get("headers") {
            None => {}
            Some(Json::Object(table)) => {
                for (name, value) in table {
                    let Json::String(value) = value else {
                        return Err(invalid(format!("header \"{name}\" must be a string")));
                    };
                    headers.push((name.clone(), value.clone()));
                }
            }
            Some(_) => return Err(invalid("\"headers\" must be a table".to_string())),
        }
        let timeout = match map.get("timeout_seconds") {
            None => DEFAULT_TIMEOUT_SECONDS,
            // Seconds the run has yet to fill stand in as the default while the file is read; the node reads the
            // filled value and checks it for real.
            Some(value) if pending(value.as_str().unwrap_or_default()) => DEFAULT_TIMEOUT_SECONDS,
            Some(value) => match value.as_f64() {
                Some(seconds) if seconds > 0.0 && seconds <= MAX_TIMEOUT_SECONDS => seconds,
                _ => {
                    return Err(invalid(format!(
                        "\"timeout_seconds\" must be a number above 0 and at most {MAX_TIMEOUT_SECONDS}"
                    )));
                }
            },
        };
        let secret = text("secret")?;
        Ok(Self {
            method,
            url,
            headers,
            body: map.get("body").filter(|body| !body.is_null()).cloned(),
            timeout: Duration::from_secs_f64(timeout),
            secret,
            settings,
        })
    }

    /// The URL, if the policy allows the call: `https` or `http`, no user name or password, and a host the server
    /// configuration names.
    fn checked_url(&self) -> Result<Url, NodeError> {
        let refuse = |reason: String| {
            NodeError::Failed(format!("the URL policy refuses the call: {reason}"))
        };
        let url = Url::parse(&self.url).map_err(|e| refuse(format!("not a URL ({e})")))?;
        if !matches!(url.scheme(), "https" | "http") {
            return Err(refuse(format!(
                "the scheme \"{}\" is not https or http",
                url.scheme()
            )));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(refuse(
                "the URL carries a user name or password; name a secret instead".to_string(),
            ));
        }
        let host = url
            .host_str()
            .ok_or_else(|| refuse("the URL has no host".to_string()))?
            .to_ascii_lowercase();
        let allowed = self
            .settings
            .as_ref()
            .is_some_and(|settings| settings.hosts.contains(&host));
        if !allowed {
            return Err(refuse(format!(
                "the host \"{host}\" is not in workflows.http_hosts"
            )));
        }
        Ok(url)
    }

    fn request_headers(&self) -> Result<HeaderMap, NodeError> {
        let failed = |message: String| NodeError::Failed(message);
        let mut map = HeaderMap::new();
        for (name, value) in &self.headers {
            let header = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| failed(format!("\"{name}\" is not a header name")))?;
            let value = HeaderValue::from_str(value)
                .map_err(|_| failed(format!("the value of header \"{name}\" is not valid")))?;
            map.insert(header, value);
        }
        if let Some(secret) = &self.secret {
            let settings = self
                .settings
                .as_ref()
                .ok_or_else(|| failed("no secrets are configured".to_string()))?;
            let value = settings
                .secrets
                .get(secret)
                .map_err(|_| failed("the secrets file cannot be read".to_string()))?
                .ok_or_else(|| failed(format!("no secret is named \"{secret}\"")))?;
            remember_secret(&value);
            let mut value = HeaderValue::from_str(&format!("{SECRET_PREFIX}{}", value.trim()))
                .map_err(|_| {
                    failed(format!(
                        "the secret \"{secret}\" is not a valid header value"
                    ))
                })?;
            value.set_sensitive(true);
            map.insert(HeaderName::from_static("authorization"), value);
        }
        Ok(map)
    }

    fn call(&self) -> Result<Json, NodeError> {
        let url = self.checked_url()?;
        let headers = self.request_headers()?;
        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            // A redirect would leave the hosts the policy checked.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| NodeError::Failed(describe(&e)))?;
        let method = reqwest::Method::from_bytes(self.method.as_bytes()).map_err(|_| {
            NodeError::Failed(format!(
                "\"{}\" is not one of {}",
                self.method,
                METHODS.join(", ")
            ))
        })?;
        let mut request = client.request(method, url).headers(headers);
        match &self.body {
            None => {}
            Some(Json::String(text)) => request = request.body(text.clone()),
            Some(other) => {
                if !self
                    .headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                {
                    request = request.header(CONTENT_TYPE, "application/json");
                }
                request = request.body(other.to_string());
            }
        }
        let response = request.send().map_err(|e| self.failure(&e))?;
        let status = response.status().as_u16();
        let mut headers = Map::new();
        for (name, value) in response.headers() {
            let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
            match headers.get_mut(name.as_str()) {
                Some(Json::String(joined)) => {
                    joined.push_str(", ");
                    joined.push_str(&value);
                }
                _ => {
                    headers.insert(name.as_str().to_string(), Json::String(value));
                }
            }
        }
        let is_json = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.to_ascii_lowercase().contains("json"));
        let mut bytes = Vec::new();
        response
            .take(MAX_BODY_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| NodeError::Failed(describe(&e)))?;
        if bytes.len() as u64 > MAX_BODY_BYTES {
            return Err(NodeError::Failed(format!(
                "the response is longer than {MAX_BODY_BYTES} bytes"
            )));
        }
        let body = is_json
            .then(|| serde_json::from_slice::<Json>(&bytes).ok())
            .flatten()
            .unwrap_or_else(|| Json::String(String::from_utf8_lossy(&bytes).into_owned()));
        Ok(serde_json::json!({ "status": status, "headers": headers, "body": body }))
    }

    fn failure(&self, error: &reqwest::Error) -> NodeError {
        if error.is_timeout() {
            NodeError::Failed(format!(
                "the request timed out after {} seconds",
                self.timeout.as_secs_f64()
            ))
        } else {
            NodeError::Failed(describe(&without_url(error)))
        }
    }
}

/// An error's text, with anything the server knows to be a secret removed.
fn describe(error: &dyn std::fmt::Display) -> String {
    redact(&format!("the request failed: {error}"))
}

/// A request error's text without the URL, which may carry a token in its query.
fn without_url(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    if let Some(url) = error.url() {
        text = text.replace(url.as_str(), "<url>");
    }
    text
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http::{HeaderMap as AxumHeaders, StatusCode};
    use axum::routing::{get, post};
    use serde_json::json;
    use std::net::SocketAddr;
    use std::sync::Mutex;

    /// A local server on a port the system picks, on a runtime that lives as long as the value.
    struct Fake {
        addr: SocketAddr,
        _runtime: tokio::runtime::Runtime,
    }

    fn serve(router: Router) -> Fake {
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

    fn settings(dir: &std::path::Path, hosts: &[&str]) -> Option<Arc<Settings>> {
        Some(Arc::new(Settings {
            hosts: hosts.iter().map(|h| h.to_string()).collect(),
            secrets: Secrets::new(dir),
        }))
    }

    fn node(params: Json, settings: Option<Arc<Settings>>) -> HttpNode {
        HttpNode::with_settings(&params, settings).unwrap()
    }

    fn fails(node: &HttpNode) -> String {
        match node.run(json!(null)) {
            Err(NodeError::Failed(message)) => message,
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn a_json_answer_reaches_the_next_node() {
        let fake = serve(Router::new().route(
            "/items",
            get(|| async {
                (
                    [("content-type", "application/json"), ("x-total", "2")],
                    r#"{"items":[1,2]}"#,
                )
            }),
        ));
        let dir = tempfile::tempdir().unwrap();
        let http = node(
            json!({"url": format!("http://{}/items", fake.addr)}),
            settings(dir.path(), &["127.0.0.1"]),
        );
        let output = http.run(json!(null)).unwrap();
        assert_eq!(output["status"], 200);
        assert_eq!(output["body"], json!({"items": [1, 2]}));
        assert_eq!(output["headers"]["x-total"], "2");

        // The next node reads it through an expression, as any node's output.
        let scope = Scope {
            trigger: json!(null),
            input: output,
            nodes: Default::default(),
        };
        let filled = decorate::fill(
            "next",
            &json!({"second": "{{ input.body.items[2] }}", "status": "{{ input.status }}"}),
            &scope,
        )
        .unwrap();
        assert_eq!(filled, json!({"second": 2, "status": 200}));
    }

    /// The node fills the expressions its own parameters hold from the run: the trigger, the value that reached it,
    /// and the values of the nodes before it.
    #[test]
    fn the_expressions_in_the_parameters_are_filled_before_the_call() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let fake = serve(Router::new().fallback(get(
            |uri: axum::extract::OriginalUri, headers: AxumHeaders| async move {
                let branch = headers["x-branch"].to_str().unwrap_or_default().to_string();
                record
                    .lock()
                    .unwrap()
                    .push((uri.0.path().to_string(), branch));
                StatusCode::OK
            },
        )));
        let dir = tempfile::tempdir().unwrap();
        let mut earlier = std::collections::BTreeMap::new();
        earlier.insert("build".to_string(), json!({"tag": "v2"}));
        let http = node(
            json!({
                "url": format!("http://{}/runs/{{{{ trigger.number }}}}", fake.addr),
                "headers": {"X-Branch": "{{ node(\"build\").tag }}"},
            }),
            settings(dir.path(), &["127.0.0.1"]),
        );
        let scope = Scope {
            trigger: json!({"number": 7}),
            input: json!(null),
            nodes: earlier,
        };
        assert_eq!(http.run_routed(&scope).unwrap().value["status"], 200);
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            [("/runs/7".to_string(), "v2".to_string())]
        );
    }

    /// An expression that does not evaluate fails the node closed, naming the parameter it was in, and no request is
    /// made.
    #[test]
    fn an_expression_that_fails_fails_the_node_without_a_call() {
        let seen = Arc::new(Mutex::new(0));
        let record = seen.clone();
        let fake = serve(Router::new().fallback(get(move || async move {
            *record.lock().unwrap() += 1;
            StatusCode::OK
        })));
        let dir = tempfile::tempdir().unwrap();
        let http = node(
            json!({"url": format!("http://{}/{{{{ nil + 1 }}}}", fake.addr)}),
            settings(dir.path(), &["127.0.0.1"]),
        );
        let error = match http.run(json!(null)) {
            Err(NodeError::Failed(message)) => message,
            other => panic!("expected a failure, got {other:?}"),
        };
        assert!(error.contains("url"), "{error}");
        assert_eq!(*seen.lock().unwrap(), 0);
    }

    #[test]
    fn a_body_that_is_not_json_stays_text() {
        let fake = serve(Router::new().route(
            "/text",
            get(|| async { ([("content-type", "text/plain")], r#"{"not":"read"}"#) }),
        ));
        let dir = tempfile::tempdir().unwrap();
        let http = node(
            json!({"url": format!("http://{}/text", fake.addr)}),
            settings(dir.path(), &["127.0.0.1"]),
        );
        assert_eq!(
            http.run(json!(null)).unwrap()["body"],
            json!(r#"{"not":"read"}"#)
        );
    }

    #[test]
    fn a_json_body_is_sent_with_its_method_and_headers() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let fake = serve(Router::new().route(
            "/post",
            post(move |headers: AxumHeaders, body: String| async move {
                let kind = headers["content-type"].to_str().unwrap().to_string();
                let extra = headers["x-extra"].to_str().unwrap().to_string();
                record.lock().unwrap().push((kind, extra, body));
                StatusCode::CREATED
            }),
        ));
        let dir = tempfile::tempdir().unwrap();
        let http = node(
            json!({
                "method": "post",
                "url": format!("http://{}/post", fake.addr),
                "headers": {"X-Extra": "yes"},
                "body": {"a": 1},
            }),
            settings(dir.path(), &["127.0.0.1"]),
        );
        assert_eq!(http.run(json!(null)).unwrap()["status"], 201);
        assert_eq!(
            *seen.lock().unwrap(),
            [(
                "application/json".to_string(),
                "yes".to_string(),
                r#"{"a":1}"#.to_string()
            )]
        );
    }

    #[test]
    fn a_secret_is_sent_but_never_stored_or_logged() {
        let secret = "node-http-secret-9f3a7c21d4e8b6";
        let fake = serve(Router::new().route(
            "/echo",
            get(|headers: AxumHeaders| async move {
                let sent = headers["authorization"].to_str().unwrap().to_string();
                ([("content-type", "text/plain")], format!("you sent {sent}"))
            }),
        ));
        let dir = tempfile::tempdir().unwrap();
        Secrets::new(dir.path()).set("service", secret).unwrap();
        let params = json!({
            "url": format!("http://{}/echo", fake.addr),
            "secret": "service",
        });
        let http = node(params.clone(), settings(dir.path(), &["127.0.0.1"]));
        let output = http.run(json!(null)).unwrap();

        // The server got it ...
        assert_eq!(output["body"], format!("you sent Bearer {secret}"));
        // ... the node holds only the name, in its parameters and in what it prints ...
        assert!(!format!("{http:?}").contains(secret));
        assert!(!params.to_string().contains(secret));
        // ... and what the store keeps of the output has it removed.
        let stored = redact(&output.to_string());
        assert!(!stored.contains(secret), "{stored}");
        assert!(stored.contains("[redacted:secret]"));
    }

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

    #[test]
    fn a_secret_that_is_not_there_fails_the_node() {
        let dir = tempfile::tempdir().unwrap();
        let http = node(
            json!({"url": "http://127.0.0.1:1/", "secret": "missing"}),
            settings(dir.path(), &["127.0.0.1"]),
        );
        assert_eq!(fails(&http), "no secret is named \"missing\"");
    }

    #[test]
    fn the_url_policy_refuses_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let allow = settings(dir.path(), &["api.example.com"]);
        for (url, reason) in [
            (
                "https://other.example.com/x",
                "the host \"other.example.com\" is not in workflows.http_hosts",
            ),
            (
                "ftp://api.example.com/x",
                "the scheme \"ftp\" is not https or http",
            ),
            (
                "file:///etc/passwd",
                "the scheme \"file\" is not https or http",
            ),
            (
                "https://user:pw@api.example.com/x",
                "the URL carries a user name or password",
            ),
            ("not a url", "not a URL"),
        ] {
            let message = fails(&node(json!({"url": url}), allow.clone()));
            assert!(
                message.starts_with("the URL policy refuses the call: ")
                    && message.contains(reason),
                "{url}: {message}"
            );
        }
        // Without any host in the configuration nothing is allowed.
        let message = fails(&node(
            json!({"url": "https://api.example.com/"}),
            settings(dir.path(), &[]),
        ));
        assert!(message.contains("not in workflows.http_hosts"), "{message}");
        let message = fails(&node(json!({"url": "https://api.example.com/"}), None));
        assert!(message.contains("not in workflows.http_hosts"), "{message}");
    }

    #[test]
    fn a_redirect_is_not_followed() {
        let fake = serve(Router::new().route(
            "/go",
            get(|| async {
                (
                    StatusCode::FOUND,
                    [("location", "http://elsewhere.invalid/")],
                )
            }),
        ));
        let dir = tempfile::tempdir().unwrap();
        let http = node(
            json!({"url": format!("http://{}/go", fake.addr)}),
            settings(dir.path(), &["127.0.0.1"]),
        );
        let output = http.run(json!(null)).unwrap();
        assert_eq!(output["status"], 302);
        assert_eq!(output["headers"]["location"], "http://elsewhere.invalid/");
    }

    #[test]
    fn a_slow_answer_times_out_and_fails_the_node() {
        let fake = serve(Router::new().route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                "late"
            }),
        ));
        let dir = tempfile::tempdir().unwrap();
        let http = node(
            json!({"url": format!("http://{}/slow", fake.addr), "timeout_seconds": 0.3}),
            settings(dir.path(), &["127.0.0.1"]),
        );
        assert_eq!(fails(&http), "the request timed out after 0.3 seconds");
    }

    /// A method that only the run knows is checked when it runs, and a wrong one fails the node before any request.
    #[test]
    fn a_method_filled_from_the_run_is_checked_when_the_node_runs() {
        let seen = Arc::new(Mutex::new(0));
        let record = seen.clone();
        let fake = serve(Router::new().fallback(get(move || async move {
            *record.lock().unwrap() += 1;
            StatusCode::OK
        })));
        let dir = tempfile::tempdir().unwrap();
        let http = node(
            json!({
                "url": format!("http://{}/go", fake.addr),
                "method": "{{ trigger.verb }}",
            }),
            settings(dir.path(), &["127.0.0.1"]),
        );
        let scope = Scope {
            trigger: json!({"verb": "TRACE"}),
            input: json!(null),
            nodes: Default::default(),
        };
        let error = match http.run_routed(&scope) {
            Err(NodeError::Parameters { message, .. }) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(error.contains("\"method\" must be one of"), "{error}");
        assert_eq!(*seen.lock().unwrap(), 0);
    }

    #[test]
    fn parameters_are_checked() {
        let bad = |params: Json| match HttpNode::with_settings(&params, None) {
            Err(NodeError::Parameters { message, .. }) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert_eq!(bad(json!({})), "\"url\" is required");
        assert_eq!(
            bad(json!({"url": "https://a", "token": "x"})),
            "unknown parameter \"token\""
        );
        assert!(
            bad(json!({"url": "https://a", "method": "TRACE"}))
                .contains("\"method\" must be one of")
        );
        assert!(bad(json!({"url": "https://a", "timeout_seconds": 0})).contains("timeout_seconds"));
        assert!(bad(json!({"url": "https://a", "headers": {"a": 1}})).contains("header \"a\""));
        // A value the run will fill is not a value to refuse while the file is read.
        HttpNode::with_settings(
            &json!({"url": "{{ trigger.url }}", "timeout_seconds": "{{ trigger.seconds }}"}),
            None,
        )
        .unwrap();
    }
}
