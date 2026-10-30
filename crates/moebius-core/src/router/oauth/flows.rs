//! The tests of the built-in providers (see `preset`) against a fake provider: the login by the loopback address and by
//! a pasted address, the refresh, and the requests sent with the token.

use axum::http::HeaderMap;
use serde_json::{Value, json};

use super::testing::{FakeProvider, router_with_preset, wait_for};
use super::*;
use crate::router::classes::Answered;

/// The query of `url` as a map.
fn query(url: &str) -> HashMap<String, String> {
    reqwest::Url::parse(url)
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// A `POST /v1/messages` of a harness, sent through the router to its first upstream.
async fn send(router: &Router, request: Value, harness: &[(&str, &str)]) -> Answered {
    let mut headers = HeaderMap::new();
    for (name, value) in harness {
        headers.insert(
            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    let upstreams = router.upstreams_of(Protocol::Anthropic);
    let mut request = request;
    router
        .send_first(
            Protocol::Anthropic,
            "/v1/messages",
            &headers,
            &mut request,
            &upstreams,
            (&["claude-sonnet-fake".to_string()], false),
        )
        .await
        .unwrap_or_else(|_| panic!("the request was not sent"))
}

#[tokio::test]
async fn the_anthropic_preset_logs_in_by_the_loopback_address_with_what_pi_asks_and_json_tokens() {
    let provider = FakeProvider::start(3600).await;
    let (router, _dir) = router_with_preset(&provider, "anthropic", Protocol::Anthropic);
    let login = router.start_login(0, true).await.unwrap();
    let asked = query(&login.authorize_url.unwrap());
    assert_eq!(asked["client_id"], "moebius-test");
    assert_eq!(asked["code"], "true");
    assert_eq!(asked["response_type"], "code");
    assert_eq!(
        asked["scope"],
        "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload"
    );
    // The browser is sent back to localhost, which the provider registered, on a port that is free.
    let redirect = asked["redirect_uri"].replace("localhost", "127.0.0.1");
    assert!(asked["redirect_uri"].starts_with("http://localhost:"));
    assert!(asked["redirect_uri"].ends_with("/callback"));
    reqwest::Client::new()
        .get(format!("{redirect}?code=the-code&state={}", asked["state"]))
        .send()
        .await
        .unwrap();
    wait_for(&router, 0, LoginState::LoggedIn).await;
    // The code was exchanged as JSON, with the state, and the refresh token is kept under the preset's name.
    assert!(provider.json_tokens());
    assert_eq!(
        provider.exchanged_state().as_deref(),
        Some(asked["state"].as_str())
    );
    let (code, _, uri) = &provider.exchanged_codes()[0];
    assert_eq!((code.as_str(), uri), ("the-code", &asked["redirect_uri"]));
    let secrets = Secrets::new(_dir.path());
    assert_eq!(
        secrets.get("oauth-anthropic").unwrap(),
        Some(provider.refresh_token(1))
    );
}

#[tokio::test]
async fn the_openai_preset_logs_in_with_a_form_and_the_codex_parameters_and_a_pasted_address() {
    let provider = FakeProvider::start(3600).await;
    let (router, dir) = router_with_preset(&provider, "openai", Protocol::OpenAi);
    let login = router.start_login(0, true).await.unwrap();
    let asked = query(&login.authorize_url.unwrap());
    assert_eq!(asked["scope"], "openid profile email offline_access");
    assert_eq!(asked["codex_cli_simplified_flow"], "true");
    assert_eq!(asked["id_token_add_organizations"], "true");
    assert!(!asked.contains_key("code"));
    // The address a browser ended on that could not reach this machine.
    let ended = format!(
        "{}?code=pasted&scope=openid&state={}",
        asked["redirect_uri"], asked["state"]
    );
    router.paste_login(0, &ended).await.unwrap();
    let login = wait_for(&router, 0, LoginState::LoggedIn).await;
    assert!(!provider.json_tokens());
    assert_eq!(provider.exchanged_state(), None);
    assert_eq!(provider.exchanged_codes()[0].0, "pasted");
    // The login holds, but the upstream needs a protocol that is not spoken: that is said, not listed.
    assert!(
        login.message.contains("Codex Responses protocol"),
        "{}",
        login.message
    );
    assert_eq!(
        Secrets::new(dir.path()).get("oauth-openai").unwrap(),
        Some(provider.refresh_token(1))
    );
}

#[tokio::test]
async fn the_openai_upstream_fails_with_the_protocol_it_needs_without_asking_the_provider() {
    let provider = FakeProvider::start(3600).await;
    let (router, _dir) = router_with_preset(&provider, "openai", Protocol::OpenAi);
    let upstreams = router.upstreams_of(Protocol::OpenAi);
    let mut request = json!({"model": "gpt-fake", "messages": []});
    let failed = router
        .send_first(
            Protocol::OpenAi,
            "/v1/chat/completions",
            &HeaderMap::new(),
            &mut request,
            &upstreams,
            (&["gpt-fake".to_string()], false),
        )
        .await
        .err()
        .expect("the request must fail");
    assert_eq!(failed.status(), axum::http::StatusCode::BAD_GATEWAY);
    let body = axum::body::to_bytes(failed.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("needs the Codex Responses protocol"),
        "{message}"
    );
    assert!(message.contains("does not speak yet"), "{message}");
    assert_eq!(provider.token_requests(), 0);
    assert!(provider.upstream_auth().is_empty());
    let listed = router.list_models(&upstreams[0]).await.unwrap_err();
    assert!(
        listed.starts_with("needs the Codex Responses protocol"),
        "{listed}"
    );
}

#[tokio::test]
async fn a_request_with_the_anthropic_token_carries_the_headers_and_the_system_prefix_pi_sends() {
    let provider = FakeProvider::start(3600).await;
    let (router, dir) = router_with_preset(&provider, "anthropic", Protocol::Anthropic);
    Secrets::new(dir.path())
        .set("oauth-anthropic", "refresh-token-0")
        .unwrap();
    let answered = send(
        &router,
        json!({"model": "claude-sonnet-fake", "system": "Be brief.", "messages": [], "max_tokens": 5}),
        &[
            ("anthropic-version", "2023-06-01"),
            ("anthropic-beta", "prompt-caching-2024-07-31, oauth-2025-04-20"),
        ],
    )
    .await;
    assert!(answered.response.status().is_success());
    let (headers, body) = provider.messages().remove(0);
    assert_eq!(
        headers["authorization"],
        format!("Bearer {}", provider.access_token(1))
    );
    assert_eq!(headers["user-agent"], "claude-cli/2.1.280");
    assert_eq!(headers["x-app"], "cli");
    assert_eq!(headers["anthropic-dangerous-direct-browser-access"], "true");
    assert_eq!(headers["anthropic-version"], "2023-06-01");
    // The harness's features stay, the provider's are added once.
    assert_eq!(
        headers["anthropic-beta"],
        "prompt-caching-2024-07-31,oauth-2025-04-20,claude-code-20250219"
    );
    assert_eq!(
        body["system"],
        json!([
            {"type": "text", "text": "You are Claude Code, Anthropic's official CLI for Claude."},
            {"type": "text", "text": "Be brief."},
        ])
    );
    assert!(!headers.contains_key("x-api-key"));

    // The access token is refreshed (as JSON) when it is due, and the next request carries the new one.
    provider.set_lifetime(0);
    router.oauth.access.lock().await.clear();
    let _ = send(&router, json!({"model": "m", "messages": []}), &[]).await;
    let _ = send(&router, json!({"model": "m", "messages": []}), &[]).await;
    assert!(provider.json_tokens());
    let sent = provider.messages();
    assert_eq!(sent.len(), 3);
    assert_ne!(sent[1].0["authorization"], sent[0].0["authorization"]);
    assert_eq!(
        sent[1].0["anthropic-beta"],
        "claude-code-20250219,oauth-2025-04-20"
    );
    assert_eq!(provider.refreshed_with()[0], "refresh-token-0");
}

#[tokio::test]
async fn a_table_names_its_preset_and_what_it_writes_wins() {
    let table = |text: &str| toml::from_str::<OAuthConfig>(text);
    let anthropic = table("preset = \"anthropic\"").unwrap();
    assert_eq!(anthropic.flow, Flow::Code);
    assert_eq!(anthropic.client_id, "9d1c250a-e61b-44d9-88ed-5944d1962f5e");
    assert_eq!(
        anthropic.token_url,
        "https://platform.claude.com/v1/oauth/token"
    );
    assert_eq!(anthropic.redirect_port, Some(53692));
    assert_eq!(anthropic.secret, "oauth-anthropic");
    assert_eq!(anthropic.problem(), None);
    let openai = table("preset = \"openai\"\nsecret = \"mine\"\nscopes = [\"openid\"]").unwrap();
    assert_eq!(openai.client_id, "app_EMoamEEZ73f0CkXaXp7hrann");
    assert_eq!(openai.redirect_path.as_deref(), Some("/auth/callback"));
    assert_eq!((openai.secret.as_str(), openai.scopes.len()), ("mine", 1));
    assert!(openai.needs().is_some() && anthropic.needs().is_none());
    // Without a preset the fields of a table are all needed.
    let missing = table("flow = \"code\"\nclient_id = \"x\"")
        .unwrap_err()
        .to_string();
    assert!(missing.contains("token_url is needed"), "{missing}");
    let unknown = table("preset = \"gemini\"").unwrap_err().to_string();
    assert!(unknown.contains("anthropic, openai"), "{unknown}");
    assert!(table("preset = \"openai\"\nnot_a_key = 1").is_err());
}
