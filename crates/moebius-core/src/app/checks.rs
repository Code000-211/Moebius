//! A change's checks: how they stand for its current commit, and a repository's own checks (`.moebius/checks`) run
//! on a change, or on a trial's commit, in a throwaway worktree with a time limit.

use std::io::{Read, Seek, SeekFrom};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use super::{App, AppError, Result};
use crate::blobs::redact;
use crate::bus::AppEvent;
use crate::repos::git;
use crate::store::{Change, Repository, now};

/// How long after a publish a change without checks for its commit is taken to be waiting for them; after that, the
/// repository has no checks (in seconds).
const CHECKS_GRACE: i64 = 120;

/// How much of the end of the output of a repository's checks is kept.
const CHECK_OUTPUT: usize = 4096;

impl App {
    /// How the checks of the change stand for its current commit: `pending`, `success`, `failure` or `none`. Checks
    /// recorded for an older commit do not count; on a forge the change then waits for its checks for a while after
    /// it was published, so that a pushed commit is not taken for one without CI.
    pub fn checks_of(&self, change: &Change) -> Result<&'static str> {
        let recorded = change
            .checks
            .as_deref()
            .filter(|_| change.checks_head == change.head);
        Ok(match recorded {
            Some("success") => "success",
            Some("failure") => "failure",
            Some(_) => "pending",
            None if change.repository_id.is_some()
                && self.repository_of(change)?.forge != "none"
                && now() - change.published_at.unwrap_or(0) < CHECKS_GRACE =>
            {
                "pending"
            }
            None => "none",
        })
    }

    /// Runs the repository's own checks, the command in `.moebius/checks` on its default branch (never on the change's,
    /// so that its author cannot change them), on the card's change. They run on a thread of their own, in a throwaway
    /// worktree; meanwhile the change's checks are `pending`. Does nothing for a repository on a forge, which has its
    /// own checks, or without that file, or when the checks already ran or are running for the change's commit.
    pub fn run_checks(self: &Arc<Self>, card_id: i64) -> Result<Change> {
        // Held while the change is read: checks that end record their result before they are removed from the set,
        // so a change read under the lock is final when its checks are not in the set.
        let mut checking = self.checking.lock().expect("checks are not poisoned");
        let change = self
            .store
            .open_change(card_id)?
            .ok_or_else(|| AppError::Invalid("the card has no change to check".into()))?;
        let repository = self.repository_of(&change)?;
        let (Some(head), Some(script)) = (change.head.clone(), self.checks_script(&repository))
        else {
            return Ok(change);
        };
        let key = (change.id, head.clone());
        let finished = change.checks_head.as_ref() == Some(&head)
            && change.checks.as_deref().is_some_and(|c| c != "pending");
        if finished || checking.contains(&key) {
            return Ok(change);
        }
        self.store
            .set_local_checks(change.id, "pending", &head, None)?;
        checking.insert(key.clone());
        drop(checking);
        let app = self.clone();
        let (checked, running) = (change.clone(), key.clone());
        let started = std::thread::Builder::new()
            .name(format!("checks of change {}", change.id))
            .spawn(move || {
                app.check_change(&repository, &checked, &head, &script);
                app.checking
                    .lock()
                    .expect("checks are not poisoned")
                    .remove(&running);
            });
        if let Err(e) = started {
            // Nothing runs, so the next call starts the checks again.
            self.checking
                .lock()
                .expect("checks are not poisoned")
                .remove(&key);
            return Err(AppError::Invalid(format!("cannot start the checks: {e}")));
        }
        let change = self.change(change.id)?;
        self.bus
            .publish(AppEvent::new("change_updated", json!(change)));
        Ok(change)
    }

    /// The repository's own checks: the command in `.moebius/checks` on its default branch. None for a repository on a
    /// forge, which has its own checks, and for one without that file.
    fn checks_script(&self, repository: &Repository) -> Option<String> {
        if repository.forge != "none" {
            return None;
        }
        let spec = format!("{}:.moebius/checks", repository.base_ref());
        git(Path::new(&repository.path), &["show", &spec]).ok()
    }

    /// Runs `script` on the commit `rev` in a throwaway worktree named `name` and returns whether it succeeded and the
    /// end of its output.
    fn run_script_on(
        &self,
        repository: &Repository,
        name: &str,
        rev: &str,
        script: &str,
    ) -> (bool, String) {
        let clone = Path::new(&repository.path);
        match self
            .repos
            .add_detached_worktree(&repository.name, clone, name, rev)
        {
            Ok(worktree) => {
                let log = self.data_dir.join(format!("{name}.log"));
                let result = execute(script, &worktree, &log, self.checks_timeout);
                let _ = std::fs::remove_file(&log);
                let _ = self.repos.remove_worktree(clone, &worktree);
                result
            }
            Err(e) => (false, format!("cannot make a worktree for the checks: {e}")),
        }
    }

    /// Runs the repository's own checks on the commit `rev` and waits for them: whether they succeeded, or none when the
    /// repository has none (see `run_checks`). For a trial (see `trial`), which has no change to record them on.
    pub fn local_checks(&self, repository: &Repository, name: &str, rev: &str) -> Option<bool> {
        let script = self.checks_script(repository)?;
        Some(self.run_script_on(repository, name, rev, &script).0)
    }

    /// Runs `script` on `head` in a worktree of its own and records how it ended, unless the change has moved on.
    fn check_change(&self, repository: &Repository, change: &Change, head: &str, script: &str) {
        // Named by commit too, as checks of an older commit may still be going when a newer one is published.
        let name = format!("checks-{}-{}", change.id, &head[..12.min(head.len())]);
        let (succeeded, output) = self.run_script_on(repository, &name, head, script);
        let current = self.change(change.id);
        if current.is_ok_and(|c| c.head.as_deref() == Some(head)) {
            let checks = if succeeded { "success" } else { "failure" };
            let _ = self
                .store
                .set_local_checks(change.id, checks, head, Some(&output));
            if let Ok(change) = self.change(change.id) {
                self.bus
                    .publish(AppEvent::new("change_updated", json!(change)));
            }
        }
    }
}

/// Runs `script` with `sh -c` in `dir`, writing its output to `log`, and stops it (and what it started) after
/// `timeout`. Returns whether it succeeded and the end of its output with secrets removed.
fn execute(script: &str, dir: &Path, log: &Path, timeout: Duration) -> (bool, String) {
    let run = || -> std::io::Result<(bool, bool)> {
        let file = std::fs::File::create(log)?;
        let mut child = Command::new("sh")
            .args(["-c", script])
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(file)
            .process_group(0)
            .spawn()?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok((status.success(), false));
            }
            if Instant::now() >= deadline {
                let group = format!("-{}", child.id());
                let _ = Command::new("kill").args(["-KILL", "--", &group]).status();
                child.wait()?;
                return Ok((false, true));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    match run() {
        Ok((succeeded, timed_out)) => {
            let mut output = tail(log);
            if timed_out {
                output.push_str(&format!(
                    "\nThe checks were stopped after {} seconds.",
                    timeout.as_secs()
                ));
            }
            (succeeded, output)
        }
        Err(e) => (false, format!("cannot run the checks: {e}")),
    }
}

/// The end of the file `log`, without secrets, as much as is kept of the output of checks.
pub(crate) fn tail(log: &Path) -> String {
    let Ok(mut file) = std::fs::File::open(log) else {
        return String::new();
    };
    // Read more than is kept, so that a secret cut in two by the start is still whole when it is redacted.
    let len = file.metadata().map_or(0, |m| m.len());
    let _ = file.seek(SeekFrom::Start(len.saturating_sub(4 * CHECK_OUTPUT as u64)));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    let text = redact(&String::from_utf8_lossy(&bytes));
    let start = (text.len().saturating_sub(CHECK_OUTPUT)..)
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(text.len());
    text[start..].trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_are_stopped_after_their_time_and_their_output_is_cut_and_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let run =
            |script: &str, seconds| execute(script, dir.path(), &log, Duration::from_secs(seconds));

        let (succeeded, output) = run("echo started; sleep 30", 1);
        assert!(!succeeded);
        assert_eq!(output, "started\nThe checks were stopped after 1 seconds.");

        let token = format!("ghp_{}", "a".repeat(36));
        let (succeeded, output) = run(
            &format!("yes line | head -c 20000; echo {token}; echo END"),
            30,
        );
        assert!(succeeded);
        assert!(output.len() <= CHECK_OUTPUT, "{}", output.len());
        assert!(
            output.ends_with("[redacted:github-token]\nEND"),
            "{output:?}"
        );

        let (succeeded, output) = run("echo nope >&2; exit 3", 30);
        assert_eq!((succeeded, output.as_str()), (false, "nope"));
    }
}
