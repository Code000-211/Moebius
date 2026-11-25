//! A project's settings in the clone (ADR 0019): the files of `projects/<project name>/` that say how the project's
//! board works, the scripts of its columns in place of the built-in ones. What is about code (rules, review notes,
//! checks, test first, the hint of files) stays with each repository, in `repos/<repository name>/` (see
//! `repositories`). They are read from the clone's default branch, and an edit is a commit on that branch pushed to
//! `origin`, like the other edits of the harness configuration repository (see `edits`).
//!
//! Renaming a project moves its folder in one commit. The migration moves the column scripts that a repository's
//! settings still have, from before projects, to the repository's home project, so that its board works as it did; a
//! script that arrives there later (a proposal or a package offered before projects and merged since, or a hand edit
//! in the old place) is the newest one, and moves too.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::HarnessRepo;
use super::edits::{Edit, remove_if_there};
use super::repositories::{DIR as REPOSITORIES_DIR, reserved_name};
use crate::app::columns::{KANBAN_FILE, safe_script_name};
use crate::app::{App, AppError, Result};
use crate::board::{Kanban, built_in_script};
use crate::decide::Actor;
use crate::repos::files::writable;
use crate::repos::git;
use crate::workflows::definition;

/// The directory of the clone that holds the settings of all projects.
pub const DIR: &str = "projects";

/// The kind of the decision recorded for the migration of the column scripts.
const MOVE_KIND: &str = "move_scripts";

/// The short id of a commit that moved workflow files, and the `(workflow, project)` pairs it moved.
pub type WorkflowMoves = (String, Vec<(String, String)>);

/// The kind of the decision recorded when the server-wide workflow files are moved to their projects.
const MOVE_WORKFLOWS_KIND: &str = "move_workflows";
/// The kind of the decision recorded for moving a project's folder when it is renamed.
const RENAME_KIND: &str = "rename_project";

/// Whether `file` is one of a project's files: the script of a column of the default Kanban, such as `ready.luau`,
/// the project's `kanban.toml` that defines its custom columns, or a script file such a column names (a plain
/// `.luau` file name, see `app::columns::safe_script_name`).
pub fn is_project_file(file: &str) -> bool {
    built_in_script(file).is_some() || file == KANBAN_FILE || safe_script_name(file)
}

/// Whether the project name `name` can be a folder on every system: not empty, not starting with a dot, not ending
/// with a dot or a space, without a slash, a backslash, a control character or one of `<>:"|?*`, and not a device
/// name of Windows.
pub fn valid_folder(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.ends_with(['.', ' '])
        && !reserved_name(name)
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*')
        })
}

/// The text of the file at `parts` below `root`; none when it is not there, cannot be read, or a part of its path is a
/// symbolic link, which (as `origin` may have added it) must not lead a read out of the clone.
fn read_below(root: &Path, parts: &[&str]) -> Option<String> {
    let mut path = root.to_path_buf();
    for part in parts {
        path.push(part);
        if path.symlink_metadata().ok()?.file_type().is_symlink() {
            return None;
        }
    }
    std::fs::read_to_string(path).ok()
}

impl HarnessRepo {
    /// The directory of the settings of the project `name`.
    pub fn project_dir(&self, name: &str) -> PathBuf {
        self.path.join(DIR).join(name)
    }

    /// The text of the project's `file` as the default branch has it; none when the project has no such file, the file
    /// is not one a project has, or it cannot be read.
    pub fn project_file(&self, name: &str, file: &str) -> Option<String> {
        if !valid_folder(name) || !is_project_file(file) {
            return None;
        }
        read_below(&self.path, &[DIR, name, file])
    }

    /// A person or agent writes the project's `file` (`Some(text)`) or removes it (`None`): committed on the default
    /// branch with `message` and pushed to `origin`, recorded as a decision of `kind` with `context`. The short id of the
    /// commit is given; a change that changes nothing makes no commit. A name that cannot be a folder or a file that is
    /// not one of a project's is refused with `Invalid`, and removing one the project does not have with `NotFound`.
    /// `name` gives the project's name once the clone is locked, so that a rename cannot come between reading the name
    /// and writing the file.
    pub fn save_project_file(
        &self,
        app: &App,
        by: &Actor,
        (name, file): (&dyn Fn() -> Result<String>, &str),
        text: Option<&str>,
        (kind, message): (&str, &str),
        mut context: BTreeMap<String, String>,
    ) -> Result<Option<String>> {
        let mut read_at = self.lock();
        let name = name()?;
        let name = name.as_str();
        if !valid_folder(name) {
            return Err(AppError::Invalid(format!(
                "{name:?} cannot be the folder {DIR}/{name}/ on every system; rename the project to keep its settings"
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
        if file == KANBAN_FILE {
            // The board must stay one: a valid file, and no card in a column it no longer has.
            let project = app
                .store
                .projects()?
                .into_iter()
                .find(|p| p.name == name)
                .ok_or_else(|| AppError::NotFound(format!("project {name}")))?;
            app.check_kanban_save(&project, text)?;
        }
        let path = format!("{DIR}/{name}/{file}");
        context.insert("project".into(), name.into());
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

    /// Moves the settings of the project `from` to `to` as the project is renamed, one commit pushed to `origin`, and
    /// then calls `rename`, which renames the record, while the clone is still locked, so that no edit of the project's
    /// settings comes between. When `rename` fails, the folder is moved back and its error is given. A project without
    /// settings makes no commit. Refused with `Invalid` when `to` cannot be a folder while there is something to move,
    /// and with `Conflict` when `projects/<to>/` is there already.
    pub fn move_project(
        &self,
        app: &App,
        by: &Actor,
        (from, to): (&str, &str),
        rename: impl FnOnce() -> Result<()>,
    ) -> Result<Option<String>> {
        let mut read_at = self.lock();
        let moved = self.move_folder(app, by, &mut read_at, (from, to))?;
        if let Err(error) = rename() {
            if moved.is_some()
                && let Err(back) = self.move_folder(app, by, &mut read_at, (to, from))
            {
                eprintln!("moebius: cannot move the settings of {to} back to {from}: {back}");
            }
            return Err(error);
        }
        Ok(moved)
    }

    /// Moves `projects/<from>/` to `projects/<to>/` in one commit, with the clone locked by the caller (`read_at`).
    fn move_folder(
        &self,
        app: &App,
        by: &Actor,
        read_at: &mut String,
        (from, to): (&str, &str),
    ) -> Result<Option<String>> {
        let old = self.project_dir(from);
        let moving = valid_folder(from) && old.symlink_metadata().is_ok_and(|m| m.is_dir());
        if !moving || from == to {
            return Ok(None);
        }
        if !valid_folder(to) {
            return Err(AppError::Invalid(format!(
                "{to:?} cannot be the folder {DIR}/{to}/ on every system"
            )));
        }
        // A name that differs only in case is the same folder where file names ignore case.
        if !from.eq_ignore_ascii_case(to) && self.project_dir(to).symlink_metadata().is_ok() {
            return Err(AppError::Conflict(format!(
                "{DIR}/{to}/ is in the harness configuration repository already; move or remove it first"
            )));
        }
        let message = format!("Rename the project {from} to {to}");
        let context = BTreeMap::from([
            ("from".to_string(), format!("{DIR}/{from}")),
            ("to".to_string(), format!("{DIR}/{to}")),
        ]);
        let edit = Edit {
            kind: RENAME_KIND,
            files: DIR,
            message: &message,
        };
        let write = || {
            let from = format!("{DIR}/{from}");
            let to = format!("{DIR}/{to}");
            git(&self.path, &["mv", "--", &from, &to])?;
            Ok(())
        };
        self.commit(app, by, read_at, &edit, context, write)
    }

    /// Moves the column scripts that the settings of a repository still have to its project, for each `(repository,
    /// project)` of `homes`: `repos/<repository>/<script>` becomes `projects/<project>/<script>`, all in one commit
    /// pushed to `origin` and recorded as a decision of the first owner. A script the project has already is replaced:
    /// one that is in `repos/` after the first migration was put there since, so it is the newer. Nothing to move makes
    /// no commit. The moved paths are given.
    pub fn move_scripts_to_projects(
        &self,
        app: &App,
        homes: &[(String, String)],
    ) -> Result<Vec<String>> {
        let mut read_at = self.lock();
        let scripts: Vec<String> = Kanban::default()
            .columns
            .into_iter()
            .map(|column| column.script)
            .collect();
        let mut moves = Vec::new();
        for (repository, project) in homes {
            if !valid_folder(project) {
                continue;
            }
            for script in &scripts {
                let from = format!("{REPOSITORIES_DIR}/{repository}/{script}");
                let to = format!("{DIR}/{project}/{script}");
                let there = read_below(&self.path, &[REPOSITORIES_DIR, repository, script]);
                if there.is_some() {
                    moves.push((from, to));
                }
            }
        }
        if moves.is_empty() {
            return Ok(Vec::new());
        }
        let moved: Vec<String> = moves
            .iter()
            .map(|(from, to)| format!("{from} -> {to}"))
            .collect();
        let context = BTreeMap::from([("moved".to_string(), moved.join("\n"))]);
        let edit = Edit {
            kind: MOVE_KIND,
            files: DIR,
            message: "Move the column scripts of each repository to its project",
        };
        let write = || {
            for (from, to) in &moves {
                let target = writable(&self.path, to)?;
                std::fs::rename(self.path.join(from), target)?;
                // The script leaves the repository's settings whether it was committed or not.
                git(
                    &self.path,
                    &["rm", "--cached", "--quiet", "--ignore-unmatch", "--", from],
                )?;
            }
            Ok(())
        };
        self.commit(
            app,
            &Actor::first_owner(),
            &mut read_at,
            &edit,
            context,
            write,
        )?;
        Ok(moved)
    }

    /// Moves server-wide workflow files to the projects that own them (ADR 0025), for each `(workflow, project)` of
    /// `moves`: `workflows/<workflow>.toml` becomes `projects/<project>/workflows/<workflow>.toml`, all in one commit
    /// pushed to `origin` and recorded as a decision of the first owner (kind `move_workflows`). A move whose file is
    /// gone, whose target is there already or whose project cannot be a folder is left out, so nothing is overwritten
    /// and a second call with the same moves makes no commit. The short id of the commit and the moves made are given;
    /// nothing to move makes no commit.
    pub fn move_workflows_to_projects(
        &self,
        app: &App,
        moves: &[(String, String)],
    ) -> Result<Option<WorkflowMoves>> {
        let mut read_at = self.lock();
        let moves: Vec<(String, String)> = moves
            .iter()
            .filter(|(workflow, project)| {
                valid_folder(project)
                    && definition::file_for(workflow).is_ok()
                    && self
                        .path
                        .join(format!("{}/{workflow}.toml", definition::DIRECTORY))
                        .is_file()
                    && !self
                        .path
                        .join(format!(
                            "{}/{workflow}.toml",
                            definition::project_directory(project)
                        ))
                        .exists()
            })
            .cloned()
            .collect();
        if moves.is_empty() {
            return Ok(None);
        }
        let lines: Vec<String> = moves
            .iter()
            .map(|(workflow, project)| format!("{workflow} -> {project}"))
            .collect();
        let context = BTreeMap::from([("moved".to_string(), lines.join("\n"))]);
        let edit = Edit {
            kind: MOVE_WORKFLOWS_KIND,
            files: DIR,
            message: "Move the workflows to their projects",
        };
        let write = || {
            for (workflow, project) in &moves {
                let from = format!("{}/{workflow}.toml", definition::DIRECTORY);
                let to = format!("{}/{workflow}.toml", definition::project_directory(project));
                let target = writable(&self.path, &to)?;
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::rename(self.path.join(&from), target)?;
                // The file leaves the server-wide folder whether it was committed or not.
                git(
                    &self.path,
                    &["rm", "--cached", "--quiet", "--ignore-unmatch", "--", &from],
                )?;
            }
            Ok(())
        };
        let commit = self.commit(
            app,
            &Actor::first_owner(),
            &mut read_at,
            &edit,
            context,
            write,
        )?;
        Ok(commit.map(|commit| (commit, moves)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::harness::repo::testing;
    use crate::store::Store;
    use moebius_plugin_api::Host;

    fn setup() -> (tempfile::TempDir, App, Arc<HarnessRepo>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(std::time::Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        let repo = testing::use_harness(&app, dir.path());
        (dir, app, repo)
    }

    fn save(
        app: &App,
        repo: &HarnessRepo,
        project: &str,
        file: &str,
        text: Option<&str>,
    ) -> Result<Option<String>> {
        repo.save_project_file(
            app,
            &Actor::first_owner(),
            (&|| Ok(project.to_string()), file),
            text,
            ("edit_script", "Change"),
            BTreeMap::new(),
        )
    }

    #[test]
    fn a_project_has_only_column_scripts_and_names_that_are_folders_everywhere() {
        let (_dir, app, repo) = setup();
        for (project, file) in [
            ("Web", "rules.md"),
            ("Web", "checks"),
            ("Web", "../repos/demo/ready.luau"),
            ("a/b", "ready.luau"),
            ("a:b", "ready.luau"),
            ("nul", "ready.luau"),
            ("Web.", "ready.luau"),
            (".hidden", "ready.luau"),
            ("", "ready.luau"),
        ] {
            assert!(
                matches!(
                    save(&app, &repo, project, file, Some("x")),
                    Err(AppError::Invalid(_))
                ),
                "{project}/{file}"
            );
            assert_eq!(repo.project_file(project, file), None, "{project}/{file}");
        }
        assert!(
            save(&app, &repo, "The web app", "ready.luau", Some("-- mine\n"))
                .unwrap()
                .is_some()
        );
        assert_eq!(
            repo.project_file("The web app", "ready.luau").unwrap(),
            "-- mine\n"
        );
        assert!(matches!(
            save(&app, &repo, "The web app", "code.luau", None),
            Err(AppError::NotFound(_))
        ));
        assert!(
            save(&app, &repo, "The web app", "ready.luau", None)
                .unwrap()
                .is_some()
        );
        assert_eq!(repo.project_file("The web app", "ready.luau"), None);
    }

    #[test]
    fn renaming_a_project_moves_its_folder_in_one_commit() {
        let (_dir, app, repo) = setup();
        // Nothing to move: no commit, whatever the new name.
        assert_eq!(
            repo.move_project(&app, &Actor::first_owner(), ("Web", "Site"), || Ok(()))
                .unwrap(),
            None
        );
        save(&app, &repo, "Web", "ready.luau", Some("-- web\n")).unwrap();
        assert!(
            repo.move_project(&app, &Actor::first_owner(), ("Web", "Site"), || Ok(()))
                .unwrap()
                .is_some()
        );
        assert_eq!(repo.project_file("Site", "ready.luau").unwrap(), "-- web\n");
        assert_eq!(repo.project_file("Web", "ready.luau"), None);
        // A record that cannot be renamed moves the folder back.
        let refused = repo.move_project(&app, &Actor::first_owner(), ("Site", "Shop"), || {
            Err(AppError::Exists("a project named Shop".into()))
        });
        assert!(matches!(refused, Err(AppError::Exists(_))));
        assert_eq!(repo.project_file("Site", "ready.luau").unwrap(), "-- web\n");
        assert!(!repo.project_dir("Shop").exists());
        let status = git(repo.path(), &["status", "--porcelain"]).unwrap();
        assert!(status.is_empty(), "{status}");
        // A folder the new name has already is not overwritten.
        save(&app, &repo, "Old", "code.luau", Some("-- old\n")).unwrap();
        assert!(matches!(
            repo.move_project(&app, &Actor::first_owner(), ("Site", "Old"), || Ok(())),
            Err(AppError::Conflict(_))
        ));
        assert_eq!(repo.project_file("Site", "ready.luau").unwrap(), "-- web\n");
    }

    #[test]
    fn renaming_a_project_carries_its_columns_and_their_scripts_along() {
        let (_dir, app, repo) = setup();
        let board = "[[column]]\nname = \"Ready for testing\"\nscript = \"testing.luau\"\nafter = \"In review\"\n";
        app.store.add_project("Web", "WEB").unwrap();
        save(&app, &repo, "Web", "kanban.toml", Some(board)).unwrap();
        save(&app, &repo, "Web", "testing.luau", Some("-- test\n")).unwrap();
        assert!(
            repo.move_project(&app, &Actor::first_owner(), ("Web", "Site"), || Ok(()))
                .unwrap()
                .is_some()
        );
        assert_eq!(repo.project_file("Site", "kanban.toml").unwrap(), board);
        assert_eq!(
            repo.project_file("Site", "testing.luau").unwrap(),
            "-- test\n"
        );
        assert_eq!(repo.project_file("Web", "kanban.toml"), None);
        assert_eq!(repo.project_file("Web", "testing.luau"), None);
        let status = git(repo.path(), &["status", "--porcelain"]).unwrap();
        assert!(status.is_empty(), "{status}");
    }

    #[test]
    fn the_migration_leaves_a_repositorys_other_luau_files_and_its_kanban_file_where_they_are() {
        let (_dir, app, repo) = setup();
        let old = repo.repository_dir("api");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("ready.luau"), "-- ready\n").unwrap();
        std::fs::write(old.join("testing.luau"), "-- not a built-in column's\n").unwrap();
        std::fs::write(old.join("kanban.toml"), "# not a repository's file\n").unwrap();
        git(repo.path(), &["add", "-A"]).unwrap();
        git(repo.path(), &["commit", "--quiet", "-m", "Old settings"]).unwrap();
        let homes = [("api".to_string(), "API".to_string())];
        assert_eq!(
            repo.move_scripts_to_projects(&app, &homes).unwrap(),
            ["repos/api/ready.luau -> projects/API/ready.luau"]
        );
        assert_eq!(repo.project_file("API", "testing.luau"), None);
        assert_eq!(repo.project_file("API", "kanban.toml"), None);
        assert!(repo.path().join("repos/api/testing.luau").exists());
    }

    #[test]
    fn the_migration_moves_each_repositorys_scripts_to_its_project_and_a_later_one_too() {
        let (_dir, app, repo) = setup();
        // As a repository's settings had them before projects.
        let commit_old = |files: &[(&str, &str)]| {
            let old = repo.repository_dir("api");
            std::fs::create_dir_all(&old).unwrap();
            for (file, text) in files {
                std::fs::write(old.join(file), text).unwrap();
            }
            git(repo.path(), &["add", "-A"]).unwrap();
            git(repo.path(), &["commit", "--quiet", "-m", "Old settings"]).unwrap();
        };
        commit_old(&[
            ("ready.luau", "-- ready\n"),
            ("code.luau", "-- code\n"),
            ("checks", "make\n"),
        ]);
        let homes = [("api".to_string(), "API".to_string())];
        let moved = repo.move_scripts_to_projects(&app, &homes).unwrap();
        assert_eq!(
            moved,
            [
                "repos/api/ready.luau -> projects/API/ready.luau",
                "repos/api/code.luau -> projects/API/code.luau",
            ]
        );
        assert_eq!(
            repo.project_file("API", "ready.luau").unwrap(),
            "-- ready\n"
        );
        assert!(!repo.path().join("repos/api/ready.luau").exists());
        assert_eq!(repo.repository_file("api", "checks").unwrap(), "make\n");
        let status = git(repo.path(), &["status", "--porcelain"]).unwrap();
        assert!(status.is_empty(), "{status}");
        // Moved once: the next pass has nothing to do.
        assert!(
            repo.move_scripts_to_projects(&app, &homes)
                .unwrap()
                .is_empty()
        );

        // A proposal offered before projects and merged since puts a script in the old place: it is the newer, and
        // replaces the project's.
        save(&app, &repo, "API", "code.luau", Some("-- edited since\n")).unwrap();
        commit_old(&[("code.luau", "-- approved later\n")]);
        let moved = repo.move_scripts_to_projects(&app, &homes).unwrap();
        assert_eq!(moved, ["repos/api/code.luau -> projects/API/code.luau"]);
        assert_eq!(
            repo.project_file("API", "code.luau").unwrap(),
            "-- approved later\n"
        );
        assert!(!repo.path().join("repos/api/code.luau").exists());
    }
}
