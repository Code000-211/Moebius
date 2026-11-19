//! Where the access tokens of the rail's servers are kept: the system's keychain (Secret Service on Linux, Credential
//! Manager on Windows), one entry per server under the server's id. A token is never written to a file of the app.
//! [`Memory`] is the keychain's stand-in for tests; the app never falls back to it, so a system without a keychain
//! adds no server.

use std::collections::HashMap;
use std::sync::Mutex;

/// The name the app's entries have in the keychain.
const SERVICE: &str = "Moebius desktop";

/// Why a token could not be kept or read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("the system's keychain does not answer: {0}")]
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
