//! A new harness configuration repository, made from the agents and rules of `moebius.toml` (`POST /v1/harness/setup`
//! and `moebius init`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::AgentSettings;
use super::read::{Layout, read_agent_at};
use crate::app::{AppError, Result};
use crate::config::{AgentConfig, valid_agent_name};
use crate::harness::skill;
use crate::repos::git;

/// The `mcp.toml` of a new repository: no servers yet.
const NEW_MCP_TOML: &str = "# The MCP servers agents can name, one table each (see harness-configuration.md of the Moebius reference).\n";

/// The `README.md` of a new repository.
const NEW_README: &str = "# Harness configuration\n\nThe agents, skills, MCP servers and rules of a Moebius server: \
agents/<name>.toml and agents/<name>.md, skills/<name>/SKILL.md, extensions/, mcp.toml and rules.md. The server clones \
this repository; an edit in the app is a commit here. The format is docs/reference/harness-configuration.md of Moebius.\n";

/// The message of the first commit of a new repository.
pub const SET_UP_MESSAGE: &str = "Set up the harness configuration repository";

/// Makes `dir`, which must not exist or be empty, a new harness configuration repository on `main` that holds `agents`
/// (as `moebius.toml` has them) and the rules of the file `rules` (empty when there is none), with an empty `mcp.toml`
/// and a `README.md`: what `POST /v1/harness/setup` and `moebius init` set up. Each skill directory an agent names is
/// copied to `skills/<its name>/`, and each extension file to `extensions/<its name>`; the agents are read back as the
/// server reads them before anything is committed. A push to the repository updates its working tree, so that the
/// server's clone can push edits to it. Whatever fails, `dir` is left as it was found. Gives where each skill and
/// extension went, by the path it had.
pub fn create(
    dir: &Path,
    agents: &BTreeMap<String, AgentConfig>,
    rules: Option<&Path>,
) -> Result<BTreeMap<PathBuf, String>> {
    let existed = dir.exists();
    if existed && std::fs::read_dir(dir)?.next().is_some() {
        return Err(AppError::Conflict(format!(
            "{} exists and is not empty",
            dir.display()
        )));
    }
    let made = fill(dir, agents, rules);
    if made.is_err() {
        let _ = std::fs::remove_dir_all(dir);
        if existed {
            let _ = std::fs::create_dir(dir);
        }
    }
    made
}

/// Writes and commits what `create` describes into `dir`.
fn fill(
    dir: &Path,
    agents: &BTreeMap<String, AgentConfig>,
    rules: Option<&Path>,
) -> Result<BTreeMap<PathBuf, String>> {
    std::fs::create_dir_all(dir.join("agents"))?;
    let mut moved = BTreeMap::new();
    for (name, agent) in agents {
        let mut skills = vec![];
        for path in &agent.skills {
            // A skill may be named by its SKILL.md; the skill is the directory.
            let source = match path.file_name() {
                Some(file) if file == "SKILL.md" && path.is_file() => path.parent().unwrap_or(path),
                _ => path.as_path(),
            };
            if !source.join("SKILL.md").is_file() {
                return Err(AppError::Invalid(format!(
                    "the skill {} of the agent {name} is not a directory with a SKILL.md",
                    path.display()
                )));
            }
            let skill = source
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !valid_agent_name(&skill) || skill::built_in(&skill).is_some() {
                return Err(AppError::Invalid(format!(
                    "the skill {} of the agent {name} cannot be named {skill:?} in skills/: rename its directory \
                     (letters, digits, - and _, not the name of a built-in skill)",
                    path.display()
                )));
            }
            let place = place_in(&mut moved, source, "skills", &skill)?;
            if !dir.join(&place).exists() {
                copy_dir(source, &dir.join(&place))?;
            }
            skills.push(skill);
        }
        let mut extensions = vec![];
        for path in &agent.extensions {
            if !path.is_file() {
                return Err(AppError::Invalid(format!(
                    "the extension {} of the agent {name} is not a file",
                    path.display()
                )));
            }
            let file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let place = place_in(&mut moved, path, "extensions", &file)?;
            std::fs::create_dir_all(dir.join("extensions"))?;
            std::fs::copy(path, dir.join(&place))?;
            extensions.push(place);
        }
        let settings = AgentSettings {
            model: agent.model.clone(),
            protocol: agent.protocol,
            thinking: agent.thinking.clone(),
            token_budget: agent.token_budget,
            skills,
            extensions,
            mcp: vec![],
            harness: agent.harness,
            tool_output: agent.tool_output,
            engineering_skills: agent.engineering_skills,
            runner: agent.runner.clone(),
        };
        let settings = toml::to_string(&settings)
            .map_err(|e| AppError::Invalid(format!("cannot write the agent {name}: {e}")))?;
        std::fs::write(dir.join("agents").join(format!("{name}.toml")), settings)?;
        if let Some(text) = agent.instructions.as_deref().map(str::trim)
            && !text.is_empty()
        {
            std::fs::write(
                dir.join("agents").join(format!("{name}.md")),
                format!("{text}\n"),
            )?;
        }
    }
    match rules.filter(|file| file.is_file()) {
        Some(file) => drop(std::fs::copy(file, dir.join("rules.md"))?),
        None => std::fs::write(dir.join("rules.md"), "")?,
    }
    std::fs::write(dir.join("mcp.toml"), NEW_MCP_TOML)?;
    std::fs::write(dir.join("README.md"), NEW_README)?;
    let layout = Layout::read(dir, &[]);
    for name in agents.keys() {
        read_agent_at(dir, &layout, name).map_err(|why| {
            AppError::Invalid(format!(
                "the agent {name} would not work in the repository: {why}"
            ))
        })?;
    }
    git(dir, &["init", "--quiet", "--initial-branch=main"])?;
    git(
        dir,
        &["config", "receive.denyCurrentBranch", "updateInstead"],
    )?;
    git(dir, &["add", "--all"])?;
    git(dir, &["commit", "--quiet", "-m", SET_UP_MESSAGE])?;
    Ok(moved)
}

/// The place `<under>/<name>` in a new repository that `source` is copied to: the same place for the same source, and
/// an error when two sources would share one.
fn place_in(
    moved: &mut BTreeMap<PathBuf, String>,
    source: &Path,
    under: &str,
    name: &str,
) -> Result<String> {
    if let Some(place) = moved.get(source) {
        return Ok(place.clone());
    }
    let place = format!("{under}/{name}");
    if let Some((other, _)) = moved.iter().find(|(_, p)| **p == place) {
        return Err(AppError::Invalid(format!(
            "{} and {} would both be {place}: rename one of them",
            other.display(),
            source.display()
        )));
    }
    moved.insert(source.to_path_buf(), place.clone());
    Ok(place)
}

/// Copies the directory `from` and everything below it to `to`, which is made; a `.git` in it is left out.
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let path = entry?.path();
        if path.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        let target = to.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}
