//! The agents: `agents` lists those the server can run, `agent` shows, saves and deletes one of the harness
//! configuration repository, with its settings from flags and its instructions from a file.

use clap::{Args, Subcommand};
use serde_json::json;

use super::{Plan, Request, Show, read_input};

// Parsed once per command line, so the size of `Save` does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// An agent as its files say.
    Show { name: String },
    /// Add an agent or replace it, committed to the harness configuration repository.
    Save(Box<SaveAgent>),
    /// Delete an agent.
    Delete { name: String },
}

/// The settings `agent save` sends.
#[derive(Debug, Args)]
pub struct SaveAgent {
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
    /// `pi-bolt` or `claude-code`.
    #[arg(long)]
    harness: Option<String>,
    /// A skill of the repository; give it again for more.
    #[arg(long = "skill")]
    skills: Vec<String>,
    /// An extension file of the repository; give it again for more.
    #[arg(long = "extension")]
    extensions: Vec<String>,
    /// A server of mcp.toml; give it again for more.
    #[arg(long = "mcp")]
    mcp: Vec<String>,
    /// Hide the outputs of tool calls older than this many turns; 0 keeps them all (default 10).
    #[arg(long)]
    hide_outputs_after: Option<u32>,
    /// The most lines of one tool result; 0 is no limit (default 200).
    #[arg(long)]
    output_lines: Option<u32>,
    /// The most bytes of one tool result; 0 is no limit (default 16384).
    #[arg(long)]
    output_bytes: Option<u32>,
    /// Give tool outputs as they are: no compression of test runs and build logs.
    #[arg(long)]
    no_compress: bool,
    /// A file with the agent's instructions, or `-` for standard input.
    #[arg(long)]
    instructions: Option<String>,
}

/// `agents`.
pub fn agents() -> Plan {
    Ok((
        Request::get("/v1/agents"),
        Show::Table(&["name", "model", "protocol", "harness", "built_in", "error"]),
    ))
}

/// `agent show`, `save` and `delete`.
pub fn agent(command: &AgentCommand) -> Plan {
    Ok(match command {
        AgentCommand::Show { name } => (Request::get(format!("/v1/agents/{name}")), Show::Object),
        AgentCommand::Save(save) => {
            let SaveAgent {
                name,
                model,
                protocol,
                thinking,
                token_budget,
                harness,
                skills,
                extensions,
                mcp,
                hide_outputs_after,
                output_lines,
                output_bytes,
                no_compress,
                instructions,
            } = save.as_ref();
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
                ("harness", json!(harness)),
            ] {
                if !value.is_null() {
                    settings[key] = value;
                }
            }
            // What is not given of the tool outputs keeps its default.
            let mut tool_output = serde_json::Map::new();
            for (key, value) in [
                ("hide_after_turns", hide_outputs_after),
                ("max_lines", output_lines),
                ("max_bytes", output_bytes),
            ] {
                if let Some(value) = value {
                    tool_output.insert(key.into(), json!(value));
                }
            }
            if *no_compress {
                tool_output.insert("compress".into(), json!(false));
            }
            if !tool_output.is_empty() {
                settings["tool_output"] = tool_output.into();
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
            "--harness",
            "claude-code",
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
                "settings": {"model": "m", "thinking": "low", "harness": "claude-code", "skills": ["a", "b"],
                             "extensions": [], "mcp": []},
                "instructions": "Be brief.\n",
            }))
        );
    }

    #[test]
    fn an_agents_tool_output_is_given_only_as_far_as_flags_say() {
        let (request, _) = plan(&[
            "agent",
            "save",
            "Fast",
            "--model",
            "m",
            "--hide-outputs-after",
            "0",
            "--no-compress",
        ])
        .unwrap();
        assert_eq!(
            request.body.unwrap()["settings"]["tool_output"],
            json!({"hide_after_turns": 0, "compress": false})
        );
    }
}
