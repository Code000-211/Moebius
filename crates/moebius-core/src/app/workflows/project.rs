//! Workflows of a project (ADR 0025): the rule for which repository a run of a project's workflow belongs to, and the
//! door that starts one. A run's repository is one of the project's repositories, or the project's own files when it
//! has none; what a trigger names (a manual trigger's repository input, the `repository` parameter of a schedule or a
//! webhook) is checked against that set and, left out, is the project's only repository.

use std::sync::Arc;

use serde_json::Value;

use crate::app::{App, AppError, Result};
use crate::store::{Project, Repository, WorkflowRun};

impl App {
    /// The repository a run of a workflow of the project `project` belongs to. `requested` is the name a trigger gives
    /// the repository, if it gives one.
    ///
    /// - The repository must be one of the project's repositories (its own files repository when it has none); another
    ///   repository, even one the server has, is refused with a message that lists the choices.
    /// - With none requested, a project with exactly one choice runs there; with more than one the run is refused,
    ///   again naming the choices.
    pub fn workflow_run_repository(
        &self,
        project: &Project,
        requested: Option<&str>,
    ) -> Result<Repository> {
        let choices = self.workflow_run_repositories(project)?;
        let names = || {
            choices
                .iter()
                .map(|repository| repository.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match requested.filter(|name| !name.is_empty()) {
            Some(name) => choices
                .iter()
                .find(|repository| repository.name == name)
                .cloned()
                .ok_or_else(|| {
                    AppError::Invalid(format!(
                        "the repository {name:?} is not one of the repositories of the project {}; a run of its workflows belongs to one of: {}",
                        project.name,
                        names()
                    ))
                }),
            None => match choices.as_slice() {
                [only] => Ok(only.clone()),
                _ => Err(AppError::Invalid(format!(
                    "the project {} has more than one repository, so the run must name one of: {}",
                    project.name,
                    names()
                ))),
            },
        }
    }

    /// The repositories a run of a workflow of `project` may belong to: the project's own, in the order it names them,
    /// or its files repository when it names none.
    fn workflow_run_repositories(&self, project: &Project) -> Result<Vec<Repository>> {
        if project.repositories.is_empty() {
            return Ok(vec![self.project_files(project.id)?]);
        }
        let repositories = self.store.repositories()?;
        Ok(project
            .repositories
            .iter()
            .filter_map(|id| repositories.iter().find(|repository| repository.id == *id))
            .cloned()
            .collect())
    }

    /// Stores a run of the workflow `workflow` of the project `project_id`, started with `value`, and drives it on its
    /// own thread, as [`Self::start_workflow_run`] does for a workflow kept server-wide. The run records the project and
    /// belongs to the repository [`Self::workflow_run_repository`] picks from `repository`, the name the trigger gave.
    /// Nothing is stored when the workflow cannot be read or the repository is refused.
    pub fn start_project_workflow_run(
        self: &Arc<Self>,
        project_id: i64,
        workflow: &str,
        repository: Option<&str>,
        value: Value,
    ) -> Result<WorkflowRun> {
        let project = self.project(project_id)?;
        let definition = self
            .harness_repo()?
            .project_workflow(&project.name, workflow)?;
        let repository = self.workflow_run_repository(&project, repository)?;
        self.start_workflow_run_with(definition, repository.id, value, (None, Some(project.id)))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use moebius_plugin_api::Host;
    use serde_json::json;

    use super::*;
    use crate::decide::Actor;
    use crate::harness::repo::testing;
    use crate::store::Store;
    use crate::workflows::definition::WorkflowDefinition;

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

    fn release(description: &str) -> WorkflowDefinition {
        WorkflowDefinition::parse(
            &format!(
                r#"
name = "release"
description = "{description}"

[[trigger]]
kind = "test"

[[nodes]]
id = "build"
kind = "test"

[[edges]]
from = "trigger"
port = "out"
to = "build"
"#
            ),
            "workflows/release.toml",
        )
        .unwrap()
    }

    fn save(app: &App, project: &str, definition: &WorkflowDefinition) {
        app.harness_repo()
            .unwrap()
            .save_project_workflow(app, &Actor::first_owner(), project, "release", definition)
            .unwrap();
    }

    /// A project with two repositories, `api` and `web`, one with only `docs`, and one with none.
    fn projects(app: &App, dir: &std::path::Path) -> [Project; 4] {
        let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
        let api = app.add_repository("api", &path("api")).unwrap();
        let web = app.add_repository("web", &path("web")).unwrap();
        app.add_repository("docs", &path("docs")).unwrap();
        let owner = Actor::first_owner();
        // `api` is the home project of the repository of that name; `web` joins it.
        app.add_repository_to_project(&owner, api.home_project_id, web.id, "the api serves web")
            .unwrap();
        let by_name = |name: &str| app.store.project_by_name(name).unwrap().unwrap();
        let empty = app.add_project("Contracts", "CON").unwrap();
        [
            by_name("api"),
            by_name("docs"),
            by_name(&empty.name),
            by_name("web"),
        ]
    }

    #[test]
    fn a_run_belongs_to_the_only_repository_of_its_project_and_to_the_files_of_one_with_none() {
        let (dir, app) = app();
        let [api, docs, contracts, _] = projects(&app, dir.path());
        let only = app.workflow_run_repository(&docs, None).unwrap();
        assert_eq!(only.name, "docs");
        assert_eq!(
            app.workflow_run_repository(&docs, Some("docs")).unwrap().id,
            only.id
        );
        let files = app.workflow_run_repository(&contracts, None).unwrap();
        assert_eq!(files.files_of, Some(contracts.id));
        // Naming the project's own files is no foreign repository either.
        assert_eq!(
            app.workflow_run_repository(&contracts, Some(&files.name))
                .unwrap()
                .id,
            files.id
        );
        // Two repositories: the run must name one of them.
        assert_eq!(
            app.workflow_run_repository(&api, Some("web")).unwrap().name,
            "web"
        );
    }

    #[test]
    fn an_ambiguous_run_is_refused_with_the_choices() {
        let (dir, app) = app();
        let [api, ..] = projects(&app, dir.path());
        let Err(AppError::Invalid(message)) = app.workflow_run_repository(&api, None) else {
            panic!("a project with two repositories needs a name");
        };
        assert_eq!(
            message,
            "the project api has more than one repository, so the run must name one of: api, web"
        );
    }

    #[test]
    fn a_repository_outside_the_project_is_refused_with_the_choices() {
        let (dir, app) = app();
        let [api, docs, contracts, _] = projects(&app, dir.path());
        let Err(AppError::Invalid(message)) = app.workflow_run_repository(&api, Some("docs"))
        else {
            panic!("docs is no repository of the project api");
        };
        assert_eq!(
            message,
            "the repository \"docs\" is not one of the repositories of the project api; a run of its workflows belongs to one of: api, web"
        );
        assert!(matches!(
            app.workflow_run_repository(&docs, Some("api")),
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            app.workflow_run_repository(&contracts, Some("docs")),
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            app.workflow_run_repository(&api, Some("nobody")),
            Err(AppError::Invalid(_))
        ));
    }

    #[test]
    fn a_started_run_records_its_project_and_a_refused_one_stores_nothing() {
        let (dir, app) = app();
        let [api, docs, ..] = projects(&app, dir.path());
        save(&app, &docs.name, &release("Build docs."));
        save(&app, &api.name, &release("Build the api."));

        let run = app
            .start_project_workflow_run(docs.id, "release", None, json!({}))
            .unwrap();
        assert_eq!(run.project_id, Some(docs.id));
        assert_eq!(run.repository_id, docs.repositories[0]);

        // Two repositories and none named, or one of another project: refused before anything is stored.
        for requested in [None, Some("docs")] {
            let refused = app.start_project_workflow_run(api.id, "release", requested, json!({}));
            assert!(
                matches!(refused, Err(AppError::Invalid(_))),
                "{requested:?}"
            );
        }
        assert_eq!(app.store.workflow_runs(None, None, 10).unwrap().len(), 1);
        let named = app
            .start_project_workflow_run(api.id, "release", Some("web"), json!({}))
            .unwrap();
        assert_eq!(named.project_id, Some(api.id));
        // A workflow the project does not have is not found, though another project has one of that name.
        let other = app.add_project("Other", "OTH").unwrap();
        assert!(matches!(
            app.start_project_workflow_run(other.id, "release", None, json!({})),
            Err(AppError::Invalid(_))
        ));
    }

    #[test]
    fn a_workflow_of_one_project_is_invisible_to_another_and_names_repeat() {
        let (dir, app) = app();
        let [api, docs, ..] = projects(&app, dir.path());
        let harness = app.harness_repo().unwrap();
        assert!(harness.project_workflows(&api.name).unwrap().is_empty());
        save(&app, &api.name, &release("Build the api."));
        assert_eq!(harness.project_workflows(&api.name).unwrap().len(), 1);
        assert!(harness.project_workflows(&docs.name).unwrap().is_empty());
        assert!(harness.project_workflow(&docs.name, "release").is_err());
        // The same name in the other project is a file of its own.
        save(&app, &docs.name, &release("Build docs."));
        assert_eq!(
            harness
                .project_workflow(&api.name, "release")
                .unwrap()
                .description,
            "Build the api."
        );
        assert_eq!(
            harness
                .project_workflow(&docs.name, "release")
                .unwrap()
                .description,
            "Build docs."
        );
        // Deleting one leaves the other.
        harness
            .delete_project_workflow(&app, &Actor::first_owner(), &api.name, "release")
            .unwrap();
        assert!(harness.project_workflow(&api.name, "release").is_err());
        assert!(harness.project_workflow(&docs.name, "release").is_ok());
        assert!(matches!(
            harness.delete_project_workflow(&app, &Actor::first_owner(), &api.name, "release"),
            Err(AppError::NotFound(_))
        ));
        // A project the server does not have owns nothing.
        assert!(matches!(
            harness.save_project_workflow(
                &app,
                &Actor::first_owner(),
                "Nobody",
                "release",
                &release("x")
            ),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            harness.save_project_workflow(
                &app,
                &Actor::first_owner(),
                &docs.name,
                "../release",
                &release("x")
            ),
            Err(AppError::Invalid(_))
        ));
    }

    #[test]
    fn renaming_a_project_moves_its_workflows() {
        let (dir, app) = app();
        let [_, docs, ..] = projects(&app, dir.path());
        save(&app, &docs.name, &release("Build docs."));
        app.rename_project(&Actor::first_owner(), docs.id, "Handbook", None)
            .unwrap();
        let harness = app.harness_repo().unwrap();
        assert!(harness.project_workflow("docs", "release").is_err());
        assert_eq!(
            harness
                .project_workflow("Handbook", "release")
                .unwrap()
                .description,
            "Build docs."
        );
    }

    #[test]
    fn a_project_is_not_deleted_while_it_owns_workflows_or_active_runs() {
        let (dir, app) = app();
        let [_, docs, contracts, _] = projects(&app, dir.path());
        app.check_project_deletable(docs.id).unwrap();
        save(&app, &docs.name, &release("Build docs."));
        let Err(AppError::Conflict(message)) = app.check_project_deletable(docs.id) else {
            panic!("a project with a workflow stays");
        };
        assert!(message.contains("owns workflows"), "{message}");

        // An active run keeps the project, whether or not its file is still there.
        app.store
            .add_project_workflow_run(contracts.id, "release", 1, "test", &json!({}))
            .unwrap();
        let Err(AppError::Conflict(message)) = app.check_project_deletable(contracts.id) else {
            panic!("a project with an active run stays");
        };
        assert!(message.contains("queued, running or waiting"), "{message}");
        // The delete of the workflow is refused for the same run.
        let harness = app.harness_repo().unwrap();
        save(&app, &contracts.name, &release("Check contracts."));
        assert!(matches!(
            harness.delete_project_workflow(
                &app,
                &Actor::first_owner(),
                &contracts.name,
                "release"
            ),
            Err(AppError::Conflict(_))
        ));
    }

    #[test]
    fn finished_runs_keep_the_project_as_a_plain_value() {
        let (dir, app) = app();
        let [_, docs, ..] = projects(&app, dir.path());
        let run = app
            .store
            .add_project_workflow_run(docs.id, "release", docs.repositories[0], "test", &json!({}))
            .unwrap();
        app.store
            .finish_workflow_run(run.id, "succeeded", None)
            .unwrap();
        // Finished, the run no longer holds the project, and the run keeps its project id.
        app.check_project_deletable(docs.id).unwrap();
        assert_eq!(
            app.store.workflow_run(run.id).unwrap().unwrap().project_id,
            Some(docs.id)
        );
    }
}
