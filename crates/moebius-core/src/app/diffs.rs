//! What a change or a run changed, as a unified diff for a client to draw: the change's branch against the default
//! branch, or a run's work against the commit it started from (its worktree included while it is going on).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{App, Result};
use crate::blobs::redact;
use crate::repos::git_bytes;

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
        let text = git_bytes(dir, args)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default();
        let mut diff = redact(&text);
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
            return Ok(Diff::read(worktree, &["diff", &start]));
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
