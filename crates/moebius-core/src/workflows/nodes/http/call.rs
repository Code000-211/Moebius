//! Making the call the `http` node describes: the request headers with a named secret resolved from the server's
//! secrets file, the request itself with a time limit and no redirects, and the answer read back as
//! `{status, headers, body}`. A failure is closed and its message never carries the URL's query or a secret.

use std::io::Read;

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value as Json};

use super::{MAX_BODY_BYTES, METHODS, NodeError, Request, SECRET_PREFIX};
use crate::blobs::{redact, remember_secret};

impl Request {
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

    pub(super) fn call(&self) -> Result<Json, NodeError> {
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

#[cfg(test)]
mod tests {
    use super::super::testing::{fails, node, serve, settings};
    use super::super::{MAX_BODY_BYTES, Node};
    use super::*;
    use crate::server::settings::Secrets;
    use axum::Router;
    use axum::http::{HeaderMap as AxumHeaders, StatusCode};
    use axum::routing::{get, post};
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn a_json_answer_reaches_the_next_node() {
        use crate::workflows::decorate::{self, Scope};

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
    fn a_secret_that_is_not_there_fails_the_node() {
        let dir = tempfile::tempdir().unwrap();
        let http = node(
            json!({"url": "https://api.example.com/", "secret": "missing"}),
            settings(dir.path(), &["api.example.com"]),
        );
        assert_eq!(fails(&http), "no secret is named \"missing\"");
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

    #[test]
    fn a_body_over_the_cap_fails_the_node() {
        let fake = serve(Router::new().route(
            "/large",
            get(|| async {
                let body = vec![b'x'; (MAX_BODY_BYTES + 1) as usize];
                ([("content-type", "text/plain")], body)
            }),
        ));
        let dir = tempfile::tempdir().unwrap();
        let http = node(
            json!({"url": format!("http://{}/large", fake.addr)}),
            settings(dir.path(), &["127.0.0.1"]),
        );
        assert_eq!(
            fails(&http),
            format!("the response is longer than {MAX_BODY_BYTES} bytes")
        );
    }

    /// A connection that cannot be made reports no part of the URL's query, which may carry a token.
    #[test]
    fn a_request_error_carries_no_url_query() {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = closed.local_addr().unwrap();
        drop(closed);
        let dir = tempfile::tempdir().unwrap();
        let token = "query-token-6f1d3a95c2b8";
        let http = node(
            json!({"url": format!("http://{addr}/path?token={token}")}),
            settings(dir.path(), &["127.0.0.1"]),
        );
        let message = fails(&http);
        assert!(message.starts_with("the request failed: "), "{message}");
        assert!(!message.contains(token), "{message}");
        assert!(!message.contains("token="), "{message}");
    }

    /// No secret value reaches the debug print of a node built from its parameters.
    #[test]
    fn a_node_holds_no_secret_value() {
        let dir = tempfile::tempdir().unwrap();
        let secret = "held-http-secret-2a7e4c9f1b5d";
        Secrets::new(dir.path()).set("service", secret).unwrap();
        let http = node(
            json!({"url": "https://api.example.com/", "secret": "service"}),
            settings(dir.path(), &["api.example.com"]),
        );
        // The value is only read when the request is made, so the node's own state names the secret, not its value.
        assert!(!format!("{http:?}").contains(secret));
    }
}
