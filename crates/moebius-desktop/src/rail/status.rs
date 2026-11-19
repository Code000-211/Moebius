//! What the rail shows of a server it does not have open: whether it answers, and the count of its Inbox, which is what
//! the web app's sidebar counts: the items of the inbox plus the notices not read yet.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::server::Server;

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

#[derive(Deserialize)]
struct Snapshot {
    #[serde(default)]
    inbox: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Notice {
    #[serde(default)]
    read: bool,
}

/// The count of the Inbox of `server`, or none when it does not answer (or does not accept its token).
pub async fn inbox_count(http: &reqwest::Client, server: &Server) -> Option<u32> {
    let get = |path: &str| {
        http.get(format!("{}{path}", server.url))
            .bearer_auth(&server.token)
            .timeout(TIMEOUT)
            .send()
    };
    let snapshot: Snapshot = get("/v1/snapshot")
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    // The server answers, so a count of the notices it cannot give leaves the items of the inbox alone.
    let unread = match get("/v1/notices").await {
        Ok(response) => response
            .json::<Vec<Notice>>()
            .await
            .map(|notices| notices.iter().filter(|n| !n.read).count())
            .unwrap_or(0),
        Err(_) => 0,
    };
    u32::try_from(snapshot.inbox.len() + unread).ok()
}
