//! What a change or a run changed, as a unified diff for a client to draw: the change's branch against the default
//! branch, or a run's work against the commit it started from (its worktree included while it is going on).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{App, Result};
use crate::blobs::redact;
use crate::repos::{git_bytes, git_bytes_env};

/// The most bytes of a diff one answer carries; a longer one is cut and says so.
pub const DIFF_LIMIT: usize = 2_000_000;

/// A unified diff, secrets removed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diff {
    /// Empty when nothing changed or the branch is gone.
    pub diff: String,
    /// Whether the diff was cut at [`DIFF_LIMIT`].
    pub truncated: bool,
}

impl Diff {
    fn read(dir: &Path, args: &[&str]) -> Diff {
        Diff::from_bytes(&git_bytes(dir, args).unwrap_or_default())
    }

    fn from_bytes(bytes: &[u8]) -> Diff {
        let mut diff = redact(&String::from_utf8_lossy(bytes));
        let truncated = diff.len() > DIFF_LIMIT;
        if truncated {
            let mut end = DIFF_LIMIT;
            while !diff.is_char_boundary(end) {
                end -= 1;
            }
            diff.truncate(end);
        }
        Diff { diff, truncated }
    }
}

/// Everything in `worktree` since `start`, files not yet added included: the files are added to a temporary copy of the
/// index, so the worktree's own index stays as the agent left it. Without the copy, only tracked paths are shown.
fn worktree_diff(worktree: &Path, start: &str, scratch: &Path) -> Diff {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let index = scratch.join(format!(
        "diff-{}-{}.index",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let env = [("GIT_INDEX_FILE", index.as_path())];
    let staged = git_bytes_env(worktree, &["read-tree", "HEAD"], &env)
        .and_then(|_| git_bytes_env(worktree, &["add", "-A"], &env))
        .and_then(|_| git_bytes_env(worktree, &["diff", "--cached", start], &env));
    let _ = std::fs::remove_file(&index);
    match staged {
        Ok(bytes) => Diff::from_bytes(&bytes),
        Err(_) => Diff::read(worktree, &["diff", start]),
    }
}

impl App {
    /// The change `id` against the default branch of its repository.
    pub fn change_diff(&self, id: i64) -> Result<Diff> {
        let change = self.change(id)?;
        let (clone, base) = self.clone_and_base(change.repository_id, change.harness_repository)?;
        let range = format!("{base}...{}", change.branch);
        Ok(Diff::read(&clone, &["diff", &range]))
    }

    /// What the run `id` changed: while its worktree exists, everything in it since it started, uncommitted work
    /// included; afterwards its branch against the commit it started from.
    pub fn run_diff(&self, id: i64) -> Result<Diff> {
        let run = self.run(id)?;
        let repository = match run.repository_id {
            Some(repository) => Some(repository),
            None => match run.card_id {
                Some(card) => Some(self.card(card)?.repository_id),
                None => None,
            },
        };
        if repository.is_none() && !run.harness_repository {
            // A chat in an empty directory has no repository to compare with.
            return Ok(Diff::default());
        }
        let (clone, default) = self.clone_and_base(repository, run.harness_repository)?;
        let base = self.store.run_base(id)?.unwrap_or(default);
        let worktree = run
            .worktree
            .as_deref()
            .map(Path::new)
            .filter(|dir| run.ended_at.is_none() && dir.is_dir());
        if let Some(worktree) = worktree {
            let start = git_bytes(worktree, &["merge-base", &base, "HEAD"])
                .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string())
                .unwrap_or(base);
            return Ok(worktree_diff(worktree, &start, &self.data_dir));
        }
        let range = format!("{base}...{}", run.branch());
        Ok(Diff::read(&clone, &["diff", &range]))
    }

    /// The clone a repository (or the harness configuration repository) lives in, and its default branch.
    fn clone_and_base(
        &self,
        repository: Option<i64>,
        harness_repository: bool,
    ) -> Result<(PathBuf, String)> {
        if harness_repository || repository.is_none() {
            let repo = self.harness_repo()?;
            let base = self.repos.default_branch(repo.path())?;
            return Ok((repo.path().to_path_buf(), base));
        }
        let repository = self.repository(repository.unwrap_or_default())?;
        Ok((PathBuf::from(&repository.path), repository.base_ref()))
    }
}
