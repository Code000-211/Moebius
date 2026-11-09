//! The examples in `examples/` work: the column script runs as a project's override, the rules parse, and the
//! workflow examples validate and run against a fake harness and a fake HTTP server.

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::http::{Method, Uri};
use axum::{Json, Router};
use moebius_core::app::{App, PERSON};
use moebius_core::board::{CardRuns, Engine, Kanban, Limits, StartError, Variant};
use moebius_core::decide::rules;
#[cfg(unix)] // Only the scheduled-agent example runs an agent node.
use moebius_core::harness::pick::Harnesses;
use moebius_core::repos::git;
use moebius_core::store::{Card, Run, Store};
#[cfg(unix)] // Only the scheduled-agent example runs an agent node.
use moebius_core::workflows::decorate::Scope;
#[cfg(unix)] // Only the scheduled-agent example runs an agent node.
use moebius_core::workflows::definition::NodeDefinition;
use moebius_core::workflows::definition::WorkflowDefinition;
#[cfg(unix)] // Only the scheduled-agent example runs an agent node.
use moebius_core::workflows::nodes::{
    self, NodeError, Routed,
    agent::{self, Agent},
};
#[cfg(unix)] // Only the scheduled-agent example runs an agent node.
use moebius_core::workflows::run::{Limits as WorkflowLimits, Runner, execute};
use moebius_plugin_api::Host;
use serde_json::{Value, json};

fn examples_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn example(path: &str) -> String {
    let file = examples_root().join(path);
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

/// Starts runs that never end.
struct IdleRuns;

impl CardRuns for IdleRuns {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: (&str, Option<&str>),
        _: Variant,
    ) -> Result<Run, StartError> {
        app.store
            .add_run(Some(card.id), agent, "idle", purpose.0)
            .map_err(|e| StartError::Store(e.to_string()))
    }

    fn has_agent(&self, _: &str) -> bool {
        false
    }

    fn runners(&self) -> Vec<moebius_core::harness::remote::RunnerInfo> {
        Vec::new()
    }

    fn agents(&self) -> Vec<moebius_core::board::AgentInfo> {
        Vec::new()
    }

    fn token_budget(&self, _: &str) -> Option<i64> {
        None
    }

    fn steer(&self, _: i64, _: &str) -> bool {
        false
    }

    fn stop(&self, _: i64) -> bool {
        false
    }
}

/// A project whose settings carry `script` as `ready.luau`, and an engine over it.
fn engine_with_ready_script(script: &str) -> (tempfile::TempDir, Arc<App>, Engine, i64) {
    let dir = tempfile::tempdir().unwrap();
    let origin: PathBuf = dir.path().join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "hello\n").unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
    let host = Arc::new(Host::new(Duration::from_secs(1)));
    let app = Arc::new(App::new(store, host, &dir.path().join("data")));
    let harness = common::use_harness(&app, dir.path());
    common::put_project(&harness, "demo", "ready.luau", script);
    let repository = app
        .add_repository("demo", origin.to_str().unwrap())
        .unwrap();
    let limits = Limits {
        run_limit: 3,
        ..Limits::default()
    };
    let engine = Engine::new(app.clone(), Arc::new(IdleRuns), Kanban::default(), limits);
    (dir, app, engine, repository.id)
}

/// A new card, moved to Not started by hand.
fn ready_card(app: &App, repository: i64, title: &str) -> Card {
    let card = app.create_issue(repository, title, "", PERSON).unwrap();
    app.move_card(card.id, "Not started").unwrap()
}

#[test]
fn the_large_cards_script_gives_a_large_card_the_machine_to_itself() {
    let (_dir, app, engine, repository) =
        engine_with_ready_script(&example("scripts/ready-large-cards-alone.luau"));
    let column = |card: &Card| app.card(card.id).unwrap().column;

    // An ordinary card starts while fewer than run_limit (3) runs are going.
    let busy = app.create_issue(repository, "Busy", "", PERSON).unwrap();
    app.store
        .add_run(Some(busy.id), "Coder", "idle", "code")
        .unwrap();
    let ordinary = ready_card(&app, repository, "A small fix");
    let step = engine.tick(ordinary.id).unwrap();
    assert_eq!(column(&ordinary), "In progress");
    assert_eq!(step.moved.as_deref(), Some("In progress"));

    // A large card waits while any run is going, and says why.
    let large = ready_card(&app, repository, "[large] Rewrite the parser");
    let step = engine.tick(large.id).unwrap();
    assert_eq!(column(&large), "Not started");
    assert_eq!(
        step.waiting.as_deref(),
        Some("waiting for a free slot (limit 1)")
    );

    // It starts once nothing runs.
    for run in app.store.active_runs().unwrap() {
        app.store.set_run_status(run.id, "succeeded").unwrap();
    }
    let ended = app.store.active_runs().unwrap();
    assert!(ended.is_empty(), "set_run_status ends the run: {ended:?}");
    engine.tick(large.id).unwrap();
    assert_eq!(column(&large), "In progress");
}

#[test]
fn the_large_cards_script_still_waits_for_blockers() {
    let (_dir, app, engine, repository) =
        engine_with_ready_script(&example("scripts/ready-large-cards-alone.luau"));
    let blocker = app.create_issue(repository, "First", "", PERSON).unwrap();
    let card = ready_card(&app, repository, "Second");
    app.store
        .set_dependencies(card.id, &[blocker.number].into())
        .unwrap();
    let step = engine.tick(card.id).unwrap();
    assert_eq!(
        step.waiting,
        Some(format!("waiting for #{}", blocker.number))
    );
    assert_eq!(app.card(card.id).unwrap().column, "Not started");
}

#[test]
fn the_example_rules_are_three_named_rules() {
    let names: Vec<String> = rules::parse(&example("rules.md"))
        .into_iter()
        .map(|rule| rule.name)
        .collect();
    assert_eq!(
        names,
        ["no-new-dependencies", "tests-with-code", "leave-ci-alone"]
    );
}

#[test]
fn the_workflow_examples_validate() {
    let dir = examples_root().join("workflows");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .collect();
    files.sort();
    assert!(files.len() >= 3, "only {} workflow examples", files.len());
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy();
        let text = std::fs::read_to_string(&path).unwrap();
        let definition = WorkflowDefinition::parse(&text, &format!("workflows/{name}"))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        definition
            .validate(&format!("workflows/{name}"))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

fn workflow_example(name: &str) -> WorkflowDefinition {
    let text = std::fs::read_to_string(examples_root().join("workflows").join(name)).unwrap();
    WorkflowDefinition::parse(&text, &format!("workflows/{name}")).unwrap()
}

fn wait_for_status(
    app: &App,
    events: &mut tokio::sync::broadcast::Receiver<moebius_core::bus::AppEvent>,
    id: i64,
    status: &str,
) -> moebius_core::store::WorkflowRun {
    loop {
        match events.blocking_recv() {
            Ok(event)
                if event.kind == "workflow_run_status"
                    && event.data["run"] == id
                    && event.data["status"] == status =>
            {
                return app.store.workflow_run(id).unwrap().unwrap();
            }
            Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(error) => panic!("the bus closed before the run was {status}: {error}"),
        }
    }
}

fn wait_for_end(
    app: &App,
    events: &mut tokio::sync::broadcast::Receiver<moebius_core::bus::AppEvent>,
    id: i64,
) -> moebius_core::store::WorkflowRun {
    loop {
        match events.blocking_recv() {
            Ok(event) if event.kind == "workflow_run_ended" && event.data["run"] == id => {
                return app.store.workflow_run(id).unwrap().unwrap();
            }
            Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(error) => panic!("the bus closed before the run ended: {error}"),
        }
    }
}

/// A repository for workflow runs and the app that drives them.
struct WorkflowWorld {
    dir: tempfile::TempDir,
    app: Arc<App>,
    repository: moebius_core::store::Repository,
}

impl WorkflowWorld {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        moebius_core::repos::git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        moebius_core::repos::git(&origin, &["add", "."]).unwrap();
        moebius_core::repos::git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let store =
            Arc::new(moebius_core::store::Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = Arc::new(App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(5))),
            &dir.path().join("data"),
        ));
        let repository = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        WorkflowWorld {
            dir,
            app,
            repository,
        }
    }

    /// Lets the HTTP node reach the fake server on this machine.
    fn allow_local_http(&self) {
        let config = moebius_core::config::Config::parse(
            "[workflows]\nhttp_hosts = [\"127.0.0.1\"]\n",
            self.dir.path(),
            self.dir.path(),
        )
        .unwrap();
        moebius_core::workflows::nodes::http::configure(&config);
    }
}

/// A local HTTP server on port 0 that records every request and answers with a JSON object.
struct FakeHttp {
    base: String,
    requests: Arc<Mutex<Vec<(String, Value)>>>,
}

impl FakeHttp {
    fn start() -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let recorded = Arc::clone(&requests);
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let app = Router::new().fallback(move |method: Method, uri: Uri, body: Bytes| {
                    let recorded = Arc::clone(&recorded);
                    async move {
                        let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
                        recorded
                            .lock()
                            .unwrap()
                            .push((format!("{method} {}", uri.path()), body));
                        Json(json!({"ok": true}))
                    }
                });
                axum::serve(listener, app).await.unwrap();
            });
        });
        FakeHttp { base, requests }
    }

    fn requests(&self) -> Vec<(String, Value)> {
        self.requests.lock().unwrap().clone()
    }
}

#[test]
fn the_manual_example_shapes_a_value_and_follows_the_condition() {
    let server = FakeHttp::start();
    let world = WorkflowWorld::new();
    world.allow_local_http();
    let definition = workflow_example("release-preview.toml");
    let mut events = world.app.bus.subscribe();

    for channel in ["stable", "preview"] {
        let started = world
            .app
            .start_workflow_run(
                definition.clone(),
                world.repository.id,
                json!({
                    "release": "1.4.0",
                    "channel": channel,
                    "api_base": server.base,
                }),
            )
            .unwrap();
        let run = wait_for_end(&world.app, &mut events, started.id);
        assert_eq!(run.status, "succeeded", "{:?}", run.error);
    }

    let requests = server.requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert!(
        requests
            .iter()
            .any(|(path, _)| path == "POST /announce/stable")
    );
    assert!(
        requests
            .iter()
            .any(|(path, _)| path == "POST /announce/preview")
    );
    for (_, body) in requests {
        assert_eq!(body["version"], "1.4.0");
        assert!(body["message"].as_str().unwrap().contains("Release 1.4.0"));
    }
}

#[test]
fn the_approved_example_waits_for_a_person_before_it_publishes() {
    use moebius_core::app::workflows::approval::DecisionRequest;

    let server = FakeHttp::start();
    let world = WorkflowWorld::new();
    world.allow_local_http();
    let definition = workflow_example("approved-publish.toml");
    let mut events = world.app.bus.subscribe();
    let run = world
        .app
        .start_workflow_run(
            definition,
            world.repository.id,
            json!({"version": "2.0.0", "api_base": server.base}),
        )
        .unwrap();

    let waiting = wait_for_status(&world.app, &mut events, run.id, "waiting");
    assert_eq!(waiting.status, "waiting");
    assert!(
        server.requests().is_empty(),
        "nothing is published while the run waits"
    );
    let item = world
        .app
        .store
        .inbox_for_workflow_run(run.id)
        .unwrap()
        .expect("the wait asks a person");
    assert_eq!(
        item.data["value"]["announcement"],
        "Version 2.0.0 is ready to publish."
    );

    world
        .app
        .resume_workflow_run(
            run.id,
            &DecisionRequest {
                decision: "approve".into(),
                value: None,
                reason: None,
            },
        )
        .unwrap();
    let ended = wait_for_end(&world.app, &mut events, run.id);
    assert_eq!(ended.status, "succeeded", "{:?}", ended.error);

    let requests = server.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].0, "POST /publish");
    assert_eq!(requests[0].1["version"], "2.0.0");
}

/// Runs the agent node through the executor, since the app's own node runner does not yet know that kind.
#[cfg(unix)] // Only the scheduled-agent example runs an agent node.
struct AgentRunner {
    app: Arc<App>,
    runs: Harnesses,
    run_id: i64,
}

#[cfg(unix)] // Only the scheduled-agent example runs an agent node.
impl Runner for AgentRunner {
    fn run(&self, node: &NodeDefinition, input: &Value) -> Result<Value, NodeError> {
        nodes::node(&node.kind, &node.params)?.run(input.clone())
    }

    fn run_routed(&self, node: &NodeDefinition, scope: &Scope) -> Result<Routed, NodeError> {
        if node.kind == agent::KIND {
            let agent = Agent::from_params(&node.params)?;
            let value =
                agent.run_node(&self.app, &self.runs, &node.id, self.run_id, &scope.input)?;
            let port = node
                .params
                .get("output")
                .and_then(Value::as_str)
                .map(str::to_string);
            return Ok(Routed { value, port });
        }
        nodes::node(&node.kind, &node.params)?.run_routed(scope)
    }
}

#[cfg(unix)] // The fake harness is a Python script started by its `#!` line.
fn agent_world() -> (
    tempfile::TempDir,
    Arc<App>,
    Harnesses,
    moebius_core::store::Repository,
) {
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;

    use moebius_core::config::{AgentConfig, HarnessKind};
    use moebius_core::harness::pibolt::PiBolt;
    use moebius_core::router::{Protocol, Router};

    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    moebius_core::repos::git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "hello\n").unwrap();
    moebius_core::repos::git(&origin, &["add", "."]).unwrap();
    moebius_core::repos::git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    let store = Arc::new(moebius_core::store::Store::in_memory(&dir.path().join("blobs")).unwrap());
    let app = Arc::new(App::new(
        store.clone(),
        Arc::new(Host::new(Duration::from_secs(5))),
        &dir.path().join("data"),
    ));
    let repository = app
        .add_repository("demo", origin.to_str().unwrap())
        .unwrap();

    let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
    let pi = dir.path().join("fake-pi");
    std::fs::copy(testdata.join("fake-pi"), &pi).unwrap();
    std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o755)).unwrap();
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
    let picker = PiBolt::new(
        pi,
        "/usr/local/bin/moebius".into(),
        "http://127.0.0.1:9".into(),
        Arc::new(Router::new(store, vec![])),
        BTreeMap::from([("Coder".to_string(), coder)]),
    );
    let runs = Harnesses::new(picker, &testdata.join("fake-claude"));
    (dir, app, runs, repository)
}

#[cfg(unix)] // The fake harness is a Python script started by its `#!` line.
#[test]
fn the_scheduled_agent_example_runs_the_agent_and_posts_its_report() {
    let server = FakeHttp::start();
    let (dir, app, runs, repository) = agent_world();
    let config = moebius_core::config::Config::parse(
        "[workflows]\nhttp_hosts = [\"127.0.0.1\"]\n",
        dir.path(),
        dir.path(),
    )
    .unwrap();
    moebius_core::workflows::nodes::http::configure(&config);

    let definition = workflow_example("nightly-triage.toml");
    let mut inputs = definition.trigger[0].params["inputs"].clone();
    inputs["api_base"] = json!(server.base);
    let value = json!({
        "fired": "2026-01-01T03:30:00Z",
        "inputs": inputs,
    });
    let run = app
        .store
        .add_workflow_run(&definition.name, repository.id, "schedule", &value)
        .unwrap();
    let runner: Arc<dyn Runner> = Arc::new(AgentRunner {
        app: Arc::clone(&app),
        runs,
        run_id: run.id,
    });
    let mut observer = ();
    let result = execute(
        &definition,
        value,
        WorkflowLimits::from_definition(&definition),
        &runner,
        &mut observer,
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert!(result.is_ok(), "{result:?}");

    let requests = server.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].0, "POST /reports");
    assert_eq!(requests[0].1["status"], "ready");
    assert_eq!(
        requests[0].1["goal"],
        "Review the board and report what needs attention."
    );
}
