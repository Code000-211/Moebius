//! Setting the server up and keeping it running: `doctor` checks the steps to a first card (`GET /v1/start`),
//! `settings` shows and changes `moebius.toml`, `secrets set` stores an API key read from standard input, `usage`
//! writes the tokens and calls of the last days, and `server restart` restarts the server.

use clap::Subcommand;
use serde_json::{Map, Value, json};

use super::{Plan, Request, Show, cell, read_input, table};

#[derive(Debug, Subcommand)]
pub enum SettingsCommand {
    /// Set keys and remove others. A value is JSON when it parses as JSON (`5`, `true`, `["a"]`, `'{"model":"m"}'`)
    /// and text otherwise.
    Set {
        #[arg(value_name = "KEY=VALUE", value_parser = assignment)]
        assignments: Vec<(String, Value)>,
        /// A key or table to remove; give it again for more.
        #[arg(long = "remove")]
        remove: Vec<String>,
    },
}

/// `key=value`: the value is JSON if it parses as JSON, else the text.
fn assignment(text: &str) -> Result<(String, Value), String> {
    let (key, value) = text
        .split_once('=')
        .ok_or("write it as KEY=VALUE".to_string())?;
    if key.is_empty() {
        return Err("a key is needed before the =".into());
    }
    let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
    Ok((key.to_string(), value))
}

#[derive(Debug, Subcommand)]
pub enum SecretsCommand {
    /// Store an API key under a name. The key is read from standard input, never from an argument.
    Set { name: String },
}

#[derive(Debug, Subcommand)]
pub enum ServerCommand {
    /// Restart the server.
    Restart,
}

/// `doctor`.
pub fn doctor() -> Plan {
    Ok((Request::get("/v1/start"), Show::Doctor))
}

/// `settings` and `settings set`.
pub fn settings(command: Option<&SettingsCommand>) -> Plan {
    Ok(match command {
        None => (Request::get("/v1/settings"), Show::Settings),
        Some(SettingsCommand::Set {
            assignments,
            remove,
        }) => {
            let set: Map<String, Value> = assignments.iter().cloned().collect();
            (
                Request::put("/v1/settings", json!({ "set": set, "remove": remove })),
                Show::Object,
            )
        }
    })
}

/// `secrets set`, with the key from standard input.
pub fn secrets(command: &SecretsCommand) -> Plan {
    let SecretsCommand::Set { name } = command;
    let value = read_input("-")?;
    Ok((
        Request::put(
            format!("/v1/secrets/{name}"),
            json!({ "value": value.trim_end_matches(['\n', '\r']) }),
        ),
        Show::Done(format!("stored the key {name}")),
    ))
}

/// `usage [--days <n>]`.
pub fn usage(days: Option<i64>) -> Plan {
    Ok((
        Request::get(match days {
            Some(days) => format!("/v1/usage?days={days}"),
            None => "/v1/usage".into(),
        }),
        Show::Usage,
    ))
}

/// `server restart`.
pub fn server(command: &ServerCommand) -> Plan {
    let ServerCommand::Restart = command;
    Ok((
        Request::post("/v1/server/restart", json!({})),
        Show::Done("the server is restarting".into()),
    ))
}

/// One line for each step of `GET /v1/start`.
pub fn show_doctor(view: &Value) -> String {
    let steps = view["steps"].as_array().into_iter().flatten();
    steps
        .map(|step| {
            let state = if step["done"] == true {
                "ok"
            } else {
                "missing"
            };
            format!(
                "{state:<8} {}: {}",
                cell(&step["key"]),
                cell(&step["detail"])
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every key of the settings with its value, the stored keys' names and what needs a restart.
pub fn show_settings(view: &Value) -> String {
    let mut out = vec![format!("file: {}", cell(&view["file"]))];
    for (key, entry) in view["values"].as_object().into_iter().flatten() {
        let note = if entry["default"] == true {
            "  (default)"
        } else {
            ""
        };
        out.push(format!("{key} = {}{note}", entry["value"]));
    }
    let names: Vec<String> = view["secrets"]
        .as_array()
        .into_iter()
        .flatten()
        .map(cell)
        .collect();
    out.push(format!(
        "stored keys: {}",
        if names.is_empty() {
            "none".into()
        } else {
            names.join(", ")
        }
    ));
    if view["restart_needed"] == true {
        out.push(format!("restart needed for: {}", view["restart_keys"]));
    }
    out.join("\n")
}

/// The usage totals of the window, then by agent and by model.
pub fn show_usage(view: &Value) -> String {
    let totals = &view["totals"];
    [
        format!(
            "{} day(s) up to {}: {} tokens, {} calls, {}% cached, day budget {}",
            cell(&view["days_in_window"]),
            cell(&view["today"]),
            cell(&totals["tokens"]),
            cell(&totals["calls"]),
            (totals["cached_share"].as_f64().unwrap_or(0.0) * 100.0).round(),
            cell(&totals["day_budget"]),
        ),
        "agents:".into(),
        table(&view["agents"], &["agent", "tokens", "calls"]),
        "models:".into(),
        table(&view["models"], &["model", "tokens", "calls"]),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use crate::cli::tests::plan;
    use serde_json::json;

    #[test]
    fn settings_values_are_json_when_they_parse_as_json() {
        let (request, _) = plan(&[
            "settings",
            "set",
            "board.round_limit=7",
            "teacher.model=qwen",
            "budgets.day_tokens=null",
            "harness.mcp_env=[\"A\"]",
            "--remove",
            "agents.Coder",
        ])
        .unwrap();
        assert_eq!(request.path, "/v1/settings");
        assert_eq!(
            request.body,
            Some(json!({
                "set": {
                    "board.round_limit": 7,
                    "teacher.model": "qwen",
                    "budgets.day_tokens": null,
                    "harness.mcp_env": ["A"],
                },
                "remove": ["agents.Coder"],
            }))
        );
        assert!(
            plan(&["settings", "set", "nokey"])
                .unwrap_err()
                .contains("KEY=VALUE")
        );
    }

    #[test]
    fn a_key_is_never_an_argument() {
        // `secrets set` takes the name only; the key is read from standard input.
        assert!(plan(&["secrets", "set", "anthropic", "sk-123"]).is_err());
    }
}
