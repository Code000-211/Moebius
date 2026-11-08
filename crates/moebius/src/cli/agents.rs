//! The agents: `agents` lists those the server can run, `agent` shows, saves and deletes one of the harness
//! configuration repository, with its settings from flags and its instructions from a file.

use clap::{Args, Subcommand};
use serde_json::{Value, json};

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
    /// The model; required unless `--template` names an existing agent, whose model is copied.
    #[arg(long, required_unless_present = "template")]
    model: Option<String>,
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
    /// A runner of moebius.toml (`runners`) the agent's runs go to; without one they run on the server.
    #[arg(long)]
    runner: Option<String>,
    /// A skill of the repository; give it again for more.
    #[arg(long = "skill")]
    skills: Vec<String>,
    /// An extension file of the repository; give it again for more.
    #[arg(long = "extension")]
    extensions: Vec<String>,
    /// A server of mcp.toml; give it again for more.
    #[arg(long = "mcp")]
    mcp: Vec<String>,
    /// Do not give the agent the engineering skills Moebius brings for it (only the Coder, the Reviewer and the
    /// Grilling partner have any).
    #[arg(long)]
    no_engineering_skills: bool,
    /// An engineering skill the agent is given (tdd, diagnosing-bugs, code-review, grilling, to-tickets, wayfinder,
    /// domain-modeling, research, prototype or moebius-autonomous); give it again for more. The agent has exactly these, not the defaults of its name.
    #[arg(long = "engineering-skill", conflicts_with = "no_engineering_skills")]
    engineering_skills: Vec<String>,
    /// A file with the agent's instructions, or `-` for standard input.
    #[arg(long)]
    instructions: Option<String>,
    /// Start from an existing agent (the Coder and the Reviewer included): copy its settings and instructions; the
    /// flags given change them.
    #[arg(long)]
    template: Option<String>,
}

/// `agents`.
pub fn agents() -> Plan {
    Ok((
        Request::get("/v1/agents"),
        Show::Table(&["name", "model", "protocol", "harness", "built_in", "error"]),
    ))
}

/// `runners`.
pub fn runners() -> Plan {
    Ok((
        Request::get("/v1/runners"),
        Show::Table(&["name", "host", "user", "max_runs", "busy"]),
    ))
}

/// `agent show`, `save` and `delete`.
pub fn agent(command: &AgentCommand, get: &mut dyn FnMut(&str) -> Result<Value, String>) -> Plan {
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
                runner,
                skills,
                extensions,
                mcp,
                no_engineering_skills,
                engineering_skills,
                instructions,
                template,
            } = save.as_ref();
            let (mut settings, mut copied) = match template {
                Some(template) => start_from(template, get)?,
                None => (json!({}), String::new()),
            };
            if let Some(model) = model {
                settings["model"] = json!(model);
            }
            if settings.get("model").is_none() {
                return Err(format!(
                    "{name} has no model to copy: give one with --model"
                ));
            }
            for (key, values) in [("skills", skills), ("extensions", extensions), ("mcp", mcp)] {
                if !values.is_empty() || settings.get(key).is_none() {
                    settings[key] = json!(values);
                }
            }
            for (key, value) in [
                ("protocol", json!(protocol)),
                ("thinking", json!(thinking)),
                ("token_budget", json!(token_budget)),
                ("harness", json!(harness)),
                ("runner", json!(runner)),
            ] {
                if !value.is_null() {
                    settings[key] = value;
                }
            }
            if *no_engineering_skills {
                settings["engineering_skills"] = json!(false);
            } else if !engineering_skills.is_empty() {
                settings["engineering_skills"] = json!(engineering_skills);
            }
            let instructions = match instructions {
                Some(source) => read_input(source)?,
                None => std::mem::take(&mut copied),
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

/// The agents Moebius runs for itself, which are no template: only the Coder and the Reviewer are copied.
const NOT_TEMPLATES: [&str; 4] = [
    "Moebius",
    "Grilling-partner",
    "Retro-analyst",
    "Autoresearcher",
];

/// The settings and instructions a new agent copies from the existing agent `name`. The Coder and the Reviewer are
/// built in: their settings are those they run with, and their instructions are the built-in ones followed by the
/// agent's own, which the copy keeps as its own. The engineering skills the agent has, by default for its name or by
/// its setting, are written as an explicit list, since the copy has another name.
fn start_from(
    name: &str,
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
) -> Result<(Value, String), String> {
    if NOT_TEMPLATES.contains(&name) {
        return Err(format!("{name} is not an agent to start from"));
    }
    let shown = get(&format!("/v1/agents/{name}")).map_err(|e| {
        if e.contains("404") {
            format!("no agent named {name} to start from")
        } else {
            e
        }
    })?;
    let mut instructions = shown["instructions"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if let Some(built_in) = shown["built_in"]["instructions"].as_str() {
        instructions = if instructions.is_empty() {
            built_in.into()
        } else {
            format!("{built_in}\n\n{instructions}")
        };
    }
    let defaults: Vec<Value> = shown["built_in"]["engineering_skills"]
        .as_array()
        .map(|skills| skills.iter().map(|skill| skill["name"].clone()).collect())
        .unwrap_or_default();
    let with_skills = |mut settings: Value| {
        let chosen = &settings["engineering_skills"];
        if !chosen.is_array() && *chosen != json!(false) && !defaults.is_empty() {
            settings["engineering_skills"] = json!(defaults);
        }
        settings
    };
    if shown.get("settings").is_some_and(Value::is_object) {
        return Ok((with_skills(shown["settings"].clone()), instructions));
    }
    // A built-in agent without files: the settings it runs with are in the list.
    let listed = get("/v1/agents")?;
    let entry = listed
        .as_array()
        .and_then(|agents| agents.iter().find(|a| a["name"] == name))
        .ok_or_else(|| format!("no agent named {name} to start from"))?;
    let mut settings = json!({});
    for key in ["model", "protocol", "thinking", "token_budget", "harness"] {
        let value = &entry[key];
        if !value.is_null() && value != "" {
            settings[key] = value.clone();
        }
    }
    Ok((with_skills(settings), instructions))
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

    /// What the server answers to the lookups of a copy.
    fn answers(path: &str) -> Result<serde_json::Value, String> {
        Ok(match path {
            "/v1/agents" => json!([
                {"name": "Coder", "model": "big", "protocol": "anthropic", "thinking": "high", "harness": "pi-bolt", "built_in": true},
                {"name": "Reviewer", "model": "", "protocol": "openai", "harness": "pi-bolt", "built_in": true},
                {"name": "Fast", "model": "small", "protocol": "openai", "harness": "pi-bolt", "built_in": false},
            ]),
            "/v1/agents/Fast" => json!({
                "name": "Fast",
                "settings": {"model": "small", "protocol": "anthropic", "skills": ["a"], "extensions": [], "mcp": [],
                             "engineering_skills": false},
                "instructions": "Be brief.\n",
            }),
            "/v1/agents/Coder" => {
                json!({"name": "Coder", "built_in": {"instructions": "You are the Coder.", "engineering_skills": [
                    {"name": "tdd"}, {"name": "diagnosing-bugs"}, {"name": "moebius-autonomous"}]}})
            }
            "/v1/agents/Reviewer" => {
                json!({"name": "Reviewer", "built_in": {"instructions": "You are the Reviewer.", "engineering_skills": [
                    {"name": "code-review"}, {"name": "moebius-autonomous"}]}})
            }
            other => Err(format!("unexpected {other}"))?,
        })
    }

    fn copy(args: &[&str]) -> Result<serde_json::Value, String> {
        let mut words = vec!["agent", "save"];
        words.extend(args);
        crate::cli::tests::plan_with(&words, &mut |path| answers(path))
            .map(|(request, _)| json!({"path": request.path, "body": request.body}))
    }

    #[test]
    fn an_agent_is_saved_as_a_copy_of_an_existing_one() {
        let saved = copy(&["Faster", "--template", "Fast", "--thinking", "low"]).unwrap();
        assert_eq!(saved["path"], "/v1/agents/Faster");
        assert_eq!(
            saved["body"],
            json!({
                "settings": {"model": "small", "protocol": "anthropic", "skills": ["a"], "extensions": [], "mcp": [],
                             "engineering_skills": false, "thinking": "low"},
                "instructions": "Be brief.\n",
            })
        );
        // The flags change what is copied, instructions included.
        let saved = copy(&["Faster", "--template", "Fast", "--model", "other"]).unwrap();
        assert_eq!(saved["body"]["settings"]["model"], "other");
    }

    /// The Coder is built in and has no files: its copy has the settings it runs with and its built-in instructions.
    #[test]
    fn a_built_in_agent_is_copied_with_the_settings_it_runs_with_and_its_instructions() {
        let saved = copy(&["Quick", "--template", "Coder"]).unwrap();
        assert_eq!(
            saved["body"],
            json!({
                "settings": {"model": "big", "protocol": "anthropic", "thinking": "high", "harness": "pi-bolt",
                             "skills": [], "extensions": [], "mcp": [],
                             "engineering_skills": ["tdd", "diagnosing-bugs", "moebius-autonomous"]},
                "instructions": "You are the Coder.",
            })
        );
        // The Reviewer has no model yet, so the copy needs one.
        let error = copy(&["Second", "--template", "Reviewer"]).unwrap_err();
        assert!(error.contains("--model"), "{error}");
        let saved = copy(&["Second", "--template", "Reviewer", "--model", "m"]).unwrap();
        assert_eq!(saved["body"]["settings"]["model"], "m");
        assert_eq!(
            saved["body"]["settings"]["engineering_skills"],
            json!(["code-review", "moebius-autonomous"])
        );
    }

    #[test]
    fn the_engineering_skills_of_a_copy_can_be_chosen() {
        let saved = copy(&["Quick", "--template", "Coder", "--engineering-skill", "tdd"]).unwrap();
        assert_eq!(
            saved["body"]["settings"]["engineering_skills"],
            json!(["tdd"])
        );
        let saved = copy(&["Quick", "--template", "Coder", "--no-engineering-skills"]).unwrap();
        assert_eq!(
            saved["body"]["settings"]["engineering_skills"],
            json!(false)
        );
    }

    #[test]
    fn the_agents_moebius_runs_for_itself_are_no_template() {
        for name in [
            "Moebius",
            "Grilling-partner",
            "Retro-analyst",
            "Autoresearcher",
        ] {
            let error = copy(&["X", "--template", name, "--model", "m"]).unwrap_err();
            assert!(error.contains("is not an agent to start from"), "{error}");
        }
    }

    #[test]
    fn the_runners_are_listed_and_an_agent_can_name_one() {
        let (request, _) = plan(&["runners"]).unwrap();
        assert_eq!(request.path, "/v1/runners");
        let (request, _) = plan(&[
            "agent", "save", "Far", "--model", "m", "--runner", "gpu-box",
        ])
        .unwrap();
        assert_eq!(request.body.unwrap()["settings"]["runner"], "gpu-box");
    }

    #[test]
    fn the_engineering_skills_are_turned_off_only_by_the_flag() {
        let save = |extra: &[&str]| {
            let mut words = vec!["agent", "save", "Coder", "--model", "m"];
            words.extend(extra);
            plan(&words).unwrap().0.body.unwrap()["settings"].clone()
        };
        assert!(save(&[]).get("engineering_skills").is_none());
        assert_eq!(
            save(&["--no-engineering-skills"])["engineering_skills"],
            json!(false)
        );
        assert_eq!(
            save(&[
                "--engineering-skill",
                "tdd",
                "--engineering-skill",
                "moebius-autonomous"
            ])["engineering_skills"],
            json!(["tdd", "moebius-autonomous"])
        );
    }
}
