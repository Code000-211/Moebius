//! The test lane for a change of the server's own (ADR 0035): the lane is the server itself, a column script asks it for a
//! job with `moebius.lane_job()`, the job runs the project's steps at the change's head, and what comes of it is what
//! the script reads and what the author is sent back with.
#![cfg(unix)] // The lane runs `sh`, `git`, `timeout` and `sha256sum` on its machine.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use moebius_core::app::{App, OWNER_AUTHOR};
use moebius_core::board::{CardRuns, Engine, Kanban, Limits, StartError, Variant};
use moebius_core::config::{Isolation, LaneConfig};
use moebius_core::repos::git;
use moebius_core::store::{Card, Run, Store};
use moebius_plugin_api::Host;

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
    fn hold(&self, _: bool) {}
}

const KANBAN: &str = "[[column]]\nname = \"Ready for testing\"\nafter = \"In review\"\nscript = \"lane.luau\"\nlane = true\n";

const SCRIPT: &str = "function on_card(card)
  local job, why = moebius.lane_job()
  if job == nil then
    moebius.wait(\"no job: \" .. why)
  elseif job.state == \"passed\" or job.state == \"measured\" then
    moebius.move(\"Ready to merge\", \"the lane \" .. job.lane .. \" passed it\")
  else
    moebius.wait(\"the lane: \" .. job.state)
  end
end
";

struct World {
    _dir: tempfile::TempDir,
    app: Arc<App>,
    engine: Engine,
    card: Card,
}

/// A card of the server's own whose change adds `feature.txt`, reviewed and approved, in the column of the lane.
fn world(definition: &str, approved: bool) -> World {
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
    for (file, text) in [
        ("kanban.toml", KANBAN),
        ("lane.toml", definition),
        ("lane.luau", SCRIPT),
    ] {
        common::put_project(&harness, "demo", file, text);
    }
    let repository = app
        .add_repository("demo", origin.to_str().unwrap())
        .unwrap();
    app.lanes.configure(
        vec![LaneConfig {
            name: "here".into(),
            runner: None,
            work_dir: None,
            isolation: Isolation::Off,
            runtime: "podman".into(),
            image: None,
            devices: vec![],
            groups: vec![],
            user: "65534:65534".into(),
            timeout_seconds: 120,
            memory: "16g".into(),
            cpus: None,
            disk: "1m".into(),
            gpus: Some("1x test".into()),
            power_cap: None,
            rocm: None,
            driver: None,
        }],
        vec![],
    );
    app.store
        .set_repository_lane(repository.id, Some("here"))
        .unwrap();
    let engine = Engine::new(
        app.clone(),
        Arc::new(IdleRuns),
        Kanban::default(),
        Limits {
            run_limit: 3,
            ..Limits::default()
        },
    );
    let card = app
        .create_issue(repository.id, "Add a feature", "", OWNER_AUTHOR)
        .unwrap();
    // The change: a branch of the clone with one new file, and its coder's and reviewer's runs.
    let clone = PathBuf::from(&repository.path);
    git(&clone, &["checkout", "--quiet", "-b", "moebius/card-1"]).unwrap();
    std::fs::write(clone.join("feature.txt"), "feature\n").unwrap();
    git(&clone, &["add", "."]).unwrap();
    git(&clone, &["commit", "--quiet", "-m", "Add the feature"]).unwrap();
    let head = git(&clone, &["rev-parse", "HEAD"]).unwrap();
    git(&clone, &["checkout", "--quiet", "main"]).unwrap();
    app.store
        .add_change(repository.id, Some(card.id), "moebius/card-1", None, &head)
        .unwrap();
    let code = app
        .store
        .add_run(Some(card.id), "Coder", "m", "code")
        .unwrap();
    app.store.set_run_status(code.id, "succeeded").unwrap();
    if approved {
        let review = app
            .store
            .add_run(Some(card.id), "Reviewer", "m", "review")
            .unwrap();
        app.store
            .set_run_summary(review.id, "Fine.\nVERDICT: approve")
            .unwrap();
        app.store.set_run_status(review.id, "succeeded").unwrap();
    }
    World {
        _dir: dir,
        app,
        engine,
        card,
    }
}

impl World {
    fn run_the_lane(&self) {
        for id in self.app.lane_pass() {
            self.app.run_lane_job(id);
        }
    }

    fn waiting(&self) -> String {
        self.engine
            .tick(self.card.id)
            .unwrap()
            .waiting
            .unwrap_or_default()
    }
}

#[test]
fn a_script_asks_the_lane_that_is_the_server_for_a_job_and_moves_the_card_when_it_passes() {
    let w = world(
        "[[step]]\nname = \"has the feature\"\ncommand = \"test -f feature.txt && test ! -f missing.txt\"\n",
        true,
    );
    w.app
        .move_card(w.card.id, "Ready for testing", None)
        .unwrap();
    // Entering the column queued it; the script sees the job, and asking again makes no second.
    assert_eq!(w.waiting(), "the lane: queued");
    assert_eq!(w.app.store.lane_jobs_of_card(w.card.id).unwrap().len(), 1);
    w.run_the_lane();
    let job = w.app.store.lane_jobs_of_card(w.card.id).unwrap().remove(0);
    assert_eq!(job.state, "passed", "{job:?}");
    assert_eq!(job.report["machine"]["where"], "the server");
    assert_eq!(job.report["machine"]["gpus"], "1x test");
    w.engine.tick(w.card.id).unwrap();
    assert_eq!(w.app.card(w.card.id).unwrap().column, "Ready to merge");
}

#[test]
fn a_script_is_told_why_when_the_review_has_not_approved_and_nothing_runs() {
    let w = world("[[step]]\nname = \"s\"\ncommand = \"true\"\n", false);
    w.app
        .move_card(w.card.id, "Ready for testing", None)
        .unwrap();
    assert!(
        w.waiting().contains("the Reviewer has not approved"),
        "{}",
        w.waiting()
    );
    assert!(w.app.lane_pass().is_empty());
    assert!(w.app.store.lane_jobs_of_card(w.card.id).unwrap().is_empty());
}

#[test]
fn a_failing_step_sends_the_change_of_the_server_back_to_its_coder_with_the_output_as_the_note() {
    let w = world(
        "[[step]]\nname = \"build\"\ncommand = \"echo 'link error: no kernel'; exit 4\"\n",
        true,
    );
    w.app
        .move_card(w.card.id, "Ready for testing", None)
        .unwrap();
    w.run_the_lane();
    let job = w.app.store.lane_jobs_of_card(w.card.id).unwrap().remove(0);
    assert_eq!(job.state, "failed", "{job:?}");
    let card = w.app.card(w.card.id).unwrap();
    assert_eq!(card.column, "In progress");
    let note = card.note.unwrap_or_default();
    assert!(
        note.contains("build") && note.contains("link error: no kernel"),
        "{note}"
    );
}

#[test]
fn a_lane_that_is_not_in_the_configuration_queues_nothing_and_says_so() {
    let w = world("[[step]]\nname = \"s\"\ncommand = \"true\"\n", true);
    w.app.lanes.configure(vec![], vec![]);
    let asked = w.app.request_lane_job(w.card.id).unwrap();
    assert!(asked.job.is_none());
    assert!(
        asked
            .why
            .unwrap()
            .contains("not in the server's configuration")
    );
}
