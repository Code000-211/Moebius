//! A project's settings as the core reads them: the files of `repos/<repository name>/` in the harness configuration
//! repository (see `harness::repo`), which replaced the `.moebius/` folder a repository kept on its default branch.
//! A server without that repository has no settings for any project. A repository whose default branch still has a
//! `.moebius/` folder is told about once: nothing in it is read.

use std::path::Path;

use super::App;
use crate::bus::Event;
use crate::repos::git;
use crate::store::Repository;

impl App {
    /// The harness configuration repository, where a project's settings are written; refused with a conflict (`409`)
    /// when the server has none, since then the project has no settings to change.
    pub(crate) fn project_harness_repo(
        &self,
    ) -> super::Result<&std::sync::Arc<crate::harness::repo::HarnessRepo>> {
        self.harness_repo.get().ok_or_else(|| {
            super::AppError::Conflict(
                "a project's settings are files of the harness configuration repository, and the server has none ([harness])"
                    .into(),
            )
        })
    }

    /// The text of `file` in the project `repository`'s settings (`rules.md`, `review.md`, `lint.toml`, `checks`,
    /// `test-first`, `localize` or a column's script such as `ready.luau`); none when the project has no such file or
    /// the server has no harness configuration repository.
    pub fn project_file(&self, repository: &Repository, file: &str) -> Option<String> {
        self.harness_repo
            .get()?
            .project_file(&repository.name, file)
    }

    /// The text of `file` in the settings of the project the run works in: its card's repository, or else the one the
    /// run names (a chat in a repository).
    pub fn run_project_file(&self, run_id: i64, file: &str) -> Option<String> {
        let run = self.store.run(run_id).ok()??;
        let repository = match run.card_id {
            Some(card) => self.store.card(card).ok()??.repository_id,
            None => run.repository_id?,
        };
        let repository = self.store.repository(repository).ok()??;
        self.project_file(&repository, file)
    }

    /// The files of the `.moebius/` folder the default branch of `repository` still has, which are no longer read.
    pub fn leftover_settings(&self, repository: &Repository) -> Vec<String> {
        let listed = git(
            Path::new(&repository.path),
            &[
                "ls-tree",
                "-r",
                "--name-only",
                &repository.base_ref(),
                "--",
                ".moebius",
            ],
        );
        listed
            .map(|files| files.lines().map(String::from).collect())
            .unwrap_or_default()
    }

    /// Says, once for each repository while the server runs, that its default branch has a `.moebius/` folder: in the
    /// log and as the event `settings_moved`, naming where a project's settings are now.
    pub fn warn_about_old_settings(&self) {
        let Ok(repositories) = self.store.repositories() else {
            return;
        };
        for repository in repositories {
            let files = self.leftover_settings(&repository);
            if files.is_empty() {
                continue;
            }
            let newly = self
                .moved_told
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(repository.id);
            if !newly {
                continue;
            }
            let next = if self.harness_repo.get().is_some() {
                format!(
                    "the settings of a project are the files of repos/{name}/ in the harness configuration \
                     repository, so move them there",
                    name = repository.name,
                )
            } else {
                "the server has no [harness] section, so no project has settings; add one (a server with \
                 rules_file moves its rules there too) and then move the files to repos/<repository name>/ of the \
                 harness configuration repository"
                    .to_string()
            };
            let message = format!(
                "the default branch of {name} has a .moebius/ folder ({files}), which is not read any more; {next}",
                name = repository.name,
                files = files.join(", "),
            );
            eprintln!("moebius: {message}");
            self.bus.publish(Event::SettingsMoved {
                repository: repository.name.clone(),
                message,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::harness::repo::testing;
    use crate::store::Store;

    /// A server with a harness configuration repository, and a repository whose default branch has `files`.
    fn setup(files: &[(&str, &str)]) -> (tempfile::TempDir, Arc<App>, Repository) {
        setup_with(files, true)
    }

    /// As `setup`, with the harness configuration repository only when `harness`.
    fn setup_with(
        files: &[(&str, &str)],
        harness: bool,
    ) -> (tempfile::TempDir, Arc<App>, Repository) {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        for (path, text) in files {
            std::fs::create_dir_all(origin.join(path).parent().unwrap()).unwrap();
            std::fs::write(origin.join(path), text).unwrap();
        }
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = Arc::new(App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(1))),
            &dir.path().join("data"),
        ));
        if harness {
            testing::use_harness(&app, dir.path());
        }
        let repository = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        (dir, app, repository)
    }

    #[test]
    fn nothing_is_read_from_a_repositorys_own_dot_moebius_folder() {
        let (_dir, app, repository) = setup(&[
            (".moebius/rules.md", "## sneaky\nx\n"),
            (".moebius/checks", "exit 1\n"),
            (".moebius/test-first", ""),
            (".moebius/ready.luau", "error('read')"),
        ]);
        for file in [
            "rules.md",
            "checks",
            "test-first",
            "ready.luau",
            "lint.toml",
        ] {
            assert_eq!(app.project_file(&repository, file), None, "{file}");
        }
        assert!(!app.test_first(&repository));
        assert_eq!(app.checks_script(&repository), None);
        testing::put(&app, "demo", "test-first", "");
        assert!(app.test_first(&repository));
    }

    #[test]
    fn a_repository_that_still_has_a_dot_moebius_folder_is_told_so_once() {
        let (_dir, app, repository) = setup(&[
            (".moebius/rules.md", "## a\nb\n"),
            (".moebius/ready.luau", "-- x\n"),
        ]);
        assert_eq!(
            app.leftover_settings(&repository),
            [".moebius/ready.luau", ".moebius/rules.md"]
        );
        let mut events = app.bus.subscribe();
        app.warn_about_old_settings();
        let event = events.try_recv().unwrap();
        assert_eq!(event.kind, "settings_moved");
        assert_eq!(event.data["repository"], "demo");
        let message = event.data["message"].as_str().unwrap();
        assert!(message.contains("repos/demo/"), "{message}");
        assert!(message.contains(".moebius/rules.md"), "{message}");
        // Said once while the server runs.
        app.warn_about_old_settings();
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn without_a_harness_repository_the_warning_says_the_server_needs_one_first() {
        let (_dir, app, _repository) = setup_with(&[(".moebius/rules.md", "## a\nb\n")], false);
        let mut events = app.bus.subscribe();
        app.warn_about_old_settings();
        let event = events.try_recv().unwrap();
        let message = event.data["message"].as_str().unwrap();
        assert!(message.contains("no [harness] section"), "{message}");
        assert!(message.contains("rules_file"), "{message}");
    }

    #[test]
    fn a_repository_without_a_dot_moebius_folder_is_told_nothing() {
        let (_dir, app, repository) = setup(&[]);
        assert!(app.leftover_settings(&repository).is_empty());
        let mut events = app.bus.subscribe();
        app.warn_about_old_settings();
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn a_server_without_a_harness_repository_has_no_project_settings() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(1))),
            &dir.path().join("data"),
        );
        let repository = app
            .store
            .add_repository("demo", "/nowhere", "none", "main")
            .unwrap();
        assert_eq!(app.project_file(&repository, "rules.md"), None);
        assert!(!app.test_first(&repository));
    }
}
