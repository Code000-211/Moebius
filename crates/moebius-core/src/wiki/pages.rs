//! Reading the wiki: the list of its pages, a page as it is or as an older version had it, and a page's history,
//! which follows the page through its renames and moves.

use serde::Serialize;

use super::{EMAIL_DOMAIN, Wiki, file_of, labels, literal, page_of};
use crate::app::{AppError, Result};
use crate::repos::{git, kept};

/// A page in the list of pages.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PageInfo {
    /// The page's path without `.md`, such as `Team/On call`.
    pub path: String,
    /// The last part of the path, which the page is called by.
    pub name: String,
    /// The page's labels, from the front matter of its file; none for a page without any.
    pub labels: Vec<String>,
}

/// A page as it is, or as one version had it.
#[derive(Debug, Clone, Serialize)]
pub struct PageView {
    pub path: String,
    pub name: String,
    /// The page's file as it is, front matter included: what the editor shows and saves.
    pub text: String,
    /// The text without its front matter: what is drawn.
    pub body: String,
    /// The labels in the front matter.
    pub labels: Vec<String>,
    /// The last commit that changed the page, as of the version read.
    pub version: Version,
}

/// A commit that changed a page.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Version {
    /// The commit's full id.
    pub commit: String,
    /// The author's name.
    pub author: String,
    /// The author's handle when a person of Moebius made the commit; `None` for a commit pushed from elsewhere.
    pub handle: Option<String>,
    /// When the commit was made, in seconds since the epoch.
    pub at: i64,
    /// The commit message's first line.
    pub message: String,
    /// The page's path in that commit, which differs from today's after a rename or a move.
    pub path: String,
}

impl Wiki {
    /// Every page, sorted by path; empty while the wiki has no commit.
    pub fn pages(&self) -> Result<Vec<PageInfo>> {
        let _turn = self.turn();
        self.list()
    }

    /// Every page, the turn held.
    pub(super) fn list(&self) -> Result<Vec<PageInfo>> {
        if kept::head(&self.path).is_none() {
            return Ok(Vec::new());
        }
        let files = git(
            &self.path,
            &["-c", "core.quotePath=false", "ls-files", "--", "*.md"],
        )?;
        let mut pages: Vec<PageInfo> = files
            .lines()
            .filter(|file| self.inside(file).is_ok())
            .filter_map(page_of)
            .map(|path| self.info_of(path))
            .collect();
        pages.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(pages)
    }

    /// The page `path` as it is, or with `at` as the version of that commit, which must be one of its history.
    pub fn page(&self, path: &str, at: Option<&str>) -> Result<PageView> {
        let _turn = self.turn();
        self.page_held(path, at)
    }

    /// [`Wiki::page`] with the turn held.
    pub(super) fn page_held(&self, path: &str, at: Option<&str>) -> Result<PageView> {
        let file = file_of(path)?;
        let history = self.history_of(&file)?;
        let version = match at {
            None => {
                if !self.inside(&file)?.is_file() {
                    return Err(AppError::NotFound(format!("page {path}")));
                }
                history
                    .into_iter()
                    .next()
                    .ok_or_else(|| AppError::NotFound(format!("page {path}")))?
            }
            Some(at) => history
                .into_iter()
                .find(|version| {
                    version.commit == at || (at.len() >= 7 && version.commit.starts_with(at))
                })
                .ok_or_else(|| AppError::NotFound(format!("version {at} of page {path}")))?,
        };
        let text = match at {
            None => std::fs::read_to_string(self.inside(&file)?)?,
            Some(_) => {
                let blob = format!("{}:{}.md", version.commit, version.path);
                git_text(&self.path, &blob).map_err(|_| {
                    AppError::NotFound(format!("version {} of page {path}", version.commit))
                })?
            }
        };
        Ok(PageView {
            path: path.to_string(),
            name: name_of(path).to_string(),
            body: labels::body(&text).to_string(),
            labels: labels::labels_of(&text),
            text,
            version,
        })
    }

    /// The commits that changed the page `path`, the newest first, through its renames and moves. A page that never
    /// was is not found; a deleted page still has its history.
    pub fn history(&self, path: &str) -> Result<Vec<Version>> {
        let _turn = self.turn();
        let file = file_of(path)?;
        let history = self.history_of(&file)?;
        if history.is_empty() {
            return Err(AppError::NotFound(format!("page {path}")));
        }
        Ok(history)
    }

    /// The history of `file`, the turn held.
    pub(super) fn history_of(&self, file: &str) -> Result<Vec<Version>> {
        if kept::head(&self.path).is_none() {
            return Ok(Vec::new());
        }
        let log = git(
            &self.path,
            &[
                "-c",
                "core.quotePath=false",
                "log",
                "--follow",
                "--name-only",
                "--format=%x1e%H%x1f%an%x1f%ae%x1f%at%x1f%s",
                "--",
                &literal(file),
            ],
        )?;
        Ok(log.split('\x1e').filter_map(version).collect())
    }
}

/// A commit of `git log` as `history_of` asks for it: the fields, then a blank line and the file's name.
fn version(entry: &str) -> Option<Version> {
    let (head, files) = entry.split_once('\n')?;
    let mut fields = head.split('\x1f');
    let commit = fields.next()?.to_string();
    let author = fields.next()?.to_string();
    let email = fields.next()?;
    let at = fields.next()?.parse().ok()?;
    let message = fields.next().unwrap_or_default().to_string();
    let file = files.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(Version {
        commit,
        author,
        handle: email.strip_suffix(EMAIL_DOMAIN).map(str::to_string),
        at,
        message,
        path: file.strip_suffix(".md").unwrap_or(file).to_string(),
    })
}

/// A blob of the repository as text.
fn git_text(path: &std::path::Path, blob: &str) -> crate::repos::Result<String> {
    let bytes = crate::repos::git_bytes(path, &["show", blob])?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

impl Wiki {
    /// The page `path` as the list shows it, with the labels of its file as it is now; the turn held.
    pub(super) fn info_of(&self, path: String) -> PageInfo {
        let labels = self
            .read(&path)
            .map(|text| labels::labels_of(&text))
            .unwrap_or_default();
        PageInfo {
            name: name_of(&path).to_string(),
            path,
            labels,
        }
    }
}

/// The last part of a page's path.
pub(super) fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
