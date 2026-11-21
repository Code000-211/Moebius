//! The one-time migration of the workflow files that were kept server-wide, at `workflows/<name>.toml`, to the projects
//! that own them (ADR 0025), at `projects/<project>/workflows/<name>.toml`, and of the store rows that go with them.
//!
//! The rules, per file:
//!
//! - The repository of a workflow is the first one the file names that the server has: the `repository` parameter of a
//!   `schedule` or `webhook` trigger, then the default of a `manual` trigger's `repository` input, then the first node
//!   whose `repository` parameter names one (a template such as `{{ trigger.repository }}` names none).
//! - That repository goes to its home project: the oldest project that is not private among those that hold it, or,
//!   when only private projects hold it, the oldest of them (the workflow belongs there by its repository). A project's
//!   own files repository goes to that project.
//! - A workflow that names no repository goes to the oldest project that is not private. Never to a private one.
//! - A file no project can take (no project at all, only private ones for a workflow with no repository, a file that
//!   cannot be read, a project that has a workflow of that name already) stays where it is, is said so with the event
//!   `workflows_migration_deferred`, shows on Get started, and is tried again at every start.
//!
//! The order makes a crash harmless. The files move first, all in one commit of the harness configuration repository;
//! the store step comes after and works from what the files are *now*: a project that holds a file by a name that has
//! server-wide rows (`workflow_runs` without a project, a `workflow_schedules` row of project 0, an `automations` row
//! without a project) is the owner of those rows. Each statement only touches a row that is still unplaced, so the
//! step can be repeated, and a start after a crash between the commit and the step finishes it. Nothing is copied, so
//! no schedule has two rows to fire from, and the minute a schedule last fired at moves with its row.

use crate::app::{App, AppError, Result};
use crate::bus::Event;
use crate::harness::repo::valid_project_folder;
use crate::store::{Project, Repository};
use crate::workflows::definition::{self, WorkflowDefinition};

/// A server-wide file and where the migration puts it: the project, or why no project can take it yet.
type Placement = (String, std::result::Result<Project, String>);

impl App {
    /// Moves every server-wide workflow file to its project and the rows that belong to those files with them (see the
    /// module's rules). What cannot be moved is said in the log and as `workflows_migration_deferred`; what fails is
    /// said as `harness_error`. Nothing here stops the server from starting, and everything is tried again at the next
    /// start.
    pub fn migrate_workflows_to_projects(&self) {
        if let Err(error) = self.move_workflow_files() {
            self.report_workflow_migration_error(&error);
        }
        // Whatever the files did, the rows follow the files that are in the projects now.
        if let Err(error) = self.settle_workflow_rows() {
            self.report_workflow_migration_error(&error);
        }
        for (workflow, reason) in self.workflows_awaiting_a_project() {
            eprintln!("moebius: the workflow {workflow} stays where it is for now: {reason}");
            self.bus
                .publish(Event::WorkflowsMigrationDeferred { workflow, reason });
        }
    }

    /// The first step: remembers where each file goes, then moves them all in one commit. Where a file went is what the
    /// second step ([`Self::settle_workflow_rows`]) gives its rows to, so that the step does not have to guess it from
    /// what the projects hold afterwards.
    fn move_workflow_files(&self) -> Result<()> {
        let Some(harness) = self.harness_repo.get() else {
            return Ok(());
        };
        let moves: Vec<(String, Project)> = self
            .plan_workflow_migration()?
            .into_iter()
            .filter_map(|(workflow, project)| Some((workflow, project.ok()?)))
            .collect();
        if moves.is_empty() {
            return Ok(());
        }
        for (workflow, project) in &moves {
            self.store.record_workflow_move(workflow, project.id)?;
        }
        let names: Vec<(String, String)> = moves
            .iter()
            .map(|(workflow, project)| (workflow.clone(), project.name.clone()))
            .collect();
        if let Some((commit, made)) = harness.move_workflows_to_projects(self, &names)? {
            eprintln!(
                "moebius: moved the workflows to their projects ({commit}): {}",
                made.iter()
                    .map(|(workflow, project)| format!("{workflow} -> {project}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            self.bus.publish(Event::WorkflowsMigrated {
                commit,
                moved: made.len(),
            });
        }
        Ok(())
    }

    /// The server-wide workflow files that stay where they are, each with why, for Get started to show as pending. A
    /// server without a harness configuration repository has none.
    pub fn workflows_awaiting_a_project(&self) -> Vec<(String, String)> {
        self.plan_workflow_migration()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(workflow, placement)| Some((workflow, placement.err()?)))
            .collect()
    }

    fn report_workflow_migration_error(&self, error: &AppError) {
        let message = format!("cannot move the workflows to their projects: {error}");
        eprintln!("moebius: {message}");
        self.bus.publish(Event::HarnessError {
            agent: None,
            message,
        });
    }

    /// Where each server-wide file would go now, in name order.
    fn plan_workflow_migration(&self) -> Result<Vec<Placement>> {
        let Some(harness) = self.harness_repo.get() else {
            return Ok(Vec::new());
        };
        let names = definition::names(harness.path())
            .map_err(|error| AppError::Invalid(error.to_string()))?;
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let projects = oldest_first(self.store.projects()?);
        let repositories = self.store.repositories()?;
        Ok(names
            .into_iter()
            .map(|name| {
                let placement = self.place_workflow_file(&name, &projects, &repositories);
                (name, placement)
            })
            .collect())
    }

    fn place_workflow_file(
        &self,
        name: &str,
        projects: &[Project],
        repositories: &[Repository],
    ) -> std::result::Result<Project, String> {
        let root = self
            .harness_repo
            .get()
            .expect("a plan is made with a harness configuration repository")
            .path();
        let definition = definition::read_text(root, name)
            .and_then(|text| WorkflowDefinition::parse(&text, &format!("workflows/{name}.toml")))
            .map_err(|error| format!("it cannot be read: {error}"))?;
        let project = project_of(&definition, projects, repositories)?.clone();
        if !valid_project_folder(&project.name) {
            return Err(
                "the project it belongs to has a name that is no folder on every system".into(),
            );
        }
        let taken = definition::project_file_for(&project.name, name)
            .is_ok_and(|file| root.join(file).exists());
        if taken {
            return Err(
                "the project it belongs to has a workflow of that name already; rename one of them"
                    .into(),
            );
        }
        Ok(project)
    }

    /// Gives the store rows of the workflows that left `workflows/` to the project that holds their file now: the
    /// runs that record no project, the schedule rows of project 0 (with the minute they last fired at) and the
    /// automations without a project. A name no project holds, or that a server-wide file still holds, is left alone.
    /// Safe to run at every start and after a crash.
    pub fn settle_workflow_rows(&self) -> Result<()> {
        if self.harness_repo.get().is_none() {
            return Ok(());
        }
        let mut names = self.store.workflows_with_unplaced_runs()?;
        names.extend(self.store.server_wide_schedule_names()?);
        names.extend(self.store.server_wide_automation_keys()?);
        names.extend(self.store.workflow_move_names()?);
        names.sort();
        names.dedup();
        for name in names {
            let Some(project) = self.owner_of_server_wide_rows(&name)? else {
                continue;
            };
            self.store.place_workflow_runs(&name, project.id)?;
            self.store.move_schedule_to_project(&name, project.id)?;
            self.store.move_automation_to_project(&name, project.id)?;
            self.store.forget_workflow_move(&name)?;
        }
        Ok(())
    }

    /// The project the unplaced rows of the workflow `name` belong to: the one that holds a file of that name now.
    /// With several, the one the migration would have chosen for that file, else the oldest.
    fn owner_of_server_wide_rows(&self, name: &str) -> Result<Option<Project>> {
        let harness = self.harness_repo()?;
        let root = harness.path();
        // The file is still server-wide, so its rows are too.
        if root
            .join(definition::DIRECTORY)
            .join(format!("{name}.toml"))
            .is_file()
        {
            return Ok(None);
        }
        let projects = oldest_first(self.store.projects()?);
        // The project the file was moved to is the owner, whatever else other projects hold under the name.
        if let Some(moved_to) = self.store.workflow_move(name)?
            && let Some(project) = projects.iter().find(|project| project.id == moved_to)
            && definition::project_file_for(&project.name, name)
                .is_ok_and(|file| root.join(file).is_file())
        {
            return Ok(Some(project.clone()));
        }
        let holders: Vec<&Project> = projects
            .iter()
            .filter(|project| {
                definition::project_file_for(&project.name, name)
                    .is_ok_and(|file| root.join(file).is_file())
            })
            .collect();
        if holders.len() > 1 {
            let repositories = self.store.repositories()?;
            for project in &holders {
                let chosen = definition::read_in_project(root, &project.name, name)
                    .ok()
                    .and_then(|definition| {
                        project_of(&definition, &projects, &repositories)
                            .ok()
                            .map(|p| p.id)
                    });
                if chosen == Some(project.id) {
                    return Ok(Some((*project).clone()));
                }
            }
        }
        Ok(holders.first().map(|project| (*project).clone()))
    }
}

/// The projects, the oldest first.
fn oldest_first(mut projects: Vec<Project>) -> Vec<Project> {
    projects.sort_by_key(|project| (project.created_at, project.id));
    projects
}

/// The project the workflow `definition` belongs to, by the rules of the module; the reason when none does.
fn project_of<'a>(
    definition: &WorkflowDefinition,
    projects: &'a [Project],
    repositories: &[Repository],
) -> std::result::Result<&'a Project, String> {
    let repository = named_repositories(definition).iter().find_map(|name| {
        repositories
            .iter()
            .find(|repository| repository.name == *name)
    });
    if let Some(repository) = repository
        && let Some(project) = home_project(repository, projects)
    {
        return Ok(project);
    }
    // No repository to follow: the oldest project anyone may see.
    projects
        .iter()
        .find(|project| !project.private)
        .ok_or_else(|| {
            if projects.is_empty() {
                "no project exists yet; it moves when one is created".to_string()
            } else {
                "it names no repository and every project is private; it moves when a project that is not \
                 private exists"
                    .to_string()
            }
        })
}

/// The home project of `repository` among `projects` (oldest first): its files' project, or the oldest project that
/// holds it and is not private, or else the oldest private one that holds it.
fn home_project<'a>(repository: &Repository, projects: &'a [Project]) -> Option<&'a Project> {
    if let Some(project) = repository.files_of {
        return projects.iter().find(|candidate| candidate.id == project);
    }
    let holding = || {
        projects
            .iter()
            .filter(|project| project.repositories.contains(&repository.id))
    };
    holding()
        .find(|project| !project.private)
        .or_else(|| holding().next())
}

/// The repository names a workflow's file gives, in the order the rules look at them: a `schedule` or `webhook`
/// trigger's `repository`, a `manual` trigger's `repository` input with a default, a node's `repository`.
fn named_repositories(definition: &WorkflowDefinition) -> Vec<String> {
    let text = |value: Option<&serde_json::Value>| {
        value
            .and_then(|value| value.as_str())
            .filter(|name| !name.is_empty() && !name.contains("{{"))
            .map(str::to_string)
    };
    let mut names = Vec::new();
    for trigger in &definition.trigger {
        if matches!(trigger.kind.as_str(), "schedule" | "webhook") {
            names.extend(text(trigger.params.get("repository")));
        }
    }
    for trigger in definition.trigger.iter().filter(|t| t.kind == "manual") {
        let inputs = trigger
            .params
            .get("inputs")
            .and_then(|inputs| inputs.as_array());
        for input in inputs.into_iter().flatten() {
            if input.get("type").and_then(|ty| ty.as_str()) == Some("repository") {
                names.extend(text(input.get("default")));
            }
        }
    }
    for node in &definition.nodes {
        names.extend(text(node.params.get("repository")));
    }
    names
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;
    use time::macros::datetime;

    use super::*;
    use crate::decide::Actor;
    use crate::harness::repo::testing;
    use crate::repos::git;
    use crate::store::{NewAutomation, Store};

    /// A workflow with the trigger `trigger` (a line of TOML after `kind`), and one node that names `node` as its
    /// repository when it is not empty.
    fn workflow(trigger: &str, node: &str) -> String {
        let params = if node.is_empty() {
            String::new()
        } else {
            format!("params = {{ repository = \"{node}\" }}\n")
        };
        format!(
            "name = \"w\"\ndescription = \"A test workflow.\"\n\n[[trigger]]\n{trigger}\n\n[[nodes]]\nid = \"build\"\nkind = \"test\"\n{params}\n\
             [[edges]]\nfrom = \"trigger\"\nport = \"out\"\nto = \"build\"\n"
        )
    }

    const SCHEDULE_DOCS: &str =
        "kind = \"schedule\"\nparams = { cron = \"30 3 * * *\", repository = \"docs\" }";
    const MANUAL: &str = "kind = \"manual\"";

    fn app() -> (tempfile::TempDir, Arc<App>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = Arc::new(App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(1))),
            &dir.path().join("data"),
        ));
        testing::use_harness(&app, dir.path());
        (dir, app)
    }

    /// The repositories `api`, `web` and `docs`, each with the project it comes with; `web` also joins `api`, which is
    /// older, so `web` is in two projects.
    fn repositories(app: &App, dir: &Path) {
        let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
        let api = app.add_repository("api", &path("api")).unwrap();
        let web = app.add_repository("web", &path("web")).unwrap();
        app.add_repository("docs", &path("docs")).unwrap();
        app.add_repository_to_project(&Actor::first_owner(), api.home_project_id, web.id, "shared")
            .unwrap();
    }

    fn put(app: &App, name: &str, text: &str) {
        let folder = app.harness_repo().unwrap().path().join("workflows");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(format!("{name}.toml")), text).unwrap();
    }

    fn project(app: &App, name: &str) -> Project {
        app.store.project_by_name(name).unwrap().unwrap()
    }

    /// Where a file is: the name of the project that holds it, none while it is server-wide, and a panic when it is
    /// nowhere.
    fn home(app: &App, name: &str) -> Option<String> {
        let root = app.harness_repo().unwrap().path().to_path_buf();
        let server_wide = root.join("workflows").join(format!("{name}.toml")).exists();
        let held: Vec<String> = app
            .store
            .projects()
            .unwrap()
            .into_iter()
            .filter(|project| {
                root.join("projects")
                    .join(&project.name)
                    .join("workflows")
                    .join(format!("{name}.toml"))
                    .exists()
            })
            .map(|project| project.name)
            .collect();
        assert!(
            server_wide as usize + held.len() == 1,
            "{name}: server-wide {server_wide}, held by {held:?}"
        );
        held.into_iter().next()
    }

    #[test]
    fn a_file_goes_to_the_home_project_of_the_repository_it_names_in_the_order_of_the_rules() {
        let (dir, app) = app();
        repositories(&app, dir.path());
        // A schedule's repository beats a manual input's default, which beats a node's.
        put(&app, "by-schedule", &workflow(SCHEDULE_DOCS, "api"));
        put(
            &app,
            "by-input",
            &workflow(
                "kind = \"manual\"\nparams = { inputs = [{ name = \"repo\", type = \"repository\", default = \"docs\" }] }",
                "api",
            ),
        );
        put(&app, "by-node", &workflow(MANUAL, "docs"));
        put(
            &app,
            "by-webhook",
            &workflow(
                "kind = \"webhook\"\nparams = { path = \"build\", repository = \"docs\" }",
                "",
            ),
        );
        // A template names no repository, and one the server does not have is no repository either.
        put(
            &app,
            "templated",
            &workflow(MANUAL, "{{ trigger.repository }}"),
        );
        put(&app, "unknown", &workflow(MANUAL, "nobody"));
        app.migrate_workflows_to_projects();
        assert_eq!(home(&app, "by-schedule").as_deref(), Some("docs"));
        assert_eq!(home(&app, "by-input").as_deref(), Some("docs"));
        assert_eq!(home(&app, "by-node").as_deref(), Some("docs"));
        assert_eq!(home(&app, "by-webhook").as_deref(), Some("docs"));
        // The oldest project that is not private takes what names no repository.
        assert_eq!(home(&app, "templated").as_deref(), Some("api"));
        assert_eq!(home(&app, "unknown").as_deref(), Some("api"));
    }

    #[test]
    fn a_file_that_names_no_repository_goes_to_the_oldest_project() {
        let (dir, app) = app();
        repositories(&app, dir.path());
        put(&app, "plain", &workflow(MANUAL, ""));
        app.migrate_workflows_to_projects();
        assert_eq!(home(&app, "plain").as_deref(), Some("api"));
    }

    #[test]
    fn a_repository_in_two_projects_goes_to_the_oldest_of_them() {
        let (dir, app) = app();
        repositories(&app, dir.path());
        // `web` has its own project, made after `api`, and joined `api`.
        put(&app, "on-web", &workflow(MANUAL, "web"));
        app.migrate_workflows_to_projects();
        assert_eq!(home(&app, "on-web").as_deref(), Some("api"));
    }

    #[test]
    fn a_file_is_committed_once_and_keeps_its_text() {
        let (dir, app) = app();
        repositories(&app, dir.path());
        let text = workflow(SCHEDULE_DOCS, "");
        put(&app, "nightly", &text);
        put(&app, "plain", &workflow(MANUAL, ""));
        let root = app.harness_repo().unwrap().path().to_path_buf();
        let before = git(&root, &["rev-list", "--count", "HEAD"]).unwrap();
        let mut events = app.bus.subscribe();
        app.migrate_workflows_to_projects();
        let after = git(&root, &["rev-list", "--count", "HEAD"]).unwrap();
        assert_eq!(
            after.trim().parse::<u32>().unwrap(),
            before.trim().parse::<u32>().unwrap() + 1,
            "one commit for both files"
        );
        assert_eq!(
            git(&root, &["log", "-1", "--format=%s"]).unwrap().trim(),
            "Move the workflows to their projects"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("projects/docs/workflows/nightly.toml")).unwrap(),
            text
        );
        // The commit is on `origin` too.
        let bare = dir.path().join("harness-origin.git");
        assert_eq!(
            git(&bare, &["rev-parse", "main"]).unwrap(),
            git(&root, &["rev-parse", "HEAD"]).unwrap()
        );
        let mut kinds = Vec::new();
        while let Ok(event) = events.try_recv() {
            kinds.push((event.kind, event.data));
        }
        let migrated: Vec<_> = kinds
            .iter()
            .filter(|(kind, _)| kind == "workflows_migrated")
            .collect();
        assert_eq!(migrated.len(), 1);
        assert_eq!(migrated[0].1["moved"], 2);
        assert!(
            migrated[0].1["commit"]
                .as_str()
                .is_some_and(|c| !c.is_empty())
        );
    }

    #[test]
    fn a_schedule_keeps_its_minute_and_does_not_fire_again() {
        let (dir, app) = app();
        repositories(&app, dir.path());
        put(&app, "nightly", &workflow(SCHEDULE_DOCS, ""));
        app.store
            .record_schedule_firing("nightly", "2026-11-09 03:30")
            .unwrap();
        app.migrate_workflows_to_projects();
        let docs = project(&app, "docs");
        assert_eq!(
            app.store
                .project_schedule_firing(docs.id, "nightly")
                .unwrap()
                .as_deref(),
            Some("2026-11-09 03:30")
        );
        assert_eq!(app.store.schedule_firing("nightly").unwrap(), None);
        // The occurrence of that very minute has fired, in the project's row, and fires nowhere else.
        let harness = app.harness_repo().unwrap().clone();
        let started = crate::server::schedule::on_workflow_schedules(
            &app,
            Some(&*harness),
            None,
            datetime!(2026-11-09 03:30),
        )
        .unwrap();
        assert!(started.is_empty(), "{started:?}");
        // A project that already has a row for the name keeps its own, and the server-wide row is dropped.
        assert!(
            !app.store
                .move_schedule_to_project("nightly", docs.id)
                .unwrap()
        );
    }

    /// An automation of the repository `docs`, written to its server-wide workflow file, with the minute its schedule
    /// last fired at.
    fn automation(app: &App) -> (i64, String) {
        let docs = app
            .store
            .repositories()
            .unwrap()
            .into_iter()
            .find(|repository| repository.name == "docs")
            .unwrap();
        let new = NewAutomation {
            name: "Nightly sweep".into(),
            schedule: "30 3 * * *".into(),
            agent: "Moebius".into(),
            prompt: "Sweep.".into(),
            repository: Some(docs.id),
            harness: false,
            enabled: true,
        };
        let row = app.store.add_automation(&new, "2026-11-09 03:30").unwrap();
        app.migrate_automations().unwrap();
        let key = app.store.automation_workflow_key(row.id).unwrap().unwrap();
        (row.id, key)
    }

    #[test]
    fn an_automation_moves_with_its_file_and_keeps_its_minute() {
        let (dir, app) = app();
        repositories(&app, dir.path());
        let (id, key) = automation(&app);
        assert_eq!(app.store.automation_project(id).unwrap(), None);
        app.migrate_workflows_to_projects();
        let docs = project(&app, "docs");
        assert_eq!(home(&app, &key).as_deref(), Some("docs"));
        assert_eq!(app.store.automation_project(id).unwrap(), Some(docs.id));
        assert_eq!(
            app.store
                .automation_with_project_workflow_key(docs.id, &key)
                .unwrap()
                .map(|automation| automation.id),
            Some(id)
        );
        assert_eq!(
            app.store
                .project_schedule_firing(docs.id, &key)
                .unwrap()
                .as_deref(),
            Some("2026-11-09 03:30")
        );
        assert_eq!(app.store.schedule_firing(&key).unwrap(), None);
        // The next start does not give the automation a second file where the first one was.
        app.settle_workflow_rows().unwrap();
        app.migrate_automations().unwrap();
        assert!(
            !app.harness_repo()
                .unwrap()
                .path()
                .join(format!("workflows/{key}.toml"))
                .exists()
        );
    }

    #[test]
    fn existing_runs_take_the_project_of_their_workflow_whatever_their_repository() {
        let (dir, app) = app();
        repositories(&app, dir.path());
        put(&app, "nightly", &workflow(SCHEDULE_DOCS, ""));
        put(&app, "other", &workflow(MANUAL, ""));
        let by_name = |name: &str| {
            app.store
                .repositories()
                .unwrap()
                .into_iter()
                .find(|repository| repository.name == name)
                .unwrap()
        };
        // A run of `nightly` in `api`, which is no repository of the project `docs` the workflow goes to.
        let foreign = app
            .store
            .add_workflow_run(
                "nightly",
                by_name("api").id,
                "manual",
                &serde_json::json!({}),
            )
            .unwrap();
        let own = app
            .store
            .add_workflow_run(
                "nightly",
                by_name("docs").id,
                "manual",
                &serde_json::json!({}),
            )
            .unwrap();
        let other = app
            .store
            .add_workflow_run(
                "other",
                by_name("docs").id,
                "manual",
                &serde_json::json!({}),
            )
            .unwrap();
        // A run of a workflow with no file anywhere has no project to take.
        let gone = app
            .store
            .add_workflow_run(
                "deleted",
                by_name("docs").id,
                "manual",
                &serde_json::json!({}),
            )
            .unwrap();
        app.migrate_workflows_to_projects();
        let project_of = |id: i64| app.store.workflow_run(id).unwrap().unwrap().project_id;
        let docs = project(&app, "docs").id;
        let api = project(&app, "api").id;
        assert_eq!(project_of(foreign.id), Some(docs));
        assert_eq!(project_of(own.id), Some(docs));
        assert_eq!(project_of(other.id), Some(api));
        assert_eq!(project_of(gone.id), None);
        // The repository of a run is not touched.
        assert_eq!(
            app.store
                .workflow_run(foreign.id)
                .unwrap()
                .unwrap()
                .repository_id,
            by_name("api").id
        );
    }

    #[test]
    fn a_private_project_takes_only_what_belongs_to_it_by_its_repository() {
        let (dir, app) = app();
        let path = |name: &str| dir.path().join(name).to_string_lossy().into_owned();
        // `secret` is the oldest project and is private; `docs` is public.
        app.add_repository("secret", &path("secret")).unwrap();
        app.add_repository("docs", &path("docs")).unwrap();
        let secret = project(&app, "secret");
        app.store.set_project_access(secret.id, true, &[]).unwrap();
        put(&app, "by-repository", &workflow(MANUAL, "secret"));
        put(&app, "plain", &workflow(MANUAL, ""));
        app.migrate_workflows_to_projects();
        assert_eq!(home(&app, "by-repository").as_deref(), Some("secret"));
        assert_eq!(home(&app, "plain").as_deref(), Some("docs"));
    }

    #[test]
    fn with_only_private_projects_a_file_without_a_repository_waits_and_says_so() {
        let (dir, app) = app();
        let path = |name: &str| dir.path().join(name).to_string_lossy().into_owned();
        app.add_repository("secret", &path("secret")).unwrap();
        let secret = project(&app, "secret");
        app.store.set_project_access(secret.id, true, &[]).unwrap();
        put(&app, "plain", &workflow(MANUAL, ""));
        let mut events = app.bus.subscribe();
        app.migrate_workflows_to_projects();
        assert_eq!(home(&app, "plain"), None);
        let waiting = app.workflows_awaiting_a_project();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].0, "plain");
        assert!(waiting[0].1.contains("private"), "{waiting:?}");
        let mut deferred = Vec::new();
        while let Ok(event) = events.try_recv() {
            if event.kind == "workflows_migration_deferred" {
                deferred.push(event.data);
            }
        }
        assert_eq!(deferred.len(), 1);
        assert_eq!(deferred[0]["workflow"], "plain");
        assert!(!deferred[0]["reason"].as_str().unwrap().contains("secret"));
    }

    #[test]
    fn with_no_project_a_file_waits_and_moves_once_a_project_is_made() {
        let (_dir, app) = app();
        assert!(app.store.projects().unwrap().is_empty());
        put(&app, "plain", &workflow(MANUAL, ""));
        app.store
            .record_schedule_firing("plain", "2026-11-09 03:30")
            .unwrap();
        let root = app.harness_repo().unwrap().path().to_path_buf();
        let head = git(&root, &["rev-parse", "HEAD"]).unwrap();
        app.migrate_workflows_to_projects();
        assert_eq!(home(&app, "plain"), None);
        assert_eq!(
            git(&root, &["rev-parse", "HEAD"]).unwrap(),
            head,
            "no commit"
        );
        assert_eq!(app.workflows_awaiting_a_project().len(), 1);
        // The row stays where its file is.
        assert_eq!(
            app.store.schedule_firing("plain").unwrap().as_deref(),
            Some("2026-11-09 03:30")
        );
        let made = app.add_project("Contracts", "CTR").unwrap();
        app.migrate_workflows_to_projects();
        assert_eq!(home(&app, "plain").as_deref(), Some(made.name.as_str()));
        assert!(app.workflows_awaiting_a_project().is_empty());
        assert_eq!(
            app.store
                .project_schedule_firing(made.id, "plain")
                .unwrap()
                .as_deref(),
            Some("2026-11-09 03:30")
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_or_would_overwrite_another_stays() {
        let (dir, app) = app();
        repositories(&app, dir.path());
        put(&app, "broken", "this is not a workflow");
        put(&app, "taken", &workflow(SCHEDULE_DOCS, ""));
        let docs = project(&app, "docs");
        let folder = app
            .harness_repo()
            .unwrap()
            .path()
            .join("projects/docs/workflows");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("taken.toml"), "# the project's own\n").unwrap();
        app.migrate_workflows_to_projects();
        assert_eq!(home(&app, "broken"), None);
        // Both are there, once each: nothing was overwritten.
        assert_eq!(
            std::fs::read_to_string(folder.join("taken.toml")).unwrap(),
            "# the project's own\n"
        );
        assert!(
            app.harness_repo()
                .unwrap()
                .path()
                .join("workflows/taken.toml")
                .exists()
        );
        let waiting = app.workflows_awaiting_a_project();
        let names: Vec<&str> = waiting.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["broken", "taken"]);
        let _ = docs;
    }

    #[test]
    fn the_rows_of_a_moved_file_go_to_the_project_it_moved_to_not_to_an_older_one_with_the_same_name()
     {
        let (dir, app) = app();
        repositories(&app, dir.path());
        // `api` is the oldest project and has a workflow `x` of its own; the server-wide `x` names `docs`, a newer one.
        let own = app
            .harness_repo()
            .unwrap()
            .path()
            .join("projects/api/workflows");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("x.toml"), workflow(MANUAL, "")).unwrap();
        put(&app, "x", &workflow(SCHEDULE_DOCS, ""));
        app.store
            .record_schedule_firing("x", "2026-11-09 03:30")
            .unwrap();
        let docs_repository = app
            .store
            .repositories()
            .unwrap()
            .into_iter()
            .find(|repository| repository.name == "docs")
            .unwrap();
        let run = app
            .store
            .add_workflow_run("x", docs_repository.id, "manual", &serde_json::json!({}))
            .unwrap();
        app.migrate_workflows_to_projects();
        let docs = project(&app, "docs");
        let api = project(&app, "api");
        assert!(
            app.harness_repo()
                .unwrap()
                .path()
                .join("projects/docs/workflows/x.toml")
                .is_file()
        );
        assert_eq!(
            app.store.workflow_run(run.id).unwrap().unwrap().project_id,
            Some(docs.id)
        );
        assert_eq!(
            app.store
                .project_schedule_firing(docs.id, "x")
                .unwrap()
                .as_deref(),
            Some("2026-11-09 03:30")
        );
        assert_eq!(
            app.store.project_schedule_firing(api.id, "x").unwrap(),
            None
        );
        // What was remembered of the move is forgotten with the rows placed.
        assert_eq!(app.store.workflow_move("x").unwrap(), None);
    }

    /// Everything the migration changes, as text: where each file is, the runs' projects, the schedule rows and the
    /// automations' projects.
    fn snapshot(app: &App, names: &[&str]) -> String {
        let root = app.harness_repo().unwrap().path().to_path_buf();
        let mut out = Vec::new();
        for name in names {
            let server_wide = root.join("workflows").join(format!("{name}.toml")).exists();
            out.push(format!("{name} server-wide {server_wide}"));
            for project in app.store.projects().unwrap() {
                let file = root
                    .join("projects")
                    .join(&project.name)
                    .join("workflows")
                    .join(format!("{name}.toml"));
                out.push(format!("{name} in {} {}", project.name, file.exists()));
                out.push(format!(
                    "{name} fired in {} {:?}",
                    project.name,
                    app.store.project_schedule_firing(project.id, name).unwrap()
                ));
            }
            out.push(format!(
                "{name} fired server-wide {:?}",
                app.store.schedule_firing(name).unwrap()
            ));
        }
        for run in app.store.workflow_runs(None, None, 100).unwrap() {
            out.push(format!(
                "run {} {} {:?}",
                run.id, run.workflow, run.project_id
            ));
        }
        for automation in app.store.automations().unwrap() {
            out.push(format!(
                "automation {} {:?}",
                automation.id,
                app.store.automation_project(automation.id).unwrap()
            ));
        }
        out.join("\n")
    }

    /// A server with a schedule workflow, a plain one, an automation and runs of both, none of it migrated.
    fn old_server() -> (tempfile::TempDir, Arc<App>, Vec<String>) {
        let (dir, app) = app();
        repositories(&app, dir.path());
        put(&app, "nightly", &workflow(SCHEDULE_DOCS, ""));
        put(&app, "plain", &workflow(MANUAL, ""));
        app.store
            .record_schedule_firing("nightly", "2026-11-09 03:30")
            .unwrap();
        let (_, key) = automation(&app);
        let docs = app
            .store
            .repositories()
            .unwrap()
            .into_iter()
            .find(|repository| repository.name == "docs")
            .unwrap();
        for name in ["nightly", "plain"] {
            app.store
                .add_workflow_run(name, docs.id, "manual", &serde_json::json!({}))
                .unwrap();
        }
        (dir, app, vec!["nightly".into(), "plain".into(), key])
    }

    #[test]
    fn running_twice_changes_nothing() {
        let (_dir, app, names) = old_server();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        app.migrate_workflows_to_projects();
        let root = app.harness_repo().unwrap().path().to_path_buf();
        let head = git(&root, &["rev-parse", "HEAD"]).unwrap();
        let once = snapshot(&app, &names);
        let mut events = app.bus.subscribe();
        app.migrate_workflows_to_projects();
        assert_eq!(snapshot(&app, &names), once);
        assert_eq!(git(&root, &["rev-parse", "HEAD"]).unwrap(), head);
        assert_eq!(git(&root, &["status", "--porcelain"]).unwrap(), "");
        // No second move was announced either.
        while let Ok(event) = events.try_recv() {
            assert_ne!(event.kind, "workflows_migrated");
        }
        // Nothing is duplicated: each name is in exactly one place.
        for name in names {
            home(&app, name);
        }
    }

    #[test]
    fn a_crash_between_the_commit_and_the_store_step_is_repaired_by_the_next_start() {
        // The clean run.
        let (_clean_dir, clean, names) = old_server();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        clean.migrate_workflows_to_projects();
        let expected = snapshot(&clean, &names);

        // The run that stops after the commit.
        let (_dir, app, _) = old_server();
        // The first step, with the second not done.
        assert_eq!(app.plan_workflow_migration().unwrap().len(), 3);
        app.move_workflow_files().unwrap();
        // The files are moved and the rows are not.
        assert_ne!(snapshot(&app, &names), expected);
        assert!(app.store.server_wide_schedule_names().unwrap().len() >= 2);
        assert!(!app.store.workflows_with_unplaced_runs().unwrap().is_empty());

        // The next start, as `set_harness_repo` does it.
        app.settle_workflow_rows().unwrap();
        app.migrate_automations().unwrap();
        app.migrate_workflows_to_projects();
        assert_eq!(snapshot(&app, &names), expected);
        // And no file the crash left behind came back server-wide.
        let root = app.harness_repo().unwrap().path();
        assert!(!root.join("workflows").join("nightly.toml").exists());
        assert!(app.workflows_awaiting_a_project().is_empty());
    }
}
