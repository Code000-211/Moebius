//! The desktop app (ADR 0021): the web app of the same release in a window of its own, against one server. The library
//! holds what the window needs and the tests check without a window; the Tauri side is the program's `window` module.
//!
//! The window first shows the connect page (`connect/`, bundled by Tauri), where a person gives a server's address and
//! an access token. [`server::check`] asks that server for its version with the token and refuses a server of another
//! minor version, naming the release that matches. Once it passes, [`proxy`] serves the web app built into the app
//! (`web/dist`) on the loopback and forwards `/v1/` to the server with the token, so the web app talks to its own
//! address as it does when the server serves it, and the token stays in this process. [`links`] says which addresses
//! the window shows and which go to the system's browser. [`local`] sets up and starts a server of this machine (Linux, or
//! WSL 2 on Windows) for the connect page's first-start offer. [`rail`] keeps the servers the app remembers (their tokens
//! in the system's keychain) and the page of each, for the rail at the window's left.

pub mod links;
pub mod local;
pub mod proxy;
pub mod rail;
pub mod server;
