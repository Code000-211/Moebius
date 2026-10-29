//! The Moebius desktop app, built to `docs/design.md`. It talks to the server only through the desktop API
//! (`docs/reference/desktop-api.md`) and does not depend on `moebius-core`.
//!
//! `app` holds the state and draws the window, `client` talks to the server, `ssh` reaches a server over SSH, `sidebar` and `search` are on every
//! screen, `theme` has every colour and size. `screens` has one module per page, `widgets` the pieces screens share.

pub mod app;
pub mod client;
pub mod screens;
pub mod search;
pub mod settings;
pub mod sidebar;
pub mod ssh;
pub mod theme;
pub mod widgets;
