//! The Ports page: the ports that runs, chats and terminals on the server listen on, and the ones forwarded by hand,
//! each made reachable on this computer. A port that opens on the server is forwarded at once, to the same port number
//! here when that is free and else to a free one; the forwarder is a subscription for as long as the forward exists
//! (`client::forward`). The page also opens from the live view, the chat and the Terminal page, showing the ports of
//! that run, chat or terminal, and an offer to open a port in the browser stands over the pages when one appears in
//! what is being watched.
//!
//! `update` has how a message changes the state and `view` the page and the offer.

#[cfg(test)]
mod tests;
mod update;
mod view;

use std::collections::BTreeSet;

use iced::Subscription;

use crate::app::Message as AppMessage;
use crate::client::{self, ClientError, ForwardEvent, Port};
use crate::settings::{Forwarded, Settings};

pub use view::{OPEN_PORTS, button, offer_banner};

#[derive(Debug, Clone)]
pub enum Message {
    /// The ports of the server were read.
    Listed(Result<Vec<Port>, ClientError>),
    /// A forwarder told something.
    Forward(ForwardEvent),
    /// "Forward a port…": the field for a port opens.
    OpenForm,
    FormEdited(String),
    FormSubmit,
    FormCancel,
    /// Forward the port on the server again, after "Stop forwarding".
    Start(u16),
    /// "Open in browser" for the port on the server.
    Open(u16),
    /// "Copy address" for the port on the server.
    Copy(u16),
    /// "Change local port": the field for the port here opens.
    Change(u16),
    LocalEdited(String),
    LocalSubmit,
    LocalCancel,
    /// "Stop forwarding".
    Stop(u16),
    /// Back from the ports of one run, chat or terminal to all of them.
    ShowAll,
    /// "Restore forwarded ports when reconnecting".
    ToggleRestore,
    /// The offer to open a port was answered: opened or dismissed.
    OpenOffer,
    DismissOffer,
}

/// Which ports the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The ports of a run or chat, and of the terminals opened on it.
    Run(i64),
    /// The ports of the terminal on the server itself.
    Server,
}

impl Scope {
    fn holds(self, port: &Port) -> bool {
        match self {
            Scope::Run(run) => port.run == Some(run),
            Scope::Server => port.kind == "terminal" && port.run.is_none(),
        }
    }

    fn label(self) -> String {
        match self {
            Scope::Run(run) => format!("run {run}"),
            Scope::Server => "the server's terminal".to_string(),
        }
    }
}

/// How a forwarder stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Starting,
    Listening,
    /// It cannot listen on the local port.
    Failed(String),
}

/// A port of the server that is forwarded to this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Forward {
    remote: u16,
    local: u16,
    state: State,
    /// Asked for by the person, so that it stays when the port on the server closes.
    by_hand: bool,
    /// Why the last connection to the local port was not carried.
    refused: Option<String>,
}

/// The state of the Ports page and of the forwards.
pub struct Ports {
    detected: Vec<Port>,
    forwards: Vec<Forward>,
    /// The ports whose forwarding the person stopped while they stay open: they are not forwarded again by themselves.
    stopped: BTreeSet<u16>,
    scope: Option<Scope>,
    /// The field of "Forward a port…", while it is open.
    form: Option<String>,
    /// The port on the server whose local port is being changed, and what is typed.
    editing: Option<(u16, String)>,
    note: Option<String>,
    /// The ports that opened in what is being watched, the oldest first, offered to be opened in the browser.
    offers: Vec<Port>,
    restore: bool,
    /// Set when the forwards to remember changed, until `take_changed` is called.
    changed: bool,
}

impl Default for Ports {
    fn default() -> Self {
        Ports {
            detected: Vec::new(),
            forwards: Vec::new(),
            stopped: BTreeSet::new(),
            scope: None,
            form: None,
            editing: None,
            note: None,
            offers: Vec::new(),
            restore: true,
            changed: false,
        }
    }
}

/// A free port of `127.0.0.1`: `preferred` when nothing uses it, else one the system picks.
fn free_port(preferred: u16, taken: &[u16]) -> u16 {
    let free = |port| std::net::TcpListener::bind(("127.0.0.1", port));
    if !taken.contains(&preferred) && free(preferred).is_ok() && preferred != 0 {
        return preferred;
    }
    free(0)
        .and_then(|listener| listener.local_addr())
        .map_or(0, |address| address.port())
}

impl Ports {
    /// Starts over for a server: nothing is detected, and `remembered` are forwarded again if `restore` is on.
    pub fn connected(&mut self, remembered: Vec<Forwarded>, restore: bool) {
        *self = Ports {
            restore,
            forwards: remembered
                .into_iter()
                .map(|saved| Forward {
                    remote: saved.remote,
                    local: saved.local,
                    state: State::Starting,
                    by_hand: saved.by_hand,
                    refused: None,
                })
                .collect(),
            ..Ports::default()
        };
    }

    /// The forwards to remember for the server.
    pub fn remembered(&self) -> Vec<Forwarded> {
        self.forwards
            .iter()
            .map(|forward| Forwarded {
                remote: forward.remote,
                local: forward.local,
                by_hand: forward.by_hand,
            })
            .collect()
    }

    /// Whether the forwards to remember changed since this was last asked.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Whether the forwarded ports are restored at the next connection.
    pub fn restores(&self) -> bool {
        self.restore
    }

    /// Shows only the ports of `scope`, or all of them.
    pub fn show(&mut self, scope: Option<Scope>) {
        self.scope = scope;
        self.note = None;
    }

    /// How many ports of `scope` are open now.
    pub fn count(&self, scope: Scope) -> usize {
        self.detected
            .iter()
            .filter(|port| scope.holds(port))
            .count()
    }

    /// The ports detected now.
    pub fn detected(&self) -> &[Port] {
        &self.detected
    }

    /// Reads the ports of the server.
    pub fn reload(&self, settings: &Settings) -> iced::Task<AppMessage> {
        let client = client::Client::new(settings.clone());
        iced::Task::perform(async move { client.ports().await }, |ports| {
            AppMessage::Ports(Message::Listed(ports))
        })
    }

    /// An event of the stream: a port opened or closed. `watched` says whether the port belongs to what the person is
    /// looking at, which makes an offer to open it.
    pub fn on_event(
        &mut self,
        kind: &str,
        data: &serde_json::Value,
        watched: impl Fn(&Port) -> bool,
    ) {
        match kind {
            "port_opened" => {
                if let Ok(port) = serde_json::from_value::<Port>(data.clone()) {
                    self.opened(port, watched);
                }
            }
            "port_closed" => {
                if let Some(port) = data["port"].as_u64().and_then(|p| u16::try_from(p).ok()) {
                    self.closed(port);
                }
            }
            _ => {}
        }
    }

    fn opened(&mut self, port: Port, watched: impl Fn(&Port) -> bool) {
        let remote = port.port;
        self.detected.retain(|known| known.port != remote);
        if watched(&port) {
            self.offers.retain(|offer| offer.port != remote);
            self.offers.push(port.clone());
        }
        self.detected.push(port);
        self.detected.sort_by_key(|port| port.port);
        if !self.stopped.contains(&remote) {
            self.forward(remote, false);
        }
    }

    fn closed(&mut self, remote: u16) {
        self.detected.retain(|port| port.port != remote);
        self.stopped.remove(&remote);
        self.offers.retain(|offer| offer.port != remote);
        let before = self.forwards.len();
        self.forwards
            .retain(|forward| forward.remote != remote || forward.by_hand);
        self.changed |= self.forwards.len() != before;
    }

    /// Makes the ports the server lists the detected ones: new ones are forwarded, and a forward the server no longer
    /// has a port for goes unless the person asked for it.
    fn listed(&mut self, ports: Vec<Port>) {
        let open: Vec<u16> = ports.iter().map(|port| port.port).collect();
        let before = self.forwards.len();
        self.forwards
            .retain(|forward| forward.by_hand || open.contains(&forward.remote));
        self.changed |= self.forwards.len() != before;
        self.stopped.retain(|port| open.contains(port));
        self.offers.retain(|offer| open.contains(&offer.port));
        for port in &ports {
            if !self.stopped.contains(&port.port) {
                self.forward(port.port, false);
            }
        }
        self.detected = ports;
        self.detected.sort_by_key(|port| port.port);
    }

    /// Forwards `remote` unless it is, remembering that the person asked for it if `by_hand`.
    fn forward(&mut self, remote: u16, by_hand: bool) {
        if let Some(forward) = self.forwards.iter_mut().find(|f| f.remote == remote) {
            if by_hand && !forward.by_hand {
                forward.by_hand = true;
                self.changed = true;
            }
            return;
        }
        let taken: Vec<u16> = self.forwards.iter().map(|forward| forward.local).collect();
        self.forwards.push(Forward {
            remote,
            local: free_port(remote, &taken),
            state: State::Starting,
            by_hand,
            refused: None,
        });
        self.changed = true;
    }

    /// The forwarders, for as long as their forwards exist.
    pub fn subscription(&self, settings: &Settings) -> Subscription<AppMessage> {
        Subscription::batch(self.forwards.iter().map(|forward| {
            Subscription::run_with(
                (settings.clone(), forward.remote, forward.local),
                |(settings, remote, local)| client::forward(settings.clone(), *remote, *local),
            )
            .map(|event| AppMessage::Ports(Message::Forward(event)))
        }))
    }

    /// The address of the forward of `remote` on this computer, once it has one.
    pub fn address(&self, remote: u16) -> Option<String> {
        self.forwards
            .iter()
            .find(|forward| forward.remote == remote)
            .map(|forward| format!("127.0.0.1:{}", forward.local))
    }

    /// The oldest offer to open a port, if there is one.
    pub fn offer(&self) -> Option<&Port> {
        self.offers.first()
    }
}
