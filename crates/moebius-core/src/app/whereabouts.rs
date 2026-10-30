//! Where Moebius runs: the server's program, files and addresses, found once at start, and the desktop app's, which it
//! reports when it connects. The Moebius agent is told both when its run starts (see [`App::where_moebius_runs`]), so
//! that it finds the server's files and the app when it is asked to fix something. Kept in memory only, and never a token
//! or a key.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use super::App;
use crate::blobs::redact;

/// Moebius's own repository, which the Moebius agent offers fixes and problems to.
pub const OFFICIAL_REPOSITORY: &str = "https://github.com/Code000-211/Moebius";

/// The longest text the app may report in one field.
const FIELD_LIMIT: usize = 500;

/// What the server knows of itself: `GET /v1/server`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    /// The server's program, as it was started.
    pub program: String,
    pub version: String,
    /// The `moebius.toml` the server runs with.
    pub config_file: String,
    pub data_dir: String,
    /// The source checkout the program was built from, when it lies in one (`target/` of a cargo workspace).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_dir: Option<String>,
    /// The address the desktop API listens on, once it is bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_address: Option<String>,
    /// The address the router (the model provider) listens on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_address: Option<String>,
    pub host: String,
    pub os: String,
    pub arch: String,
}

impl ServerInfo {
    /// What this process is: the program running, the files it was given and where it listens.
    pub fn detect(config_file: &Path, data_dir: &Path, router_address: String) -> Self {
        let program = std::env::current_exe().ok();
        let source_dir = program.as_deref().and_then(source_checkout);
        ServerInfo {
            program: program.map_or_else(String::new, |p| p.display().to_string()),
            version: crate::VERSION.to_string(),
            config_file: config_file.display().to_string(),
            data_dir: data_dir.display().to_string(),
            source_dir: source_dir.map(|p| p.display().to_string()),
            api_address: None,
            router_address: Some(router_address),
            host: crate::platform::host_name(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
        }
    }
}

/// The source checkout `program` was built in: the folder above `target/` that has `crates/moebius-core`.
fn source_checkout(program: &Path) -> Option<PathBuf> {
    program
        .ancestors()
        .skip(1)
        .take(6)
        .find(|dir| dir.join("crates/moebius-core").is_dir() && dir.join("Cargo.toml").is_file())
        .map(Path::to_path_buf)
}

/// What the desktop app tells about itself: `PUT /v1/client`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientInfo {
    /// The app's program.
    pub program: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    /// The computer the app runs on.
    pub host: String,
    /// The address the app reaches the server by (over SSH, the local end of the tunnel).
    pub server_address: String,
    /// The host name the app reaches the server by over SSH; none when it connects directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    /// When the server last heard this (Unix seconds); set by the server.
    #[serde(default)]
    pub reported_at: i64,
}

impl ClientInfo {
    /// The report with every field redacted and cut to a sensible length.
    fn cleaned(self) -> Self {
        let clean = |text: String| redact(&text.chars().take(FIELD_LIMIT).collect::<String>());
        ClientInfo {
            program: clean(self.program),
            version: clean(self.version),
            os: clean(self.os),
            arch: clean(self.arch),
            host: clean(self.host),
            server_address: clean(self.server_address),
            ssh_host: self.ssh_host.map(clean),
            reported_at: self.reported_at,
        }
    }
}

/// What the app and the server know of where they run.
#[derive(Default)]
pub(super) struct Whereabouts {
    server: RwLock<Option<ServerInfo>>,
    client: RwLock<Option<ClientInfo>>,
}

impl App {
    /// Records what the server found out about itself at start.
    pub fn set_server_info(&self, info: ServerInfo) {
        *self
            .whereabouts
            .server
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(info);
    }

    /// Records the address the desktop API listens on, once it is bound.
    pub fn set_api_address(&self, address: String) {
        if let Some(info) = self
            .whereabouts
            .server
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            info.api_address = Some(address);
        }
    }

    /// What the server knows of itself; none for a core that was not started as a server.
    pub fn server_info(&self) -> Option<ServerInfo> {
        self.whereabouts
            .server
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Records what the app reported, the latest report replacing the one before.
    pub fn set_client_info(&self, info: ClientInfo) -> ClientInfo {
        let mut info = info.cleaned();
        info.reported_at = crate::store::now();
        *self
            .whereabouts
            .client
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(info.clone());
        info
    }

    /// What the app last reported, if it has.
    pub fn client_info(&self) -> Option<ClientInfo> {
        self.whereabouts
            .client
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The part of the Moebius agent's instructions that says where Moebius runs and where to report what it fixes,
    /// built from what is known now.
    pub fn where_moebius_runs(&self) -> String {
        let mut text = String::from("Where Moebius runs.\n");
        match self.server_info() {
            Some(s) => {
                text.push_str(&format!(
                    "The server (the Moebius you operate) is version {} on {} ({} {}). Its program is {}; it runs \
                     with the configuration {} and keeps its data, skills and clones in {}.",
                    s.version, s.host, s.os, s.arch, s.program, s.config_file, s.data_dir
                ));
                if let Some(source) = &s.source_dir {
                    text.push_str(&format!(
                        " It was built from the source checkout {source}: that is where to read and fix its code."
                    ));
                }
                if let Some(api) = &s.api_address {
                    text.push_str(&format!(
                        " Its desktop API listens on {api}; `\"$MOEBIUS\"` already reaches it."
                    ));
                }
                if let Some(router) = &s.router_address {
                    text.push_str(&format!(" Its router (the model provider) listens on {router}."));
                }
                text.push_str(
                    " Read the server's own files only as far as the task needs. `\"$MOEBIUS\" api GET /v1/server` \
                     tells this again if it may have changed.\n",
                );
            }
            None => text.push_str(
                "The server has not said where it runs. Ask it with `\"$MOEBIUS\" api GET /v1/server`.\n",
            ),
        }
        match self.client_info() {
            Some(c) => {
                text.push_str(&format!(
                    "The desktop app the person uses is version {} on {} ({} {}); its program is {}. It reaches the \
                     server at {}",
                    c.version, c.host, c.os, c.arch, c.program, c.server_address
                ));
                match &c.ssh_host {
                    Some(host) => text.push_str(&format!(", over SSH through {host}")),
                    None => text.push_str(", directly"),
                }
                text.push_str(
                    ". Files on the person's computer are not on the server unless both are the same computer. \
                     `\"$MOEBIUS\" api GET /v1/client` shows what the app last reported.\n",
                );
            }
            None => text.push_str(
                "The desktop app has not reported where it runs yet (it does so when it connects).\n",
            ),
        }
        text.push_str(&format!(
            "\nMoebius's own repository is {OFFICIAL_REPOSITORY}. When the person fixes something in Moebius itself, \
             or you meet a problem in Moebius (not in the person's project), offer to send it there: a pull request \
             for a fix, an issue for a problem. Ask first and send nothing without the person's yes. Read the \
             moebius-report skill for how.\n"
        ));
        text
    }
}
