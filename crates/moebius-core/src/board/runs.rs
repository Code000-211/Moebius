//! What the board engine needs of a harness (`Runs`): starting, steering and stopping runs, and the chats, retros,
//! research and trials a harness may also offer. Pi-Bolt (`pibolt`) is the real one; the tests use fakes.

use std::path::Path;
use std::sync::Arc;

use crate::app::{App, ChatSettings};
use crate::config::AgentConfig;
use crate::router::Protocol;
use crate::store::{Card, Repository, Run};

/// An agent the harness can run, as the desktop API lists it: never its instructions or anything secret.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub model: String,
    pub protocol: Protocol,
    /// How hard the agent thinks, if its configuration says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    /// The most tokens a card of the agent may use, if its configuration says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
    /// Whether Moebius brings the agent itself, as it does the Moebius agent.
    pub built_in: bool,
    /// Why the agent's files in the harness configuration repository could not be used. Such an agent has no model
    /// and cannot be run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The terminal interface of a chat while it is open: the program to run on a terminal. Dropped once the program has
/// ended, it gives the chat back to its own process (see [`Runs::close_tui`]).
pub struct Tui {
    pub command: std::process::Command,
    runs: Arc<dyn Runs>,
    app: Arc<App>,
    run_id: i64,
}

impl Tui {
    /// The terminal interface `command` that `runs` opened for the chat `run_id`.
    pub fn new(
        command: std::process::Command,
        runs: Arc<dyn Runs>,
        app: Arc<App>,
        run_id: i64,
    ) -> Self {
        Self {
            command,
            runs,
            app,
            run_id,
        }
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        self.runs.close_tui(&self.app, self.run_id);
    }
}

/// Starts runs for the engine. The real one drives a harness (Pi-Bolt); tests use a fake.
pub trait Runs: Send + Sync {
    /// Starts a run for `card` and returns it without waiting for it to end.
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
        note: Option<&str>,
    ) -> Result<Run, String>;

    /// Whether an agent of this name is configured.
    fn has_agent(&self, name: &str) -> bool;

    /// The agents this harness can run.
    fn agents(&self) -> Vec<AgentInfo> {
        Vec::new()
    }

    /// The token budget of `agent` for a card, if it has one.
    fn token_budget(&self, _agent: &str) -> Option<i64> {
        None
    }

    /// Sends a message to a running run's agent, which takes it into account as it works. False when the run is not
    /// running.
    fn steer(&self, run_id: i64, message: &str) -> bool;

    /// Asks a running run to stop; it ends as `stopped`. False when the run is not running.
    fn stop(&self, run_id: i64) -> bool;

    /// Moves the waiting chat `run_id` to its terminal interface and returns the program: the harness's interactive
    /// mode in the chat's worktree, set up as the chat's own process is and continuing the chat's conversation, whose
    /// process is ended meanwhile. What is said in the interface is recorded as the chat's events. A harness without
    /// one refuses.
    fn tui(&self, _app: &Arc<App>, _run_id: i64) -> Result<std::process::Command, String> {
        Err("this harness has no terminal interface".into())
    }

    /// The terminal interface of the chat `run_id` has ended: the chat's own process starts again on the same
    /// conversation and waits for the next message, or, when the chat was ended or stopped meanwhile, the chat ends.
    fn close_tui(&self, _app: &Arc<App>, _run_id: i64) {}

    /// Starts a chat with `agent` in a branch of `repository`, of the harness configuration repository when `harness`
    /// is set, or in an empty directory, and returns it without waiting for the answer. The chat shows `message`, what
    /// the person wrote; the agent answers `prompt`, which is the message with what else the agent is told (the
    /// conversation an ended chat had, where the person is in the app). Without either the chat's agent starts and the
    /// chat waits for the first message. The chat is stored with `origin` (see `Run::origin`) and runs with `settings`
    /// in place of its agent's. A harness without chats refuses.
    #[allow(clippy::too_many_arguments)]
    fn start_chat(
        &self,
        _app: &Arc<App>,
        _agent: &str,
        _place: (Option<&Repository>, bool),
        _message: Option<&str>,
        _prompt: Option<&str>,
        _origin: &str,
        _settings: &ChatSettings,
    ) -> Result<Run, String> {
        Err("this harness has no chats".into())
    }

    /// The waiting chat `run_id` was given other settings (see `ChatSettings`), which its latest `settings_changed`
    /// event holds: its agent answers the next message with them. A harness without chats refuses.
    fn change_chat(&self, _app: &Arc<App>, _run_id: i64) -> Result<(), String> {
        Err("this harness has no chats".into())
    }

    /// Starts the Retro analyst with `prompt` in a fresh worktree of the harness configuration clone at `clone`,
    /// checked out at `base`, and returns the run without waiting for it to end. A harness without retros refuses.
    fn start_retro(
        &self,
        _app: &Arc<App>,
        _clone: &Path,
        _base: &str,
        _prompt: &str,
    ) -> Result<Run, String> {
        Err("this harness has no retros".into())
    }

    /// Starts the Autoresearcher (see `research`) with `prompt` in a fresh worktree of `repository` checked out at
    /// `base`, and returns the run without waiting for it to end. A harness without research refuses.
    fn start_research(
        &self,
        _app: &Arc<App>,
        _repository: &Repository,
        _base: &str,
        _prompt: &str,
    ) -> Result<Run, String> {
        Err("this harness has no research".into())
    }

    /// Starts a trial run (see `trial`) for `card`: the agent `agent`, as `definition` has it, is given the card's prompt
    /// in a fresh worktree of the card's repository on a branch of its own, checked out at `base`. The branch is named
    /// after `proposal` and the card. It returns the run without waiting for it to end. A harness without trials
    /// refuses.
    fn start_trial(
        &self,
        _app: &Arc<App>,
        _card: &Card,
        _agent: &str,
        _definition: AgentConfig,
        _proposal: i64,
        _base: &str,
    ) -> Result<Run, String> {
        Err("this harness has no trials".into())
    }

    /// Sends a person's message to a chat's agent; it answers when it is done with what it is doing. False when the
    /// chat is not running.
    fn say(&self, _run_id: i64, _message: &str) -> bool {
        false
    }

    /// Ends a chat: its agent is shut down and its work kept. False when the chat is not running.
    fn end(&self, _run_id: i64) -> bool {
        false
    }
}
