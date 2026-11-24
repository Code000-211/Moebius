//! The tests of the run lifecycle every harness shares: a run is recorded, given a worktree and a prompt, and ended
//! with its work committed, its worktree removed and its status set; what a run of a card is told; and the hint of the
//! files likely to matter with the outcome it becomes. Each test that can runs against Pi-Bolt and Claude Code alike,
//! on the fake of each process.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use super::lifecycle::write_untracked;
use super::prompts::{prompt_for, run_prompt};
use super::testing::{
    Setup, prompt_of, setup, setup_localizing, setup_with, wait_for_events, wait_until_ended,
};
use crate::board::{CardRuns, StartError, Variant};
use crate::repos::git;

#[test]
fn files_written_into_a_worktree_stay_out_of_git_and_inside_it() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet"]).unwrap();
    let files = BTreeMap::from([
        (".retro/run-1.events.jsonl".to_string(), "{}\n".to_string()),
        (".retro/run-1.diff".to_string(), "diff\n".to_string()),
    ]);
    write_untracked(dir.path(), &files).unwrap();
    write_untracked(dir.path(), &files).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".retro/run-1.diff")).unwrap(),
        "diff\n"
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]).unwrap(), "");
    let exclude = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
    assert_eq!(exclude.matches("/.retro\n").count(), 1, "{exclude}");
    let outside = BTreeMap::from([("../x".to_string(), String::new())]);
    assert!(write_untracked(dir.path(), &outside).is_err());
}

#[test]
fn a_code_run_commits_what_the_agent_left_and_removes_its_worktree() {
    let setup = setup();
    for (name, runs) in setup.both() {
        let card = setup
            .app
            .create_issue(setup.repo, "Write done.txt", "", "person")
            .unwrap();
        let run = runs
            .start(
                &setup.app,
                &card,
                "Coder",
                ("code", None),
                Variant::default(),
            )
            .unwrap();
        assert_eq!(run.status, "running", "{name}");
        let run = wait_until_ended(&setup.app, run.id);
        assert_eq!(run.status, "succeeded", "{name}");
        let clone = PathBuf::from(
            &setup
                .app
                .store
                .repository(setup.repo)
                .unwrap()
                .unwrap()
                .path,
        );
        assert_eq!(
            git(&clone, &["show", &format!("{}:done.txt", run.branch())]).unwrap(),
            "done",
            "{name}"
        );
        assert!(!PathBuf::from(run.worktree.unwrap()).exists(), "{name}");
    }
}

#[test]
fn a_run_that_changed_nothing_has_failed() {
    let setup = setup();
    for (name, runs) in setup.both() {
        let card = setup
            .app
            .create_issue(setup.repo, "Do [nothing]", "", "person")
            .unwrap();
        let run = runs
            .start(
                &setup.app,
                &card,
                "Coder",
                ("code", None),
                Variant::default(),
            )
            .unwrap();
        let run = wait_until_ended(&setup.app, run.id);
        assert_eq!(run.status, "failed", "{name}");
        let last = setup.app.store.events(run.id).unwrap().pop().unwrap();
        assert_eq!(last.data["message"], "the agent changed nothing", "{name}");
    }
}

#[test]
fn a_run_can_be_stopped() {
    let setup = setup();
    for (name, runs) in setup.both() {
        let card = setup
            .app
            .create_issue(setup.repo, "Please [hang]", "", "person")
            .unwrap();
        let run = runs
            .start(
                &setup.app,
                &card,
                "Coder",
                ("code", None),
                Variant::default(),
            )
            .unwrap();
        // Wait until the process has taken the prompt.
        wait_for_events(&setup.app, run.id, 2);
        assert!(runs.stop(run.id), "{name}");
        assert_eq!(
            wait_until_ended(&setup.app, run.id).status,
            "stopped",
            "{name}"
        );
        assert!(!runs.stop(run.id), "{name}");
    }
}

#[test]
fn a_run_is_given_a_scratch_directory_and_loses_it_when_it_ends() {
    let setup = setup();
    for (name, runs) in setup.both() {
        let card = setup
            .app
            .create_issue(setup.repo, "Please [hang]", "", "person")
            .unwrap();
        let run = runs
            .start(
                &setup.app,
                &card,
                "Coder",
                ("code", None),
                Variant::default(),
            )
            .unwrap();
        wait_for_events(&setup.app, run.id, 2);
        let scratch = super::scratch::dir(&setup.app.data_dir, run.id);
        assert!(scratch.is_dir(), "{name}");
        // The run is told where its scratch is and keeps its temporary files there, so that they are deleted with it.
        let env = match name {
            "pi-bolt" => format!("runs/{}/agent/env.json", run.id),
            _ => format!("runs/{}/claude/env.json", run.id),
        };
        let env: Value =
            serde_json::from_str(&std::fs::read_to_string(setup.app.data_dir.join(env)).unwrap())
                .unwrap();
        assert_eq!(
            env["MOEBIUS_RUN_SCRATCH"],
            scratch.to_str().unwrap(),
            "{name}"
        );
        assert_eq!(env["TMPDIR"], scratch.to_str().unwrap(), "{name}");
        // What the run leaves there goes with it when it ends, whether it ends by itself or is stopped.
        std::fs::create_dir_all(scratch.join("build/cache")).unwrap();
        assert!(runs.stop(run.id), "{name}");
        wait_until_ended(&setup.app, run.id);
        assert!(!scratch.exists(), "{name}");
    }
}

#[test]
fn a_review_run_starts_from_the_code_branch_and_commits_nothing() {
    let setup = setup();
    for (name, runs) in setup.both() {
        let card = setup
            .app
            .create_issue(setup.repo, "Write done.txt", "", "person")
            .unwrap();
        let code = runs
            .start(
                &setup.app,
                &card,
                "Coder",
                ("code", None),
                Variant::default(),
            )
            .unwrap();
        let code = wait_until_ended(&setup.app, code.id);
        assert_eq!(code.status, "succeeded", "{name}");

        let review = runs
            .start(
                &setup.app,
                &card,
                "Reviewer",
                ("review", Some("Check the edge.")),
                Variant::default(),
            )
            .unwrap();
        let review = wait_until_ended(&setup.app, review.id);
        // Nothing was changed, and that is no failure for a review.
        assert_eq!(review.status, "succeeded", "{name}");
        assert_eq!(
            review.summary.as_deref(),
            Some("Looks fine.\nVERDICT: approve"),
            "{name}"
        );
        let clone = PathBuf::from(
            &setup
                .app
                .store
                .repository(setup.repo)
                .unwrap()
                .unwrap()
                .path,
        );
        assert_eq!(
            git(&clone, &["rev-parse", &review.branch()]).unwrap(),
            git(&clone, &["rev-parse", &code.branch()]).unwrap(),
            "{name}"
        );
        assert_eq!(
            setup.app.store.code_branch(card.id).unwrap(),
            Some(code.branch()),
            "{name}"
        );
        let prompt = prompt_of(&setup.app, &review);
        assert!(
            prompt.contains(&format!("Work on issue #{}: Write done.txt", card.number)),
            "{name}"
        );
        assert!(prompt.contains("VERDICT: approve"), "{name}");
        assert!(
            prompt.contains("Notes from the last round:\n\nCheck the edge."),
            "{name}"
        );
        assert!(
            prompt_of(&setup.app, &code).contains("Moebius commits what you leave"),
            "{name}"
        );
    }
}

#[test]
fn a_run_that_committed_its_own_work_has_succeeded() {
    let setup = setup();
    // Only the fake Pi commits on its own; the fake Claude Code leaves its write uncommitted.
    let runs = setup.harnesses.pi_bolt.as_ref();
    let card = setup
        .app
        .create_issue(setup.repo, "Do [commit]", "", "person")
        .unwrap();
    let run = runs
        .start(
            &setup.app,
            &card,
            "Coder",
            ("code", None),
            Variant::default(),
        )
        .unwrap();
    let run = wait_until_ended(&setup.app, run.id);
    assert_eq!(run.status, "succeeded");
    let clone = PathBuf::from(
        &setup
            .app
            .store
            .repository(setup.repo)
            .unwrap()
            .unwrap()
            .path,
    );
    assert_eq!(
        git(&clone, &["show", &format!("{}:own.txt", run.branch())]).unwrap(),
        "own"
    );
}

#[test]
fn an_unknown_agent_starts_no_run() {
    let setup = setup();
    for (name, runs) in setup.both() {
        let card = setup
            .app
            .create_issue(setup.repo, "Anything", "", "person")
            .unwrap();
        let error = runs
            .start(
                &setup.app,
                &card,
                "Nobody",
                ("code", None),
                Variant::default(),
            )
            .unwrap_err();
        assert!(
            matches!(error, StartError::UnknownAgent(_)),
            "{name}: {error}"
        );
        assert!(
            setup.app.store.runs_for_card(card.id).unwrap().is_empty(),
            "{name}"
        );
    }
}

#[test]
fn a_missing_program_fails_the_run_and_leaves_nothing() {
    let missing = Path::new("/nonexistent/harness");
    let setup = setup_with(missing, missing, false);
    for (name, runs) in setup.both() {
        let card = setup
            .app
            .create_issue(setup.repo, "Anything", "", "person")
            .unwrap();
        let error = runs
            .start(
                &setup.app,
                &card,
                "Coder",
                ("code", None),
                Variant::default(),
            )
            .unwrap_err();
        assert!(
            matches!(error, StartError::LaunchFailed(_)),
            "{name}: {error}"
        );
        let run = setup
            .app
            .store
            .runs_for_card(card.id)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(run.status, "failed", "{name}");
        assert!(!PathBuf::from(run.worktree.unwrap()).exists(), "{name}");
        let scratch = super::scratch::dir(&setup.app.data_dir, run.id);
        assert!(!scratch.exists(), "{name}: {scratch:?} outlived the run");
    }
}

#[test]
fn the_review_axes_are_told_their_rules_and_how_to_report_findings() {
    let setup = setup();
    let card = setup
        .app
        .create_issue(setup.repo, "Fix a bug", "", "person")
        .unwrap();
    let standards = prompt_for(&setup.app, &card, "review-standards", None);
    assert!(standards.contains("You are the reviewer"), "{standards}");
    for word in [
        "CONTRIBUTING.md",
        "Possible <smell>",
        "Feature Envy",
        "EVIDENCE:",
        "git diff main...HEAD",
    ] {
        assert!(standards.contains(word), "{word}: {standards}");
    }
    // The project's review notes are told to the standards axis, in the prompt itself.
    assert!(
        !standards.contains("review notes of this repository"),
        "{standards}"
    );
    crate::harness::repo::testing::put(&setup.app, "demo", "review.md", "Use British spelling.\n");
    let standards = prompt_for(&setup.app, &card, "review-standards", None);
    assert!(standards.contains("Use British spelling."), "{standards}");
    let spec = prompt_for(&setup.app, &card, "review-spec", None);
    assert!(!spec.contains("British"), "{spec}");
    assert!(spec.contains("what the issue asks"), "{spec}");
    assert!(spec.contains("FINDING blocking"), "{spec}");
    assert!(!spec.contains("Feature Envy"), "{spec}");
}

#[test]
fn a_code_run_in_a_repository_that_works_test_first_is_told_red_then_green() {
    let setup = setup();
    let card = setup
        .app
        .create_issue(setup.repo, "Fix a bug", "", "person")
        .unwrap();
    assert!(!prompt_for(&setup.app, &card, "code", None).contains("red, then green"));

    crate::harness::repo::testing::put(&setup.app, "demo", "test-first", "");
    let prompt = prompt_for(&setup.app, &card, "code", None);
    assert!(prompt.contains("red, then green"), "{prompt}");
    assert!(prompt.contains("`REPRODUCE: <the command"), "{prompt}");
    assert!(!prompt.contains(".moebius"), "{prompt}");
    assert!(!prompt_for(&setup.app, &card, "review", None).contains("red, then green"));
}

#[test]
fn a_code_or_review_run_is_given_the_lessons_that_match_its_issue() {
    let setup = setup();
    let card = setup
        .app
        .create_issue(
            setup.repo,
            "Fix the tokenizer",
            "It breaks on emoji",
            "person",
        )
        .unwrap();
    assert!(!prompt_for(&setup.app, &card, "code", None).contains("Lessons from earlier cards"));
    setup
        .app
        .store
        .add_lesson(&crate::store::NewLesson {
            repository_id: setup.repo,
            kind: "rejection".into(),
            situation: "Changing the tokenizer".into(),
            lesson: "The reviewer found no test for emoji".into(),
            advice: "Add a test with a multi-byte character".into(),
            cards: vec![card.id],
            retro_date: None,
        })
        .unwrap();
    let code = prompt_for(&setup.app, &card, "code", None);
    assert!(code.contains("Lessons from earlier cards"), "{code}");
    assert!(code.contains("Do: Add a test with a multi-byte character"));
    // After the issue and the instructions, before the notes of the last round.
    let note = prompt_for(&setup.app, &card, "code", Some("Fix it."));
    assert!(note.find("Lessons from").unwrap() < note.find("Notes from the last round").unwrap());
    assert!(
        prompt_for(&setup.app, &card, "review-spec", None).contains("Lessons from earlier cards")
    );
    let other = setup
        .app
        .create_issue(setup.repo, "Paint the button", "Make it red", "person")
        .unwrap();
    assert!(!prompt_for(&setup.app, &other, "code", None).contains("Lessons from earlier cards"));
}

/// Commits `files` (path and text) in the clone of `setup`'s repository.
fn commit_files(setup: &Setup, files: &[(&str, &str)]) -> PathBuf {
    let clone = PathBuf::from(
        &setup
            .app
            .store
            .repository(setup.repo)
            .unwrap()
            .unwrap()
            .path,
    );
    for (path, text) in files {
        let file = clone.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    git(&clone, &["add", "."]).unwrap();
    git(&clone, &["commit", "--quiet", "-m", "files"]).unwrap();
    clone
}

#[test]
fn a_code_run_is_given_the_likely_files_and_the_files_its_change_touched_become_the_outcome() {
    let setup = setup_localizing();
    commit_files(
        &setup,
        &[
            ("done.txt", "old\n"),
            ("src/render.rs", "fn draw() {}\n"),
            ("src/parser.rs", "fn parse() {}\n"),
        ],
    );
    for (name, runs) in setup.both() {
        let card = setup
            .app
            .create_issue(setup.repo, "Write done.txt", "Its text is wrong.", "person")
            .unwrap();
        let run = runs
            .start(
                &setup.app,
                &card,
                "Coder",
                ("code", None),
                Variant::default(),
            )
            .unwrap();
        let run = wait_until_ended(&setup.app, run.id);
        assert_eq!(run.status, "succeeded", "{name}");

        let prompt = prompt_of(&setup.app, &run);
        assert!(
            prompt.contains(
                "Files that are likely relevant (a hint, not a limit):\n\n- done.txt: named in the issue"
            ),
            "{name}: {prompt}"
        );
        assert!(
            prompt.contains("Verify them before you rely on them"),
            "{name}"
        );

        let hints = setup.app.store.judgements("localize").unwrap();
        let hint = hints
            .iter()
            .find(|hint| hint.card_id == Some(card.id))
            .unwrap();
        assert!(
            hint.choice.starts_with("[{\"path\":\"done.txt\""),
            "{}",
            hint.choice
        );
        let outcome: crate::decide::localize::Outcome =
            serde_json::from_str(hint.outcome.as_deref().unwrap()).unwrap();
        assert_eq!(outcome.touched, ["done.txt"], "{name}");
        assert_eq!(
            (outcome.recall, outcome.precision),
            (Some(1.0), Some(1.0)),
            "{name}"
        );
    }
}

#[test]
fn a_repository_can_turn_the_hint_off_for_its_own_workflow() {
    let setup = setup_localizing();
    let card = setup
        .app
        .create_issue(setup.repo, "Change README.md", "", "person")
        .unwrap();
    assert!(run_prompt(&setup.app, &card, "code", None).contains("- README.md"));
    // Only a code run asks.
    assert!(!run_prompt(&setup.app, &card, "review", None).contains("likely relevant"));
    crate::harness::repo::testing::put(&setup.app, "demo", "localize", "# nothing here\noff\n");
    assert!(!run_prompt(&setup.app, &card, "code", None).contains("likely relevant"));
    crate::harness::repo::testing::put(&setup.app, "demo", "localize", "on\nfiles 1\n");
    let prompt = run_prompt(&setup.app, &card, "code", None);
    assert_eq!(prompt.matches("\n- ").count(), 1, "{prompt}");
    // Two hints were asked for and recorded, and the one that was not asked for left no judgement.
    assert_eq!(setup.app.store.judgements("localize").unwrap().len(), 2);
}

#[test]
fn the_localizer_turned_off_gives_runs_no_hint_and_no_judgement_and_on_again_gives_it_back() {
    use crate::decide::localize::KIND;
    let setup = setup_localizing();
    let card = setup
        .app
        .create_issue(setup.repo, "Change README.md", "", "person")
        .unwrap();
    assert!(run_prompt(&setup.app, &card, "code", None).contains("- README.md"));
    let asked = || setup.app.store.judgements("localize").unwrap().len();
    assert_eq!(asked(), 1);
    // Off, as `localize.enabled = false` is: the run starts without a hint and nothing is judged. A project that
    // says `on` does not bring it back.
    setup.app.decisions.unroute_kind(KIND);
    crate::harness::repo::testing::put(&setup.app, "demo", "localize", "on\n");
    assert!(!run_prompt(&setup.app, &card, "code", None).contains("likely relevant"));
    assert_eq!(asked(), 1);
    setup.app.decisions.route(KIND, KIND);
    assert!(run_prompt(&setup.app, &card, "code", None).contains("- README.md"));
    assert_eq!(asked(), 2);
}

#[test]
fn a_decider_that_fails_leaves_the_run_without_a_hint() {
    let setup = setup();
    setup
        .app
        .decisions
        .route(crate::decide::localize::KIND, "nobody");
    let card = setup
        .app
        .create_issue(setup.repo, "Change README.md", "", "person")
        .unwrap();
    let prompt = run_prompt(&setup.app, &card, "code", None);
    assert!(!prompt.contains("likely relevant"), "{prompt}");
    assert!(prompt.contains("Moebius commits what you leave"));
    // The failure is recorded as the decision point records any: the fail-closed answer, and why.
    let hints = setup.app.store.judgements("localize").unwrap();
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].choice, "[]");
    assert!(
        hints[0].reason.starts_with("refused: "),
        "{}",
        hints[0].reason
    );
}

/// What a review run of a pull request someone else opened is started with, on either harness: recorded restricted on
/// its row, a checked token, no MCP servers, no runner, and no checkout: its working directory is an empty directory
/// and it has no clone. The same review of a card that follows no such request is none of that.
/// Whether the directory is one only its owner may enter (anything but Unix has no such mode to read).
fn private_to_its_owner(dir: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(dir).is_ok_and(|m| m.permissions().mode() & 0o077 == 0)
    }
    #[cfg(not(unix))]
    {
        dir.is_dir()
    }
}

#[test]
fn a_review_of_a_pull_request_someone_else_opened_is_started_restricted_on_either_harness() {
    use crate::config::HarnessKind;
    use crate::harness::remote::Runners;
    use crate::harness::run::start_card;
    use crate::router::Router;

    let setup = setup();
    let router = Router::new(setup.app.store.clone(), vec![]);
    let runners = Arc::new(Runners::default());
    let repository = setup.app.store.repository(setup.repo).unwrap().unwrap();
    let clone = PathBuf::from(&repository.path);
    // The pull request's branch has a file, and a link that leads out of any worktree.
    git(&clone, &["checkout", "--quiet", "-b", "moebius/pr-7"]).unwrap();
    std::fs::write(clone.join("feature.txt"), "feature\n").unwrap();
    git(&clone, &["add", "."]).unwrap();
    git(
        &clone,
        &[
            "-c",
            "user.name=alice",
            "-c",
            "user.email=alice@example.com",
            "commit",
            "--quiet",
            "-m",
            "a feature",
        ],
    )
    .unwrap();
    let head = git(&clone, &["rev-parse", "HEAD"]).unwrap();
    git(&clone, &["checkout", "--quiet", "main"]).unwrap();
    let followed = setup
        .app
        .create_issue(setup.repo, "Followed", "", "person")
        .unwrap();
    let change = setup
        .app
        .store
        .add_change(
            setup.repo,
            Some(followed.id),
            "moebius/pr-7",
            Some(7),
            &head,
        )
        .unwrap();
    setup
        .app
        .store
        .set_change_outside(change.id, "alice", "In progress")
        .unwrap();
    setup
        .app
        .store
        .set_change_reviewed_head(change.id, &head)
        .unwrap();
    let base = git(&clone, &["rev-parse", "main"]).unwrap();
    setup
        .app
        .store
        .set_change_reviewed_base(change.id, &base)
        .unwrap();
    // The card has an earlier code run of Moebius's own: the review is of the pull request all the same.
    let earlier = setup
        .app
        .store
        .add_run(Some(followed.id), "Coder", "idle", "code")
        .unwrap();
    setup
        .app
        .store
        .set_run_status(earlier.id, "succeeded")
        .unwrap();
    let own = setup
        .app
        .create_issue(setup.repo, "Own", "", "person")
        .unwrap();

    for harness in [HarnessKind::PiBolt, HarnessKind::ClaudeCode] {
        for (card, restricted) in [(&followed, true), (&own, false)] {
            let mut mcp = BTreeMap::new();
            mcp.insert(
                "search".to_string(),
                crate::harness::mcp::McpServer {
                    command: None,
                    args: vec![],
                    env: BTreeMap::new(),
                    url: Some("https://search.test/mcp".into()),
                    headers: BTreeMap::new(),
                },
            );
            let config = crate::config::AgentConfig {
                model: "qwen-local".into(),
                protocol: crate::router::Protocol::Anthropic,
                thinking: None,
                instructions: None,
                skills: vec![],
                extensions: vec![],
                mcp,
                token_budget: None,
                harness,
                tool_output: Default::default(),
                engineering_skills: Default::default(),
                runner: restricted.then(|| "far-away".to_string()),
            };
            let seen = std::cell::RefCell::new(None);
            let started = start_card(
                &setup.app,
                (&router, &runners),
                card,
                ("Reviewer", config),
                ("review", None),
                Variant::default(),
                &|launching| {
                    let prompt = launching.prompt.unwrap_or_default().to_string();
                    *seen.borrow_mut() = Some((
                        launching.run.restricted,
                        router.is_checked(&launching.run.token),
                        launching.config.mcp.len(),
                        launching.config.runner.clone(),
                        launching.run.clone.is_some(),
                        std::fs::read_dir(&launching.run.worktree).map(|d| d.count()),
                        launching.run.worktree.clone(),
                        setup.app.store.run_restricted(launching.run.id).unwrap(),
                        prompt,
                        launching.run.id,
                        private_to_its_owner(&launching.run.worktree),
                    ));
                    Err("not started: the test only looks".into())
                },
            );
            let name = format!("{harness:?}, restricted {restricted}");
            assert!(started.is_err(), "{name}");
            let (
                marked,
                checked,
                servers,
                runner,
                has_clone,
                files,
                worktree,
                stored,
                prompt,
                id,
                private,
            ) = seen.into_inner().expect("the run was launched");
            if !restricted {
                // Moebius's own change is reviewed as before: a worktree of its clone, its servers, and its token as
                // its harness has it (Claude Code's calls are checked, Pi-Bolt's are not).
                assert!(!marked && !stored, "{name}");
                assert_eq!(checked, harness == HarnessKind::ClaudeCode, "{name}");
                assert_eq!(servers, 1, "{name}");
                assert!(has_clone, "{name}");
                assert!(!prompt.contains("You have no tools at all"), "{name}");
                continue;
            }
            assert!(marked && stored, "{name}");
            // The two commits it reads are on its row.
            assert_eq!(
                setup.app.store.run_review(id).unwrap(),
                Some((head.clone(), base.clone())),
                "{name}"
            );
            // Its name is not one another person on the machine can guess and make first, and only its owner enters it.
            assert!(
                worktree.file_name().unwrap().to_string_lossy()
                    != format!("moebius-review-{id}-{}", std::process::id()),
                "{name}: {worktree:?}"
            );
            assert!(private, "{name}: only its owner may enter {worktree:?}");
            assert!(checked, "{name}: a checked token on either harness");
            assert_eq!(servers, 0, "{name}");
            assert_eq!(runner, None, "{name}: never on a runner");
            assert!(!has_clone, "{name}: no clone, no checkout");
            assert_eq!(files.ok(), Some(0), "{name}: an empty directory");
            // Its own directory under the system's temporary directory, with no context file of the data directory
            // or the home directory above it; the run was undone, so it is gone.
            assert!(
                worktree.starts_with(std::env::temp_dir()),
                "{name}: {worktree:?}"
            );
            assert!(
                !worktree.starts_with(&setup.app.data_dir),
                "{name}: {worktree:?}"
            );
            assert!(!worktree.exists(), "{name}: removed with the run");
            // The prompt holds the pull request as of its commit, and no other branch: not the earlier code run's.
            assert!(prompt.contains(&head), "{name}: {prompt}");
            assert!(prompt.contains("feature.txt"), "{name}");
            assert!(prompt.contains("You have no tools at all"), "{name}");
        }
    }
}
