//! The automations: `automation list` lists them, `save` adds or replaces one, `run` fires one now and `delete`
//! deletes one.

use clap::Subcommand;
use serde_json::{Map, Value, json};

use super::{Plan, Request, Show, read_input};

#[derive(Debug, Subcommand)]
pub enum AutomationCommand {
    /// The automations, with when each fires next (Unix seconds).
    List,
    /// Add an automation, or replace the one `--id` names.
    Save {
        /// The automation to replace; a new one without it.
        #[arg(long)]
        id: Option<i64>,
        #[arg(long)]
        name: String,
        /// A cron expression of five fields in the server's local time, such as "0 8 * * 1-5".
        #[arg(long)]
        schedule: String,
        /// The agent its chats are with; the Moebius agent without it.
        #[arg(long)]
        agent: Option<String>,
        /// The first message of each chat, or `-` for standard input.
        #[arg(long)]
        prompt: String,
        /// The id of the repository its chats work in.
        #[arg(long, conflicts_with = "harness")]
        repository: Option<i64>,
        /// Its chats work in the harness configuration repository.
        #[arg(long)]
        harness: bool,
        /// Save it switched off, so that only `automation run` fires it.
        #[arg(long)]
        off: bool,
    },
    /// Fire an automation now; skipped while the chat of its firing before still runs.
    Run { id: i64 },
    /// Delete an automation; the chats it started stay.
    Delete { id: i64 },
}

/// `automation list`, `save`, `run` and `delete`.
pub fn automation(command: &AutomationCommand) -> Plan {
    Ok(match command {
        AutomationCommand::List => (
            Request::get("/v1/automations"),
            Show::Table(&["id", "name", "schedule", "agent", "enabled", "next_firing"]),
        ),
        AutomationCommand::Save {
            id,
            name,
            schedule,
            agent,
            prompt,
            repository,
            harness,
            off,
        } => {
            let prompt = if prompt == "-" {
                read_input("-")?
            } else {
                prompt.clone()
            };
            let mut body = Map::new();
            body.insert("name".into(), json!(name));
            body.insert("schedule".into(), json!(schedule));
            body.insert("agent".into(), json!(agent.as_deref().unwrap_or("Moebius")));
            body.insert("prompt".into(), json!(prompt));
            body.insert("harness".into(), json!(harness));
            body.insert("enabled".into(), json!(!off));
            if let Some(repository) = repository {
                body.insert("repository".into(), json!(repository));
            }
            let body = Value::Object(body);
            let request = match id {
                Some(id) => Request::put(format!("/v1/automations/{id}"), body),
                None => Request::post("/v1/automations", body),
            };
            (request, Show::Object)
        }
        AutomationCommand::Run { id } => (
            Request::post(format!("/v1/automations/{id}/run"), json!({})),
            Show::Object,
        ),
        AutomationCommand::Delete { id } => (
            Request::delete(format!("/v1/automations/{id}")),
            Show::Done(format!("deleted the automation {id}")),
        ),
    })
}

#[cfg(test)]
mod tests {
    use crate::cli::tests::plan;
    use reqwest::Method;
    use serde_json::json;

    #[test]
    fn an_automation_is_saved_from_flags_and_run_by_its_id() {
        let (request, _) = plan(&[
            "automation",
            "save",
            "--name",
            "Triage",
            "--schedule",
            "0 8 * * 1-5",
            "--prompt",
            "Triage the new issues.",
            "--repository",
            "2",
        ])
        .unwrap();
        assert_eq!(
            (request.method, request.path.as_str()),
            (Method::POST, "/v1/automations")
        );
        assert_eq!(
            request.body,
            Some(json!({
                "name": "Triage", "schedule": "0 8 * * 1-5", "agent": "Moebius",
                "prompt": "Triage the new issues.", "harness": false, "enabled": true, "repository": 2,
            }))
        );
        let (request, _) = plan(&[
            "automation",
            "save",
            "--id",
            "4",
            "--name",
            "Deps",
            "--schedule",
            "0 9 * * fri",
            "--agent",
            "Coder",
            "--prompt",
            "Look for outdated dependencies.",
            "--off",
        ])
        .unwrap();
        assert_eq!(
            (request.method, request.path.as_str()),
            (Method::PUT, "/v1/automations/4")
        );
        assert_eq!(request.body.unwrap()["enabled"], false);
        let (request, _) = plan(&["automation", "run", "4"]).unwrap();
        assert_eq!(
            (request.method, request.path.as_str()),
            (Method::POST, "/v1/automations/4/run")
        );
        let (request, _) = plan(&["automation", "delete", "4"]).unwrap();
        assert_eq!(request.method, Method::DELETE);
        assert_eq!(
            plan(&["automation", "list"]).unwrap().0.path,
            "/v1/automations"
        );
    }
}
