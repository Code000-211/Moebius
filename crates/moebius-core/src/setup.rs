//! What the Models and Plugins pages of the desktop app show: how the server is set up, read from its configuration and
//! its plugin host. Nothing here is a secret: a key is told as set or not, and a URL has no credentials in it. The
//! Settings page reads and writes the configuration itself (see `settings`).

use serde::Serialize;

use crate::board::AgentInfo;
use crate::config::Config;
use crate::decide::Decisions;
use crate::router::Protocol;

/// `url` without the user name and password in front of its host, if it has any.
pub fn without_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}{tail}")
}

/// An upstream of the router.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpstreamView {
    pub protocol: Protocol,
    pub base_url: String,
    /// Whether the upstream has a key; the key itself is never shown.
    pub api_key_set: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TeacherView {
    pub model: String,
    pub protocol: Protocol,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionModelView {
    pub url: String,
    pub model: String,
    pub timeout_seconds: u64,
    pub api_key_set: bool,
}

/// The answer of `GET /v1/models`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Models {
    pub upstreams: Vec<UpstreamView>,
    pub agents: Vec<AgentInfo>,
    pub teacher: Option<TeacherView>,
    pub decision_model: Option<DecisionModelView>,
}

pub fn models(config: &Config, agents: Vec<AgentInfo>) -> Models {
    Models {
        upstreams: config
            .upstreams
            .iter()
            .map(|upstream| UpstreamView {
                protocol: upstream.protocol,
                base_url: without_credentials(&upstream.base_url),
                api_key_set: upstream.api_key_env.is_some() || upstream.api_key_secret.is_some(),
            })
            .collect(),
        agents,
        teacher: config.teacher.as_ref().map(|teacher| TeacherView {
            model: teacher.model.clone(),
            protocol: teacher.protocol,
            timeout_seconds: teacher.timeout.as_secs(),
        }),
        decision_model: config
            .decision_model
            .as_ref()
            .map(|model| DecisionModelView {
                url: without_credentials(&model.url),
                model: model.model.clone(),
                timeout_seconds: model.timeout.as_secs(),
                api_key_set: model.api_key_env.is_some() || model.api_key_secret.is_some(),
            }),
    }
}

/// A plugin the host has loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginView {
    pub name: String,
    pub version: String,
    pub kinds: Vec<moebius_plugin_api::Kind>,
    /// The kinds of decision it answers, those the decision model took over included.
    pub decides: Vec<String>,
}

/// The answer of `GET /v1/plugins`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plugins {
    /// How long a call into any plugin may take before it counts as refused.
    pub time_limit_seconds: u64,
    pub plugins: Vec<PluginView>,
}

pub fn plugins(decisions: &Decisions) -> Plugins {
    let routes = decisions.routes();
    Plugins {
        time_limit_seconds: decisions.host().limit().as_secs(),
        plugins: decisions
            .host()
            .manifests()
            .into_iter()
            .map(|manifest| PluginView {
                decides: routes
                    .iter()
                    .filter(|(_, plugin)| **plugin == manifest.name)
                    .map(|(kind, _)| kind.clone())
                    .collect(),
                name: manifest.name,
                version: manifest.version,
                kinds: manifest.kinds,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_in_a_url_are_removed() {
        for (url, expected) in [
            (
                "https://user:hunter2@example.com/repo.git",
                "https://example.com/repo.git",
            ),
            ("https://token@example.com", "https://example.com"),
            ("http://a:b@host:8080/x?y=z@w", "http://host:8080/x?y=z@w"),
            (
                "git@github.com:acme/harness.git",
                "git@github.com:acme/harness.git",
            ),
            ("/srv/git/harness", "/srv/git/harness"),
        ] {
            assert_eq!(without_credentials(url), expected);
        }
    }
}
