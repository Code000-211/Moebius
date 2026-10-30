//! The `oauth` table of an upstream as it is configured: the flow, the client id and the provider's URLs, filled in
//! from the built-in provider its `preset` names where the table does not say.

use serde::{Deserialize, Serialize};

use super::{Preset, TERMS_WARNING, preset};
use crate::config::valid_agent_name;

/// How a login is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    /// The device authorization grant: a code the person types at the provider's page.
    Device,
    /// An authorization code with PKCE, the browser sent back to a loopback address.
    Code,
}

/// The `oauth` table of an upstream: how it logs in. Not set together with a key. What is not written is filled in
/// from the `preset` it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawOAuth")]
pub struct OAuthConfig {
    /// The built-in provider the table was filled in from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub flow: Flow,
    pub client_id: String,
    /// Where codes and refresh tokens are exchanged for tokens.
    pub token_url: String,
    /// Where a device login is started; needed by `device`.
    pub device_url: Option<String>,
    /// The provider's page a `code` login opens in the browser.
    pub authorize_url: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    /// The loopback port the browser is sent back to; any free one when not set.
    pub redirect_port: Option<u16>,
    /// The path the browser is sent back to; `/callback` when not set.
    pub redirect_path: Option<String>,
    /// The name the refresh token is kept under in `secrets.toml`.
    pub secret: String,
}

/// The `oauth` table as it is written, before a preset fills in what is missing.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOAuth {
    preset: Option<String>,
    flow: Option<Flow>,
    client_id: Option<String>,
    token_url: Option<String>,
    device_url: Option<String>,
    authorize_url: Option<String>,
    scopes: Option<Vec<String>>,
    redirect_port: Option<u16>,
    redirect_path: Option<String>,
    secret: Option<String>,
}

impl TryFrom<RawOAuth> for OAuthConfig {
    type Error = String;

    fn try_from(raw: RawOAuth) -> Result<Self, String> {
        let built_in = match &raw.preset {
            Some(name) => Some(preset::find(name).ok_or_else(|| {
                format!("preset: {name:?} is not a provider ({})", preset::names())
            })?),
            None => None,
        };
        let missing = |key: &str| format!("{key} is needed (or a preset that has it)");
        Ok(Self {
            flow: raw
                .flow
                .or(built_in.map(|p| p.flow))
                .ok_or_else(|| missing("flow"))?,
            client_id: raw
                .client_id
                .or(built_in.map(|p| p.client_id.to_string()))
                .ok_or_else(|| missing("client_id"))?,
            token_url: raw
                .token_url
                .or(built_in.map(|p| p.token_url.to_string()))
                .ok_or_else(|| missing("token_url"))?,
            device_url: raw.device_url,
            authorize_url: raw
                .authorize_url
                .or(built_in.map(|p| p.authorize_url.to_string())),
            scopes: raw.scopes.unwrap_or_else(|| {
                built_in.map_or(Vec::new(), |p| {
                    p.scopes.iter().map(|s| s.to_string()).collect()
                })
            }),
            redirect_port: raw.redirect_port.or(built_in.map(|p| p.redirect_port)),
            redirect_path: raw
                .redirect_path
                .or(built_in.map(|p| p.redirect_path.to_string())),
            secret: raw
                .secret
                .or(built_in.map(|p| format!("oauth-{}", p.name)))
                .ok_or_else(|| missing("secret"))?,
            preset: raw.preset,
        })
    }
}

impl OAuthConfig {
    /// The built-in provider the table names, if it does.
    pub(in crate::router) fn preset(&self) -> Option<&'static Preset> {
        self.preset.as_deref().and_then(preset::find)
    }

    /// Whether codes and refresh tokens are exchanged with a JSON body instead of a form.
    pub(super) fn json_tokens(&self) -> bool {
        self.preset().is_some_and(|p| p.json_tokens)
    }

    /// What the person confirms before a login of this upstream starts: the terms warning, and for a built-in
    /// provider that Moebius uses another app's client id.
    pub fn warning(&self) -> String {
        match self.preset() {
            Some(p) => format!(
                "Moebius logs in with the client id of {}, which is another app's, the way Pi does, and {} may \
                 block an account that logs in like this. {TERMS_WARNING}",
                p.client_app, p.company
            ),
            None => TERMS_WARNING.to_string(),
        }
    }

    /// What the upstream needs that the router does not speak, if anything.
    pub fn needs(&self) -> Option<&'static str> {
        self.preset().and_then(|p| p.needs)
    }

    /// What is wrong with the table, if anything.
    pub fn problem(&self) -> Option<String> {
        let url = |name: &str, value: Option<&String>| match value {
            Some(url) if url.starts_with("http://") || url.starts_with("https://") => None,
            Some(url) => Some(format!("{name} must be an http or https URL, not {url:?}")),
            None => Some(format!("{name} is needed by the {} flow", self.flow_name())),
        };
        if self.client_id.trim().is_empty() {
            return Some("client_id must not be empty".into());
        }
        if !valid_agent_name(&self.secret) {
            return Some(format!(
                "secret: {:?} is not a name (letters, digits, - and _)",
                self.secret
            ));
        }
        if let Some(problem) = url("token_url", Some(&self.token_url)) {
            return Some(problem);
        }
        if self
            .redirect_path
            .as_ref()
            .is_some_and(|path| !path.starts_with('/'))
        {
            return Some("redirect_path must start with /".into());
        }
        match self.flow {
            Flow::Device => url("device_url", self.device_url.as_ref()),
            Flow::Code => url("authorize_url", self.authorize_url.as_ref()),
        }
    }

    fn flow_name(&self) -> &'static str {
        match self.flow {
            Flow::Device => "device",
            Flow::Code => "code",
        }
    }

    pub(super) fn scope(&self) -> Option<String> {
        (!self.scopes.is_empty()).then(|| self.scopes.join(" "))
    }
}
