//! A repository's files from the desktop app's project page: listing a directory and reading a file as they are on the
//! default branch (the same `base_ref` column scripts are read from), and changing a file with a commit on the default
//! branch of Moebius's clone, pushed to `origin` for a repository on a forge. A push that fails undoes the commit, so
//! the clone never drifts from `origin`, and every change is recorded as a decision of the person or agent who made
//! it. [`crate::scripts`] commits a column's script the same way, through [`commit`].

use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{App, AppError, Result};
use crate::blobs::redact;
use crate::bus::AppEvent;
use crate::decide::{Actor, Question};
use crate::repos::{GitError, git, git_bytes};
use crate::store::Repository;

/// The kind of the decision recorded for a change to a file.
const KIND: &str = "edit_file";

/// A repository's own rules, which are checked as the server's are before they are saved.
const RULES: &str = ".moebius/rules.md";

/// The kind of the decision recorded for a change to a repository's own rules.
const RULES_KIND: &str = "edit_repository_rules";

/// The largest file, in bytes, that is read as text; a larger one is answered with its size only.
pub const TEXT_LIMIT: u64 = 1024 * 1024;

/// Edits take turns, so that two commits never meet in one clone.
static EDITING: Mutex<()> = Mutex::new(());

/// One entry of a directory as `GET /v1/repositories/{id}/tree` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    /// From the repository's root, with `/` between directories.
    pub path: String,
    /// `dir` or `file`.
    pub kind: String,
    /// The file's size in bytes; 0 for a directory.
    pub size: u64,
}

/// A directory of the default branch, directories first, each group by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tree {
    /// The directory, `""` for the repository's root.
    pub path: String,
    pub entries: Vec<Entry>,
}

/// A file of the default branch as `GET /v1/repositories/{id}/files/{path}` answers: its text, or, for a binary file
/// or one larger than [`TEXT_LIMIT`], only its size and why there is no text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileView {
    pub path: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub too_large: bool,
}

/// What `PUT /v1/repositories/{id}/files/{path}` takes.
#[derive(Debug, Clone, Deserialize)]
pub struct NewFile {
    pub text: String,
    /// The commit's message; `Update <path>` when it is left out.
    #[serde(default)]
    pub message: Option<String>,
    /// Why the file changes, kept with the decision.
    #[serde(default)]
    pub reason: String,
}

/// `path` as a path inside the repository, without leading or trailing `/`: refused with `Invalid` when it is
/// absolute, climbs out with `..`, or reaches into `.git`.
fn inside(path: &str) -> Result<String> {
    let trimmed = path.trim_matches('/');
    if path.starts_with('/') || path.contains('\\') {
        return Err(AppError::Invalid(format!(
            "{path} is not a path inside the repository"
        )));
    }
    let mut parts = Vec::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) if part != ".git" => parts.push(part.to_string_lossy()),
            Component::CurDir => {}
            _ => {
                return Err(AppError::Invalid(format!(
                    "{path} is not a path inside the repository"
                )));
            }
        }
    }
    Ok(parts.join("/"))
}

/// The directory `dir` of repository `id`'s default branch.
pub fn tree(app: &App, id: i64, dir: &str) -> Result<Tree> {
    let repository = app.repository(id)?;
    let dir = inside(dir)?;
    let spec = format!("{}:{dir}", repository.base_ref());
    let listing = git_bytes(Path::new(&repository.path), &["ls-tree", "-l", "-z", &spec])
        .map_err(|_| AppError::NotFound(format!("directory {dir} in {}", repository.name)))?;
    let mut entries: Vec<Entry> = listing
        .split(|b| *b == 0)
        .filter_map(|line| {
            let line = String::from_utf8_lossy(line);
            let (meta, name) = line.split_once('\t')?;
            let mut meta = meta.split_whitespace();
            let kind = match meta.nth(1)? {
                "tree" => "dir",
                "blob" => "file",
                // A submodule is another repository, not a file of this one.
                _ => return None,
            };
            let size = meta.nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let path = match dir.as_str() {
                "" => name.to_string(),
                dir => format!("{dir}/{name}"),
            };
            Some(Entry {
                name: name.to_string(),
                path,
                kind: kind.into(),
                size,
            })
        })
        .collect();
    entries.sort_by(|a, b| (a.kind != "dir", &a.name).cmp(&(b.kind != "dir", &b.name)));
    Ok(Tree { path: dir, entries })
}

/// The file at `path` of repository `id`'s default branch.
pub fn file(app: &App, id: i64, path: &str) -> Result<FileView> {
    let repository = app.repository(id)?;
    read(&repository, &inside(path)?)
}

fn read(repository: &Repository, path: &str) -> Result<FileView> {
    let clone = Path::new(&repository.path);
    let spec = format!("{}:{path}", repository.base_ref());
    let not_found = || AppError::NotFound(format!("file {path} in {}", repository.name));
    if git(clone, &["cat-file", "-t", &spec]).ok().as_deref() != Some("blob") {
        return Err(not_found());
    }
    let size: u64 = git(clone, &["cat-file", "-s", &spec])?
        .parse()
        .map_err(|_| not_found())?;
    let mut view = FileView {
        path: path.to_string(),
        size,
        text: None,
        binary: false,
        too_large: false,
    };
    if size > TEXT_LIMIT {
        view.too_large = true;
        return Ok(view);
    }
    let bytes = git_bytes(clone, &["cat-file", "blob", &spec])?;
    match String::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => view.text = Some(text),
        _ => view.binary = true,
    }
    Ok(view)
}

/// Replaces the file at `path` of repository `id` with `new.text`, as a commit on the default branch, pushed on a
/// forge. Text that changes nothing makes no commit. The repository's own rules (`.moebius/rules.md`) are refused with
/// `Invalid` when they are not fit to be saved, as `PUT /v1/rules` refuses the server's, and recorded as
/// `edit_repository_rules` with their text.
pub fn save_file(app: &App, by: &Actor, id: i64, path: &str, new: &NewFile) -> Result<FileView> {
    let repository = app.repository(id)?;
    let path = inside(path)?;
    if path.is_empty() {
        return Err(AppError::Invalid("a file needs a path".into()));
    }
    let clone = Path::new(&repository.path);
    // A link in the clone could lead the write out of the repository.
    let mut walked = clone.to_path_buf();
    for part in path.split('/') {
        walked.push(part);
        if walked
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(AppError::Invalid(format!(
                "{path} goes through a link, which is not edited here"
            )));
        }
    }
    if walked.is_dir() {
        return Err(AppError::Invalid(format!("{path} is a directory")));
    }
    let (kind, context) = if path == RULES {
        crate::rules::check(&new.text)
            .map_err(|why| AppError::Invalid(format!("{RULES}: {why}")))?;
        (
            RULES_KIND,
            BTreeMap::from([("text".to_string(), new.text.clone())]),
        )
    } else {
        (KIND, BTreeMap::new())
    };
    let message = match new.message.as_deref().map(str::trim) {
        Some(message) if !message.is_empty() => message.to_string(),
        _ => format!("Update {path}"),
    };
    let write = || {
        if let Some(parent) = walked.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(std::fs::write(&walked, &new.text)?)
    };
    commit(
        app,
        by,
        &repository,
        &Change {
            kind,
            path: &path,
            message: &message,
            reason: &new.reason,
            context,
            event: AppEvent::new(
                "file_changed",
                json!({ "repository": repository.id, "path": path }),
            ),
        },
        write,
    )?;
    read(&repository, &path)
}

/// A change to one file of a repository: what it is recorded and announced as.
pub(crate) struct Change<'a> {
    /// The kind of the decision recorded for it.
    pub kind: &'a str,
    /// The file, from the repository's root.
    pub path: &'a str,
    /// The commit's message.
    pub message: &'a str,
    /// Why, kept with the decision.
    pub reason: &'a str,
    /// More for the decision's context; the repository, path and message are added.
    pub context: BTreeMap<String, String>,
    /// Published once the change is through.
    pub event: AppEvent,
}

/// Makes the change `write` does to `change.path` a commit on the default branch of the clone, pushes it for a
/// repository on a forge, and records the decision and publishes the event. Whatever fails before the push is
/// through, the default branch is as it was. A change that changes nothing makes no commit and no decision.
pub(crate) fn commit(
    app: &App,
    by: &Actor,
    repository: &Repository,
    change: &Change,
    write: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let _editing = EDITING.lock().unwrap_or_else(|e| e.into_inner());
    let clone = Path::new(&repository.path);
    let path = change.path;
    let on_forge = repository.forge != "none";
    let header = if on_forge {
        let url = app.repos.origin_url(clone).map_err(refused)?;
        app.git_auth(&url)?
    } else {
        None
    };
    if on_forge {
        // Start from the forge's default branch, so the push is a fast-forward.
        app.repos.fetch(clone, header.as_deref()).map_err(refused)?;
        let upstream = repository.base_ref();
        git(clone, &["merge", "--ff-only", "--quiet", &upstream]).map_err(refused)?;
    }
    let before = git(clone, &["rev-parse", "HEAD"])?;
    let undo = |error: AppError| {
        let _ = git(clone, &["reset", "--hard", "--quiet", &before]);
        let _ = git(clone, &["clean", "-fdq", "--", path]);
        error
    };
    write().map_err(undo)?;
    git(clone, &["add", "-A", "--", path]).map_err(|e| undo(e.into()))?;
    if git(clone, &["diff", "--cached", "--quiet"]).is_ok() {
        return Ok(());
    }
    git(clone, &["commit", "--quiet", "-m", change.message]).map_err(|e| undo(e.into()))?;
    if on_forge {
        app.repos
            .push(clone, &repository.default_branch, header.as_deref())
            .map_err(|e| undo(refused(e)))?;
    }
    let mut context = change.context.clone();
    context.insert("repository".into(), repository.name.clone());
    context.insert("path".into(), path.into());
    context.insert("change".into(), change.message.into());
    app.decisions.record_by(
        by,
        &Question {
            kind: change.kind.into(),
            context,
            options: vec![change.kind.into(), "leave".into()],
            fail_closed: "leave".into(),
            run_id: None,
            card_id: None,
        },
        change.kind,
        change.reason,
    )?;
    app.bus.publish(change.event.clone());
    Ok(())
}

/// A git failure that the forge or the clone's state caused, as the 409 that says why, with keys redacted.
fn refused(error: GitError) -> AppError {
    AppError::Conflict(redact(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_stays_inside_the_repository() {
        assert_eq!(inside("src/main.rs").unwrap(), "src/main.rs");
        assert_eq!(inside("src/").unwrap(), "src");
        assert_eq!(inside("./a/./b").unwrap(), "a/b");
        assert_eq!(inside("").unwrap(), "");
        for path in [
            "../x",
            "a/../../x",
            "/etc/passwd",
            ".git/config",
            "a/.git/HEAD",
            "a\\b",
        ] {
            assert!(matches!(inside(path), Err(AppError::Invalid(_))), "{path}");
        }
    }
}
