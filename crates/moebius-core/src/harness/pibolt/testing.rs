//! What the tests of the Pi-Bolt harness share: a server with one repository and a fake `pi-bolt`, and ways to wait
//! for what its runs do.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use moebius_plugin_api::Host;
use serde_json::Value;

use super::PiBolt;
use crate::app::App;
use crate::config::AgentConfig;
use crate::repos::git;
use crate::router::{Protocol, Router};
use crate::store::{Run, Store};

pub(super) fn fake_pi() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi")
}

pub(super) fn setup() -> (tempfile::TempDir, Arc<App>, PiBolt, i64) {
    let dir = tempfile::tempdir().unwrap();
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
    let repo = app
        .add_repository("demo", origin.to_str().unwrap())
        .unwrap();
    let profile = AgentConfig {
        model: "qwen-local".into(),
        thinking: Some("low".into()),
        instructions: Some("Be brief.".into()),
        skills: vec![],
        extensions: vec!["/agent/own.ts".into()],
        mcp: BTreeMap::new(),
        protocol: Protocol::OpenAi,
        token_budget: Some(1000),
        harness: Default::default(),
        tool_output: Default::default(),
        engineering_skills: true,
        runner: None,
    };
    let pi = PiBolt::new(
        fake_pi(),
        "/usr/local/bin/moebius".into(),
        "http://127.0.0.1:9".into(),
        Arc::new(Router::new(store, vec![])),
        BTreeMap::from([("Coder".to_string(), profile)]),
    );
    (dir, app, pi, repo.id)
}

pub(super) fn wait_until_ended(app: &App, run_id: i64) -> Run {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let run = app.store.run(run_id).unwrap().unwrap();
        if run.ended_at.is_some() {
            return run;
        }
        assert!(
            Instant::now() < deadline,
            "the run did not end: {run:?} {:?} {:?}",
            std::fs::read_to_string(app.data_dir.join(format!("runs/{run_id}/stderr.log"))),
            app.store
                .events(run_id)
                .unwrap()
                .iter()
                .map(|e| &e.kind)
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits until `holds` holds of the run `run_id`.
pub(super) fn wait_for_run(app: &App, run_id: i64, what: &str, holds: impl Fn(&Run) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let run = app.store.run(run_id).unwrap().unwrap();
        if holds(&run) {
            return;
        }
        assert!(Instant::now() < deadline, "{what}: {run:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits until `holds` holds.
pub(super) fn wait_for(what: &str, mut holds: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !holds() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// What the fake Pi of the run `run_id` was started with, and the commands it was sent.
/// Waits for the fake Pi to have written its arguments.
pub(super) fn argv_and_stdin(app: &App, run_id: i64) -> (Vec<String>, Vec<Value>) {
    let agent_dir = app.data_dir.join(format!("runs/{run_id}/agent"));
    let argv = || {
        let argv = std::fs::read(agent_dir.join("argv.json")).ok()?;
        serde_json::from_slice::<Vec<String>>(&argv).ok()
    };
    // Keep the arguments that were read: a Pi started again writes the file anew, and a second read can fall
    // between the two.
    let mut read = None;
    wait_for("Pi writes its arguments", || {
        read = argv();
        read.is_some()
    });
    let stdin = std::fs::read_to_string(agent_dir.join("stdin.jsonl")).unwrap_or_default();
    let stdin = stdin.lines().filter_map(|l| serde_json::from_str(l).ok());
    (read.unwrap(), stdin.collect())
}

/// The value after `flag` in `argv`.
pub(super) fn flag<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    let at = argv.iter().position(|arg| arg == flag)?;
    argv.get(at + 1).map(String::as_str)
}
