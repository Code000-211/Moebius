//! `packages.lock`: what each vendored package was fetched as, so that a change that installs or updates one shows the
//! source, the version, the exact download or commit and the integrity that was checked.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{LOCK_FILE, PackagesError};

/// A package of the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locked {
    /// The name of the directory below `packages/`: the npm name, or, for a git source, the `name` of its
    /// `package.json` (the repository's name without one).
    pub name: String,
    /// The source as `pi install` takes it, with the version or ref that was asked for.
    pub source: String,
    /// The version that was fetched; for a git source, the `version` of its `package.json` or the short commit.
    pub version: String,
    /// The tarball URL of an npm package, the commit of a git one.
    pub resolved: String,
    /// What the registry said the tarball is (`sha512-...`, the `dist.integrity`); none for a git source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    /// The SHA-256 of the tarball, in hex; none for a git source, whose commit names its content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// The file as a whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    #[serde(rename = "package", default)]
    pub packages: Vec<Locked>,
}

const HEADER: &str = "# The packages vendored in packages/, as `moebius ext` fetched them. Changes are proposed by Moebius and merged\n# by a person; see docs/reference/harness-configuration.md.\n";

impl Lock {
    /// The lock of the repository at `root`; none when it has no `packages.lock`.
    pub fn read(root: &Path) -> Result<Self, PackagesError> {
        match std::fs::read_to_string(root.join(LOCK_FILE)) {
            Ok(text) => toml::from_str(&text)
                .map_err(|e| PackagesError::Invalid(format!("{LOCK_FILE}: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes the lock to the repository at `root`, the packages by name; no packages remove the file.
    pub fn write(&mut self, root: &Path) -> Result<(), PackagesError> {
        let file = root.join(LOCK_FILE);
        self.packages.sort_by(|a, b| a.name.cmp(&b.name));
        if self.packages.is_empty() {
            return match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
                _ => Ok(()),
            };
        }
        let text = toml::to_string(self)
            .map_err(|e| PackagesError::Invalid(format!("cannot write {LOCK_FILE}: {e}")))?;
        std::fs::write(file, format!("{HEADER}{text}"))?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Locked> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Adds `entry`, or replaces the package of its name.
    pub fn put(&mut self, entry: Locked) {
        self.packages.retain(|p| p.name != entry.name);
        self.packages.push(entry);
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.packages.len();
        self.packages.retain(|p| p.name != name);
        self.packages.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> Locked {
        Locked {
            name: name.into(),
            source: format!("npm:{name}"),
            version: "1.0.0".into(),
            resolved: format!("https://registry/{name}.tgz"),
            integrity: Some("sha512-abc".into()),
            sha256: Some("00".into()),
        }
    }

    #[test]
    fn a_lock_is_written_by_name_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Lock::read(dir.path()).unwrap(), Lock::default());
        let mut lock = Lock::default();
        lock.put(entry("zeta"));
        lock.put(entry("alpha"));
        lock.put(Locked {
            version: "2.0.0".into(),
            ..entry("zeta")
        });
        lock.write(dir.path()).unwrap();
        let text = std::fs::read_to_string(dir.path().join(LOCK_FILE)).unwrap();
        assert!(
            text.find("alpha").unwrap() < text.find("zeta").unwrap(),
            "{text}"
        );
        let read = Lock::read(dir.path()).unwrap();
        assert_eq!(read.packages.len(), 2);
        assert_eq!(read.get("zeta").unwrap().version, "2.0.0");
        let mut read = read;
        assert!(read.remove("alpha") && !read.remove("alpha"));
        read.remove("zeta");
        read.write(dir.path()).unwrap();
        assert!(!dir.path().join(LOCK_FILE).exists());
    }

    #[test]
    fn a_lock_that_cannot_be_read_says_so() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LOCK_FILE), "[[package]]\nname = 1\n").unwrap();
        assert!(
            Lock::read(dir.path())
                .unwrap_err()
                .to_string()
                .contains("packages.lock")
        );
    }
}
