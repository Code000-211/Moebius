//! The core of Moebius: everything that is not a plugin (see `docs/adr/0009-what-belongs-in-the-core.md`).

pub mod api;
pub mod app;
pub mod blobs;
pub mod board;
pub mod bus;
pub mod check;
pub mod config;
pub mod decide;
pub mod forge;
pub mod github;
pub mod guard;
pub mod pibolt;
pub mod poll;
pub mod repos;
pub mod router;
pub mod rules;
pub mod server;
pub mod skill;
pub mod store;
pub mod terminal;

/// The version of this build of Moebius.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
