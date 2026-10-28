//! The core of Moebius: everything that is not a plugin (see `docs/adr/0009-what-belongs-in-the-core.md`).

pub mod api;
pub mod app;
pub mod blobs;
pub mod board;
pub mod bus;
pub mod check;
pub mod config;
pub mod decide;
pub mod decision_model;
pub mod export;
pub mod files;
pub mod forge;
pub mod github;
pub mod grilling;
pub mod guard;
pub mod harness_repo;
pub mod mcp;
pub mod pibolt;
pub mod poll;
pub mod replay;
pub mod repos;
pub mod research;
pub mod retro;
pub mod router;
pub mod rules;
pub mod scripts;
pub mod server;
pub mod settings;
pub mod setup;
pub mod skill;
pub mod store;
pub mod terminal;
pub mod trial;

/// The version of this build of Moebius.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
