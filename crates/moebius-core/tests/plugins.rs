//! Plugins in their own process, as `[[plugins]]` names them: the guide's Python decider answers its kind through the
//! server with the guide's configuration, a plugin that crashes fails closed and is started again, and the Plugins
//! answer shows both with their state and their logs, redacted.

// Unix only: the plugins are Python scripts started by their `#!` line or by `python3`.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use moebius_core::config::Config;
use moebius_core::decide::Question;
use moebius_core::server::Server;
use moebius_core::server::setup;

/// The first block of `language` in the guide's plugins page after `after`.
fn guide_block(language: &str, after: &str) -> String {
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/guide/plugins.md");
    let page = std::fs::read_to_string(page).unwrap();
    let (_, rest) = page.split_once(after).unwrap();
    let (_, block) = rest.split_once(&format!("```{language}\n")).unwrap();
    block.split_once("```").unwrap().0.to_string()
}

fn question(kind: &str, title: &str, options: [&str; 2]) -> Question {
    Question {
        kind: kind.into(),
        context: BTreeMap::from([("title".to_string(), title.to_string())]),
        options: options.map(String::from).to_vec(),
        fail_closed: options[1].into(),
        ..Question::default()
    }
}

/// Waits, at most a minute, until `done` holds.
fn eventually(mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "never happened"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_plugin_in_its_own_process_decides_and_one_that_crashes_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("plugins")).unwrap();
    std::fs::write(
        dir.path().join("plugins/start_gate.py"),
        guide_block("python", "## Run it as its own process"),
    )
    .unwrap();
    let crashes =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../moebius-plugin-api/testdata/plugin.py");
    let secret = format!("ghp_{}", "a1".repeat(18));
    let text = format!(
        "{}\n[[plugins]]\nname = \"crashes\"\npath = {:?}\nargs = [\"crashes\", \"crashes\", \"token\", {secret:?}]\n\
         decides = [\"crash_kind\"]\n",
        guide_block("toml", "## Run it as its own process"),
        crashes.display().to_string(),
    );
    let config = Config::parse(&text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
    let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
    let decisions = &server.app.decisions;

    let held = decisions
        .decide(&question(
            "start_card",
            "[wip] Add a menu",
            ["start", "hold"],
        ))
        .unwrap();
    assert_eq!((held.choice.as_str(), held.refused), ("hold", false));
    assert_eq!(held.decider, "start-gate");
    let started = decisions
        .decide(&question("start_card", "Add a menu", ["start", "hold"]))
        .unwrap();
    assert_eq!(started.choice, "start");

    let crashed = decisions
        .decide(&question("crash_kind", "x", ["yes", "no"]))
        .unwrap();
    assert_eq!((crashed.choice.as_str(), crashed.refused), ("no", true));
    assert!(
        crashed
            .reason
            .starts_with("refused: crashes is not running: it exited"),
        "{}",
        crashed.reason
    );

    let data_dir = &server.config.data_dir;
    let plugins = setup::plugins(decisions, data_dir);
    let plugin = |name: &str| {
        plugins
            .plugins
            .iter()
            .find(|plugin| plugin.name == name)
            .unwrap()
            .clone()
    };
    let gate = plugin("start-gate").process.unwrap();
    assert_eq!(gate.state, moebius_plugin_api::State::Running);
    assert_eq!(gate.program, "python3");
    assert_eq!(plugin("start-gate").decides, ["start_card"]);
    assert!(plugin("guard").process.is_none());
    let crash = plugin("crashes").process.unwrap();
    assert!(crash.error.unwrap().starts_with("it exited"));
    assert_eq!(
        crash.log,
        data_dir.join("plugins/crashes.log").display().to_string()
    );

    // It is started again, and what it wrote to standard error is in its log without the secret.
    eventually(|| {
        let status = decisions.host().status("crashes").unwrap();
        status.restarts >= 1 && status.state == moebius_plugin_api::State::Running
    });
    eventually(|| {
        std::fs::read_to_string(data_dir.join("plugins/crashes.log")).is_ok_and(|log| {
            log.contains("moebius: crashes stopped: it exited") && log.contains("started token")
        })
    });
    let log = std::fs::read_to_string(data_dir.join("plugins/crashes.log")).unwrap();
    assert!(!log.contains(&secret), "{log}");
    assert!(log.contains("[redacted:"), "{log}");
}
