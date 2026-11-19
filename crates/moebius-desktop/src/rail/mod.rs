//! The rail of servers at the far left of the window (ADR 0021, D2): the servers the app remembers, which one is open,
//! and what the rail shows of each (its Inbox's count, a grey dot when it cannot be reached).
//!
//! Each remembered server is a [`records::Record`] in `servers.json` and a token in the system's keychain
//! ([`secrets`]), never in a file. A server that has been opened has its own [`Proxy`] (its own port, cookie and origin,
//! so the web app of one server keeps its own storage apart from another's) which stays up while another is open, and
//! the page the person was on, which they come back to. A server reached over SSH ([`ssh`]) keeps its tunnel beside its
//! proxy. Removing a server drops all of it and deletes its token.
//!
//! The program's `rail` module wraps a [`Rail`] in the window's state and turns it into commands; everything here is
//! tested without a window.

pub mod records;
pub mod secrets;
pub mod ssh;
mod status;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use records::{Record, Records, Saved};
use secrets::{SecretError, Secrets};
use ssh::{SshError, Tunnel};
pub use status::Entry;

use crate::proxy::{self, Files, Proxy};
use crate::server::{self, ConnectError, Server};

/// Why the rail did not do what it was asked.
#[derive(Debug, thiserror::Error)]
pub enum RailError {
    #[error(transparent)]
    Connect(#[from] ConnectError),
    #[error(transparent)]
    Keychain(#[from] SecretError),
    /// No server is set up on this SSH host; setting one up needs a yes.
    #[error("no server is set up on {0}")]
    NoServer(String),
    #[error("{0}")]
    Ssh(String),
    #[error("cannot start the app's own address: {0}")]
    Proxy(std::io::Error),
    #[error("cannot keep the list of servers: {0}")]
    Records(std::io::Error),
    #[error("there is no such server in the rail")]
    Unknown,
    #[error("the keychain has no token for {0}: connect to it again")]
    NoToken(String),
}

impl From<SshError> for RailError {
    fn from(error: SshError) -> Self {
        match error {
            SshError::NoServer(host) => RailError::NoServer(host),
            SshError::Failed(message) => RailError::Ssh(message),
        }
    }
}

/// What the rail runs: the `ssh` and `moebius` programs, and where the web app's files come from.
#[derive(Clone)]
pub struct Options {
    pub ssh: String,
    pub moebius: String,
    pub files: Files,
}

impl Options {
    /// The programs by their usual names, and the web app built into the app.
    pub fn standard() -> Self {
        Self {
            ssh: "ssh".into(),
            moebius: "moebius".into(),
            files: proxy::embedded,
        }
    }
}

/// A server that has been opened: its proxy, and its tunnel when it is reached over SSH.
struct Live {
    proxy: Proxy,
    server: Server,
    _tunnel: Option<Tunnel>,
}

pub struct Rail {
    records: Records,
    saved: Saved,
    secrets: Arc<dyn Secrets>,
    options: Options,
    live: HashMap<String, Live>,
    /// The page each server was on when another was opened, as a path with its query.
    pages: HashMap<String, String>,
    open: Option<String>,
}

/// What it takes to ask a server how it is, taken out of the rail so that the asking holds no lock on it.
pub struct Target {
    id: String,
    name: String,
    open: bool,
    server: Option<Server>,
}

impl Rail {
    /// The rail of the servers saved in `dir`, with their tokens in `secrets`.
    pub fn new(dir: &Path, secrets: Arc<dyn Secrets>, options: Options) -> Self {
        let records = Records::new(dir);
        Self {
            saved: records.load(),
            records,
            secrets,
            options,
            live: HashMap::new(),
            pages: HashMap::new(),
            open: None,
        }
    }

    /// The ids of the remembered servers, the one open when the app was last closed first.
    pub fn startup_order(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.saved.servers.iter().map(|r| r.id.clone()).collect();
        if let Some(at) = ids
            .iter()
            .position(|id| Some(id) == self.saved.last.as_ref())
        {
            let last = ids.remove(at);
            ids.insert(0, last);
        }
        ids
    }

    /// The id of the server that is open.
    pub fn open_id(&self) -> Option<&str> {
        self.open.as_deref()
    }

    fn record(&self, id: &str) -> Option<&Record> {
        self.saved.servers.iter().find(|r| r.id == id)
    }

    fn save(&self) -> Result<(), RailError> {
        self.records.save(&self.saved).map_err(RailError::Records)
    }

    /// Adds the server at `address` signed in to with `token`, once it accepts the token and is of this app's release
    /// line; its token goes to the keychain. A server the rail has already is updated with the new token. Opens it.
    pub async fn add(&mut self, address: &str, token: &str) -> Result<String, RailError> {
        let server = Server::new(address, token);
        server::check(&server).await?;
        let known = self
            .saved
            .servers
            .iter()
            .find(|r| !r.ssh && r.address == server.url)
            .map(|r| r.id.clone());
        let name = server.url.trim_start_matches("http://").to_string();
        let address = server.url.clone();
        self.keep(known, name, address, false, server, None).await
    }

    /// Adds the server of the SSH host `host` (as `moebius connect` reaches it), setting it up there first when
    /// `set_up` says so. Without a server on the host and without `set_up` it fails with [`RailError::NoServer`]. Opens it.
    pub async fn add_ssh(&mut self, host: &str, set_up: bool) -> Result<String, RailError> {
        let host = host.trim().to_string();
        if host.is_empty() {
            return Err(ConnectError::Missing.into());
        }
        let tunnel = self.tunnel(&host, set_up).await?;
        let server = tunnel.server.clone();
        server::check(&server).await?;
        let known = self
            .saved
            .servers
            .iter()
            .find(|r| r.ssh && r.address == host)
            .map(|r| r.id.clone());
        self.keep(known, host.clone(), host, true, server, Some(tunnel))
            .await
    }

    async fn tunnel(&self, host: &str, set_up: bool) -> Result<Tunnel, RailError> {
        let (program, moebius, host) = (
            self.options.ssh.clone(),
            self.options.moebius.clone(),
            host.to_string(),
        );
        tokio::task::spawn_blocking(move || {
            if set_up {
                Tunnel::open_setting_up(&program, &moebius, &host)
            } else {
                Tunnel::open(&program, &host)
            }
        })
        .await
        .map_err(|e| RailError::Ssh(e.to_string()))?
        .map_err(RailError::from)
    }

    /// Keeps a server that passed its check: its token in the keychain, its record in the file, its proxy up.
    async fn keep(
        &mut self,
        known: Option<String>,
        name: String,
        address: String,
        ssh: bool,
        server: Server,
        tunnel: Option<Tunnel>,
    ) -> Result<String, RailError> {
        let new = known.is_none();
        let id = known.unwrap_or_else(records::new_id);
        let proxy = proxy::start(server.clone(), self.options.files)
            .await
            .map_err(RailError::Proxy)?;
        self.secrets.set(&id, &server.token)?;
        if new {
            self.saved.servers.push(Record {
                id: id.clone(),
                name,
                address,
                ssh,
            });
            if let Err(e) = self.save() {
                self.saved.servers.retain(|r| r.id != id);
                let _ = self.secrets.delete(&id);
                return Err(e);
            }
        }
        self.live.insert(
            id.clone(),
            Live {
                proxy,
                server,
                _tunnel: tunnel,
            },
        );
        // A server that is kept but cannot be remembered as the open one is still kept; the file failed, not the server.
        self.opened(&id)?;
        Ok(id)
    }

    fn opened(&mut self, id: &str) -> Result<(), RailError> {
        self.open = Some(id.to_string());
        self.saved.last = Some(id.to_string());
        self.save()
    }

    /// Opens the server `id`: starts its proxy (and tunnel) unless it is up, after checking the server with the token
    /// from the keychain. Returns the address the window shows: the page the person left it on, or its start.
    pub async fn open(&mut self, id: &str) -> Result<String, RailError> {
        let record = self.record(id).cloned().ok_or(RailError::Unknown)?;
        // A tunnel that ended (the network dropped, the host restarted) is rebuilt, not shown again.
        if self.live.get_mut(id).is_some_and(|live| {
            live._tunnel
                .as_mut()
                .is_some_and(|tunnel| !tunnel.is_running())
        }) {
            self.live.remove(id);
        }
        if !self.live.contains_key(id) {
            let (server, tunnel) = if record.ssh {
                let tunnel = self.tunnel(&record.address, false).await?;
                (tunnel.server.clone(), Some(tunnel))
            } else {
                let token = self
                    .secrets
                    .get(id)?
                    .ok_or_else(|| RailError::NoToken(record.name.clone()))?;
                (Server::new(&record.address, &token), None)
            };
            server::check(&server).await?;
            // A tunnel reads the token again on every start, so the keychain follows the host's.
            self.secrets.set(id, &server.token)?;
            let proxy = proxy::start(server.clone(), self.options.files)
                .await
                .map_err(RailError::Proxy)?;
            self.live.insert(
                id.to_string(),
                Live {
                    proxy,
                    server,
                    _tunnel: tunnel,
                },
            );
        }
        self.opened(id)?;
        let live = &self.live[id];
        Ok(match self.pages.get(id) {
            Some(page) => live.proxy.entry_url_at(page),
            None => live.proxy.entry_url(),
        })
    }

    /// Notes the page the window is on in the server that is open, so that coming back to it leads there.
    pub fn note_page(&mut self, page: &str) {
        if let Some(id) = &self.open {
            self.pages.insert(id.clone(), page.to_string());
        }
    }

    /// The origin of the web app of the server that is open, which the window's handlers compare addresses with.
    pub fn open_origin(&self) -> Option<String> {
        self.live.get(self.open.as_ref()?).map(|l| l.proxy.origin())
    }

    /// Removes the server `id`: its proxy and tunnel stop, its token leaves the keychain and it leaves the file. Says
    /// whether it was the one open.
    pub fn remove(&mut self, id: &str) -> Result<bool, RailError> {
        if self.record(id).is_none() {
            return Err(RailError::Unknown);
        }
        self.live.remove(id);
        self.pages.remove(id);
        self.saved.servers.retain(|r| r.id != id);
        if self.saved.last.as_deref() == Some(id) {
            self.saved.last = None;
        }
        let was_open = self.open.as_deref() == Some(id);
        if was_open {
            self.open = None;
        }
        self.save()?;
        // The server is gone even when the keychain cannot be asked (locked, absent): the error says its token is left.
        self.secrets.delete(id)?;
        Ok(was_open)
    }

    /// What is needed to ask each server how it is; see [`poll`].
    pub fn targets(&self) -> Vec<Target> {
        self.saved
            .servers
            .iter()
            .map(|record| Target {
                id: record.id.clone(),
                name: record.name.clone(),
                open: self.open.as_deref() == Some(&record.id),
                server: match self.live.get(&record.id) {
                    Some(live) => Some(live.server.clone()),
                    // An SSH server is reached through a tunnel that is not up: it is not reached.
                    None if record.ssh => None,
                    None => self
                        .secrets
                        .get(&record.id)
                        .ok()
                        .flatten()
                        .map(|token| Server::new(&record.address, &token)),
                },
            })
            .collect()
    }
}

/// Asks each server of `targets` how it is, all at once, in the order of the rail.
pub async fn poll(targets: Vec<Target>) -> Vec<Entry> {
    let http = reqwest::Client::new();
    let asking = targets.into_iter().map(|target| {
        let http = http.clone();
        async move {
            let count = match &target.server {
                Some(server) => status::inbox_count(&http, server).await,
                None => None,
            };
            Entry {
                id: target.id,
                name: target.name,
                open: target.open,
                reachable: count.is_some(),
                inbox: count.unwrap_or(0),
            }
        }
    });
    futures_util::future::join_all(asking).await
}
