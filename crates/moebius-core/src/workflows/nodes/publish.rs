//! The publish node: offers the branch of a workflow run as its change. The branch is the one the run's agent node
//! works on, so a run with no agent node has nothing to publish and the node fails closed; so does a branch with
//! no commit beyond the base. A published change goes through the same offer as a card's change, so its review,
//! checks and merge work the same way, and it is recorded with the run: the inbox names the run, and merging or
//! rejecting it is the run's outcome. The node's value is the change's id and kind.

use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};

use super::{Metadata, Node, NodeError, Ports};
use crate::app::App;
use crate::repos::git;
use crate::workflows::decorate::{Scope, fill};

/// The kind of the node.
pub const KIND: &str = "publish";

/// The port the change leaves by.
const OUT: &str = "out";

/// The publish node: `title` and `body` are built from parameters and expressions, and the run's branch is offered
/// as its change through the repository's forge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publish {
    title: String,
    body: String,
}

impl Publish {
    /// The node from its parameters: `title` and `body`, each a non-empty text of its own.
    pub fn from_params(params: &Value) -> Result<Self, NodeError> {
        let invalid = |message: &str| NodeError::Parameters {
            kind: KIND.to_string(),
            message: message.to_string(),
        };
        let parameter = |name: &str| {
            params
                .get(name)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
                .ok_or_else(|| invalid(&format!("`{name}` must be a non-empty string")))
        };
        Ok(Self {
            title: parameter("title")?,
            body: parameter("body")?,
        })
    }

    /// Runs the node for the workflow run `run_id`, with `input` reaching it, and returns the change's id and kind.
    /// The run must have a worktree on its branch, made by an agent node, with a commit beyond the base; otherwise
    /// the node fails closed rather than offer nothing. `node_id` names the node in the errors of its expressions.
    pub fn run_node(
        &self,
        app: &Arc<App>,
        node_id: &str,
        run_id: i64,
        input: &Value,
    ) -> Result<Value, NodeError> {
        let failed = |why: String| NodeError::Failed(format!("node {node_id}: {why}"));
        let run = app
            .store
            .workflow_run(run_id)
            .map_err(|e| failed(e.to_string()))?
            .ok_or_else(|| failed(format!("the workflow run {run_id} is gone")))?;
        let repository = app
            .store
            .repository(run.repository_id)
            .map_err(|e| failed(e.to_string()))?
            .ok_or_else(|| {
                failed(format!(
                    "the run's repository {} is gone",
                    run.repository_id
                ))
            })?;
        match &run.worktree {
            Some(path) if Path::new(path).exists() => {}
            Some(path) => return Err(failed(format!("the run's worktree {path} is gone"))),
            None => {
                return Err(failed(
                    "the run has no agent node: there is nothing to publish".to_string(),
                ));
            }
        }
        let branch = format!("moebius/workflow-run-{run_id}");
        let beyond = git(
            Path::new(&repository.path),
            &[
                "rev-list",
                "--count",
                &format!("{}..{branch}", repository.base_ref()),
            ],
        )
        .map_err(|_| failed(format!("the run's branch {branch} has no commits")))?;
        if beyond.trim() == "0" {
            return Err(failed(format!(
                "the run's branch {branch} has nothing committed"
            )));
        }
        let scope = Scope {
            trigger: run.value.clone(),
            input: input.clone(),
            nodes: Default::default(),
        };
        let filled = fill(
            node_id,
            &json!({ "title": self.title.clone(), "body": self.body.clone() }),
            &scope,
        )
        .map_err(|e| failed(e.to_string()))?;
        let text = |name: &str| {
            filled
                .get(name)
                .and_then(Value::as_str)
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| failed(format!("`{name}` is empty after its expressions")))
        };
        let title = text("title")?;
        let body = text("body")?;
        let change = app
            .offer_run_branch_change(&repository, run_id, &branch, &title, &body)
            .map_err(|e| failed(e.to_string()))?;
        Ok(json!({ "id": change.id, "kind": "change" }))
    }
}

impl Node for Publish {
    /// Fails closed: offering a branch is the workflow run's business, and the plain executor has no run; see
    /// [`Publish::run_node`].
    fn run(&self, _input: Value) -> Result<Value, NodeError> {
        Err(NodeError::Failed(
            "publish needs an executor that gives it the workflow run".to_string(),
        ))
    }

    fn ports(&self) -> Ports {
        Ports::new(["in"], [OUT])
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            kind: KIND,
            title: "Publish",
            description: "Offers the run's branch as a change for review, checks and merge.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> Value {
        json!({ "title": "Release ${trigger.version}", "body": "Offered by the run." })
    }

    #[test]
    fn a_parameter_that_is_not_a_non_empty_string_is_refused() {
        for missing in ["title", "body"] {
            let mut params = params();
            params.as_object_mut().unwrap().remove(missing);
            let error = Publish::from_params(&params).unwrap_err();
            assert!(
                matches!(&error, NodeError::Parameters { kind, message }
                    if kind == KIND && message.contains(missing)),
                "{error}"
            );
            let mut empty = params.clone();
            empty[missing] = json!("");
            assert!(
                Publish::from_params(&empty).is_err(),
                "{missing} taken while empty"
            );
        }
    }

    #[test]
    fn the_registry_builds_the_publish_node() {
        let built = super::super::node(KIND, &params());
        assert!(built.is_ok());
        let ports = built.unwrap().ports();
        assert_eq!(ports.inputs, ["in"]);
        assert_eq!(ports.outputs, [OUT]);
        assert!(matches!(
            super::super::node(KIND, &json!({})),
            Err(NodeError::Parameters { .. })
        ));
    }

    #[test]
    fn the_node_is_run_for_a_workflow_run_only() {
        let publish = Publish::from_params(&params()).unwrap();
        let error = publish.run(json!(null)).unwrap_err();
        assert!(matches!(error, NodeError::Failed(_)), "{error}");
    }
}

/// The node against a real local repository and a workflow run whose agent node left a worktree with a commit.
#[cfg(test)]
mod a_run {
    use std::time::Duration;

    use moebius_plugin_api::Host;
    use serde_json::json;

    use super::*;
    use crate::repos::git;
    use crate::store::{Repository, Store};

    /// A world for the node: one repository a workflow run belongs to.
    struct World {
        _dir: tempfile::TempDir,
        app: Arc<App>,
        repository: Repository,
    }

    impl World {
        fn new() -> World {
            let dir = tempfile::tempdir().unwrap();
            let origin = dir.path().join("origin");
            std::fs::create_dir_all(&origin).unwrap();
            git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
            std::fs::write(origin.join("README.md"), "hello\n").unwrap();
            git(&origin, &["add", "."]).unwrap();
            git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
            let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
            let app = Arc::new(App::new(
                store,
                Arc::new(Host::new(Duration::from_secs(1))),
                &dir.path().join("data"),
            ));
            let repository = app
                .add_repository("demo", origin.to_str().unwrap())
                .unwrap();
            World {
                _dir: dir,
                app,
                repository,
            }
        }

        /// A workflow run of the world's repository with the value its trigger gave it.
        fn workflow_run(&self, value: Value) -> i64 {
            self.app
                .store
                .add_workflow_run("release", self.repository.id, "manual", &value)
                .unwrap()
                .id
        }

        /// The worktree the run keeps, made by its agent node, with one commit on the run's branch.
        fn worktree_with_a_commit(&self, run_id: i64) -> std::path::PathBuf {
            let worktree = self
                .app
                .repos
                .add_workflow_worktree(
                    &self.repository.name,
                    Path::new(&self.repository.path),
                    run_id,
                    &self.repository.base_ref(),
                )
                .unwrap()
                .path;
            self.app
                .store
                .set_workflow_run_worktree(run_id, &worktree)
                .unwrap();
            std::fs::write(worktree.join("note.txt"), "work\n").unwrap();
            git(&worktree, &["add", "."]).unwrap();
            git(
                &worktree,
                &[
                    "-c",
                    "user.email=test@example.com",
                    "-c",
                    "user.name=Test",
                    "commit",
                    "--quiet",
                    "-m",
                    "work",
                ],
            )
            .unwrap();
            worktree
        }

        fn publish(&self) -> Publish {
            Publish::from_params(&json!({
                "title": "Release {{ trigger.release }}",
                "body": "The run offers its work.",
            }))
            .unwrap()
        }

        fn branch(run_id: i64) -> String {
            format!("moebius/workflow-run-{run_id}")
        }
    }

    #[test]
    fn the_node_offers_the_runs_branch_with_the_run_and_the_repository() {
        let world = World::new();
        let run_id = world.workflow_run(json!({ "release": "1.0" }));
        world.worktree_with_a_commit(run_id);
        let value = world
            .publish()
            .run_node(&world.app, "publish", run_id, &json!(null))
            .unwrap();
        let change = world
            .app
            .store
            .change(value["id"].as_i64().unwrap())
            .unwrap()
            .expect("the change is stored");
        assert_eq!(value["kind"], "change");
        assert_eq!(change.run_id, Some(run_id));
        assert_eq!(change.repository_id, Some(world.repository.id));
        assert_eq!(change.branch, World::branch(run_id));
        assert_eq!(change.card_id, None);
        let items = world.app.store.inbox().unwrap();
        let item = items
            .iter()
            .find(|item| item.kind == "ready_to_merge")
            .expect("the change waits in the inbox");
        assert_eq!(item.title, "Release 1.0");
        assert_eq!(item.data["change"], change.id);
        assert_eq!(item.data["branch"], World::branch(run_id));
        assert_eq!(item.data["run"], run_id);
    }

    #[test]
    fn merging_the_runs_change_records_the_merge_on_the_run() {
        let world = World::new();
        let run_id = world.workflow_run(json!({}));
        world.worktree_with_a_commit(run_id);
        let value = world
            .publish()
            .run_node(&world.app, "publish", run_id, &json!(null))
            .unwrap();
        let change = value["id"].as_i64().unwrap();
        world
            .app
            .merge_change(&crate::decide::Actor::Person, change, "fine")
            .unwrap();
        let changes = world.app.store.changes_for_run(run_id).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].state, "merged");
        assert!(world.app.store.inbox().unwrap().is_empty());
    }

    #[test]
    fn rejecting_the_runs_change_records_the_rejection_on_the_run() {
        let world = World::new();
        let run_id = world.workflow_run(json!({}));
        world.worktree_with_a_commit(run_id);
        let value = world
            .publish()
            .run_node(&world.app, "publish", run_id, &json!(null))
            .unwrap();
        let change = value["id"].as_i64().unwrap();
        let card = world
            .app
            .reject_change(&crate::decide::Actor::Person, change, "not this")
            .unwrap();
        assert!(card.is_none(), "a run's change has no card");
        let changes = world.app.store.changes_for_run(run_id).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].state, "closed");
        assert!(world.app.store.inbox().unwrap().is_empty());
    }

    #[test]
    fn a_run_with_no_agent_node_fails_closed() {
        let world = World::new();
        let run_id = world.workflow_run(json!({}));
        let error = world
            .publish()
            .run_node(&world.app, "publish", run_id, &json!(null))
            .unwrap_err();
        assert!(
            matches!(&error, NodeError::Failed(message) if message.contains("no agent node")),
            "{error}"
        );
    }

    #[test]
    fn a_run_with_nothing_committed_fails_closed() {
        let world = World::new();
        let run_id = world.workflow_run(json!({}));
        let worktree = world
            .app
            .repos
            .add_workflow_worktree(
                &world.repository.name,
                Path::new(&world.repository.path),
                run_id,
                &world.repository.base_ref(),
            )
            .unwrap();
        world
            .app
            .store
            .set_workflow_run_worktree(run_id, &worktree.path)
            .unwrap();
        let error = world
            .publish()
            .run_node(&world.app, "publish", run_id, &json!(null))
            .unwrap_err();
        assert!(
            matches!(&error, NodeError::Failed(message) if message.contains("nothing committed")),
            "{error}"
        );
    }
}
