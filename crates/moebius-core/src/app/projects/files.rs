//! A project's own store of files (ADR 0019): a git repository Moebius keeps for the project without a forge, at
//! `<data_dir>/projects/<key>`, made with one empty commit on `main`. The store records it as a repository whose
//! `files_of` is the project and whose home project is the project, named by the project's key, which never changes;
//! it is not one of the project's repositories. Everything a repository without a forge has works the same in it: the
//! cards of a project without a repository are on its board, their runs work in worktrees of it under the same
//! tool-call check, their changes are reviewed and merged into it, and the Files page lists it. It has no `origin`, so
//! nothing refreshes it, and it goes only with its project.

use std::path::{Path, PathBuf};

use crate::app::{App, AppError, Result};
use crate::bus::Event;
use crate::store::{Project, Repository};

/// Where the files of the project with `key` are kept under `data_dir`.
fn files_path(data_dir: &Path, key: &str) -> PathBuf {
    data_dir.join("projects").join(key)
}

impl App {
    /// The repository that holds the own files of the project `id`, made the first time it is asked for: a new git
    /// repository at [`files_path`], named by the project's key. A repository a person added with that name is
    /// refused, since the key is the files' name for good.
    pub fn project_files(&self, id: i64) -> Result<Repository> {
        let project = self.project(id)?;
        if let Some(files) = project.files {
            return self.repository(files);
        }
        if self
            .store
            .repositories()?
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case(&project.key))
        {
            return Err(AppError::Exists(format!(
                "a repository named {}, the name of the files of the project {}",
                project.key, project.name
            )));
        }
        let (path, branch) = self.init_project_files(&project.key)?;
        self.record_project_files(&project, &path, &branch)
    }

    /// Makes the git repository of the files of the project with `key` at [`files_path`] and returns its path and
    /// default branch. A folder left by a project that was removed with its key is made anew.
    pub(super) fn init_project_files(&self, key: &str) -> Result<(PathBuf, String)> {
        let path = files_path(&self.data_dir, key);
        if path.exists() {
            std::fs::remove_dir_all(&path).map_err(|e| AppError::Invalid(e.to_string()))?;
        }
        self.repos.init(&path)?;
        let branch = self.repos.default_branch(&path)?;
        Ok((path, branch))
    }

    /// Records the git repository at `path` as the files of `project` and announces both.
    pub(super) fn record_project_files(
        &self,
        project: &Project,
        path: &Path,
        branch: &str,
    ) -> Result<Repository> {
        let files = self.store.add_project_files(
            project.id,
            &project.key,
            &path.to_string_lossy(),
            branch,
        )?;
        self.bus.publish(Event::RepositoryAdded(files.clone()));
        self.bus
            .publish(Event::ProjectChanged(self.project(project.id)?));
        Ok(files)
    }

    /// Makes the files of every project that has neither a repository nor its files yet, such as one made before
    /// projects had files; a failure is reported on the bus and the others are still made.
    pub fn make_missing_project_files(&self) {
        let Ok(projects) = self.store.projects() else {
            return;
        };
        for project in projects
            .iter()
            .filter(|p| p.repositories.is_empty() && p.files.is_none())
        {
            if let Err(e) = self.project_files(project.id) {
                self.bus.publish(Event::ForgeError {
                    repository: project.key.clone(),
                    message: format!("cannot make the files of the project {}: {e}", project.name),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::decide::Actor;
    use crate::repos::git;
    use crate::store::Store;

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(std::time::Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        (dir, app)
    }

    #[test]
    fn a_new_project_has_files_of_its_own_that_are_none_of_its_repositories() {
        let (dir, app) = app();
        let project = app.add_project("Pricing page", "PRICE").unwrap();
        let files = app.repository(project.files.unwrap()).unwrap();
        assert_eq!(
            (files.name.as_str(), files.forge.as_str(), files.files_of),
            ("PRICE", "none", Some(project.id))
        );
        assert_eq!(files.home_project_id, project.id);
        assert!(project.repositories.is_empty());
        let path = dir.path().join("data/projects/PRICE");
        assert_eq!(Path::new(&files.path), path);
        assert_eq!(
            git(&path, &["log", "--format=%s"]).unwrap(),
            "Start the repository"
        );
        // Asked again, it is the same.
        assert_eq!(app.project_files(project.id).unwrap().id, files.id);

        // Its cards are on the project's board.
        let card = app
            .create_issue(files.id, "Write the prices", "", crate::app::OWNER_AUTHOR)
            .unwrap();
        assert_eq!(card.project_id, project.id);
        assert_eq!(
            app.project_view_of(project.id, &crate::app::Sight::everything())
                .unwrap()
                .open_cards,
            1
        );

        // Nothing refreshes it, and it goes only with its project.
        app.refresh(&files).unwrap();
        let refused = app
            .remove_repository(&Actor::first_owner(), files.id, "")
            .unwrap_err();
        assert!(refused.to_string().contains("files"), "{refused}");
        assert!(path.exists());
    }

    #[test]
    fn the_key_of_a_project_is_the_name_of_its_files() {
        let (dir, app) = app();
        let origin = dir.path().join("origin");
        app.repos.init(&origin).unwrap();
        // A project from before files were made has none yet, and its key is no repository's to take.
        let bare = app.store.add_project("Contracts", "LEGAL").unwrap();
        assert!(bare.files.is_none());
        for name in ["LEGAL", "legal"] {
            let refused = app
                .add_repository(name, origin.to_str().unwrap())
                .unwrap_err();
            assert!(refused.to_string().contains("key"), "{refused}");
        }
        let notes = app
            .add_repository("notes", origin.to_str().unwrap())
            .unwrap();

        // The next start makes its files; a project with a repository needs none.
        app.make_missing_project_files();
        let files = app.project(bare.id).unwrap().files.unwrap();
        assert_eq!(app.repository(files).unwrap().name, "LEGAL");
        assert!(app.project(notes.home_project_id).unwrap().files.is_none());
        // Nor can a project take a key a repository has as its name, in any case.
        assert!(app.add_project("Notebook", "NOTES").is_err());
    }

    #[test]
    fn a_project_whose_files_cannot_be_made_is_not_made() {
        let (dir, app) = app();
        // A file where the folder of the projects' files goes keeps git from making them.
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data/projects"), "in the way").unwrap();
        assert!(app.add_project("Pricing page", "PRICE").is_err());
        assert!(app.store.project_by_key("PRICE").unwrap().is_none());
        std::fs::remove_file(dir.path().join("data/projects")).unwrap();
        assert!(app.add_project("Pricing page", "PRICE").is_ok());
    }
}
