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
    // The login holds when the upstream lists the subscription's models, which the router knows.
    assert!(
        login.message.contains("lists 8 models"),
        "{}",
        login.message
    );
    assert_eq!(
        Secrets::new(dir.path()).get("oauth-openai").unwrap(),
        Some(provider.refresh_token(1))
    );
}

#[tokio::test]
async fn the_openai_upstream_goes_to_the_codex_endpoint_with_the_account_of_the_login() {
    let provider = FakeProvider::start(3600).await;
    let (router, dir) = router_with_preset(&provider, "openai", Protocol::OpenAi);
    Secrets::new(dir.path())
        .set("oauth-openai", "refresh-token-0")
        .unwrap();
    let upstreams = router.upstreams_of(Protocol::OpenAi);
    let mut request = json!({"model": "gpt-fake", "messages": []});
    // The fake provider has no Codex endpoint: the request is sent there, not refused for its protocol.
    let answered = router
        .send_first(
            Protocol::OpenAi,
            "/v1/chat/completions",
            &HeaderMap::new(),
            &mut request,
            &upstreams,
            (&["gpt-fake".to_string()], false),
        )
        .await
        .unwrap_or_else(|_| panic!("the request was not sent"));
    assert!(answered.codex);
    assert_eq!(answered.response.status(), 404);
    assert!(answered.response.url().path().ends_with("/codex/responses"));
    assert_eq!(provider.token_requests(), 1);
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
    router.oauth.forget_access().await;
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
    assert!(openai.codex() && !anthropic.codex());
    assert!(openai.needs().is_none() && anthropic.needs().is_none());
    // Without a preset the fields of a table are all needed.
    let missing = table("flow = \"code\"\nclient_id = \"x\"")
        .unwrap_err()
        .to_string();
    assert!(missing.contains("token_url is needed"), "{missing}");
    let unknown = table("preset = \"gemini\"").unwrap_err().to_string();
    assert!(unknown.contains("anthropic, openai"), "{unknown}");
    assert!(table("preset = \"openai\"\nnot_a_key = 1").is_err());
}

#[tokio::test]
async fn the_anthropic_preset_adds_the_billing_header_sanitizes_pi_prompts_and_recovers_from_an_old_version()
 {
    let provider = FakeProvider::start(3600).await;
    provider.require_claude_version("2.1.295");
    let (router, dir) = router_with_preset(&provider, "anthropic", Protocol::Anthropic);
    Secrets::new(dir.path())
        .set("oauth-anthropic", "refresh-token-0")
        .unwrap();

    let pi_bolt_prompt = "You are an expert coding assistant operating inside Pi-Bolt, a build of pi (a coding agent harness) compiled ahead of time. You help users by reading files, executing commands, editing code, and writing new files.\n\n\
        <tools>\n- read: Read file contents\n\nIn addition to the tools above, you may have access to other custom tools depending on the project.\n</tools>\n\n\
        <docs>\nPi documentation (read only when the user asks about pi itself, its SDK, extensions, themes, skills, or TUI):\n- Main documentation: /opt/pi/README.md\n</docs>\n\n\
        <cwd>\n/work\n</cwd>";
    let answered = send(
        &router,
        json!({
            "model": "claude-sonnet-fake",
            "system": pi_bolt_prompt,
            "messages": [{"role": "user", "content": "Write a poem"}],
            "max_tokens": 5
        }),
        &[],
    )
    .await;
    assert!(answered.response.status().is_success());

    let seen = provider.messages();
    assert_eq!(seen.len(), 2);
    // First attempt used the default 2.1.280 floor; the retry used the learned 2.1.295 floor.
    assert_eq!(seen[0].0["user-agent"], "claude-cli/2.1.280");
    assert_eq!(seen[1].0["user-agent"], "claude-cli/2.1.295");
    assert_eq!(seen[1].0["anthropic-version"], "2023-06-01");
    let system = seen[1].1["system"].as_array().unwrap();
    assert_eq!(system.len(), 3);
    let billing = system[0]["text"].as_str().unwrap();
    assert!(
        billing.starts_with("x-anthropic-billing-header: cc_version=2.1.295.")
            && billing.ends_with("; cc_entrypoint=sdk-cli; cch=2d0d5;"),
        "{billing}"
    );
    assert_eq!(
        system[1]["text"],
        "You are Claude Code, Anthropic's official CLI for Claude."
    );
    let sanitized = system[2]["text"].as_str().unwrap();
    assert!(sanitized.starts_with("You are an expert coding assistant.\nBe concise and helpful."));
    assert!(!sanitized.contains("Pi documentation"));
    assert!(!sanitized.contains("In addition to the tools above"));
    assert!(sanitized.contains("<cwd>\n/work\n</cwd>"));

    // The next request starts at 2.1.295 without another 400 retry.
    let answered = send(
        &router,
        json!({
            "model": "claude-sonnet-fake",
            "messages": [{"role": "user", "content": "Write a poem"}],
            "max_tokens": 5
        }),
        &[],
    )
    .await;
    assert!(answered.response.status().is_success());
    assert_eq!(provider.messages().len(), 3);
}

#[test]
fn billing_header_matches_reference_hashes_and_utf16_sampling() {
    use super::anthropic::billing_header;
    assert_eq!(
        billing_header("Write a poem", "2.1.280"),
        "x-anthropic-billing-header: cc_version=2.1.280.31b; cc_entrypoint=sdk-cli; cch=2d0d5;"
    );
    let short = billing_header("Hi", "2.1.280");
    assert!(
        short.starts_with("x-anthropic-billing-header: cc_version=2.1.280."),
        "{short}"
    );
    assert!(
        short.ends_with("; cc_entrypoint=sdk-cli; cch=3639e;"),
        "{short}"
    );
}

#[test]
fn sanitizes_pi_and_pi_bolt_system_prompts_preserving_context_and_skills() {
    use super::anthropic::{MINIMAL_ANTHROPIC_OAUTH_PROMPT, sanitize_system_prompt};
    for identity in [
        "pi, a coding agent harness",
        "Pi-Bolt, a build of pi (a coding agent harness) compiled ahead of time",
    ] {
        let prompt = format!(
            "You are an expert coding assistant operating inside {identity}. \
             You help users by reading files, executing commands, editing code, and writing new files.\n\n\
             <tools>\n\
             - read: Read file contents\n\
             - bash: Execute a bash command\n\n\
             In addition to the tools above, you may have access to other custom tools depending on the project.\n\
             </tools>\n\n\
             <rules>\n\
             - Here is some useful information about the environment you are running in:\n\
             - Be concise in your responses\n\
             </rules>\n\n\
             <docs>\n\
             Pi documentation (read only when the user asks about pi itself, its SDK, extensions, themes, skills, or TUI):\n\
             - Main documentation: /opt/pi/README.md\n\
             </docs>\n\n\
             <addendum>\n\
             Keep answers factual.\n\
             </addendum>\n\n\
             <project_context>\n\
             Project-specific instructions and guidelines:\n\n\
             <project_instructions path=\"/repo/AGENTS.md\">\n\
             Run tests before finishing.\n\
             </project_instructions>\n\
             </project_context>\n\n\
             <skills>\n\
             <available_skills>\n\
             <skill><name>moebius</name></skill>\n\
             </available_skills>\n\
             </skills>\n\n\
             <cwd>\n\
             /repo\n\
             </cwd>"
        );
        let sanitized = sanitize_system_prompt(&prompt);
        assert!(sanitized.starts_with(MINIMAL_ANTHROPIC_OAUTH_PROMPT));
        assert!(!sanitized.contains("operating inside"));
        assert!(!sanitized.contains("In addition to the tools above"));
        assert!(!sanitized.contains("<docs>"));
        assert!(!sanitized.contains("Pi documentation"));
        assert!(sanitized.contains("- Environment context you are running in:"));
        assert!(sanitized.contains("<addendum>\nKeep answers factual.\n</addendum>"));
        assert!(sanitized.contains("<project_instructions path=\"/repo/AGENTS.md\">"));
        assert!(sanitized.contains("<skill><name>moebius</name></skill>"));
        assert!(sanitized.contains("<cwd>\n/repo\n</cwd>"));
    }

    let custom = "You are a specialized code reviewer.\n\n<docs>\nPi documentation (read only when the user asks about pi itself): keep\n</docs>";
    assert_eq!(sanitize_system_prompt(custom), custom);
}

#[test]
fn shapes_system_blocks_and_mid_conversation_section_updates() {
    use super::anthropic::{
        CLAUDE_CODE_PREFIX, MINIMAL_ANTHROPIC_OAUTH_PROMPT, required_version_from_error,
        shape_request,
    };
    let mut request = json!({
        "model": "m",
        "system": [
            {
                "type": "text",
                "text": "You are an expert coding assistant operating inside Pi-Bolt, a build of pi (a coding agent harness) compiled ahead of time.\n\n<tools>\n- read: Read\n\nIn addition to the tools above, custom tools.\n</tools>\n\n<docs>\nPi documentation (read only when the user asks about pi itself, its SDK, extensions, themes, skills, or TUI):\n- Main: /readme\n</docs>",
                "cache_control": {"type": "ephemeral"}
            }
        ],
        "messages": [
            {"role": "user", "content": "Write a poem"},
            {
                "role": "system",
                "content": [
                    {"type": "text", "text": "Updated system prompt section \"docs\":\n\n<docs>\nPi documentation (read only when the user asks about pi itself): /readme\n</docs>"}
                ]
            },
            {
                "role": "system",
                "content": [
                    {"type": "text", "text": "Removed system prompt section \"docs\"."}
                ],
                "output_config": {"effort": "high"}
            },
            {
                "role": "system",
                "content": [
                    {"type": "text", "text": "Updated system prompt section \"tools\":\n\n<tools>\n- bash: Run\n\nIn addition to the tools above, more.\n</tools>"}
                ]
            }
        ]
    });

    shape_request(&mut request, "2.1.280");
    let system = request["system"].as_array().unwrap();
    assert_eq!(system.len(), 3);
    assert_eq!(
        system[0],
        json!({
            "type": "text",
            "text": "x-anthropic-billing-header: cc_version=2.1.280.31b; cc_entrypoint=sdk-cli; cch=2d0d5;"
        })
    );
    assert!(system[0].get("cache_control").is_none());
    assert_eq!(
        system[1],
        json!({"type": "text", "text": CLAUDE_CODE_PREFIX})
    );
    assert_eq!(system[2]["cache_control"]["type"], "ephemeral");
    let body_text = system[2]["text"].as_str().unwrap();
    assert!(body_text.starts_with(MINIMAL_ANTHROPIC_OAUTH_PROMPT));
    assert!(!body_text.contains("<docs>"));
    assert!(!body_text.contains("In addition to the tools above"));

    let messages = request["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1]["content"], json!([]));
    assert_eq!(messages[1]["output_config"]["effort"], "high");
    assert_eq!(
        messages[2]["content"][0]["text"],
        "Updated system prompt section \"tools\":\n\n<tools>\n- bash: Run\n</tools>"
    );

    let err = json!({
        "type": "error",
        "error": {
            "type": "invalid_request_error",
            "message": "This version of Claude Code is no longer supported. Version 2.1.295 or newer is required.",
            "details": {"error_code": "claude_code_version_too_old"}
        }
    });
    let bytes = serde_json::to_vec(&err).unwrap();
    assert_eq!(
        required_version_from_error(&bytes, "2.1.280").as_deref(),
        Some("2.1.295")
    );
    assert_eq!(required_version_from_error(&bytes, "2.1.295"), None);
    assert_eq!(required_version_from_error(&bytes, "2.2.0"), None);
}
