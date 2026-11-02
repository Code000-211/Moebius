//! A change to the clone that a person merges: files written on a branch of the default branch and offered in the
//! inbox as a change of the harness configuration repository, the way the branch of a chat is. The package manager
//! proposes skills this way.

use std::path::Path;

use serde_json::json;

use super::HarnessRepo;
use crate::app::{App, AppError, Result};
use crate::repos::{commit_all, git};
use crate::store::Change;

impl HarnessRepo {
    /// Commits what `write` changes in a worktree of the default branch on `branch`, with `message`, and offers the
    /// branch as a change (`ready_to_merge` in the inbox). Nothing is pushed until a person merges it. A branch of
    /// that name is replaced; what `write` leaves unchanged is refused, and what fails leaves no branch behind.
    pub fn offer_files(
        &self,
        app: &App,
        branch: &str,
        message: &str,
        write: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<Change> {
        let _read_at = self.lock();
        let base = self.repos.default_branch(&self.path)?;
        let name = branch.replace('/', "-");
        let worktree = app
            .repos
            .add_detached_worktree("harness", &self.path, &name, &base)?;
        let committed = (|| -> Result<String> {
            write(&worktree)?;
            if !commit_all(&worktree, message)? {
                return Err(AppError::Invalid(
                    "the harness configuration repository has these files already".into(),
                ));
            }
            Ok(git(&worktree, &["rev-parse", "HEAD"])?)
        })();
        let _ = app.repos.remove_worktree(&self.path, &worktree);
        let head = committed?;
        app.repos.point_branch(&self.path, branch, &head)?;
        let title = message.lines().next().unwrap_or(message);
        let offered = (|| -> Result<Change> {
            let change = app.store.add_harness_change(branch, &head)?;
            app.store.add_inbox_item(
                "ready_to_merge",
                None,
                title,
                &json!({ "change": change.id, "branch": branch }),
            )?;
            Ok(change)
        })();
        if offered.is_err() {
            let _ = git(&self.path, &["branch", "-D", "--quiet", branch]);
        }
        offered
    }
}
