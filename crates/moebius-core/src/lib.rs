//! The core of Moebius: everything that is not a plugin (see `docs/adr/0009-what-belongs-in-the-core.md`).

pub mod config;

/// The version of this build of Moebius.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
