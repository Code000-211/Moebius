//! Runs on a runner, over the fake `ssh` of `testdata` whose "remote machine" is a directory of this one.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use super::PiBolt;
use super::testing::{setup, wait_for, wait_for_run, wait_until_ended};
use crate::app::App;
use crate::board::Runs;
use crate::config::RunnerConfig;
use crate::harness::remote::Runners;
use crate::repos::git;

/// The runner `box`, whose working directory is `remote` and whose key file, `key`, collects a line for each `ssh`.
fn runner(dir: &Path) -> (RunnerConfig, PathBuf, PathBuf) {
    let (remote, key) = (dir.join("remote"), dir.join("box.key"));
    std::fs::write(&key, "not a key").unwrap();
    let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
    let config = RunnerConfig {
        name: "box".into(),
        host: "gpu.test".into(),
        user: Some("moebius".into()),
        port: Some(2222),
        key_file: Some(key.clone()),
        working_dir: remote.to_string_lossy().into_owned(),
        max_runs: 1,
        bin_dir: Some(testdata.to_string_lossy().into_owned()),
        ssh: testdata.join("fake-ssh"),
    };
    (config, remote, key)
}

/// A server whose Coder goes to the runner `box`.
fn with_runner() -> (tempfile::TempDir, Arc<App>, PiBolt, i64, PathBuf, PathBuf) {
    let (dir, app, mut pi, repo) = setup();
    let (config, remote, key) = runner(dir.path());
    pi.runners = Arc::new(Runners::new(vec![config]));
    pi.agents
        .write()
        .unwrap()
        .agents
        .get_mut("Coder")
        .unwrap()
        .runner = Some("box".into());
    (dir, app, pi, repo, remote, key)
}

fn log(key: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(format!("{}.log", key.display())).unwrap_or_default();
    text.lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[test]
fn a_run_on_a_runner_works_there_and_its_commits_come_back() {
    let (_dir, app, pi, repo, remote, key) = with_runner();
    let card = app
        .create_issue(repo, "Write done.txt", "", "person")
        .unwrap();
    let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
    assert_eq!(run.runner.as_deref(), Some("box"));
    let run = wait_until_ended(&app, run.id);
    assert_eq!(
        run.status,
        "succeeded",
        "{:?}",
        app.store.events(run.id).unwrap()
    );
    assert_eq!(run.runner.as_deref(), Some("box"));

    // What the agent wrote on the runner is the run's branch here, with the files of the card.
    let clone = PathBuf::from(app.store.repository(repo).unwrap().unwrap().path);
    let tree = git(&clone, &["ls-tree", "-r", "--name-only", &run.branch()]).unwrap();
    assert!(
        tree.contains("README.md") && tree.lines().count() > 1,
        "{tree}"
    );
    // The runner keeps its clone, and nothing of the run.
    assert!(remote.join("repos/repo-1.git").is_dir());
    assert!(!remote.join("runs/1").exists());

    let lines = log(&key);
    assert!(lines.iter().all(|l| l["target"] == "moebius@gpu.test"));
    let commands: Vec<&str> = lines
        .iter()
        .map(|l| l["command"].as_str().unwrap())
        .collect();
    assert!(
        commands.iter().any(|c| c.contains("git-receive-pack")),
        "{commands:?}"
    );
    assert!(
        commands
            .iter()
            .any(|c| c.contains("exec env") && c.contains("fake-pi")),
        "{commands:?}"
    );
    assert!(lines.iter().any(|l| l["tunnel"] == true));
    // Every call named the port and the key.
    assert!(lines.iter().all(|l| {
        let options = l["options"].as_array().unwrap();
        options
            .iter()
            .any(|o| o == &serde_json::json!(["p", "2222"]))
            || l["command"].as_str().unwrap().contains("git-")
    }));
}

#[test]
fn a_run_on_a_runner_reaches_the_router_through_the_tunnel() {
    let (_dir, app, mut pi, repo, remote, _key) = with_runner();
    // The router is a listener of this test.
    let router = TcpListener::bind("127.0.0.1:0").unwrap();
    pi.router_url = format!("http://{}", router.local_addr().unwrap());
    let card = app
        .create_issue(repo, "Please [hang]", "", "person")
        .unwrap();
    let run = pi.start(&app, &card, "Coder", "code", None).unwrap();

    // The runner holds the run's files, with the tunnel's address where the router's was.
    let files = remote.join(format!("runs/{}/files", run.id)).join(
        app.data_dir
            .join(format!("runs/{}", run.id))
            .strip_prefix("/")
            .unwrap(),
    );
    let env_file = files.join("agent/env.json");
    wait_for("the fake Pi writes its environment", || env_file.is_file());
    let env: Value = serde_json::from_slice(&std::fs::read(&env_file).unwrap()).unwrap();
    let check = env["MOEBIUS_CHECK_URL"].as_str().unwrap().to_string();
    assert!(
        check.starts_with("http://127.0.0.1:") && check != pi.router_url,
        "{check}"
    );
    // The server's own services stay on the server.
    assert!(env["MOEBIUS"].is_null() && env["MOEBIUS_URL"].is_null());
    assert!(env["MOEBIUS_RUN_TOKEN"].is_string());
    let models: Value =
        serde_json::from_slice(&std::fs::read(files.join("agent/models.json")).unwrap()).unwrap();
    assert_eq!(
        models["providers"]["moebius"]["baseUrl"],
        format!("{check}/v1")
    );
    assert!(
        remote
            .join(format!("runs/{}/work/README.md", run.id))
            .is_file()
    );

    // A connection to the tunnel's end arrives at the router.
    let port: u16 = check.rsplit(':').next().unwrap().parse().unwrap();
    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    client.write_all(b"ping").unwrap();
    let (mut arrived, _) = router.accept().unwrap();
    let mut got = [0u8; 4];
    arrived.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"ping");
    arrived.write_all(b"pong").unwrap();
    client.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"pong");

    // Stopping goes over the same session, and the runner is free again.
    assert_eq!(pi.runners.list()[0].busy, 1);
    wait_for("Pi has taken the prompt", || {
        app.store.events(run.id).unwrap().len() >= 2
    });
    assert!(pi.stop(run.id));
    let run = wait_until_ended(&app, run.id);
    assert_eq!(run.status, "stopped");
    wait_for("the runner is free", || pi.runners.list()[0].busy == 0);
    assert!(!remote.join(format!("runs/{}", run.id)).exists());
}

#[test]
fn a_full_or_unknown_runner_starts_no_run() {
    let (_dir, app, pi, repo, remote, _key) = with_runner();
    let card = app
        .create_issue(repo, "Please [hang]", "", "person")
        .unwrap();
    let first = pi.start(&app, &card, "Coder", "code", None).unwrap();
    wait_for_run(&app, first.id, "the first run is going", |run| {
        run.status == "running"
    });

    let busy = pi.start(&app, &card, "Coder", "code", None).unwrap_err();
    assert!(busy.contains("busy"), "{busy}");
    let unknown = pi
        .start_on(&app, &card, "Coder", "code", None, Some("elsewhere"))
        .unwrap_err();
    assert!(unknown.contains("no runner named elsewhere"), "{unknown}");
    // Neither made a run.
    assert_eq!(app.store.runs_for_card(card.id).unwrap().len(), 1);

    // A script's runner replaces the agent's, and an agent without one runs on the server.
    pi.stop(first.id);
    wait_until_ended(&app, first.id);
    wait_for("the runner is free", || pi.runners.list()[0].busy == 0);
    pi.agents
        .write()
        .unwrap()
        .agents
        .get_mut("Coder")
        .unwrap()
        .runner = None;
    let card = app
        .create_issue(repo, "Write done.txt", "", "person")
        .unwrap();
    let local = pi.start(&app, &card, "Coder", "code", None).unwrap();
    let local = wait_until_ended(&app, local.id);
    assert_eq!(local.runner, None);
    let elsewhere = pi
        .start_on(&app, &card, "Coder", "code", None, Some("box"))
        .unwrap();
    let elsewhere = wait_until_ended(&app, elsewhere.id);
    assert_eq!(elsewhere.runner.as_deref(), Some("box"));
    assert!(remote.join("repos/repo-1.git").is_dir());
}

#[test]
fn a_runner_that_cannot_be_reached_fails_the_run_and_leaves_nothing() {
    let (_dir, app, mut pi, repo, remote, _key) = with_runner();
    let mut config = runner(_dir.path()).0;
    config.ssh = PathBuf::from("/nonexistent/ssh");
    pi.runners = Arc::new(Runners::new(vec![config]));
    let card = app.create_issue(repo, "Anything", "", "person").unwrap();
    let error = pi.start(&app, &card, "Coder", "code", None).unwrap_err();
    assert!(error.contains("cannot start /nonexistent/ssh"), "{error}");
    let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
    assert_eq!(run.status, "failed");
    assert!(!PathBuf::from(run.worktree.unwrap()).exists());
    assert!(!remote.exists());
    wait_for("the runner is free", || pi.runners.list()[0].busy == 0);
}
