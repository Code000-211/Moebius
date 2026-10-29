//! Attempts: a hard card's several runs in one go (see `store::Attempt`). Each attempt that succeeded is judged on a
//! thread of its own against what the card is held to: the repository's own checks (`.moebius/checks`) and, in a
//! repository that works test first, the reproduction test the attempt names in `.moebius/reproduce`, which must fail on
//! the commit the attempt starts from and pass on the attempt (see `reproduce`). Among the attempts that pass, a script
//! has one picked, by a selection run or because only one passed; the picked attempt becomes the card's `code` run and
//! the others keep their branches. The reasons are kept with the attempts and shown in the card's history.

use std::path::Path;
use std::sync::Arc;

use serde_json::json;

use super::reproduce::{Reproduction, add_files, reproduction_at};
use super::{App, AppError, Result};
use crate::bus::AppEvent;
use crate::repos::git;
use crate::store::Repository;

/// How the judging of an attempt stands: `pending` while it goes, then `passed` or `failed`; `output` says why it
/// failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptCheck {
    pub status: &'static str,
    pub output: String,
}

/// What a selection run picked: the run id on the last line `PICK: <run id>` of its summary, and its reasons, the text
/// before that line. No pick when the summary has no such line.
pub fn read_pick(summary: Option<&str>) -> (Option<i64>, String) {
    let text = summary.unwrap_or("").trim_end();
    let (reasons, last) = text.rsplit_once('\n').unwrap_or(("", text));
    let pick = last
        .trim()
        .strip_prefix("PICK:")
        .and_then(|id| id.trim().trim_start_matches('#').parse().ok());
    match pick {
        Some(id) => (Some(id), reasons.trim().to_string()),
        None => (None, text.trim().to_string()),
    }
}

impl App {
    /// How the attempt `run_id` fares against the card's tests and checks. A run that did not succeed has failed. The
    /// judging goes on a thread of its own and ends in the store: `pending` until it has, then the same answer on
    /// every call.
    pub fn check_attempt(self: &Arc<Self>, run_id: i64) -> Result<AttemptCheck> {
        let not_an_attempt = || AppError::Invalid(format!("run {run_id} is not an attempt"));
        let judged = |attempt: crate::store::Attempt| AttemptCheck {
            status: if attempt.result.as_deref() == Some("passed") {
                "passed"
            } else {
                "failed"
            },
            output: attempt.output,
        };
        // Held while the attempt is read: judging records its result before it leaves the set, so an attempt read under
        // the lock that is not in the set and has no result has not been judged.
        let mut judging = self.judging.lock().unwrap_or_else(|e| e.into_inner());
        let attempt = self.store.attempt(run_id)?.ok_or_else(not_an_attempt)?;
        if attempt.result.is_some() {
            return Ok(judged(attempt));
        }
        if judging.contains(&run_id) {
            return Ok(pending());
        }
        let run = self.run(run_id)?;
        match run.status.as_str() {
            "starting" | "running" => return Ok(pending()),
            "succeeded" => {}
            other => {
                drop(judging);
                let output = format!("The run {other}.");
                self.record_attempt_result(attempt.card_id, run_id, "failed", &output)?;
                return Ok(AttemptCheck {
                    status: "failed",
                    output,
                });
            }
        }
        judging.insert(run_id);
        drop(judging);
        let app = self.clone();
        let card_id = attempt.card_id;
        let started = std::thread::Builder::new()
            .name(format!("judging of attempt {run_id}"))
            .spawn(move || {
                let (passed, output) = match app.judge_attempt(card_id, &run.branch(), run_id) {
                    Ok(verdict) => verdict,
                    Err(e) => (false, format!("The attempt could not be judged: {e}")),
                };
                let result = if passed { "passed" } else { "failed" };
                let _ = app.record_attempt_result(card_id, run_id, result, &output);
                app.judging
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&run_id);
            });
        if let Err(e) = started {
            // Nothing runs, so the next call tries again.
            self.judging
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&run_id);
            return Err(AppError::Invalid(format!("cannot start the judging: {e}")));
        }
        Ok(pending())
    }

    fn record_attempt_result(
        &self,
        card_id: i64,
        run_id: i64,
        result: &str,
        output: &str,
    ) -> Result<()> {
        self.store.set_attempt_result(run_id, result, output)?;
        self.bus.publish(AppEvent::new(
            "attempt_judged",
            json!({ "card": card_id, "run": run_id, "result": result }),
        ));
        Ok(())
    }

    /// Runs the repository's checks and, test first, the reproduction test on the branch of the attempt `run_id`; whether
    /// they all hold, and what did not.
    fn judge_attempt(&self, card_id: i64, branch: &str, run_id: i64) -> Result<(bool, String)> {
        let card = self.card(card_id)?;
        let repository = self.repository(card.repository_id)?;
        let clone = Path::new(&repository.path);
        let head = git(clone, &["rev-parse", branch])?;
        if let Some(script) = self.checks_script(&repository) {
            let name = format!("checks-attempt-{run_id}");
            let (succeeded, output) = self.run_script_on(&repository, &name, &head, &script);
            if !succeeded {
                return Ok((false, format!("The checks failed:\n{output}")));
            }
        }
        if !self.test_first(&repository) {
            return Ok((true, String::new()));
        }
        let base = git(clone, &["merge-base", &repository.base_ref(), &head])?;
        match reproduction_at(clone, &head, &base) {
            None => Ok((
                false,
                "Test first: the attempt names no reproduction test in .moebius/reproduce.".into(),
            )),
            // The review judges the reason, as it does for a change.
            Some(Reproduction::Untestable(why)) => Ok((true, format!("Untestable: {why}"))),
            Some(Reproduction::Test { command, files }) => {
                self.judge_reproduction(&repository, run_id, (&head, &base), (&command, &files))
            }
        }
    }

    /// Runs the test `command` on `base` with `files` as `head` has them, where it must fail, and on `head`, where it
    /// must pass.
    fn judge_reproduction(
        &self,
        repository: &Repository,
        run_id: i64,
        (head, base): (&str, &str),
        (command, files): (&str, &[String]),
    ) -> Result<(bool, String)> {
        let what = ("The test", command);
        let before = self.run_prepared(
            repository,
            (&format!("test-attempt-{run_id}-base"), base),
            |worktree| add_files(worktree, head, files),
            what,
        );
        let after = self.run_prepared(
            repository,
            (&format!("test-attempt-{run_id}-change"), head),
            |_| Ok(()),
            what,
        );
        let mut wrong = Vec::new();
        match before {
            Err(output) => wrong.push(format!(
                "It could not run on the commit the attempt starts from:\n{output}"
            )),
            Ok((true, output)) => wrong.push(format!(
                "It passes on the commit the attempt starts from, so it does not reproduce the issue; it must fail \
                 there:\n{output}"
            )),
            Ok((false, _)) => {}
        }
        match after {
            Err(output) => wrong.push(format!("It could not run on the attempt:\n{output}")),
            Ok((false, output)) => wrong.push(format!(
                "It fails on the attempt; it must pass there:\n{output}"
            )),
            Ok((true, _)) => {}
        }
        if wrong.is_empty() {
            return Ok((true, String::new()));
        }
        Ok((
            false,
            format!(
                "Test first: the reproduction test `{command}` does not hold.\n{}",
                wrong.join("\n")
            ),
        ))
    }

    /// Settles the card's open attempts: `pick`, the run of an attempt that passed, becomes the card's `code` run for the
    /// reason `text`, and the others keep their branches. Without a pick none passed: the round counts as a failed
    /// one, and `text` (a default when it is empty) is the note the next round is told. Publishes `attempts_settled`.
    pub fn settle_attempts(&self, card_id: i64, pick: Option<i64>, text: &str) -> Result<()> {
        let open = self.store.open_attempts(card_id)?;
        if open.is_empty() {
            return Err(AppError::Invalid("the card has no open attempts".into()));
        }
        if let Some(run_id) = pick {
            match open.iter().find(|a| a.run_id == run_id) {
                Some(a) if a.result.as_deref() == Some("passed") => {}
                Some(_) => {
                    return Err(AppError::Invalid(format!(
                        "attempt {run_id} did not pass, so it cannot be picked"
                    )));
                }
                None => {
                    return Err(AppError::Invalid(format!(
                        "run {run_id} is not an open attempt of the card"
                    )));
                }
            }
        }
        let text = match (pick, text.trim()) {
            (None, "") => "None of the attempts passed.",
            (_, text) => text,
        };
        let runs = self.store.settle_attempts(card_id, pick, text)?;
        self.bus.publish(AppEvent::new(
            "attempts_settled",
            json!({ "card": card_id, "picked": pick, "runs": runs }),
        ));
        Ok(())
    }
}

fn pending() -> AttemptCheck {
    AttemptCheck {
        status: "pending",
        output: String::new(),
    }
}

// Unix only: the tests and checks the attempts are judged by are shell commands.
#[cfg(all(test, unix))]
mod judging_tests {
    use std::time::{Duration, Instant};

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::Store;

    /// A test-first repository and an attempt of one of its cards that committed `files`, ended as `status`.
    fn attempt(files: &[(&str, &str)], status: &str) -> (tempfile::TempDir, Arc<App>, i64) {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(origin.join(".moebius")).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join(".moebius/test-first"), "").unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = Arc::new(App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(5))),
            &dir.path().join("data"),
        ));
        let repo = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        let card = app.create_issue(repo.id, "Fix it", "", "person").unwrap();
        let run = app
            .store
            .add_run(Some(card.id), "Coder", "fake", "attempt")
            .unwrap();
        app.store
            .add_attempt(run.id, card.id, run.id, None)
            .unwrap();
        let clone = Path::new(&repo.path);
        let worktree = app
            .repos
            .add_worktree(&repo.name, clone, run.id, &repo.base_ref())
            .unwrap();
        for (path, text) in files {
            let file = worktree.path.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        git(&worktree.path, &["add", "--all"]).unwrap();
        git(
            &worktree.path,
            &["commit", "--quiet", "--allow-empty", "-m", "attempt"],
        )
        .unwrap();
        app.repos.remove_worktree(clone, &worktree.path).unwrap();
        app.store.set_run_status(run.id, status).unwrap();
        (dir, app, run.id)
    }

    /// Asks until the attempt is judged.
    fn judged(app: &Arc<App>, run: i64) -> AttemptCheck {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let check = app.check_attempt(run).unwrap();
            if check.status != "pending" {
                return check;
            }
            assert!(Instant::now() < deadline, "the attempt was not judged");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn an_attempt_whose_test_fails_before_and_passes_after_has_passed() {
        let files = [
            (".moebius/reproduce", "sh issue.sh\nissue.sh\n"),
            ("issue.sh", "test -f fixed.txt\n"),
            ("fixed.txt", "yes\n"),
        ];
        let (_dir, app, run) = attempt(&files, "succeeded");
        let check = judged(&app, run);
        assert_eq!(check.status, "passed", "{}", check.output);
        // The result is kept and the same answer comes again, with an event for the first time.
        let stored = app.store.attempt(run).unwrap().unwrap();
        assert_eq!(stored.result.as_deref(), Some("passed"));
        assert!(stored.judged_at.is_some());
        assert_eq!(app.check_attempt(run).unwrap(), check);
    }

    #[test]
    fn an_attempt_whose_test_passes_without_the_fix_has_failed() {
        let files = [
            (".moebius/reproduce", "sh issue.sh\nissue.sh\n"),
            ("issue.sh", "true\n"),
        ];
        let (_dir, app, run) = attempt(&files, "succeeded");
        let check = judged(&app, run);
        assert_eq!(check.status, "failed");
        assert!(
            check.output.contains("does not reproduce the issue"),
            "{}",
            check.output
        );
    }

    #[test]
    fn an_attempt_whose_test_fails_on_the_attempt_has_failed() {
        let files = [
            (".moebius/reproduce", "sh issue.sh\nissue.sh\n"),
            ("issue.sh", "test -f fixed.txt\n"),
        ];
        let (_dir, app, run) = attempt(&files, "succeeded");
        let check = judged(&app, run);
        assert_eq!(check.status, "failed");
        assert!(
            check.output.contains("It fails on the attempt"),
            "{}",
            check.output
        );
    }

    #[test]
    fn an_attempt_without_a_test_fails_unless_it_says_it_cannot_be_tested() {
        let (_dir, app, run) = attempt(&[("notes.txt", "x\n")], "succeeded");
        let check = judged(&app, run);
        assert_eq!(check.status, "failed");
        assert!(
            check.output.contains("names no reproduction test"),
            "{}",
            check.output
        );

        let untestable = [(".moebius/reproduce", "untestable: only the docs change\n")];
        let (_dir, app, run) = attempt(&untestable, "succeeded");
        assert_eq!(judged(&app, run).status, "passed");
    }

    #[test]
    fn an_attempt_whose_run_did_not_succeed_has_failed_and_a_run_that_is_no_attempt_is_refused() {
        let (_dir, app, run) = attempt(&[], "failed");
        let check = app.check_attempt(run).unwrap();
        assert_eq!(
            (check.status, check.output.as_str()),
            ("failed", "The run failed.")
        );
        assert_eq!(
            app.store.attempt(run).unwrap().unwrap().result.as_deref(),
            Some("failed")
        );

        let (_dir, app, run) = attempt(&[], "running");
        assert_eq!(app.check_attempt(run).unwrap().status, "pending");
        let card = app.store.attempt(run).unwrap().unwrap().card_id;
        let other = app.store.add_run(Some(card), "Coder", "x", "code").unwrap();
        assert!(app.check_attempt(other.id).is_err());
    }

    #[test]
    fn only_an_attempt_that_passed_can_be_picked_and_the_pick_settles_the_round() {
        let (_dir, app, run) = attempt(&[], "succeeded");
        let card = app.store.attempt(run).unwrap().unwrap().card_id;
        assert!(
            app.settle_attempts(card, Some(run), "why").is_err(),
            "not judged yet"
        );
        app.store.set_attempt_result(run, "failed", "no").unwrap();
        assert!(
            app.settle_attempts(card, Some(run), "why").is_err(),
            "it failed"
        );
        assert!(app.settle_attempts(card, Some(run + 99), "why").is_err());
        app.store.set_attempt_result(run, "passed", "").unwrap();
        let mut events = app.bus.subscribe();
        app.settle_attempts(card, Some(run), "the only one")
            .unwrap();
        assert_eq!(events.try_recv().unwrap().kind, "attempts_settled");
        assert!(
            app.settle_attempts(card, None, "").is_err(),
            "nothing is open now"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_names_its_pick_on_the_last_line() {
        assert_eq!(
            read_pick(Some("Run 4 is smaller.\nRun 5 misses a case.\nPICK: 4\n")),
            (Some(4), "Run 4 is smaller.\nRun 5 misses a case.".into())
        );
        assert_eq!(read_pick(Some("PICK: #7")), (Some(7), String::new()));
        for text in ["", "I like both.", "PICK: the first\n", "PICK: 4\nand more"] {
            assert_eq!(read_pick(Some(text)).0, None, "{text:?}");
        }
        assert_eq!(read_pick(None).0, None);
    }
}
