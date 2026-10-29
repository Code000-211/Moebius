//! The agents: `agents` lists those the server can run, `agent` shows, saves and deletes one of the harness
//! configuration repository, with its settings from flags and its instructions from a file.

use clap::Subcommand;
use serde_json::json;

use super::{Plan, Request, Show, read_input};

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// An agent as its files say.
    Show { name: String },
    /// Add an agent or replace it, committed to the harness configuration repository.
    Save {
        name: String,
        #[arg(long)]
        model: String,
        /// `openai` or `anthropic`.
        #[arg(long)]
        protocol: Option<String>,
        #[arg(long)]
        thinking: Option<String>,
        #[arg(long)]
        token_budget: Option<i64>,
        /// A skill of the repository; give it again for more.
        #[arg(long = "skill")]
        skills: Vec<String>,
        /// An extension file of the repository; give it again for more.
        #[arg(long = "extension")]
        extensions: Vec<String>,
        /// A server of mcp.toml; give it again for more.
        #[arg(long = "mcp")]
        mcp: Vec<String>,
        /// A file with the agent's instructions, or `-` for standard input.
        #[arg(long)]
        instructions: Option<String>,
    },
    /// Delete an agent.
    Delete { name: String },
}

/// `agents`.
pub fn agents() -> Plan {
    Ok((
        Request::get("/v1/agents"),
        Show::Table(&["name", "model", "protocol", "built_in", "error"]),
    ))
}

/// `agent show`, `save` and `delete`.
pub fn agent(command: &AgentCommand) -> Plan {
    Ok(match command {
        AgentCommand::Show { name } => (Request::get(format!("/v1/agents/{name}")), Show::Object),
        AgentCommand::Save {
            name,
            model,
            protocol,
            thinking,
            token_budget,
            skills,
            extensions,
            mcp,
            instructions,
        } => {
            let mut settings = json!({
                "model": model,
                "skills": skills,
                "extensions": extensions,
                "mcp": mcp,
            });
            for (key, value) in [
                ("protocol", json!(protocol)),
                ("thinking", json!(thinking)),
                ("token_budget", json!(token_budget)),
            ] {
                if !value.is_null() {
                    settings[key] = value;
                }
            }
            let instructions = match instructions {
                Some(source) => read_input(source)?,
                None => String::new(),
            };
            (
                Request::put(
                    format!("/v1/agents/{name}"),
                    json!({ "settings": settings, "instructions": instructions }),
                ),
                Show::Object,
            )
        }
        AgentCommand::Delete { name } => (
            Request::delete(format!("/v1/agents/{name}")),
            Show::Done(format!("deleted the agent {name}")),
        ),
    })
}

#[cfg(test)]
mod tests {
    use crate::cli::tests::plan;
    use reqwest::Method;
    use serde_json::json;

    #[test]
    fn an_agent_is_saved_from_flags() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("be-brief.md");
        std::fs::write(&file, "Be brief.\n").unwrap();
        let (request, _) = plan(&[
            "agent",
            "save",
            "Fast",
            "--model",
            "m",
            "--thinking",
            "low",
            "--skill",
            "a",
            "--skill",
            "b",
            "--instructions",
            file.to_str().unwrap(),
        ])
        .unwrap();
        assert_eq!(
            (request.method, request.path.as_str()),
            (Method::PUT, "/v1/agents/Fast")
        );
        assert_eq!(
            request.body,
            Some(json!({
                "settings": {"model": "m", "thinking": "low", "skills": ["a", "b"], "extensions": [], "mcp": []},
                "instructions": "Be brief.\n",
            }))
        );
    }
}
