//! Running the agent of an agent node: the workflow run's repository checked out into the worktree the run owns,
//! the prompt filled from the run and the agent started through the picker, failing closed, and its fenced block
//! taken as the node's value (see `value`). The tests below run the node against a real launcher and a fake
//! `pi-bolt`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};

use super::super::NodeError;
use super::value::value_of;
use super::{Agent, OUTPUT_REQUEST};
use crate::app::App;
use crate::harness::pick::Harnesses;
use crate::improve::retro::wait_for_end;
use crate::workflows::decorate::{Scope, fill};

impl Agent {
    /// Runs the agent of the node for the workflow run `run_id`, with `input` reaching the node, and returns the
    /// value of the node's declared output. The first agent node of a run checks the run's repository out into a
    /// worktree on a branch named for the run and records the worktree with the run; the later nodes reuse it. The
    /// agent is started through the picker `runs`, failing closed; `node_id` names the node in the errors of its
    /// expressions.
    pub fn run_node(
        &self,
        app: &Arc<App>,
        runs: &Harnesses,
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
        // The worktree of the run, on the branch named for it: what its first agent node made, recorded with it.
        let worktree = match &run.worktree {
            Some(path) if Path::new(path).exists() => PathBuf::from(path),
            Some(path) => return Err(failed(format!("the run's worktree {path} is gone"))),
            None => {
                let worktree = app
                    .repos
                    .add_workflow_worktree(
                        &repository.name,
                        Path::new(&repository.path),
                        run.id,
                        &repository.base_ref(),
                    )
                    .map_err(|e| {
                        failed(format!("cannot check the run out into its worktree: {e}"))
                    })?;
                app.store
                    .set_workflow_run_worktree(run.id, &worktree.path)
                    .map_err(|e| failed(e.to_string()))?;
                worktree.path
            }
        };
        // The prompt, from the parameters and the expressions of the run.
        let scope = Scope {
            trigger: run.value.clone(),
            input: input.clone(),
            nodes: BTreeMap::new(),
        };
        let filled = fill(node_id, &json!({ "prompt": self.prompt.clone() }), &scope)
            .map_err(|e| failed(e.to_string()))?;
        let prompt = match filled.get("prompt") {
            Some(Value::String(text)) => format!("{text}\n\n{OUTPUT_REQUEST}"),
            _ => return Err(failed("the filled prompt is not text".to_string())),
        };
        // The agent's run, through the picker: failing closed, its transcript in the run's log.
        let mut ended = app.bus.subscribe();
        let started = runs
            .start_workflow_run(app, &self.agent, &repository, &worktree, (run.id, &prompt))
            .map_err(|e| failed(format!("cannot start the agent: {e}")))?;
        wait_for_end(app, &mut ended, started.id);
        let done = app
            .store
            .run(started.id)
            .map_err(|e| failed(e.to_string()))?
            .ok_or_else(|| failed("the agent's run is gone".to_string()))?;
        if done.status != "succeeded" {
            return Err(failed(format!("the agent's run ended {}", done.status)));
        }
        let answer = done
            .summary
            .ok_or_else(|| failed("the agent ended without a final answer".to_string()))?;
        value_of(&answer)
    }
}

/// The node against a real launcher and a `pi-bolt` that is a script. The fake is a Python script started by its
/// `#!` line, so these run on Unix only.
#[cfg(all(test, unix))]
mod a_run {
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use moebius_plugin_api::Host;
    use serde_json::json;

    use super::*;
    use crate::config::{AgentConfig, HarnessKind};
    use crate::harness::pibolt::PiBolt;
    use crate::repos::git;
    use crate::router::{Protocol, Router};
    use crate::store::{Repository, Run, Store};

    /// A world for the node: a repository its runs belong to, and a `Coder` the fake `pi-bolt` answers for.
    struct World {
        _dir: tempfile::TempDir,
        app: Arc<App>,
        runs: Harnesses,
        repository: Repository,
    }

    impl World {
        fn new() -> World {
            let dir = tempfile::tempdir().unwrap();
            let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
            World::with_pi(dir, testdata.join("fake-pi"))
        }

        /// A world whose harness is `script`, a fake `pi-bolt` the test writes itself.
        fn with_script(script: &str) -> World {
            let dir = tempfile::tempdir().unwrap();
            let pi = dir.path().join("fake-pi");
            std::fs::write(&pi, script).unwrap();
            std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o755)).unwrap();
            World::with_pi(dir, pi)
        }

        fn with_pi(dir: tempfile::TempDir, pi: PathBuf) -> World {
            let origin = dir.path().join("origin");
            std::fs::create_dir_all(&origin).unwrap();
            git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
            std::fs::write(origin.join("README.md"), "hello\n").unwrap();
            git(&origin, &["add", "."]).unwrap();
            git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
            let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
            let app = Arc::new(App::new(
                store.clone(),
                Arc::new(Host::new(Duration::from_secs(1))),
                &dir.path().join("data"),
            ));
            let repository = app
                .add_repository("demo", origin.to_str().unwrap())
                .unwrap();
            let coder = AgentConfig {
                model: "m".into(),
                protocol: Protocol::OpenAi,
                thinking: None,
                instructions: None,
                skills: vec![],
                extensions: vec![],
                mcp: BTreeMap::new(),
                token_budget: None,
                harness: HarnessKind::PiBolt,
                tool_output: Default::default(),
                engineering_skills: Default::default(),
                runner: None,
            };
            let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
            let pi = PiBolt::new(
                pi,
                "/usr/local/bin/moebius".into(),
                "http://127.0.0.1:9".into(),
                Arc::new(Router::new(store, vec![])),
                BTreeMap::from([("Coder".to_string(), coder)]),
            );
            let runs = Harnesses::new(pi, &testdata.join("fake-claude"));
            World {
                _dir: dir,
                app,
                runs,
                repository,
            }
        }

        /// A workflow run of the world's repository, started by a trigger that produced `value`.
        fn workflow_run(&self, value: Value) -> i64 {
            self.app
                .store
                .add_workflow_run("release", self.repository.id, "manual", &value)
                .unwrap()
                .id
        }

        /// The node that runs the world's `Coder` on `prompt` and calls its value `output`.
        fn agent_node(&self, prompt: &str, output: &str) -> Agent {
            Agent::from_params(&json!({
                "agent": "Coder",
                "prompt": prompt,
                "output": output,
            }))
            .unwrap()
        }

        /// The run's worktree the store keeps, which its first agent node made.
        fn worktree(&self, run_id: i64) -> PathBuf {
            let worktree = self
                .app
                .store
                .workflow_run(run_id)
                .unwrap()
                .expect("the run is stored")
                .worktree
                .expect("the run keeps its worktree");
            PathBuf::from(worktree)
        }

        /// The branch the run owns, named for the run.
        fn branch(run_id: i64) -> String {
            format!("moebius/workflow-run-{run_id}")
        }

        /// The file `name` as the run's branch holds it.
        fn on_branch(&self, run_id: i64, name: &str) -> String {
            let branch = Self::branch(run_id);
            git(
                Path::new(&self.repository.path),
                &["show", &format!("{branch}:{name}")],
            )
            .unwrap_or_else(|e| panic!("{name} is not on {branch}: {e}"))
        }

        /// The clone the run's branch lives in, beside the worktree the run owns.
        fn clone(&self) -> &Path {
            Path::new(&self.repository.path)
        }
    }

    /// The runs the store holds, oldest first: a world starts no run but its agents', and a fresh store numbers
    /// them from one.
    fn agent_runs(app: &App) -> Vec<Run> {
        (1..10)
            .filter_map(|id| app.store.run(id).unwrap())
            .collect()
    }

    #[test]
    fn the_first_agent_node_checks_the_run_out_and_yields_its_block() {
        let world = World::new();
        let run_id = world.workflow_run(json!({"release": "1.0"}));
        // The fake answers a prompt that is not a card by saying it back, so the block the prompt asks for comes
        // back in the answer; `[write note.txt]` leaves a file where the agent works.
        let node = world.agent_node(
            "Write the note. [write note.txt]\nAnswer with:\n```json\n{\"release\": \"{{ trigger.release }}\"}\n```",
            "note",
        );
        let value = node
            .run_node(&world.app, &world.runs, "draft", run_id, &json!(null))
            .unwrap_or_else(|e| panic!("{e}"));
        // The value is the block, with the run's expressions filled into the prompt the agent answers.
        assert_eq!(value, json!({"release": "1.0"}));
        // The work the agent left is on the branch the run owns.
        assert_eq!(world.on_branch(run_id, "note.txt"), "written");
        // The run keeps its worktree, where the agent worked.
        let worktree = world.worktree(run_id);
        assert!(
            worktree.join("note.txt").is_file(),
            "the worktree {worktree:?} was kept with the work in it"
        );
        // The agent ran once, as a run of the workflow whose transcript is kept in its log.
        let agents = agent_runs(&world.app);
        assert_eq!(agents.len(), 1, "one agent node, one run");
        assert_eq!(agents[0].purpose, "workflow");
        assert_eq!(agents[0].status, "succeeded");
        // The run names the branch its work is committed on, so a diff of it finds that work.
        assert_eq!(agents[0].branch(), World::branch(run_id));
        assert!(
            !world.app.store.events(agents[0].id).unwrap().is_empty(),
            "the agent's transcript is in its run's log"
        );
        // The prompt the agent got asks for the fenced JSON block the node parses.
        let prompt = std::fs::read_to_string(
            world
                .app
                .data_dir
                .join(format!("runs/{}/agent/prompt.txt", agents[0].id)),
        )
        .unwrap();
        assert!(
            prompt.ends_with(OUTPUT_REQUEST),
            "the prompt asks for the block: {prompt}"
        );
    }

    #[test]
    fn the_last_fenced_block_of_the_agents_answer_is_the_nodes_value() {
        let world = World::new();
        let run_id = world.workflow_run(json!({}));
        let node = world.agent_node(
            "A draft first:\n```json\n{\"draft\": true}\n```\nThen the answer:\n```json\n{\"done\": true}\n```",
            "note",
        );
        let value = node
            .run_node(&world.app, &world.runs, "draft", run_id, &json!(null))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(value, json!({"done": true}));
    }

    #[test]
    fn the_agent_nodes_of_a_run_share_its_worktree_and_branch() {
        let world = World::new();
        let run_id = world.workflow_run(json!({}));
        let first = world.agent_node("First. [write one.txt]\n```json\n{\"step\": 1}\n```", "one");
        let second = world.agent_node("Then. [write two.txt]\n```json\n{\"step\": 2}\n```", "two");
        assert_eq!(
            first
                .run_node(&world.app, &world.runs, "one", run_id, &json!(null))
                .unwrap_or_else(|e| panic!("{e}")),
            json!({"step": 1})
        );
        let worktree = world.worktree(run_id);
        assert_eq!(
            second
                .run_node(&world.app, &world.runs, "two", run_id, &json!(null))
                .unwrap_or_else(|e| panic!("{e}")),
            json!({"step": 2})
        );
        // The second node worked where the first one did, and left its work on the same branch.
        assert_eq!(world.worktree(run_id), worktree);
        assert_eq!(world.on_branch(run_id, "one.txt"), "written");
        assert_eq!(world.on_branch(run_id, "two.txt"), "written");
        // One branch for the run, and one worktree of the repository.
        let branches = git(world.clone(), &["branch", "--list", "moebius/*"]).unwrap();
        assert_eq!(branches.trim().lines().count(), 1, "{branches}");
        let worktrees = std::fs::read_dir(world.app.data_dir.join("worktrees").join("demo"))
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1, "one worktree for the run");
        assert_eq!(worktrees[0].as_ref().unwrap().path(), worktree);
    }

    #[test]
    fn an_agent_that_is_not_configured_starts_nothing() {
        let world = World::new();
        let run_id = world.workflow_run(json!({}));
        let node = Agent::from_params(&json!({
            "agent": "Nobody",
            "prompt": "```json\n{}\n```",
            "output": "note",
        }))
        .unwrap();
        let error = node
            .run_node(&world.app, &world.runs, "draft", run_id, &json!(null))
            .unwrap_err();
        assert!(matches!(error, NodeError::Failed(_)), "{error}");
        assert!(
            agent_runs(&world.app).is_empty(),
            "an agent that is not there"
        );
    }

    #[test]
    fn a_malformed_block_fails_the_node_after_the_agent_ended() {
        let world = World::new();
        let run_id = world.workflow_run(json!({}));
        let node = world.agent_node("Answer with:\n```json\n{\"release\": oops}\n```", "note");
        let error = node
            .run_node(&world.app, &world.runs, "draft", run_id, &json!(null))
            .unwrap_err();
        assert!(
            matches!(&error, NodeError::Failed(why) if why.contains("not JSON")),
            "{error}"
        );
        // The run ends as it is: the node has no value, and nothing else was changed.
        assert_eq!(agent_runs(&world.app)[0].status, "succeeded");
    }

    #[test]
    fn an_answer_without_a_block_fails_the_node_after_the_agent_ended() {
        let world = World::new();
        let run_id = world.workflow_run(json!({}));
        let node = world.agent_node("Say hello and nothing else.", "note");
        let error = node
            .run_node(&world.app, &world.runs, "draft", run_id, &json!(null))
            .unwrap_err();
        assert!(
            matches!(&error, NodeError::Failed(why) if why.contains("without a fenced JSON block")),
            "{error}"
        );
    }

    /// A fake `pi-bolt` that answers the prompt, then either leaves before the run settles or settles without an
    /// assistant's answer: a workflow agent has nothing to take from either.
    const FAILING_PI: &str = r#"#!/usr/bin/env python3
import json, sys
for line in sys.stdin.buffer:
    command = json.loads(line.split(b"\n")[0])
    if command.get("type") != "prompt":
        continue
    print(json.dumps({"id": command.get("id"), "type": "response", "command": "prompt", "success": True,
                      "data": {"disposition": "started"}}), flush=True)
    if "exit" in command.get("message", ""):
        sys.exit(1)
    print(json.dumps({"type": "agent_end", "messages": []}), flush=True)
    print(json.dumps({"type": "agent_settled", "aborted": False}), flush=True)
    sys.exit(0)
"#;

    #[test]
    fn an_agent_run_that_failed_fails_the_node_closed() {
        let world = World::with_script(FAILING_PI);
        let run_id = world.workflow_run(json!({}));
        let node = world.agent_node("Answer, then exit.", "note");
        let error = node
            .run_node(&world.app, &world.runs, "draft", run_id, &json!(null))
            .unwrap_err();
        assert!(
            matches!(&error, NodeError::Failed(why) if why.contains("ended failed")),
            "{error}"
        );
        assert_eq!(agent_runs(&world.app)[0].status, "failed");
    }

    #[test]
    fn an_agent_run_without_a_final_answer_fails_the_node_closed() {
        let world = World::with_script(FAILING_PI);
        let run_id = world.workflow_run(json!({}));
        let node = world.agent_node("Answer nothing.", "note");
        let error = node
            .run_node(&world.app, &world.runs, "draft", run_id, &json!(null))
            .unwrap_err();
        assert!(
            matches!(&error, NodeError::Failed(why) if why.contains("without a final answer")),
            "{error}"
        );
        assert_eq!(agent_runs(&world.app)[0].status, "succeeded");
    }
}
