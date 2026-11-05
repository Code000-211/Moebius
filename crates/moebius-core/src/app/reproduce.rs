//! Test first: a project opts in with a file `test-first` in its settings, and the Coder then names the test that
//! reproduces the issue in the end of its summary, in lines `REPRODUCE: <command>` and `REPRODUCE-FILE: <path>`. The
//! summary is stored with the run, so the attempt or code run that made a change is also where its test is named. The
//! review script runs that test on the change and on the commit the change was made from with the test's files added,
//! each in a throwaway worktree with the time limit and cleared environment of the repository's checks (see
//! `checks`); the change goes on only when it fails there and passes here. A run goes on a thread of its own, and its
//! result is kept in memory for the change's commit, so a script asks again on each tick until it has ended.

use std::path::Path;
use std::sync::Arc;

use super::{App, AppError, Result};
use crate::repos::git;
use crate::store::Repository;

/// The file of a project's settings that turns test first on.
const TEST_FIRST: &str = "test-first";

/// What a run says of its reproduction test in its summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reproduction {
    /// The command that runs the test, and the files of the change that hold it.
    Test { command: String, files: Vec<String> },
    /// Why the change cannot be tested this way, such as a change of the documentation only.
    Untestable(String),
}

/// `text` without the spaces and backticks around it that a summary may put there.
fn bare(text: &str) -> &str {
    text.trim().trim_matches('`').trim()
}

/// Reads the reproduction test a run names in its summary: the last line that starts with `REPRODUCE:` is the command,
/// or `untestable: <why>` for a change that has no test, and the lines `REPRODUCE-FILE: <path>` of its block (up to a
/// blank line, above or below it; those under an earlier `REPRODUCE:` line of the block are that line's) are the files,
/// relative to the repository's root, that hold the test. None for a summary with no such line.
pub fn parse(summary: &str) -> Option<Reproduction> {
    let lines: Vec<&str> = summary.lines().map(str::trim).collect();
    let at = lines
        .iter()
        .rposition(|line| line.starts_with("REPRODUCE:"))?;
    let first = bare(&lines[at]["REPRODUCE:".len()..]);
    if let Some(why) = first.strip_prefix("untestable:") {
        return Some(Reproduction::Untestable(why.trim().to_string()));
    }
    if first.is_empty() {
        return None;
    }
    let start = lines[..at]
        .iter()
        .rposition(|line| line.is_empty())
        .map_or(0, |blank| blank + 1);
    let end = lines[at..]
        .iter()
        .position(|line| line.is_empty())
        .map_or(lines.len(), |blank| at + blank);
    // A file belongs to the `REPRODUCE:` line above it in the block, or to the first one below it when none is above.
    let first_line = (start..end).find(|&i| lines[i].starts_with("REPRODUCE:"));
    let mut owner = None;
    let mut files = Vec::new();
    for (index, line) in lines.iter().enumerate().take(end).skip(start) {
        if line.starts_with("REPRODUCE:") {
            owner = Some(index);
        } else if let Some(file) = line.strip_prefix("REPRODUCE-FILE:")
            && owner.or(first_line) == Some(at)
            && !bare(file).is_empty()
        {
            files.push(bare(file).to_string());
        }
    }
    Some(Reproduction::Test {
        command: first.to_string(),
        files,
    })
}

/// How a run of a reproduction test stands: `pending` while it goes, then `passed` or `failed`, or `error` when it
/// could not run (no worktree, a named file missing from the change). `output` is the end of what it printed, or why
/// it could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRun {
    pub status: &'static str,
    pub output: String,
}

/// A run of a test: the change, its commit, the command, and the files added to the base commit (none on the change).
pub(super) type TestKey = (i64, String, String, Option<Vec<String>>);

impl App {
    /// Whether the repository works test first: its project has a `test-first` file.
    pub fn test_first(&self, repository: &Repository) -> bool {
        self.project_file(repository, TEST_FIRST).is_some()
    }

    /// What the card's open change says of its reproduction test: what the latest succeeded `code` run of the card
    /// that names one says in its summary. None when no run of the card does. Raises `Invalid` before the change is
    /// published.
    pub fn reproduction(&self, card_id: i64) -> Result<Option<Reproduction>> {
        self.change_and_base(card_id)?;
        let runs = self.store.runs_for_card(card_id)?;
        Ok(runs
            .iter()
            .rev()
            .filter(|run| run.purpose == "code" && run.status == "succeeded")
            .find_map(|run| parse(run.summary.as_deref()?)))
    }

    /// What the run `run_id` (an attempt) says of its reproduction test in its summary.
    pub(super) fn reproduction_of_run(&self, run_id: i64) -> Result<Option<Reproduction>> {
        Ok(self.run(run_id)?.summary.as_deref().and_then(parse))
    }

    /// Runs `command` on the card's open change, or with `files` on the commit the change was made from with those
    /// files as the change has them, unless that run already ran or is running for the change's commit; returns how
    /// it stands now. It goes on a thread of its own: `pending` until it has ended.
    pub fn run_test(
        self: &Arc<Self>,
        card_id: i64,
        command: &str,
        files: Option<Vec<String>>,
    ) -> Result<TestRun> {
        let (change, repository, head, base) = self.change_and_base(card_id)?;
        let key: TestKey = (change, head.clone(), command.to_string(), files.clone());
        {
            let mut tests = self.tests.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(run) = tests.get(&key) {
                return Ok(run.clone());
            }
            // Only the change's latest commit matters, and a closed change needs no answers.
            tests.retain(|(other, other_head, _, _), _| {
                if *other == change {
                    *other_head == head
                } else {
                    self.change(*other).is_ok_and(|c| c.state == "open")
                }
            });
            tests.insert(key.clone(), pending());
        }
        let app = self.clone();
        let key_for_failure = key.clone();
        let command = command.to_string();
        let started = std::thread::Builder::new()
            .name(format!("test of change {change}"))
            .spawn(move || {
                let name = test_name(change, &head, &command, &files);
                let (rev, prepare) = match &files {
                    Some(files) => (base.as_str(), files.as_slice()),
                    None => (head.as_str(), &[][..]),
                };
                let ran = app.run_prepared(
                    &repository,
                    (&name, rev),
                    |worktree| add_files(worktree, &head, prepare),
                    ("The test", &command),
                );
                let run = match ran {
                    Ok((true, output)) => TestRun {
                        status: "passed",
                        output,
                    },
                    Ok((false, output)) => TestRun {
                        status: "failed",
                        output,
                    },
                    Err(output) => TestRun {
                        status: "error",
                        output,
                    },
                };
                // An entry pruned while the test ran (the change moved on or closed) stays gone.
                let mut tests = app.tests.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(slot) = tests.get_mut(&key) {
                    *slot = run;
                }
            });
        if let Err(e) = started {
            // Nothing runs, so nothing may stay pending: a script would wait for it until the commit moves.
            self.tests
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&key_for_failure);
            return Ok(TestRun {
                status: "error",
                output: format!("cannot start the test: {e}"),
            });
        }
        Ok(pending())
    }

    /// The card's open change, its repository, its latest commit, and the commit it was made from: where it leaves
    /// the repository's default branch.
    fn change_and_base(&self, card_id: i64) -> Result<(i64, Repository, String, String)> {
        let change = self
            .store
            .open_change(card_id)?
            .ok_or_else(|| AppError::Invalid("the card has no change to test".into()))?;
        let repository = self.repository_of(&change)?;
        let head = change
            .head
            .clone()
            .ok_or_else(|| AppError::Invalid("the change has no commit yet".into()))?;
        let base = git(
            Path::new(&repository.path),
            &["merge-base", &repository.base_ref(), &head],
        )?;
        Ok((change.id, repository, head, base))
    }
}

/// The name of the worktree and log of one test: the command and the files are in it as a short hash, as two tests of one
/// commit may run at the same time and must not share a worktree.
fn test_name(change: i64, head: &str, command: &str, files: &Option<Vec<String>>) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (command, files).hash(&mut hasher);
    format!(
        "test-{}-{}-{}-{:08x}",
        change,
        if files.is_some() { "base" } else { "change" },
        &head[..12.min(head.len())],
        hasher.finish() as u32
    )
}

fn pending() -> TestRun {
    TestRun {
        status: "pending",
        output: String::new(),
    }
}

/// Checks out `files` as the commit `head` has them in `worktree`; an error names what is missing.
pub(super) fn add_files(
    worktree: &Path,
    head: &str,
    files: &[String],
) -> std::result::Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }
    let mut args = vec!["--literal-pathspecs", "checkout", head, "--"];
    args.extend(files.iter().map(String::as_str));
    git(worktree, &args)
        .map(drop)
        .map_err(|e| format!("cannot add the test's files to the base commit: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_tests_of_one_commit_have_worktrees_of_their_own() {
        let head = "0123456789abcdef";
        let name = |command: &str, files: Option<Vec<&str>>| {
            test_name(
                1,
                head,
                command,
                &files.map(|f| f.into_iter().map(String::from).collect()),
            )
        };
        let names = [
            name("cargo test a", None),
            name("cargo test b", None),
            name("cargo test a", Some(vec!["t.rs"])),
            name("cargo test a", Some(vec!["u.rs"])),
        ];
        for (i, one) in names.iter().enumerate() {
            for other in &names[i + 1..] {
                assert_ne!(one, other);
            }
        }
        assert_eq!(names[0], name("cargo test a", None));
    }

    #[test]
    fn a_summary_names_a_command_and_its_files_or_why_there_is_none() {
        assert_eq!(
            parse(
                "Fixed it.\n\nREPRODUCE: cargo test --test issue_7\nREPRODUCE-FILE: tests/issue_7.rs\n\
                 REPRODUCE-FILE: `tests/helper.rs`\n"
            ),
            Some(Reproduction::Test {
                command: "cargo test --test issue_7".into(),
                files: vec!["tests/issue_7.rs".into(), "tests/helper.rs".into()],
            })
        );
        assert_eq!(
            parse("REPRODUCE: `sh t.sh`\n"),
            Some(Reproduction::Test {
                command: "sh t.sh".into(),
                files: vec![],
            })
        );
        assert_eq!(
            parse("Done.\nREPRODUCE: untestable: only the README changes\n"),
            Some(Reproduction::Untestable("only the README changes".into()))
        );
        // The last REPRODUCE line stands, and only the files after it.
        assert_eq!(
            parse("REPRODUCE: a\nREPRODUCE-FILE: x\nREPRODUCE: b\nREPRODUCE-FILE: y\n"),
            Some(Reproduction::Test {
                command: "b".into(),
                files: vec!["y".into()],
            })
        );
        // The files may come before the line, in the same block.
        assert_eq!(
            parse(
                "Notes.\nREPRODUCE-FILE: tests/old.rs\n\nREPRODUCE-FILE: t.rs\nREPRODUCE: sh t.sh\nREPRODUCE-FILE: u.rs\n\nREPRODUCE-FILE: z.rs\n"
            ),
            Some(Reproduction::Test {
                command: "sh t.sh".into(),
                files: vec!["t.rs".into(), "u.rs".into()],
            })
        );
        assert_eq!(parse("Fixed it.\n"), None);
        assert_eq!(parse("REPRODUCE:   \n"), None);
    }

    // Unix only: the test is a shell command.
    #[cfg(unix)]
    #[test]
    fn running_a_test_forgets_old_commits_and_closed_changes() {
        use crate::store::Store;
        use moebius_plugin_api::Host;
        use std::time::{Duration, Instant};

        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let head = git(&origin, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
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
        let open = app
            .store
            .add_change(repo.id, Some(card.id), "moebius/card", None, &head)
            .unwrap();
        let closed = app
            .store
            .add_change(repo.id, None, "other", None, &head)
            .unwrap();
        app.store.set_change_state(closed.id, "closed").unwrap();

        let key = |change: i64, head: &str| -> TestKey {
            (change, head.to_string(), "true".to_string(), None)
        };
        {
            let mut tests = app.tests.lock().unwrap();
            tests.insert(key(open.id, "old"), pending());
            tests.insert(key(closed.id, &head), pending());
        }
        app.run_test(card.id, "true", None).unwrap();
        // Wait for the thread, which must not bring back what was pruned.
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let status = app.tests.lock().unwrap()[&key(open.id, &head)].status;
            if status != "pending" {
                assert_eq!(status, "passed");
                break;
            }
            assert!(Instant::now() < deadline, "the test did not end");
            std::thread::yield_now();
        }
        let tests = app.tests.lock().unwrap();
        assert!(!tests.contains_key(&key(open.id, "old")));
        assert!(!tests.contains_key(&key(closed.id, &head)));
        assert_eq!(tests.len(), 1);
    }
}
