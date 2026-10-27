//! The event bus: what the core publishes for the desktop app's event stream.

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// One thing that happened, as the desktop app receives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppEvent {
    /// What happened, such as `card_moved` or `run_event`.
    pub kind: String,
    pub data: serde_json::Value,
}

impl AppEvent {
    pub fn new(kind: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            kind: kind.into(),
            data,
        }
    }
}

/// Fans events out to every listener. A listener that falls too far behind loses the oldest events and is told so.
#[derive(Debug, Clone)]
pub struct Bus {
    tx: broadcast::Sender<AppEvent>,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(1024).0,
        }
    }

    /// Publishes an event; with nobody listening it is dropped.
    pub fn publish(&self, event: AppEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}
