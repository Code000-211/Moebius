//! Webhooks from the forges (ADR 0012): what makes a delivery trusted, and how a trusted one wakes the poll.
//!
//! A forge that can reach the server may tell it that something happened. Nothing in the delivery is taken as data: it
//! is checked against the secret of its forge (`webhook-github`, `webhook-gitlab` or `webhook-forgejo` in the secrets),
//! and when it checks out it only asks the poll of that forge to run at once, for the repository the delivery names.
//! The poll stays the source of truth and still runs at its usual pace. `api::webhooks` has the routes.

use std::collections::BTreeSet;
use std::sync::Mutex;

use axum::http::HeaderMap;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

/// The forges that can send webhooks.
pub const FORGES: [&str; 3] = ["github", "gitlab", "forgejo"];

/// The name of the secret that holds the webhook secret of `forge`.
pub fn secret_name(forge: &str) -> String {
    format!("webhook-{forge}")
}

/// A new webhook secret: random, and of a shape `blobs::redact` knows.
pub fn generate() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(format!("whsec_{}", hex::encode(bytes)))
}

/// What the polls of the three forges are asked, from the webhooks that came.
#[derive(Default)]
pub struct Webhooks {
    github: Wake,
    gitlab: Wake,
    forgejo: Wake,
}

impl Webhooks {
    pub fn github(&self) -> &Wake {
        &self.github
    }

    pub fn gitlab(&self) -> &Wake {
        &self.gitlab
    }

    pub fn forgejo(&self) -> &Wake {
        &self.forgejo
    }

    /// The wake-up of `forge`; none for a name that is no forge.
    pub fn of(&self, forge: &str) -> Option<&Wake> {
        match forge {
            "github" => Some(&self.github),
            "gitlab" => Some(&self.gitlab),
            "forgejo" => Some(&self.forgejo),
            _ => None,
        }
    }
}

/// Asks the poll of one forge to run now, for some repositories or for all of them. Asking twice before the poll
/// wakes is one poll.
#[derive(Default)]
pub struct Wake {
    pending: Mutex<Pending>,
    notify: Notify,
}

#[derive(Default)]
struct Pending {
    all: bool,
    names: BTreeSet<String>,
}

impl Wake {
    /// Asks for a poll of the repository `name` on the forge, or of all of them without a name.
    pub fn ask(&self, name: Option<String>) {
        {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            match name {
                Some(name) => {
                    pending.names.insert(name);
                }
                None => pending.all = true,
            }
        }
        self.notify.notify_one();
    }

    /// Whether nothing is asked.
    #[cfg(test)]
    pub fn is_idle(&self) -> bool {
        let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        !pending.all && pending.names.is_empty()
    }

    /// Waits for a request, and answers which repositories it was for: none named means all.
    pub async fn wait(&self) -> Option<Vec<String>> {
        loop {
            self.notify.notified().await;
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            let taken = std::mem::take(&mut *pending);
            if taken.all {
                return None;
            }
            if !taken.names.is_empty() {
                return Some(taken.names.into_iter().collect());
            }
        }
    }
}

/// Whether the delivery of `forge`, with these `headers` and `body`, was signed with `secret`. Anything missing or
/// malformed is not verified.
pub fn verified(forge: &str, headers: &HeaderMap, body: &[u8], secret: &str) -> bool {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    match forge {
        "github" => header("x-hub-signature-256")
            .and_then(|v| v.strip_prefix("sha256="))
            .is_some_and(|given| signed(given, secret, body)),
        "forgejo" => header("x-gitea-signature").is_some_and(|given| signed(given, secret, body)),
        // GitLab sends the secret itself. Both sides are hashed so that the comparison does not depend on the length.
        "gitlab" => header("x-gitlab-token").is_some_and(|given| {
            same(
                &Sha256::digest(given.as_bytes()),
                &Sha256::digest(secret.as_bytes()),
            )
        }),
        _ => false,
    }
}

/// Whether `given` (hex) is the HMAC-SHA256 of `body` under `secret`.
fn signed(given: &str, secret: &str, body: &[u8]) -> bool {
    match hex::decode(given.trim()) {
        Ok(given) => same(&given, &hmac_sha256(secret.as_bytes(), body)),
        Err(_) => false,
    }
}

/// The HMAC-SHA256 of `message` under `key` (RFC 2104).
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let pad = |byte: u8| block.map(|b| b ^ byte);
    let inner = Sha256::new()
        .chain_update(pad(0x36))
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(pad(0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

/// Compares in time that depends only on the lengths.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The repository a delivery names, as the forge calls it: only to choose which repository to poll, never as data. None
/// when the body does not say.
pub fn repository(forge: &str, body: &[u8]) -> Option<String> {
    let body: Value = serde_json::from_slice(body).ok()?;
    let name = match forge {
        "gitlab" => body.pointer("/project/path_with_namespace"),
        _ => body.pointer("/repository/full_name"),
    };
    name?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(secret: &str, body: &[u8]) -> String {
        hex::encode(hmac_sha256(secret.as_bytes(), body))
    }

    #[test]
    fn the_hmac_is_the_one_of_rfc_4231() {
        let key = [0x0b; 20];
        assert_eq!(
            hex::encode(hmac_sha256(&key, b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        // A key longer than a block is hashed first.
        let key = [0xaa; 131];
        assert_eq!(
            hex::encode(hmac_sha256(
                &key,
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn each_forge_is_verified_by_its_own_header() {
        let body = br#"{"repository":{"full_name":"acme/widgets"}}"#;
        let headers = |name: &'static str, value: String| {
            let mut headers = HeaderMap::new();
            headers.insert(name, value.parse().unwrap());
            headers
        };
        let good = signature("s3cret", body);
        let wrong = signature("other", body);

        let github = |value: String| headers("x-hub-signature-256", value);
        assert!(verified(
            "github",
            &github(format!("sha256={good}")),
            body,
            "s3cret"
        ));
        assert!(!verified(
            "github",
            &github(format!("sha256={wrong}")),
            body,
            "s3cret"
        ));
        assert!(!verified("github", &github(good.clone()), body, "s3cret"));
        assert!(!verified(
            "github",
            &github("sha256=zz".into()),
            body,
            "s3cret"
        ));
        assert!(!verified("github", &HeaderMap::new(), body, "s3cret"));

        let forgejo = |value: String| headers("x-gitea-signature", value);
        assert!(verified("forgejo", &forgejo(good.clone()), body, "s3cret"));
        assert!(!verified("forgejo", &forgejo(wrong), body, "s3cret"));
        // A signature of another body does not carry over.
        assert!(!verified("forgejo", &forgejo(good), b"{}", "s3cret"));

        let gitlab = |value: &str| headers("x-gitlab-token", value.to_string());
        assert!(verified("gitlab", &gitlab("s3cret"), body, "s3cret"));
        assert!(!verified("gitlab", &gitlab("s3cre"), body, "s3cret"));
        assert!(!verified("gitlab", &gitlab(""), body, "s3cret"));
        assert!(!verified("bitbucket", &gitlab("s3cret"), body, "s3cret"));
    }

    #[test]
    fn the_repository_a_delivery_names_is_read_for_the_poll() {
        let github = br#"{"repository":{"full_name":"acme/widgets"}}"#;
        assert_eq!(
            repository("github", github).as_deref(),
            Some("acme/widgets")
        );
        assert_eq!(
            repository("forgejo", github).as_deref(),
            Some("acme/widgets")
        );
        let gitlab = br#"{"project":{"path_with_namespace":"acme/widgets"}}"#;
        assert_eq!(
            repository("gitlab", gitlab).as_deref(),
            Some("acme/widgets")
        );
        assert_eq!(repository("gitlab", github), None);
        assert_eq!(repository("github", b"not json"), None);
    }

    #[tokio::test]
    async fn asks_are_gathered_into_one_wake() {
        let wake = Wake::default();
        wake.ask(Some("acme/b".into()));
        wake.ask(Some("acme/a".into()));
        wake.ask(Some("acme/b".into()));
        assert_eq!(
            wake.wait().await,
            Some(vec!["acme/a".to_string(), "acme/b".to_string()])
        );
        wake.ask(Some("acme/a".into()));
        wake.ask(None);
        assert_eq!(wake.wait().await, None);
    }

    #[test]
    fn a_generated_secret_is_redacted_by_its_shape() {
        let secret = generate().unwrap();
        assert_ne!(secret, generate().unwrap());
        assert_eq!(
            crate::blobs::redact(&format!("header {secret}")),
            "header [redacted:webhook-secret]"
        );
    }
}
