//! The agents Moebius brings itself (the Moebius agent, the Grilling partner, the Retro analyst and the
//! Autoresearcher), with their instructions and skills, and the agent a run uses: its configuration, or for a built-in
//! agent without one the Coder's model, with the built-in instructions and skills before its own.

use std::collections::BTreeMap;
use std::path::Path;

use super::PiBolt;
use crate::app::App;
use crate::board::AgentInfo;
use crate::config::{AgentConfig, AgentSet, HarnessKind};
use crate::harness::skill;
use crate::router::Protocol;

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
     Change only harness configuration files, or, for a proposal that changes Moebius's own configuration as the \
     prompt describes, only files under moebius/, and leave the files as the format requires, since a proposal that does not parse is dropped. Do not edit the files on the branch you start on, do not push, \
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
     cargo. Files you leave in a worktree are offered to the person as a change when they offer the chat's changes, \
     between your answers, or the chat ends; commands act at once, as yours: the server records each decision you make as the Moebius agent's.\n\n\
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

impl PiBolt {
    /// The agent `name` as a run uses it (see [`resolve`]).
    pub(super) fn agent(&self, app: &App, name: &str) -> Result<AgentConfig, String> {
        resolve(&self.agent_set(), &app.data_dir, name)
    }

    /// The agent of the run `run_id`, a chat, as the chat runs it: with the model and reasoning level it was given in
    /// place of the agent's (see `ChatSettings`).
    pub(super) fn chat_config(
        &self,
        app: &App,
        run_id: i64,
        agent: &str,
    ) -> Result<AgentConfig, String> {
        let settings = app.chat_settings(run_id).map_err(|e| e.to_string())?;
        Ok(settings.apply(&self.agent(app, agent)?))
    }

    /// The agents Pi-Bolt can run (see [`agent_list`]).
    pub(super) fn agent_list(&self) -> Vec<AgentInfo> {
        agent_list(&self.agent_set())
    }
}

/// Whether `set` has the agent `name`, or Moebius brings it.
pub fn has_agent(set: &AgentSet, name: &str) -> bool {
    name == MOEBIUS_AGENT || name == GRILLING_AGENT || set.agents.contains_key(name)
}

/// The agent `name` of `set` as a run uses it, whatever its harness; the built-in skills are in `data_dir`. The Moebius
/// agent and the Retro analyst are made of their configuration, or else the Coder's model, protocol and harness, and
/// always have their built-in instructions and skills before their own.
pub fn resolve(set: &AgentSet, data_dir: &Path, name: &str) -> Result<AgentConfig, String> {
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
            harness: coder.harness,
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
    let built_in_skills = built_in_skills(name).map(|built_in| skill::dir(data_dir, built_in.name));
    agent.skills.splice(0..0, built_in_skills);
    Ok(agent)
}

/// The agents of `set` as the API lists them: the built-in chat agents first, the Moebius agent and the Grilling
/// partner, if they have a model, then the agents by name, those whose files are broken among them with their error. A
/// built-in agent with a broken file still runs, on the Coder's model.
pub fn agent_list(set: &AgentSet) -> Vec<AgentInfo> {
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
                    harness: agent.harness,
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
            harness: agent.harness,
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
            harness: HarnessKind::PiBolt,
            built_in: false,
            error: Some(error.clone()),
        });
    let mut agents: Vec<AgentInfo> = configured.chain(broken).collect();
    agents.sort_by(|a, b| a.name.cmp(&b.name));
    built_in.chain(agents).collect()
}
