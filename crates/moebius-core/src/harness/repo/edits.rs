//! A person's edits of the clone: an agent, a skill, an MCP server or the rules written, committed on the default
//! branch and pushed to `origin`, or undone when anything fails, and recorded as the person's decision.

use std::collections::BTreeMap;
use std::path::Path;

use super::read::agent_names;
use super::{
    AgentDefinition, AgentSettings, AgentView, HarnessRepo, McpServerView, SkillText, SkillView,
};
use crate::app::{App, AppError, Result};
use crate::config::valid_agent_name;
use crate::decide::Actor;
use crate::decide::rules;
use crate::harness::mcp::{self, McpServer};
use crate::harness::skill;
use crate::repos::git;

impl HarnessRepo {
    /// A person adds or changes the agent `name`: the definition is validated as a file read is, committed on the
    /// default branch and pushed to `origin`. An edit that changes nothing commits nothing.
    pub fn save_agent(
        &self,
        app: &App,
        by: &Actor,
        name: &str,
        definition: &AgentDefinition,
    ) -> Result<AgentView> {
        let mut read_at = self.lock();
        if !valid_agent_name(name) {
            return Err(AppError::Invalid(format!(
                "{name:?} is not a valid agent name (letters, digits, - and _)"
            )));
        }
        self.layout()
            .check(&self.path, &definition.settings)
            .map_err(AppError::Invalid)?;
        let settings = toml::to_string(&definition.settings)
            .map_err(|e| AppError::Invalid(format!("cannot write the settings: {e}")))?;
        let verb = if self.agent_file(name, "toml").exists() {
            "Update"
        } else {
            "Add"
        };
        let context = BTreeMap::from([
            ("agent".to_string(), name.to_string()),
            ("settings".to_string(), settings.clone()),
            ("instructions".to_string(), definition.instructions.clone()),
        ]);
        let write = || {
            std::fs::create_dir_all(self.path.join("agents"))?;
            std::fs::write(self.agent_file(name, "toml"), &settings)?;
            let markdown = self.agent_file(name, "md");
            if definition.instructions.trim().is_empty() {
                remove_if_there(&markdown)
            } else {
                Ok(std::fs::write(&markdown, &definition.instructions)?)
            }
        };
        let message = format!("{verb} the agent {name}");
        let files = format!("agents/{name}.*");
        let commit = self.commit(
            app,
            by,
            &mut read_at,
            &Edit {
                kind: "edit_agent",
                files: &files,
                message: &message,
            },
            context,
            write,
        )?;
        Ok(AgentView {
            name: name.into(),
            definition: definition.clone(),
            commit,
        })
    }

    /// A person deletes the agent `name`, the same way.
    pub fn delete_agent(&self, app: &App, by: &Actor, name: &str) -> Result<()> {
        let mut read_at = self.lock();
        let (settings, markdown) = (self.agent_file(name, "toml"), self.agent_file(name, "md"));
        if !valid_agent_name(name) || !(settings.exists() || markdown.exists()) {
            return Err(AppError::NotFound(format!("agent {name}")));
        }
        let write = || {
            remove_if_there(&settings)?;
            remove_if_there(&markdown)
        };
        let message = format!("Delete the agent {name}");
        let files = format!("agents/{name}.*");
        let edit = Edit {
            kind: "edit_agent",
            files: &files,
            message: &message,
        };
        let context = BTreeMap::from([("agent".to_string(), name.to_string())]);
        self.commit(app, by, &mut read_at, &edit, context, write)
            .map(drop)
    }

    /// A person replaces the server's rules, the `rules.md` of the repository: committed on the default branch and
    /// pushed to `origin`, the same way. The text must pass `rules::check`. The short id of the commit is given;
    /// text that changes nothing makes no commit. Every tool call reads the file afresh, so the next one is checked
    /// against the new rules.
    pub fn save_rules(&self, app: &App, by: &Actor, text: &str) -> Result<Option<String>> {
        let mut read_at = self.lock();
        rules::check(text).map_err(AppError::Invalid)?;
        let mut text = text.to_string();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        let edit = Edit {
            kind: "edit_rules",
            files: "rules.md",
            message: "Update the rules",
        };
        let context = BTreeMap::from([("text".to_string(), text.clone())]);
        let write = || Ok(std::fs::write(self.path.join("rules.md"), &text)?);
        self.commit(app, by, &mut read_at, &edit, context, write)
    }

    /// Makes the change `write` does to the files of `edit` a commit on the default branch and pushes it. Whatever
    /// fails before the push is through, the working tree and the default branch are as they were. On success the
    /// agents are read again, the person's decision is recorded and the short id of the commit is given; a change
    /// that changed nothing makes no commit.
    fn commit(
        &self,
        app: &App,
        by: &Actor,
        read_at: &mut String,
        edit: &Edit,
        mut context: BTreeMap<String, String>,
        write: impl FnOnce() -> Result<()>,
    ) -> Result<Option<String>> {
        let Edit {
            kind,
            files,
            message,
        } = *edit;
        let before = git(&self.path, &["rev-parse", "HEAD"]).map_err(|_| {
            AppError::Invalid("the harness configuration repository has no commits".into())
        })?;
        let undo = |error: AppError| {
            let _ = git(&self.path, &["reset", "--hard", "--quiet", &before]);
            let _ = git(&self.path, &["clean", "-fdq", "--", files]);
            error
        };
        write().map_err(undo)?;
        git(&self.path, &["add", "-A", "--", files]).map_err(|e| undo(e.into()))?;
        if git(&self.path, &["diff", "--cached", "--quiet"]).is_ok() {
            return Ok(None);
        }
        let branch = self
            .repos
            .default_branch(&self.path)
            .map_err(|e| undo(e.into()))?;
        git(&self.path, &["commit", "--quiet", "-m", message]).map_err(|e| undo(e.into()))?;
        let header = app.git_auth(&self.url).map_err(undo)?;
        self.repos
            .push(&self.path, &branch, header.as_deref())
            .map_err(|e| undo(e.into()))?;
        let head = git(&self.path, &["rev-parse", "HEAD"])?;
        let short = git(&self.path, &["rev-parse", "--short", "HEAD"])?;
        self.read(app, read_at, head);
        context.insert("change".into(), message.into());
        app.record_action(by, kind, None, None, context, "")?;
        Ok(Some(short))
    }
}

/// A change a person makes to the repository's files, as `HarnessRepo::commit` takes it.
#[derive(Clone, Copy)]
struct Edit<'a> {
    /// The kind of the decision recorded once the change is pushed.
    kind: &'a str,
    /// The pathspec of the files the change touches.
    files: &'a str,
    /// The commit message.
    message: &'a str,
}

impl HarnessRepo {
    /// A person adds or changes the skill `name`: its `SKILL.md` is written with the description and text, committed
    /// on the default branch and pushed to `origin`, the same way as an agent. The skill's other files are left alone.
    pub fn save_skill(
        &self,
        app: &App,
        by: &Actor,
        name: &str,
        new: &SkillText,
    ) -> Result<SkillView> {
        let mut read_at = self.lock();
        if skill::built_in(name).is_some() {
            return Err(built_in(name));
        }
        if !valid_agent_name(name) {
            return Err(AppError::Invalid(format!(
                "{name:?} is not a valid skill name (letters, digits, - and _)"
            )));
        }
        let description = new.description.trim();
        if description.is_empty() || description.contains('\n') {
            return Err(AppError::Invalid(
                "a skill needs a description of one line".into(),
            ));
        }
        let mut text = new.text.clone();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        let file = self.path.join("skills").join(name).join("SKILL.md");
        let verb = if file.is_file() { "Update" } else { "Add" };
        let page = skill::page(name, description, &text);
        let context = BTreeMap::from([
            ("skill".to_string(), name.to_string()),
            ("description".to_string(), description.to_string()),
            ("text".to_string(), text),
        ]);
        let write = || {
            std::fs::create_dir_all(file.parent().expect("a skill file is in a directory"))?;
            Ok(std::fs::write(&file, &page)?)
        };
        let message = format!("{verb} the skill {name}");
        let files = format!("skills/{name}");
        let commit = self.commit(
            app,
            by,
            &mut read_at,
            &Edit {
                kind: "edit_skill",
                files: &files,
                message: &message,
            },
            context,
            write,
        )?;
        Ok(self.skill_view(name, &page, commit))
    }

    /// A person deletes the skill `name` and its directory, the same way. A skill an agent names is not deleted.
    pub fn delete_skill(&self, app: &App, by: &Actor, name: &str) -> Result<()> {
        let mut read_at = self.lock();
        if skill::built_in(name).is_some() {
            return Err(built_in(name));
        }
        let dir = self.path.join("skills").join(name);
        if !valid_agent_name(name) || !dir.join("SKILL.md").is_file() {
            return Err(AppError::NotFound(format!("skill {name}")));
        }
        self.refuse_if_named("skill", name, |settings| &settings.skills)?;
        let message = format!("Delete the skill {name}");
        let files = format!("skills/{name}");
        let edit = Edit {
            kind: "edit_skill",
            files: &files,
            message: &message,
        };
        let context = BTreeMap::from([("skill".to_string(), name.to_string())]);
        let write = || Ok(std::fs::remove_dir_all(&dir)?);
        self.commit(app, by, &mut read_at, &edit, context, write)
            .map(drop)
    }

    /// A person adds or changes the MCP server `name`: its table in `mcp.toml` is rewritten, checked as reading checks
    /// it, committed on the default branch and pushed to `origin`, the same way. The file's other tables and comments
    /// are kept.
    pub fn save_mcp_server(
        &self,
        app: &App,
        by: &Actor,
        name: &str,
        server: &McpServer,
    ) -> Result<McpServerView> {
        let mut read_at = self.lock();
        let server = mcp::check(name, server.clone(), &self.mcp_env).map_err(AppError::Invalid)?;
        let file = self.path.join("mcp.toml");
        let old = read_mcp_toml(&file)?;
        let exists =
            toml::from_str::<toml::Table>(&old).is_ok_and(|table| table.contains_key(name));
        let new = mcp::set_server(&old, name, &server).map_err(AppError::Invalid)?;
        let verb = if exists { "Update" } else { "Add" };
        let message = format!("{verb} the MCP server {name}");
        let definition = serde_json::to_string(&server)
            .map_err(|e| AppError::Invalid(format!("cannot write the server: {e}")))?;
        let context = BTreeMap::from([
            ("server".to_string(), name.to_string()),
            ("definition".to_string(), definition),
        ]);
        let commit = self.commit(
            app,
            by,
            &mut read_at,
            &Edit {
                kind: "edit_mcp_server",
                files: "mcp.toml",
                message: &message,
            },
            context,
            || Ok(std::fs::write(&file, new)?),
        )?;
        Ok(McpServerView {
            name: name.into(),
            server,
            commit,
        })
    }

    /// A person deletes the MCP server `name` from `mcp.toml`, the same way. A server an agent names is not deleted.
    pub fn delete_mcp_server(&self, app: &App, by: &Actor, name: &str) -> Result<()> {
        let mut read_at = self.lock();
        let file = self.path.join("mcp.toml");
        let old = read_mcp_toml(&file)?;
        let table: toml::Table = toml::from_str(&old)
            .map_err(|e| AppError::Invalid(format!("mcp.toml is not valid: {e}")))?;
        if !table.contains_key(name) {
            return Err(AppError::NotFound(format!("MCP server {name}")));
        }
        self.refuse_if_named("MCP server", name, |settings| &settings.mcp)?;
        let new = mcp::remove_server(&old, name).map_err(AppError::Invalid)?;
        let message = format!("Delete the MCP server {name}");
        let edit = Edit {
            kind: "edit_mcp_server",
            files: "mcp.toml",
            message: &message,
        };
        let context = BTreeMap::from([("server".to_string(), name.to_string())]);
        self.commit(app, by, &mut read_at, &edit, context, || {
            Ok(std::fs::write(&file, new)?)
        })
        .map(drop)
    }

    /// A conflict naming the agents whose settings list `name` in the list `pick` takes, if any do.
    fn refuse_if_named(
        &self,
        what: &str,
        name: &str,
        pick: fn(&AgentSettings) -> &Vec<String>,
    ) -> Result<()> {
        let named: Vec<String> = agent_names(&self.path)
            .into_iter()
            .filter(|agent| {
                std::fs::read_to_string(self.agent_file(agent, "toml"))
                    .ok()
                    .and_then(|text| toml::from_str::<AgentSettings>(&text).ok())
                    .is_some_and(|settings| pick(&settings).iter().any(|n| n == name))
            })
            .collect();
        if named.is_empty() {
            return Ok(());
        }
        Err(AppError::Conflict(format!(
            "the {what} {name} is named by the agents {}; remove it from them first",
            named.join(", ")
        )))
    }
}

fn built_in(name: &str) -> AppError {
    AppError::Conflict(format!("the skill {name} is built in and read only"))
}

/// The text of `mcp.toml`, empty when there is none.
fn read_mcp_toml(file: &Path) -> Result<String> {
    match std::fs::read_to_string(file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        other => Ok(other?),
    }
}

fn remove_if_there(file: &Path) -> Result<()> {
    match std::fs::remove_file(file) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}
