//! Repositories and worktrees (spec 3): one clone per repository under the data directory, a worktree and branch per
//! run, and local merges for repositories without a forge. Everything goes through the `git` command line.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Why a git operation failed.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("cannot run git: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("git {args} failed: {stderr}")]
    Failed { args: String, stderr: String },
    #[error("{branch} does not merge cleanly into {into}")]
    Conflict { branch: String, into: String },
}

pub type Result<T> = std::result::Result<T, GitError>;

/// The name Moebius commits and merges under.
const IDENTITY: [&str; 4] = [
    "-c",
    "user.name=Moebius",
    "-c",
    "user.email=moebius@localhost",
];

/// Runs git in `dir` and returns its trimmed standard output.
pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(IDENTITY)
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !output.status.success() {
        return Err(GitError::Failed {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// A worktree made for one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
}

/// The clones and worktrees under the data directory.
#[derive(Debug, Clone)]
pub struct Repos {
    clones: PathBuf,
    worktrees: PathBuf,
}

impl Repos {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            clones: data_dir.join("repos"),
            worktrees: data_dir.join("worktrees"),
        }
    }

    /// Where the clone of the repository `name` lives.
    pub fn clone_path(&self, name: &str) -> PathBuf {
        self.clones.join(name)
    }

    /// Clones `url` as `name`, or fetches if the clone already exists. Returns the clone's path.
    pub fn clone_or_fetch(&self, name: &str, url: &str) -> Result<PathBuf> {
        let path = self.clone_path(name);
        if path.join(".git").exists() {
            self.fetch(&path)?;
        } else {
            std::fs::create_dir_all(&self.clones)?;
            let target = path.to_string_lossy().into_owned();
            git(&self.clones, &["clone", "--quiet", url, &target])?;
        }
        Ok(path)
    }

    pub fn fetch(&self, clone: &Path) -> Result<()> {
        git(clone, &["fetch", "--quiet", "--prune", "origin"]).map(drop)
    }

    /// The branch checked out in the clone, which Moebius treats as its default branch.
    pub fn default_branch(&self, clone: &Path) -> Result<String> {
        git(clone, &["symbolic-ref", "--short", "HEAD"])
    }

    /// Creates a worktree for run `run_id` on a new branch `moebius/run-<id>` starting at `base`.
    pub fn add_worktree(
        &self,
        name: &str,
        clone: &Path,
        run_id: i64,
        base: &str,
    ) -> Result<Worktree> {
        let branch = format!("moebius/run-{run_id}");
        let path = self.worktrees.join(name).join(format!("run-{run_id}"));
        std::fs::create_dir_all(path.parent().expect("worktrees have a parent"))?;
        let target = path.to_string_lossy().into_owned();
        git(
            clone,
            &["worktree", "add", "--quiet", "-b", &branch, &target, base],
        )?;
        Ok(Worktree { path, branch })
    }

    /// Removes a run's worktree; its branch is kept, since the change lives on it.
    pub fn remove_worktree(&self, clone: &Path, worktree: &Worktree) -> Result<()> {
        let target = worktree.path.to_string_lossy().into_owned();
        git(clone, &["worktree", "remove", "--force", &target])?;
        git(clone, &["worktree", "prune"]).map(drop)
    }

    /// Merges `branch` into the default branch checked out in the clone and returns the merge commit. A conflict
    /// leaves the default branch as it was.
    pub fn merge(&self, clone: &Path, branch: &str, message: &str) -> Result<String> {
        let into = self.default_branch(clone)?;
        match git(
            clone,
            &["merge", "--no-ff", "--quiet", "-m", message, branch],
        ) {
            Ok(_) => git(clone, &["rev-parse", "HEAD"]),
            Err(GitError::Failed { .. }) => {
                let _ = git(clone, &["merge", "--abort"]);
                Err(GitError::Conflict {
                    branch: branch.to_string(),
                    into,
                })
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An origin repository with one commit on `main`.
    fn origin(dir: &Path) -> PathBuf {
        let origin = dir.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        origin
    }

    fn commit(dir: &Path, file: &str, text: &str) {
        std::fs::write(dir.join(file), text).unwrap();
        git(dir, &["add", "."]).unwrap();
        git(dir, &["commit", "--quiet", "-m", file]).unwrap();
    }

    #[test]
    fn worktrees_are_created_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let origin = origin(dir.path());
        let repos = Repos::new(&dir.path().join("data"));
        let clone = repos
            .clone_or_fetch("demo", origin.to_str().unwrap())
            .unwrap();
        assert_eq!(repos.default_branch(&clone).unwrap(), "main");
        // A second call fetches instead of cloning.
        repos
            .clone_or_fetch("demo", origin.to_str().unwrap())
            .unwrap();

        let wt = repos.add_worktree("demo", &clone, 7, "main").unwrap();
        assert_eq!(wt.branch, "moebius/run-7");
        assert!(wt.path.join("README.md").is_file());
        repos.remove_worktree(&clone, &wt).unwrap();
        assert!(!wt.path.exists());
        assert!(git(&clone, &["rev-parse", "--verify", "moebius/run-7"]).is_ok());
    }

    #[test]
    fn a_local_merge_lands_on_the_default_branch() {
        let dir = tempfile::tempdir().unwrap();
        let origin = origin(dir.path());
        let repos = Repos::new(&dir.path().join("data"));
        let clone = repos
            .clone_or_fetch("demo", origin.to_str().unwrap())
            .unwrap();
        let wt = repos.add_worktree("demo", &clone, 1, "main").unwrap();
        commit(&wt.path, "feature.txt", "new\n");
        repos.remove_worktree(&clone, &wt).unwrap();

        let sha = repos.merge(&clone, &wt.branch, "Merge run 1").unwrap();
        assert_eq!(git(&clone, &["rev-parse", "main"]).unwrap(), sha);
        assert!(clone.join("feature.txt").is_file());
    }

    #[test]
    fn a_conflicting_merge_is_refused_and_leaves_the_branch_alone() {
        let dir = tempfile::tempdir().unwrap();
        let origin = origin(dir.path());
        let repos = Repos::new(&dir.path().join("data"));
        let clone = repos
            .clone_or_fetch("demo", origin.to_str().unwrap())
            .unwrap();
        let wt = repos.add_worktree("demo", &clone, 2, "main").unwrap();
        commit(&wt.path, "README.md", "theirs\n");
        commit(&clone, "README.md", "ours\n");
        let before = git(&clone, &["rev-parse", "main"]).unwrap();
        assert!(matches!(
            repos.merge(&clone, &wt.branch, "Merge run 2"),
            Err(GitError::Conflict { .. })
        ));
        assert_eq!(git(&clone, &["rev-parse", "main"]).unwrap(), before);
        assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    }
}
