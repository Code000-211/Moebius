//! A hard card tried several times at once (issue #118) on the fake Pi of `testdata`: three attempts, each a run of
//! its own on a branch of its own, are judged by the repository's checks. The script is the one the workflows guide
//! shows. The model an attempt is given decides what it writes (`fake:good` or `fake:bad` writes `attempt.txt`), and the
//! checks (`grep -q good attempt.txt`) pass for the good ones. The fake Pi is a Python script started by its `#!` line,
//! so these tests run on Unix only.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use moebius_core::app::{App, PERSON};
use moebius_core::board::{Engine, Limits, Workflow};
use moebius_core::config::AgentConfig;
use moebius_core::harness::pibolt::PiBolt;
use moebius_core::repos::git;
use moebius_core::router::{Protocol, Router};
use moebius_core::store::{Attempt, Card, Run, Store};
use moebius_plugin_api::Host;

/// The `code.luau` of the workflows guide that tries a hard card several times.
fn guide_script(models: &str) -> String {
    let page = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/guide/workflows.md"),
    )
    .unwrap();
    let (_, after) = page
        .split_once("<!-- script: code.luau; test: hard-attempts -->\n```luau\n")
        .expect("the guide has the script");
    let script = after.split("\n```").next().unwrap();
    let line = "local MODELS = { \"strong\", \"fast\" }";
    assert!(script.contains(line));
    script.replace(line, &format!("local MODELS = {{ {models} }}"))
}

struct Setup {
    _dir: tempfile::TempDir,
    app: Arc<App>,
    engine: Engine,
    card: Card,
}

fn agent(model: &str) -> AgentConfig {
    AgentConfig {
        model: model.into(),
        thinking: None,
        instructions: None,
        skills: vec![],
        extensions: vec![],
        mcp: BTreeMap::new(),
        protocol: Protocol::OpenAi,
        token_budget: None,
        harness: Default::default(),
        tool_output: Default::default(),
        engineering_skills: true,
        runner: None,
    }
}

/// A repository whose checks pass for an attempt that wrote `good`, with the guide's script for `models`, and a hard
/// card in progress.
fn setup(models: &str) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin");
    std::fs::create_dir_all(origin.join(".moebius")).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "hello\n").unwrap();
    std::fs::write(origin.join(".moebius/checks"), "grep -q good attempt.txt\n").unwrap();
    std::fs::write(origin.join(".moebius/code.luau"), guide_script(models)).unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();

    let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
    let app = Arc::new(App::new(
        store.clone(),
        Arc::new(Host::new(Duration::from_secs(5))),
        &dir.path().join("data"),
    ));
    let repository = app
        .add_repository("demo", origin.to_str().unwrap())
        .unwrap();
    let pi = PiBolt::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi"),
        "/usr/local/bin/moebius".into(),
        "http://127.0.0.1:9".into(),
        Arc::new(Router::new(store, vec![])),
        BTreeMap::from([
            ("Coder".to_string(), agent("qwen-local")),
            ("Reviewer".to_string(), agent("qwen-local")),
        ]),
    );
    let engine = Engine::new(
        app.clone(),
        Arc::new(pi),
        Workflow::default(),
        Limits::default(),
    );
    let card = app
        .create_issue(repository.id, "[hard] Say hello", "Write hello.", PERSON)
        .unwrap();
    let card = app.move_card(card.id, "In progress").unwrap();
    Setup {
        _dir: dir,
        app,
        engine,
        card,
    }
}

impl Setup {
    /// Ticks the card until it is in `In review`.
    fn until_in_review(&self) {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            self.engine.settle(self.card.id).unwrap();
            if self.app.card(self.card.id).unwrap().column == "In review" {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the card did not reach review: {:#?} {:#?}",
                self.runs(),
                self.attempts()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn runs(&self) -> Vec<Run> {
        self.app.store.runs_for_card(self.card.id).unwrap()
    }

    fn attempts(&self) -> Vec<Attempt> {
        self.app.store.attempts_for_card(self.card.id).unwrap()
    }

    fn purposes(&self) -> Vec<String> {
        self.runs().into_iter().map(|run| run.purpose).collect()
    }
}

#[test]
fn of_three_attempts_the_one_that_passes_is_kept_without_a_selection_run() {
    let s = setup("\"fake:bad\", \"fake:good\"");
    s.until_in_review();

    // Three attempts on three branches, from the same commit; the second passed and became the card's code run.
    let attempts = s.attempts();
    assert_eq!(attempts.len(), 3);
    assert_eq!(
        attempts
            .iter()
            .map(|a| a.model.as_deref())
            .collect::<Vec<_>>(),
        [Some("fake:bad"), Some("fake:good"), Some("fake:bad")]
    );
    assert_eq!(
        attempts
            .iter()
            .map(|a| a.result.as_deref())
            .collect::<Vec<_>>(),
        [Some("failed"), Some("passed"), Some("failed")]
    );
    assert!(
        attempts[0].output.contains("The checks failed"),
        "{:?}",
        attempts[0].output
    );
    assert_eq!(
        attempts.iter().map(|a| a.picked).collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(
        attempts[1].reason.as_deref(),
        Some("The only attempt that passed.")
    );
    assert!(attempts.iter().all(|a| a.settled));
    assert_eq!(
        s.purposes(),
        ["attempt", "code", "attempt"],
        "no selection run was needed"
    );

    // The card builds on the picked branch; the others are kept.
    let runs = s.runs();
    let clone = Path::new(
        &s.app
            .store
            .repository(s.card.repository_id)
            .unwrap()
            .unwrap()
            .path,
    )
    .to_path_buf();
    assert_eq!(
        s.app.store.code_branch(s.card.id).unwrap(),
        Some(runs[1].branch())
    );
    for run in &runs {
        assert_eq!(run.status, "succeeded");
        git(&clone, &["rev-parse", &run.branch()]).unwrap();
    }
    let kept = git(
        &clone,
        &["show", &format!("{}:attempt.txt", runs[1].branch())],
    )
    .unwrap();
    assert_eq!(kept.trim(), "good");
    let other = git(
        &clone,
        &["show", &format!("{}:attempt.txt", runs[0].branch())],
    )
    .unwrap();
    assert_eq!(other.trim(), "bad");
}

#[test]
fn of_two_that_pass_the_reviewer_picks_one_with_its_reasons() {
    let s = setup("\"fake:good\", \"fake:good\", \"fake:bad\"");
    s.until_in_review();

    assert_eq!(
        s.purposes(),
        ["attempt", "code", "attempt", "review-select"],
    );
    let runs = s.runs();
    let attempts = s.attempts();
    assert_eq!(
        attempts
            .iter()
            .map(|a| a.result.as_deref())
            .collect::<Vec<_>>(),
        [Some("passed"), Some("passed"), Some("failed")]
    );
    // The selection run was told the two that passed and not the third; the fake Pi picks the last one listed.
    let select = runs
        .iter()
        .find(|r| r.purpose == "review-select")
        .expect("a selection run");
    assert_eq!(select.agent, "Reviewer");
    let prompt = std::fs::read_to_string(
        s.app
            .data_dir
            .join(format!("runs/{}/agent/prompt.txt", select.id)),
    )
    .unwrap();
    assert!(
        prompt.contains(&format!(
            "- run {}: branch moebius/run-{}",
            attempts[0].run_id, attempts[0].run_id
        )),
        "{prompt}"
    );
    assert!(
        prompt.contains(&format!("- run {}:", attempts[1].run_id)),
        "{prompt}"
    );
    assert!(
        !prompt.contains(&format!("- run {}:", attempts[2].run_id)),
        "{prompt}"
    );
    assert_eq!(
        attempts.iter().map(|a| a.picked).collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(
        attempts[1].reason.as_deref(),
        Some("Both are fine; the last one is clearer.")
    );
    assert_eq!(
        s.app.store.code_branch(s.card.id).unwrap(),
        Some(runs[1].branch())
    );
    assert_eq!(
        s.app
            .store
            .run(attempts[1].run_id)
            .unwrap()
            .unwrap()
            .purpose,
        "code"
    );
    assert_eq!(
        s.app
            .store
            .run(attempts[0].run_id)
            .unwrap()
            .unwrap()
            .purpose,
        "attempt"
    );
}

#[test]
fn when_none_passes_the_round_fails_and_the_next_is_told_why() {
    let s = setup("\"fake:bad\"");
    let deadline = Instant::now() + Duration::from_secs(90);
    // Ticks until the first group is settled with none picked.
    while s.app.store.failed_attempt_rounds(s.card.id).unwrap() == 0 {
        s.engine.settle(s.card.id).unwrap();
        assert!(Instant::now() < deadline, "{:#?}", s.attempts());
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(s.app.card(s.card.id).unwrap().column, "In progress");
    let note = s.app.card(s.card.id).unwrap().note.unwrap();
    assert!(note.starts_with("None of the attempts passed."), "{note}");
    assert!(note.contains("The checks failed"), "{note}");
    assert!(
        s.attempts()
            .iter()
            .all(|a| a.result.as_deref() == Some("failed") && !a.picked)
    );
    assert_eq!(s.app.store.code_branch(s.card.id).unwrap(), None);

    // The next tick tries again, with the note in the prompt of each attempt.
    s.engine.settle(s.card.id).unwrap();
    let next = s.app.store.open_attempts(s.card.id).unwrap();
    assert_eq!(next.len(), 3);
    let prompt = std::fs::read_to_string(
        s.app
            .data_dir
            .join(format!("runs/{}/agent/prompt.txt", next[0].run_id)),
    )
    .unwrap();
    assert!(prompt.contains("None of the attempts passed."), "{prompt}");
}
