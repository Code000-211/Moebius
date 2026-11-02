//! The Pi packages in the clone (see `harness::packages`): what the default branch vendors, which agents name a
//! package, and the branch that installs, updates or removes one. A branch is made in a throwaway worktree, so the
//! clone's working tree is never touched, and is offered as a change that a person merges.

use std::path::Path;

use super::HarnessRepo;
use super::read::{AgentSettings, agent_names};
use crate::app::{App, AppError, Result};
use crate::harness::packages::{Lock, PACKAGES_DIR, referenced_package};
use crate::repos::git;

/// Why an agent is refused the merge of a change that touches a package.
pub const PERSON_ONLY: &str =
    "only a person can merge a change that installs, updates or removes a package";

/// The most characters of a package's file diff that are kept.
const DIFF_LIMIT: usize = 100_000;

/// The files of a package that a branch changes, and their diff against the default branch.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageDiff {
    /// Each file as `A path`, `M path` or `D path`, without what npm installed below `node_modules`.
    pub files: Vec<String>,
    /// How many files below `node_modules` the branch changes.
    pub dependency_files: usize,
    /// The diff of the files above; cut off when it is long.
    pub diff: String,
}

impl HarnessRepo {
    /// The lock of the default branch.
    pub fn package_lock(&self) -> Result<Lock> {
        let _read_at = self.lock();
        Ok(Lock::read(&self.path)?)
    }

    /// The lock of the worktree at `root`, which is a branch's.
    pub fn package_lock_at(&self, root: &Path) -> Result<Lock> {
        Ok(Lock::read(root)?)
    }

    /// The agents of the default branch that name a resource of the package `package`.
    pub fn agents_naming(&self, package: &str) -> Vec<String> {
        let _read_at = self.lock();
        agents_naming_at(&self.path, package)
    }

    /// Makes the branch `moebius/ext-<slug>-<n>` of the default branch, with one commit of what `write` does to a
    /// worktree of it; the branch is given. What `write` leaves is committed even if the repository ignores it, since
    /// the dependencies of a package are in `node_modules`. A `write` that fails, and one that changes nothing, leave
    /// no branch.
    pub fn branch_with(
        &self,
        app: &App,
        slug: &str,
        message: &str,
        write: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<String> {
        let _read_at = self.lock();
        let base = self.repos.default_branch(&self.path)?;
        let exists = |branch: &str| {
            git(
                &self.path,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{branch}"),
                ],
            )
            .is_ok()
        };
        let n = (1..)
            .find(|n| !exists(&format!("moebius/ext-{slug}-{n}")))
            .unwrap_or(1);
        let name = format!("ext-{slug}-{n}");
        let branch = format!("moebius/{name}");
        let worktree = app
            .repos
            .add_detached_worktree("harness", &self.path, &name, &base)?;
        let made = (|| {
            git(&worktree, &["checkout", "--quiet", "-b", &branch])?;
            write(&worktree)?;
            git(&worktree, &["add", "--all", "--force"])?;
            if git(&worktree, &["status", "--porcelain"])?.is_empty() {
                return Err(AppError::Invalid("the change would change nothing".into()));
            }
            git(&worktree, &["commit", "--quiet", "-m", message])?;
            Ok(())
        })();
        let _ = app.repos.remove_worktree(&self.path, &worktree);
        if let Err(e) = made {
            let _ = git(&self.path, &["branch", "-D", "--quiet", &branch]);
            return Err(e);
        }
        Ok(branch)
    }

    /// Deletes a branch that was made but not offered.
    pub fn delete_branch(&self, branch: &str) {
        let _read_at = self.lock();
        let _ = git(&self.path, &["branch", "-D", "--quiet", branch]);
    }

    /// The files of `package` that `branch` changes against the default branch, and their diff.
    pub fn package_diff(&self, branch: &str, package: &str) -> Result<PackageDiff> {
        let _read_at = self.lock();
        let base = self.repos.default_branch(&self.path)?;
        let range = format!("{base}...{branch}");
        let folder = format!("{PACKAGES_DIR}/{package}");
        let status = git(
            &self.path,
            &["diff", "--name-status", &range, "--", &folder],
        )?;
        let (mut files, mut dependency_files) = (vec![], 0);
        for line in status.lines() {
            let Some((code, path)) = line.split_once('\t') else {
                continue;
            };
            if path.contains("/node_modules/") {
                dependency_files += 1;
            } else {
                files.push(format!("{} {path}", code.chars().next().unwrap_or('M')));
            }
        }
        let skip = format!(":(exclude){folder}/node_modules");
        let diff = git(&self.path, &["diff", &range, "--", &folder, &skip])?;
        let diff = match diff.char_indices().nth(DIFF_LIMIT) {
            Some((end, _)) => format!("{}\n... (cut off)", &diff[..end]),
            None => diff,
        };
        Ok(PackageDiff {
            files,
            dependency_files,
            diff,
        })
    }

    /// Whether `branch` changes a package or the lock: what only a person merges.
    pub fn changes_packages(&self, branch: &str) -> Result<bool> {
        let _read_at = self.lock();
        let base = self.repos.default_branch(&self.path)?;
        self.changes_packages_at(&base, branch)
    }

    /// Like [`Self::changes_packages`] for a lock already held, with the base and the commit or branch given.
    pub(super) fn changes_packages_at(&self, base: &str, branch: &str) -> Result<bool> {
        let names = git(
            &self.path,
            &["diff", "--name-only", &format!("{base}...{branch}")],
        )?;
        Ok(names
            .lines()
            .any(|file| file.starts_with("packages/") || file == "packages.lock"))
    }
}

/// The agents of the repository at `root` that name a resource of the package `package`.
pub(super) fn agents_naming_at(root: &Path, package: &str) -> Vec<String> {
    agent_names(root)
        .into_iter()
        .filter(|name| {
            std::fs::read_to_string(root.join("agents").join(format!("{name}.toml")))
                .ok()
                .and_then(|text| toml::from_str::<AgentSettings>(&text).ok())
                .is_some_and(|settings| {
                    settings
                        .extensions
                        .iter()
                        .chain(&settings.skills)
                        .any(|item| referenced_package(item) == Some(package))
                })
        })
        .collect()
}

/// Has the agent `agent` of the worktree at `root` name the package `package`: its extensions if `extensions`, and its
/// skills if `skills`, each as `pkg:<package>` unless it is named already.
pub fn add_to_agent(
    root: &Path,
    agent: &str,
    package: &str,
    (extensions, skills): (bool, bool),
) -> Result<()> {
    if !crate::config::valid_agent_name(agent) {
        return Err(AppError::Invalid(format!("{agent} is not an agent name")));
    }
    let file = root.join("agents").join(format!("{agent}.toml"));
    let text =
        std::fs::read_to_string(&file).map_err(|_| AppError::NotFound(format!("agent {agent}")))?;
    let mut settings: AgentSettings = toml::from_str(&text)
        .map_err(|e| AppError::Invalid(format!("agents/{agent}.toml: {e}")))?;
    let name = format!("pkg:{package}");
    for (wanted, list) in [
        (extensions, &mut settings.extensions),
        (skills, &mut settings.skills),
    ] {
        if wanted && !list.contains(&name) {
            list.push(name.clone());
        }
    }
    let text = toml::to_string(&settings)
        .map_err(|e| AppError::Invalid(format!("cannot write the settings: {e}")))?;
    Ok(std::fs::write(file, text)?)
}

/// Takes every name of the package `package` out of the agents of the worktree at `root`; the agents it changed.
pub fn remove_from_agents(root: &Path, package: &str) -> Result<Vec<String>> {
    let mut changed = vec![];
    for agent in agents_naming_at(root, package) {
        let file = root.join("agents").join(format!("{agent}.toml"));
        let text = std::fs::read_to_string(&file)?;
        let mut settings: AgentSettings = toml::from_str(&text)
            .map_err(|e| AppError::Invalid(format!("agents/{agent}.toml: {e}")))?;
        for list in [&mut settings.extensions, &mut settings.skills] {
            list.retain(|item| referenced_package(item) != Some(package));
        }
        let text = toml::to_string(&settings)
            .map_err(|e| AppError::Invalid(format!("cannot write the settings: {e}")))?;
        std::fs::write(file, text)?;
        changed.push(agent);
    }
    Ok(changed)
}
