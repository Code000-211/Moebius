//! Forges: where a repository's issues and changes live (ADR 0009). The git forge "none" is for repositories whose
//! issues and changes live in Moebius and are merged locally; the GitHub forge offers changes as pull requests and
//! merges them through the GitHub API.

use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use tokio::runtime::Handle;

use crate::github::{GitHub, GitHubError};
use crate::repos::{GitError, Repos};
use crate::store::{Card, Change, Repository};

/// Why a forge could not do what it was asked.
#[derive(Debug, thiserror::Error)]
pub enum ForgeError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    GitHub(#[from] GitHubError),
    #[error("{0}")]
    Invalid(String),
}

/// A forge as the core uses it. Its methods block, so they are called from threads that may block.
pub trait Forge: Send + Sync {
    /// The name repositories refer to it by, such as `none` or `github`.
    fn name(&self) -> &'static str;
    /// Offers the work of `card` on `branch` as a change and returns the number of the pull request made for it, if
    /// the forge has them.
    fn offer(
        &self,
        repository: &Repository,
        card: &Card,
        branch: &str,
    ) -> Result<Option<i64>, ForgeError>;
    /// Merges `change` into the repository's default branch and returns the merge commit.
    fn merge(
        &self,
        repository: &Repository,
        change: &Change,
        message: &str,
    ) -> Result<String, ForgeError>;
    /// Brings the clone up to date with the forge, so that new work starts from the latest default branch.
    fn refresh(&self, repository: &Repository) -> Result<(), ForgeError>;
    /// The HTTP header git needs to reach `url`, where the forge knows the repository as `name`; none if it needs
    /// none.
    fn auth(&self, _url: &str, _name: &str) -> Result<Option<String>, ForgeError> {
        Ok(None)
    }
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

    fn offer(&self, _: &Repository, _: &Card, _: &str) -> Result<Option<i64>, ForgeError> {
        Ok(None)
    }

    /// The clone is where changes are merged; there is nothing to catch up with.
    fn refresh(&self, _: &Repository) -> Result<(), ForgeError> {
        Ok(())
    }

    fn merge(
        &self,
        repository: &Repository,
        change: &Change,
        message: &str,
    ) -> Result<String, ForgeError> {
        Ok(self
            .repos
            .merge(Path::new(&repository.path), &change.branch, message)?)
    }
}

/// The forge for repositories on GitHub: a change is a branch pushed with the installation's token and a pull request.
/// The GitHub client is asynchronous, so the methods wait for it on `runtime`; they must be called from a thread that
/// may block, never from an async task.
pub struct GitHubForge {
    github: Arc<GitHub>,
    repos: Repos,
    runtime: Handle,
}

impl GitHubForge {
    pub fn new(github: Arc<GitHub>, repos: Repos, runtime: Handle) -> Self {
        Self {
            github,
            repos,
            runtime,
        }
    }

    /// The repository's name on GitHub, as `owner/repo`.
    fn github_name(repository: &Repository) -> Result<&str, ForgeError> {
        repository.github.as_deref().ok_or_else(|| {
            ForgeError::Invalid(format!("{} is not a GitHub repository", repository.name))
        })
    }

    /// The header for git to reach the clone's `origin` (see `Forge::auth`).
    fn origin_auth(&self, clone: &Path, name: &str) -> Result<Option<String>, ForgeError> {
        self.auth(&self.repos.origin_url(clone)?, name)
    }
}

impl Forge for GitHubForge {
    fn name(&self) -> &'static str {
        "github"
    }

    fn offer(
        &self,
        repository: &Repository,
        card: &Card,
        branch: &str,
    ) -> Result<Option<i64>, ForgeError> {
        let name = Self::github_name(repository)?;
        let clone = Path::new(&repository.path);
        let header = self.origin_auth(clone, name)?;
        self.repos.push(clone, branch, header.as_deref())?;
        let body = format!("Closes #{}\n\nMade by Moebius.", card.number);
        let number = self.runtime.block_on(self.github.open_pull_request(
            name,
            &card.title,
            branch,
            &repository.default_branch,
            &body,
        ))?;
        Ok(Some(number))
    }

    fn merge(
        &self,
        repository: &Repository,
        change: &Change,
        message: &str,
    ) -> Result<String, ForgeError> {
        let name = Self::github_name(repository)?;
        let number = change.pull_request.ok_or_else(|| {
            ForgeError::Invalid(format!("change {} has no pull request", change.id))
        })?;
        Ok(self
            .runtime
            .block_on(self.github.merge_pull_request(name, number, message))?)
    }

    fn refresh(&self, repository: &Repository) -> Result<(), ForgeError> {
        let name = Self::github_name(repository)?;
        let clone = Path::new(&repository.path);
        let header = self.origin_auth(clone, name)?;
        Ok(self.repos.fetch(clone, header.as_deref())?)
    }

    /// The installation's token as a basic-auth header, for web addresses only: any other remote, such as a local
    /// directory, needs none.
    fn auth(&self, url: &str, name: &str) -> Result<Option<String>, ForgeError> {
        if !url.starts_with("https://") {
            return Ok(None);
        }
        let (owner, repo) = name
            .split_once('/')
            .ok_or_else(|| ForgeError::Invalid(format!("{name:?} is not owner/repo")))?;
        let token = self
            .runtime
            .block_on(self.github.installation_token(owner, repo))?;
        let credentials =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
        Ok(Some(format!("Authorization: Basic {credentials}")))
    }
}
