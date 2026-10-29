//! The connection settings: the server's address and the token, the servers connected to before and the SSH
//! preferences, kept in the platform's config directory (`<config>/moebius/desktop.toml`) where only the person can read
//! them. A server reached over SSH is remembered by its host name alone: its token is read over SSH at every connection
//! and never saved.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where the app finds its server and how it proves who it is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Settings {
    pub address: String,
    pub token: String,
}

/// A server connected to before: one at an address with its own token, or one reached over SSH by a host name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl Server {
    pub fn ssh(host: &str) -> Server {
        Server {
            host: Some(host.to_string()),
            address: None,
            token: None,
        }
    }

    /// What the sidebar calls it: the SSH host, else the address without its scheme.
    pub fn name(&self) -> String {
        match (&self.host, &self.address) {
            (Some(host), _) => host.clone(),
            (None, Some(address)) => display_address(address),
            (None, None) => String::new(),
        }
    }
}

/// An address as a name: without `http://`.
pub fn display_address(address: &str) -> String {
    address
        .split_once("://")
        .map_or(address, |(_, rest)| rest)
        .to_string()
}

/// Everything in the settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    /// The last server connected to by address, and its token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// The SSH host connected to last, which the app opens again at the next start; none when the last was by address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ssh: Option<String>,
    /// The SSH configuration file the host list reads besides `~/.ssh/config`, and where a new host is written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_config: Option<PathBuf>,
    /// The `ssh` program to run instead of the one on the `PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_program: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("cannot read or write {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not valid: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

/// The settings file of this person, if the platform has a config directory.
pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("moebius").join("desktop.toml"))
}

/// Everything in the file; empty when there is none.
pub fn load_all(path: &Path) -> Result<Stored, SettingsError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Stored::default()),
        Err(source) => {
            return Err(SettingsError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    toml::from_str(&text).map_err(|source| SettingsError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// Reads the settings; `None` when there is no file yet or it names no server by address.
pub fn load(path: &Path) -> Result<Option<Settings>, SettingsError> {
    let stored = load_all(path)?;
    Ok(match (stored.address, stored.token) {
        (Some(address), Some(token)) => Some(Settings { address, token }),
        _ => None,
    })
}

/// Writes the settings of a server by address, and remembers the server.
pub fn save(path: &Path, settings: &Settings) -> Result<(), SettingsError> {
    update(path, |stored| {
        stored.address = Some(settings.address.clone());
        stored.token = Some(settings.token.clone());
        stored.last_ssh = None;
        let known = stored
            .servers
            .iter_mut()
            .find(|server| server.address.as_deref() == Some(&settings.address));
        match known {
            Some(server) => server.token = Some(settings.token.clone()),
            None => stored.servers.push(Server {
                host: None,
                address: Some(settings.address.clone()),
                token: Some(settings.token.clone()),
            }),
        }
    })
}

/// Remembers that the app connected to `host` over SSH, to open it again at the next start.
pub fn save_ssh(path: &Path, host: &str) -> Result<(), SettingsError> {
    update(path, |stored| {
        stored.last_ssh = Some(host.to_string());
        if !stored
            .servers
            .iter()
            .any(|server| server.host.as_deref() == Some(host))
        {
            stored.servers.push(Server::ssh(host));
        }
    })
}

/// Reads the file, lets `change` edit it and writes it back, readable by the person only (mode 0600 on Unix).
pub fn update(path: &Path, change: impl FnOnce(&mut Stored)) -> Result<(), SettingsError> {
    let mut stored = load_all(path)?;
    change(&mut stored);
    write(path, &stored)
}

fn write(path: &Path, stored: &Stored) -> Result<(), SettingsError> {
    let io = |source| SettingsError::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let text = toml::to_string(stored).expect("settings are plain strings");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(io)?;
    // A file made earlier with wider permissions keeps them when it is opened, so set them before the token goes in.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(io)?;
    }
    file.write_all(text.as_bytes()).map_err(io)
}

/// What the person typed as an address, made into a base URL: `http://` when no scheme is given, no trailing slash.
pub fn normalize_address(typed: &str) -> String {
    let typed = typed.trim().trim_end_matches('/');
    if typed.contains("://") {
        typed.to_string()
    } else {
        format!("http://{typed}")
    }
}

/// A token as it may be shown after it is saved: only its last four characters.
pub fn mask(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 12 {
        "••••".to_string()
    } else {
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("••••{tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            address: "http://10.0.0.5:7171".into(),
            token: "0123456789abcdef0123456789abcdef".into(),
        }
    }

    #[test]
    fn settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("moebius").join("desktop.toml");
        assert_eq!(load(&path).unwrap(), None);
        save(&path, &settings()).unwrap();
        assert_eq!(load(&path).unwrap(), Some(settings()));
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_by_the_person_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("desktop.toml");
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        save(&path, &settings()).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_broken_file_is_an_error_not_an_empty_setting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("desktop.toml");
        std::fs::write(&path, "address = 3").unwrap();
        assert!(matches!(load(&path), Err(SettingsError::Parse { .. })));
    }

    #[test]
    fn servers_are_remembered_and_ssh_hosts_keep_no_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("desktop.toml");
        save(&path, &settings()).unwrap();
        save_ssh(&path, "build").unwrap();
        save_ssh(&path, "build").unwrap();
        let mut other = settings();
        other.token = "another-token-another-token".into();
        save(&path, &other).unwrap();
        let stored = load_all(&path).unwrap();
        assert_eq!(stored.last_ssh, None);
        assert_eq!(stored.servers.len(), 2);
        assert_eq!(
            stored.servers[0].token.as_deref(),
            Some(other.token.as_str())
        );
        assert_eq!(stored.servers[1], Server::ssh("build"));
        assert_eq!(stored.servers[1].name(), "build");
        assert_eq!(stored.servers[0].name(), "10.0.0.5:7171");
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("token =").count(), 2, "{text}");
        save_ssh(&path, "build").unwrap();
        assert_eq!(load_all(&path).unwrap().last_ssh.as_deref(), Some("build"));
    }

    #[test]
    fn addresses_are_made_into_base_urls() {
        assert_eq!(
            normalize_address(" 10.0.0.5:7171/ "),
            "http://10.0.0.5:7171"
        );
        assert_eq!(normalize_address("http://host:1"), "http://host:1");
    }

    #[test]
    fn a_token_is_never_shown_in_full() {
        let shown = mask(&settings().token);
        assert_eq!(shown, "••••cdef");
        assert_eq!(mask("short"), "••••");
    }
}
