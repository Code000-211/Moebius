//! A person's edits of the wiki: a page written, moved (a rename is a move within its folder) or deleted, each a
//! commit by the person on the clone's branch, pushed to `origin` after the clone has followed it, or undone when
//! anything fails. Every edit that commits sends `wiki_changed`.

use std::path::Path;

use super::pages::name_of;
use super::{Author, PageView, Wiki, file_of, literal, project_key};
use crate::app::{App, AppError, Result};
use crate::repos::kept;

impl Wiki {
    /// The person `by` writes the page `path` with `text`, making it if it is not there. `message` is the commit's,
    /// or one that says what changed. Text that changes nothing makes no commit. The page is given as it now is.
    pub fn write_page(
        &self,
        app: &App,
        by: &Author,
        path: &str,
        text: &str,
        message: Option<&str>,
    ) -> Result<PageView> {
        self.write_page_from(app, by, path, text, message, None)
    }

    /// [`Wiki::write_page`] for a save that is built on the page as a person read it: `base` is the commit of the
    /// page they loaded (at least its first 7 characters), or `""` for a page that was not there. When the page's
    /// latest commit is another, or the page is there or gone against that, nothing is written and the error is
    /// [`AppError::Stale`] with the page as it is. Without `base` the text replaces the page whatever it holds.
    pub fn write_page_from(
        &self,
        app: &App,
        by: &Author,
        path: &str,
        text: &str,
        message: Option<&str>,
        base: Option<&str>,
    ) -> Result<PageView> {
        let file = file_of(path)?;
        check_project(app, path)?;
        let mut text = text.to_string();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        {
            let _turn = self.turn();
            self.follow(app)?;
            let target = self.inside(&file)?;
            if let Some(base) = base {
                let latest = if target.is_file() {
                    self.history_of(&file)?.into_iter().next()
                } else {
                    None
                };
                let based_on_it = match &latest {
                    None => base.is_empty(),
                    Some(version) => {
                        version.commit == base
                            || (base.len() >= 7 && version.commit.starts_with(base))
                    }
                };
                if !based_on_it {
                    let current = match latest {
                        Some(_) => Some(Box::new(self.page_held(path, None)?)),
                        None => None,
                    };
                    return Err(AppError::Stale {
                        path: path.to_string(),
                        current,
                    });
                }
            }
            let verb = if target.is_file() { "Update" } else { "Add" };
            let message = message_or(message, || format!("{verb} {path}"));
            let write = || {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                Ok(std::fs::write(&target, &text)?)
            };
            self.commit(app, by, &[&literal(&file)], &message, write)?;
        }
        self.page(path, None)
    }

    /// The person `by` moves the page `from` to `to`, which may be in another folder; a rename is a move to the
    /// same folder. A page already at `to` is a conflict. The history follows the page.
    pub fn move_page(&self, app: &App, by: &Author, from: &str, to: &str) -> Result<PageView> {
        let (old, new) = (file_of(from)?, file_of(to)?);
        check_project(app, to)?;
        if old == new {
            return Err(AppError::Invalid(format!("{from} is already there")));
        }
        {
            let _turn = self.turn();
            self.follow(app)?;
            let (source, target) = (self.inside(&old)?, self.inside(&new)?);
            if !source.is_file() {
                return Err(AppError::NotFound(format!("page {from}")));
            }
            if target.exists() && !same_file_but_case(&source, &target, &old, &new) {
                return Err(AppError::Exists(format!("page {to}")));
            }
            let parent = |p: &str| p.rsplit_once('/').map(|(folder, _)| folder.to_string());
            let message = if parent(from) == parent(to) {
                format!("Rename {from} to {}", name_of(to))
            } else {
                format!("Move {from} to {to}")
            };
            // The links to the page in the other pages follow it, in the same commit.
            let relinked = self.retargeted(from, to)?;
            let write = || {
                if let Some(folder) = target.parent() {
                    std::fs::create_dir_all(folder)?;
                }
                std::fs::rename(&source, &target)?;
                remove_empty_folders(&self.path, &source);
                for (file, text) in &relinked {
                    std::fs::write(self.inside(file)?, text)?;
                }
                Ok(())
            };
            let mut files = vec![literal(&old), literal(&new)];
            files.extend(relinked.iter().map(|(file, _)| literal(file)));
            let files: Vec<&str> = files.iter().map(String::as_str).collect();
            self.commit(app, by, &files, &message, write)?;
        }
        self.page(to, None)
    }

    /// The person `by` deletes the page `path`. Its history stays, and an older version can still be read.
    pub fn delete_page(&self, app: &App, by: &Author, path: &str) -> Result<()> {
        let file = file_of(path)?;
        let _turn = self.turn();
        self.follow(app)?;
        let target = self.inside(&file)?;
        if !target.is_file() {
            return Err(AppError::NotFound(format!("page {path}")));
        }
        let message = format!("Delete {path}");
        let write = || {
            std::fs::remove_file(&target)?;
            remove_empty_folders(&self.path, &target);
            Ok(())
        };
        self.commit(app, by, &[&literal(&file)], &message, write)
    }

    /// Commits what `write` does to `files` as `by` and pushes it, telling the bus. The caller holds the turn.
    fn commit(
        &self,
        app: &App,
        by: &Author,
        files: &[&str],
        message: &str,
        write: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let author = by.git();
        let change = kept::Change {
            files,
            message,
            author: Some(&author),
        };
        if let Some(commit) =
            kept::commit_pushed(app, &self.repos, &self.url, &self.path, &change, write)?
        {
            self.changed(app, commit);
        }
        Ok(())
    }
}

/// A page under `projects/<key>/` must name a project there is.
pub(super) fn check_project(app: &App, path: &str) -> Result<()> {
    match project_key(path) {
        Some(key) if app.store.project_by_key(key)?.is_none() => Err(AppError::Invalid(format!(
            "no project has the key {key}, so projects/{key}/ holds no pages"
        ))),
        _ => Ok(()),
    }
}

/// `message` when it says something, trimmed to its first line, or the one `otherwise` makes.
pub(super) fn message_or(message: Option<&str>, otherwise: impl FnOnce() -> String) -> String {
    message
        .and_then(|m| m.lines().map(str::trim).find(|line| !line.is_empty()))
        .map_or_else(otherwise, str::to_string)
}

/// Whether `target` exists only because it is `source` on a file system that ignores case, as a rename that changes
/// only the case of a name meets on Windows and macOS.
fn same_file_but_case(source: &Path, target: &Path, old: &str, new: &str) -> bool {
    old.eq_ignore_ascii_case(new)
        && std::fs::canonicalize(source).ok() == std::fs::canonicalize(target).ok()
}

/// Removes the folders above `file` that are left empty, up to the clone itself; git keeps no empty folders.
fn remove_empty_folders(clone: &Path, file: &Path) {
    let mut folder = file.parent();
    while let Some(dir) = folder {
        if dir == clone || std::fs::remove_dir(dir).is_err() {
            break;
        }
        folder = dir.parent();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::super::testing::{bare_origin, push_from_elsewhere};
    use super::*;
    use crate::repos::git;
    use crate::store::Store;

    fn app(dir: &Path) -> App {
        let store = Arc::new(Store::in_memory(dir).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(5)));
        App::new(store, host, &dir.join("data"))
    }

    fn ada() -> Author {
        Author {
            name: "Ada Lovelace".into(),
            handle: "ada".into(),
        }
    }

    fn opened(dir: &Path) -> (App, Wiki, std::path::PathBuf) {
        let app = app(dir);
        let origin = bare_origin(dir);
        let wiki = Wiki::open(&app, &origin.to_string_lossy(), &dir.join("data/wiki")).unwrap();
        (app, wiki, origin)
    }

    fn origin_log(origin: &Path) -> String {
        git(origin, &["log", "--format=%an <%ae>|%s", "main"]).unwrap_or_default()
    }

    #[test]
    fn an_edit_is_a_commit_by_its_person_on_origin() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, origin) = opened(dir.path());
        assert!(wiki.pages().unwrap().is_empty());
        let mut events = app.bus.subscribe();

        let page = wiki
            .write_page(
                &app,
                &ada(),
                "Team/On call",
                "# On call\n\nWho answers.",
                None,
            )
            .unwrap();
        assert_eq!(page.text, "# On call\n\nWho answers.\n");
        assert_eq!(page.name, "On call");
        assert_eq!(page.version.author, "Ada Lovelace");
        assert_eq!(page.version.handle.as_deref(), Some("ada"));
        assert_eq!(page.version.message, "Add Team/On call");
        assert_eq!(
            origin_log(&origin),
            "Ada Lovelace <ada@moebius>|Add Team/On call"
        );
        let event = events.try_recv().unwrap();
        assert_eq!(event.kind, "wiki_changed");

        let again = wiki
            .write_page(
                &app,
                &ada(),
                "Team/On call",
                "# On call\n\nWho answers.\n",
                None,
            )
            .unwrap();
        assert_eq!(again.version.commit, page.version.commit, "nothing changed");

        wiki.write_page(
            &app,
            &ada(),
            "Team/On call",
            "# On call\n\nThe person of the week.\n",
            Some("Say who\nmore lines"),
        )
        .unwrap();
        let history = wiki.history("Team/On call").unwrap();
        let messages: Vec<&str> = history.iter().map(|v| v.message.as_str()).collect();
        assert_eq!(messages, ["Say who", "Add Team/On call"]);
        let first = wiki.page("Team/On call", Some(&history[1].commit)).unwrap();
        assert_eq!(first.text, "# On call\n\nWho answers.\n");
        assert_eq!(
            wiki.pages().unwrap(),
            [super::super::PageInfo {
                path: "Team/On call".into(),
                name: "On call".into(),
                labels: vec![]
            }]
        );
    }

    #[cfg(unix)] // The refusing hook of `origin` is a shell script.
    #[test]
    fn a_failing_push_leaves_the_clone_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, origin) = opened(dir.path());
        wiki.write_page(&app, &ada(), "Handbook", "Hello\n", None)
            .unwrap();
        let before = wiki.head().unwrap();
        super::super::testing::refuse_pushes(&origin);

        let refused = wiki
            .write_page(&app, &ada(), "Handbook", "Changed\n", None)
            .unwrap_err();
        assert!(matches!(refused, AppError::Conflict(_)), "{refused:?}");
        let made = wiki.write_page(&app, &ada(), "New/Page", "New\n", None);
        assert!(made.is_err());
        let moved = wiki.move_page(&app, &ada(), "Handbook", "Guides/Handbook");
        assert!(moved.is_err());
        assert!(wiki.delete_page(&app, &ada(), "Handbook").is_err());

        assert_eq!(wiki.head().unwrap(), before);
        assert_eq!(git(&wiki.path, &["status", "--porcelain"]).unwrap(), "");
        assert!(!wiki.path.join("New").exists());
        assert!(!wiki.path.join("Guides").exists());
        assert_eq!(wiki.page("Handbook", None).unwrap().text, "Hello\n");
    }

    #[cfg(unix)] // The refusing hook of `origin` is a shell script.
    #[test]
    fn a_failing_first_push_leaves_the_empty_clone_empty() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, origin) = opened(dir.path());
        super::super::testing::refuse_pushes(&origin);
        assert!(
            wiki.write_page(&app, &ada(), "First", "Hi\n", None)
                .is_err()
        );
        assert_eq!(wiki.head(), None);
        assert_eq!(git(&wiki.path, &["status", "--porcelain"]).unwrap(), "");
        assert!(wiki.pages().unwrap().is_empty());
    }

    #[test]
    fn a_change_pushed_elsewhere_is_read_after_a_refresh() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, origin) = opened(dir.path());
        wiki.write_page(&app, &ada(), "Handbook", "Ours\n", None)
            .unwrap();
        push_from_elsewhere(dir.path(), &origin, "Handbook.md", "Theirs\n");
        assert_eq!(wiki.page("Handbook", None).unwrap().text, "Ours\n");
        let mut events = app.bus.subscribe();

        wiki.refresh(&app).unwrap();
        let page = wiki.page("Handbook", None).unwrap();
        assert_eq!(page.text, "Theirs\n");
        assert_eq!(page.version.author, "Grace");
        assert_eq!(page.version.handle, None);
        assert_eq!(events.try_recv().unwrap().kind, "wiki_changed");
        wiki.refresh(&app).unwrap();
        assert!(events.try_recv().is_err(), "nothing new, nothing sent");

        // An edit follows origin first, so it lands on what was pushed there.
        push_from_elsewhere(dir.path(), &origin, "Other.md", "Theirs\n");
        wiki.write_page(&app, &ada(), "Handbook", "Ours again\n", None)
            .unwrap();
        let paths: Vec<String> = wiki.pages().unwrap().into_iter().map(|p| p.path).collect();
        assert_eq!(paths, ["Handbook", "Other"]);
    }

    #[test]
    fn a_moved_page_keeps_its_history_and_a_deleted_one_too() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, _origin) = opened(dir.path());
        wiki.write_page(&app, &ada(), "Drafts/Payments", "How we pay\n", None)
            .unwrap();
        let renamed = wiki
            .move_page(&app, &ada(), "Drafts/Payments", "Drafts/Paying")
            .unwrap();
        assert_eq!(renamed.version.message, "Rename Drafts/Payments to Paying");
        let moved = wiki
            .move_page(&app, &ada(), "Drafts/Paying", "Architecture/Paying")
            .unwrap();
        assert_eq!(
            moved.version.message,
            "Move Drafts/Paying to Architecture/Paying"
        );
        assert!(!wiki.path.join("Drafts").exists(), "an empty folder goes");
        let history = wiki.history("Architecture/Paying").unwrap();
        let paths: Vec<&str> = history.iter().map(|v| v.path.as_str()).collect();
        assert_eq!(
            paths,
            ["Architecture/Paying", "Drafts/Paying", "Drafts/Payments"]
        );
        let oldest = wiki
            .page("Architecture/Paying", Some(&history[2].commit))
            .unwrap();
        assert_eq!(oldest.text, "How we pay\n");

        wiki.write_page(&app, &ada(), "Taken", "x\n", None).unwrap();
        let taken = wiki
            .move_page(&app, &ada(), "Architecture/Paying", "Taken")
            .unwrap_err();
        assert!(matches!(taken, AppError::Exists(_)), "{taken:?}");
        let missing = wiki.move_page(&app, &ada(), "Nothing", "Else").unwrap_err();
        assert!(matches!(missing, AppError::NotFound(_)), "{missing:?}");

        wiki.delete_page(&app, &ada(), "Architecture/Paying")
            .unwrap();
        assert!(matches!(
            wiki.page("Architecture/Paying", None).unwrap_err(),
            AppError::NotFound(_)
        ));
        let history = wiki.history("Architecture/Paying").unwrap();
        assert_eq!(history[0].message, "Delete Architecture/Paying");
        assert!(matches!(
            wiki.history("Never").unwrap_err(),
            AppError::NotFound(_)
        ));
    }

    #[test]
    fn a_name_with_brackets_is_that_page_alone() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, _origin) = opened(dir.path());
        wiki.write_page(&app, &ada(), "ab", "Plain\n", None)
            .unwrap();
        wiki.write_page(&app, &ada(), "a[b]", "Bracketed\n", None)
            .unwrap();
        wiki.write_page(&app, &ada(), "ab", "Plain again\n", None)
            .unwrap();
        let history = wiki.history("a[b]").unwrap();
        assert_eq!(history.len(), 1, "{history:?}");
        let page = wiki.page("a[b]", None).unwrap();
        assert_eq!(page.version.message, "Add a[b]");
        assert!(
            wiki.page("a[b]", Some(&wiki.history("ab").unwrap()[0].commit))
                .is_err()
        );
        assert!(matches!(
            wiki.history("x[y]").unwrap_err(),
            AppError::NotFound(_)
        ));
        wiki.write_page(&app, &ada(), "xy", "x\n", None).unwrap();
        assert!(wiki.history("x[y]").is_err());
        wiki.delete_page(&app, &ada(), "a[b]").unwrap();
        assert_eq!(wiki.page("ab", None).unwrap().text, "Plain again\n");
    }

    #[cfg(unix)] // Links are made with the Unix call; Windows checks links out as files anyway.
    #[test]
    fn a_link_never_leads_out_of_the_wiki() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, origin) = opened(dir.path());
        let secret = dir.path().join("secret.toml");
        std::fs::write(&secret, "token = \"hush\"\n").unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir(&outside).unwrap();

        // A link pushed to origin is checked out as a plain file holding where it pointed.
        push_from_elsewhere(dir.path(), &origin, "Start.md", "Start\n");
        let other = dir.path().join("elsewhere");
        std::os::unix::fs::symlink(&secret, other.join("leak.md")).unwrap();
        git(&other, &["add", "-A"]).unwrap();
        git(&other, &["commit", "--quiet", "-m", "Link"]).unwrap();
        git(&other, &["push", "--quiet", "origin", "HEAD:main"]).unwrap();
        wiki.refresh(&app).unwrap();
        let leak = wiki.page("leak", None).unwrap();
        assert!(!leak.text.contains("hush"), "{}", leak.text);

        // A link in a clone made before links were turned off is neither listed, read nor written through.
        std::os::unix::fs::symlink(&secret, wiki.path.join("old.md")).unwrap();
        std::os::unix::fs::symlink(&outside, wiki.path.join("dir")).unwrap();
        assert!(wiki.pages().unwrap().iter().all(|p| p.path != "old"));
        assert!(wiki.page("old", None).is_err());
        assert!(wiki.write_page(&app, &ada(), "old", "x\n", None).is_err());
        assert!(wiki.write_page(&app, &ada(), "dir/x", "x\n", None).is_err());
        assert!(wiki.move_page(&app, &ada(), "Start", "dir/Start").is_err());
        assert!(wiki.delete_page(&app, &ada(), "old").is_err());
        assert_eq!(
            std::fs::read_to_string(&secret).unwrap(),
            "token = \"hush\"\n"
        );
        assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
    }

    #[test]
    fn a_projects_pages_need_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, _origin) = opened(dir.path());
        let refused = wiki
            .write_page(&app, &ada(), "projects/NOPE/Page", "x\n", None)
            .unwrap_err();
        assert!(matches!(refused, AppError::Invalid(_)), "{refused:?}");
        app.store.add_project("Storefront", "SHOP").unwrap();
        let page = wiki
            .write_page(&app, &ada(), "projects/SHOP/Payments", "x\n", None)
            .unwrap();
        assert_eq!(page.path, "projects/SHOP/Payments");
    }
}
