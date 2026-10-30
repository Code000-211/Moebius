//! The providers whose subscription Moebius can log in to without the person writing the provider's client id and
//! URLs: one entry for each in [`PRESETS`], named by `preset` in an upstream's `oauth` table. The values are those of
//! Pi (commit `f09d01b85e0384647fcab6779d35ab9dabe04be3` of `badlogic/pi-mono`, `packages/ai/src/auth/oauth/` and
//! `packages/ai/src/api/anthropic-messages.ts`), whose client ids are the ones of Claude Code and of the Codex CLI.
//! They are public values, not secrets.
//!
//! A preset also says what the provider needs of the requests sent with its token: the headers and the system prompt
//! prefix that make an Anthropic subscription token accepted, or the protocol an upstream needs that the router does
//! not speak.

use reqwest::RequestBuilder;
use serde::Serialize;
use serde_json::{Value, json};

use super::Flow;
use crate::router::Protocol;

/// What the system prompt of a request with an Anthropic subscription token must start with: the subscription is
/// granted to Claude Code, and the API refuses its token for a request that does not say so.
const CLAUDE_CODE_PREFIX: &str = "You are Claude Code, Anthropic's official CLI for Claude.";

/// One provider.
pub(in crate::router) struct Preset {
    pub name: &'static str,
    /// What the Models page calls it.
    pub title: &'static str,
    /// The company, and the app whose client id the login uses.
    pub company: &'static str,
    pub client_app: &'static str,
    /// The protocol and base URL of the upstream that uses the login.
    pub protocol: Protocol,
    pub base_url: &'static str,
    pub flow: Flow,
    pub client_id: &'static str,
    pub authorize_url: &'static str,
    pub token_url: &'static str,
    pub scopes: &'static [&'static str],
    /// The loopback address the provider registered: the host the browser is sent back to, the port and the path.
    pub redirect_host: &'static str,
    pub redirect_port: u16,
    pub redirect_path: &'static str,
    /// Whether the provider takes any other loopback port when the registered one is in use.
    pub any_port: bool,
    /// Parameters added to the authorization URL.
    pub authorize_params: &'static [(&'static str, &'static str)],
    /// Whether codes and refresh tokens are exchanged with a JSON body instead of a form.
    pub json_tokens: bool,
    /// Headers added to every request sent with the token.
    pub headers: &'static [(&'static str, &'static str)],
    /// Features added to the `anthropic-beta` header of every request sent with the token.
    pub beta: &'static [&'static str],
    /// Text the system prompt of every request sent with the token starts with.
    pub system_prefix: Option<&'static str>,
    /// What the upstream needs that the router does not speak; its requests fail with it as the reason.
    pub needs: Option<&'static str>,
}

/// The presets, in the order the Models page offers them.
pub(in crate::router) const PRESETS: &[Preset] = &[
    Preset {
        name: "anthropic",
        title: "Anthropic (Claude Pro/Max)",
        company: "Anthropic",
        client_app: "Claude Code",
        protocol: Protocol::Anthropic,
        base_url: "https://api.anthropic.com",
        flow: Flow::Code,
        client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
        authorize_url: "https://claude.ai/oauth/authorize",
        token_url: "https://platform.claude.com/v1/oauth/token",
        scopes: &[
            "org:create_api_key",
            "user:profile",
            "user:inference",
            "user:sessions:claude_code",
            "user:mcp_servers",
            "user:file_upload",
        ],
        redirect_host: "localhost",
        redirect_port: 53692,
        redirect_path: "/callback",
        any_port: true,
        authorize_params: &[("code", "true")],
        json_tokens: true,
        headers: &[
            ("accept", "application/json"),
            ("anthropic-dangerous-direct-browser-access", "true"),
            ("user-agent", "claude-cli/2.1.280"),
            ("x-app", "cli"),
        ],
        beta: &["claude-code-20250219", "oauth-2025-04-20"],
        system_prefix: Some(CLAUDE_CODE_PREFIX),
        needs: None,
    },
    Preset {
        name: "openai",
        title: "OpenAI (ChatGPT Plus/Pro)",
        company: "OpenAI",
        client_app: "the Codex CLI",
        protocol: Protocol::OpenAi,
        base_url: "https://chatgpt.com/backend-api",
        flow: Flow::Code,
        client_id: "app_EMoamEEZ73f0CkXaXp7hrann",
        authorize_url: "https://auth.openai.com/oauth/authorize",
        token_url: "https://auth.openai.com/oauth/token",
        scopes: &["openid", "profile", "email", "offline_access"],
        redirect_host: "localhost",
        redirect_port: 1455,
        redirect_path: "/auth/callback",
        any_port: false,
        authorize_params: &[
            ("id_token_add_organizations", "true"),
            ("codex_cli_simplified_flow", "true"),
            ("originator", "moebius"),
        ],
        json_tokens: false,
        headers: &[],
        beta: &[],
        system_prefix: None,
        needs: Some(
            "the Codex Responses protocol of the ChatGPT backend (POST /codex/responses with the \
             chatgpt-account-id header)",
        ),
    },
];

/// A provider as the Models page offers it (`GET /v1/models/providers`): what to write to log in to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Provider {
    /// The `preset` of its `oauth` table.
    pub name: &'static str,
    pub title: &'static str,
    /// The protocol and base URL of the upstream that logs in.
    pub protocol: Protocol,
    pub base_url: &'static str,
    /// What the upstream needs that the router does not speak yet, if anything: its requests fail.
    pub needs: Option<&'static str>,
}

/// The built-in providers, in the order the Models page offers them.
pub fn providers() -> Vec<Provider> {
    PRESETS
        .iter()
        .map(|preset| Provider {
            name: preset.name,
            title: preset.title,
            protocol: preset.protocol,
            base_url: preset.base_url,
            needs: preset.needs,
        })
        .collect()
}

/// The preset called `name`.
pub(in crate::router) fn find(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.name == name)
}

/// The names of the presets, for a message.
pub(super) fn names() -> String {
    PRESETS
        .iter()
        .map(|preset| preset.name)
        .collect::<Vec<_>>()
        .join(", ")
}

impl Preset {
    /// Puts the preset's text at the start of the system prompt of `request`, unless it is already there.
    pub(in crate::router) fn shape(&self, request: &mut Value) {
        let Some(prefix) = self.system_prefix else {
            return;
        };
        let block = json!({"type": "text", "text": prefix});
        let mut blocks = match request.get_mut("system").map(Value::take) {
            Some(Value::String(text)) if !text.is_empty() => {
                vec![json!({"type": "text", "text": text})]
            }
            Some(Value::Array(blocks)) => blocks,
            _ => Vec::new(),
        };
        if blocks.first() != Some(&block) {
            blocks.insert(0, block);
        }
        request["system"] = Value::Array(blocks);
    }

    /// Puts the preset's headers on `request`; its `anthropic-beta` features come after those of the harness
    /// (`harness_beta`, the header it sent), without repeating one.
    pub(in crate::router) fn dress(
        &self,
        mut request: RequestBuilder,
        harness_beta: Option<&str>,
    ) -> RequestBuilder {
        for (name, value) in self.headers {
            request = request.header(*name, *value);
        }
        if !self.beta.is_empty() {
            let mut features: Vec<&str> = harness_beta
                .into_iter()
                .flat_map(|beta| beta.split(','))
                .map(str::trim)
                .filter(|feature| !feature.is_empty())
                .collect();
            for feature in self.beta {
                if !features.contains(feature) {
                    features.push(feature);
                }
            }
            request = request.header("anthropic-beta", features.join(","));
        }
        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_presets_are_complete_and_named_once() {
        for preset in PRESETS {
            assert!(preset.authorize_url.starts_with("https://"));
            assert!(preset.token_url.starts_with("https://"));
            assert!(!preset.client_id.is_empty());
            assert!(preset.redirect_path.starts_with('/'));
            assert_eq!(find(preset.name).unwrap().name, preset.name);
        }
        assert_eq!(names(), "anthropic, openai");
        let providers = providers();
        assert_eq!(providers[0].base_url, "https://api.anthropic.com");
        assert_eq!(providers[1].protocol, Protocol::OpenAi);
        assert!(providers[0].needs.is_none() && providers[1].needs.is_some());
    }

    #[test]
    fn the_prefix_comes_first_in_a_system_prompt_of_any_shape_and_only_once() {
        let anthropic = find("anthropic").unwrap();
        let block = json!({"type": "text", "text": CLAUDE_CODE_PREFIX});
        let mut none = json!({"model": "m"});
        anthropic.shape(&mut none);
        assert_eq!(none["system"], json!([block]));
        let mut text = json!({"system": "Be brief."});
        anthropic.shape(&mut text);
        assert_eq!(
            text["system"],
            json!([block, {"type": "text", "text": "Be brief."}])
        );
        anthropic.shape(&mut text);
        assert_eq!(text["system"].as_array().unwrap().len(), 2);
        let mut blocks = json!({"system": [{"type": "text", "text": "A", "cache_control": {"type": "ephemeral"}}]});
        anthropic.shape(&mut blocks);
        assert_eq!(blocks["system"][0], block);
        assert_eq!(blocks["system"][1]["cache_control"]["type"], "ephemeral");
        // A preset without a prefix leaves the request alone.
        let mut other = json!({"system": "x"});
        find("openai").unwrap().shape(&mut other);
        assert_eq!(other, json!({"system": "x"}));
    }
}
