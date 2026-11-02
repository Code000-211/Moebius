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
/// is readable by its owner only when `private`; otherwise it keeps the mode of the file it replaces. Each write
/// has its own temporary file.
pub fn write_atomically(path: &Path, bytes: &[u8], private: bool) -> std::io::Result<()> {
    use std::io::Write;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = path.with_file_name(format!(".{name}.{}.{unique}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    if private {
        crate::platform::owner_only(&mut options);
    }
    let written = options.open(&temporary).and_then(|mut file| {
        // A file that is replaced keeps its permissions, unless it must be private: that never depends on what was there.
        if !private && let Ok(existing) = std::fs::metadata(path) {
            file.set_permissions(existing.permissions())?;
        }
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

// Unix modes only; Windows has no modes (see `platform`).
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn replacing_a_file_keeps_its_mode_and_a_private_one_is_private_whatever_was_there() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("moebius.toml");
        std::fs::write(&file, "a = 1\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        write_atomically(&file, b"a = 2\n", false).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "a = 2\n");
        assert_eq!(mode(&file), 0o600);
        // A file that is not there yet takes the usual mode, and no temporary file is left.
        let fresh = dir.path().join("fresh.toml");
        write_atomically(&fresh, b"", false).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        // A private file is owner-only also over a wider one (a restored backup, say).
        let secrets = dir.path().join("secrets.toml");
        std::fs::write(&secrets, "k = 1\n").unwrap();
        std::fs::set_permissions(&secrets, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_atomically(&secrets, b"k = 2\n", true).unwrap();
        assert_eq!(std::fs::read_to_string(&secrets).unwrap(), "k = 2\n");
        assert_eq!(mode(&secrets), 0o600);
    }
}
