//! What the tests of the logins share: a fake provider on port 0 that is both the OAuth server and an upstream that
//! lists its models to a logged-in client, and a router with one upstream that logs in and one with a key.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::json;

use super::{Flow, LoginState, OAuthConfig, Secrets};
use crate::router::{Login, Protocol, Router, Upstream};
use crate::store::Store;

/// What the fake provider does and what it was asked.
#[derive(Default)]
struct Provider {
    url: String,
    /// How long the tokens it gives live, in seconds.
    lifetime: u64,
    /// How many tokens it has given.
    given: u64,
    pending_polls: u32,
    decline_polls: bool,
    refuse_refresh: bool,
    device_requests: u32,
    device_scope: String,
    device_polls: u32,
    refreshed_with: Vec<String>,
    /// The code, verifier and redirect URI of each code exchanged.
    exchanged: Vec<(String, String, String)>,
    upstream_auth: Vec<String>,
    /// Whether the last token request had a JSON body (otherwise a form).
    json_tokens: bool,
    /// The state of the last code exchange, if the request had one.
    exchanged_state: Option<String>,
    /// The headers (lower case names) and body of each request to `/v1/messages`.
    messages: Vec<(HashMap<String, String>, serde_json::Value)>,
}

pub(crate) struct FakeProvider {
    pub(crate) url: String,
    state: Arc<Mutex<Provider>>,
}

/// Where the fake provider starts a device login and gives tokens: its own, not routes of the router.
const DEVICE_PATH: &str = "/device/code";
const TOKEN_PATH: &str = "/token";

fn access_token(n: u64) -> String {
    format!("access-token-{n}")
}

fn refresh_token(n: u64) -> String {
    format!("refresh-token-{n}")
}

type Shared = State<Arc<Mutex<Provider>>>;

fn tokens(provider: &mut Provider) -> Response {
    provider.given += 1;
    axum::Json(json!({
        "access_token": access_token(provider.given),
        "refresh_token": refresh_token(provider.given),
        "token_type": "Bearer",
        "expires_in": provider.lifetime,
    }))
    .into_response()
}

fn oauth_error(code: &str) -> Response {
    (StatusCode::BAD_REQUEST, axum::Json(json!({"error": code}))).into_response()
}

async fn device(
    State(state): Shared,
    axum::Form(form): axum::Form<HashMap<String, String>>,
) -> Response {
    let mut provider = state.lock().unwrap();
    provider.device_requests += 1;
    provider.device_scope = form.get("scope").cloned().unwrap_or_default();
    axum::Json(json!({
        "device_code": "device-code-of-the-test",
        "user_code": "WDJB-MJHT",
        "verification_uri": format!("{}/device", provider.url),
        "expires_in": 600,
        "interval": 0,
    }))
    .into_response()
}

async fn token(State(state): Shared, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    let mut provider = state.lock().unwrap();
    let json = headers
        .get("content-type")
        .is_some_and(|v| v.to_str().unwrap().starts_with("application/json"));
    let form: HashMap<String, String> = if json {
        serde_json::from_slice(&body).unwrap()
    } else {
        reqwest::Url::parse(&format!("http://x/?{}", String::from_utf8_lossy(&body)))
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    };
    provider.json_tokens = json;
    assert_eq!(form["client_id"], "moebius-test");
    match form["grant_type"].as_str() {
        "refresh_token" => {
            provider.refreshed_with.push(form["refresh_token"].clone());
            if provider.refuse_refresh {
                return oauth_error("invalid_grant");
            }
            tokens(&mut provider)
        }
        "urn:ietf:params:oauth:grant-type:device_code" => {
            assert_eq!(form["device_code"], "device-code-of-the-test");
            provider.device_polls += 1;
            if provider.decline_polls {
                return oauth_error("access_denied");
            }
            if provider.pending_polls > 0 {
                provider.pending_polls -= 1;
                return oauth_error("authorization_pending");
            }
            tokens(&mut provider)
        }
        "authorization_code" => {
            provider.exchanged_state = form.get("state").cloned();
            provider.exchanged.push((
                form["code"].clone(),
                form["code_verifier"].clone(),
                form["redirect_uri"].clone(),
            ));
            tokens(&mut provider)
        }
        other => panic!("grant type {other}"),
    }
}

async fn models(State(state): Shared, headers: HeaderMap) -> Response {
    let mut provider = state.lock().unwrap();
    let auth = headers
        .get("authorization")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    provider.upstream_auth.push(auth.clone());
    if auth != format!("Bearer {}", access_token(provider.given)) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    axum::Json(json!({"data": [{"id": "subscribed-a"}, {"id": "subscribed-b"}]})).into_response()
}

/// An upstream of either protocol: answers a message, noting how it was asked.
async fn message(
    State(state): Shared,
    headers: HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Response {
    let mut provider = state.lock().unwrap();
    let headers = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap().to_string()))
        .collect();
    provider.messages.push((headers, body));
    axum::Json(json!({
        "type": "message", "model": "subscribed-a", "role": "assistant",
        "content": [{"type": "text", "text": "Hi"}],
        "usage": {"input_tokens": 3, "output_tokens": 2},
    }))
    .into_response()
}

impl FakeProvider {
    /// A provider whose tokens live `lifetime` seconds.
    pub(crate) async fn start(lifetime: u64) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(Provider {
            url: url.clone(),
            lifetime,
            ..Provider::default()
        }));
        let app = axum::Router::new()
            .route(DEVICE_PATH, post(device))
            .route(TOKEN_PATH, post(token))
            .route("/models", get(models))
            .route("/v1/models", get(models))
            .route("/v1/messages", post(message))
            .with_state(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
        Self { url, state }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Provider) -> T) -> T {
        f(&mut self.state.lock().unwrap())
    }

    pub(crate) fn access_token(&self, n: u64) -> String {
        access_token(n)
    }

    pub(crate) fn refresh_token(&self, n: u64) -> String {
        refresh_token(n)
    }

    pub(crate) fn set_lifetime(&self, seconds: u64) {
        self.with(|p| p.lifetime = seconds);
    }

    pub(crate) fn pending_polls(&self, polls: u32) {
        self.with(|p| p.pending_polls = polls);
    }

    pub(crate) fn decline_polls(&self) {
        self.with(|p| p.decline_polls = true);
    }

    pub(crate) fn refuse_refresh(&self) {
        self.with(|p| p.refuse_refresh = true);
    }

    pub(crate) fn device_requests(&self) -> u32 {
        self.with(|p| p.device_requests)
    }

    pub(crate) fn device_scope(&self) -> String {
        self.with(|p| p.device_scope.clone())
    }

    pub(crate) fn device_polls(&self) -> u32 {
        self.with(|p| p.device_polls)
    }

    /// How many refreshes it was asked for.
    pub(crate) fn token_requests(&self) -> usize {
        self.with(|p| p.refreshed_with.len())
    }

    pub(crate) fn refreshed_with(&self) -> Vec<String> {
        self.with(|p| p.refreshed_with.clone())
    }

    pub(crate) fn exchanged_codes(&self) -> Vec<(String, String, String)> {
        self.with(|p| p.exchanged.clone())
    }

    pub(crate) fn json_tokens(&self) -> bool {
        self.with(|p| p.json_tokens)
    }

    pub(crate) fn exchanged_state(&self) -> Option<String> {
        self.with(|p| p.exchanged_state.clone())
    }

    /// The headers and body of each request to `/v1/messages`.
    pub(crate) fn messages(&self) -> Vec<(HashMap<String, String>, serde_json::Value)> {
        self.with(|p| p.messages.clone())
    }

    pub(crate) fn upstream_auth(&self) -> Vec<String> {
        self.with(|p| p.upstream_auth.clone())
    }
}

/// The `oauth` table of an upstream that logs in at `url` with `flow`.
pub(crate) fn config(url: &str, flow: Flow) -> OAuthConfig {
    OAuthConfig {
        preset: None,
        flow,
        client_id: "moebius-test".into(),
        token_url: format!("{url}{TOKEN_PATH}"),
        device_url: Some(format!("{url}{DEVICE_PATH}")),
        authorize_url: Some(format!("{url}/authorize")),
        scopes: vec!["models".into(), "offline".into()],
        redirect_port: None,
        redirect_path: None,
        secret: "provider-login".into(),
    }
}

/// The `oauth` table of an upstream that names `preset` and sends the logins to the fake provider, as a person who
/// wrote `preset` and the URLs would have it; any free loopback port.
pub(crate) fn preset_config(url: &str, preset: &str) -> OAuthConfig {
    toml::from_str(&format!(
        "preset = \"{preset}\"\nclient_id = \"moebius-test\"\ntoken_url = \"{url}{TOKEN_PATH}\"\n\
         authorize_url = \"{url}/authorize\"\nredirect_port = 0\n"
    ))
    .unwrap()
}

/// A router with one upstream of `protocol` that logs in at `provider` with the preset `preset`; its secrets are kept
/// in the directory returned.
pub(crate) fn router_with_preset(
    provider: &FakeProvider,
    preset: &str,
    protocol: Protocol,
) -> (Arc<Router>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::in_memory(dir.path()).unwrap());
    let upstreams = vec![Upstream {
        protocol,
        base_url: provider.url.clone(),
        api_key: None,
        oauth: Some(preset_config(&provider.url, preset)),
    }];
    let router = Router::new(store, upstreams).with_secrets(Secrets::new(dir.path()));
    (Arc::new(router), dir)
}

/// A router whose first upstream logs in at `provider` with `flow`, and whose second has a key; its secrets are kept
/// in the directory returned.
pub(crate) fn router_with(provider: &FakeProvider, flow: Flow) -> (Arc<Router>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::in_memory(dir.path()).unwrap());
    let upstreams = vec![
        Upstream {
            protocol: Protocol::OpenAi,
            base_url: provider.url.clone(),
            api_key: None,
            oauth: Some(config(&provider.url, flow)),
        },
        Upstream {
            protocol: Protocol::Anthropic,
            base_url: provider.url.clone(),
            api_key: Some("sk-key".into()),
            oauth: None,
        },
    ];
    let router = Router::new(store, upstreams).with_secrets(Secrets::new(dir.path()));
    (Arc::new(router), dir)
}

/// Waits until the login started for the upstream at `place` reaches `state`.
pub(crate) async fn wait_for(router: &Router, place: usize, state: LoginState) -> Login {
    let (_, oauth) = router.oauth_upstream(place).unwrap();
    for _ in 0..1000 {
        let login = router
            .oauth
            .running
            .lock()
            .unwrap()
            .get(&oauth.secret)
            .map(|r| r.login.clone());
        if let Some(login) = login.filter(|l| l.state == state) {
            return login;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the login never reached {state:?}");
}
