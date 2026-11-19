//! The columns of a board and the default Kanban (spec 6): its scripts, built into the server from `kanban/`, and
//! the scripts a project has in its settings (`projects/<project name>/` of the harness configuration repository) in
//! their place.

use crate::app::{App, columns};
use crate::store::Project;

/// A column and the script behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub script: String,
}

/// The columns of a board, left to right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kanban {
    pub columns: Vec<Column>,
}

/// The default Kanban's scripts (spec 6), built into the server.
pub(super) const DEFAULT_SCRIPTS: &[(&str, &str)] = &[
    ("backlog.luau", include_str!("../../kanban/backlog.luau")),
    ("ready.luau", include_str!("../../kanban/ready.luau")),
    ("code.luau", include_str!("../../kanban/code.luau")),
    ("review.luau", include_str!("../../kanban/review.luau")),
    ("merge.luau", include_str!("../../kanban/merge.luau")),
    ("done.luau", include_str!("../../kanban/done.luau")),
];

/// The name of the column `name` stands for: "Ready", the name the column "Not started" had, is its alias, so scripts
/// and callers written for it keep working. Any other name is returned as it is.
pub fn resolve_column(name: &str) -> &str {
    if name.eq_ignore_ascii_case("Ready") {
        crate::app::columns::NOT_STARTED
    } else {
        name
    }
}

/// The column of the default Kanban called `name` (or its alias, see [`resolve_column`]), in any case.
pub fn column_named(name: &str) -> Option<Column> {
    let name = resolve_column(name);
    Kanban::default()
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

/// The script file `project` has for `file`, if it overrides the built-in one.
pub fn project_script(app: &App, project: &Project, file: &str) -> Option<String> {
    app.project_file(project, file)
}

impl Default for Kanban {
    fn default() -> Self {
        // The script file of a column keeps the name it was first given, so a project that wrote
        // `ready.luau` still overrides the column now called "Not started".
        let columns = [
            (columns::BACKLOG, "backlog.luau"),
            (columns::NOT_STARTED, "ready.luau"),
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
// Some tests are Unix only; on Windows what only they use is unused.
#[cfg_attr(not(unix), allow(unused))]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::app::{App, OWNER_AUTHOR};
    use crate::board::Limits;
    use crate::board::testing::{FakeHarness, column, setup, setup_unreviewed, setup_with_harness};
    use crate::repos::git;
    use crate::store::Card;

    #[test]
    fn a_card_goes_back_to_coding_until_the_reviewer_approves() {
        let harness = Arc::new(FakeHarness::with_reviewer(&[
            "Follows the rules.\nVERDICT: approve",
            "FINDING blocking: There is no test.\nEVIDENCE: the issue says \"tested\"\nVERDICT: changes",
            "Fine now.\nVERDICT: approve",
            "Fine now.\nVERDICT: approve",
        ]));
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();

        // Round one: coded, then reviewed, and the review wants changes.
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        let step = engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        assert_eq!(step.waiting.as_deref(), Some("reviewing the standards"));
        let step = engine.settle(card.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("reviewing the spec"));
        // The findings send it back, and the next Coder round starts with them.
        let step = engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert_eq!(step.waiting.as_deref(), Some("coding, round 2"));
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");

        let runs = app.store.runs_for_card(card.id).unwrap();
        let purposes: Vec<_> = runs.iter().map(|r| r.purpose.as_str()).collect();
        assert_eq!(
            purposes,
            [
                "code",
                "review-standards",
                "review-spec",
                "code",
                "review-standards",
                "review-spec"
            ]
        );
        assert_eq!(
            *harness.notes.lock().unwrap(),
            [
                None,
                None,
                None,
                Some(
                    "Spec (does the change do what the issue asked):\n- There is no test.\n  \
                     Evidence: the issue says \"tested\""
                        .to_string()
                ),
                None,
                None
            ]
        );

        // The change is one branch that carries both rounds' work.
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.store.changes(repo).unwrap().len(), 1);
        assert_eq!(change.branch, "moebius/card-1");
        let clone = Path::new(&app.store.repository(repo).unwrap().unwrap().path).to_owned();
        for run in [&runs[0], &runs[3]] {
            let file = format!("{}:run-{}.txt", change.branch, run.id);
            assert_eq!(git(&clone, &["show", &file]).unwrap(), "Add a feature");
        }
    }

    /// A card coded and in review, whose standards and spec reviews answer `standards` and `spec`.
    fn reviewed_card(
        standards: &str,
        spec: &str,
    ) -> (
        tempfile::TempDir,
        Arc<App>,
        crate::board::Engine,
        Card,
        Arc<FakeHarness>,
    ) {
        let harness = Arc::new(FakeHarness::with_reviewer(&[standards, spec]));
        let (dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        (dir, app, engine, card, harness)
    }

    #[test]
    fn a_blocking_finding_with_evidence_sends_the_card_back_naming_its_axis() {
        let (_dir, app, _engine, card, harness) = reviewed_card(
            "FINDING blocking: Breaks the naming rule.\nEVIDENCE: fn Do()\nVERDICT: changes",
            "VERDICT: approve",
        );
        assert_eq!(column(&app, &card), "In progress");
        let note = harness.notes.lock().unwrap()[3].clone().unwrap();
        assert!(note.starts_with("Standards ("), "{note}");
        assert!(note.contains("Evidence: fn Do()"), "{note}");
        assert!(!note.contains("Spec"), "{note}");
    }

    #[test]
    fn findings_without_evidence_are_dropped_and_the_card_goes_on() {
        let (_dir, app, _engine, card, _) = reviewed_card(
            "FINDING blocking: Feels off.\nVERDICT: changes",
            "FINDING blocking: Not what I wanted.\nVERDICT: changes",
        );
        assert_eq!(column(&app, &card), "Ready to merge");
        assert_eq!(app.store.inbox().unwrap().len(), 1);
    }

    #[test]
    fn judgement_calls_go_with_the_offered_change_and_do_not_send_the_card_back() {
        let (_dir, app, _engine, card, _) = reviewed_card(
            "FINDING judgement: Possible Feature Envy.\nEVIDENCE: order.items.len()\nVERDICT: approve",
            "FINDING blocking: Wrong.\nFINDING judgement: Extra logging.\nEVIDENCE: + log(x)\nVERDICT: changes",
        );
        assert_eq!(column(&app, &card), "Ready to merge");
        let item = app.store.inbox().unwrap().pop().unwrap();
        assert_eq!(item.kind, "ready_to_merge");
        let judgements = item.data["judgements"].as_str().unwrap();
        assert!(
            judgements.contains("Possible Feature Envy."),
            "{judgements}"
        );
        assert!(judgements.contains("Extra logging."), "{judgements}");
        assert!(judgements.contains("Standards ("), "{judgements}");
        assert!(judgements.contains("Spec ("), "{judgements}");
        assert!(!judgements.contains("Wrong."), "{judgements}");
    }

    #[test]
    fn an_axis_with_an_agent_of_its_own_is_reviewed_by_it() {
        let harness = Arc::new(FakeHarness {
            agents: vec!["Spec"],
            ..FakeHarness::default()
        });
        let (_dir, app, engine, repo) = setup_with_harness(None, harness);
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
        for _ in 0..4 {
            engine.settle(card.id).unwrap();
        }
        let runs = app.store.runs_for_card(card.id).unwrap();
        let agents: Vec<_> = runs.iter().map(|r| r.agent.as_str()).collect();
        assert_eq!(agents, ["Coder", "Reviewer", "Spec"]);
    }

    #[test]
    fn a_review_without_a_verdict_sends_the_card_back_to_coding() {
        let harness = Arc::new(FakeHarness::with_reviewer(&["Looks fine to me."]));
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
        for _ in 0..4 {
            engine.settle(card.id).unwrap();
        }
        assert_eq!(column(&app, &card), "In progress");
        assert_eq!(
            harness.notes.lock().unwrap()[3].as_deref(),
            Some(
                "Standards (does the change follow the repository's standards):\nThe Standards \
                 review gave no verdict."
            )
        );
    }

    #[test]
    fn a_failed_review_is_tried_again_until_the_round_limit() {
        let harness = Arc::new(FakeHarness::with_reviewer(&[]));
        let (_dir, app, engine, repo) = setup_with_harness(None, harness);
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
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
                runs.iter()
                    .filter(|r| r.purpose == "review-standards")
                    .count()
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
        let harness = Arc::new(FakeHarness::without_review());
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();

        app.reject_change(
            &crate::decide::Actor::first_owner(),
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

    #[test]
    fn the_reason_of_a_rejection_is_redacted_in_the_note_and_the_judgement() {
        let harness = Arc::new(FakeHarness::without_review());
        let (_dir, app, engine, repo) = setup_with_harness(None, harness);
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        let key = format!("sk-ant-api03-{}", "x".repeat(40));
        app.reject_change(
            &crate::decide::Actor::first_owner(),
            change.id,
            &format!("Remove {key}"),
        )
        .unwrap();
        let note = app.store.card(card.id).unwrap().unwrap().note;
        assert_eq!(note.as_deref(), Some("Remove [redacted:anthropic-key]"));
        let judged = app.store.judgements_for_card(card.id).unwrap();
        assert!(
            judged.iter().all(|j| !j.reason.contains("sk-ant")),
            "{judged:?}"
        );
        assert!(
            judged
                .iter()
                .any(|j| j.reason == "Remove [redacted:anthropic-key]")
        );
    }

    /// Commits `text` as `path` on the repository's default branch, as someone merging elsewhere would.
    fn commit_to_base(app: &App, repo: i64, path: &str, text: &str) {
        let clone = std::path::PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        std::fs::create_dir_all(clone.join(path).parent().unwrap()).unwrap();
        std::fs::write(clone.join(path), text).unwrap();
        git(&clone, &["add", "."]).unwrap();
        git(&clone, &["commit", "--quiet", "-m", path]).unwrap();
    }

    /// Writes `text` as `file` of the settings of the repository `demo`, or of its home project `demo` for a script.
    fn setting(app: &App, file: &str, text: &str) {
        crate::harness::repo::testing::put(app, "demo", file, text);
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

    // Unix only: the repository's checks run with `sh`.
    #[cfg(unix)]
    #[test]
    fn failing_checks_send_the_card_back_with_their_output_and_passing_ones_let_it_on() {
        let harness = Arc::new(FakeHarness::without_review());
        let (dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let count = dir.path().join("count.txt");
        let checks = format!(
            "echo x >> {}\necho 'boom: 3 tests failed'\nexit 1\n",
            count.display()
        );
        setting(&app, "checks", &checks);
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();

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
        setting(&app, "checks", "ls run-*.txt\n");
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
        let (_dir, app, engine, repo) = setup_unreviewed();
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.checks_of(&change).unwrap(), "none");
        assert_eq!(change.checks, None);
    }

    // Unix only: the repository's checks run with `sh`.
    #[cfg(unix)]
    #[test]
    fn the_checks_are_the_projects_settings_not_a_file_of_the_change() {
        let harness = Arc::new(FakeHarness {
            files: vec![(".moebius/checks", "exit 1\n")],
            unreviewed: true,
            ..FakeHarness::default()
        });
        let (_dir, app, engine, repo) = setup_with_harness(None, harness);
        setting(&app, "checks", "exit 0\n");
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
        engine.settle(card.id).unwrap();
        engine.tick(card.id).unwrap();
        engine.tick(card.id).unwrap();
        wait_for_checks(&app, &card);
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
        let change = app.store.changes(repo).unwrap().pop().unwrap();
        assert_eq!(app.checks_of(&change).unwrap(), "success");
    }

    /// Makes the project `Ops`, which shares the repository `repo` with its home project `demo`, and puts `cards` on
    /// its board.
    fn on_a_second_project(app: &App, repo: i64, cards: &[&Card]) -> i64 {
        let ops = app.add_project("Ops", "OPS").unwrap().id;
        app.store.link_repository(ops, repo).unwrap();
        for card in cards {
            app.store.set_card_project(card.id, ops).unwrap();
        }
        ops
    }

    #[test]
    fn two_projects_that_share_a_repository_run_their_own_scripts() {
        let (_dir, app, engine, repo) = setup();
        let home = app.create_issue(repo, "At home", "", OWNER_AUTHOR).unwrap();
        let other = app
            .create_issue(repo, "Elsewhere", "", OWNER_AUTHOR)
            .unwrap();
        let late = app.create_issue(repo, "Later", "", OWNER_AUTHOR).unwrap();
        let ops = on_a_second_project(&app, repo, &[&other, &late]);
        assert_eq!(app.card(other.id).unwrap().project_id, ops);
        setting(
            &app,
            "ready.luau",
            "function on_card(card) moebius.wait('the way of demo') end",
        );
        crate::harness::repo::testing::put(
            &app,
            "Ops",
            "ready.luau",
            "function on_card(card) moebius.wait('the way of Ops') end",
        );
        let step = engine.settle(home.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("the way of demo"));
        let step = engine.settle(other.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("the way of Ops"));
        // A column the project has no script for runs the built-in one, whatever the other project has.
        setting(
            &app,
            "backlog.luau",
            "function on_card(card) moebius.wait('only demo') end",
        );
        engine.settle(late.id).unwrap();
        assert_eq!(column(&app, &late), "Not started");
    }

    // Unix only: the repository's checks run with `sh`.
    #[cfg(unix)]
    #[test]
    fn a_card_of_another_project_is_checked_with_its_repositorys_checks() {
        let harness = Arc::new(FakeHarness::without_review());
        let (_dir, app, engine, repo) = setup_with_harness(None, harness);
        setting(&app, "checks", "echo 'the checks of demo'\nexit 1\n");
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
        on_a_second_project(&app, repo, &[&card]);
        engine.settle(card.id).unwrap();
        engine.tick(card.id).unwrap();
        engine.tick(card.id).unwrap();
        wait_for_checks(&app, &card);
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert!(
            note(&app, &card)
                .unwrap()
                .starts_with("The checks failed:\nthe checks of demo")
        );
    }

    #[test]
    fn a_change_that_no_longer_merges_goes_back_with_the_conflicting_files() {
        let harness = Arc::new(FakeHarness::default());
        let (_dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
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
        let (_dir, app, engine, repo) = setup_unreviewed();
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
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

    /// The end of a summary that names the reproduction test of the tests below.
    const REPRODUCE: &str = "Done.\nREPRODUCE: sh tests/repro.sh\nREPRODUCE-FILE: tests/repro.sh\n";

    /// Ticks the card until it no longer waits for its reproduction test.
    fn settle_test(engine: &crate::board::Engine, card: &Card) -> crate::board::Step {
        let give_up = Instant::now() + Duration::from_secs(60);
        loop {
            let step = engine.tick(card.id).unwrap();
            if step.waiting.as_deref() != Some("running the reproduction test") {
                return step;
            }
            assert!(Instant::now() < give_up, "the test never ended");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// A card of a repository that works test first, whose every round commits `files` and ends with `summary`, coded
    /// and moved to review.
    fn test_first_card(
        files: Vec<(&'static str, &'static str)>,
        summary: &'static str,
        reviewer: bool,
    ) -> (
        tempfile::TempDir,
        Arc<App>,
        crate::board::Engine,
        Card,
        Arc<FakeHarness>,
    ) {
        let harness = Arc::new(FakeHarness {
            files,
            summary,
            unreviewed: !reviewer,
            ..FakeHarness::default()
        });
        let (dir, app, engine, repo) = setup_with_harness(None, harness.clone());
        setting(&app, "test-first", "");
        let card = app
            .create_issue(repo, "Fix a bug", "", OWNER_AUTHOR)
            .unwrap();
        engine.settle(card.id).unwrap();
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        (dir, app, engine, card, harness)
    }

    // Unix only: the reproduction test is run with `sh`.
    #[cfg(unix)]
    #[test]
    fn test_first_a_test_that_fails_at_the_base_and_passes_on_the_change_lets_the_card_on() {
        // The test looks for the file each round adds, which the base commit does not have.
        let (_dir, app, engine, card, _) =
            test_first_card(vec![("tests/repro.sh", "ls run-*.txt\n")], REPRODUCE, false);
        let step = engine.tick(card.id).unwrap();
        assert_eq!(
            step.waiting.as_deref(),
            Some("running the reproduction test")
        );
        settle_test(&engine, &card);
        assert_eq!(
            column(&app, &card),
            "Ready to merge",
            "{:?}",
            note(&app, &card)
        );
    }

    // Unix only: the reproduction test is run with `sh`.
    #[cfg(unix)]
    #[test]
    fn test_first_a_test_that_does_not_fail_at_the_base_sends_the_card_back_saying_so() {
        let (_dir, app, engine, card, harness) = test_first_card(
            vec![("tests/repro.sh", "echo always fine\n")],
            REPRODUCE,
            false,
        );
        settle_test(&engine, &card);
        assert_eq!(column(&app, &card), "In progress");
        let note = note(&app, &card).unwrap();
        assert!(
            note.starts_with(
                "Test first: the reproduction test `sh tests/repro.sh` does not hold."
            ),
            "{note}"
        );
        assert!(
            note.contains("It passes on the commit the change starts from"),
            "{note}"
        );
        assert!(note.contains("always fine"), "{note}");
        assert!(!note.contains("on the change"), "{note}");
        engine.tick(card.id).unwrap();
        assert_eq!(harness.notes.lock().unwrap()[1].as_deref(), Some(&*note));
    }

    #[test]
    fn test_first_a_change_without_a_reproduction_test_goes_back_and_one_that_cannot_be_tested_is_judged()
     {
        let (_dir, app, engine, card, _) = test_first_card(vec![], "Done.", false);
        engine.tick(card.id).unwrap();
        assert_eq!(column(&app, &card), "In progress");
        assert!(
            note(&app, &card)
                .unwrap()
                .contains("the change names no reproduction test")
        );

        let (_dir, app, engine, card, harness) = test_first_card(
            vec![],
            "Done.\nREPRODUCE: untestable: only the README changes\n",
            true,
        );
        let step = engine.tick(card.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("reviewing the standards"));
        let step = engine.tick(card.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("reviewing the spec"));
        let review_note = harness.notes.lock().unwrap()[2].clone().unwrap();
        assert!(
            review_note.contains("only the README changes"),
            "{review_note}"
        );
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
    }

    #[test]
    fn without_test_first_the_reproduction_test_is_not_asked_for() {
        let (_dir, app, engine, repo) = setup_unreviewed();
        let card = app
            .create_issue(repo, "Fix a bug", "", OWNER_AUTHOR)
            .unwrap();
        engine.settle(card.id).unwrap();
        engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "Ready to merge");
    }

    const DAY: i64 = 86_400;

    /// A card whose last move was `days` days ago.
    fn aged(app: &App, card: &Card, days: i64) {
        app.store
            .set_card_moved_at(card.id, crate::store::now() - days * DAY)
            .unwrap();
    }

    #[test]
    fn a_card_waiting_in_the_backlog_for_a_month_is_held_and_a_younger_one_is_not() {
        let (_dir, app, engine, repo) = setup_unreviewed();
        let young = app.create_issue(repo, "Young", "", "stranger").unwrap();
        let old = app.create_issue(repo, "Old", "", "stranger").unwrap();
        assert_eq!(column(&app, &old), "Backlog");
        aged(&app, &young, 29);
        aged(&app, &old, 31);

        engine.tick(young.id).unwrap();
        assert!(app.card(young.id).unwrap().held.is_none());
        let step = engine.tick(old.id).unwrap();
        let held = app.card(old.id).unwrap().held.unwrap();
        assert_eq!(held.by, "script:Backlog");
        assert_eq!(
            held.reason.as_deref(),
            Some("stale: it has not moved for 30 days")
        );
        assert_eq!(step.waiting, held.reason);
        assert_eq!(column(&app, &old), "Backlog", "it stays in its column");
        // Held, the board leaves it alone until a person releases it, which starts its time again.
        assert_eq!(engine.tick(old.id).unwrap(), Default::default());
        app.unblock_card(&crate::decide::Actor::first_owner(), old.id, "still wanted")
            .unwrap();
        engine.tick(old.id).unwrap();
        assert!(app.card(old.id).unwrap().held.is_none());

        // Stale again later: the people are told again, since the release resolved the first item.
        aged(&app, &old, 31);
        engine.tick(old.id).unwrap();
        assert!(app.card(old.id).unwrap().held.is_some());
        let open = |app: &App| {
            app.store
                .inbox()
                .unwrap()
                .iter()
                .filter(|item| item.kind == "stale" && item.resolved_at.is_none())
                .count()
        };
        assert_eq!(open(&app), 1);
    }

    #[test]
    fn a_parent_waiting_for_its_sub_issues_is_not_held_as_stale() {
        let (_dir, app, engine, repo) = setup_unreviewed();
        let parent = app.create_issue(repo, "Plan", "", OWNER_AUTHOR).unwrap();
        let child = app.create_issue(repo, "Step", "", OWNER_AUTHOR).unwrap();
        app.store.set_parent(child.id, Some(parent.number)).unwrap();
        app.move_card(parent.id, "Not started", None).unwrap();
        aged(&app, &parent, 90);
        engine.tick(parent.id).unwrap();
        assert!(app.card(parent.id).unwrap().held.is_none());
    }

    #[test]
    fn a_stale_card_tells_the_people_it_is_given_to_once() {
        use crate::store::Role;
        let (_dir, app, engine, repo) = setup_unreviewed();
        let lea = app.store.add_person("lea", "Lea", Role::Member).unwrap();
        let card = app.create_issue(repo, "Mine", "", OWNER_AUTHOR).unwrap();
        app.move_card(card.id, "Not started", None).unwrap();
        app.store
            .set_card_assignees(card.id, &std::collections::BTreeSet::from([lea.id]))
            .unwrap();
        aged(&app, &card, 45);

        engine.tick(card.id).unwrap();
        let inbox = app.store.inbox().unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].kind, "stale");
        assert_eq!(inbox[0].card_id, Some(card.id));
        assert_eq!(inbox[0].assignees, ["lea"]);
        assert!(app.card(card.id).unwrap().held.is_some());
    }

    #[test]
    fn a_card_done_for_two_weeks_is_archived_and_stays_in_the_issue_list() {
        use crate::app::IssueQuery;
        let (_dir, app, engine, repo) = setup_unreviewed();
        let fresh = app.create_issue(repo, "Fresh", "", OWNER_AUTHOR).unwrap();
        let old = app.create_issue(repo, "Old", "", OWNER_AUTHOR).unwrap();
        for card in [&fresh, &old] {
            app.move_card(card.id, "Done", None).unwrap();
        }
        aged(&app, &fresh, 13);
        aged(&app, &old, 15);

        engine.tick(fresh.id).unwrap();
        engine.tick(old.id).unwrap();
        assert_eq!(app.card(fresh.id).unwrap().archived_at, None);
        assert!(app.card(old.id).unwrap().archived_at.is_some());
        assert_eq!(column(&app, &old), "Done", "it is not moved or deleted");

        // Archived, it is still found: the issue list has it, and filters by it.
        let titles = |archived| {
            app.issues(
                repo,
                &IssueQuery {
                    archived,
                    ..Default::default()
                },
            )
            .unwrap()
            .into_iter()
            .map(|issue| issue.card.title)
            .collect::<Vec<_>>()
        };
        assert_eq!(titles(None), ["Fresh", "Old"]);
        assert_eq!(titles(Some(true)), ["Old"]);
        assert_eq!(titles(Some(false)), ["Fresh"]);
    }

    #[test]
    fn a_script_archives_only_its_own_card_and_a_move_brings_an_archived_card_back() {
        let (_dir, app, engine, repo) = setup_unreviewed();
        let card = app.create_issue(repo, "Mine", "", OWNER_AUTHOR).unwrap();
        let project = app.harness_repo().unwrap().project_dir("demo");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("backlog.luau"),
            "function on_card(card) moebius.archive({ id = card.id + 1 }) end",
        )
        .unwrap();
        let error = engine.tick(card.id).unwrap_err().to_string();
        assert!(error.contains("its own card"), "{error}");

        assert!(app.archive_card(card.id).unwrap());
        assert!(!app.archive_card(card.id).unwrap(), "once");
        app.move_card(card.id, "Backlog", None).unwrap();
        assert_eq!(app.card(card.id).unwrap().archived_at, None);
    }
}
