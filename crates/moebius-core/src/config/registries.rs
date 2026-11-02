//! The package registries from `[[registries]]`: git repositories with an `index.toml` at their root that
//! `server::packages` installs packages from. The registry of Moebius's own plugins is always the first.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{ConfigError, valid_agent_name};

/// The name of the registry every server has.
pub const DEFAULT_REGISTRY: &str = "moebius-plugins";

/// Where the default registry is.
pub const DEFAULT_REGISTRY_URL: &str = "https://github.com/Code000-211/moebius-plugins";

/// A registry of packages, from `[[registries]]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryConfig {
    /// The name the registry is known by, in `moebius pkg` and in the package lock.
    pub name: String,
    /// Anything `git clone` takes, as seen from the server; a path with a directory in it is made absolute against the
    /// configuration file's directory.
    pub url: String,
}

/// The registries of a configuration: the default registry first, unless an entry named like it replaces its URL,
/// then the others in the order of the file. Names must be words and distinct.
pub(super) fn registries(
    configured: Vec<RegistryConfig>,
    base: &Path,
) -> Result<Vec<RegistryConfig>, ConfigError> {
    let mut all = vec![RegistryConfig {
        name: DEFAULT_REGISTRY.into(),
        url: DEFAULT_REGISTRY_URL.into(),
    }];
    let mut names = std::collections::BTreeSet::new();
    for mut registry in configured {
        let name = registry.name.clone();
        if !valid_agent_name(&name) {
            return Err(ConfigError::RegistryName { name });
        }
        if !names.insert(name.clone()) {
            return Err(ConfigError::RegistryTwice { name });
        }
        let url = registry.url.trim();
        if url.is_empty() || url.starts_with('-') || url.contains(['\n', '\0']) {
            return Err(ConfigError::RegistryUrl { name });
        }
        if !url.contains("://") && !url.contains('@') && Path::new(url).components().count() > 1 {
            registry.url = base.join(PathBuf::from(url)).display().to_string();
        }
        match all.iter_mut().find(|r| r.name == name) {
            Some(default) => *default = registry,
            None => all.push(registry),
        }
    }
    Ok(all)
}
