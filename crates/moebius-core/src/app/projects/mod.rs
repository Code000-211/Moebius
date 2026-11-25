//! Projects (ADR 0019): making one with a name and a key, renaming it (its key stays), and what the Projects page shows
//! of each: its repositories, its members and its open cards. The records are in `store::projects`; the settings of a
//! project and of a repository, the files the harness configuration repository keeps for them, are in `settings`
//! (renaming a project moves its settings with it); the project's own store of files, where the cards of a project
//! without a repository are worked on, is in `files`; who sees a private
//! project and what of it is in `sight`.

mod files;
mod settings;
mod sight;

pub use sight::{Sight, UNQUOTED};

use serde::{Deserialize, Serialize};

use super::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::harness::repo::valid_project_folder;
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
    /// Who works in it: the people a private project names, everyone of the organisation in a public one; disabled
    /// people are left out.
    pub members: Vec<Member>,
    /// The cards on the project's board that are neither closed nor in `Done`; 0 for someone who does not see them.
    pub open_cards: i64,
    /// The id of the repository that holds the project's own files (see `files`), once it is made; the board of a
    /// project without a repository is that repository's.
    pub files: Option<i64>,
    /// The ids of the kinds of issues the project offers (see `kinds`), in the order the kinds were made.
    pub kinds: Vec<i64>,
    /// Whether the project takes requests (see `app::intake`).
    pub intake: bool,
    /// Whether only its members see it.
    pub private: bool,
    /// Whether the person asking sees its board: true for a public project, and for a private one when they are one of
    /// its members. An owner who is not sees the project's record only, to add its members.
    pub member: bool,
    /// The project's board, left to right: the six built-in columns and the custom ones of its `kanban.toml`.
    #[serde(default)]
    pub columns: Vec<crate::app::columns::ColumnView>,
    /// What is wrong with the project's `kanban.toml`, if anything: the board then runs as the notice says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns_problem: Option<String>,
}

/// Refuses a name for a project: empty, longer than [`NAME_LIMIT`], starting with a dot or a space, ending with a dot
/// or a space, with a slash, a backslash, a control character or one of `<>:"|?*`, or a device name of Windows, since
/// the name is also the folder `projects/<name>/` of the harness configuration repository on every system.
fn check_name(name: &str) -> Result<()> {
    let plain = name.chars().count() <= NAME_LIMIT && valid_project_folder(name);
    if plain && name.trim() == name {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "{name:?} is not a project name: 1 to {NAME_LIMIT} characters, none of / \\ < > : \" | ? * or a control \
             character, not starting with a dot or a space, not ending with a dot or a space, and not a device name of \
             Windows such as nul"
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
        self.project_view_of(project.id, &Sight::everything())
    }

    /// `by` renames the project `id`; its key stays as it was, and a `key` that is not the project's own is refused. A
    /// name another project has, in any case, is refused. The project's settings move to `projects/<name>/` in a commit
    /// of the harness configuration repository, and the record changes while that repository is still locked; if the
    /// record cannot change, the folder is moved back.
    pub fn rename_project(
        &self,
        by: &Actor,
        id: i64,
        name: &str,
        key: Option<&str>,
    ) -> Result<ProjectView> {
        let sight = self.sight(by)?;
        let project = self.project_seen(id, &sight)?;
        if key.is_some_and(|key| key != project.key) {
            return Err(AppError::Invalid(format!(
                "a project keeps its key {}: the keys of its cards would change",
                project.key
            )));
        }
        check_name(name)?;
        if project.name == name {
            return self.project_view(project, &sight);
        }
        if self
            .store
            .project_by_name(name)?
            .is_some_and(|other| other.id != id)
        {
            return Err(AppError::Exists(format!("a project named {name}")));
        }
        let rename = || {
            self.store
                .rename_project(id, name)
                .map(drop)
                .map_err(|e| taken(e, &format!("a project named {name}")))
        };
        match self.harness_repo.get() {
            Some(harness) => harness.move_project(self, by, (&project.name, name), rename)?,
            None => {
                rename()?;
                None
            }
        };
        let renamed = self.project(id)?;
        self.bus.publish(Event::ProjectChanged(renamed.clone()));
        self.project_view(renamed, &sight)
    }

    /// Whether the project `id` may be deleted (ADR 0025): `Conflict` while it owns workflow files or any of its
    /// workflow runs is queued, running or waiting, `Ok` otherwise. The finished runs of a project are no reason to
    /// keep it: they keep its id as a plain value, no cascade removes them, and show the project as deleted.
    pub fn check_project_deletable(&self, id: i64) -> Result<()> {
        let project = self.project(id)?;
        let active = self.store.active_runs_of_project(id)?;
        if active > 0 {
            return Err(AppError::Conflict(format!(
                "the project {} has {active} workflow run{} queued, running or waiting; wait for {} or cancel {}",
                project.name,
                if active == 1 { "" } else { "s" },
                if active == 1 { "it" } else { "them" },
                if active == 1 { "it" } else { "them" },
            )));
        }
        if let Some(harness) = self.harness_repo.get()
            && harness.project_owns_workflows(&project.name)?
        {
            return Err(AppError::Conflict(format!(
                "the project {} owns workflows; delete them first",
                project.name
            )));
        }
        Ok(())
    }

    /// An owner makes the project `id` private or public and names its members, by their handles (with or without the
    /// `@`), each once. A private project is seen only by its members, the owner who sets it included only when they
    /// name themselves. Recorded as an `edit_project_access` decision and announced as `project_changed`.
    pub fn set_project_access(
        &self,
        by: &Actor,
        id: i64,
        private: bool,
        members: &[String],
        reason: &str,
    ) -> Result<ProjectView> {
        let sight = self.sight(by)?;
        let project = self.project_seen(id, &sight)?;
        let mut ids = Vec::new();
        let mut handles = Vec::new();
        for handle in members {
            let handle = handle.trim().trim_start_matches('@');
            let person = self
                .store
                .person_by_handle(handle)?
                .ok_or_else(|| AppError::Invalid(format!("no person has the handle @{handle}")))?;
            if !ids.contains(&person.id) {
                ids.push(person.id);
                handles.push(person.handle);
            }
        }
        self.store.set_project_access(id, private, &ids)?;
        let context = std::collections::BTreeMap::from([
            ("project".to_string(), project.key.clone()),
            ("private".to_string(), private.to_string()),
            ("members".to_string(), handles.join(", ")),
            ("previous_private".to_string(), project.private.to_string()),
        ]);
        self.record_action(by, "edit_project_access", None, None, context, reason)?;
        let changed = self.project(id)?;
        self.bus.publish(Event::ProjectChanged(changed.clone()));
        self.project_view(changed, &self.sight(by)?)
    }

    /// An owner puts the repository `repository` among the repositories of the project `id`; nothing changes when it is
    /// one already. The repository's home project stays where its new issues land. Recorded as an
    /// `edit_project_repositories` decision and announced as `project_changed`.
    pub fn add_repository_to_project(
        &self,
        by: &Actor,
        id: i64,
        repository: i64,
        reason: &str,
    ) -> Result<ProjectView> {
        let sight = self.sight(by)?;
        let project = self.project_seen(id, &sight)?;
        let repo = self.repository_of_project(repository, &sight)?;
        if !project.repositories.contains(&repo.id) && repo.home_project_id != id {
            self.store.link_repository(id, repo.id)?;
            self.record_repositories(by, &project, "add", &repo.name, reason)?;
        }
        let changed = self.project(id)?;
        self.bus.publish(Event::ProjectChanged(changed.clone()));
        self.project_view(changed, &sight)
    }

    /// An owner takes the repository `repository` out of the project `id`. A repository's home project keeps it: the
    /// repository has to be removed instead. Recorded as an `edit_project_repositories` decision.
    pub fn remove_repository_from_project(
        &self,
        by: &Actor,
        id: i64,
        repository: i64,
        reason: &str,
    ) -> Result<ProjectView> {
        let sight = self.sight(by)?;
        let project = self.project_seen(id, &sight)?;
        let repo = self.repository_of_project(repository, &sight)?;
        if repo.home_project_id == id {
            return Err(AppError::Invalid(format!(
                "{} is the home project of {}: remove the repository to take it out",
                project.name, repo.name
            )));
        }
        if !self.store.unlink_repository(id, repo.id)? {
            return Err(AppError::NotFound(format!(
                "repository {} in project {id}",
                repo.id
            )));
        }
        self.record_repositories(by, &project, "remove", &repo.name, reason)?;
        let changed = self.project(id)?;
        self.bus.publish(Event::ProjectChanged(changed.clone()));
        self.project_view(changed, &sight)
    }

    /// The repository `id` when `sight` sees it, or `NotFound`.
    fn repository_of_project(&self, id: i64, sight: &Sight) -> Result<crate::store::Repository> {
        if !sight.repository(id) {
            return Err(AppError::NotFound(format!("repository {id}")));
        }
        self.store
            .repository(id)?
            .ok_or_else(|| AppError::NotFound(format!("repository {id}")))
    }

    /// Records an `edit_project_repositories` decision: `change` is `add` or `remove`.
    fn record_repositories(
        &self,
        by: &Actor,
        project: &Project,
        change: &str,
        repository: &str,
        reason: &str,
    ) -> Result<()> {
        let context = std::collections::BTreeMap::from([
            ("project".to_string(), project.key.clone()),
            ("change".to_string(), change.to_string()),
            ("repository".to_string(), repository.to_string()),
        ]);
        self.record_action(by, "edit_project_repositories", None, None, context, reason)?;
        Ok(())
    }

    /// The project `id`, or `NotFound`.
    pub fn project(&self, id: i64) -> Result<Project> {
        self.store
            .project(id)?
            .ok_or_else(|| AppError::NotFound(format!("project {id}")))
    }

    /// The project `id` when `sight` sees at least its record, or `NotFound`.
    pub(super) fn project_seen(&self, id: i64, sight: &Sight) -> Result<Project> {
        if !sight.project_record(id) {
            return Err(AppError::NotFound(format!("project {id}")));
        }
        self.project(id)
    }

    /// Every project `sight` sees, as the Projects page shows it, in the order they were made.
    pub fn project_views(&self, sight: &Sight) -> Result<Vec<ProjectView>> {
        self.store
            .projects()?
            .into_iter()
            .filter(|project| sight.project_record(project.id))
            .map(|project| self.project_view(project, sight))
            .collect()
    }

    /// The project `id` as the Projects page shows it, or `NotFound` when `sight` does not see it.
    pub fn project_view_of(&self, id: i64, sight: &Sight) -> Result<ProjectView> {
        self.project_view(self.project_seen(id, sight)?, sight)
    }

    fn project_view(&self, project: Project, sight: &Sight) -> Result<ProjectView> {
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
            .filter(|person| !project.private || project.members.contains(&person.id))
            .map(|person| Member {
                id: person.id,
                handle: person.handle,
                name: person.name,
            })
            .collect();
        let member = sight.project(project.id);
        let board = self.project_columns(&project);
        Ok(ProjectView {
            columns: board.views(),
            columns_problem: board.problem.as_ref().map(|(_, p)| p.clone()),
            kinds: self.store.project_kinds(project.id)?,
            open_cards: if member {
                self.store.open_cards(project.id)?
            } else {
                0
            },
            private: project.private,
            member,
            id: project.id,
            name: project.name,
            key: project.key,
            created_at: project.created_at,
            repositories,
            members,
            files: project.files,
            intake: project.intake,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::{Role, Store};

    pub(super) fn app() -> (tempfile::TempDir, App) {
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
            "a:b",
            "Shop?",
            "Shop.",
            "nul",
            "COM1.txt",
            &"x".repeat(81),
        ] {
            let refused = app.add_project(name, "OTHER").unwrap_err();
            assert!(
                matches!(refused, AppError::Invalid(_)),
                "{name:?}: {refused}"
            );
        }
        assert_eq!(app.project_views(&Sight::everything()).unwrap().len(), 1);
    }

    #[test]
    fn a_renamed_project_keeps_its_key() {
        let (_dir, app) = app();
        let shop = app.add_project("Storefront", "SHOP").unwrap();
        app.add_project("Marketing", "MKT").unwrap();
        let renamed = app
            .rename_project(&Actor::first_owner(), shop.id, "Shop", None)
            .unwrap();
        assert_eq!(
            (renamed.name.as_str(), renamed.key.as_str()),
            ("Shop", "SHOP")
        );
        assert_eq!(
            app.project_view_of(shop.id, &Sight::everything())
                .unwrap()
                .key,
            "SHOP"
        );
        // Its own name in another case is no clash; another project's is.
        assert_eq!(
            app.rename_project(&Actor::first_owner(), shop.id, "SHOP", Some("SHOP"))
                .unwrap()
                .name,
            "SHOP"
        );
        assert!(matches!(
            app.rename_project(&Actor::first_owner(), shop.id, "marketing", None),
            Err(AppError::Exists(_))
        ));
        assert!(matches!(
            app.rename_project(&Actor::first_owner(), shop.id, "Shop", Some("SHOPS")),
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
            app.rename_project(&Actor::first_owner(), 99, "Gone", None),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn an_owner_puts_a_repository_in_another_project_and_takes_it_out() {
        let (dir, app) = app();
        let path = dir.path().join("web").to_string_lossy().into_owned();
        let web = app.add_repository("web", &path).unwrap();
        let shop = app.add_project("Storefront", "SHOP").unwrap();
        let owner = Actor::first_owner();
        let mut events = app.bus.subscribe();
        let added = app
            .add_repository_to_project(&owner, shop.id, web.id, "the shop is built from web")
            .unwrap();
        assert_eq!(
            added.repositories,
            vec![ProjectRepository {
                id: web.id,
                name: "web".into(),
                home: false
            }]
        );
        assert_eq!(events.try_recv().unwrap().kind, "project_changed");
        // Again changes nothing, and its home project keeps it too.
        let again = app
            .add_repository_to_project(&owner, shop.id, web.id, "")
            .unwrap();
        assert_eq!(again.repositories.len(), 1);
        let home = app
            .add_repository_to_project(&owner, web.home_project_id, web.id, "")
            .unwrap();
        assert!(home.repositories.iter().all(|r| r.home));
        // The home project cannot give the repository up.
        assert!(matches!(
            app.remove_repository_from_project(&owner, web.home_project_id, web.id, ""),
            Err(AppError::Invalid(_))
        ));
        let removed = app
            .remove_repository_from_project(&owner, shop.id, web.id, "")
            .unwrap();
        assert!(removed.repositories.is_empty());
        assert!(matches!(
            app.remove_repository_from_project(&owner, shop.id, web.id, ""),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            app.add_repository_to_project(&owner, shop.id, 99, ""),
            Err(AppError::NotFound(_))
        ));
        let kinds: Vec<_> = app
            .store
            .judgements_page(None, None, Some("edit_project_repositories"), 5)
            .unwrap();
        assert_eq!(kinds.len(), 2);
    }

    #[test]
    fn an_owner_makes_a_project_private_and_names_its_members() {
        let (dir, app) = app();
        let path = dir.path().join("web").to_string_lossy().into_owned();
        let web = app.add_repository("web", &path).unwrap();
        app.store
            .add_card(web.id, "Open", "", "lea", "Backlog")
            .unwrap();
        let lea = app.store.add_person("lea", "Lea", Role::Member).unwrap();
        let ben = app.store.add_person("ben", "Ben", Role::Member).unwrap();
        let owner = Actor::first_owner();
        let project = web.home_project_id;
        let mut events = app.bus.subscribe();

        // The owner names only Lea: the owner still sees the record, not the board.
        let view = app
            .set_project_access(&owner, project, true, &["@lea".into(), "lea".into()], "")
            .unwrap();
        assert!(view.private && !view.member);
        assert_eq!(view.open_cards, 0);
        let handles: Vec<&str> = view.members.iter().map(|m| m.handle.as_str()).collect();
        assert_eq!(handles, ["lea"]);
        assert_eq!(events.try_recv().unwrap().kind, "project_changed");
        let decided = app
            .store
            .judgements_page(None, None, Some("edit_project_access"), 5)
            .unwrap();
        assert_eq!(decided.len(), 1);

        let lea_sees = app.sight(&Actor::person(lea.id, "lea")).unwrap();
        assert_eq!(
            app.project_view_of(project, &lea_sees).unwrap().open_cards,
            1
        );
        let ben_sees = app.sight(&Actor::person(ben.id, "ben")).unwrap();
        assert!(matches!(
            app.project_view_of(project, &ben_sees),
            Err(AppError::NotFound(_))
        ));
        assert!(app.project_views(&ben_sees).unwrap().is_empty());
        assert!(matches!(
            app.rename_project(&Actor::person(ben.id, "ben"), project, "Hidden", None),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            app.set_project_access(&owner, project, true, &["nobody".into()], ""),
            Err(AppError::Invalid(_))
        ));
        // Public again, it is everyone's.
        let open = app
            .set_project_access(&owner, project, false, &[], "")
            .unwrap();
        assert!(!open.private && open.member && open.members.len() == 3);
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

        let view = app
            .project_view_of(web.home_project_id, &Sight::everything())
            .unwrap();
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
        assert!(!view.private && view.member);

        // The name of a project cannot be a new repository's, whose home project would have it.
        app.add_project("Docs", "DOCS").unwrap();
        let other = dir.path().join("docs").to_string_lossy().into_owned();
        assert!(matches!(
            app.add_repository("docs", &other),
            Err(AppError::Invalid(_))
        ));
    }
}
