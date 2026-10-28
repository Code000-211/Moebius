//! The Moebius desktop app, built to `docs/design.md`. It talks to the server only through the desktop API
//! (`docs/reference/desktop-api.md`) and does not depend on `moebius-core`.

pub mod add_repository;
pub mod agents;
pub mod app;
pub mod board;
pub mod chat;
pub mod client;
pub mod connect;
pub mod conversation;
pub mod decision_model;
pub mod experiments;
pub mod history;
pub mod icons;
pub mod inbox;
pub mod live;
pub mod mcp_servers;
pub mod model_picker;
pub mod new_issue;
pub mod page;
pub mod retro;
pub mod rules;
pub mod search;
pub mod settings;
pub mod setup;
pub mod sidebar;
pub mod skills;
pub mod start;
pub mod terminal;
pub mod theme;
pub mod transcript;
pub mod usage;
