//! What `git merge-tree` finds for a change, remembered for as long as neither branch moves.
//!
//! The board asks for a change's conflicts at every tick of a card in Ready to merge or in review. The answer only
//! changes when the head of the base branch or the head of the change's branch does, so it is kept per
//! (clone, base, branch) together with those two heads. The heads are read from the clone's ref files, not through a
//! git process; when a head cannot be read that way (a symbolic ref, an unusual layout) the answer is not kept.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::{Repos, Result};

/// A kept answer: the heads it was found for, and what was found.
type Kept = ((String, String), Option<String>);

/// The kept answers, shared by the clones of `Repos`.
pub(super) type KeptConflicts =
    std::sync::Arc<std::sync::Mutex<std::collections::HashMap<(PathBuf, String, String), Kept>>>;

impl Repos {
    /// The files that conflict when `branch` is merged into `base`, as [`Repos::conflicts`] finds them, but without
    /// running git again while the head of `base` and the head of `branch` are the same as at the last call.
    pub fn conflicts_kept(&self, clone: &Path, base: &str, branch: &str) -> Result<Option<String>> {
        let heads = read_head(clone, base).zip(read_head(clone, branch));
        let key = (clone.to_path_buf(), base.to_string(), branch.to_string());
        if let Some(heads) = &heads
            && let Some((kept_heads, found)) = self.kept_conflicts().get(&key)
            && kept_heads == heads
        {
            return Ok(found.clone());
        }
        let found = self.conflicts(clone, base, branch)?;
        if let Some(heads) = heads {
            self.kept_conflicts().insert(key, (heads, found.clone()));
        }
        Ok(found)
    }

    /// How many times `git merge-tree` was started through [`Repos::conflicts`] since the server started.
    pub fn merge_tree_runs(&self) -> usize {
        self.merge_tree_runs.load(Ordering::Relaxed)
    }

    fn kept_conflicts(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::HashMap<(PathBuf, String, String), Kept>> {
        self.kept_conflicts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
}

/// The commit a branch name points at, read from the ref files of the clone: `main` is `refs/heads/main`, and
/// `origin/main` is `refs/remotes/origin/main`. None when it cannot be told that way.
fn read_head(clone: &Path, name: &str) -> Option<String> {
    let dot_git = clone.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        clone.to_path_buf()
    };
    let candidates = match name.strip_prefix("origin/") {
        Some(rest) => vec![format!("refs/remotes/origin/{rest}")],
        None => vec![format!("refs/heads/{name}")],
    };
    for reference in candidates {
        if let Ok(text) = std::fs::read_to_string(git_dir.join(&reference)) {
            let text = text.trim();
            return is_object_id(text).then(|| text.to_string());
        }
        if let Ok(packed) = std::fs::read_to_string(git_dir.join("packed-refs")) {
            for line in packed.lines() {
                if let Some((id, found)) = line.split_once(' ')
                    && found == reference
                    && is_object_id(id)
                {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repos::git;

    fn clone_with_branch() -> (tempfile::TempDir, PathBuf, Repos) {
        let dir = tempfile::tempdir().unwrap();
        let clone = dir.path().join("clone");
        std::fs::create_dir(&clone).unwrap();
        git(&clone, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(clone.join("a"), "1\n").unwrap();
        git(&clone, &["add", "."]).unwrap();
        git(&clone, &["commit", "--quiet", "-m", "one"]).unwrap();
        git(&clone, &["branch", "work"]).unwrap();
        let repos = Repos::new(dir.path());
        (dir, clone, repos)
    }

    #[test]
    fn a_change_is_not_merged_again_while_neither_branch_moves() {
        let (_dir, clone, repos) = clone_with_branch();
        for _ in 0..5 {
            assert_eq!(repos.conflicts_kept(&clone, "main", "work").unwrap(), None);
        }
        assert_eq!(repos.merge_tree_runs(), 1);
    }

    #[test]
    fn a_moved_branch_is_looked_at_again() {
        let (_dir, clone, repos) = clone_with_branch();
        assert_eq!(repos.conflicts_kept(&clone, "main", "work").unwrap(), None);
        git(&clone, &["checkout", "--quiet", "work"]).unwrap();
        std::fs::write(clone.join("a"), "work\n").unwrap();
        git(&clone, &["commit", "--quiet", "-am", "work"]).unwrap();
        git(&clone, &["checkout", "--quiet", "main"]).unwrap();
        assert_eq!(repos.conflicts_kept(&clone, "main", "work").unwrap(), None);
        assert_eq!(repos.merge_tree_runs(), 2);
        // The base moves with a change to the same line: now they conflict, and packed refs are read too.
        std::fs::write(clone.join("a"), "main\n").unwrap();
        git(&clone, &["commit", "--quiet", "-am", "main"]).unwrap();
        git(&clone, &["pack-refs", "--all"]).unwrap();
        assert_eq!(
            repos.conflicts_kept(&clone, "main", "work").unwrap(),
            Some("a".into())
        );
        assert_eq!(repos.merge_tree_runs(), 3);
        assert_eq!(
            repos.conflicts_kept(&clone, "main", "work").unwrap(),
            Some("a".into())
        );
        assert_eq!(repos.merge_tree_runs(), 3);
    }
}
