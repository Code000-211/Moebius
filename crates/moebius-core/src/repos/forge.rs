//! Forges: where a repository's issues and changes live (ADR 0009). The git forge "none" is for repositories whose
//! issues and changes live in Moebius and are merged locally; the GitHub forge offers changes as pull requests and
//! merges them through the GitHub API, the GitLab forge does the same with merge requests, and the Forgejo forge with
//! pull requests on Codeberg, Forgejo or Gitea. An issue as every forge lists it is an [`Issue`], which the poll hands
//! to the board.

use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use tokio::runtime::Handle;

use crate::repos::forgejo::{Forgejo, ForgejoError};
use crate::repos::github::{GitHub, GitHubError};
use crate::repos::gitlab::{GitLab, GitLabError};
use crate::repos::{GitError, Repos, git};
use crate::store::{Change, Repository};

/// Why a forge could not do what it was asked.
#[derive(Debug, thiserror::Error)]
pub enum ForgeError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    GitHub(#[from] GitHubError),
    #[error(transparent)]
    GitLab(#[from] GitLabError),
    #[error(transparent)]
    Forgejo(#[from] ForgejoError),
    #[error("{0}")]
    Invalid(String),
}

/// An issue on a forge, the same for every forge: its number there, its title and body, whether it is open, and the
/// login of whoever opened it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Issue {
    pub number: i64,
    pub title: String,
    pub body: String,
    pub open: bool,
    pub author: String,
    /// The names of its labels.
    pub labels: Vec<String>,
    /// The number of its parent issue, where the forge has sub-issues; the board also reads "Part of #n" from the body.
    pub parent: Option<i64>,
    /// The numbers of the issues that block it, where the forge keeps blocking and says so; none where it does not (the
    /// board also reads "Blocked by #n" from the body).
    pub blocked_by: Option<Vec<i64>>,
    /// How many comments the forge says it has.
    pub comment_count: i64,
    /// Its comments, once the poll has read them because they changed; none when it did not.
    pub comments: Option<Vec<Comment>>,
    /// When the forge says the issue last changed, as the forge writes it; what a read of the issue's blockers is kept
    /// with.
    pub updated: String,
    /// Whether the poll still has to read the issue's blockers (`blocked_by`) from the forge: it can, and the issue
    /// may have some.
    pub blockers_unread: bool,
}

/// A comment on an issue as a forge has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// Its id on the forge.
    pub id: i64,
    pub author: String,
    pub body: String,
    /// When it was written, in seconds since the epoch; 0 when the forge's time could not be read.
    pub at: i64,
}

/// Seconds since the epoch of a time the forges write in RFC 3339, 0 when it is not one.
pub fn seconds(time: &str) -> i64 {
    time::OffsetDateTime::parse(time, &time::format_description::well_known::Rfc3339)
        .map_or(0, |t| t.unix_timestamp())
}

/// What a poll read about each issue of a forge that has no list of it (the issues blocking an issue), kept with the
/// time the issue last changed, so that it is read again only for an issue that changed, or when the answer is old:
/// a forge may not say that a link changed in the time its issue did.
pub struct Remembered {
    values: std::sync::Mutex<RememberedMap>,
    lifetime: std::time::Duration,
}

/// The values by repository and issue number, each with the time of the issue they were read at and when they were read.
type RememberedMap =
    std::collections::HashMap<(String, i64), (String, Vec<i64>, std::time::Instant)>;

impl Default for Remembered {
    fn default() -> Self {
        Self::lasting(std::time::Duration::from_secs(600))
    }
}

impl Remembered {
    /// A memory whose answers are read again once they are `lifetime` old.
    pub fn lasting(lifetime: std::time::Duration) -> Self {
        Self {
            values: Default::default(),
            lifetime,
        }
    }

    /// What was read for issue `number` of `name` when it was last changed at `stamp`, if it is still that time and the
    /// answer is not old.
    pub fn get(&self, name: &str, number: i64, stamp: &str) -> Option<Vec<i64>> {
        let remembered = self.values.lock().unwrap_or_else(|e| e.into_inner());
        let (at, value, read) = remembered.get(&(name.to_string(), number))?;
        (at == stamp && read.elapsed() < self.lifetime).then(|| value.clone())
    }

    /// The issues of `name` whose answer is old enough to be read again, oldest first and at most `limit` of them, each
    /// with the time of the issue it was read at (which a new read passes to [`Remembered::get`] and `put`). A poll that
    /// the forge answers with "not modified" never lists them, so it reads these itself, a few at a time.
    pub fn due(&self, name: &str, limit: usize) -> Vec<(i64, String)> {
        let remembered = self.values.lock().unwrap_or_else(|e| e.into_inner());
        let mut old: Vec<_> = remembered
            .iter()
            .filter(|((n, _), (_, _, read))| n == name && read.elapsed() >= self.lifetime)
            .map(|((_, number), (stamp, _, read))| (*read, *number, stamp.clone()))
            .collect();
        old.sort();
        old.into_iter()
            .take(limit)
            .map(|(_, number, stamp)| (number, stamp))
            .collect()
    }

    /// Forgets the answers for the issues of `name` that `keep` does not name: the issue is closed or gone.
    pub fn retain(&self, name: &str, keep: impl Fn(i64) -> bool) {
        let mut remembered = self.values.lock().unwrap_or_else(|e| e.into_inner());
        remembered.retain(|(n, number), _| n != name || keep(*number));
    }

    pub fn put(&self, name: &str, number: i64, stamp: &str, value: Vec<i64>) {
        let mut remembered = self.values.lock().unwrap_or_else(|e| e.into_inner());
        remembered.insert(
            (name.to_string(), number),
            (stamp.to_string(), value, std::time::Instant::now()),
        );
    }
}

fn not_on_a_forge(repository: &Repository) -> ForgeError {
    ForgeError::Invalid(format!(
        "the issues of {} are not kept on a forge",
        repository.name
    ))
}

/// The `Authorization` header git sends to reach a forge over HTTPS as `user` with `token` as the password.
fn basic_auth(user: &str, token: &str) -> String {
    let credentials = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{token}"));
    format!("Authorization: Basic {credentials}")
}

/// A forge as the core uses it. Its methods block, so they are called from threads that may block.
pub trait Forge: Send + Sync {
    /// The name repositories refer to it by, such as `none` or `github`.
    fn name(&self) -> &'static str;
    /// Makes the work on `branch` known to the forge, as a change with `title` and `body`, and returns the number of
    /// the pull request for it if the forge has them. Called again when `branch` has moved on; `pull_request` is the
    /// number returned before, so the forge updates the change it offered instead of offering another.
    fn publish(
        &self,
        repository: &Repository,
        title: &str,
        body: &str,
        branch: &str,
        pull_request: Option<i64>,
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
    /// Opens an issue on the forge and returns its number. Only forges that keep issues have it; the issues of a
    /// repository without a forge are cards of Moebius.
    fn create_issue(
        &self,
        repository: &Repository,
        _title: &str,
        _body: &str,
    ) -> Result<i64, ForgeError> {
        Err(ForgeError::Invalid(format!(
            "the issues of {} are not kept on a forge",
            repository.name
        )))
    }
    /// Replaces the title and body of the issue `number` on the forge. Only forges that keep issues have it.
    fn update_issue(
        &self,
        repository: &Repository,
        _number: i64,
        _title: &str,
        _body: &str,
    ) -> Result<(), ForgeError> {
        Err(ForgeError::Invalid(format!(
            "the issues of {} are not kept on a forge",
            repository.name
        )))
    }
    /// Closes the issue `number` on the forge, or reopens it. Only forges that keep issues have it.
    fn set_issue_open(
        &self,
        repository: &Repository,
        _number: i64,
        _open: bool,
    ) -> Result<(), ForgeError> {
        Err(not_on_a_forge(repository))
    }
    /// Replaces the labels of the issue `number` on the forge.
    fn set_issue_labels(
        &self,
        repository: &Repository,
        _number: i64,
        _labels: &[String],
    ) -> Result<(), ForgeError> {
        Err(not_on_a_forge(repository))
    }
    /// Writes a comment on the issue `number` and returns its id on the forge.
    fn comment_issue(
        &self,
        repository: &Repository,
        _number: i64,
        _body: &str,
    ) -> Result<i64, ForgeError> {
        Err(not_on_a_forge(repository))
    }
    /// Whether the forge keeps the parent of an issue itself (GitHub's sub-issues). Where it does not, the parent is
    /// a line "Part of #n" in the issue's body, which the board writes with `update_issue`.
    fn has_sub_issues(&self) -> bool {
        false
    }
    /// Makes the issue `number` a sub-issue of `parent`, or takes it from `previous` when `parent` is none. Only
    /// where [`Forge::has_sub_issues`].
    fn set_issue_parent(
        &self,
        repository: &Repository,
        _number: i64,
        _parent: Option<i64>,
        _previous: Option<i64>,
    ) -> Result<(), ForgeError> {
        Err(not_on_a_forge(repository))
    }
    /// Makes the issue `blocker` block the issue `number` on the forge, or stops it from blocking. Says whether the
    /// forge holds the blocking itself; when it does not (GitLab without Premium links issues but not as blocking),
    /// the board writes, or removes, a line "Blocked by #n" in the issue's description.
    fn set_issue_blocker(
        &self,
        repository: &Repository,
        _number: i64,
        _blocker: i64,
        _blocks: bool,
    ) -> Result<bool, ForgeError> {
        Err(not_on_a_forge(repository))
    }
    /// The description the forge has for the issue `number` now, which the board edits instead of what it read at the
    /// last poll. Only where the board writes into the description (no sub-issues, or no blocking).
    fn issue_body(&self, repository: &Repository, _number: i64) -> Result<String, ForgeError> {
        Err(not_on_a_forge(repository))
    }
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

    /// The clone is where changes are merged; there is nothing to publish to.
    fn publish(
        &self,
        _: &Repository,
        _: &str,
        _: &str,
        _: &str,
        _: Option<i64>,
    ) -> Result<Option<i64>, ForgeError> {
        Ok(None)
    }

    /// Fetches `origin` and fast-forwards the clone's default branch to it, so that work committed in the repository
    /// the clone came from arrives. A default branch that cannot fast-forward, because Moebius merged changes into it,
    /// is left as it is.
    fn refresh(&self, repository: &Repository) -> Result<(), ForgeError> {
        let clone = Path::new(&repository.path);
        let lock = self.repos.clone_lock(clone);
        let _refreshing = lock.lock().unwrap_or_else(|e| e.into_inner());
        self.repos.fetch(clone, None)?;
        let upstream = format!("origin/{}", repository.default_branch);
        match git(clone, &["merge", "--ff-only", "--quiet", &upstream]) {
            Ok(_) | Err(GitError::Failed { .. }) => Ok(()),
            Err(e) => Err(e.into()),
        }
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

/// The forge for repositories on GitHub: a change is a branch pushed with the installation's token (or the access token) and a pull request,
/// which later pushes update.
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

    fn publish(
        &self,
        repository: &Repository,
        title: &str,
        body: &str,
        branch: &str,
        pull_request: Option<i64>,
    ) -> Result<Option<i64>, ForgeError> {
        let name = Self::github_name(repository)?;
        let clone = Path::new(&repository.path);
        let header = self.origin_auth(clone, name)?;
        self.repos.push(clone, branch, header.as_deref(), true)?;
        if pull_request.is_some() {
            return Ok(pull_request);
        }
        let number = self.runtime.block_on(self.github.open_pull_request(
            name,
            title,
            branch,
            &repository.default_branch,
            body,
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

    fn create_issue(
        &self,
        repository: &Repository,
        title: &str,
        body: &str,
    ) -> Result<i64, ForgeError> {
        let name = Self::github_name(repository)?;
        Ok(self
            .runtime
            .block_on(self.github.create_issue(name, title, body))?)
    }

    fn update_issue(
        &self,
        repository: &Repository,
        number: i64,
        title: &str,
        body: &str,
    ) -> Result<(), ForgeError> {
        let name = Self::github_name(repository)?;
        Ok(self
            .runtime
            .block_on(self.github.update_issue(name, number, title, body))?)
    }

    fn set_issue_open(
        &self,
        repository: &Repository,
        number: i64,
        open: bool,
    ) -> Result<(), ForgeError> {
        let name = Self::github_name(repository)?;
        Ok(self
            .runtime
            .block_on(self.github.set_issue_state(name, number, open))?)
    }

    fn set_issue_labels(
        &self,
        repository: &Repository,
        number: i64,
        labels: &[String],
    ) -> Result<(), ForgeError> {
        let name = Self::github_name(repository)?;
        Ok(self
            .runtime
            .block_on(self.github.set_issue_labels(name, number, labels))?)
    }

    fn comment_issue(
        &self,
        repository: &Repository,
        number: i64,
        body: &str,
    ) -> Result<i64, ForgeError> {
        let name = Self::github_name(repository)?;
        Ok(self
            .runtime
            .block_on(self.github.create_issue_comment(name, number, body))?)
    }

    fn set_issue_blocker(
        &self,
        repository: &Repository,
        number: i64,
        blocker: i64,
        blocks: bool,
    ) -> Result<bool, ForgeError> {
        let name = Self::github_name(repository)?;
        if blocks {
            self.runtime
                .block_on(self.github.add_blocker(name, number, blocker))?;
        } else {
            self.runtime
                .block_on(self.github.remove_blocker(name, number, blocker))?;
        }
        Ok(true)
    }

    fn has_sub_issues(&self) -> bool {
        true
    }

    fn set_issue_parent(
        &self,
        repository: &Repository,
        number: i64,
        parent: Option<i64>,
        previous: Option<i64>,
    ) -> Result<(), ForgeError> {
        let name = Self::github_name(repository)?;
        Ok(self
            .runtime
            .block_on(self.github.set_issue_parent(name, number, parent, previous))?)
    }

    fn refresh(&self, repository: &Repository) -> Result<(), ForgeError> {
        let name = Self::github_name(repository)?;
        let clone = Path::new(&repository.path);
        let header = self.origin_auth(clone, name)?;
        Ok(self.repos.fetch(clone, header.as_deref())?)
    }

    /// The installation's token, or the access token, as a basic-auth header, for addresses on this GitHub's web host only: the token goes
    /// to no other host, and a local directory needs none.
    fn auth(&self, url: &str, name: &str) -> Result<Option<String>, ForgeError> {
        if !self.github.is_web_url(url) {
            return Ok(None);
        }
        let (owner, repo) = name
            .split_once('/')
            .ok_or_else(|| ForgeError::Invalid(format!("{name:?} is not owner/repo")))?;
        let token = self
            .runtime
            .block_on(self.github.access_token(owner, repo))?;
        Ok(Some(basic_auth("x-access-token", &token)))
    }
}

/// The forge for repositories on GitLab: a change is a branch pushed with the access token and a merge request, which
/// later pushes update. Like [`GitHubForge`], its methods wait for the asynchronous client on `runtime`, so they must
/// be called from a thread that may block.
pub struct GitLabForge {
    gitlab: Arc<GitLab>,
    repos: Repos,
    runtime: Handle,
}

impl GitLabForge {
    pub fn new(gitlab: Arc<GitLab>, repos: Repos, runtime: Handle) -> Self {
        Self {
            gitlab,
            repos,
            runtime,
        }
    }

    /// The repository's path on GitLab, such as `group/project`.
    fn project(repository: &Repository) -> Result<&str, ForgeError> {
        repository
            .project
            .as_deref()
            .filter(|_| repository.forge == "gitlab")
            .ok_or_else(|| {
                ForgeError::Invalid(format!("{} is not a GitLab repository", repository.name))
            })
    }

    /// The header for git to reach the clone's `origin` (see `Forge::auth`).
    fn origin_auth(&self, clone: &Path) -> Result<Option<String>, ForgeError> {
        self.auth(&self.repos.origin_url(clone)?, "")
    }
}

impl Forge for GitLabForge {
    fn name(&self) -> &'static str {
        "gitlab"
    }

    fn publish(
        &self,
        repository: &Repository,
        title: &str,
        body: &str,
        branch: &str,
        merge_request: Option<i64>,
    ) -> Result<Option<i64>, ForgeError> {
        let project = Self::project(repository)?;
        let clone = Path::new(&repository.path);
        let header = self.origin_auth(clone)?;
        self.repos.push(clone, branch, header.as_deref(), true)?;
        if merge_request.is_some() {
            return Ok(merge_request);
        }
        let number = self.runtime.block_on(self.gitlab.open_merge_request(
            project,
            title,
            branch,
            &repository.default_branch,
            body,
        ))?;
        Ok(Some(number))
    }

    fn merge(
        &self,
        repository: &Repository,
        change: &Change,
        message: &str,
    ) -> Result<String, ForgeError> {
        let project = Self::project(repository)?;
        let number = change.pull_request.ok_or_else(|| {
            ForgeError::Invalid(format!("change {} has no merge request", change.id))
        })?;
        Ok(self
            .runtime
            .block_on(self.gitlab.merge(project, number, message))?)
    }

    fn create_issue(
        &self,
        repository: &Repository,
        title: &str,
        body: &str,
    ) -> Result<i64, ForgeError> {
        let project = Self::project(repository)?;
        Ok(self
            .runtime
            .block_on(self.gitlab.create_issue(project, title, body))?)
    }

    fn update_issue(
        &self,
        repository: &Repository,
        number: i64,
        title: &str,
        body: &str,
    ) -> Result<(), ForgeError> {
        let project = Self::project(repository)?;
        Ok(self
            .runtime
            .block_on(self.gitlab.update_issue(project, number, title, body))?)
    }

    fn set_issue_open(
        &self,
        repository: &Repository,
        number: i64,
        open: bool,
    ) -> Result<(), ForgeError> {
        let name = Self::project(repository)?;
        Ok(self
            .runtime
            .block_on(self.gitlab.set_issue_state(name, number, open))?)
    }

    fn set_issue_labels(
        &self,
        repository: &Repository,
        number: i64,
        labels: &[String],
    ) -> Result<(), ForgeError> {
        let name = Self::project(repository)?;
        Ok(self
            .runtime
            .block_on(self.gitlab.set_issue_labels(name, number, labels))?)
    }

    fn comment_issue(
        &self,
        repository: &Repository,
        number: i64,
        body: &str,
    ) -> Result<i64, ForgeError> {
        let name = Self::project(repository)?;
        Ok(self
            .runtime
            .block_on(self.gitlab.create_note(name, number, body))?)
    }

    fn set_issue_blocker(
        &self,
        repository: &Repository,
        number: i64,
        blocker: i64,
        blocks: bool,
    ) -> Result<bool, ForgeError> {
        let name = Self::project(repository)?;
        Ok(if blocks {
            self.runtime
                .block_on(self.gitlab.add_blocker(name, number, blocker))?
        } else {
            self.runtime
                .block_on(self.gitlab.remove_blocker(name, number, blocker))?
        })
    }

    fn issue_body(&self, repository: &Repository, number: i64) -> Result<String, ForgeError> {
        let name = Self::project(repository)?;
        Ok(self
            .runtime
            .block_on(self.gitlab.issue_description(name, number))?)
    }

    fn refresh(&self, repository: &Repository) -> Result<(), ForgeError> {
        Self::project(repository)?;
        let clone = Path::new(&repository.path);
        let header = self.origin_auth(clone)?;
        Ok(self.repos.fetch(clone, header.as_deref())?)
    }

    /// The access token as a basic-auth header, for addresses on this GitLab only: the token goes to no other host,
    /// and a local directory needs none.
    fn auth(&self, url: &str, _name: &str) -> Result<Option<String>, ForgeError> {
        if !url.starts_with(&format!("{}/", self.gitlab.url())) {
            return Ok(None);
        }
        Ok(Some(basic_auth("oauth2", self.gitlab.token())))
    }
}

/// The forge for repositories on Forgejo or Gitea, such as Codeberg: a change is a branch pushed with the access token
/// and a pull request, which later pushes update. Like [`GitHubForge`], its methods wait for the asynchronous client on
/// `runtime`, so they must be called from a thread that may block.
pub struct ForgejoForge {
    forgejo: Arc<Forgejo>,
    repos: Repos,
    runtime: Handle,
}

impl ForgejoForge {
    pub fn new(forgejo: Arc<Forgejo>, repos: Repos, runtime: Handle) -> Self {
        Self {
            forgejo,
            repos,
            runtime,
        }
    }

    /// The repository's name on Forgejo, as `owner/repo`.
    fn repo(repository: &Repository) -> Result<&str, ForgeError> {
        repository
            .project
            .as_deref()
            .filter(|_| repository.forge == "forgejo")
            .ok_or_else(|| {
                ForgeError::Invalid(format!("{} is not a Forgejo repository", repository.name))
            })
    }

    /// The header for git to reach the clone's `origin` (see `Forge::auth`).
    fn origin_auth(&self, clone: &Path) -> Result<Option<String>, ForgeError> {
        self.auth(&self.repos.origin_url(clone)?, "")
    }
}

impl Forge for ForgejoForge {
    fn name(&self) -> &'static str {
        "forgejo"
    }

    fn publish(
        &self,
        repository: &Repository,
        title: &str,
        body: &str,
        branch: &str,
        pull_request: Option<i64>,
    ) -> Result<Option<i64>, ForgeError> {
        let repo = Self::repo(repository)?;
        let clone = Path::new(&repository.path);
        let header = self.origin_auth(clone)?;
        self.repos.push(clone, branch, header.as_deref(), true)?;
        if pull_request.is_some() {
            return Ok(pull_request);
        }
        let number = self.runtime.block_on(self.forgejo.open_pull_request(
            repo,
            title,
            branch,
            &repository.default_branch,
            body,
        ))?;
        Ok(Some(number))
    }

    fn merge(
        &self,
        repository: &Repository,
        change: &Change,
        message: &str,
    ) -> Result<String, ForgeError> {
        let repo = Self::repo(repository)?;
        let number = change.pull_request.ok_or_else(|| {
            ForgeError::Invalid(format!("change {} has no pull request", change.id))
        })?;
        Ok(self
            .runtime
            .block_on(self.forgejo.merge(repo, number, message))?)
    }

    fn create_issue(
        &self,
        repository: &Repository,
        title: &str,
        body: &str,
    ) -> Result<i64, ForgeError> {
        let repo = Self::repo(repository)?;
        Ok(self
            .runtime
            .block_on(self.forgejo.create_issue(repo, title, body))?)
    }

    fn update_issue(
        &self,
        repository: &Repository,
        number: i64,
        title: &str,
        body: &str,
    ) -> Result<(), ForgeError> {
        let repo = Self::repo(repository)?;
        Ok(self
            .runtime
            .block_on(self.forgejo.update_issue(repo, number, title, body))?)
    }

    fn set_issue_open(
        &self,
        repository: &Repository,
        number: i64,
        open: bool,
    ) -> Result<(), ForgeError> {
        let name = Self::repo(repository)?;
        Ok(self
            .runtime
            .block_on(self.forgejo.set_issue_state(name, number, open))?)
    }

    fn set_issue_labels(
        &self,
        repository: &Repository,
        number: i64,
        labels: &[String],
    ) -> Result<(), ForgeError> {
        let name = Self::repo(repository)?;
        Ok(self
            .runtime
            .block_on(self.forgejo.set_issue_labels(name, number, labels))?)
    }

    fn comment_issue(
        &self,
        repository: &Repository,
        number: i64,
        body: &str,
    ) -> Result<i64, ForgeError> {
        let name = Self::repo(repository)?;
        Ok(self
            .runtime
            .block_on(self.forgejo.create_issue_comment(name, number, body))?)
    }

    fn set_issue_blocker(
        &self,
        repository: &Repository,
        number: i64,
        blocker: i64,
        blocks: bool,
    ) -> Result<bool, ForgeError> {
        let name = Self::repo(repository)?;
        if blocks {
            self.runtime
                .block_on(self.forgejo.add_blocker(name, number, blocker))?;
        } else {
            self.runtime
                .block_on(self.forgejo.remove_blocker(name, number, blocker))?;
        }
        Ok(true)
    }

    fn issue_body(&self, repository: &Repository, number: i64) -> Result<String, ForgeError> {
        let name = Self::repo(repository)?;
        Ok(self
            .runtime
            .block_on(self.forgejo.issue_body(name, number))?)
    }

    fn refresh(&self, repository: &Repository) -> Result<(), ForgeError> {
        Self::repo(repository)?;
        let clone = Path::new(&repository.path);
        let header = self.origin_auth(clone)?;
        Ok(self.repos.fetch(clone, header.as_deref())?)
    }

    /// The access token as a basic-auth header, the token as the user name, for addresses on this Forgejo only: the
    /// token goes to no other host, and a local directory needs none.
    fn auth(&self, url: &str, _name: &str) -> Result<Option<String>, ForgeError> {
        if !url.starts_with(&format!("{}/", self.forgejo.url())) {
            return Ok(None);
        }
        Ok(Some(basic_auth(self.forgejo.token(), "x-oauth-basic")))
    }
}
