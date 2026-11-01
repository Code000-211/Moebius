//! What the tests of a run's lifecycle share: a server with one repository and both harnesses, each on the fake of
//! its own process, ways to wait for what a run does, and reading what it was told.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use moebius_plugin_api::Host;

use crate::app::App;
use crate::board::CardRuns;
use crate::config::AgentConfig;
use crate::harness::pibolt::PiBolt;
use crate::harness::pick::Harnesses;
use crate::repos::git;
use crate::router::{Protocol, Router};
use crate::store::{Run, Store};

/// A server with one repository and both harnesses, on the agents `Coder` and `Reviewer`.
pub(super) struct Setup {
    pub(super) _dir: tempfile::TempDir,
    pub(super) app: Arc<App>,
    pub(super) repo: i64,
    pub(super) harnesses: Harnesses,
}

impl Setup {
    /// The two harnesses, each as the board sees it, with the name of the process it runs.
    pub(super) fn both(&self) -> [(&'static str, &dyn CardRuns); 2] {
        [
            ("pi-bolt", self.harnesses.pi_bolt.as_ref()),
            ("claude-code", self.harnesses.claude_code.as_ref()),
        ]
    }
}

/// A server with both harnesses on the fake `pi-bolt` and the fake `claude` of `testdata`.
pub(super) fn setup() -> Setup {
    let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
    setup_with(
        &testdata.join("fake-pi"),
        &testdata.join("fake-claude"),
        false,
    )
}

/// As `setup`, with the baseline decider of `localize` answering the hints of code runs.
pub(super) fn setup_localizing() -> Setup {
    let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
    setup_with(
        &testdata.join("fake-pi"),
        &testdata.join("fake-claude"),
        true,
    )
}

/// As `setup`, with the harnesses started as `pi` and `claude`.
pub(super) fn setup_with(pi: &Path, claude: &Path, localizing: bool) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "hello\n").unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
    let mut host = Host::new(Duration::from_secs(1));
    if localizing {
        host.add(crate::decide::localize::Localizer::new(store.clone()));
    }
    let app = App::new(store.clone(), Arc::new(host), &dir.path().join("data"));
    if localizing {
        app.decisions
            .route(crate::decide::localize::KIND, crate::decide::localize::KIND);
    }
    let app = Arc::new(app);
    let repo = app
        .add_repository("demo", origin.to_str().unwrap())
        .unwrap();
    // Claude Code speaks only the anthropic protocol; nothing else about the agent matters to a fake.
    let agent = AgentConfig {
        model: "qwen-local".into(),
        protocol: Protocol::Anthropic,
        thinking: Some("low".into()),
        instructions: None,
        skills: vec![],
        extensions: vec![],
        mcp: BTreeMap::new(),
        token_budget: None,
        harness: Default::default(),
        tool_output: Default::default(),
        engineering_skills: true,
        runner: None,
    };
    let pi_bolt = PiBolt::new(
        pi.to_path_buf(),
        "/usr/local/bin/moebius".into(),
        "http://127.0.0.1:9".into(),
        Arc::new(Router::new(store, vec![])),
        BTreeMap::from([
            ("Coder".to_string(), agent.clone()),
            ("Reviewer".to_string(), agent),
        ]),
    );
    let harnesses = Harnesses::new(pi_bolt, claude);
    Setup {
        _dir: dir,
        app,
        repo: repo.id,
        harnesses,
    }
}

/// Waits for the run to have recorded `count` events, so that its process has taken the prompt.
pub(super) fn wait_for_events(app: &App, run_id: i64, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.store.events(run_id).unwrap().len() < count {
        assert!(
            Instant::now() < deadline,
            "run {run_id} recorded too few events"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits for the run to end.
pub(super) fn wait_until_ended(app: &App, run_id: i64) -> Run {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let run = app.store.run(run_id).unwrap().unwrap();
        if run.ended_at.is_some() {
            return run;
        }
        assert!(Instant::now() < deadline, "the run did not end: {run:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The prompt the fake of the run's harness was given, wherever that harness keeps it.
pub(super) fn prompt_of(app: &App, run: &Run) -> String {
    let dirs = [
        app.data_dir.join(format!("runs/{}/agent", run.id)),
        app.data_dir.join(format!("runs/{}/claude", run.id)),
    ];
    dirs.iter()
        .find_map(|dir| std::fs::read_to_string(dir.join("prompt.txt")).ok())
        .unwrap_or_else(|| panic!("the fake of run {} was given no prompt", run.id))
}
