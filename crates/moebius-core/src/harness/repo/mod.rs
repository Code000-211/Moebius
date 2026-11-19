//! The harness configuration repository (spec 5): a git repository the server clones to `<data_dir>/harness`. It holds
//! `agents/<name>.toml` (settings) and `agents/<name>.md` (instructions), `skills/<name>/SKILL.md`, `rules.md`,
//! `mcp.toml`, `repos/<name>/` (the settings of a repository) and `projects/<name>/` (the settings of a project, its
//! column scripts); the format is in `docs/reference/harness-configuration.md`.
//!
//! The agents are read from the clone's default branch when the server starts and after each refresh that changes it.
//! An agent whose files are broken is left out with its error, so the others go on. An edit through the API is a commit
//! on the default branch, pushed to `origin`; a push that fails undoes the commit, so the clone never drifts from
//! `origin`. Refreshing and editing take turns. The clone is kept as `repos::kept` keeps one, as the wiki's is.
//!
//! This module holds the clone itself: opening and refreshing it. `read` reads the agents, skills, MCP servers and
//! rules from it, `edits` makes a person's edits commits pushed to `origin`, `branches` checks, proposes and merges
//! the branches of the Retro, chats and trials, `repositories` reads and edits the settings of a repository
//! (`repos/<name>/`), `projects` those of a project (`projects/<name>/`) and moves them,
//! `create` makes a new repository, `skill_files` reads and edits a
//! skill's files beside its `SKILL.md`, `offer` offers files written on a branch as a change, and `packages` makes the
//! branches that install, update and remove Pi packages.

mod branches;
mod create;
mod edits;
mod offer;
mod packages;
mod projects;
mod read;
mod repositories;
mod skill_files;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use crate::app::{App, Result};
use crate::config::AgentSet;
use crate::repos::{Repos, git, kept};

pub use branches::{OwnBranch, Proposed};
pub use create::{SET_UP_MESSAGE, create};
pub use packages::{PERSON_ONLY, PackageDiff, add_to_agent, remove_from_agents};
pub use projects::{is_project_file, valid_folder as valid_project_folder};
use read::Layout;
pub use read::{
    AgentDefinition, AgentSettings, AgentView, McpServerInfo, McpServerView, SkillInfo,
    SkillSource, SkillText, SkillView,
};
#[cfg(test)]
pub(crate) use repositories::testing;
pub use repositories::{FILES as REPOSITORY_FILES, is_repository_file};
pub use skill_files::SkillFileText;

/// The harness configuration repository's clone.
pub struct HarnessRepo {
    path: PathBuf,
    url: String,
    repos: Repos,
    /// The environment variables `mcp.toml` may take values from.
    mcp_env: Vec<String>,
    /// The agents the harness runs; replaced whenever the repository is read.
    agents: Arc<RwLock<AgentSet>>,
    /// The commit the agents were read at. Held while the repository is refreshed or edited.
    read_at: Mutex<String>,
}

impl HarnessRepo {
    /// Clones `url` to `path` unless it is there already, and reads the agents into `agents`. A clone that cannot be
    /// made is an error; one that is there already is used as it is, so that the server starts without the network.
    pub fn open(
        app: &App,
        url: &str,
        path: &Path,
        mcp_env: Vec<String>,
        agents: Arc<RwLock<AgentSet>>,
    ) -> std::result::Result<Self, String> {
        let repo = Self {
            path: path.to_path_buf(),
            url: url.to_string(),
            repos: app.repos.clone(),
            mcp_env,
            agents,
            read_at: Mutex::default(),
        };
        kept::clone_unless_there(app, &repo.repos, url, path)
            .map_err(|e| format!("cannot clone the harness configuration repository: {e}"))?;
        let head = git(path, &["rev-parse", "--verify", "HEAD"]).unwrap_or_default();
        repo.read(app, &mut repo.lock(), head);
        Ok(repo)
    }

    /// Fetches `origin`, fast-forwards the default branch to it and reads the agents again if that changed anything.
    pub fn refresh(&self, app: &App) -> Result<()> {
        let mut read_at = self.lock();
        kept::follow_origin(app, &self.repos, &self.url, &self.path)?;
        let head = git(&self.path, &["rev-parse", "HEAD"])?;
        if head != *read_at {
            self.read(app, &mut read_at, head);
        }
        Ok(())
    }

    /// The clone's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The clone's default branch.
    pub fn default_branch(&self) -> Result<String> {
        Ok(self.repos.default_branch(&self.path)?)
    }

    fn layout(&self) -> Layout {
        Layout::read(&self.path, &self.mcp_env)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, String> {
        self.read_at.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn agent_file(&self, name: &str, extension: &str) -> PathBuf {
        self.path.join("agents").join(format!("{name}.{extension}"))
    }
}
