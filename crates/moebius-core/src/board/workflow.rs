//! The columns of a board and the default workflow (spec 6): its scripts, built into the server from `workflow/`, and
//! the scripts a repository puts in its `.moebius/` on its default branch in their place.

use std::path::Path;

use crate::app::columns;
use crate::repos::git;
use crate::store::Repository;

/// A column and the script behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub script: String,
}

/// The columns of a board, left to right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workflow {
    pub columns: Vec<Column>,
}

/// The default workflow's scripts (spec 6), built into the server.
pub(super) const DEFAULT_SCRIPTS: &[(&str, &str)] = &[
    ("backlog.luau", include_str!("../../workflow/backlog.luau")),
    ("ready.luau", include_str!("../../workflow/ready.luau")),
    ("code.luau", include_str!("../../workflow/code.luau")),
    ("review.luau", include_str!("../../workflow/review.luau")),
    ("merge.luau", include_str!("../../workflow/merge.luau")),
    ("done.luau", include_str!("../../workflow/done.luau")),
];

/// The column of the default workflow called `name`, in any case.
pub fn column_named(name: &str) -> Option<Column> {
    Workflow::default()
        .columns
        .into_iter()
        .find(|column| column.name.eq_ignore_ascii_case(name))
}

/// The built-in script file `file`, such as `ready.luau`.
pub fn built_in_script(file: &str) -> Option<&'static str> {
    DEFAULT_SCRIPTS
        .iter()
        .find(|(name, _)| *name == file)
        .map(|(_, source)| *source)
}

/// The script file a repository has for `file` on its default branch, if it overrides the built-in one.
pub fn repository_script(repository: &Repository, file: &str) -> Option<String> {
    let spec = format!("{}:.moebius/{file}", repository.base_ref());
    git(Path::new(&repository.path), &["show", &spec]).ok()
}

impl Default for Workflow {
    fn default() -> Self {
        let columns = [
            (columns::BACKLOG, "backlog.luau"),
            (columns::READY, "ready.luau"),
            (columns::IN_PROGRESS, "code.luau"),
            (columns::IN_REVIEW, "review.luau"),
            (columns::READY_TO_MERGE, "merge.luau"),
            (columns::DONE, "done.luau"),
        ];
        Self {
            columns: columns
                .into_iter()
                .map(|(name, script)| Column {
                    name: name.into(),
                    script: script.into(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::app::{App, PERSON};
    use crate::board::Limits;
    use crate::board::testing::{FakeHarness, column, setup, setup_with_harness};
    use crate::repos::git;
    use crate::store::Card;

    #[test]
    fn a_card_goes_back_to_coding_until_the_reviewer_approves() {
        let harness = Arc::new(FakeHarness::with_reviewer(&[
            "There is no test.\nVERDICT: changes",
            "Fine now.\nVERDICT: approve",
        ]));
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();

        // Round one: coded, then reviewed, and the review wants changes.
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        let step = engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        assert_eq!(step.waiting.as_deref(), Some("reviewing"));

        // Back in progress, the next Coder round starts with the findings.
        let step = engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert_eq!(step.waiting.as_deref(), Some("coding, round 2"));
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");

        let runs = app.store.runs_for_card(card.id).unwrap();
        let purposes: Vec<_> = runs.iter().map(|r| r.purpose.as_str()).collect();
        assert_eq!(purposes, ["code", "review", "code", "review"]);
        assert_eq!(
            *harness.notes.lock().unwrap(),
            [None, None, Some("There is no test.".to_string()), None]
        );

        // The change is one branch that carries both rounds' work.
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.store.changes(repo).unwrap().len(), 1);
        assert_eq!(change.branch, "moebius/card-1");
        let clone = Path::new(&app.store.repository(repo).unwrap().unwrap().path).to_owned();
        for run in [&runs[0], &runs[2]] {
            let file = format!("{}:run-{}.txt", change.branch, run.id);
            assert_eq!(git(&clone, &["show", &file]).unwrap(), "Add a feature");
        }
    }

    #[test]
    fn a_review_without_a_verdict_sends_the_card_back_to_coding() {
        let harness = Arc::new(FakeHarness::with_reviewer(&["Looks fine to me."]));
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert_eq!(
            harness.notes.lock().unwrap()[2].as_deref(),
            Some("The review gave no verdict.")
        );
    }

    #[test]
    fn a_failed_review_is_tried_again_until_the_round_limit() {
        let harness = Arc::new(FakeHarness::with_reviewer(&[]));
        let (_dir, app, engine, repo) = setup_with_harness(None, harness);
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        // The code run is done and the card moves to review.
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        let fail_latest = || {
            let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
            app.store.set_run_status(run.id, "failed").unwrap();
        };
        for tries in 1..=Limits::default().round_limit {
            engine.settle(card.id).unwrap();
            let reviews = |app: &App| {
                let runs = app.store.runs_for_card(card.id).unwrap();
                runs.iter().filter(|r| r.purpose == "review").count()
            };
            assert_eq!(reviews(&app), tries as usize);
            fail_latest();
        }
        let step = engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        assert_eq!(step.waiting.as_deref(), Some("the round limit is reached"));
        assert_eq!(app.store.inbox().unwrap()[0].kind, "stuck");
    }

    #[test]
    fn a_rejected_change_goes_back_to_coding_with_the_reason_and_stays_the_same_change() {
        let harness = Arc::new(FakeHarness::default());
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();

        app.reject_change(
            &crate::decide::Actor::Person,
            change.id,
            "Please add a test.",
        )
        .unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert!(app.store.inbox().unwrap().is_empty());
        let step = engine.settle(card.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("coding, round 2"));
        assert_eq!(
            harness.notes.lock().unwrap()[1].as_deref(),
            Some("Please add a test.")
        );

        // The next round updates the change, which is offered again.
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let changes = app.store.changes(repo).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(
            (changes[0].id, changes[0].state.as_str()),
            (change.id, "open")
        );
        assert_ne!(changes[0].head, change.head);
    }

    /// Commits `text` as `path` on the repository's default branch, as someone merging elsewhere would.
    fn commit_to_base(app: &App, repo: i64, path: &str, text: &str) {
        let clone = std::path::PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        std::fs::create_dir_all(clone.join(path).parent().unwrap()).unwrap();
        std::fs::write(clone.join(path), text).unwrap();
        git(&clone, &["add", "."]).unwrap();
        git(&clone, &["commit", "--quiet", "-m", path]).unwrap();
    }

    /// Waits until the repository's checks of the card's change have ended.
    fn wait_for_checks(app: &App, card: &Card) {
        let give_up = Instant::now() + Duration::from_secs(60);
        while app
            .store
            .open_change(card.id)
            .unwrap()
            .unwrap()
            .checks
            .as_deref()
            == Some("pending")
        {
            assert!(Instant::now() < give_up, "the checks never ended");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn note(app: &App, card: &Card) -> Option<String> {
        app.card(card.id).unwrap().note
    }

    #[test]
    fn failing_checks_send_the_card_back_with_their_output_and_passing_ones_let_it_on() {
        let harness = Arc::new(FakeHarness::default());
        let (dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let count = dir.path().join("count.txt");
        let checks = format!(
            "echo x >> {}\necho 'boom: 3 tests failed'\nexit 1\n",
            count.display()
        );
        commit_to_base(&app, repo, ".moebius/checks", &checks);
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();

        engine.settle(card.id).unwrap();
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        let step = engine.tick(card.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("waiting for the checks"));
        // Asked again, whether they still run or ended, they are not started again.
        app.run_checks(card.id).unwrap();
        wait_for_checks(&app, &card);

        // The checks failed: the card goes back to coding and the next round is told what failed.
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert!(
            note(&app, &card)
                .unwrap()
                .starts_with("The checks failed:\nboom: 3 tests failed")
        );
        engine.tick(card.id).unwrap();
        let notes = harness.notes.lock().unwrap().clone();
        assert_eq!(notes.len(), 2);
        assert!(
            notes[1]
                .as_deref()
                .unwrap()
                .contains("boom: 3 tests failed")
        );
        assert_eq!(note(&app, &card), None, "the round succeeded");
        let leftovers = std::fs::read_dir(dir.path().join("data/worktrees/demo"))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("checks")
            })
            .count();
        assert_eq!(leftovers, 0, "the checks' worktree is removed");

        // The repository fixes its checks; the second round's commit is published on the same branch and checked.
        commit_to_base(&app, repo, ".moebius/checks", "ls run-*.txt\n");
        engine.tick(card.id).unwrap();
        engine.tick(card.id).unwrap();
        wait_for_checks(&app, &card);
        assert_eq!(app.store.changes(repo).unwrap().len(), 1);
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.checks_of(&change).unwrap(), "success");
        assert_eq!(std::fs::read_to_string(count).unwrap(), "x\n");
    }

    #[test]
    fn a_repository_without_checks_has_none_and_the_card_goes_on() {
        let (_dir, app, engine, repo) = setup();
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.checks_of(&change).unwrap(), "none");
        assert_eq!(change.checks, None);
    }

    #[test]
    fn the_checks_are_read_from_the_default_branch_not_from_the_change() {
        let harness = Arc::new(FakeHarness {
            files: vec![(".moebius/checks", "exit 1\n")],
            ..FakeHarness::default()
        });
        let (_dir, app, engine, repo) = setup_with_harness(None, harness);
        commit_to_base(&app, repo, ".moebius/checks", "exit 0\n");
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.tick(card.id).unwrap();
        engine.tick(card.id).unwrap();
        wait_for_checks(&app, &card);
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.checks_of(&change).unwrap(), "success");
    }

    #[test]
    fn a_change_that_no_longer_merges_goes_back_with_the_conflicting_files() {
        let harness = Arc::new(FakeHarness::default());
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        // The base meanwhile took a different file of the same name.
        commit_to_base(&app, repo, "run-1.txt", "something else\n");
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        let note = note(&app, &card).unwrap();
        assert!(note.contains("run-1.txt"), "{note}");
        assert!(
            note.contains("Merge main into your branch and resolve the conflicts"),
            "{note}"
        );

        engine.tick(card.id).unwrap();
        assert_eq!(harness.notes.lock().unwrap()[1].as_deref(), Some(&*note));
    }

    #[test]
    fn a_conflict_that_appears_while_the_change_waits_to_be_merged_sends_it_back() {
        let (_dir, app, engine, repo) = setup();
        let card = app.create_issue(repo, "Add a feature", "", PERSON).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        assert_eq!(app.store.inbox().unwrap().len(), 1);
        let step = engine.tick(card.id).unwrap();
        assert_eq!(
            step.waiting.as_deref(),
            Some("waiting for a person to merge")
        );

        commit_to_base(&app, repo, "run-1.txt", "something else\n");
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert!(note(&app, &card).unwrap().contains("run-1.txt"));
        assert!(app.store.inbox().unwrap().is_empty());
    }
}
