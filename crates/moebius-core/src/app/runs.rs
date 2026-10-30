//! A person's hand in a running run: steering it, stopping it, and opening a shell in its worktree, Pi-Bolt's terminal
//! interface in a chat, a shell on the server, or a shell in a repository's clone.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use super::{App, AppError, PERSON, Result};
use crate::blobs::redact;
use crate::board::{Runs, Tui};
use crate::decide::Actor;
use crate::harness::terminal::{MAX_PER_RUN, SERVER, Slot, repository_key, tui_key};
use crate::store::Run;

impl App {
    /// A person sends a message to a running run: the agent gets it and the run keeps it as a `steered` event.
    pub fn steer_run(
        &self,
        by: &Actor,
        runs: &dyn Runs,
        run_id: i64,
        message: &str,
    ) -> Result<Run> {
        let run = self.run(run_id)?;
        if message.trim().is_empty() {
            return Err(AppError::Invalid("a steer needs a message".into()));
        }
        if !runs.steer(run_id, message) {
            return Err(AppError::Invalid(format!("run {run_id} is not running")));
        }
        let message = redact(message);
        self.record_run_event(run_id, "steered", &json!({ "message": message }))?;
        let context = BTreeMap::from([("message".into(), message)]);
        self.record_action(by, "steer", run.card_id, Some(run_id), context, "")?;
        Ok(run)
    }

    /// A person stops a running run. It ends as `stopped` once its agent has settled, and its card then waits in the
    /// inbox as stuck.
    pub fn stop_run(&self, by: &Actor, runs: &dyn Runs, run_id: i64, reason: &str) -> Result<Run> {
        let run = self.run(run_id)?;
        if !runs.stop(run_id) {
            return Err(AppError::Invalid(format!("run {run_id} is not running")));
        }
        // A chat in its terminal interface stops once the interface has closed.
        self.terminals.close_tui(run_id);
        self.record_action(
            by,
            "stop",
            run.card_id,
            Some(run_id),
            BTreeMap::new(),
            reason,
        )?;
        Ok(run)
    }

    /// A person opens a shell in the worktree of `run_id`, which has one while the run is starting, running or
    /// waiting, and at most [`MAX_PER_RUN`] shells at a time. Returns the worktree and the shell's place among the
    /// run's. Only the opening is recorded, as a `shell_opened` event: never what is typed or printed.
    pub fn open_shell(&self, run_id: i64) -> Result<(PathBuf, Slot)> {
        let run = self.run(run_id)?;
        let worktree = match (run.status.as_str(), run.worktree) {
            ("starting" | "running" | "waiting", Some(worktree))
                if Path::new(&worktree).is_dir() =>
            {
                PathBuf::from(worktree)
            }
            _ => {
                return Err(AppError::Invalid(format!(
                    "run {run_id} has no worktree to open a shell in"
                )));
            }
        };
        let slot = self.terminals.take(run_id).ok_or_else(|| {
            AppError::Invalid(format!(
                "run {run_id} has {MAX_PER_RUN} terminals open already"
            ))
        })?;
        self.record_run_event(run_id, "shell_opened", &json!({ "who": PERSON }))?;
        Ok((worktree, slot))
    }

    /// A person opens Pi-Bolt's own terminal interface in the chat `run_id`, which continues the chat's conversation
    /// in place of the chat's own process (see [`Runs::tui`]). The chat must be waiting: while its agent works its
    /// process owns the worktree, so the answer is a conflict. Only one is open per chat, and while it is open the chat
    /// takes no message (see `say_in_chat`): the conversation goes on in the interface. Returns the program and the
    /// interface's place, to be dropped in that order once the program has ended; the opening is recorded as a
    /// `tui_opened` event.
    pub fn open_tui(self: &Arc<Self>, runs: Arc<dyn Runs>, run_id: i64) -> Result<(Tui, Slot)> {
        let run = self.running_chat(run_id)?;
        if run.status != "waiting" {
            return Err(AppError::Conflict(format!(
                "chat {run_id} is {}: its terminal opens once the agent has answered",
                run.status
            )));
        }
        let slot = self
            .terminals
            .take_up_to(tui_key(run_id), 1)
            .ok_or_else(|| AppError::Conflict(format!("chat {run_id} has its terminal open")))?;
        let command = runs.tui(self, run_id).map_err(AppError::Invalid)?;
        let tui = Tui::new(command, runs, self.clone(), run_id);
        self.record_run_event(run_id, "tui_opened", &json!({ "who": PERSON }))?;
        Ok((tui, slot))
    }

    /// A person opens a shell on the server, in `dir` (the directory of `moebius.toml`), at most [`MAX_PER_RUN`] at a
    /// time. There is no run to hold an event, so the opening is recorded as a decision of kind `open_terminal`: never
    /// what is typed or printed.
    pub fn open_server_shell(&self, dir: &Path) -> Result<Slot> {
        if !dir.is_dir() {
            return Err(AppError::Invalid(format!(
                "{} is not a directory to open a shell in",
                dir.display()
            )));
        }
        let slot = self.terminals.take(SERVER).ok_or_else(|| {
            AppError::Invalid(format!(
                "the server has {MAX_PER_RUN} terminals open already"
            ))
        })?;
        let context = BTreeMap::from([("directory".to_string(), dir.display().to_string())]);
        self.record_action(&Actor::Person, "open_terminal", None, None, context, "")?;
        Ok(slot)
    }

    /// A person opens a shell in the clone of repository `id`, at most [`MAX_PER_RUN`] at a time per repository.
    /// Returns the clone's directory and the shell's place. Like the server's shell it is recorded as an
    /// `open_terminal` decision, with the repository in its context.
    pub fn open_repository_shell(&self, id: i64) -> Result<(PathBuf, Slot)> {
        let repository = self.repository(id)?;
        let dir = PathBuf::from(&repository.path);
        if !dir.is_dir() {
            return Err(AppError::Invalid(format!(
                "the clone of {} is gone: {} is not a directory",
                repository.name,
                dir.display()
            )));
        }
        let slot = self
            .terminals
            .take_up_to(repository_key(id), MAX_PER_RUN)
            .ok_or_else(|| {
                AppError::Invalid(format!(
                    "{} has {MAX_PER_RUN} terminals open already",
                    repository.name
                ))
            })?;
        let context = BTreeMap::from([
            ("directory".to_string(), dir.display().to_string()),
            ("repository".to_string(), repository.name.clone()),
        ]);
        self.record_action(&Actor::Person, "open_terminal", None, None, context, "")?;
        Ok((dir, slot))
    }
}
