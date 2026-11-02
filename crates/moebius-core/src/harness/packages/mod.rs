//! Pi packages (issue 171): the units of extensions, skills, prompt templates and themes that Pi's catalog
//! (pi.dev/packages) lists, found in the npm registry, vendored into the harness configuration repository and given to
//! agents by name. Nothing here is fetched when a run starts: a package is `packages/<name>/` in the repository with its
//! production dependencies, and `packages.lock` pins what it was fetched as, so that installing, updating and removing
//! one is a change a person reviews and merges like any other configuration change.
//!
//! This folder holds what does not need the repository's clone or the app: `source` (the `npm:` and `git:` sources and
//! the versions a source may ask for), `lock` (the lock file), `manifest` (the resources a package has, from its `pi`
//! manifest or the convention directories), `registry` (the search and the registry's metadata and tarballs), `fetch`
//! (a package's files from a source, with the check of the integrity and `npm install --ignore-scripts` for its
//! dependencies), `changelog` (the sections of a package's changelog between two versions) and `resolve` (the `pkg:`
//! names of an agent's `extensions` and `skills`). The branches of the clone are made in `repo::packages`, and the
//! operations the API offers are in `app::packages`.

mod changelog;
mod fetch;
mod lock;
mod manifest;
mod registry;
mod resolve;
mod source;

pub use changelog::{read as read_changelog, sections_between};
pub use fetch::{Fetched, Latest, Scratch, fetch, latest, place, remove_all};
pub use lock::{Lock, Locked};
pub use manifest::{Resources, install_scripts, package_json, read_resources};
pub use registry::{Registry, SearchHit};
pub use resolve::{Kind, PREFIX, referenced_package, resolve};
pub use source::{Source, valid_package_name};

use crate::app::AppError;
use crate::repos::GitError;

/// The directory of the repository that holds the vendored packages.
pub const PACKAGES_DIR: &str = "packages";

/// The repository's file that pins them.
pub const LOCK_FILE: &str = "packages.lock";

/// Why a package could not be found, fetched or used.
#[derive(Debug, thiserror::Error)]
pub enum PackagesError {
    /// A source, a name, a version or a manifest that cannot be used.
    #[error("{0}")]
    Invalid(String),
    #[error("{0} not found")]
    NotFound(String),
    /// The registry, or a mirror of it, could not be reached or answered something else than was asked.
    #[error("the registry: {0}")]
    Registry(String),
    /// A download that is not what the registry said it is: nothing of it is used.
    #[error(
        "the integrity of {name} does not match: the registry says {expected}, the download is {actual}"
    )]
    Integrity {
        name: String,
        expected: String,
        actual: String,
    },
    #[error("npm failed: {0}")]
    Npm(String),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("cannot read or write files: {0}")]
    Io(#[from] std::io::Error),
}

impl From<PackagesError> for AppError {
    fn from(e: PackagesError) -> Self {
        match e {
            PackagesError::NotFound(what) => AppError::NotFound(what),
            PackagesError::Git(e) => AppError::Git(e),
            PackagesError::Io(e) => AppError::Io(e),
            other => AppError::Invalid(other.to_string()),
        }
    }
}
