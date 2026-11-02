//! The content-addressed blob store: large and repeating content (contexts, diffs, logs) kept once on disk under the
//! SHA-256 of its bytes, with secrets removed before anything is written. [`redact`] knows secrets by their shape, and
//! by their value for those the server was given at run time ([`remember_secret`]), such as the tokens of an OAuth login.

use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

use regex::Regex;
use sha2::{Digest, Sha256};

/// The hex SHA-256 of a blob's (redacted) bytes.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct BlobId(pub String);

/// A pattern for a kind of secret: its name, its shape, and whether it is only a key when it does not continue a word
/// (`sk-` is also the end of "task-", so those are looked at in [`starts_a_key`]).
struct Pattern {
    name: &'static str,
    regex: Regex,
    guarded: bool,
}

static SECRETS: LazyLock<Vec<Pattern>> = LazyLock::new(|| {
    [
        (
            "private-key",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        ),
        ("anthropic-key", r"sk-ant-[A-Za-z0-9_\-]{20,}"),
        ("openai-key", r"sk-(?:proj-)?[A-Za-z0-9_\-]{20,}"),
        (
            "github-token",
            r"(?:(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})",
        ),
        ("gitlab-token", r"glpat-[A-Za-z0-9_\-]{20,}"),
        ("aws-access-key", r"AKIA[0-9A-Z]{16}"),
        ("webhook-secret", r"whsec_[A-Za-z0-9]{20,}"),
    ]
    .into_iter()
    .map(|(name, pattern)| Pattern {
        name,
        regex: Regex::new(pattern).expect("secret pattern compiles"),
        guarded: pattern.starts_with("sk-"),
    })
    .collect()
});

/// Whether a match that begins at the end of `before` is a key and not the tail of a word: it is when the character
/// before is no letter or digit, or when that is the end of an escape (`\n`, `%3D`) that text carries a newline or an
/// equals sign in.
fn starts_a_key(before: &str) -> bool {
    let Some(last) = before.chars().next_back() else {
        return true;
    };
    if !last.is_ascii_alphanumeric() {
        return true;
    }
    let bytes = before.as_bytes();
    let n = bytes.len();
    (n >= 2 && bytes[n - 2] == b'\\' && matches!(last, 'n' | 'r' | 't'))
        || (n >= 3 && bytes[n - 3] == b'%' && bytes[n - 2..].iter().all(u8::is_ascii_hexdigit))
}

/// The secrets the server was given while it runs, redacted by their value; shorter ones are not kept, since they
/// would be found in ordinary text.
static REMEMBERED: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// The fewest characters a remembered secret has.
const REMEMBERED_LEAST: usize = 12;

/// Has [`redact`] replace `secret` wherever it appears, from now until the server stops: for a secret of no known shape,
/// such as an OAuth token.
pub fn remember_secret(secret: &str) {
    let secret = secret.trim();
    if secret.chars().count() < REMEMBERED_LEAST {
        return;
    }
    let mut remembered = REMEMBERED.write().unwrap_or_else(|p| p.into_inner());
    if !remembered.iter().any(|known| known == secret) {
        remembered.push(secret.to_string());
    }
}

/// Replaces every secret the patterns know with `[redacted:<kind>]`, and every remembered one with
/// `[redacted:secret]`.
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    for secret in REMEMBERED.read().unwrap_or_else(|p| p.into_inner()).iter() {
        if out.contains(secret.as_str()) {
            out = out.replace(secret.as_str(), "[redacted:secret]");
        }
    }
    for Pattern {
        name,
        regex,
        guarded,
    } in SECRETS.iter()
    {
        let replacement = format!("[redacted:{name}]");
        let text = std::mem::take(&mut out);
        out = regex
            .replace_all(&text, |found: &regex::Captures| {
                let whole = found.get(0).expect("a match has a whole");
                if *guarded && !starts_a_key(&text[..whole.start()]) {
                    whole.as_str().to_string()
                } else {
                    replacement.clone()
                }
            })
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
            // A name of its own per writer, so two puts of one content never write the same file.
            static WRITERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let tmp = path.with_extension(format!(
                "{}-{}.tmp",
                std::process::id(),
                WRITERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::write(&tmp, clean.as_bytes())?;
            if let Err(e) = std::fs::rename(&tmp, &path) {
                let _ = std::fs::remove_file(&tmp);
                // Another writer may have stored the same content first (Windows refuses to replace a file in use).
                if !path.exists() {
                    return Err(e);
                }
            }
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
    fn concurrent_puts_of_one_content_all_succeed_and_leave_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        let content = "x".repeat(1 << 20);
        let barrier = std::sync::Barrier::new(8);
        let ids: Vec<_> = std::thread::scope(|scope| {
            let writers: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        store.put(&content).unwrap()
                    })
                })
                .collect();
            writers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        assert!(ids.iter().all(|id| *id == ids[0]));
        assert_eq!(store.get(&ids[0]).unwrap(), content);
        assert_eq!(walk(dir.path()).len(), 1);
    }

    #[test]
    fn a_key_pattern_does_not_match_inside_an_ordinary_word() {
        let word = "src/task-management-integration-tests.rs and desk-lamp-with-a-long-name-here";
        assert_eq!(redact(word), word);
        let key = "sk-proj-abcdefghijklmnopqrstuvwx";
        for text in [
            format!("KEY={key}"),
            format!("\"{key}\""),
            format!("({key})"),
            key.to_string(),
        ] {
            assert!(redact(&text).contains("[redacted:openai-key]"), "{text}");
            assert!(!redact(&text).contains("abcdefghij"), "{text}");
        }
        // After an escaped newline, a percent-encoded character or a prefix, it is still a key.
        let ghp = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        for text in [
            format!("line\\n{key}"),
            format!("a=1%3D{key}"),
            format!("x_{key}"),
            format!("x_{ghp}"),
            format!("FOO{ghp}"),
        ] {
            assert!(!redact(&text).contains("abcdefghij"), "{text}");
        }
    }

    #[test]
    fn a_remembered_secret_is_redacted_by_its_value() {
        let token = "oauth-token-of-no-known-shape-41b7";
        assert_eq!(
            redact(&format!("bearer {token}.")),
            format!("bearer {token}.")
        );
        remember_secret(token);
        remember_secret("short");
        assert_eq!(
            redact(&format!("bearer {token}.")),
            "bearer [redacted:secret]."
        );
        assert_eq!(redact("a short word"), "a short word");
    }

    #[test]
    fn secrets_never_reach_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        let key = format!("sk-ant-api03-{}", "x".repeat(40));
        let token = format!("ghp_{}", "A".repeat(36));
        let gitlab = format!("glpat-{}", "b".repeat(20));
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----";
        let id = store
            .put(&format!(
                "key {key} token {token} {gitlab}\n{pem}\nAKIAABCDEFGHIJKLMNOP"
            ))
            .unwrap();
        let stored = store.get(&id).unwrap();
        assert_eq!(
            stored,
            "key [redacted:anthropic-key] token [redacted:github-token] [redacted:gitlab-token]\n[redacted:private-key]\n[redacted:aws-access-key]"
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
