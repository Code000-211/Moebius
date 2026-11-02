//! `<data_dir>/packages.lock`: what is installed, from which registry and which URLs with which sha256. It is written
//! after every install, update and removal, in one step.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::PackageError;
use crate::server::settings::write_atomically;

/// Where the lock of the server with `data_dir` is.
pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("packages.lock")
}

/// What is installed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Lock {
    #[serde(default, rename = "package")]
    pub packages: Vec<Locked>,
}

/// One installed package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Locked {
    pub name: String,
    pub version: String,
    pub kind: String,
    /// The registry it came from.
    pub registry: String,
    /// The URL and the sha256 of the program of a plugin, or of the first file of the others.
    pub url: String,
    pub sha256: String,
    /// The program of a plugin, where it is installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// The repository a workflow or rules package was proposed to; none for skills, which go to the harness
    /// configuration repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// The change that proposed it, for the packages that are proposed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<i64>,
    /// Every file with its URL and sha256: where it was proposed to go, relative to its folder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<LockedFile>,
    /// The rules a rules package added to the repository's `.moebius/rules.md`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockedFile {
    pub to: String,
    pub url: String,
    pub sha256: String,
}

impl Lock {
    /// The lock in `data_dir`; no file is an empty lock.
    pub fn read(data_dir: &Path) -> Result<Self, PackageError> {
        let file = path(data_dir);
        match std::fs::read_to_string(&file) {
            Ok(text) => toml::from_str(&text)
                .map_err(|e| PackageError::Lock(format!("{} is not valid: {e}", file.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(PackageError::Lock(format!(
                "cannot read {}: {e}",
                file.display()
            ))),
        }
    }

    pub fn write(&self, data_dir: &Path) -> Result<(), PackageError> {
        let text = toml::to_string_pretty(self)
            .map_err(|e| PackageError::Lock(format!("cannot write the lock: {e}")))?;
        std::fs::create_dir_all(data_dir)?;
        write_atomically(&path(data_dir), text.as_bytes(), false)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Locked> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Puts `entry` in place of the package of its name, or at the end.
    pub fn put(&mut self, entry: Locked) {
        match self.packages.iter_mut().find(|p| p.name == entry.name) {
            Some(old) => *old = entry,
            None => self.packages.push(entry),
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.packages.retain(|p| p.name != name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_keeps_what_is_installed_and_no_file_is_an_empty_lock() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Lock::read(dir.path()).unwrap(), Lock::default());
        let mut lock = Lock::default();
        let entry = |name: &str, version: &str| Locked {
            name: name.into(),
            version: version.into(),
            kind: "plugin".into(),
            registry: "moebius-plugins".into(),
            url: "https://example.com/p".into(),
            sha256: "ab".repeat(32),
            program: Some("/data/packages/p".into()),
            repository: None,
            change: None,
            files: vec![],
            rules: vec![],
        };
        lock.put(entry("a", "1.0.0"));
        lock.put(entry("b", "2.0.0"));
        lock.put(entry("a", "1.1.0"));
        lock.write(dir.path()).unwrap();
        let read = Lock::read(dir.path()).unwrap();
        assert_eq!(read, lock);
        assert_eq!(read.get("a").unwrap().version, "1.1.0");
        assert_eq!(read.packages.len(), 2);
        let mut read = read;
        read.remove("a");
        assert!(read.get("a").is_none());
        std::fs::write(path(dir.path()), "package = 3").unwrap();
        assert!(matches!(Lock::read(dir.path()), Err(PackageError::Lock(_))));
    }
}
