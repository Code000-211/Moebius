//! The core of Moebius: everything that is not a plugin (see `docs/adr/0009-what-belongs-in-the-core.md`).
//!
//! `app` ties the parts together and `server` serves them. The parts, each a folder of modules:
//! `store`, `blobs` and `bus` keep what happens; `repos` are the repositories and their forges; `board` runs the
//! column scripts; `decide` checks every tool call; `router` is the model provider; `harness` runs the agents; `improve`
//! learns from what was recorded; `platform` keeps what differs between Linux and Windows.
//! `docs/architecture.md` has the map of every module.

pub mod app;
pub mod blobs;
pub mod board;
pub mod bus;
pub mod config;
pub mod decide;
pub mod harness;
pub mod improve;
pub mod platform;
pub mod repos;
pub mod router;
pub mod server;
pub mod store;

/// The version of this build of Moebius.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
