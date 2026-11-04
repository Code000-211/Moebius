//! Deleting chats: a chat that is going is stopped, then it is taken out of the store with its changes, inbox items and
//! grillings, and its worktree and `moebius/chat-<id>` branch are removed from the clone it worked in (the repository's,
//! or the harness configuration repository's). The same stop and wait serve research (see `improve::research`) and
//! grillings (see `harness::grilling`), which delete their chats through [`App::remove_chat`].
//!
//! Usage and judgements are history, so they stay without a run (see `store::deletions`); issues a grilling filed stay on
//! the forge and as cards, and a change already published stays on the forge.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{App, AppError, Result};
use crate::board::Runs;
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::{ChatRemoved, Run};

/// How long a delete waits for the runs it stopped to end before it asks the person to try again.
const STOP_WAIT: Duration = Duration::from_secs(60);

impl App {
    /// Waits until `done` is true, polling; `Conflict` naming `what` when it is not within [`STOP_WAIT`].
    pub(crate) fn wait_until(&self, what: &str, done: impl Fn() -> bool) -> Result<()> {
        let deadline = Instant::now() + STOP_WAIT;
        while !done() {
            if Instant::now() > deadline {
                return Err(AppError::Conflict(format!(
                    "{what} is still stopping; try again in a moment"
                )));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    /// Waits until each of the runs `ids` has ended; a run that is gone has.
    pub(crate) fn wait_until_ended(&self, what: &str, ids: &[i64]) -> Result<()> {
        self.wait_until(what, || {
            ids.iter().all(|id| {
                self.store
                    .run(*id)
                    .ok()
                    .flatten()
                    .is_none_or(|run| run.ended_at.is_some())
            })
        })
    }

    /// Removes the worktree of `run` if it is still there and under the data directory's worktrees or chats: from `clone` when the run worked in one, else the directory.
    pub(crate) fn remove_run_worktree(&self, clone: Option<&Path>, run: &Run) {
        let Some(worktree) = run.worktree.as_deref().map(Path::new) else {
            return;
        };
        // Only what Moebius made: a stored path elsewhere is never removed.
        let ours = [self.data_dir.join("worktrees"), self.data_dir.join("chats")];
        let within = |root: &PathBuf| {
            worktree.starts_with(root)
                && !worktree
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
        };
        if !ours.iter().any(within) || !worktree.exists() {
            return;
        }
        if let Some(clone) = clone
            && self.repos.remove_worktree(clone, worktree).is_ok()
        {
            return;
        }
        let _ = std::fs::remove_dir_all(worktree);
    }

    /// The clone the chat `run` works in: its repository's, or the harness configuration repository's; none for a chat
    /// in an empty directory, or whose repository is gone.
    fn chat_clone(&self, run: &Run) -> Option<PathBuf> {
        if let Some(repository) = run.repository_id {
            return Some(PathBuf::from(self.repository(repository).ok()?.path));
        }
        if run.harness_repository {
            return Some(self.harness_repo().ok()?.path().to_path_buf());
        }
        None
    }

    /// The run `id` as a chat a person can delete; `NotFound` for a run that is no chat.
    fn deletable_chat(&self, id: i64) -> Result<Run> {
        self.run(id)
            .ok()
            .filter(|run| run.purpose == "chat")
            .ok_or_else(|| AppError::NotFound(format!("chat {id}")))
    }

    /// Stops the chat `id` if it is going, then deletes it from the store, its worktree and its branch, and returns it
    /// with what the store took out. The caller records the decision and announces it (see
    /// [`App::announce_chat_deleted`]).
    pub(crate) fn remove_chat(&self, runs: &dyn Runs, id: i64) -> Result<(Run, ChatRemoved)> {
        let run = self.deletable_chat(id)?;
        if run.ended_at.is_none() {
            if self.is_waking(id) {
                return Err(Self::waking_conflict(id));
            }
            runs.stop(id);
            // A chat in its terminal interface stops once the interface has closed.
            self.terminals.close_tui(id);
            self.wait_until_ended(&format!("chat {id}"), &[id])?;
        }
        // Under the hold a message to the chat takes, so that none takes the chat up again meanwhile.
        let turn = self.chat_turns.lock().unwrap_or_else(|e| e.into_inner());
        let run = self.deletable_chat(id)?;
        if run.ended_at.is_none() || self.is_waking(id) {
            return Err(AppError::Conflict(format!(
                "chat {id} is going again; try again in a moment"
            )));
        }
        let clone = self.chat_clone(&run);
        let removed = self.store.delete_chat(&run)?;
        drop(turn);
        self.remove_run_worktree(clone.as_deref(), &run);
        if let Some(clone) = &clone {
            let _ = self.repos.delete_branch(clone, &run.branch());
        }
        Ok((run, removed))
    }

    /// Announces that the chat `run` was deleted, and the grillings that went with it.
    pub(crate) fn announce_chat_deleted(&self, run: i64, removed: &ChatRemoved) {
        for grilling in &removed.grillings {
            self.bus.publish(Event::GrillingDeleted {
                grilling: *grilling,
            });
        }
        self.bus.publish(Event::ChatDeleted { run });
    }

    /// A person deletes a chat. A chat that is going is stopped first. Its events, its read marker, the changes it
    /// offered with their inbox items, its grillings, its worktree and its branch are deleted; its usage stays. The
    /// deletion is recorded as a `delete_chat` decision and announced as `chat_deleted` (and `grilling_deleted` for each
    /// grilling the chat ended in).
    pub fn delete_chat(&self, by: &Actor, runs: &dyn Runs, id: i64, reason: &str) -> Result<Run> {
        let title = self.chat_title(&self.deletable_chat(id)?)?;
        let (run, removed) = self.remove_chat(runs, id)?;
        let context = BTreeMap::from([
            ("chat".to_string(), id.to_string()),
            ("title".to_string(), title),
            ("agent".to_string(), run.agent.clone()),
            ("events".to_string(), removed.events.to_string()),
            ("changes".to_string(), list(&removed.changes)),
            ("inbox_items".to_string(), removed.inbox_items.to_string()),
            ("grillings".to_string(), list(&removed.grillings)),
            ("usage_kept".to_string(), removed.usage.to_string()),
        ]);
        self.record_action(by, "delete_chat", None, None, context, reason)?;
        self.announce_chat_deleted(id, &removed);
        Ok(run)
    }
}

/// Ids as a comma-separated list, for a decision's context.
pub(crate) fn list(ids: &[i64]) -> String {
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use crate::app::App;
    use crate::store::Store;

    #[test]
    fn only_a_worktree_under_the_data_directory_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(1)));
        let data = dir.path().join("data");
        let app = Arc::new(App::new(store, host, &data));
        let outside = dir.path().join("precious");
        let sneaky = data.join("chats").join("..").join("..").join("precious");
        let inside = data.join("chats").join("7");
        for path in [&outside, &inside] {
            std::fs::create_dir_all(path).unwrap();
        }
        let run = |path: &std::path::Path| {
            let mut run = app.store.add_run(None, "Coder", "fake", "chat").unwrap();
            run.worktree = Some(path.to_string_lossy().into_owned());
            run
        };
        app.remove_run_worktree(None, &run(&outside));
        app.remove_run_worktree(None, &run(&sneaky));
        assert!(outside.exists(), "a path outside the data directory stays");
        app.remove_run_worktree(None, &run(&inside));
        assert!(!inside.exists());
    }
}
