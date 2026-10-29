//! The API keys typed in the app, kept in `<data_dir>/secrets.toml`, and the keys the router's upstreams and the
//! decision model are given, from the environment or from there.

use std::path::{Path, PathBuf};

use super::{invalid, write_atomically};
use crate::app::Result;
use crate::config::{Config, valid_agent_name};
use crate::router::Upstream;

/// The API keys typed in the app: `<data_dir>/secrets.toml`, a table of name and key, readable by the owner only.
/// A key leaves this file only to be given to the router or the decision model, never to a client, the store or a log.
#[derive(Debug, Clone)]
pub struct Secrets {
    path: PathBuf,
}

impl Secrets {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("secrets.toml"),
        }
    }

    fn read(&self) -> Result<toml::Table> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|_| invalid(format!("{} is not valid TOML", self.path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// The names of the stored keys, never the keys.
    pub fn names(&self) -> Result<Vec<String>> {
        Ok(self.read()?.keys().cloned().collect())
    }

    /// The key stored under `name`.
    pub fn get(&self, name: &str) -> Result<Option<String>> {
        Ok(self
            .read()?
            .get(name)
            .and_then(toml::Value::as_str)
            .map(str::to_string))
    }

    /// Stores `value` under `name`, replacing a key stored before.
    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        if !valid_agent_name(name) {
            return Err(invalid(format!(
                "{name:?} is not a name (letters, digits, - and _)"
            )));
        }
        if value.trim().is_empty() {
            return Err(invalid("the key is empty"));
        }
        let mut secrets = self.read()?;
        secrets.insert(
            name.to_string(),
            toml::Value::String(value.trim().to_string()),
        );
        self.write(&secrets)
    }

    /// Removes the key stored under `name`; false when there was none.
    pub fn remove(&self, name: &str) -> Result<bool> {
        let mut secrets = self.read()?;
        let had = secrets.remove(name).is_some();
        if had {
            self.write(&secrets)?;
        }
        Ok(had)
    }

    fn write(&self, secrets: &toml::Table) -> Result<()> {
        let text = toml::to_string(secrets).map_err(|e| invalid(e.to_string()))?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Ok(write_atomically(&self.path, text.as_bytes(), true)?)
    }

    /// The key an upstream or the decision model is given: from the environment variable `env`, or from the secret
    /// `secret`, or none. A variable that is not set or a secret that is not stored is an error.
    pub fn resolve(
        &self,
        env: Option<&str>,
        secret: Option<&str>,
    ) -> std::result::Result<Option<String>, String> {
        match (env, secret) {
            (Some(var), _) => std::env::var(var)
                .map(Some)
                .map_err(|_| format!("the environment variable {var} is not set")),
            (None, Some(name)) => match self.get(name) {
                Ok(Some(key)) => Ok(Some(key)),
                Ok(None) => Err(format!("the secret {name} is not stored")),
                Err(e) => Err(e.to_string()),
            },
            (None, None) => Ok(None),
        }
    }

    /// The router's upstreams of `config`, each with its key.
    pub fn upstreams(&self, config: &Config) -> std::result::Result<Vec<Upstream>, String> {
        config
            .upstreams
            .iter()
            .map(|upstream| {
                Ok(Upstream {
                    protocol: upstream.protocol,
                    base_url: upstream.base_url.clone(),
                    api_key: self.resolve(
                        upstream.api_key_env.as_deref(),
                        upstream.api_key_secret.as_deref(),
                    )?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppError;

    #[test]
    fn a_secret_is_stored_privately_replaced_and_removed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::new(dir.path());
        assert_eq!(secrets.names().unwrap(), Vec::<String>::new());
        secrets.set("anthropic", "sk-first").unwrap();
        secrets.set("local", "second").unwrap();
        secrets.set("anthropic", "sk-third").unwrap();
        let path = dir.path().join("secrets.toml");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(secrets.names().unwrap(), ["anthropic", "local"]);
        assert_eq!(
            secrets.get("anthropic").unwrap().as_deref(),
            Some("sk-third")
        );
        assert!(secrets.remove("local").unwrap());
        assert!(!secrets.remove("local").unwrap());
        assert_eq!(secrets.names().unwrap(), ["anthropic"]);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(matches!(secrets.set("a b", "x"), Err(AppError::Invalid(_))));
        assert!(matches!(secrets.set("ok", "  "), Err(AppError::Invalid(_))));
        // No temporary file is left.
        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn a_key_comes_from_the_environment_or_a_secret_and_is_missing_with_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::new(dir.path());
        secrets.set("k", "typed").unwrap();
        assert_eq!(
            secrets.resolve(None, Some("k")).unwrap().as_deref(),
            Some("typed")
        );
        assert_eq!(secrets.resolve(None, None).unwrap(), None);
        assert!(
            secrets
                .resolve(None, Some("nope"))
                .unwrap_err()
                .contains("not stored")
        );
        assert!(
            secrets
                .resolve(Some("MOEBIUS_SURELY_UNSET_VARIABLE"), None)
                .unwrap_err()
                .contains("not set")
        );
    }
}
