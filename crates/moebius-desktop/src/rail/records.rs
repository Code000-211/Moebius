//! The servers the rail remembers, in `servers.json` in the app's configuration directory: an id, a name, an address
//! (or the SSH host the server is reached through) and which one was open last. There is no token in it; the tokens are
//! in the keychain ([`super::secrets`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One server of the rail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// A random id, which names the server's entry in the keychain.
    pub id: String,
    /// What the rail's square is named after: the address, or the host of an SSH server.
    pub name: String,
    /// The server's address; of an SSH server, the host's name instead (its tunnel's address changes on every start).
    pub address: String,
    /// Whether the server is reached through an SSH tunnel to `address`.
    #[serde(default)]
    pub ssh: bool,
}

/// The file's content.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub servers: Vec<Record>,
    /// The id of the server that was open when the app was closed.
    pub last: Option<String>,
}

/// The file `servers.json` of a directory.
pub struct Records {
    path: PathBuf,
}

impl Records {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("servers.json"),
        }
    }

    /// What was saved; nothing saved (or a file that cannot be read) is no servers.
    pub fn load(&self) -> Saved {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, saved: &Saved) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(saved).map_err(std::io::Error::other)?;
        std::fs::write(&self.path, text)
    }
}

/// A new random id.
pub fn new_id() -> String {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("the system gives random bytes");
    hex::encode(bytes)
}
