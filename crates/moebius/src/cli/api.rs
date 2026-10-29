//! `moebius api`: any route of the desktop API, with a method, a route and a JSON body, its answer printed as it came.
//! It reaches what no other command names.

use reqwest::Method;
use serde_json::json;

use super::{Plan, Request, Show, json_input};

/// The request of `api <method> <route> [--body <body>]`.
pub fn plan(method: &str, route: &str, body: Option<&str>) -> Plan {
    let method = Method::from_bytes(method.to_ascii_uppercase().as_bytes())
        .ok()
        .filter(|m| [Method::GET, Method::POST, Method::PUT, Method::DELETE].contains(m))
        .ok_or_else(|| format!("{method} is not GET, POST, PUT or DELETE"))?;
    let path = if route.starts_with('/') {
        route.to_string()
    } else {
        format!("/{route}")
    };
    let body = match body {
        Some(body) => Some(json_input(body)?),
        // A request that changes something has a body, `{}` at least.
        None if method == Method::POST || method == Method::PUT => Some(json!({})),
        None => None,
    };
    Ok((Request { method, path, body }, Show::Raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::tests::plan;

    #[test]
    fn an_api_call_takes_a_method_a_route_and_a_body() {
        let (request, _) = plan(&["api", "post", "v1/cards/3/approve"]).unwrap();
        assert_eq!(request.method, Method::POST);
        assert_eq!(request.path, "/v1/cards/3/approve");
        assert_eq!(request.body, Some(json!({})));

        let (request, _) = plan(&["api", "GET", "/v1/snapshot"]).unwrap();
        assert_eq!((request.method, request.body), (Method::GET, None));

        let (request, _) = plan(&["api", "PUT", "/v1/rules", "--body", r#"{"text":"x"}"#]).unwrap();
        assert_eq!(request.body, Some(json!({"text": "x"})));

        assert!(
            plan(&["api", "PATCH", "/v1/rules"])
                .unwrap_err()
                .contains("PATCH")
        );
        assert!(
            plan(&["api", "POST", "/v1/rules", "--body", "{nope"])
                .unwrap_err()
                .contains("not valid JSON")
        );
    }

    #[test]
    fn a_body_can_come_from_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("body.json");
        std::fs::write(&file, r#"{"reason": "fine"}"#).unwrap();
        let (request, _) =
            plan(&["api", "POST", "/v1/x", "--body", file.to_str().unwrap()]).unwrap();
        assert_eq!(request.body, Some(json!({"reason": "fine"})));
        assert!(
            plan(&["api", "POST", "/v1/x", "--body", "/no/such/file"])
                .unwrap_err()
                .contains("cannot read")
        );
    }
}
