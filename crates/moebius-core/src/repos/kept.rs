//! A clone the server keeps in step with its `origin`: the harness configuration repository ([`crate::harness::repo`])
//! and the wiki ([`crate::wiki`]) are each one. It is cloned once and used as it is after that, so the server starts
//! without the network; a refresh fetches `origin` and fast-forwards the checked-out branch; an edit is a commit on
//! that branch pushed to `origin`, and whatever fails before the push is through leaves the clone as it was, so it
//! never drifts from `origin`. The owner of the clone holds its own lock around each of these.

use std::path::Path;

use super::{GitError, Repos, git};
use crate::app::{App, AppError, Result};
use crate::blobs::redact;

/// Clones `url` to `path` unless a clone is there already; the error says why the clone could not be made.
pub fn clone_unless_there(
    app: &App,
    repos: &Repos,
    url: &str,
    path: &Path,
) -> std::result::Result<(), String> {
    if path.join(".git").exists() {
        return Ok(());
    }
    let header = app.git_auth(url).map_err(|e| e.to_string())?;
    repos
        .clone_into(path, url, header.as_deref())
        .map_err(|e| e.to_string())
}

/// Fetches `origin` and fast-forwards the checked-out branch to it. A branch `origin` does not have yet, as in an
/// empty repository, is left alone.
/// A clone kept without a `url` has no `origin`, so there is nothing to follow.
pub fn follow_origin(app: &App, repos: &Repos, url: Option<&str>, path: &Path) -> Result<()> {
    let Some(url) = url else {
        return Ok(());
    };
    let header = app.git_auth(url)?;
    repos.fetch(path, header.as_deref())?;
    let upstream = format!("origin/{}", repos.default_branch(path)?);
    if git(path, &["rev-parse", "--verify", "--quiet", &upstream]).is_ok() {
        git(path, &["merge", "--ff-only", "--quiet", &upstream])?;
    }
    Ok(())
}

/// The commit the clone has checked out; `None` while its branch has no commit yet.
pub fn head(path: &Path) -> Option<String> {
    git(path, &["rev-parse", "--verify", "--quiet", "HEAD"]).ok()
}

/// A change to the files of a kept clone, as [`commit_pushed`] takes it.
pub struct Change<'a> {
    /// The pathspecs of the files the change touches.
    pub files: &'a [&'a str],
    /// The commit message.
    pub message: &'a str,
    /// The author of the commit as git writes it, `Name <email>`; without one, Moebius is the author.
    pub author: Option<&'a str>,
}

/// Makes what `write` does to the files of `change` a commit on the checked-out branch and pushes it to `origin`.
/// Whatever fails before the push is through, the working tree and the branch are as they were, and a push `origin`
/// refused is a conflict, with any key in git's message redacted. The new commit is given; a change that changed
/// nothing makes none. A branch without a commit yet takes this one as its first. A clone with no `url` has no
/// `origin`: the commit is all there is to it.
pub fn commit_pushed(
    app: &App,
    repos: &Repos,
    url: Option<&str>,
    path: &Path,
    change: &Change,
    write: impl FnOnce() -> Result<()>,
) -> Result<Option<String>> {
    let before = head(path);
    let undo = |error: AppError| {
        match &before {
            Some(before) => {
                let _ = git(path, &["reset", "--hard", "--quiet", before]);
            }
            None => {
                let _ = git(path, &["update-ref", "-d", "HEAD"]);
                let _ = git(path, &["read-tree", "--empty"]);
            }
        }
        let mut clean = vec!["clean", "-fdq", "--"];
        clean.extend(change.files);
        let _ = git(path, &clean);
        error
    };
    let header = match url {
        Some(url) => app.git_auth(url)?,
        None => None,
    };
    write().map_err(undo)?;
    let mut add = vec!["add", "-A", "--"];
    add.extend(change.files);
    git(path, &add).map_err(|e| undo(e.into()))?;
    if git(path, &["diff", "--cached", "--quiet"]).is_ok() {
        return Ok(None);
    }
    let branch = repos.default_branch(path).map_err(|e| undo(e.into()))?;
    let mut commit = vec!["commit", "--quiet", "-m", change.message];
    let author = change.author.map(|author| format!("--author={author}"));
    commit.extend(author.as_deref());
    git(path, &commit).map_err(|e| undo(e.into()))?;
    if url.is_some() {
        repos
            .push(path, &branch, header.as_deref(), false)
            .map_err(|e| undo(refused(e)))?;
    }
    Ok(head(path))
}

/// A push that `origin` refused, or any other failure of git while pushing, as a conflict with the keys in its message
/// redacted, since the forge's refusal may quote one.
fn refused(error: GitError) -> AppError {
    match error {
        GitError::Failed { args, stderr } => AppError::Conflict(
            GitError::Failed {
                args,
                stderr: redact(&stderr),
            }
            .to_string(),
        ),
        other => AppError::Git(other),
    }
}
