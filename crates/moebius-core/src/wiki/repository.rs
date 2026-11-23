//! Where the wiki's repository is: opened when the server starts, kept on this server only until a person connects a
//! git repository to it (ADR 0026), and connected without a restart.
//!
//! A wiki without a `url` is a repository of its own at `<data_dir>/wiki` that has no `origin`: an edit is a commit and
//! nothing is pushed. Connecting a repository adds it as `origin` and pushes the history to it when it is empty; a
//! repository that has pages of its own is taken as the wiki when this one has none, and refused when both have,
//! since Moebius never merges two histories on its own.

use std::path::Path;

use serde::Serialize;

use super::Wiki;
use crate::app::{App, AppError, Result};
use crate::blobs::redact;
use crate::repos::{git, kept};
use crate::server::setup::without_credentials;

/// How the server keeps its wiki, as `GET /v1/wiki/status` tells it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    pub state: State,
    /// The repository the wiki is kept in, without a user name and password; when `state` is `remote`.
    pub url: Option<String>,
    /// Why the server has no wiki; when `state` is `unavailable`.
    pub problem: Option<String>,
}

/// The three ways a server has its wiki.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Kept on this server only.
    Local,
    /// Kept in a git repository too.
    Remote,
    /// The server started without it, see `problem`.
    Unavailable,
}

impl Status {
    pub(crate) fn of(wiki: Option<&Wiki>, problem: Option<String>) -> Self {
        match wiki {
            Some(wiki) => match wiki.url() {
                Some(url) => Self {
                    state: State::Remote,
                    url: Some(without_credentials(&url)),
                    problem: None,
                },
                None => Self {
                    state: State::Local,
                    url: None,
                    problem: None,
                },
            },
            None => Self {
                state: State::Unavailable,
                url: None,
                problem: Some(problem.unwrap_or_else(|| "the server has no wiki".into())),
            },
        }
    }
}

impl Wiki {
    /// Opens the wiki at `path`: a clone of `url`, made now unless it is there already (a clone that cannot be made is
    /// an error; one that is there is used as it is, so that the server starts without the network), or, without a
    /// `url`, a repository of its own, made now unless it is there already. A repository that was kept on this server
    /// only and has a `url` now is connected to it as [`Wiki::connect`] does.
    pub fn open(app: &App, url: Option<&str>, path: &Path) -> std::result::Result<Self, String> {
        let repos = app.repos.clone();
        let existed = path.join(".git").exists();
        match url {
            Some(url) => kept::clone_unless_there(app, &repos, url, path)
                .map_err(|e| format!("cannot clone the wiki: {e}"))?,
            None if !existed => {
                std::fs::create_dir_all(path)
                    .map_err(|e| format!("cannot make the wiki's repository: {e}"))?;
                git(path, &["init", "--quiet", "--initial-branch=main"])
                    .map_err(|e| format!("cannot make the wiki's repository: {e}"))?;
            }
            None => {}
        }
        // A link committed to the wiki is checked out as a plain file from now on, so it never leads out of the clone.
        git(path, &["config", "core.symlinks", "false"])
            .map_err(|e| format!("cannot set up the wiki's clone: {e}"))?;
        let wiki = Self {
            path: path.to_path_buf(),
            url: Default::default(),
            repos,
            turn: Default::default(),
        };
        match url {
            Some(url) if existed && wiki.repos.origin_url(path).is_err() => {
                wiki.connect(app, url).map_err(|e| e.to_string())?;
            }
            Some(url) => *wiki.url.write().unwrap_or_else(|e| e.into_inner()) = Some(url.into()),
            None => {}
        }
        Ok(wiki)
    }

    /// The git repository the pages are kept in too; none while they are kept on this server only.
    pub fn url(&self) -> Option<String> {
        self.url.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Connects the git repository `url` to a wiki that is kept on this server only. The repository must be reachable.
    /// When it is empty the wiki's history is pushed to it; when it has history and the wiki has no page yet, that
    /// history becomes the wiki; when both have history nothing is done and the answer is a conflict. From then on
    /// the wiki follows it and pushes to it. A wiki connected to a repository already is a conflict too.
    pub fn connect(&self, app: &App, url: &str) -> Result<()> {
        let url = url.trim();
        if url.is_empty() {
            return Err(AppError::Invalid(
                "the repository's address is empty".into(),
            ));
        }
        let _turn = self.turn();
        if let Some(now) = self.url() {
            return Err(AppError::Conflict(format!(
                "the wiki is kept in {} already",
                without_credentials(&now)
            )));
        }
        let header = app.git_auth(url)?;
        let branches = self
            .repos
            .remote_branches(&self.path, url, header.as_deref())
            .map_err(|e| {
                AppError::Invalid(format!(
                    "the repository cannot be reached: {}",
                    redact(&e.to_string())
                ))
            })?;
        let has_pages = kept::head(&self.path).is_some();
        if branches.is_empty() {
            self.add_origin(url)?;
            if has_pages {
                let pushed = self.repos.default_branch(&self.path).and_then(|branch| {
                    self.repos
                        .push(&self.path, &branch, header.as_deref(), false)
                });
                if let Err(e) = pushed {
                    let _ = git(&self.path, &["remote", "remove", "origin"]);
                    return Err(AppError::Conflict(format!(
                        "the repository refused the pages: {}",
                        redact(&e.to_string())
                    )));
                }
            }
        } else if has_pages {
            return Err(AppError::Conflict(
                "both this wiki and the repository have pages, and Moebius does not merge them: connect an empty repository, \
                 or one that has the pages of this wiki only"
                    .into(),
            ));
        } else {
            self.adopt(app, url)?;
        }
        *self.url.write().unwrap_or_else(|e| e.into_inner()) = Some(url.into());
        Ok(())
    }

    fn add_origin(&self, url: &str) -> Result<()> {
        let args = if self.repos.origin_url(&self.path).is_ok() {
            ["remote", "set-url", "origin", url]
        } else {
            ["remote", "add", "origin", url]
        };
        git(&self.path, &args)?;
        Ok(())
    }

    /// Makes the repository `url`, which has pages, the wiki that has none: it is cloned beside this one, which it
    /// then replaces, so a clone that fails leaves this one as it was.
    fn adopt(&self, app: &App, url: &str) -> Result<()> {
        let name = self.path.file_name().unwrap_or_default().to_string_lossy();
        let beside = self.path.with_file_name(format!("{name}.connecting"));
        let _ = std::fs::remove_dir_all(&beside);
        kept::clone_unless_there(app, &self.repos, url, &beside).map_err(|e| {
            AppError::Invalid(format!("the repository cannot be cloned: {}", redact(&e)))
        })?;
        git(&beside, &["config", "core.symlinks", "false"])?;
        std::fs::remove_dir_all(&self.path)?;
        std::fs::rename(&beside, &self.path)?;
        if let Some(commit) = kept::head(&self.path) {
            self.changed(app, commit);
        }
        Ok(())
    }
}

/// Connects the git repository `url` to the wiki of `app`, which is kept in `dir`. A server that started without a
/// wiki, as when its `url` could not be cloned, opens it from `url` instead.
pub fn connect(app: &App, dir: &Path, url: &str) -> Result<Status> {
    match app.wiki() {
        Ok(wiki) => wiki.connect(app, url)?,
        Err(_) => {
            let url = url.trim();
            let wiki =
                Wiki::open(app, Some(url), dir).map_err(|e| AppError::Invalid(redact(&e)))?;
            if let Err(error) = wiki.index(app) {
                app.bus.publish(crate::bus::Event::WikiError {
                    message: format!("cannot index the wiki's links: {error}"),
                });
            }
            app.set_wiki(std::sync::Arc::new(wiki));
        }
    }
    Ok(app.wiki_status())
}
