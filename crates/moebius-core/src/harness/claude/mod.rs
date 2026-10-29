//! The Claude Code harness: each run is Claude Code's command line, `claude -p --input-format stream-json
//! --output-format stream-json --verbose`, in the run's worktree, with the router as its only provider
//! (`ANTHROPIC_BASE_URL`) and the run's token as its key, on the agent's model, with the agent's instructions
//! (`--append-system-prompt`), skills and MCP servers. Claude Code has no bridge, so the run's token is a checked one:
//! the router checks every tool call in the answers to it and replaces a refused one by a text that names the rule (see
//! `router`). Each message is a JSON line on its stdin; what it writes is mapped to the run events Pi-Bolt's runs have
//! (see `reader`), so the board, the desktop app and the Retro read both alike.
//!
//! A run of a card, of the Retro analyst or of the Autoresearcher ends after its answer: stdin is closed at the first
//! `result`, and the run ends when Claude Code exits; what the agent left is committed as for Pi-Bolt. A steer is
//! another user message, which Claude Code answers before it exits. A chat keeps its Claude Code between messages and
//! ends when the person ends or stops it; a chat given another model gets a new Claude Code that resumes the chat's
//! session (`--resume`). A chat on Claude Code has no terminal interface, does not pause when idle and ignores the
//! reasoning level, which Claude Code's command line does not take. Stopping a run kills its process.
//!
//! `process` writes a run's configuration directory, builds the command line and starts it; `reader` reads what it
//! writes and ends the run.

mod process;
mod reader;
// Unix only: the fake `claude` (testdata/fake-claude) is a Python script started by its `#!` line.
#[cfg(all(test, unix))]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use serde_json::{Value, json};

use crate::app::{App, ChatSettings};
use crate::board::{AgentInfo, Runs};
use crate::config::{AgentConfig, AgentSet, HarnessKind};
use crate::harness::pibolt::{
    AUTORESEARCHER_AGENT, RETRO_AGENT, RunningRun, agent_list, card_run, chat_run, detached_run,
    fail, finish, has_agent, keep, prompt_for, resolve_agent, run_prompt,
};
use crate::harness::remote::Runners;
use crate::router::Router;
use crate::store::{Card, Repository, Run};

/// Starts and stops Claude Code runs.
pub struct ClaudeCode {
    /// The `claude` executable.
    pub program: PathBuf,
    /// The `moebius` executable, which runs find as `$MOEBIUS`.
    pub moebius: PathBuf,
    /// The router's base URL, such as `http://127.0.0.1:7172`.
    pub router_url: String,
    /// The desktop API's base URL, which a run of the Moebius agent is told as `MOEBIUS_URL`; set once the server
    /// listens.
    pub api_url: OnceLock<String>,
    pub router: Arc<Router>,
    /// The agents, shared with Pi-Bolt.
    pub agents: Arc<RwLock<AgentSet>>,
    /// The machines an agent's `runner` can name, shared with Pi-Bolt.
    pub runners: Arc<Runners>,
    live: Arc<Mutex<HashMap<i64, Live>>>,
}

/// A run whose Claude Code has not exited yet.
#[derive(Clone)]
struct Live {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Child>>,
    chat: bool,
    /// Why the process was asked to exit, so that its exit is no failure.
    closing: Arc<Mutex<Option<Closing>>>,
    /// The conversation's session, which a chat's next process resumes.
    session: String,
    /// The agent as this process was started with it.
    agent: AgentConfig,
    run: RunningRun,
    app: Arc<App>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Closing {
    Stopped,
    Ended,
    /// A chat goes on in a new process: the exit ends nothing.
    Restarting,
}

impl ClaudeCode {
    pub fn new(
        program: PathBuf,
        moebius: PathBuf,
        router_url: String,
        router: Arc<Router>,
        agents: Arc<RwLock<AgentSet>>,
    ) -> Self {
        Self {
            program,
            moebius,
            router_url,
            api_url: OnceLock::new(),
            router,
            agents,
            runners: Arc::new(Runners::default()),
            live: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Kills every Claude Code that is going, for a server that is about to restart.
    pub fn kill_all(&self) {
        let all: Vec<Live> = lock(&self.live).values().cloned().collect();
        for live in all {
            let _ = lock(&live.child).kill();
        }
    }

    fn agent_set(&self) -> std::sync::RwLockReadGuard<'_, AgentSet> {
        self.agents.read().unwrap_or_else(|e| e.into_inner())
    }

    /// The agent `name` as a run on Claude Code uses it, or why it cannot run on it.
    fn agent(&self, app: &App, name: &str) -> Result<AgentConfig, String> {
        on_claude_code(resolve_agent(&self.agent_set(), &app.data_dir, name)?, name)
    }

    /// Starts the run `running` of `agent` and returns it; when Claude Code cannot be started, what was set up for the
    /// run is undone and the run has failed.
    fn launch_or_undo(
        &self,
        app: &Arc<App>,
        card: Option<&Card>,
        agent: &AgentConfig,
        running: RunningRun,
        prompt: Option<&str>,
    ) -> Result<Run, String> {
        let session = process::new_session();
        if let Err(e) = self.launch(app, card, agent, running.clone(), (&session, false), prompt) {
            self.router.revoke_token(&running.token);
            running.remove_worktree(app);
            return Err(fail(app, running.id, e));
        }
        Ok(app
            .store
            .run(running.id)
            .map_err(|e| e.to_string())?
            .expect("the run exists"))
    }

    /// The chat `run_id`, if its Claude Code is going.
    fn live_chat(&self, run_id: i64) -> Option<Live> {
        lock(&self.live)
            .get(&run_id)
            .cloned()
            .filter(|live| live.chat)
    }
}

/// `agent` as Claude Code runs it, or why it cannot: Claude Code speaks only the anthropic protocol and loads no Pi
/// extensions.
fn on_claude_code(mut agent: AgentConfig, name: &str) -> Result<AgentConfig, String> {
    agent.harness = HarnessKind::ClaudeCode;
    match agent
        .harness
        .refuses(agent.protocol, !agent.extensions.is_empty())
    {
        Some(why) => Err(format!("the agent {name} cannot run: {why}")),
        None => Ok(agent),
    }
}

impl Runs for ClaudeCode {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
        note: Option<&str>,
    ) -> Result<Run, String> {
        self.start_on(app, card, agent, purpose, note, None)
    }

    fn start_on(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
        note: Option<&str>,
        runner: Option<&str>,
    ) -> Result<Run, String> {
        let mut config = self.agent(app, agent)?;
        if runner.is_some() {
            config.runner = runner.map(String::from);
        }
        let prompt = run_prompt(app, card, purpose, note);
        let code_branch = app.store.code_branch(card.id).map_err(|e| e.to_string())?;
        let worktree = |repository: &Repository, clone: &Path, run_id: i64| {
            let base = code_branch.unwrap_or_else(|| repository.base_ref());
            app.repos
                .add_worktree(&repository.name, clone, run_id, &base)
        };
        let running = card_run(
            app,
            (&self.router, &self.runners),
            card,
            (agent, &config),
            purpose,
            worktree,
        )?;
        self.launch_or_undo(app, Some(card), &config, running, Some(&prompt))
    }

    fn start_trial(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        definition: AgentConfig,
        proposal: i64,
        base: &str,
    ) -> Result<Run, String> {
        let config = on_claude_code(definition, agent)?;
        let prompt = prompt_for(app, card, "code", None);
        let worktree = |repository: &Repository, clone: &Path, _run_id: i64| {
            let name = &repository.name;
            app.repos
                .add_trial_worktree(name, clone, proposal, card.id, base)
        };
        let running = card_run(
            app,
            (&self.router, &self.runners),
            card,
            (agent, &config),
            "trial",
            worktree,
        )?;
        self.launch_or_undo(app, Some(card), &config, running, Some(&prompt))
    }

    fn start_chat(
        &self,
        app: &Arc<App>,
        agent: &str,
        place: (Option<&Repository>, bool),
        message: Option<&str>,
        prompt: Option<&str>,
        origin: &str,
        settings: &ChatSettings,
    ) -> Result<Run, String> {
        let config = on_claude_code(settings.apply(&self.agent(app, agent)?), agent)?;
        let running = chat_run(
            app,
            &self.router,
            (agent, &config),
            place,
            message,
            origin,
            settings,
        )?;
        self.launch_or_undo(app, None, &config, running, prompt)
    }

    /// A chat given another model gets a new Claude Code on the same session; another reasoning level changes nothing.
    fn change_chat(&self, app: &Arc<App>, run_id: i64) -> Result<(), String> {
        let Some(live) = self.live_chat(run_id) else {
            return Err(format!("chat {run_id} has ended"));
        };
        let stored = app.run(run_id).map_err(|e| e.to_string())?;
        let settings = app.chat_settings(run_id).map_err(|e| e.to_string())?;
        let config = on_claude_code(
            settings.apply(&self.agent(app, &stored.agent)?),
            &stored.agent,
        )?;
        if config.model == live.agent.model && config.protocol == live.agent.protocol {
            return Ok(());
        }
        {
            let mut closing = lock(&live.closing);
            if closing.is_some() {
                return Err(format!("chat {run_id} has ended"));
            }
            *closing = Some(Closing::Restarting);
        }
        lock(&live.stdin).take();
        let _ = lock(&live.child).kill();
        let _ = lock(&live.child).wait();
        let resumed = self.launch(
            app,
            None,
            &config,
            live.run.clone(),
            (&live.session, true),
            None,
        );
        if let Err(e) = resumed {
            let why = format!("cannot start the chat's claude again with its new model: {e}");
            finish(app, live.run, &self.router, &self.live, Err(why.clone()));
            return Err(why);
        }
        Ok(())
    }

    fn start_retro(
        &self,
        app: &Arc<App>,
        clone: &Path,
        base: &str,
        prompt: &str,
        files: &BTreeMap<String, String>,
    ) -> Result<Run, String> {
        let config = self.agent(app, RETRO_AGENT)?;
        let place = ("harness", clone, None);
        let running = detached_run(
            app,
            &self.router,
            (RETRO_AGENT, "retro"),
            place,
            (base, files),
            &config,
        )?;
        self.launch_or_undo(app, None, &config, running, Some(prompt))
    }

    fn start_research(
        &self,
        app: &Arc<App>,
        repository: &Repository,
        base: &str,
        prompt: &str,
    ) -> Result<Run, String> {
        let config = self.agent(app, AUTORESEARCHER_AGENT)?;
        let clone = PathBuf::from(&repository.path);
        let place = (
            repository.name.as_str(),
            clone.as_path(),
            Some(repository.id),
        );
        let purpose = (AUTORESEARCHER_AGENT, "research");
        let start = (base, &BTreeMap::new());
        let running = detached_run(app, &self.router, purpose, place, start, &config)?;
        self.launch_or_undo(app, None, &config, running, Some(prompt))
    }

    fn say(&self, run_id: i64, message: &str) -> bool {
        let Some(live) = self.live_chat(run_id) else {
            return false;
        };
        tell(&live, message)
    }

    /// Ends a chat: its stdin is closed, so Claude Code exits once it has answered, and the chat ends then.
    fn end(&self, run_id: i64) -> bool {
        let Some(live) = self.live_chat(run_id) else {
            return false;
        };
        {
            let mut closing = lock(&live.closing);
            if closing.is_some() {
                return false;
            }
            *closing = Some(Closing::Ended);
        }
        lock(&live.stdin).take().is_some()
    }

    fn has_agent(&self, name: &str) -> bool {
        has_agent(&self.agent_set(), name)
    }

    fn agents(&self) -> Vec<AgentInfo> {
        agent_list(&self.agent_set())
    }

    fn token_budget(&self, agent: &str) -> Option<i64> {
        self.agent_set().agents.get(agent)?.token_budget
    }

    /// Gives the agent the message as the next user message, which it answers before the run ends.
    fn steer(&self, run_id: i64, message: &str) -> bool {
        let Some(live) = lock(&self.live).get(&run_id).cloned() else {
            return false;
        };
        send(&live.stdin, &user_message(&live.session, message))
    }

    /// Kills the run's Claude Code; the run ends as `stopped`.
    fn stop(&self, run_id: i64) -> bool {
        let Some(live) = lock(&self.live).get(&run_id).cloned() else {
            return false;
        };
        *lock(&live.closing) = Some(Closing::Stopped);
        lock(&live.stdin).take();
        let _ = lock(&live.child).kill();
        true
    }
}

/// Gives a chat's agent the person's message, after saying so with `agent_start`, so that the chat reads as working
/// until its answer.
fn tell(live: &Live, message: &str) -> bool {
    if lock(&live.stdin).is_none() {
        return false;
    }
    keep(&live.app, live.run.id, live.chat, "agent_start", &json!({}));
    send(&live.stdin, &user_message(&live.session, message))
}

/// A user message as Claude Code's `stream-json` input takes it.
fn user_message(session: &str, text: &str) -> Value {
    json!({
        "type": "user",
        "message": { "role": "user", "content": [{ "type": "text", "text": text }] },
        "parent_tool_use_id": null,
        "session_id": session,
    })
}

/// Writes one JSON line to Claude Code; false when its stdin is closed or broken.
fn send(stdin: &Mutex<Option<ChildStdin>>, value: &Value) -> bool {
    let mut guard = lock(stdin);
    let Some(pipe) = guard.as_mut() else {
        return false;
    };
    let mut line = value.to_string();
    line.push('\n');
    pipe.write_all(line.as_bytes())
        .and_then(|_| pipe.flush())
        .is_ok()
}

/// Locks a mutex, also when a thread panicked while holding it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}
