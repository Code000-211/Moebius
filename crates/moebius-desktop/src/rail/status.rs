//! What the rail shows of a server it does not have open: whether it answers, and the count of its Inbox, which is what
//! the web app's sidebar counts: the items of the inbox plus the notices not read yet, given as the two figures too.

use std::time::Duration;

use serde::Serialize;

use crate::server::Server;
use crate::tray::{self, Figures};

/// How long a server has to answer before the rail draws it grey.
const TIMEOUT: Duration = Duration::from_secs(3);

/// A server as the rail draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub id: String,
    /// What the square is called: the server's own name, or its address, with the address added when another server
    /// has the same name.
    pub name: String,
    /// The address (of an SSH server, its host) the server is reached at.
    pub address: String,
    /// The one open in the window.
    pub open: bool,
    /// Whether the server answered with the token it is kept with: a grey dot when it did not.
    pub reachable: bool,
    /// The count of its Inbox as the web app's sidebar counts it: `items` and `unread` together; 0 for a server that
    /// does not answer.
    pub inbox: u32,
    /// The items of its inbox that are not resolved.
    pub items: u32,
    /// Its notices that the person has not read.
    pub unread: u32,
}

/// The figures of `server`, or none when it does not answer (or does not accept its token). It is one small read of
/// `GET /v1/status`, not the snapshot.
pub async fn figures(server: &Server) -> Option<Figures> {
    tray::read_within(server, TIMEOUT).await.ok()
}
