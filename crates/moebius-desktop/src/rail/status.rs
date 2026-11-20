//! What the rail shows of a server it does not have open: whether it answers, and the count of its Inbox, which is what
//! the web app's sidebar counts: the items of the inbox plus the notices not read yet.

use std::time::Duration;

use serde::Serialize;

use crate::server::Server;
use crate::tray;

/// How long a server has to answer before the rail draws it grey.
const TIMEOUT: Duration = Duration::from_secs(3);

/// A server as the rail draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    /// The one open in the window.
    pub open: bool,
    /// Whether the server answered with the token it is kept with: a grey dot when it did not.
    pub reachable: bool,
    /// The count of its Inbox; 0 for a server that does not answer.
    pub inbox: u32,
}

/// The count of the Inbox of `server`, or none when it does not answer (or does not accept its token). It is one
/// small read of `GET /v1/status`, not the snapshot.
pub async fn inbox_count(server: &Server) -> Option<u32> {
    tray::read_within(server, TIMEOUT)
        .await
        .ok()
        .map(|figures| figures.rail_count())
}
