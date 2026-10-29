//! Editing the server's configuration while it runs: the desktop app changes `moebius.toml` through
//! `PUT /v1/settings` and keeps API keys it is given in `<data_dir>/secrets.toml`.
//!
//! A change is applied to the text of the file with `toml_edit`, so comments and order stay; the result must pass the
//! same `Config::parse` the server uses at start, else nothing is written. [`Live`] is the configuration the running
//! server reads: the keys marked live in `keys::KEYS` take effect at once (the board engine, the router, the schedule
//! and the agents read them from it), every other key is written and used after a restart, which the answer says.
//! `POST /v1/harness/setup` edits the file the same way when it moves the agents and rules into a new harness
//! configuration repository.
//!
//! `edit` makes a change to the file's text, `keys` lists every key with its default and whether it is live, and
//! reads their values, `secrets` keeps the API keys, and `live` is the running configuration and what saves it.

mod edit;
mod keys;
mod live;
mod secrets;

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app::{AppError, Result};
use crate::blobs::redact;

pub use edit::edit;
pub use keys::values;
pub use live::{Live, SettingsView};
pub use secrets::Secrets;

/// What `PUT /v1/settings` takes: values to set and keys or tables to remove, by their dotted names.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Changes {
    /// A dotted key and its new value. A table is a JSON object, an array of tables a JSON array of objects.
    #[serde(default)]
    pub set: BTreeMap<String, Value>,
    /// Dotted keys or tables to remove; one that is not in the file is ignored.
    #[serde(default)]
    pub remove: Vec<String>,
}

/// What a saved change tells the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Saved {
    /// Whether some key changed that the running server only uses after it restarts.
    pub restart_needed: bool,
    /// Those keys, by their dotted names.
    pub restart_keys: Vec<String>,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

/// Reads and parses the configuration file; a file that is not there is an empty one.
pub fn read_raw(file: &Path) -> Result<(String, toml::Table)> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let raw = toml::from_str(&text).map_err(|e| {
        invalid(format!(
            "{} is not valid TOML: {}",
            file.display(),
            redact(&e.to_string())
        ))
    })?;
    Ok((text, raw))
}

/// Writes `bytes` to `path` through a temporary file in the same directory, which replaces it in one step. The file
/// is readable by its owner only when `private`.
fn write_atomically(path: &Path, bytes: &[u8], private: bool) -> std::io::Result<()> {
    use std::io::Write;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    if private {
        crate::platform::owner_only(&mut options);
    }
    let written = options.open(&temporary).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    match written.and_then(|()| std::fs::rename(&temporary, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temporary);
            Err(e)
        }
    }
}
