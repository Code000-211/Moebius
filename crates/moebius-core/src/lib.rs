//! The core of Moebius: everything that is not a plugin (see `docs/adr/0009-what-belongs-in-the-core.md`).

pub mod api;
pub mod blobs;
pub mod bus;
pub mod config;
pub mod decide;
pub mod repos;
pub mod store;

/// The version of this build of Moebius.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
