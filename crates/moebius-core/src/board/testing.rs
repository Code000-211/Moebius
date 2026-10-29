//! What the board's tests share: a harness that finishes at once, and a server with one repository.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use moebius_plugin_api::Host;

use super::{Engine, Limits, Runs, Workflow};
use crate::app::App;
use crate::repos::git;
use crate::store::{Card, Run, Store};

/// A harness that finishes at once. A `code` run commits a file on a branch that starts where a real run's would;
/// a `review` run changes nothing and ends with the next of `answers`, or an approval when there are none left.
#[derive(Default)]
pub(super) struct FakeHarness {
    pub(super) reviewer: bool,
    pub(super) answers: Mutex<VecDeque<String>>,
    /// The note of each run started, in order.
    pub(super) notes: Mutex<Vec<Option<String>>>,
    /// Files, as path and text, that each `code` run also commits.
    pub(super) files: Vec<(&'static str, &'static str)>,
}

impl FakeHarness {
    pub(super) fn with_reviewer(answers: &[&str]) -> Self {
        Self {
            reviewer: true,
            answers: Mutex::new(answers.iter().map(|a| a.to_string()).collect()),
            ..Self::default()
        }
    }
}

impl Runs for FakeHarness {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
        note: Option<&str>,
    ) -> Result<Run, String> {
        self.notes.lock().unwrap().push(note.map(String::from));
        let store = &app.store;
        let run = store
            .add_run(Some(card.id), agent, "fake", purpose)
            .map_err(|e| e.to_string())?;
        let repo = store.repository(card.repository_id).unwrap().unwrap();
        let clone = Path::new(&repo.path);
        let base = store
            .code_branch(card.id)
            .unwrap()
            .unwrap_or_else(|| repo.base_ref());
        let wt = app
            .repos
            .add_worktree(&repo.name, clone, run.id, &base)
            .map_err(|e| e.to_string())?;
        if purpose == "code" {
            std::fs::write(wt.path.join(format!("run-{}.txt", run.id)), &card.title).unwrap();
            for (path, text) in &self.files {
                std::fs::create_dir_all(wt.path.join(path).parent().unwrap()).unwrap();
                std::fs::write(wt.path.join(path), text).unwrap();
            }
            git(&wt.path, &["add", "."]).unwrap();
            git(&wt.path, &["commit", "--quiet", "-m", &card.title]).unwrap();
            store.set_run_summary(run.id, "Done.").unwrap();
        } else {
            let answer = self.answers.lock().unwrap().pop_front();
            store
                .set_run_summary(run.id, &answer.unwrap_or("VERDICT: approve".into()))
                .unwrap();
        }
        app.repos.remove_worktree(clone, &wt.path).unwrap();
        store.set_run_status(run.id, "succeeded").unwrap();
        Ok(store.run(run.id).unwrap().unwrap())
    }

    fn has_agent(&self, name: &str) -> bool {
        name == "Coder" || (self.reviewer && name == "Reviewer")
    }

    fn steer(&self, _: i64, _: &str) -> bool {
        false
    }

    fn stop(&self, _: i64) -> bool {
        false
    }
}

pub(super) fn setup() -> (tempfile::TempDir, Arc<App>, Engine, i64) {
    setup_with(None)
}

/// A setup whose forge for repositories without one is `forge`, if given.
pub(super) fn setup_with(
    forge: Option<Box<dyn crate::repos::forge::Forge>>,
) -> (tempfile::TempDir, Arc<App>, Engine, i64) {
    setup_with_harness(forge, Arc::new(FakeHarness::default()))
}

pub(super) fn setup_with_harness(
    forge: Option<Box<dyn crate::repos::forge::Forge>>,
    harness: Arc<FakeHarness>,
) -> (tempfile::TempDir, Arc<App>, Engine, i64) {
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), "hello\n").unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
    let host = Arc::new(Host::new(Duration::from_secs(1)));
    let mut app = App::new(store, host, &dir.path().join("data"));
    if let Some(forge) = forge {
        app.add_forge(forge);
    }
    let app = Arc::new(app);
    let repo = app
        .add_repository("demo", origin.to_str().unwrap())
        .unwrap();
    // The real limits: a short time limit would make tests fail on a busy machine. Only the tests of the limit
    // itself shorten it.
    let engine = Engine::new(app.clone(), harness, Workflow::default(), Limits::default());
    (dir, app, engine, repo.id)
}

pub(super) fn column(app: &App, card: &Card) -> String {
    app.card(card.id).unwrap().column
}

/// The engine of `setup`, with other limits.
pub(super) fn engine_with(app: &Arc<App>, limits: Limits) -> Engine {
    Engine::new(
        app.clone(),
        Arc::new(FakeHarness::default()),
        Workflow::default(),
        limits,
    )
}
