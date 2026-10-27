//! Forges: where a repository's issues and changes live (ADR 0009). The first version has the git forge "none", for
//! repositories whose issues and changes live in Moebius and are merged locally; GitHub follows in M2.

use crate::repos::{GitError, Repos};
use crate::store::{Change, Repository};

/// A forge as the core uses it.
pub trait Forge: Send + Sync {
    /// The name repositories refer to it by, such as `none` or `github`.
    fn name(&self) -> &'static str;
    /// Merges `change` into the repository's default branch and returns the merge commit.
    fn merge(
        &self,
        repository: &Repository,
        change: &Change,
        message: &str,
    ) -> Result<String, GitError>;
}

/// The forge for a plain git repository: the clone Moebius keeps is the place changes are merged into.
pub struct NoForge {
    repos: Repos,
}

impl NoForge {
    pub fn new(repos: Repos) -> Self {
        Self { repos }
    }
}

impl Forge for NoForge {
    fn name(&self) -> &'static str {
        "none"
    }

    fn merge(
        &self,
        repository: &Repository,
        change: &Change,
        message: &str,
    ) -> Result<String, GitError> {
        self.repos.merge(
            std::path::Path::new(&repository.path),
            &change.branch,
            message,
        )
    }
}
