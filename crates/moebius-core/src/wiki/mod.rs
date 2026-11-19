//! The wiki (ADR 0022): the organisation's pages, Markdown files in a git repository the server clones to
//! `<data_dir>/wiki` and keeps in step with its `origin` as it keeps the harness configuration repository (see
//! [`crate::repos::kept`]). The organisation's pages are at the top and a project's under `projects/<key>/`; a page
//! is named by its path without `.md`, such as `Onboarding/Setup` or `projects/WEB/Architecture`.
//!
//! An edit through the API is a commit by its person, pushed to `origin`; a push that fails undoes the commit, so the
//! clone never drifts from `origin`. Before an edit, and on the server's refresh, the clone follows `origin`, so a
//! page someone pushed there is read. Refreshing, reading and editing take turns.
//!
//! This module holds the clone and the names of pages; `pages` reads the pages and their history, `edits` writes,
//! moves and deletes them, `proposals` keeps an agent's edit on a branch that waits in the inbox for a person to merge
//! it, `prompt` gives a run the pages its card links to, `search` finds pages by their words, and `sight` says which
//! pages a person or run sees (a private project's pages are its members').

mod edits;
mod pages;
mod prompt;
mod proposals;
mod search;
mod sight;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use crate::app::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::repos::{Repos, git, kept};

pub use pages::{PageInfo, PageView, Version};
pub use prompt::for_card;
pub use proposals::WIKI_PROPOSAL;
pub use search::{SEARCH_LIMIT, SearchHit};
pub use sight::Seen;

/// The folder that holds the projects' pages, one folder per project key.
pub const PROJECTS: &str = "projects";

/// The longest name of a page or folder, in bytes.
const NAME_LIMIT: usize = 120;

/// The wiki's clone.
pub struct Wiki {
    path: PathBuf,
    url: String,
    repos: Repos,
    /// Held while the clone is read, refreshed or edited.
    turn: Mutex<()>,
}

/// The person an edit is by, as the commit's author.
#[derive(Debug, Clone)]
pub struct Author {
    pub name: String,
    pub handle: String,
}

impl Author {
    /// The person `by` is, with the name the store has for them; refused for an agent or a script, which do not edit
    /// the wiki directly.
    pub fn of(app: &App, by: &Actor) -> Result<Self> {
        let Actor::Member { id, handle } = by else {
            return Err(AppError::Forbidden(
                "only a person edits the wiki directly".into(),
            ));
        };
        let name = app
            .store
            .person(*id)?
            .map_or_else(|| handle.clone(), |person| person.name);
        Ok(Self {
            name,
            handle: handle.clone(),
        })
    }

    /// The author as git writes it: the name, and the handle as the address, `Ada Lovelace <ada@moebius>`.
    fn git(&self) -> String {
        let name: String = self
            .name
            .chars()
            .filter(|c| !matches!(c, '<' | '>') && !c.is_control())
            .collect();
        let name = name.trim();
        let name = if name.is_empty() { &self.handle } else { name };
        format!("{name} <{}{EMAIL_DOMAIN}>", self.handle)
    }
}

/// What follows the handle in the address of a person's commits.
const EMAIL_DOMAIN: &str = "@moebius";

impl Wiki {
    /// Clones `url` to `path` unless it is there already. A clone that cannot be made is an error; one that is there
    /// already is used as it is, so that the server starts without the network.
    pub fn open(app: &App, url: &str, path: &Path) -> std::result::Result<Self, String> {
        let repos = app.repos.clone();
        kept::clone_unless_there(app, &repos, url, path)
            .map_err(|e| format!("cannot clone the wiki: {e}"))?;
        // A link committed to the wiki is checked out as a plain file from now on, so it never leads out of the clone.
        git(path, &["config", "core.symlinks", "false"])
            .map_err(|e| format!("cannot set up the wiki's clone: {e}"))?;
        Ok(Self {
            path: path.to_path_buf(),
            url: url.to_string(),
            repos,
            turn: Mutex::default(),
        })
    }

    /// Fetches `origin` and fast-forwards the clone to it; when that changed the pages, `wiki_changed` is sent.
    pub fn refresh(&self, app: &App) -> Result<()> {
        let _turn = self.turn();
        self.follow(app)
    }

    /// The commit the pages are read at; `None` while the wiki has no page yet.
    pub fn head(&self) -> Option<String> {
        let _turn = self.turn();
        kept::head(&self.path)
    }

    /// Follows `origin`, telling the bus when that changed the pages. The caller holds the turn.
    fn follow(&self, app: &App) -> Result<()> {
        let before = kept::head(&self.path);
        kept::follow_origin(app, &self.repos, &self.url, &self.path)?;
        if let Some(commit) = kept::head(&self.path).filter(|now| Some(now) != before.as_ref()) {
            app.bus.publish(Event::WikiChanged { commit });
        }
        Ok(())
    }

    /// The page file `file` in the clone, refused when it or a folder above it is a link, which could lead out of the
    /// clone (a clone checked out before links were turned off may still hold one).
    fn inside(&self, file: &str) -> Result<PathBuf> {
        let mut at = self.path.clone();
        for part in file.split('/') {
            at.push(part);
            if std::fs::symlink_metadata(&at).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(AppError::Invalid(format!(
                    "{file} goes through a link, which the wiki does not follow"
                )));
            }
        }
        Ok(at)
    }

    fn turn(&self) -> MutexGuard<'_, ()> {
        self.turn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The file of the page `path`, relative to the clone, once the name is checked: its folders and name, each a plain
/// name (no `.` or `..`, nothing hidden, none of the characters a file system refuses), and under `projects/` only
/// in a folder of a project's key.
fn file_of(path: &str) -> Result<String> {
    let invalid = |why: &str| AppError::Invalid(format!("{path:?} is not a page name: {why}"));
    let parts: Vec<&str> = path.split('/').collect();
    for part in &parts {
        if part.is_empty() {
            return Err(invalid("a folder or name is empty"));
        }
        if part.starts_with('.') {
            return Err(invalid("a name starts with a dot"));
        }
        if part.trim() != *part {
            return Err(invalid("a name starts or ends with a space"));
        }
        if part.len() > NAME_LIMIT {
            return Err(invalid("a name is longer than 120 bytes"));
        }
        if let Some(c) = part
            .chars()
            .find(|c| c.is_control() || matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        {
            return Err(invalid(&format!("it has the character {c:?}")));
        }
    }
    if parts[0] != PROJECTS && parts[0].eq_ignore_ascii_case(PROJECTS) {
        // On a file system that ignores case this would be the folder of the projects' pages.
        return Err(invalid("projects is spelled in lower case"));
    }
    if parts[0] == PROJECTS && parts.len() > 1 {
        if parts.len() < 3 {
            return Err(invalid(
                "a project's pages are in projects/<key>/, a folder of its key",
            ));
        }
        if !crate::store::is_key(parts[1]) {
            return Err(invalid(&format!("{} is not a project's key", parts[1])));
        }
    }
    Ok(format!("{path}.md"))
}

/// `file` as a pathspec git takes literally, so that a name with `[` in it names that file alone.
fn literal(file: &str) -> String {
    format!(":(literal){file}")
}

/// The page a file of the clone is, if it is one: a Markdown file whose name is a page name.
fn page_of(file: &str) -> Option<String> {
    let path = file.strip_suffix(".md")?;
    file_of(path).ok().map(|_| path.to_string())
}

/// The project key a page is under, if it is a project's page.
pub fn project_key(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(PROJECTS)?.strip_prefix('/')?;
    let (key, page) = rest.split_once('/')?;
    (!page.is_empty()).then_some(key)
}

#[cfg(test)]
pub(crate) mod testing {
    //! A wiki over a local bare `origin`, for the tests of the wiki and its routes.

    use std::path::{Path, PathBuf};

    use crate::repos::git;

    /// Makes an empty bare repository at `<dir>/origin.git` and gives its path.
    pub(crate) fn bare_origin(dir: &Path) -> PathBuf {
        let origin = dir.join("origin.git");
        git(
            dir,
            &["init", "--quiet", "--bare", "-b", "main", "origin.git"],
        )
        .unwrap();
        origin
    }

    /// Makes `origin` refuse every push from now on, as a forge that refuses one does.
    #[cfg(unix)]
    pub(crate) fn refuse_pushes(origin: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let hook = origin.join("hooks/pre-receive");
        std::fs::write(&hook, "#!/bin/sh\necho refused by the test >&2\nexit 1\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Pushes a commit to `origin` from a clone of its own, as someone else does, writing `text` to `file`.
    pub(crate) fn push_from_elsewhere(dir: &Path, origin: &Path, file: &str, text: &str) {
        let other = dir.join("elsewhere");
        if !other.exists() {
            git(
                dir,
                &["clone", "--quiet", &origin.to_string_lossy(), "elsewhere"],
            )
            .unwrap();
        }
        git(&other, &["pull", "--quiet", "--ff-only"]).ok();
        let path = other.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        git(&other, &["add", "-A"]).unwrap();
        git(
            &other,
            &[
                "-c",
                "user.name=Grace",
                "-c",
                "user.email=grace@example.com",
                "commit",
                "--quiet",
                "-m",
                "Edit elsewhere",
            ],
        )
        .unwrap();
        git(&other, &["push", "--quiet", "origin", "HEAD:main"]).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_name_is_plain_folders_and_a_name() {
        assert_eq!(file_of("Onboarding").unwrap(), "Onboarding.md");
        assert_eq!(file_of("Team/On call").unwrap(), "Team/On call.md");
        assert_eq!(
            file_of("projects/WEB/Architecture").unwrap(),
            "projects/WEB/Architecture.md"
        );
        assert_eq!(file_of("projects").unwrap(), "projects.md");
        for bad in [
            "",
            "/a",
            "a/",
            "a//b",
            "../a",
            "a/./b",
            ".hidden",
            " a",
            "a:b",
            "a\\b",
            "a\nb",
            "projects/WEB",
            "projects/web/Page",
            "Projects/WEB/Page",
            "PROJECTS/WEB/Page",
        ] {
            assert!(file_of(bad).is_err(), "{bad:?} was taken");
        }
        assert!(file_of(&"a".repeat(121)).is_err());
    }

    #[test]
    fn a_projects_page_names_its_key() {
        assert_eq!(project_key("projects/WEB/Architecture"), Some("WEB"));
        assert_eq!(project_key("projects/WEB/Runbooks/Refunds"), Some("WEB"));
        assert_eq!(project_key("Onboarding"), None);
        assert_eq!(project_key("projects"), None);
        assert_eq!(page_of("Team/On call.md").as_deref(), Some("Team/On call"));
        assert_eq!(page_of("README"), None);
        assert_eq!(page_of(".github/x.md"), None);
    }

    #[test]
    fn an_author_is_written_with_the_handle_as_address() {
        let author = Author {
            name: "Ada <Lovelace>".into(),
            handle: "ada".into(),
        };
        assert_eq!(author.git(), "Ada Lovelace <ada@moebius>");
        let nameless = Author {
            name: " ".into(),
            handle: "ada".into(),
        };
        assert_eq!(nameless.git(), "ada <ada@moebius>");
    }
}
