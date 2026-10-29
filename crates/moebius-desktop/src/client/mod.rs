//! The client of the desktop API (`docs/reference/desktop-api.md`), the only way the app talks to the server: typed
//! answers for what the shell shows, and the event stream as an iced subscription that reconnects by itself.
//!
//! The types keep to what the reference says and are tolerant: a field that may be missing has a default and a field
//! the app does not know is ignored, so a newer server does not break an older app.
//!
//! The types are grouped by part of the API: `board` (repositories, cards, changes, the inbox, the snapshot and a
//! repository's files), `runs` (runs and chats), `harness` (agents, rules, skills, MCP servers), `server` (models,
//! plugins, the first start and the settings) and `improve` (the Retro, research, the decision model, usage). `calls`
//! has every route, one module per part of the API too; `events` is the event stream and `terminal` a shell's socket.

mod board;
mod calls;
mod events;
mod harness;
mod improve;
mod runs;
mod server;
mod terminal;

use crate::settings::Settings;

pub use board::*;
pub use events::{ServerEvent, StreamEvent, events};
pub use harness::*;
pub use improve::*;
pub use runs::*;
pub use server::*;
pub use terminal::{TerminalEvent, TerminalInput, terminal};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("the token is not accepted")]
    Unauthorized,
    #[error("cannot reach {0}")]
    Unreachable(String),
    #[error("the server answered {status}: {message}")]
    Server { status: u16, message: String },
    #[error("the server's answer is not understood: {0}")]
    Decode(String),
}

pub struct Client {
    http: reqwest::Client,
    settings: Settings,
}

impl Client {
    pub fn new(settings: Settings) -> Self {
        Client {
            http: reqwest::Client::new(),
            settings,
        }
    }
}

/// Whether `result` is the server refusing the token, which sends the app back to the connect screen.
pub fn unauthorized<T>(result: &Result<T, ClientError>) -> bool {
    matches!(result, Err(ClientError::Unauthorized))
}
