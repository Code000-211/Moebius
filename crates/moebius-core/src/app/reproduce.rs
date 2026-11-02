//! Test first: a repository opts in with a file `.moebius/test-first` on its default branch, and the Coder then names
//! the test that reproduces the issue in `.moebius/reproduce`, committed with the change. The review script runs that
//! test on the change and on the commit the change was made from with the test's files added, each in a throwaway
//! worktree with the time limit and cleared environment of the repository's checks (see `checks`); the change goes
//! on only when it fails there and passes here. A run goes on a thread of its own, and its result is kept in memory
//! for the change's commit, so a script asks again on each tick until it has ended.

use std::path::Path;
use std::sync::Arc;

use super::{App, AppError, Result};
use crate::repos::git;
use crate::store::Repository;

/// The file on the default branch that turns test first on.
const TEST_FIRST: &str = ".moebius/test-first";

/// The file in the change that names its reproduction test.
const REPRODUCE: &str = ".moebius/reproduce";

/// What a change says of its reproduction test in `.moebius/reproduce`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reproduction {
    /// The command that runs the test, and the files of the change that hold it.
    Test { command: String, files: Vec<String> },
    /// Why the change cannot be tested this way, such as a change of the documentation only.
    Untestable(String),
}

/// Reads `.moebius/reproduce`: lines that are empty or start with `#` are skipped. A first line `untestable: <why>`
/// says why the change has no test; otherwise the first line is the command and every further line a file. None for a
/// file that says nothing.
fn parse(text: &str) -> Option<Reproduction> {
    let mut lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let first = lines.next()?;
    if let Some(why) = first.strip_prefix("untestable:") {
        let why = std::iter::once(why.trim())
            .chain(lines)
            .collect::<Vec<_>>()
            .join("\n");
        return Some(Reproduction::Untestable(why.trim().to_string()));
    }
    Some(Reproduction::Test {
        command: first.to_string(),
        files: lines.map(String::from).collect(),
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
    /// Whether the repository works test first: its default branch has `.moebius/test-first`.
    pub fn test_first(&self, repository: &Repository) -> bool {
        let spec = format!("{}:{TEST_FIRST}", repository.base_ref());
        git(Path::new(&repository.path), &["cat-file", "-e", &spec]).is_ok()
    }

    /// What the card's open change says of its reproduction test, from `.moebius/reproduce` of its latest commit. None
    /// when the change has none, or has the file as the commit it was made from has it: a file left by an earlier
    /// change says nothing about this one.
    pub fn reproduction(&self, card_id: i64) -> Result<Option<Reproduction>> {
        let (_, repository, head, base) = self.change_and_base(card_id)?;
        Ok(reproduction_at(Path::new(&repository.path), &head, &base))
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

/// What the commit `head` says of its reproduction test in `.moebius/reproduce`, none when it has no such file or has it
/// as `base`, the commit it was made from, has it.
pub(super) fn reproduction_at(clone: &Path, head: &str, base: &str) -> Option<Reproduction> {
    let text = git(clone, &["show", &format!("{head}:{REPRODUCE}")]).ok()?;
    if git(clone, &["show", &format!("{base}:{REPRODUCE}")]).is_ok_and(|before| before == text) {
        return None;
    }
    parse(&text)
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
    fn the_reproduce_file_names_a_command_and_its_files_or_why_there_is_none() {
        assert_eq!(
            parse("# the test\ncargo test --test issue_7\n\ntests/issue_7.rs\n"),
            Some(Reproduction::Test {
                command: "cargo test --test issue_7".into(),
                files: vec!["tests/issue_7.rs".into()],
            })
        );
        assert_eq!(
            parse("untestable: only the README changes\nso nothing runs\n"),
            Some(Reproduction::Untestable(
                "only the README changes\nso nothing runs".into()
            ))
        );
        assert_eq!(parse("# nothing\n\n"), None);
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
