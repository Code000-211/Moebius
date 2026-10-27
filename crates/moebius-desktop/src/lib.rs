//! The Moebius desktop app, built to `docs/design.md`. It talks to the server only through the desktop API
//! (`docs/reference/desktop-api.md`) and does not depend on `moebius-core`.

pub mod app;
pub mod board;
pub mod client;
pub mod connect;
pub mod conversation;
pub mod history;
pub mod icons;
pub mod live;
pub mod page;
pub mod settings;
pub mod sidebar;
pub mod soon;
pub mod theme;
