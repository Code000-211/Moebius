//! Projects (ADR 0019): making one with a name and a key, renaming it (its key stays), and what the Projects page shows
//! of each: its repositories, its members and its open cards. The records are in `store::projects`; the settings of a
//! project, the files the harness configuration repository keeps for it, are in `settings`; the project's own store of
//! files, where the cards of a project without a repository are worked on, is in `files`.

mod files;
mod settings;

use serde::{Deserialize, Serialize};

use super::{App, AppError, Result};
use crate::bus::Event;
use crate::store::{KEY_LIMIT, Project, StoreError, is_key};

/// The most characters a project's name has.
pub const NAME_LIMIT: usize = 80;

/// A repository of a project, as a project names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRepository {
    pub id: i64,
    pub name: String,
    /// Whether this project is the repository's home project, where its new issues land.
    pub home: bool,
}

/// A person who works in a project, as a project shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub id: i64,
    pub handle: String,
    pub name: String,
}

/// A project as the desktop API answers it: the record, its repositories by name, its members and its open cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectView {
    pub id: i64,
    pub name: String,
    pub key: String,
    pub created_at: i64,
    /// In the order the project names them.
    pub repositories: Vec<ProjectRepository>,
    /// Everyone of the organisation who is not disabled, while no project is private.
    pub members: Vec<Member>,
    /// The cards on the project's board that are neither closed nor in `Done`.
    pub open_cards: i64,
    /// The id of the repository that holds the project's own files (see `files`), once it is made; the board of a
    /// project without a repository is that repository's.
    pub files: Option<i64>,
}

/// Refuses a name for a project: empty, longer than [`NAME_LIMIT`], starting with a dot, or with a slash, a backslash
/// or a control character, since the name is also a folder of the harness configuration repository.
fn check_name(name: &str) -> Result<()> {
    let plain = !name.is_empty()
        && name.chars().count() <= NAME_LIMIT
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control());
    if plain && name.trim() == name {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "{name:?} is not a project name: 1 to {NAME_LIMIT} characters, no slash, backslash or control character, \
             not starting with a dot or a space"
        )))
    }
}

/// A store error, as `Exists` with `what` when it is a unique column refusing a value another project took meanwhile.
pub(super) fn taken(error: StoreError, what: &str) -> AppError {
    match error {
        StoreError::Sql(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            AppError::Exists(what.to_string())
        }
        other => other.into(),
    }
}

impl App {
    /// Makes a project with `name` and `key` and no repository yet, with its own files (see `files`). The key is an
    /// uppercase letter and then 1 to 9 uppercase letters or digits, and no other project may have it, nor a
    /// repository as its name, since it names the project's files; no other project may have the name either, in any
    /// case.
    pub fn add_project(&self, name: &str, key: &str) -> Result<ProjectView> {
        check_name(name)?;
        if !is_key(key) {
            return Err(AppError::Invalid(format!(
                "{key:?} is not a project key: an uppercase letter, then 1 to {} uppercase letters or digits",
                KEY_LIMIT - 1
            )));
        }
        if self.store.project_by_key(key)?.is_some() {
            return Err(AppError::Exists(format!("a project with the key {key}")));
        }
        if self.store.project_by_name(name)?.is_some() {
            return Err(AppError::Exists(format!("a project named {name}")));
        }
        if self
            .store
            .repositories()?
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case(key))
        {
            return Err(AppError::Exists(format!("a repository named {key}")));
        }
        // The files are made first: a project whose files cannot be made is not made either.
        let (path, branch) = self.init_project_files(key)?;
        let project = self
            .store
            .add_project(name, key)
            .map_err(|e| taken(e, &format!("a project named {name} or with the key {key}")))?;
        self.record_project_files(&project, &path, &branch)?;
        self.project_view_of(project.id)
    }

    /// Renames the project `id`; its key stays as it was, and a `key` that is not the project's own is refused. A name
    /// another project has, in any case, is refused.
    pub fn rename_project(&self, id: i64, name: &str, key: Option<&str>) -> Result<ProjectView> {
        let project = self.project(id)?;
        if key.is_some_and(|key| key != project.key) {
            return Err(AppError::Invalid(format!(
                "a project keeps its key {}: the keys of its cards would change",
                project.key
            )));
        }
        check_name(name)?;
        if project.name == name {
            return self.project_view(project);
        }
        if self
            .store
            .project_by_name(name)?
            .is_some_and(|other| other.id != id)
        {
            return Err(AppError::Exists(format!("a project named {name}")));
        }
        self.store
            .rename_project(id, name)
            .map_err(|e| taken(e, &format!("a project named {name}")))?;
        let renamed = self.project(id)?;
        self.bus.publish(Event::ProjectChanged(renamed.clone()));
        self.project_view(renamed)
    }

    /// The project `id`, or `NotFound`.
    pub fn project(&self, id: i64) -> Result<Project> {
        self.store
            .project(id)?
            .ok_or_else(|| AppError::NotFound(format!("project {id}")))
    }

    /// Every project as the Projects page shows it, in the order they were made.
    pub fn project_views(&self) -> Result<Vec<ProjectView>> {
        self.store
            .projects()?
            .into_iter()
            .map(|project| self.project_view(project))
            .collect()
    }

    /// The project `id` as the Projects page shows it.
    pub fn project_view_of(&self, id: i64) -> Result<ProjectView> {
        self.project_view(self.project(id)?)
    }

    fn project_view(&self, project: Project) -> Result<ProjectView> {
        let all = self.store.repositories()?;
        let repositories = project
            .repositories
            .iter()
            .filter_map(|id| all.iter().find(|r| r.id == *id))
            .map(|r| ProjectRepository {
                id: r.id,
                name: r.name.clone(),
                home: r.home_project_id == project.id,
            })
            .collect();
        let members = self
            .store
            .people()?
            .into_iter()
            .filter(|person| !person.disabled)
            .map(|person| Member {
                id: person.id,
                handle: person.handle,
                name: person.name,
            })
            .collect();
        Ok(ProjectView {
            open_cards: self.store.open_cards(project.id)?,
            id: project.id,
            name: project.name,
            key: project.key,
            created_at: project.created_at,
            repositories,
            members,
            files: project.files,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::{Role, Store};

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = App::new(
            store,
            Arc::new(Host::new(std::time::Duration::from_secs(1))),
            &dir.path().join("data"),
        );
        (dir, app)
    }

    #[test]
    fn a_new_project_needs_a_key_no_other_project_has() {
        let (_dir, app) = app();
        let mut events = app.bus.subscribe();
        let shop = app.add_project("Storefront", "SHOP").unwrap();
        assert_eq!((shop.key.as_str(), shop.open_cards), ("SHOP", 0));
        assert!(shop.repositories.is_empty());
        // Its files are announced, then the project with them.
        assert_eq!(events.try_recv().unwrap().kind, "repository_added");
        assert_eq!(events.try_recv().unwrap().kind, "project_changed");

        let taken = app.add_project("Shop", "SHOP").unwrap_err();
        assert!(matches!(taken, AppError::Exists(_)), "{taken}");
        let named = app.add_project("storefront", "STORE").unwrap_err();
        assert!(matches!(named, AppError::Exists(_)), "{named}");
        for key in ["shop", "S", "2FA", "SHOP-1", "ABCDEFGHIJK", ""] {
            let refused = app.add_project("Other", key).unwrap_err();
            assert!(matches!(refused, AppError::Invalid(_)), "{key}: {refused}");
        }
        for name in [
            "",
            " Shop",
            ".hidden",
            "a/b",
            "a\\b",
            "tab\there",
            &"x".repeat(81),
        ] {
            let refused = app.add_project(name, "OTHER").unwrap_err();
            assert!(
                matches!(refused, AppError::Invalid(_)),
                "{name:?}: {refused}"
            );
        }
        assert_eq!(app.project_views().unwrap().len(), 1);
    }

    #[test]
    fn a_renamed_project_keeps_its_key() {
        let (_dir, app) = app();
        let shop = app.add_project("Storefront", "SHOP").unwrap();
        app.add_project("Marketing", "MKT").unwrap();
        let renamed = app.rename_project(shop.id, "Shop", None).unwrap();
        assert_eq!(
            (renamed.name.as_str(), renamed.key.as_str()),
            ("Shop", "SHOP")
        );
        assert_eq!(app.project_view_of(shop.id).unwrap().key, "SHOP");
        // Its own name in another case is no clash; another project's is.
        assert_eq!(
            app.rename_project(shop.id, "SHOP", Some("SHOP"))
                .unwrap()
                .name,
            "SHOP"
        );
        assert!(matches!(
            app.rename_project(shop.id, "marketing", None),
            Err(AppError::Exists(_))
        ));
        assert!(matches!(
            app.rename_project(shop.id, "Shop", Some("SHOPS")),
            Err(AppError::Invalid(_))
        ));
        // A name taken between the check and the write is still a taken name, not a failure of the server.
        assert!(matches!(
            taken(
                app.store.add_project("Marketing", "OTHER").unwrap_err(),
                "a project named Marketing"
            ),
            AppError::Exists(_)
        ));
        assert!(matches!(
            app.rename_project(99, "Gone", None),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn a_project_shows_its_repositories_members_and_open_cards() {
        let (dir, app) = app();
        let path = dir.path().join("web").to_string_lossy().into_owned();
        let web = app.add_repository("web", &path).unwrap();
        app.store
            .add_card(web.id, "Open", "", "lea", "Backlog")
            .unwrap();
        app.store
            .add_card(web.id, "Finished", "", "lea", "Done")
            .unwrap();
        app.store.add_person("lea", "Lea", Role::Member).unwrap();
        let gone = app.store.add_person("tom", "Tom", Role::Member).unwrap();
        app.store.set_person_disabled(gone.id, true).unwrap();

        let view = app.project_view_of(web.home_project_id).unwrap();
        assert_eq!((view.name.as_str(), view.key.as_str()), ("web", "WEB"));
        assert_eq!(
            view.repositories,
            [ProjectRepository {
                id: web.id,
                name: "web".into(),
                home: true
            }]
        );
        let handles: Vec<&str> = view.members.iter().map(|m| m.handle.as_str()).collect();
        assert_eq!(handles, ["owner", "lea"], "a disabled person is no member");
        assert_eq!(view.open_cards, 1);

        // The name of a project cannot be a new repository's, whose home project would have it.
        app.add_project("Docs", "DOCS").unwrap();
        let other = dir.path().join("docs").to_string_lossy().into_owned();
        assert!(matches!(
            app.add_repository("docs", &other),
            Err(AppError::Invalid(_))
        ));
    }
}
