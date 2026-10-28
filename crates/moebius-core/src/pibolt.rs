//! The Pi-Bolt harness (spec 4): each run is `pi-bolt --mode rpc` in the run's worktree, with the router as its only
//! provider. The prompt goes in as one JSON line; every record Pi writes is stored as a run event and published on the
//! bus. Records are split on LF only, never on U+2028 or U+2029, which are valid inside JSON strings. The run ends at
//! `agent_settled`; what the agent changed is committed on the run's branch and the worktree is removed. A chat is
//! the exception: its Pi stays up at `agent_settled`, the run waits for the person's next message, and it ends when
//! the person ends or stops it (or Pi exits). A chat works in a worktree on `moebius/chat-<id>` or, without a
//! repository, in an empty directory; what it left is committed on the branch when it ends, and when the branch
//! then has commits beyond the default branch they are offered as a change without a card. A `code` run
//! that changed nothing, that is, left no change and made no commit, has failed; other runs, such as a review, are not committed. A run starts from the branch of the
//! card's latest successful `code` run, so it sees the work so far. The text of the agent's final answer is kept as
//! the run's summary. A run is stopped with `abort` and steered with `steer`. Pi runs offline and does not trust
//! configuration found in the worktree, and loads only the extensions, skills and MCP servers the agent is configured with (see `mcp`). The
//! Moebius bridge (`bridge/moebius-bridge.ts`) is loaded into every run after the agent's own extensions, so that it
//! sees the final input and content of each tool call; it asks the check for each tool call and result (see `check`). Every run is told the path of the
//! `moebius` executable as `$MOEBIUS`. The Moebius agent is always available: its instructions are built in, its skills
//! are the guide and the task skills (see `skill`), and its model comes from `[agents.Moebius]` or else from the Coder. The Retro analyst
//! (see `retro`) is built the same way; its run works in a worktree of the harness configuration repository. So is the
//! Autoresearcher (see `research`), whose run works in a worktree of the repository it researches. A person can also
//! open a waiting chat in Pi-Bolt's own interactive mode (`tui`), and it stays one conversation: the chat's idle Pi is
//! ended, Pi's interactive mode continues the chat's session (`--continue`, the same agent directory, sessions and run
//! token), and what it adds to the session file is recorded as the chat's events (see `session_events`). When the
//! interface closes, the chat's Pi starts again with `--continue` and waits for the next message.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::app::App;
use crate::blobs::redact;
use crate::board::{AgentInfo, Runs};
use crate::bus::AppEvent;
use crate::config::{AgentConfig, AgentSet};
use crate::mcp;
use crate::repos::{Worktree, git};
use crate::router::{Grant, Protocol, Router};
use crate::skill;
use crate::store::{Card, Repository, Run};

/// How long Pi may take to exit after it was asked to stop.
const STOP_GRACE: Duration = Duration::from_secs(10);

/// The bridge extension, written into each run's directory.
const BRIDGE: &str = include_str!("../bridge/moebius-bridge.ts");

/// The name of the bundled agent that builds on Moebius, which every chat without an agent starts.
pub const MOEBIUS_AGENT: &str = "Moebius";

/// The name of the bundled agent that grills a person's idea until the work is clear.
pub const GRILLING_AGENT: &str = "Grilling-partner";

/// The name of the bundled agent that turns a Retro's report into proposals. Agent names allow no spaces.
pub const RETRO_AGENT: &str = "Retro-analyst";

/// The name of the bundled agent that proposes levers for a research.
pub const AUTORESEARCHER_AGENT: &str = "Autoresearcher";

/// What the Autoresearcher is told; `agents/Autoresearcher.md` comes after it.
const AUTORESEARCHER_INSTRUCTIONS: &str = "You are the Autoresearcher. A person gave you a goal for a repository and a \
     command that measures it, and Moebius has measured the repository as it is, the baseline. You work in a git \
     worktree of the repository, at the commit the baseline was measured on. Propose levers: changes that you expect \
     to move the measurement the way the goal wants. Make each lever its own branch named research/<id>-<short-name> \
     (the prompt gives the id) from the current commit, with exactly one commit whose subject says what the lever is. \
     Moebius measures every branch and keeps the levers that are better than the baseline. The prompt lists the \
     experiments done so far with their results: do not propose a lever again, and learn from what did not work. \
     Check your levers yourself as far as you can, but do not run the measuring command to choose between them; \
     Moebius does that. Do not edit the files on the branch you start on, do not push, and do not ask anyone anything: \
     nobody answers. If no lever is left that is worth trying, make no branch.";

/// What the Grilling partner is told; the `instructions` of `[agents.Grilling-partner]` and
/// `agents/Grilling-partner.md` come after it.
const GRILLING_INSTRUCTIONS: &str = "You are the Grilling partner. A person comes to you with an idea, and you grill it: \
     ask one question at a time, wait for the answer, and challenge assumptions, gaps and wishes that hide a harder \
     problem, until you both know exactly what the work is. You work in a git worktree of the repository the idea is \
     for, so look at the code when the answers depend on it. When the work is clear, write your result into the \
     worktree as exactly one of two files. If the idea can be built, write `.moebius/grilling/issues.json`: a JSON list \
     of small issues, each doable in one card, as objects `{\"title\": \"...\", \"body\": \"...\", \"blocked_by\": [0]}` where \
     `blocked_by` lists the indexes in the list of the issues that must be done first (an empty list when none). If the \
     idea needs research before it can be built, write `.moebius/grilling/report.md`: a research report in Markdown. \
     Write one file only. Never commit these files and never push: Moebius reads them when the person ends the chat, \
     and the person confirms the issues before any card is made.";

/// What the Retro analyst is told; `agents/Retro-analyst.md` comes after it.
const RETRO_INSTRUCTIONS: &str = "You are the Retro analyst. Each day Moebius gives you a report of that day's errors: \
     failed and refused tool calls, failed and stopped runs, steering, failed checks, conflicts and the Reviewer's \
     findings. You work in a git worktree of the harness configuration repository, which holds agents/<name>.toml and \
     agents/<name>.md, skills/, rules.md, mcp.toml and extensions/. Use the moebius skill: its reference pages \
     harness-configuration and rules give the exact format of every file. Find the causes the errors share and \
     propose changes to the configuration that would avoid them, such as a clearer instruction, a rule or a skill. \
     Make each proposal its own branch named proposal/<date>-<short-name> from the current commit, with exactly one \
     commit whose message is the proposal: the subject is its title, the body says what changes and which errors it \
     addresses, and the last line is `Cards: 48, 50`, the `card` of each error in the report that it addresses. Moebius \
     tries a proposal by running those cards' tasks again with it, so a proposal without that line is never tried. \
     Change only harness configuration files, and leave the files as the format requires, since a proposal that does not parse is dropped. Do not edit the files on the branch you start on, do not push, \
     and do not ask anyone anything: nobody answers. If no change is called for, make no branch.";

/// What the Moebius agent is told; the `instructions` of `[agents.Moebius]` come after it.
const MOEBIUS_INSTRUCTIONS: &str = "You are the Moebius agent. You work for the person in this chat. You answer their \
     questions about Moebius and their work in it, and you build or change whatever they ask for: column scripts and \
     workflows, rules, agents, skills, MCP servers, Pi extensions, plugins, settings, repositories, issues, briefs and \
     the rest of what the desktop app does.\n\n\
     Know before you act. Each kind of task has a skill with its steps; read it first: moebius-operate to read the \
     state and act on it (board, inbox, cards, chats, usage, repositories, issues, research, grillings), \
     moebius-configure for agents, rules, a column's script, settings, skills and MCP servers, moebius-workflows for \
     column scripts and workflows in a repository, moebius-extensions for Pi extensions, moebius-plugins for decider \
     plugins. The moebius skill holds the guide (how to do each task, with examples that work) and the reference \
     (every exact name): read the page the task skill names, and check every name you use in the reference: what it \
     does not list does not exist. To know the state of Moebius (boards, cards, runs, the inbox, \
     chats, usage, agents, rules, settings), ask the server with `\"$MOEBIUS\"`, for example `\"$MOEBIUS\" board` or \
     `\"$MOEBIUS\" api GET /v1/snapshot`; the cli reference lists every command. Never guess the state.\n\n\
     Where things go. What belongs to one repository (column scripts, its rules, its checks) is files in its `.moebius/` \
     in your worktree. What is for the whole server (agents, skills, the server's rules, MCP servers, extensions) is \
     files in the harness configuration repository when this chat is in it, or the matching command otherwise \
     (`\"$MOEBIUS\" agent save`, `rules save`, `api PUT /v1/skills/<name>`). Settings, repositories, issues, briefs, the \
     inbox and everything else are commands. A plugin is a Rust crate as the plugin guide shows, built and tested with \
     cargo. Files you leave in a worktree are offered to the person as a change when the chat ends; commands act at \
     once, as yours: the server records each decision you make as the Moebius agent's.\n\n\
     How you work. Understand what is wanted; if it is unclear, ask one short question, but decide what you can decide \
     yourself. Build the smallest thing that does it, try it the way the guide says (`\"$MOEBIUS\" check-script` for a \
     column script, cargo test for a plugin, reading it back with the cli after a command) and fix what fails. Before \
     anything that is hard to undo (deleting, merging, dismissing, restarting the server, a setting that applies at \
     once to every run), do it only if the person asked for exactly that, and say what you did.\n\n\
     Limits. Never weaken a rule, a check or a refusal to make something pass; say so instead. Never write a secret \
     into a file, a message or a command line: name the environment variable or the stored key, and leave storing \
     keys to the person (you cannot). A refused tool call names its rule: respect it, find another way or tell the \
     person.\n\n\
     End every answer with what you made or found, how you tried it, and what the person has to do now (merge the \
     change, set a key, restart the server), if anything. Answer in the person's language, and keep it short.";

/// What a built-in agent brings itself, as `GET /v1/agents/{name}` shows it: read only, and given to every run before
/// what the agent's configuration adds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BuiltInAgent {
    pub instructions: String,
    pub skills: Vec<skill::Summary>,
}

/// The built-in instructions and skills of the agent `name`, if Moebius brings it.
pub fn built_in_agent(name: &str) -> Option<BuiltInAgent> {
    Some(BuiltInAgent {
        instructions: built_in_instructions(name)?.into(),
        skills: built_in_skills(name).map(skill::BuiltIn::summary).collect(),
    })
}

/// The built-in skills of a built-in agent: the reference library, and for the Moebius agent the task skills after it.
fn built_in_skills(name: &str) -> impl Iterator<Item = &'static skill::BuiltIn> {
    let tasks: &[skill::BuiltIn] = if name == MOEBIUS_AGENT {
        &skill::TASKS
    } else {
        &[]
    };
    std::iter::once(&skill::MOEBIUS).chain(tasks)
}

/// The instructions of a built-in agent, which exists without configuration.
fn built_in_instructions(name: &str) -> Option<&'static str> {
    match name {
        MOEBIUS_AGENT => Some(MOEBIUS_INSTRUCTIONS),
        GRILLING_AGENT => Some(GRILLING_INSTRUCTIONS),
        RETRO_AGENT => Some(RETRO_INSTRUCTIONS),
        AUTORESEARCHER_AGENT => Some(AUTORESEARCHER_INSTRUCTIONS),
        _ => None,
    }
}

/// Starts and stops Pi-Bolt runs.
pub struct PiBolt {
    /// The `pi-bolt` executable.
    pub program: PathBuf,
    /// The `moebius` executable, which runs find as `$MOEBIUS`.
    pub moebius: PathBuf,
    /// The router's base URL, such as `http://127.0.0.1:7172`.
    pub router_url: String,
    /// The desktop API's base URL, which a run of the Moebius agent is told as `MOEBIUS_URL`; set once the server
    /// listens, since the address is only known then.
    pub api_url: OnceLock<String>,
    pub router: Arc<Router>,
    /// The agents runs use. The harness configuration repository swaps them when it changes.
    pub agents: Arc<RwLock<AgentSet>>,
    live: Arc<Mutex<HashMap<i64, Live>>>,
    /// The chats whose terminal interface is open, which have no Pi of their own meanwhile. Taken before `live` by
    /// whoever takes both, and held while a chat moves between the two.
    paused: Arc<Mutex<HashMap<i64, Paused>>>,
}

/// Where a Pi starts and what it is given: the worktree it works in, its agent directory and sessions, whether it
/// continues the latest session there, the bridge extension, the token of the router and whether it operates Moebius
/// with the desktop API.
struct Place<'a> {
    worktree: &'a Path,
    agent_dir: &'a Path,
    sessions: &'a Path,
    continues: bool,
    bridge: &'a Path,
    token: &'a str,
    operates: bool,
}

/// A run whose Pi has not ended yet.
#[derive(Clone)]
struct Live {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Child>>,
    chat: bool,
    /// Why a chat's Pi was asked to exit, so that its exit is no failure.
    closing: Arc<Mutex<Option<Closing>>>,
    run: RunningRun,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Closing {
    Stopped,
    Ended,
    /// The chat goes on in its terminal interface: the exit ends nothing.
    Paused,
}

/// A chat in its terminal interface: the run its Pi held, the follower of its session file, and whether the person
/// ended or stopped the chat meanwhile.
struct Paused {
    run: RunningRun,
    follower: Follower,
    closing: Option<Closing>,
}

impl PiBolt {
    pub fn new(
        program: PathBuf,
        moebius: PathBuf,
        router_url: String,
        router: Arc<Router>,
        agents: BTreeMap<String, AgentConfig>,
    ) -> Self {
        Self {
            program,
            moebius,
            router_url,
            api_url: OnceLock::new(),
            router,
            agents: Arc::new(RwLock::new(AgentSet {
                agents,
                errors: BTreeMap::new(),
            })),
            live: Arc::new(Mutex::new(HashMap::new())),
            paused: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The agent `name` as a run uses it. The Moebius agent and the Retro analyst are made of their configuration, or
    /// else the Coder's model and protocol, and always have their built-in instructions and skills before their own.
    fn agent(&self, app: &App, name: &str) -> Result<AgentConfig, String> {
        let set = self.agent_set();
        let configured = set.agents.get(name);
        let Some(built_in) = built_in_instructions(name) else {
            return configured
                .cloned()
                .ok_or_else(|| match set.errors.get(name) {
                    Some(error) => format!("the agent {name} cannot be used: {error}"),
                    None => format!("no agent named {name}"),
                });
        };
        let mut agent = match (configured, set.agents.get("Coder")) {
            (Some(own), _) => own.clone(),
            (None, Some(coder)) => AgentConfig {
                model: coder.model.clone(),
                protocol: coder.protocol,
                thinking: None,
                instructions: None,
                skills: vec![],
                extensions: vec![],
                mcp: BTreeMap::new(),
                token_budget: None,
            },
            (None, None) => {
                return Err(format!(
                    "the {name} agent needs a model: configure [agents.{name}] or [agents.Coder]"
                ));
            }
        };
        agent.instructions = Some(match agent.instructions {
            Some(extra) => format!("{built_in}\n\n{extra}"),
            None => built_in.into(),
        });
        let built_in_skills =
            built_in_skills(name).map(|built_in| skill::dir(&app.data_dir, built_in.name));
        agent.skills.splice(0..0, built_in_skills);
        Ok(agent)
    }

    /// Kills every Pi that is going, for a server that is about to restart; the runs they leave are failed by the next
    /// start, so that the board starts them again.
    pub fn kill_all(&self) {
        let all: Vec<Live> = lock(&self.live).values().cloned().collect();
        for live in all {
            let _ = lock(&live.child).kill();
        }
    }

    /// The agents as they are now.
    fn agent_set(&self) -> std::sync::RwLockReadGuard<'_, AgentSet> {
        self.agents.read().unwrap_or_else(|e| e.into_inner())
    }

    /// The command that starts Pi for a run: `--mode rpc` for the run's own process, or, for the terminal interface of
    /// a chat, Pi's interactive mode. Both get the same provider, model, instructions, skills, MCP servers, extensions
    /// and bridge, so the checks see the same tool calls.
    fn command(&self, agent: &AgentConfig, place: &Place, interactive: bool) -> Command {
        let mut command = Command::new(&self.program);
        command
            .current_dir(place.worktree)
            .env("PI_CODING_AGENT_DIR", place.agent_dir)
            .env("MOEBIUS", &self.moebius)
            .env("MOEBIUS_CHECK_URL", &self.router_url)
            .env("MOEBIUS_RUN_TOKEN", place.token);
        if let (true, Some(url)) = (place.operates, self.api_url.get()) {
            // The Moebius agent operates Moebius with `$MOEBIUS`, as the run itself: its token is accepted by the
            // desktop API.
            command
                .env("MOEBIUS_URL", url)
                .env("MOEBIUS_TOKEN", place.token);
        }
        if !interactive {
            command.args(["--mode", "rpc"]);
        }
        command
            .args(["--offline", "--no-approve"])
            .args(["--provider", "moebius", "--model", &agent.model])
            .args(["--no-extensions", "--no-skills"])
            .arg("--session-dir")
            .arg(place.sessions);
        if place.continues {
            command.arg("--continue");
        }
        if let Some(thinking) = &agent.thinking {
            command.args(["--thinking", thinking]);
        }
        if let Some(instructions) = &agent.instructions {
            command.arg("--append-system-prompt").arg(instructions);
        }
        for skill in &agent.skills {
            command.arg("--skill").arg(skill);
        }
        // Pi's MCP support is an extension that `--no-extensions` turns off; it reads the `mcp.json` of the agent
        // directory, which holds the agent's servers.
        if !agent.mcp.is_empty() {
            command.args(["--extension", "builtin:mcp"]);
        }
        for extension in &agent.extensions {
            command.arg("--extension").arg(extension);
        }
        // Last, so that it checks the input and redacts the content the other extensions left.
        command.arg("--extension").arg(place.bridge);
        command
    }

    /// Writes the agent directory `agent_dir`: `models.json` with the router as the `moebius` provider, and `mcp.json` with
    /// the agent's MCP servers, if it has any (without it Pi connects to none). Both hold secrets (the run's token, the
    /// servers' environment), so only the server's user can read them.
    fn write_agent_dir(
        &self,
        agent_dir: PathBuf,
        agent: &AgentConfig,
        token: &str,
    ) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&agent_dir).map_err(|e| e.to_string())?;
        if !agent.mcp.is_empty() {
            let servers = mcp::pi_config(&agent.mcp, &|name| std::env::var(name).ok())?;
            write_private(&agent_dir.join("mcp.json"), &servers)?;
        }
        let (api, base_url) = match agent.protocol {
            Protocol::OpenAi => ("openai-completions", format!("{}/v1", self.router_url)),
            Protocol::Anthropic => ("anthropic-messages", self.router_url.clone()),
        };
        let models = json!({
            "providers": {
                "moebius": {
                    "baseUrl": base_url,
                    "api": api,
                    "apiKey": token,
                    "models": [{ "id": agent.model }]
                }
            }
        });
        write_private(&agent_dir.join("models.json"), &models)?;
        Ok(agent_dir)
    }

    /// Starts `agent` for `purpose`, with no card, on `prompt` in a fresh detached worktree of the clone `place` names
    /// (the name its worktrees go under, its path, and the repository the run records, if any), checked out at `base`.
    fn start_detached(
        &self,
        app: &Arc<App>,
        (agent, purpose): (&str, &str),
        (name, clone, repository): (&str, &Path, Option<i64>),
        base: &str,
        prompt: &str,
    ) -> Result<Run, String> {
        let config = self.agent(app, agent)?;
        let run = app
            .store
            .add_run(None, agent, "pi-bolt", purpose)
            .map_err(|e| e.to_string())?;
        let worktree = repository
            .map_or(Ok(()), |id| app.store.set_run_repository(run.id, id))
            .map_err(|e| e.to_string())
            .and_then(|()| {
                let worktree = format!("{purpose}-{}", run.id);
                app.repos
                    .add_detached_worktree(name, clone, &worktree, base)
                    .map_err(|e| e.to_string())
            })
            .map_err(|e| fail(app, run.id, e))?;
        let start =
            git(&worktree, &["rev-parse", "HEAD"]).map_err(|e| fail(app, run.id, e.to_string()))?;
        let running = RunningRun {
            id: run.id,
            clone: Some(clone.to_path_buf()),
            start,
            worktree,
            token: self.router.issue_token(Grant {
                run_id: run.id,
                card_id: None,
                budget: config.token_budget,
            }),
            purpose: purpose.into(),
            commit_message: String::new(),
        };
        self.launch_or_undo(app, None, &config, running, prompt)
    }

    /// Starts Pi for a run whose worktree and token exist, and the thread that reads it. Pi is given `prompt`; without
    /// one, a chat's Pi continues the chat's session and the chat waits for the person's next message.
    fn launch(
        &self,
        app: &Arc<App>,
        card: Option<&Card>,
        agent: &AgentConfig,
        run: RunningRun,
        prompt: Option<&str>,
    ) -> Result<(), String> {
        app.store
            .set_run_worktree(run.id, &run.worktree.to_string_lossy())
            .map_err(|e| e.to_string())?;
        let run_dir = app.data_dir.join("runs").join(run.id.to_string());
        let agent_dir = self
            .write_agent_dir(run_dir.join("agent"), agent, &run.token)
            .map_err(|e| format!("cannot write the agent directory: {e}"))?;
        let bridge = run_dir.join("moebius-bridge.ts");
        std::fs::write(&bridge, BRIDGE)
            .map_err(|e| format!("cannot write the bridge extension: {e}"))?;
        let stderr = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(run_dir.join("stderr.log"))
            .map_err(|e| e.to_string())?;
        let operates = app
            .store
            .run(run.id)
            .map_err(|e| e.to_string())?
            .is_some_and(|stored| stored.agent == MOEBIUS_AGENT);
        let sessions = run_dir.join("sessions");
        let place = Place {
            worktree: &run.worktree,
            agent_dir: &agent_dir,
            sessions: &sessions,
            continues: prompt.is_none(),
            bridge: &bridge,
            token: &run.token,
            operates,
        };
        let mut command = self.command(agent, &place, false);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", self.program.display()))?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let live = Live {
            stdin: Arc::new(Mutex::new(child.stdin.take())),
            child: Arc::new(Mutex::new(child)),
            chat: run.purpose == "chat",
            closing: Arc::new(Mutex::new(None)),
            run: run.clone(),
        };
        lock(&self.live).insert(run.id, live.clone());
        match prompt {
            Some(prompt) => {
                app.store
                    .set_run_status(run.id, "running")
                    .map_err(|e| e.to_string())?;
                app.bus.publish(AppEvent::new(
                    "run_started",
                    json!({ "run": run.id, "card": card.map(|card| card.id) }),
                ));
                send(
                    &live.stdin,
                    &json!({ "id": "moebius-prompt", "type": "prompt", "message": prompt }),
                );
            }
            None => app
                .store
                .set_run_status(run.id, "waiting")
                .map_err(|e| e.to_string())?,
        }
        let reader = Reader {
            app: app.clone(),
            run,
            live,
            router: self.router.clone(),
            lives: self.live.clone(),
        };
        let name = format!("pi-bolt run {}", reader.run.id);
        std::thread::Builder::new()
            .name(name)
            .spawn(move || reader.read(stdout))
            .map(drop)
            .map_err(|e| e.to_string())
    }
    /// Launches a run that has its worktree and token and returns it; when Pi cannot be started, what was set up for
    /// the run is undone and the run has failed.
    fn launch_or_undo(
        &self,
        app: &Arc<App>,
        card: Option<&Card>,
        config: &AgentConfig,
        running: RunningRun,
        prompt: &str,
    ) -> Result<Run, String> {
        if let Err(e) = self.launch(app, card, config, running.clone(), Some(prompt)) {
            // Nothing reads this run: undo what was set up for it.
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
}

impl PiBolt {
    /// Starts a run of `card` by `agent`, as `config` has it, in a worktree `worktree` makes for the run, and returns
    /// it. The agent leaves its work in the worktree, which is committed when the run ends.
    fn start_card_run(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        config: &AgentConfig,
        (purpose, prompt): (&str, &str),
        worktree: impl FnOnce(&Repository, &Path, i64) -> crate::repos::Result<Worktree>,
    ) -> Result<Run, String> {
        let repository = app
            .store
            .repository(card.repository_id)
            .map_err(|e| e.to_string())?
            .ok_or("the card's repository is gone")?;
        let run = app
            .store
            .add_run(Some(card.id), agent, "pi-bolt", purpose)
            .map_err(|e| e.to_string())?;
        let clone = PathBuf::from(&repository.path);
        let worktree =
            worktree(&repository, &clone, run.id).map_err(|e| fail(app, run.id, e.to_string()))?;
        let start = git(&worktree.path, &["rev-parse", "HEAD"])
            .map_err(|e| fail(app, run.id, e.to_string()))?;
        let running = RunningRun {
            id: run.id,
            clone: Some(clone),
            start,
            worktree: worktree.path,
            token: self.router.issue_token(Grant {
                run_id: run.id,
                card_id: Some(card.id),
                budget: config.token_budget,
            }),
            purpose: purpose.to_string(),
            commit_message: format!(
                "#{}: {}\n\nCommitted by Moebius for its run.",
                card.number, card.title
            ),
        };
        self.launch_or_undo(app, Some(card), config, running, prompt)
    }
}

impl Runs for PiBolt {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
        note: Option<&str>,
    ) -> Result<Run, String> {
        let config = self.agent(app, agent)?;
        let prompt = prompt_for(card, purpose, note);
        let code_branch = app.store.code_branch(card.id).map_err(|e| e.to_string())?;
        let worktree = |repository: &Repository, clone: &Path, run_id: i64| {
            let base = code_branch.unwrap_or_else(|| repository.base_ref());
            app.repos
                .add_worktree(&repository.name, clone, run_id, &base)
        };
        self.start_card_run(app, card, agent, &config, (purpose, &prompt), worktree)
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
        let prompt = prompt_for(card, "code", None);
        let worktree = |repository: &Repository, clone: &Path, _run_id: i64| {
            let name = &repository.name;
            app.repos
                .add_trial_worktree(name, clone, proposal, card.id, base)
        };
        self.start_card_run(app, card, agent, &definition, ("trial", &prompt), worktree)
    }

    fn start_chat(
        &self,
        app: &Arc<App>,
        agent: &str,
        repository: Option<&Repository>,
        harness: bool,
        message: &str,
        earlier: Option<&str>,
    ) -> Result<Run, String> {
        let config = self.agent(app, agent)?;
        let run = app
            .store
            .add_run(None, agent, "pi-bolt", "chat")
            .map_err(|e| e.to_string())?;
        let (clone, worktree, start) = match repository {
            None if harness => {
                let (made, clone) = app
                    .start_harness_chat(run.id)
                    .map_err(|e| fail(app, run.id, e.to_string()))?;
                let start = git(&made.path, &["rev-parse", "HEAD"])
                    .map_err(|e| fail(app, run.id, e.to_string()))?;
                (Some(clone), made.path, start)
            }
            Some(repository) => {
                let clone = PathBuf::from(&repository.path);
                let made = app
                    .store
                    .set_run_repository(run.id, repository.id)
                    .map_err(|e| e.to_string())
                    .and_then(|()| {
                        let base = repository.base_ref();
                        let name = &repository.name;
                        let worktree = app.repos.add_chat_worktree(name, &clone, run.id, &base);
                        worktree.map_err(|e| e.to_string())
                    })
                    .map_err(|e| fail(app, run.id, e))?;
                let start = git(&made.path, &["rev-parse", "HEAD"])
                    .map_err(|e| fail(app, run.id, e.to_string()))?;
                (Some(clone), made.path, start)
            }
            None => {
                let empty = app.data_dir.join("chats").join(run.id.to_string());
                std::fs::create_dir_all(&empty).map_err(|e| fail(app, run.id, e.to_string()))?;
                (None, empty, String::new())
            }
        };
        let running = RunningRun {
            id: run.id,
            clone,
            start,
            worktree,
            token: self.router.issue_token(Grant {
                run_id: run.id,
                card_id: None,
                budget: config.token_budget,
            }),
            purpose: "chat".into(),
            commit_message: format!(
                "Chat {}\n\nCommitted by Moebius when the chat ended.",
                run.id
            ),
        };
        let said = json!({ "message": redact(message) });
        let _ = app.record_run_event(run.id, "said", &said);
        // The chat shows what the person wrote; the agent is also told what was said before.
        let prompt = match earlier {
            Some(earlier) => format!(
                "This chat continues an earlier one, which ended. What was said in it:\n\n{earlier}\n\n\
                 The person now writes:\n\n{message}"
            ),
            None => message.to_string(),
        };
        self.launch_or_undo(app, None, &config, running, &prompt)
    }

    /// Ends the chat's idle Pi and returns Pi's interactive mode on the chat's own agent directory, run token and
    /// sessions, continuing the chat's session; what it adds to the session file is recorded as the chat's events
    /// until `close_tui`.
    fn tui(&self, app: &Arc<App>, run_id: i64) -> Result<Command, String> {
        let stored = app.run(run_id).map_err(|e| e.to_string())?;
        let config = self.agent(app, &stored.agent)?;
        let mut paused = lock(&self.paused);
        let ended = || format!("chat {run_id} has ended");
        let live = lock(&self.live)
            .get(&run_id)
            .cloned()
            .filter(|live| live.chat)
            .ok_or_else(ended)?;
        let run_dir = app.data_dir.join("runs").join(run_id.to_string());
        let agent_dir = self.write_agent_dir(run_dir.join("agent"), &config, &live.run.token)?;
        {
            let mut closing = lock(&live.closing);
            if closing.is_some() {
                return Err(ended());
            }
            *closing = Some(Closing::Paused);
        }
        // Closing stdin asks Pi to shut down; it is idle, as the chat waits.
        lock(&live.stdin).take();
        if !wait_for_exit(&live.child, STOP_GRACE) {
            let mut child = lock(&live.child);
            let _ = child.kill();
            let _ = child.wait();
        }
        lock(&self.live).remove(&run_id);
        let sessions = run_dir.join("sessions");
        let place = Place {
            worktree: &live.run.worktree,
            agent_dir: &agent_dir,
            sessions: &sessions,
            continues: true,
            bridge: &run_dir.join("moebius-bridge.ts"),
            token: &live.run.token,
            operates: stored.agent == MOEBIUS_AGENT,
        };
        let command = self.command(&config, &place, true);
        let follower = Follower::start(app.clone(), run_id, sessions);
        let run = live.run;
        paused.insert(
            run_id,
            Paused {
                run,
                follower,
                closing: None,
            },
        );
        Ok(command)
    }

    /// Stops following the chat's session file and starts the chat's Pi again on the same session, waiting for the
    /// next message; a chat that was ended or stopped meanwhile ends instead.
    fn close_tui(&self, app: &Arc<App>, run_id: i64) {
        let mut paused = lock(&self.paused);
        let Some(Paused {
            run,
            follower,
            closing,
        }) = paused.remove(&run_id)
        else {
            return;
        };
        follower.stop();
        if let Some(closing) = closing {
            return finish(
                app,
                run,
                &self.router,
                &self.live,
                Ok(closing == Closing::Stopped),
            );
        }
        let agent = app.run(run_id).map(|stored| stored.agent);
        let launched = agent
            .map_err(|e| e.to_string())
            .and_then(|agent| self.agent(app, &agent))
            .and_then(|config| self.launch(app, None, &config, run.clone(), None));
        if let Err(e) = launched {
            let why = format!("cannot start the chat's pi-bolt again after its terminal: {e}");
            finish(app, run, &self.router, &self.live, Err(why));
        }
    }

    fn start_retro(
        &self,
        app: &Arc<App>,
        clone: &Path,
        base: &str,
        prompt: &str,
    ) -> Result<Run, String> {
        let place = ("harness", clone, None);
        self.start_detached(app, (RETRO_AGENT, "retro"), place, base, prompt)
    }

    fn start_research(
        &self,
        app: &Arc<App>,
        repository: &Repository,
        base: &str,
        prompt: &str,
    ) -> Result<Run, String> {
        let clone = PathBuf::from(&repository.path);
        let place = (
            repository.name.as_str(),
            clone.as_path(),
            Some(repository.id),
        );
        self.start_detached(app, (AUTORESEARCHER_AGENT, "research"), place, base, prompt)
    }

    fn say(&self, run_id: i64, message: &str) -> bool {
        let Some(live) = lock(&self.live).get(&run_id).cloned().filter(|l| l.chat) else {
            return false;
        };
        // Pi starts a run for the prompt, or queues it when the agent is working, so the answer does not depend on
        // whether the agent has just settled.
        send(
            &live.stdin,
            &json!({ "type": "prompt", "message": message, "streamingBehavior": "followUp" }),
        )
    }

    fn end(&self, run_id: i64) -> bool {
        // Held throughout, so that the chat does not move to or from its terminal interface meanwhile.
        let mut paused = lock(&self.paused);
        if let Some(paused) = paused.get_mut(&run_id) {
            paused.closing = Some(Closing::Ended);
            return true;
        }
        let Some(live) = lock(&self.live).get(&run_id).cloned().filter(|l| l.chat) else {
            return false;
        };
        *lock(&live.closing) = Some(Closing::Ended);
        // Closing stdin asks Pi to shut down.
        lock(&live.stdin).take().is_some()
    }

    fn has_agent(&self, name: &str) -> bool {
        name == MOEBIUS_AGENT
            || name == GRILLING_AGENT
            || self.agent_set().agents.contains_key(name)
    }

    /// The built-in chat agents first, the Moebius agent and the Grilling partner, if they have a model, then the
    /// agents by name, those whose files are broken among them with their error. A built-in agent with a broken file
    /// still runs, on the Coder's model.
    fn agents(&self) -> Vec<AgentInfo> {
        let set = self.agent_set();
        let built_in = [MOEBIUS_AGENT, GRILLING_AGENT]
            .into_iter()
            .filter_map(|name| {
                set.agents
                    .get(name)
                    .or_else(|| set.agents.get("Coder"))
                    .map(|agent| AgentInfo {
                        name: name.into(),
                        model: agent.model.clone(),
                        protocol: agent.protocol,
                        thinking: agent.thinking.clone(),
                        token_budget: agent.token_budget,
                        built_in: true,
                        error: set.errors.get(name).cloned(),
                    })
            });
        let own = |name: &String| name != MOEBIUS_AGENT && name != GRILLING_AGENT;
        let configured = set
            .agents
            .iter()
            .filter(|(name, _)| own(name))
            .map(|(name, agent)| AgentInfo {
                name: name.clone(),
                model: agent.model.clone(),
                protocol: agent.protocol,
                thinking: agent.thinking.clone(),
                token_budget: agent.token_budget,
                built_in: false,
                error: None,
            });
        let broken = set
            .errors
            .iter()
            .filter(|(name, _)| own(name))
            .map(|(name, error)| AgentInfo {
                name: name.clone(),
                model: String::new(),
                protocol: Protocol::OpenAi,
                thinking: None,
                token_budget: None,
                built_in: false,
                error: Some(error.clone()),
            });
        let mut agents: Vec<AgentInfo> = configured.chain(broken).collect();
        agents.sort_by(|a, b| a.name.cmp(&b.name));
        built_in.chain(agents).collect()
    }

    fn token_budget(&self, agent: &str) -> Option<i64> {
        self.agent_set().agents.get(agent)?.token_budget
    }

    fn steer(&self, run_id: i64, message: &str) -> bool {
        let Some(live) = lock(&self.live).get(&run_id).cloned() else {
            return false;
        };
        send(
            &live.stdin,
            &json!({ "id": "moebius-steer", "type": "steer", "message": message }),
        )
    }

    /// Asks Pi to abort; the run ends with status `stopped` once Pi has settled. A Pi that does not exit within
    /// `STOP_GRACE` is killed.
    fn stop(&self, run_id: i64) -> bool {
        // Held throughout, so that the chat does not move to or from its terminal interface meanwhile.
        let mut paused = lock(&self.paused);
        if let Some(paused) = paused.get_mut(&run_id) {
            paused.closing = Some(Closing::Stopped);
            return true;
        }
        let Some(live) = lock(&self.live).get(&run_id).cloned() else {
            return false;
        };
        send(
            &live.stdin,
            &json!({ "id": "moebius-abort", "type": "abort" }),
        );
        if live.chat {
            // A waiting chat has no turn to abort, so Pi is also asked to exit.
            *lock(&live.closing) = Some(Closing::Stopped);
            lock(&live.stdin).take();
        }
        std::thread::spawn(move || {
            if !wait_for_exit(&live.child, STOP_GRACE) {
                let _ = lock(&live.child).kill();
            }
        });
        true
    }
}

/// What a started run holds until it ends.
#[derive(Clone)]
struct RunningRun {
    id: i64,
    /// The repository's clone; none for a chat in an empty directory.
    clone: Option<PathBuf>,
    /// The commit the worktree was made at; what the run changed is everything after it.
    start: String,
    worktree: PathBuf,
    token: String,
    purpose: String,
    /// The message of the commit of what the agent leaves in the worktree.
    commit_message: String,
}

impl RunningRun {
    /// Commits what the agent left in the worktree of a repository; false when it changed nothing, that is, left
    /// the worktree clean and made no commit of its own. A chat without a repository has nothing to commit.
    fn commit(&self) -> crate::repos::Result<bool> {
        if self.clone.is_none() {
            return Ok(false);
        }
        let worktree = &self.worktree;
        git(worktree, &["add", "--all"])?;
        if git(worktree, &["status", "--porcelain"])?.is_empty() {
            return Ok(git(worktree, &["rev-parse", "HEAD"])? != self.start);
        }
        git(worktree, &["commit", "--quiet", "-m", &self.commit_message])?;
        Ok(true)
    }

    /// Closes the terminals open on the run, then removes the worktree, or the chat's empty directory.
    fn remove_worktree(&self, app: &App) {
        app.terminals.close(self.id);
        match &self.clone {
            Some(clone) => drop(app.repos.remove_worktree(clone, &self.worktree)),
            None => drop(std::fs::remove_dir_all(&self.worktree)),
        }
    }
}

/// Fails a run that could not be started, with an event saying why; returns the message.
/// Writes `value` as JSON to a new `file` that only the server's user can read.
fn write_private(file: &Path, value: &Value) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let text = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(file)
        .and_then(|mut file| file.write_all(&text))
        .map_err(|e| e.to_string())
}

fn fail(app: &App, run_id: i64, message: String) -> String {
    let _ = app
        .store
        .append_event(run_id, "moebius_error", &json!({ "message": message }));
    let _ = app.store.set_run_status(run_id, "failed");
    message
}

/// What a `code` run is told after the issue.
const CODE_PROMPT: &str = "You are in a git worktree on the issue's own branch. Make the change, run what checks the \
     repository has, and stop when the issue is done. Moebius commits what you leave in the worktree.";

/// What a `review` run is told after the issue.
const REVIEW_PROMPT: &str = "You are the reviewer. You are in a git worktree on the branch that holds the change for \
     this issue. Review that change against the issue: is it correct, is it tested, and does it do what the issue \
     asks? Do not edit any files. Write your findings, then end your answer with one last line, either \
     `VERDICT: approve` or `VERDICT: changes`.";

/// The prompt for a run of `card`; `note` is added under a heading of its own.
fn prompt_for(card: &Card, purpose: &str, note: Option<&str>) -> String {
    let mut prompt = format!("Work on issue #{}: {}\n", card.number, card.title);
    if !card.body.trim().is_empty() {
        prompt.push('\n');
        prompt.push_str(&card.body);
        prompt.push('\n');
    }
    prompt.push('\n');
    prompt.push_str(if purpose == "review" {
        REVIEW_PROMPT
    } else {
        CODE_PROMPT
    });
    prompt.push('\n');
    if let Some(note) = note {
        prompt.push_str("\nNotes from the last round:\n\n");
        prompt.push_str(note);
        prompt.push('\n');
    }
    prompt
}

/// The text of the last assistant message in an `agent_end` record, if it has any.
pub(crate) fn final_answer(agent_end: &Value) -> Option<String> {
    let messages = agent_end["messages"].as_array()?;
    let last = messages.iter().rev().find(|m| m["role"] == "assistant")?;
    let text: String = last["content"]
        .as_array()?
        .iter()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    Some(text)
}

/// Writes one JSON line to Pi; false when its stdin is closed or broken.
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

/// Waits up to `limit` for the process to exit; true when it did.
fn wait_for_exit(child: &Mutex<Child>, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        // Bound first, so the lock is not held while sleeping.
        let exited = lock(child).try_wait();
        match exited {
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => return false,
            _ => return true,
        }
    }
}

/// Locks a mutex, also when a thread panicked while holding it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

/// Reads one run's records and ends the run.
struct Reader {
    app: Arc<App>,
    run: RunningRun,
    live: Live,
    router: Arc<Router>,
    lives: Arc<Mutex<HashMap<i64, Live>>>,
}

impl Reader {
    fn read(self, stdout: impl Read) {
        let mut lines = BufReader::new(stdout);
        let mut record = Vec::new();
        let mut ending = Err("pi-bolt exited before it settled".to_string());
        loop {
            record.clear();
            match lines.read_until(b'\n', &mut record) {
                Ok(0) => {
                    match *lock(&self.live.closing) {
                        // The chat goes on in its terminal interface, which `close_tui` ends.
                        Some(Closing::Paused) => return,
                        Some(closing) => ending = Ok(closing == Closing::Stopped),
                        None => {}
                    }
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    ending = Err(format!("cannot read from pi-bolt: {e}"));
                    break;
                }
            }
            while matches!(record.last(), Some(b'\n' | b'\r')) {
                record.pop();
            }
            if record.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_slice(&record).unwrap_or_else(|e| {
                json!({ "type": "unparsed", "error": e.to_string(), "text": String::from_utf8_lossy(&record) })
            });
            let kind = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let chat = self.run.purpose == "chat";
            let aborted = value["aborted"] == true;
            keep(&self.app, self.run.id, chat, &kind, &value);
            if kind == "response" && value["command"] == "prompt" && value["success"] == false {
                ending = Err(format!("pi-bolt refused the prompt: {}", value["error"]));
                break;
            }
            if kind == "agent_settled" && !(chat && !aborted) {
                ending = Ok(aborted);
                break;
            }
        }
        self.end(ending);
    }

    /// Ends Pi, then the run: `Ok(aborted)` when Pi settled, `Err(why)` when it failed.
    fn end(self, ending: Result<bool, String>) {
        // Closing stdin asks Pi to shut down.
        lock(&self.live.stdin).take();
        if !wait_for_exit(&self.live.child, Duration::from_secs(10)) {
            let mut child = lock(&self.live.child);
            let _ = child.kill();
            let _ = child.wait();
        }
        finish(&self.app, self.run, &self.router, &self.lives, ending);
    }
}

/// Stores what a record of Pi says about the run (the final answer as its summary; for a chat, whether the agent works
/// or waits for the person), then the record as a run event. The run is updated before the event is published, so
/// that a client acting on the event reads the run as it is now.
fn keep(app: &App, run_id: i64, chat: bool, kind: &str, value: &Value) {
    if kind == "agent_end"
        && let Some(answer) = final_answer(value)
    {
        let _ = app.store.set_run_summary(run_id, &answer);
    }
    if chat && kind == "agent_start" {
        let _ = app.store.set_run_status(run_id, "running");
    }
    // A chat waits for the person's next message; only an abort ends it.
    if chat && kind == "agent_settled" && value["aborted"] != true {
        let _ = app.store.set_run_status(run_id, "waiting");
    }
    let _ = app.record_run_event(run_id, kind, value);
}

/// Ends a run whose Pi is gone: `Ok(aborted)` when it settled, `Err(why)` when it failed. What the agent left is
/// committed (and a chat's work offered as a change), the worktree removed, the token revoked and the end published.
fn finish(
    app: &Arc<App>,
    run: RunningRun,
    router: &Router,
    lives: &Mutex<HashMap<i64, Live>>,
    ending: Result<bool, String>,
) {
    router.revoke_token(&run.token);
    let failed = |why: &str| {
        let _ = app.record_run_event(run.id, "moebius_error", &json!({ "message": why }));
        "failed"
    };
    // A grilling leaves its result for Moebius to read, not a change to offer.
    let grilling =
        run.purpose == "chat" && app.run(run.id).is_ok_and(|run| run.agent == GRILLING_AGENT);
    let status = match ending {
        Ok(aborted) if grilling => {
            if aborted {
                "stopped"
            } else {
                "succeeded"
            }
        }
        Ok(aborted) if run.purpose == "chat" => match run.commit() {
            // A chat that changed nothing has still succeeded.
            Ok(_) if aborted => "stopped",
            Ok(_) => "succeeded",
            Err(e) => failed(&e.to_string()),
        },
        Ok(true) => "stopped",
        Ok(false) if !matches!(run.purpose.as_str(), "code" | "trial") => "succeeded",
        Ok(false) => match run.commit() {
            Ok(true) => "succeeded",
            Ok(false) => failed("the agent changed nothing"),
            Err(e) => failed(&e.to_string()),
        },
        Err(why) => failed(&why),
    };
    if grilling && status != "failed" {
        // Before the worktree is removed, with the result in it.
        if let Err(e) = crate::grilling::conclude(app, run.id, &run.worktree) {
            failed(&format!("cannot read the result of the grilling: {e}"));
        }
    }
    run.remove_worktree(app);
    if run.purpose == "chat" && !grilling && status != "failed" {
        // Before the status is set, so that an ended chat has offered what it made.
        if let Err(e) = app.offer_chat_change(run.id) {
            failed(&format!("cannot offer the chat's work as a change: {e}"));
        }
    }
    let _ = app.store.set_run_status(run.id, status);
    lock(lives).remove(&run.id);
    app.bus.publish(AppEvent::new(
        "run_ended",
        json!({ "run": run.id, "status": status }),
    ));
}

/// How often the session file of a chat in its terminal interface is read.
const FOLLOW_EVERY: Duration = Duration::from_millis(500);

/// Follows the Pi session files of a chat while its terminal interface is open: every [`FOLLOW_EVERY`], the complete
/// lines added to the newest file are recorded as the chat's events (see `session_events`). What the files held when
/// it started is not recorded again.
struct Follower {
    stop: std::sync::mpsc::Sender<()>,
    thread: std::thread::JoinHandle<()>,
}

impl Follower {
    fn start(app: Arc<App>, run_id: i64, sessions: PathBuf) -> Follower {
        // How far each file has been read: those there now, to their end.
        let mut read: HashMap<PathBuf, u64> = session_files(&sessions)
            .into_iter()
            .map(|(path, len, _)| (path, len))
            .collect();
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name(format!("pi-bolt terminal {run_id}"))
            .spawn(move || {
                loop {
                    let last = !matches!(
                        stopped.recv_timeout(FOLLOW_EVERY),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                    );
                    for line in new_lines(&sessions, &mut read) {
                        let Ok(entry) = serde_json::from_slice::<Value>(&line) else {
                            continue;
                        };
                        for (kind, data) in session_events(&entry) {
                            keep(&app, run_id, true, kind, &data);
                        }
                    }
                    if last {
                        break;
                    }
                }
            })
            .expect("a thread can be started");
        Follower { stop, thread }
    }

    /// Reads what was added last, then stops.
    fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.thread.join();
    }
}

/// The session files in `sessions`: each one's path, length and when it was last written.
fn session_files(sessions: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let Ok(entries) = std::fs::read_dir(sessions) else {
        return vec![];
    };
    entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some((entry.path(), meta.len(), meta.modified().ok()?))
        })
        .collect()
}

/// The complete lines added to the newest session file in `sessions` since `read` says it was read up to, which then
/// says how far they go. A file that is new is read from its start.
fn new_lines(sessions: &Path, read: &mut HashMap<PathBuf, u64>) -> Vec<Vec<u8>> {
    use std::io::{Seek, SeekFrom};
    let newest = session_files(sessions)
        .into_iter()
        .max_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
    let Some((path, _, _)) = newest else {
        return vec![];
    };
    let from = read.get(&path).copied().unwrap_or(0);
    let mut added = Vec::new();
    let opened = std::fs::File::open(&path).and_then(|mut file| {
        file.seek(SeekFrom::Start(from))?;
        file.read_to_end(&mut added)
    });
    if opened.is_err() {
        return vec![];
    }
    // A line Pi is still writing is read the next time.
    let complete = added.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    added.truncate(complete);
    read.insert(path, from + complete as u64);
    added
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(<[u8]>::to_vec)
        .collect()
}

/// The chat's events for one entry of a Pi session file, in the shapes of the records Pi's RPC mode writes for the
/// same, so the chat reads the same either way: a person's message is a `said` event and starts the agent
/// (`agent_start`); an assistant message is a `message_update` with a `thinking_delta` or `text_delta` for each
/// thinking and text part and a `tool_execution_start` for each tool call, and, unless it ends in a tool call, the
/// end of the agent's turn (`agent_end` with the message, then `agent_settled`); a tool's result is a
/// `tool_execution_end`. Other entries (the header, model changes, the system prompt, ...) are none.
fn session_events(entry: &Value) -> Vec<(&'static str, Value)> {
    if entry["type"] != "message" {
        return vec![];
    }
    let message = &entry["message"];
    let parts = message["content"].as_array().map_or(&[][..], Vec::as_slice);
    match message["role"].as_str() {
        Some("user") => {
            let text = match &message["content"] {
                Value::String(text) => text.clone(),
                _ => parts
                    .iter()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            vec![
                ("said", json!({ "message": text })),
                ("agent_start", json!({})),
            ]
        }
        Some("assistant") => {
            let mut events = Vec::new();
            for (index, part) in parts.iter().enumerate() {
                let delta = |kind: &str, text: &Value| {
                    let update = json!({ "type": kind, "contentIndex": index, "delta": text });
                    ("message_update", json!({ "assistantMessageEvent": update }))
                };
                match part["type"].as_str() {
                    Some("text") => events.push(delta("text_delta", &part["text"])),
                    Some("thinking") => events.push(delta("thinking_delta", &part["thinking"])),
                    Some("toolCall") => events.push((
                        "tool_execution_start",
                        json!({ "toolCallId": part["id"], "toolName": part["name"], "args": part["arguments"] }),
                    )),
                    _ => {}
                }
            }
            if message["stopReason"] != "toolUse" {
                events.push(("agent_end", json!({ "messages": [message] })));
                // Not `aborted` even when the person interrupted it: the chat still waits for them.
                events.push(("agent_settled", json!({ "aborted": false })));
            }
            events
        }
        Some("toolResult") => {
            let mut end = json!({
                "toolCallId": message["toolCallId"],
                "toolName": message["toolName"],
                "result": { "content": message["content"] },
                "isError": message["isError"].as_bool().unwrap_or(false),
            });
            if message["durationMs"].is_number() {
                end["durationMs"] = message["durationMs"].clone();
            }
            vec![("tool_execution_end", end)]
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::McpServer;
    use crate::store::Store;
    use moebius_plugin_api::Host;

    fn fake_pi() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi")
    }

    fn setup() -> (tempfile::TempDir, Arc<App>, PiBolt, i64) {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("README.md"), "hello\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = Arc::new(App::new(
            store.clone(),
            Arc::new(Host::new(Duration::from_secs(1))),
            &dir.path().join("data"),
        ));
        let repo = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        let profile = AgentConfig {
            model: "qwen-local".into(),
            thinking: Some("low".into()),
            instructions: Some("Be brief.".into()),
            skills: vec![],
            extensions: vec!["/agent/own.ts".into()],
            mcp: BTreeMap::new(),
            protocol: Protocol::OpenAi,
            token_budget: Some(1000),
        };
        let pi = PiBolt::new(
            fake_pi(),
            "/usr/local/bin/moebius".into(),
            "http://127.0.0.1:9".into(),
            Arc::new(Router::new(store, vec![])),
            BTreeMap::from([("Coder".to_string(), profile)]),
        );
        (dir, app, pi, repo.id)
    }

    fn wait_until_ended(app: &App, run_id: i64) -> Run {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let run = app.store.run(run_id).unwrap().unwrap();
            if run.ended_at.is_some() {
                return run;
            }
            assert!(
                Instant::now() < deadline,
                "the run did not end: {run:?} {:?} {:?}",
                std::fs::read_to_string(app.data_dir.join(format!("runs/{run_id}/stderr.log"))),
                app.store
                    .events(run_id)
                    .unwrap()
                    .iter()
                    .map(|e| &e.kind)
                    .collect::<Vec<_>>()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_run_is_stored_event_by_event_and_its_work_committed() {
        let (_dir, app, pi, repo) = setup();
        let card = app
            .create_issue(repo, "Write done.txt", "", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        assert_eq!(run.status, "running");
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "succeeded");
        // The summary is the text of the last assistant message that agent_end reports.
        assert_eq!(run.summary.as_deref(), Some("line one line two"));

        let events = app.store.events(run.id).unwrap();
        let kinds: Vec<_> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "response",
                "agent_start",
                "message_update",
                "tool_execution_start",
                "tool_execution_end",
                "agent_end",
                "agent_settled"
            ]
        );
        // U+2028 inside a string is part of the record, not a line break.
        assert_eq!(
            events[2].data["assistantMessageEvent"]["delta"],
            "line one\u{2028}line two"
        );

        let clone = PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        let branch = format!("moebius/run-{}", run.id);
        assert_eq!(
            git(&clone, &["show", &format!("{branch}:done.txt")]).unwrap(),
            "done"
        );
        assert!(!PathBuf::from(run.worktree.unwrap()).exists());

        let run_dir = app.data_dir.join("runs").join(run.id.to_string());
        let argv: Vec<String> =
            serde_json::from_slice(&std::fs::read(run_dir.join("agent/argv.json")).unwrap())
                .unwrap();
        assert!(argv.windows(2).any(|w| w == ["--model", "qwen-local"]));
        assert!(argv.iter().any(|a| a == "--no-approve") && argv.iter().any(|a| a == "--offline"));
        assert!(argv.windows(2).any(|w| w == ["--thinking", "low"]));
        // Only the configured extensions and skills load, and the bridge is the last extension.
        let bridge = run_dir.join("moebius-bridge.ts");
        assert!(
            argv.iter().any(|a| a == "--no-extensions") && argv.iter().any(|a| a == "--no-skills")
        );
        let extensions: Vec<_> = argv.windows(2).filter(|w| w[0] == "--extension").collect();
        assert_eq!(extensions[0][1], "/agent/own.ts");
        assert_eq!(extensions.last().unwrap()[1], bridge.to_str().unwrap());
        assert_eq!(std::fs::read_to_string(&bridge).unwrap(), BRIDGE);
        let env: Value =
            serde_json::from_slice(&std::fs::read(run_dir.join("agent/env.json")).unwrap())
                .unwrap();
        assert_eq!(env["MOEBIUS"], "/usr/local/bin/moebius");
        assert_eq!(env["MOEBIUS_CHECK_URL"], "http://127.0.0.1:9");
        assert!(
            env["MOEBIUS_RUN_TOKEN"]
                .as_str()
                .unwrap()
                .starts_with("moebius-run-")
        );
        assert!(
            argv.windows(2)
                .any(|w| w == ["--append-system-prompt", "Be brief."])
        );
        let models: Value =
            serde_json::from_slice(&std::fs::read(run_dir.join("agent/models.json")).unwrap())
                .unwrap();
        // The run's token is in the file, so only the server's user can read it.
        let mode = std::fs::metadata(run_dir.join("agent/models.json"))
            .unwrap()
            .permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        let provider = &models["providers"]["moebius"];
        assert_eq!(provider["baseUrl"], "http://127.0.0.1:9/v1");
        assert!(
            provider["apiKey"]
                .as_str()
                .unwrap()
                .starts_with("moebius-run-")
        );
    }

    #[test]
    fn a_run_gets_exactly_the_skills_and_mcp_servers_of_its_agent() {
        let (_dir, app, pi, repo) = setup();
        let server = |text: &str| {
            let servers = mcp::parse_file(&format!("[s]\n{text}"), &["CARGO_PKG_NAME".to_string()]);
            servers.unwrap().remove("s").unwrap().unwrap()
        };
        let agent = |skill: &str, mcp: &[(&str, McpServer)]| AgentConfig {
            skills: vec![format!("/skills/{skill}").into()],
            extensions: vec![],
            mcp: mcp
                .iter()
                .map(|(name, server)| (name.to_string(), server.clone()))
                .collect(),
            ..pi.agent_set().agents["Coder"].clone()
        };
        let docs = server("command = \"docs\"\nargs = [\"--name\", \"${CARGO_PKG_NAME}\"]\n");
        let search = server(
            "url = \"https://search.test/mcp\"\nheaders = { Name = \"${CARGO_PKG_NAME}\" }\n",
        );
        let agents = [
            ("A".to_string(), agent("a", &[("docs", docs)])),
            ("B".to_string(), agent("b", &[("search", search)])),
            ("C".to_string(), agent("c", &[])),
        ];
        pi.agents.write().unwrap().agents.extend(agents);
        let run_agent = |name: &str| {
            let card = app
                .create_issue(repo, "Write done.txt", "", "person")
                .unwrap();
            let run = pi.start(&app, &card, name, "code", None).unwrap();
            let run = wait_until_ended(&app, run.id);
            let agent_dir = app
                .data_dir
                .join("runs")
                .join(run.id.to_string())
                .join("agent");
            let argv: Vec<String> =
                serde_json::from_slice(&std::fs::read(agent_dir.join("argv.json")).unwrap())
                    .unwrap();
            (argv, agent_dir.join("mcp.json"))
        };
        let values = |argv: &[String], flag: &str| -> Vec<String> {
            argv.windows(2)
                .filter(|w| w[0] == flag)
                .map(|w| w[1].clone())
                .collect()
        };

        let (argv, mcp_json) = run_agent("A");
        assert_eq!(values(&argv, "--skill"), ["/skills/a"]);
        let mode = std::fs::metadata(&mcp_json).unwrap().permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        let config: Value = serde_json::from_slice(&std::fs::read(mcp_json).unwrap()).unwrap();
        assert_eq!(
            config,
            json!({ "mcpServers": { "docs": {
                "command": "docs", "args": ["--name", "moebius-core"], "env": {}, "exposure": "direct"
            } } })
        );
        // Pi's MCP support is switched on explicitly, before the bridge, which is still last.
        let extensions = values(&argv, "--extension");
        assert_eq!(extensions[0], "builtin:mcp");
        assert!(extensions.last().unwrap().ends_with("moebius-bridge.ts"));

        let (argv, mcp_json) = run_agent("B");
        assert_eq!(values(&argv, "--skill"), ["/skills/b"]);
        let config: Value = serde_json::from_slice(&std::fs::read(mcp_json).unwrap()).unwrap();
        assert_eq!(
            config,
            json!({ "mcpServers": { "search": {
                "url": "https://search.test/mcp", "headers": { "Name": "moebius-core" }, "exposure": "direct"
            } } })
        );

        // An agent without servers gets no mcp.json and no MCP support.
        let (argv, mcp_json) = run_agent("C");
        assert_eq!(values(&argv, "--skill"), ["/skills/c"]);
        assert!(!mcp_json.exists());
        assert_eq!(values(&argv, "--extension").len(), 1);
    }

    #[test]
    fn a_review_run_starts_from_the_code_branch_and_commits_nothing() {
        let (_dir, app, pi, repo) = setup();
        {
            let mut set = pi.agents.write().unwrap();
            let coder = set.agents["Coder"].clone();
            set.agents.insert("Reviewer".into(), coder);
        }
        let card = app
            .create_issue(repo, "Write done.txt", "", "person")
            .unwrap();
        let code = pi.start(&app, &card, "Coder", "code", None).unwrap();
        let code = wait_until_ended(&app, code.id);

        let review = pi
            .start(&app, &card, "Reviewer", "review", Some("Check the edge."))
            .unwrap();
        let review = wait_until_ended(&app, review.id);
        // Nothing was changed, and that is no failure for a review.
        assert_eq!(review.status, "succeeded");
        assert_eq!(
            review.summary.as_deref(),
            Some("Looks fine.\nVERDICT: approve")
        );
        let clone = PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        assert_eq!(
            git(&clone, &["rev-parse", &review.branch()]).unwrap(),
            git(&clone, &["rev-parse", &code.branch()]).unwrap()
        );
        assert_eq!(app.store.code_branch(card.id).unwrap(), Some(code.branch()));

        let prompt_of = |run: &Run| {
            let file = format!("runs/{}/agent/prompt.txt", run.id);
            std::fs::read_to_string(app.data_dir.join(file)).unwrap()
        };
        let prompt = prompt_of(&review);
        assert!(prompt.contains("Work on issue #1: Write done.txt"));
        assert!(prompt.contains("VERDICT: approve"));
        assert!(prompt.contains("Notes from the last round:\n\nCheck the edge."));
        let prompt = prompt_of(&code);
        assert!(prompt.contains("Moebius commits what you leave"));
        assert!(!prompt.contains("Notes from the last round"));
    }

    #[test]
    fn a_run_can_be_stopped() {
        let (_dir, app, pi, repo) = setup();
        let card = app
            .create_issue(repo, "Please [hang]", "", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        // Wait until Pi has taken the prompt.
        let deadline = Instant::now() + Duration::from_secs(20);
        while app.store.events(run.id).unwrap().len() < 2 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(pi.stop(run.id));
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "stopped");
        let last = app.store.events(run.id).unwrap().pop().unwrap();
        assert_eq!(
            (last.kind.as_str(), &last.data["aborted"]),
            ("agent_settled", &json!(true))
        );
    }

    #[test]
    fn a_run_that_changed_nothing_has_failed() {
        let (_dir, app, pi, repo) = setup();
        let card = app
            .create_issue(repo, "Do [nothing]", "", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "failed");
        let last = app.store.events(run.id).unwrap().pop().unwrap();
        assert_eq!(last.data["message"], "the agent changed nothing");
    }

    #[test]
    fn a_run_that_committed_its_own_work_has_succeeded() {
        let (_dir, app, pi, repo) = setup();
        let card = app.create_issue(repo, "Do [commit]", "", "person").unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "succeeded");
        let clone = PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        let shown = git(&clone, &["show", &format!("{}:own.txt", run.branch())]);
        assert_eq!(shown.unwrap(), "own");
    }

    #[test]
    fn an_unknown_agent_or_a_missing_program_fails_cleanly() {
        let (_dir, app, mut pi, repo) = setup();
        let card = app.create_issue(repo, "Anything", "", "person").unwrap();
        assert!(pi.start(&app, &card, "Nobody", "code", None).is_err());
        pi.program = PathBuf::from("/nonexistent/pi-bolt");
        assert!(pi.start(&app, &card, "Coder", "code", None).is_err());
        let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        assert_eq!(run.status, "failed");
        assert!(!PathBuf::from(run.worktree.unwrap()).exists());
    }

    #[test]
    fn only_complete_lines_added_to_the_newest_session_file_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old.jsonl");
        std::fs::write(&old, "{\"type\":\"session\"}\n").unwrap();
        let mut read: HashMap<PathBuf, u64> = session_files(dir.path())
            .into_iter()
            .map(|(path, len, _)| (path, len))
            .collect();
        assert!(
            new_lines(dir.path(), &mut read).is_empty(),
            "what was there is not read again"
        );

        let mut file = std::fs::OpenOptions::new().append(true).open(&old).unwrap();
        file.write_all(b"{\"a\":1}\n{\"b\":").unwrap();
        assert_eq!(new_lines(dir.path(), &mut read), [b"{\"a\":1}".to_vec()]);
        file.write_all(b"2}\n").unwrap();
        assert_eq!(new_lines(dir.path(), &mut read), [b"{\"b\":2}".to_vec()]);

        // A session started later is read from its start.
        let new = dir.path().join("new.jsonl");
        std::fs::write(&new, "{\"c\":3}\n").unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(60);
        let opened = std::fs::File::options().write(true).open(&new).unwrap();
        opened.set_modified(later).unwrap();
        assert_eq!(new_lines(dir.path(), &mut read), [b"{\"c\":3}".to_vec()]);
    }

    #[test]
    fn a_session_entry_is_recorded_as_pis_records_of_the_same() {
        let entry = |message: Value| json!({ "type": "message", "id": "a1", "parentId": null, "message": message });
        let kinds = |events: Vec<(&'static str, Value)>| -> Vec<&'static str> {
            events.into_iter().map(|(kind, _)| kind).collect()
        };
        assert!(session_events(&json!({ "type": "session", "version": 3 })).is_empty());
        assert!(session_events(&entry(json!({ "role": "system", "content": "" }))).is_empty());
        let said = session_events(&entry(json!({ "role": "user", "content": "Hi" })));
        assert_eq!(said[0], ("said", json!({ "message": "Hi" })));
        let working = json!({ "role": "assistant", "stopReason": "toolUse", "content": [
            { "type": "text", "text": "Looking." },
            { "type": "toolCall", "id": "c1", "name": "bash", "arguments": { "command": "ls" } }
        ]});
        assert_eq!(
            kinds(session_events(&entry(working))),
            ["message_update", "tool_execution_start"]
        );
        let done = json!({ "role": "assistant", "stopReason": "stop", "content": [{ "type": "text", "text": "Done." }] });
        let events = session_events(&entry(done));
        assert_eq!(
            kinds(events.clone()),
            ["message_update", "agent_end", "agent_settled"]
        );
        assert_eq!(final_answer(&events[1].1).as_deref(), Some("Done."));
    }
}
