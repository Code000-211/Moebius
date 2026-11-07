//! A project's settings in the clone: the files of `repos/<repository name>/` that say how Moebius works on one
//! repository (its rules, review notes, checks, test-first switch and column scripts, and the settings
//! plugins ask for in `plugins/<plugin>/<key>`). They are read from
//! the clone's default branch, which is checked out, and a person's or an agent's edit is a commit on that branch
//! pushed to `origin`, like the other edits of the harness configuration repository (see `edits`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::HarnessRepo;
use super::edits::{Edit, remove_if_there};
use crate::app::{App, AppError, Result};
use crate::board::built_in_script;
use crate::decide::Actor;
use crate::repos::files::writable;

/// The directory of the clone that holds the settings of all projects.
pub const DIR: &str = "repos";

/// The directory of a project's settings that holds what plugins ask for, a folder for each plugin with a file for each
/// of its settings.
pub const PLUGINS_DIR: &str = "plugins";

/// The files of a project that are not column scripts: the repository's rules, notes for the reviewer, the checks of a
/// repository without a forge, the test-first switch and the hint of files for code runs.
pub const FILES: &[&str] = &["rules.md", "review.md", "checks", "test-first", "localize"];

/// Whether `file` is one of a project's files: one of [`FILES`] or the script of a column of the default workflow.
pub fn is_project_file(file: &str) -> bool {
    FILES.contains(&file) || built_in_script(file).is_some()
}

/// Whether `name` is a device name of Windows (`con`, `nul`, `com1`, ...), with or without an extension, in any case,
/// which Windows cannot make a directory of.
fn reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_lowercase();
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || ["com", "lpt"].iter().any(|prefix| {
            stem.strip_prefix(prefix)
                .is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
        })
}

/// Whether `name` can name a directory or file of a project's settings (a repository, a plugin or a setting's key):
/// letters, digits and `-_.`, not starting with a dot, and not a device name of Windows.
fn valid_project(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !reserved_name(name)
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

impl HarnessRepo {
    /// The directory of the settings of the project `name`.
    pub fn project_dir(&self, name: &str) -> PathBuf {
        self.path.join(DIR).join(name)
    }

    /// The text of the project's `file` as the default branch has it; none when the project has no such file, the
    /// file is not one a project has, or it cannot be read.
    pub fn project_file(&self, name: &str, file: &str) -> Option<String> {
        if !valid_project(name) || !is_project_file(file) {
            return None;
        }
        // A link in the clone (which `origin` may have added) must not lead a read out of it, as it must not a write.
        let mut path = self.path.clone();
        for part in [DIR, name, file] {
            path.push(part);
            let meta = path.symlink_metadata().ok()?;
            if meta.file_type().is_symlink() {
                return None;
            }
        }
        std::fs::read_to_string(path).ok()
    }

    /// The values the plugin `plugin` has for the project `name`, by key: the files of `repos/<name>/plugins/<plugin>/`,
    /// as the default branch has them. An empty file is no value; a link, a directory or a name that is not a key is not
    /// read.
    pub fn project_plugin_settings(&self, name: &str, plugin: &str) -> BTreeMap<String, String> {
        let mut values = BTreeMap::new();
        if !valid_project(name) || !valid_project(plugin) {
            return values;
        }
        let mut dir = self.path.clone();
        for part in [DIR, name, PLUGINS_DIR, plugin] {
            dir.push(part);
            match dir.symlink_metadata() {
                Ok(meta) if !meta.file_type().is_symlink() => {}
                _ => return values,
            }
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return values;
        };
        for entry in entries.flatten() {
            let key = entry.file_name().to_string_lossy().into_owned();
            let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
            if !is_file || !valid_project(&key) {
                continue;
            }
            if let Ok(value) = std::fs::read_to_string(entry.path())
                && !value.is_empty()
            {
                values.insert(key, value);
            }
        }
        values
    }

    /// A person makes `values` the settings plugin `plugin` has for the project `name`, in place of all it had: one
    /// file for each value under `repos/<name>/plugins/<plugin>/`, an empty value or a key left out removing its
    /// file. One commit on the default branch, pushed to `origin`, recorded as a decision of `kind` with `context`.
    /// The short id of the commit is given; values that change nothing make no commit.
    pub fn save_project_plugin_settings(
        &self,
        app: &App,
        by: &Actor,
        (name, plugin): (&str, &str),
        values: &BTreeMap<String, String>,
        (kind, message): (&str, &str),
        mut context: BTreeMap<String, String>,
    ) -> Result<Option<String>> {
        let mut read_at = self.lock();
        if reserved_name(name) {
            return Err(AppError::Invalid(format!(
                "{name:?} is a device name on Windows, which cannot make the directory repos/{name}/; rename the \
                 repository to keep its settings"
            )));
        }
        if !valid_project(name) {
            return Err(AppError::Invalid(format!(
                "{name:?} is not a repository name"
            )));
        }
        for part in std::iter::once(plugin).chain(values.keys().map(String::as_str)) {
            if !valid_project(part) {
                return Err(AppError::Invalid(format!(
                    "{part:?} cannot be the name of a file of a project's settings (letters, digits, - _ and .)"
                )));
            }
        }
        let folder = format!("{DIR}/{name}/{PLUGINS_DIR}/{plugin}");
        context.insert("repository".into(), name.into());
        context.insert("path".into(), folder.clone());
        let edit = Edit {
            kind,
            files: &folder,
            message,
        };
        let write = || {
            let target = writable(&self.path, &format!("{folder}/x"))?;
            let dir = target.parent().expect("the file is in the folder");
            for entry in std::fs::read_dir(dir)?.flatten() {
                let key = entry.file_name().to_string_lossy().into_owned();
                let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
                if !is_dir && values.get(&key).is_none_or(|value| value.is_empty()) {
                    remove_if_there(&entry.path())?;
                }
            }
            for (key, value) in values.iter().filter(|(_, value)| !value.is_empty()) {
                std::fs::write(writable(&self.path, &format!("{folder}/{key}"))?, value)?;
            }
            Ok(())
        };
        self.commit(app, by, &mut read_at, &edit, context, write)
    }

    /// A person or agent writes the project's `file` (`Some(text)`) or removes it (`None`): committed on the default
    /// branch with `message` and pushed to `origin`, recorded as a decision of `kind` with `context`. The short id of the
    /// commit is given; a change that changes nothing makes no commit. A file that is not one of a project's is refused
    /// with `Invalid`, and removing one the project does not have with `NotFound`.
    pub fn save_project_file(
        &self,
        app: &App,
        by: &Actor,
        (name, file): (&str, &str),
        text: Option<&str>,
        (kind, message): (&str, &str),
        mut context: BTreeMap<String, String>,
    ) -> Result<Option<String>> {
        let mut read_at = self.lock();
        if reserved_name(name) {
            return Err(AppError::Invalid(format!(
                "{name:?} is a device name on Windows, which cannot make the directory repos/{name}/; rename the \
                 repository to keep its settings"
            )));
        }
        if !valid_project(name) {
            return Err(AppError::Invalid(format!(
                "{name:?} is not a repository name"
            )));
        }
        if !is_project_file(file) {
            return Err(AppError::Invalid(format!(
                "{file} is not a file of a project's settings"
            )));
        }
        if text.is_none() && !self.project_dir(name).join(file).is_file() {
            return Err(AppError::NotFound(format!("{file} of the project {name}")));
        }
        let path = format!("{DIR}/{name}/{file}");
        context.insert("repository".into(), name.into());
        context.insert("path".into(), path.clone());
        let edit = Edit {
            kind,
            files: &path,
            message,
        };
        // A link in the clone (which `origin` may have added) must not lead the write out of it.
        let write = || {
            let target = writable(&self.path, &path)?;
            match text {
                Some(text) => Ok(std::fs::write(&target, text)?),
                None => remove_if_there(&target),
            }
        };
        self.commit(app, by, &mut read_at, &edit, context, write)
    }
}

/// A harness configuration repository for tests of the parts that read a project's settings.
#[cfg(test)]
pub(crate) mod testing {
    use std::path::Path;
    use std::sync::{Arc, RwLock};

    use super::HarnessRepo;
    use crate::app::App;
    use crate::config::AgentSet;
    use crate::decide::Actor;
    use crate::repos::git;

    /// Makes a harness configuration repository in `dir` (a bare origin and a clone) and gives it to `app`.
    pub(crate) fn use_harness(app: &App, dir: &Path) -> Arc<HarnessRepo> {
        let work = dir.join("harness-work");
        std::fs::create_dir_all(&work).unwrap();
        git(&work, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(work.join("README.md"), "harness\n").unwrap();
        git(&work, &["add", "."]).unwrap();
        git(&work, &["commit", "--quiet", "-m", "Start"]).unwrap();
        let bare = dir.join("harness-origin.git");
        git(
            dir,
            &[
                "init",
                "--quiet",
                "--bare",
                "-b",
                "main",
                "harness-origin.git",
            ],
        )
        .unwrap();
        git(&work, &["push", "--quiet", bare.to_str().unwrap(), "main"]).unwrap();
        let repo = Arc::new(
            HarnessRepo::open(
                app,
                bare.to_str().unwrap(),
                &dir.join("harness"),
                vec![],
                Arc::new(RwLock::new(AgentSet::default())),
            )
            .unwrap(),
        );
        app.set_harness_repo(repo.clone());
        repo
    }

    /// Writes the project `project`'s `file` as a person would, a commit in the harness configuration repository.
    pub(crate) fn put(app: &App, project: &str, file: &str, text: &str) {
        app.harness_repo()
            .unwrap()
            .save_project_file(
                app,
                &Actor::Person,
                (project, file),
                Some(text),
                ("edit_project_file", "Put a file"),
                Default::default(),
            )
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use super::*;
    use crate::app::AppError;
    use crate::decide::Actor;
    use crate::store::Store;
    use moebius_plugin_api::Host;

    // Unix only: a link is made with `std::os::unix`.
    #[cfg(unix)]
    #[test]
    fn a_link_in_the_clone_does_not_lead_a_save_or_a_removal_out_of_it() {
        use crate::repos::git;

        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(std::time::Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        let repo = testing::use_harness(&app, dir.path());
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("rules.md"), "keep\n").unwrap();
        std::fs::create_dir_all(repo.path().join("repos")).unwrap();
        std::os::unix::fs::symlink(&outside, repo.path().join("repos/demo")).unwrap();
        let before = git(repo.path(), &["rev-parse", "HEAD"]).unwrap();

        for text in [Some("## x\ny\n"), None] {
            let error = repo
                .save_project_file(
                    &app,
                    &Actor::Person,
                    ("demo", "rules.md"),
                    text,
                    ("edit_project_file", "Change"),
                    BTreeMap::new(),
                )
                .unwrap_err();
            assert!(matches!(error, AppError::Invalid(_)), "{error}");
        }
        assert_eq!(
            std::fs::read_to_string(outside.join("rules.md")).unwrap(),
            "keep\n"
        );
        assert_eq!(git(repo.path(), &["rev-parse", "HEAD"]).unwrap(), before);
    }

    // Unix only: a link is made with `std::os::unix`.
    #[cfg(unix)]
    #[test]
    fn a_link_in_the_clone_is_not_followed_by_a_read() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(std::time::Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        let repo = testing::use_harness(&app, dir.path());
        let outside = dir.path().join("secret");
        std::fs::write(&outside, "secret\n").unwrap();
        std::fs::create_dir_all(repo.path().join("repos/demo")).unwrap();
        std::os::unix::fs::symlink(&outside, repo.path().join("repos/demo/rules.md")).unwrap();
        assert_eq!(repo.project_file("demo", "rules.md"), None);
        // A linked project directory is not followed either.
        let folder = dir.path().join("folder");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("checks"), "exit 1\n").unwrap();
        std::os::unix::fs::symlink(&folder, repo.path().join("repos/linked")).unwrap();
        assert_eq!(repo.project_file("linked", "checks"), None);
    }

    #[test]
    fn a_name_windows_reserves_is_refused_with_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(std::time::Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        let repo = testing::use_harness(&app, dir.path());
        for name in ["con", "NUL", "Aux", "prn", "com1", "LPT9", "con.txt"] {
            let error = repo
                .save_project_file(
                    &app,
                    &Actor::Person,
                    (name, "rules.md"),
                    Some("## x\ny\n"),
                    ("edit_project_file", "Change"),
                    BTreeMap::new(),
                )
                .unwrap_err();
            assert!(error.to_string().contains("device name"), "{name}: {error}");
        }
        assert!(!reserved_name("com0"));
        assert!(!reserved_name("com10"));
        assert!(!reserved_name("console"));
    }

    #[test]
    fn only_the_files_of_a_project_are_read_and_written() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(std::time::Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        let repo = testing::use_harness(&app, dir.path());
        let save = |project: &str, file: &str, text| {
            repo.save_project_file(
                &app,
                &Actor::Person,
                (project, file),
                text,
                ("edit_project_file", "Change"),
                BTreeMap::new(),
            )
        };
        for (project, file) in [
            ("demo", "../agents/Coder.toml"),
            ("demo", "notes.md"),
            ("..", "rules.md"),
            ("a/b", "rules.md"),
            (".hidden", "rules.md"),
            ("", "rules.md"),
        ] {
            assert!(
                matches!(save(project, file, Some("x")), Err(AppError::Invalid(_))),
                "{project}/{file}"
            );
            assert_eq!(repo.project_file(project, file), None, "{project}/{file}");
        }
        assert!(matches!(
            save("demo", "checks", None),
            Err(AppError::NotFound(_))
        ));
        assert!(
            save("demo", "checks", Some("make test\n"))
                .unwrap()
                .is_some()
        );
        assert_eq!(repo.project_file("demo", "checks").unwrap(), "make test\n");
        // Nothing changed: no commit.
        assert_eq!(save("demo", "checks", Some("make test\n")).unwrap(), None);
        assert!(save("demo", "checks", None).unwrap().is_some());
        assert_eq!(repo.project_file("demo", "checks"), None);
    }
}
