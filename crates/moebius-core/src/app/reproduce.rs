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
        let clone = Path::new(&repository.path);
        let Ok(text) = git(clone, &["show", &format!("{head}:{REPRODUCE}")]) else {
            return Ok(None);
        };
        if git(clone, &["show", &format!("{base}:{REPRODUCE}")]).is_ok_and(|before| before == text)
        {
            return Ok(None);
        }
        Ok(parse(&text))
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
            tests.insert(key.clone(), pending());
        }
        let app = self.clone();
        let command = command.to_string();
        let started = std::thread::Builder::new()
            .name(format!("test of change {change}"))
            .spawn(move || {
                let at_base = files.is_some();
                let name = format!(
                    "test-{}-{}-{}",
                    change,
                    if at_base { "base" } else { "change" },
                    &head[..12.min(head.len())]
                );
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
                app.tests
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(key, run);
            });
        if let Err(e) = started {
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

fn pending() -> TestRun {
    TestRun {
        status: "pending",
        output: String::new(),
    }
}

/// Checks out `files` as the commit `head` has them in `worktree`; an error names what is missing.
fn add_files(worktree: &Path, head: &str, files: &[String]) -> std::result::Result<(), String> {
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
}
