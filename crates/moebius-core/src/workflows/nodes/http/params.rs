//! Reading an `http` node's parameters into a request: what the workflow wrote when it is read, and what the run
//! filled when the node runs. A parameter that is wrong in shape is refused here, before any call.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value as Json;

use super::{
    DEFAULT_TIMEOUT_SECONDS, KIND, MAX_TIMEOUT_SECONDS, METHODS, NodeError, Request, Settings,
};

impl Request {
    /// Reads parameters into a request. `allow_expressions` leaves a value that still holds an expression unchecked,
    /// since its value is only known when the node runs; the node reads the filled parameters with it off.
    pub(super) fn parse(
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
}

#[cfg(test)]
mod tests {
    use super::super::testing::{node, serve, settings};
    use super::super::{HttpNode, Node, NodeError};
    use super::*;
    use axum::Router;
    use axum::http::{HeaderMap as AxumHeaders, StatusCode};
    use axum::routing::get;
    use serde_json::json;
    use std::sync::Mutex;

    /// The node fills the expressions its own parameters hold from the run: the trigger, the value that reached it,
    /// and the values of the nodes before it.
    #[test]
    fn the_expressions_in_the_parameters_are_filled_before_the_call() {
        use crate::workflows::decorate::Scope;

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

    /// A method that only the run knows is checked when it runs, and a wrong one fails the node before any request.
    #[test]
    fn a_method_filled_from_the_run_is_checked_when_the_node_runs() {
        use crate::workflows::decorate::Scope;

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
        assert!(
            bad(json!({"url": "https://a", "timeout_seconds": 301}))
                .contains("\"timeout_seconds\" must be a number above 0 and at most 300")
        );
        assert!(bad(json!({"url": "https://a", "headers": {"a": 1}})).contains("header \"a\""));
        // The longest timeout a node may ask for is allowed.
        HttpNode::with_settings(&json!({"url": "https://a", "timeout_seconds": 300}), None)
            .unwrap();
        // A value the run will fill is not a value to refuse while the file is read.
        HttpNode::with_settings(
            &json!({"url": "{{ trigger.url }}", "timeout_seconds": "{{ trigger.seconds }}"}),
            None,
        )
        .unwrap();
    }
}
