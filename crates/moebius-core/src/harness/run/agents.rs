//! The agents Moebius brings itself (the Moebius agent, the Grilling partner, the Coder, the Reviewer, the Retro analyst
//! and the Autoresearcher), with their instructions and skills, and the agent a run uses: its configuration, or for a built-in
//! agent without one the Coder's model, with the built-in instructions and skills before its own.

use std::collections::BTreeMap;
use std::path::Path;

use super::coding_agents::{CODER_INSTRUCTIONS, REVIEWER_INSTRUCTIONS};
use crate::app::{App, ChatSettings};
use crate::board::AgentInfo;
use crate::config::{AgentConfig, AgentSet, EngineeringSkills};
use crate::harness::skill;
use crate::router::Protocol;

/// The name of the bundled agent that builds on Moebius, which every chat without an agent starts.
pub const MOEBIUS_AGENT: &str = "Moebius";

/// The name of the bundled agent that grills a person's idea until the work is clear.
pub const GRILLING_AGENT: &str = "Grilling-partner";

/// The name of the bundled agent that makes the change a card asks for. Its model must be configured.
pub const CODER_AGENT: &str = "Coder";

/// The name of the bundled agent that reviews a change. Without a model of its own it takes the Coder's.
pub const REVIEWER_AGENT: &str = "Reviewer";

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
     When the research is limited to a number of experiments, the prompt also says how many are left: make no more \
     levers than that, the most promising first, because Moebius stops measuring when they are used. \
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
     tries a proposal by running those cards' tasks again with it, so a proposal without that line is never tried, \
     and also on cards merged in the past, which it must not break, so propose nothing broader than the errors call \
     for. The raw traces of the report's runs (their events and diffs) are in .retro/ of your worktree: read them to \
     see how each error came about, and never commit them. \
     Change only harness configuration files, or, for a proposal that changes Moebius's own configuration as the \
     prompt describes, only files under moebius/, and leave the files as the format requires, since a proposal that does not parse is dropped. Do not edit the files on the branch you start on, do not push, \
     and do not ask anyone anything: nobody answers. If no change is called for, make no branch.";

/// What the Moebius agent is told; the `instructions` of `[agents.Moebius]` come after it, and last the section that says
/// where Moebius runs (see `tell_where_moebius_runs`).
const MOEBIUS_INSTRUCTIONS: &str = "You are the Moebius agent. You work for the person in this chat. You answer their \
     questions about Moebius and their work in it, and you build or change whatever they ask for: column scripts and \
     workflows, rules, agents, skills, MCP servers, Pi extensions, plugins, settings, repositories, issues, briefs and \
     the rest of what the web app does.\n\n\
     Know before you act. Each kind of task has a skill with its steps; read it first: moebius-operate to read the \
     state and act on it (board, inbox, cards, chats, usage, repositories, issues, research, grillings), \
     moebius-configure for agents, rules, a column's script, settings, skills and MCP servers, moebius-workflows for \
     column scripts and workflows in a repository, moebius-extensions for Pi extensions, moebius-plugins for decider \
     plugins, moebius-report to send a fix or a problem in Moebius itself to its official repository (only when the \
     person says yes). The moebius skill holds the guide (how to do each task, with examples that work) and the reference \
     (every exact name): read the page the task skill names, and check every name you use in the reference: what it \
     does not list does not exist. To know the state of Moebius (boards, cards, runs, the inbox, \
     chats, usage, agents, rules, settings), ask the server with `\"$MOEBIUS\"`, for example `\"$MOEBIUS\" board` or \
     `\"$MOEBIUS\" api GET /v1/snapshot`; the cli reference lists every command. Never guess the state.\n\n\
     Where things go. What belongs to one repository (column scripts, its rules, its checks, its review notes) is the files of its project \
     settings, `repos/<repository name>/` of the harness configuration repository; change them with `\"$MOEBIUS\" api PUT \
     /v1/repositories/<id>/scripts/<column>` and `/v1/repositories/<id>/settings/<file>`, or, in a chat in the harness \
     configuration repository, as files. What is for the whole server (agents, skills, the server's rules, MCP servers, extensions) is \
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
    /// The engineering skills it is given besides, unless its configuration turns them off with
    /// `engineering_skills = false`.
    pub engineering_skills: Vec<skill::Summary>,
}

/// The built-in instructions and skills of the agent `name`, if Moebius brings it.
pub fn built_in_agent(name: &str) -> Option<BuiltInAgent> {
    Some(BuiltInAgent {
        instructions: built_in_instructions(name)?.into(),
        skills: built_in_skills(name).map(skill::BuiltIn::summary).collect(),
        engineering_skills: engineering_skills(name)
            .into_iter()
            .map(skill::BuiltIn::summary)
            .collect(),
    })
}

/// The built-in skills of a built-in agent: the reference library (not for the Coder and the Reviewer), and for the
/// Moebius agent the task skills after it.
/// The engineering skills of the Grilling partner (see [`engineering_skills`]) are not among them.
fn built_in_skills(name: &str) -> impl Iterator<Item = &'static skill::BuiltIn> {
    let tasks: &[skill::BuiltIn] = if name == MOEBIUS_AGENT {
        &skill::TASKS
    } else {
        &[]
    };
    // The Coder and the Reviewer work from the engineering skills alone.
    let library = (name != CODER_AGENT && name != REVIEWER_AGENT).then_some(&skill::MOEBIUS);
    library.into_iter().chain(tasks)
}

/// The skills Moebius brings by default for the agent `name` besides its built-in ones: the engineering skills it has,
/// then `moebius-autonomous`. Only the Coder, the Reviewer and the Grilling partner have any.
pub(super) fn engineering_skills(name: &str) -> Vec<&'static skill::BuiltIn> {
    let mut skills = skill::engineering(name);
    if !skills.is_empty() {
        skills.push(&skill::AUTONOMOUS);
    }
    skills
}

/// The engineering skills the agent `name` is given with the setting `chosen`: the defaults of its name, none, or
/// exactly the skills the list names (in the order of the list).
pub fn engineering_skills_for(
    name: &str,
    chosen: &EngineeringSkills,
) -> Vec<&'static skill::BuiltIn> {
    match chosen {
        EngineeringSkills::Default => engineering_skills(name),
        EngineeringSkills::Off => vec![],
        EngineeringSkills::Only(names) => names
            .iter()
            .filter_map(|skill_name| {
                skill::vendored(skill_name).or_else(|| {
                    (skill_name == skill::AUTONOMOUS.name).then_some(&skill::AUTONOMOUS)
                })
            })
            .collect(),
    }
}

/// The instructions of a built-in agent, which exists without configuration.
fn built_in_instructions(name: &str) -> Option<&'static str> {
    match name {
        MOEBIUS_AGENT => Some(MOEBIUS_INSTRUCTIONS),
        GRILLING_AGENT => Some(GRILLING_INSTRUCTIONS),
        CODER_AGENT => Some(CODER_INSTRUCTIONS),
        REVIEWER_AGENT => Some(REVIEWER_INSTRUCTIONS),
        RETRO_AGENT => Some(RETRO_INSTRUCTIONS),
        AUTORESEARCHER_AGENT => Some(AUTORESEARCHER_INSTRUCTIONS),
        _ => None,
    }
}

/// The agent `name` as a chat with `settings` runs it. A built-in agent without a model of its own or of the Coder
/// runs on the model the chat was given.
pub(crate) fn chat_agent(
    set: &AgentSet,
    app: &App,
    name: &str,
    settings: &ChatSettings,
) -> Result<AgentConfig, String> {
    let fallback = settings
        .model
        .as_ref()
        .map(|model| model_only(model, settings.protocol));
    let agent = resolve_with(set, &app.data_dir, name, fallback)?;
    Ok(settings.apply(&tell_where_moebius_runs(app, name, agent)))
}

/// Adds to the instructions of the Moebius agent `agent` where Moebius runs now (see `App::where_moebius_runs`), so a
/// run always starts with the current addresses. Any other agent is returned as it is.
pub(crate) fn tell_where_moebius_runs(
    app: &App,
    name: &str,
    mut agent: AgentConfig,
) -> AgentConfig {
    if name == MOEBIUS_AGENT {
        let section = app.where_moebius_runs();
        agent.instructions = Some(match agent.instructions.take() {
            Some(text) => format!("{text}\n\n{section}"),
            None => section,
        });
    }
    agent
}

/// Whether the runs of the agent `name` operate Moebius with `"$MOEBIUS"` and the desktop API, as the run itself: the
/// Moebius agent, and the Grilling partner, which keeps a wayfinder map on the issue tracker (the API accepts its token
/// for the issue routes only, see `server::api`).
pub(crate) fn operates(name: &str) -> bool {
    name == MOEBIUS_AGENT || name == GRILLING_AGENT
}

/// Whether `set` has the agent `name`, or Moebius brings it: the Moebius agent, the Grilling partner, the Coder and the
/// Reviewer exist always, so the default workflow sends every card to review. A person who wants none changes the
/// workflow script.
pub fn has_agent(set: &AgentSet, name: &str) -> bool {
    name == MOEBIUS_AGENT
        || name == GRILLING_AGENT
        || name == CODER_AGENT
        || name == REVIEWER_AGENT
        || set.agents.contains_key(name)
}

/// The agent `name` of `set` as a run uses it, whatever its harness; the built-in skills are in `data_dir`. The Moebius
/// agent and the Retro analyst are made of their configuration, or else the Coder's model, protocol and harness, and
/// always have their built-in instructions and skills before their own.
pub fn resolve(set: &AgentSet, data_dir: &Path, name: &str) -> Result<AgentConfig, String> {
    resolve_with(set, data_dir, name, None)
}

/// An agent with only a model, for a built-in agent that has none of its own and no Coder to take one from.
fn model_only(model: &str, protocol: Option<Protocol>) -> AgentConfig {
    AgentConfig {
        model: model.to_string(),
        protocol: protocol.unwrap_or(Protocol::OpenAi),
        thinking: None,
        instructions: None,
        skills: vec![],
        extensions: vec![],
        mcp: BTreeMap::new(),
        token_budget: None,
        harness: Default::default(),
        tool_output: Default::default(),
        engineering_skills: EngineeringSkills::Default,
        runner: None,
    }
}

/// Like [`resolve`], with `fallback` for a built-in agent that has no model of its own and no Coder.
fn resolve_with(
    set: &AgentSet,
    data_dir: &Path,
    name: &str,
    fallback: Option<AgentConfig>,
) -> Result<AgentConfig, String> {
    let configured = set.agents.get(name);
    let Some(built_in) = built_in_instructions(name) else {
        let mut agent = configured
            .cloned()
            .ok_or_else(|| match set.errors.get(name) {
                Some(error) => format!("the agent {name} cannot be used: {error}"),
                None => format!("no agent named {name}"),
            })?;
        give_engineering_skills(set, data_dir, name, &mut agent);
        return Ok(agent);
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
            tool_output: coder.tool_output,
            // The agent only borrows the Coder's model and harness: it keeps the skill defaults of its own name,
            // whatever the Coder's `engineering_skills` says.
            engineering_skills: EngineeringSkills::Default,
            // A Reviewer reviews where the Coder coded; the other built-in agents run on the server.
            runner: if name == REVIEWER_AGENT {
                coder.runner.clone()
            } else {
                None
            },
        },
        (None, None) => fallback.ok_or_else(|| match (set.errors.get(name), name) {
            (Some(error), _) => format!("the agent {name} cannot be used: {error}"),
            (None, CODER_AGENT) => {
                "the Coder agent needs a model: configure [agents.Coder] or agents/Coder.toml"
                    .into()
            }
            _ => format!(
                "the {name} agent needs a model: configure [agents.{name}] or [agents.Coder]"
            ),
        })?,
    };
    agent.instructions = Some(match agent.instructions {
        Some(extra) => format!("{built_in}\n\n{extra}"),
        None => built_in.into(),
    });
    let built_in_skills = built_in_skills(name).map(|built_in| skill::dir(data_dir, built_in.name));
    agent.skills.splice(0..0, built_in_skills);
    give_engineering_skills(set, data_dir, name, &mut agent);
    Ok(agent)
}

/// Adds the engineering skills Moebius brings for the agent `name` (see [`engineering_skills`]) to `agent`'s skills,
/// before the agent's own, unless it turns them off or already names the skill. A skill of the harness configuration
/// repository with the name of one replaces it. A skill that its author hides from the model's list is named in the
/// instructions with where it is, so that the agent can read it, and what a message that starts with its name means.
fn give_engineering_skills(set: &AgentSet, data_dir: &Path, name: &str, agent: &mut AgentConfig) {
    let mut hidden = vec![];
    let mut given = vec![];
    for built_in in engineering_skills_for(name, &agent.engineering_skills) {
        let path = set
            .skill_overrides
            .get(built_in.name)
            .cloned()
            .unwrap_or_else(|| skill::dir(data_dir, built_in.name));
        if agent.skills.contains(&path) || given.contains(&path) {
            continue;
        }
        if skill::hidden_from_model(built_in.page()) {
            hidden.push(path.join("SKILL.md"));
        }
        given.push(path);
    }
    agent.skills.splice(0..0, given);
    if !hidden.is_empty() {
        let files: Vec<_> = hidden.iter().map(|p| p.display().to_string()).collect();
        let note = format!(
            "Skills that are not in your list of skills, because their author hides them from the model, are still \
             yours: read {} when you need them, and when a message starts with `/` and the name of one (such as \
             `/wayfinder`), read that skill and follow it.",
            files.join(" and ")
        );
        agent.instructions = Some(match agent.instructions.take() {
            Some(text) => format!("{text}\n\n{note}"),
            None => note,
        });
    }
}

/// The agents of `set` as the API lists them: the built-in chat agents first, the Moebius agent and the Grilling
/// partner, then the agents by name, the Coder and the Reviewer always among them, those whose files are broken with
/// their error. A built-in agent with a broken file still runs, on the Coder's model; one without a model of its own
/// or of the Coder (the Coder itself has no other to take) is listed with an empty model, and a chat with it runs on
/// the model it is given.
pub fn agent_list(set: &AgentSet) -> Vec<AgentInfo> {
    let info = |name: &str| {
        let built_in = [MOEBIUS_AGENT, GRILLING_AGENT, CODER_AGENT, REVIEWER_AGENT].contains(&name);
        let agent = set
            .agents
            .get(name)
            .or_else(|| built_in.then(|| set.agents.get(CODER_AGENT)).flatten());
        let error = set.errors.get(name).cloned();
        match agent {
            Some(agent) => AgentInfo {
                name: name.into(),
                model: agent.model.clone(),
                protocol: agent.protocol,
                thinking: agent.thinking.clone(),
                token_budget: agent.token_budget,
                harness: agent.harness,
                built_in,
                error,
            },
            None => AgentInfo {
                name: name.into(),
                model: String::new(),
                protocol: Protocol::OpenAi,
                thinking: None,
                token_budget: None,
                harness: Default::default(),
                built_in,
                error,
            },
        }
    };
    let first = [MOEBIUS_AGENT, GRILLING_AGENT];
    let mut names: Vec<&str> = set
        .agents
        .keys()
        .chain(set.errors.keys())
        .map(String::as_str)
        .chain([CODER_AGENT, REVIEWER_AGENT])
        .filter(|name| !first.contains(name))
        .collect();
    names.sort_unstable();
    names.dedup();
    first.into_iter().chain(names).map(info).collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::config::HarnessKind;

    use super::*;

    fn agent(skills: &[&str]) -> AgentConfig {
        AgentConfig {
            model: "m".into(),
            protocol: Protocol::OpenAi,
            thinking: None,
            instructions: None,
            skills: skills.iter().map(PathBuf::from).collect(),
            extensions: vec![],
            mcp: BTreeMap::new(),
            token_budget: None,
            harness: HarnessKind::PiBolt,
            tool_output: Default::default(),
            engineering_skills: EngineeringSkills::Default,
            runner: None,
        }
    }

    fn set() -> AgentSet {
        AgentSet {
            agents: BTreeMap::from([
                ("Coder".to_string(), agent(&["/own/house-style"])),
                ("Reviewer".to_string(), agent(&[])),
                ("Planner".to_string(), agent(&[])),
            ]),
            ..AgentSet::default()
        }
    }

    /// The agent's skill folders, written with `/` on every system.
    fn skills(agent: &AgentConfig) -> Vec<String> {
        agent
            .skills
            .iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    #[test]
    fn the_built_in_agents_name_their_engineering_skills_and_the_adapter() {
        let data = Path::new("/data");
        let coder = resolve(&set(), data, "Coder").unwrap();
        assert_eq!(
            skills(&coder),
            [
                "/data/skills/tdd",
                "/data/skills/diagnosing-bugs",
                "/data/skills/moebius-autonomous",
                "/own/house-style"
            ]
        );
        let reviewer = resolve(&set(), data, "Reviewer").unwrap();
        assert_eq!(
            skills(&reviewer),
            [
                "/data/skills/code-review",
                "/data/skills/moebius-autonomous"
            ]
        );
        // Another agent has none.
        assert!(resolve(&set(), data, "Planner").unwrap().skills.is_empty());

        // The Grilling partner has the reference library after them, and is told where the skill that its author hides
        // from the model is.
        let grilling = resolve(&set(), data, GRILLING_AGENT).unwrap();
        assert_eq!(
            skills(&grilling),
            [
                "/data/skills/grilling",
                "/data/skills/to-tickets",
                "/data/skills/wayfinder",
                "/data/skills/domain-modeling",
                "/data/skills/research",
                "/data/skills/prototype",
                "/data/skills/moebius-autonomous",
                "/data/skills/moebius"
            ]
        );
        let instructions = grilling.instructions.unwrap().replace('\\', "/");
        assert!(instructions.starts_with("You are the Grilling partner."));
        assert!(
            instructions.contains("read /data/skills/to-tickets/SKILL.md and /data/skills/wayfinder/SKILL.md when you need them")
                && instructions.contains("`/wayfinder`")
        );
        // The Coder and the Reviewer have their instructions built in, and no reference library.
        let coder_instructions = coder.instructions.unwrap();
        assert!(coder_instructions.starts_with("You are the Coder."));
        assert!(
            reviewer
                .instructions
                .unwrap()
                .starts_with("You are the Reviewer.")
        );

        // The built-in agents list them for the web app.
        let shown = built_in_agent(GRILLING_AGENT).unwrap();
        let names: Vec<_> = shown
            .engineering_skills
            .iter()
            .map(|s| &s.name[..])
            .collect();
        assert_eq!(
            names,
            [
                "grilling",
                "to-tickets",
                "wayfinder",
                "domain-modeling",
                "research",
                "prototype",
                "moebius-autonomous"
            ]
        );
        assert!(
            built_in_agent(MOEBIUS_AGENT)
                .unwrap()
                .engineering_skills
                .is_empty()
        );
    }

    #[test]
    fn the_coder_and_the_reviewer_exist_without_configuration_and_need_a_model() {
        let data = Path::new("/data");
        let empty = AgentSet::default();
        // The Coder has no other agent to take a model from: it says what is missing.
        let error = resolve(&empty, data, "Coder").unwrap_err();
        assert!(error.contains("needs a model"), "{error}");
        let error = resolve(&empty, data, "Reviewer").unwrap_err();
        assert!(error.contains("needs a model"), "{error}");
        assert!(has_agent(&empty, "Coder"));
        // The Reviewer exists too, so the default workflow sends every card to review.
        assert!(has_agent(&empty, "Reviewer"));

        // With only a Coder, the Reviewer takes its model, and the instructions are built in.
        let only_coder = AgentSet {
            agents: BTreeMap::from([("Coder".to_string(), agent(&[]))]),
            ..AgentSet::default()
        };
        let reviewer = resolve(&only_coder, data, "Reviewer").unwrap();
        assert_eq!(reviewer.model, "m");
        assert!(
            reviewer
                .instructions
                .as_deref()
                .unwrap()
                .starts_with("You are the Reviewer.")
        );
        assert_eq!(
            skills(&reviewer),
            [
                "/data/skills/code-review",
                "/data/skills/moebius-autonomous"
            ]
        );
    }

    #[test]
    fn an_agents_own_instructions_come_after_the_built_in_ones() {
        let mut own = agent(&[]);
        own.instructions = Some("Prefer small scripts.".into());
        let set = AgentSet {
            agents: BTreeMap::from([("Coder".to_string(), own)]),
            ..AgentSet::default()
        };
        let coder = resolve(&set, Path::new("/data"), "Coder").unwrap();
        let text = coder.instructions.unwrap();
        assert!(text.starts_with("You are the Coder."));
        assert!(text.contains("\n\nPrefer small scripts."));
    }

    #[test]
    fn the_list_always_has_the_coder_and_the_reviewer_as_built_in() {
        let names = |list: &[AgentInfo]| -> Vec<(String, String, bool)> {
            list.iter()
                .map(|a| (a.name.clone(), a.model.clone(), a.built_in))
                .collect()
        };
        let listed = names(&agent_list(&AgentSet::default()));
        assert_eq!(
            listed,
            [
                ("Moebius".to_string(), String::new(), true),
                ("Grilling-partner".to_string(), String::new(), true),
                ("Coder".to_string(), String::new(), true),
                ("Reviewer".to_string(), String::new(), true),
            ]
        );
        let listed = names(&agent_list(&set()));
        assert_eq!(listed[2], ("Coder".to_string(), "m".to_string(), true));
        assert_eq!(listed[3], ("Planner".to_string(), "m".to_string(), false));
        assert_eq!(listed[4], ("Reviewer".to_string(), "m".to_string(), true));
    }

    #[test]
    fn a_reviewer_that_takes_the_coders_model_takes_its_runner_too() {
        let data = Path::new("/data");
        let mut coder = agent(&[]);
        coder.runner = Some("gpu-box".into());
        let mut set = AgentSet {
            agents: BTreeMap::from([("Coder".to_string(), coder)]),
            ..Default::default()
        };
        assert_eq!(
            resolve(&set, data, "Reviewer").unwrap().runner.as_deref(),
            Some("gpu-box")
        );
        // Not another built-in agent, and not a Reviewer with a configuration of its own.
        assert_eq!(resolve(&set, data, RETRO_AGENT).unwrap().runner, None);
        set.agents.insert("Reviewer".into(), agent(&[]));
        assert_eq!(resolve(&set, data, "Reviewer").unwrap().runner, None);
    }

    #[test]
    fn turning_the_engineering_skills_off_leaves_the_agent_its_own() {
        let data = Path::new("/data");
        let mut off = set();
        for name in ["Coder", GRILLING_AGENT] {
            let mut agent = agent(&["/own/house-style"]);
            agent.engineering_skills = EngineeringSkills::Off;
            off.agents.insert(name.into(), agent);
        }
        let coder = resolve(&off, data, "Coder").unwrap();
        assert_eq!(skills(&coder), ["/own/house-style"]);
        let grilling = resolve(&off, data, GRILLING_AGENT).unwrap();
        assert_eq!(
            skills(&grilling),
            ["/data/skills/moebius", "/own/house-style"]
        );
        assert!(!grilling.instructions.unwrap().contains("to-tickets"));
    }

    fn only(names: &[&str]) -> EngineeringSkills {
        EngineeringSkills::Only(names.iter().map(|n| n.to_string()).collect())
    }

    #[test]
    fn a_list_gives_an_agent_exactly_those_engineering_skills_whatever_its_name() {
        let data = Path::new("/data");
        let mut set = set();
        // A copy of the Coder under another name has the Coder's skills, as the list a copy is written with.
        let mut fast = agent(&["/own/house-style"]);
        fast.engineering_skills = only(&["tdd", "diagnosing-bugs", "moebius-autonomous"]);
        set.agents.insert("Coder-fast".into(), fast);
        let mut small = agent(&[]);
        small.engineering_skills = only(&["code-review", "tdd"]);
        set.agents.insert("Planner".into(), small);
        // The Coder itself with a list has only that.
        let mut narrow = agent(&[]);
        narrow.engineering_skills = only(&["tdd"]);
        set.agents.insert("Coder".into(), narrow);

        let fast = resolve(&set, data, "Coder-fast").unwrap();
        assert_eq!(
            skills(&fast),
            [
                "/data/skills/tdd",
                "/data/skills/diagnosing-bugs",
                "/data/skills/moebius-autonomous",
                "/own/house-style"
            ]
        );
        // Exactly the list: no moebius-autonomous that it does not name.
        let small = resolve(&set, data, "Planner").unwrap();
        assert_eq!(
            skills(&small),
            ["/data/skills/code-review", "/data/skills/tdd"]
        );
        assert_eq!(
            skills(&resolve(&set, data, "Coder").unwrap()),
            ["/data/skills/tdd"]
        );
        // The Reviewer without a setting of its own keeps its defaults, not the Coder's list.
        let reviewer = resolve(&set, data, "Reviewer").unwrap();
        assert_eq!(
            skills(&reviewer),
            [
                "/data/skills/code-review",
                "/data/skills/moebius-autonomous"
            ]
        );
        // Nor does it follow the Coder in turning its skills off.
        let mut off = set.clone();
        off.agents.get_mut("Coder").unwrap().engineering_skills = EngineeringSkills::Off;
        assert_eq!(
            skills(&resolve(&off, data, "Reviewer").unwrap()),
            [
                "/data/skills/code-review",
                "/data/skills/moebius-autonomous"
            ]
        );
        // A skill of the harness repository with the name of one replaces it for every agent that has it.
        set.skill_overrides
            .insert("tdd".into(), "/harness/skills/tdd".into());
        assert_eq!(
            skills(&resolve(&set, data, "Coder-fast").unwrap())[0],
            "/harness/skills/tdd"
        );
        assert_eq!(
            skills(&resolve(&set, data, "Planner").unwrap()),
            ["/data/skills/code-review", "/harness/skills/tdd"]
        );
    }

    #[test]
    fn true_gives_the_defaults_of_the_name_false_none_and_a_list_its_own() {
        let data = Path::new("/data");
        let given = |name: &str, chosen: EngineeringSkills| {
            let mut agent = agent(&[]);
            agent.engineering_skills = chosen;
            let set = AgentSet {
                agents: BTreeMap::from([(name.to_string(), agent)]),
                ..AgentSet::default()
            };
            skills(&resolve(&set, data, name).unwrap())
        };
        assert_eq!(
            given("Planner", EngineeringSkills::Default),
            Vec::<String>::new()
        );
        assert_eq!(
            given("Reviewer", EngineeringSkills::Default),
            [
                "/data/skills/code-review",
                "/data/skills/moebius-autonomous"
            ]
        );
        assert!(given("Reviewer", EngineeringSkills::Off).is_empty());
        assert_eq!(
            given("Reviewer", only(&["grilling"])),
            ["/data/skills/grilling"]
        );
    }

    #[test]
    fn a_skill_of_the_harness_repository_replaces_the_one_moebius_brings() {
        let data = Path::new("/data");
        let mut replaced = set();
        replaced
            .skill_overrides
            .insert("tdd".into(), "/harness/skills/tdd".into());
        // The agent that names it too has it once.
        replaced
            .agents
            .insert("Coder".into(), agent(&["/harness/skills/tdd"]));
        let coder = resolve(&replaced, data, "Coder").unwrap();
        assert_eq!(
            skills(&coder),
            [
                "/data/skills/diagnosing-bugs",
                "/data/skills/moebius-autonomous",
                "/harness/skills/tdd"
            ]
        );
        replaced.agents.insert("Coder".into(), agent(&[]));
        let coder = resolve(&replaced, data, "Coder").unwrap();
        assert_eq!(skills(&coder)[0], "/harness/skills/tdd");
    }

    #[test]
    fn only_the_moebius_agent_is_told_where_moebius_runs() {
        let dir = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(crate::store::Store::in_memory(dir.path()).unwrap());
        let app = App::new(
            store,
            std::sync::Arc::new(moebius_plugin_api::Host::new(
                std::time::Duration::from_secs(1),
            )),
            dir.path(),
        );
        let plain = agent(&[]);
        let told = tell_where_moebius_runs(&app, MOEBIUS_AGENT, plain.clone());
        let text = told.instructions.unwrap();
        assert!(text.contains("The server has not said where it runs"));
        assert!(text.contains("https://github.com/Code000-211/Moebius"));
        for other in ["Coder", GRILLING_AGENT, RETRO_AGENT, AUTORESEARCHER_AGENT] {
            assert_eq!(tell_where_moebius_runs(&app, other, plain.clone()), plain);
        }
    }
}
