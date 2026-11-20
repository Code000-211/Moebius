//! Where the access tokens of the rail's servers are kept: the system's keychain (Secret Service on Linux, Credential
//! Manager on Windows), one entry per server under the server's id. A token is never written to a file of the app.
//! [`Memory`] is the keychain's stand-in for tests; the app never falls back to it, so a system without a keychain
//! adds no server, unless the person asks to connect for this session only (the rail then keeps that token in memory).
//!
//! A keychain call can block for as long as a D-Bus call takes to time out (25 s each), so the rail calls it only through
//! [`Guarded`]: on a thread of its own, one call at a time, each with a time limit.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Semaphore;

/// The name the app's entries have in the keychain.
const SERVICE: &str = "Moebius desktop";

/// Why a token could not be kept or read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error(
    "the system's keychain does not answer: {0}. Install or unlock a keyring (gnome-keyring or KWallet on Linux), or \
     connect for this session only"
)]
pub struct SecretError(pub String);

/// A place for one secret per server id.
pub trait Secrets: Send + Sync {
    /// The token of server `id`, or none when it has none.
    fn get(&self, id: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, id: &str, token: &str) -> Result<(), SecretError>;
    /// Removes the token of `id`; a server with none is as removed.
    fn delete(&self, id: &str) -> Result<(), SecretError>;
}

/// The system's keychain.
pub struct Keychain;

impl Keychain {
    fn entry(id: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(SERVICE, id).map_err(|e| SecretError(e.to_string()))
    }
}

impl Secrets for Keychain {
    fn get(&self, id: &str) -> Result<Option<String>, SecretError> {
        match Self::entry(id)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError(e.to_string())),
        }
    }

    fn set(&self, id: &str, token: &str) -> Result<(), SecretError> {
        Self::entry(id)?
            .set_password(token)
            .map_err(|e| SecretError(e.to_string()))
    }

    fn delete(&self, id: &str) -> Result<(), SecretError> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError(e.to_string())),
        }
    }
}

/// Secrets held in memory only.
#[derive(Default)]
pub struct Memory(Mutex<HashMap<String, String>>);

impl Secrets for Memory {
    fn get(&self, id: &str) -> Result<Option<String>, SecretError> {
        Ok(self.0.lock().expect("not poisoned").get(id).cloned())
    }

    fn set(&self, id: &str, token: &str) -> Result<(), SecretError> {
        self.0
            .lock()
            .expect("not poisoned")
            .insert(id.to_string(), token.to_string());
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<(), SecretError> {
        self.0.lock().expect("not poisoned").remove(id);
        Ok(())
    }
}

/// How long the rail waits for the keychain in all, a call that waits for an earlier one included.
pub const LIMIT: Duration = Duration::from_secs(8);

/// A [`Secrets`] asked from async code: each call runs on a blocking thread, at most one at a time, and gives up after
/// the limit. A call that never returns keeps its thread and its turn, so the calls after it fail at once with the same
/// message instead of piling up threads; they work again when it returns.
#[derive(Clone)]
pub struct Guarded {
    inner: Arc<dyn Secrets>,
    turn: Arc<Semaphore>,
    limit: Duration,
}

impl Guarded {
    pub fn new(inner: Arc<dyn Secrets>, limit: Duration) -> Self {
        Self {
            inner,
            turn: Arc::new(Semaphore::new(1)),
            limit,
        }
    }

    async fn call<T: Send + 'static>(
        &self,
        what: &'static str,
        run: impl FnOnce(&dyn Secrets) -> Result<T, SecretError> + Send + 'static,
    ) -> Result<T, SecretError> {
        let waiting = SecretError(format!(
            "no answer within {} seconds ({what})",
            self.limit.as_secs()
        ));
        let within = async {
            let turn = self
                .turn
                .clone()
                .acquire_owned()
                .await
                .expect("never closed");
            let inner = self.inner.clone();
            tokio::task::spawn_blocking(move || {
                let _turn = turn;
                run(inner.as_ref())
            })
            .await
            .map_err(|e| SecretError(e.to_string()))?
        };
        tokio::time::timeout(self.limit, within)
            .await
            .unwrap_or(Err(waiting))
    }

    pub async fn get(&self, id: &str) -> Result<Option<String>, SecretError> {
        let id = id.to_string();
        self.call("reading the token", move |s| s.get(&id)).await
    }

    pub async fn set(&self, id: &str, token: &str) -> Result<(), SecretError> {
        let (id, token) = (id.to_string(), token.to_string());
        self.call("saving the token", move |s| s.set(&id, &token))
            .await
    }

    pub async fn delete(&self, id: &str) -> Result<(), SecretError> {
        let id = id.to_string();
        self.call("deleting the token", move |s| s.delete(&id))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_error_names_the_remedy_and_the_phrase_the_connect_page_looks_for() {
        let text = SecretError("x".into()).to_string();
        // connect/connect.js offers "Connect for this session only" when the error has this phrase.
        assert!(text.contains("keychain does not answer"), "{text}");
        assert!(text.contains("gnome-keyring or KWallet"), "{text}");
        assert!(text.contains("this session only"), "{text}");
    }

    #[tokio::test]
    async fn a_guarded_call_reaches_the_secrets_and_back() {
        let guarded = Guarded::new(Arc::new(Memory::default()), LIMIT);
        guarded.set("a", "t").await.unwrap();
        assert_eq!(guarded.get("a").await.unwrap().as_deref(), Some("t"));
        guarded.delete("a").await.unwrap();
        assert_eq!(guarded.get("a").await.unwrap(), None);
    }
}
