//! The content-addressed blob store: large and repeating content (contexts, diffs, logs) kept once on disk under the
//! SHA-256 of its bytes, with secrets removed before anything is written.

use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;
use sha2::{Digest, Sha256};

/// The hex SHA-256 of a blob's (redacted) bytes.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct BlobId(pub String);

static SECRETS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    [
        (
            "private-key",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        ),
        ("anthropic-key", r"sk-ant-[A-Za-z0-9_\-]{20,}"),
        ("openai-key", r"sk-(?:proj-)?[A-Za-z0-9_\-]{20,}"),
        (
            "github-token",
            r"(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}",
        ),
        ("aws-access-key", r"AKIA[0-9A-Z]{16}"),
    ]
    .into_iter()
    .map(|(name, pattern)| (name, Regex::new(pattern).expect("secret pattern compiles")))
    .collect()
});

/// Replaces every secret the patterns know with `[redacted:<kind>]`.
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    for (name, re) in SECRETS.iter() {
        out = re
            .replace_all(&out, format!("[redacted:{name}]"))
            .into_owned();
    }
    out
}

/// `value` with every string in it redacted, keys included.
pub fn redact_json(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(text) => Value::String(redact(text)),
        Value::Array(items) => Value::Array(items.iter().map(redact_json).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (redact(key), redact_json(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Blobs on disk, at `<root>/<first two hex digits>/<rest>`.
#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, id: &BlobId) -> PathBuf {
        self.root.join(&id.0[..2]).join(&id.0[2..])
    }

    /// Stores `content` with secrets removed and returns its id; content already stored is not written again.
    pub fn put(&self, content: &str) -> std::io::Result<BlobId> {
        let clean = redact(content);
        let id = BlobId(hex::encode(Sha256::digest(clean.as_bytes())));
        let path = self.path(&id);
        if !path.exists() {
            std::fs::create_dir_all(path.parent().expect("blob paths have a parent"))?;
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, clean.as_bytes())?;
            std::fs::rename(&tmp, &path)?;
        }
        Ok(id)
    }

    pub fn get(&self, id: &BlobId) -> std::io::Result<String> {
        if id.0.len() != 64 || !id.0.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "not a blob id",
            ));
        }
        std::fs::read_to_string(self.path(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn blobs_round_trip_and_are_stored_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        let a = store.put("hello world").unwrap();
        let b = store.put("hello world").unwrap();
        assert_eq!(a, b);
        assert_eq!(store.get(&a).unwrap(), "hello world");
        let files: Vec<_> = walk(dir.path());
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn secrets_never_reach_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        let key = format!("sk-ant-api03-{}", "x".repeat(40));
        let token = format!("ghp_{}", "A".repeat(36));
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----";
        let id = store
            .put(&format!(
                "key {key} token {token}\n{pem}\nAKIAABCDEFGHIJKLMNOP"
            ))
            .unwrap();
        let stored = store.get(&id).unwrap();
        assert_eq!(
            stored,
            "key [redacted:anthropic-key] token [redacted:github-token]\n[redacted:private-key]\n[redacted:aws-access-key]"
        );
    }

    #[test]
    fn ids_that_are_not_hashes_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            BlobStore::new(dir.path())
                .get(&BlobId("../../etc/passwd".into()))
                .is_err()
        );
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                out.extend(walk(&p))
            } else {
                out.push(p)
            }
        }
        out
    }
}
