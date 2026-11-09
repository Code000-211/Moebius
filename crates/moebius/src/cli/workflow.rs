//! The workflow commands: list the workflows of the harness configuration repository, show one, save a file as one,
//! run one by hand with the inputs its `manual` trigger names, and delete one. Saving and deleting are commits on
//! that repository. A run of a workflow is cancelled by its id.

use clap::Subcommand;
use serde_json::{Map, Value, json};

use super::{Plan, Request, Show, json_input, read_input};

/// What `moebius workflow` does.
#[derive(Debug, Subcommand)]
pub enum WorkflowCommand {
    /// The workflows, with their trigger kind and how many nodes they have.
    List,
    /// One workflow: its file and its parsed form.
    Show {
        /// The workflow's name.
        name: String,
    },
    /// Add a workflow or replace it, committed to the harness configuration repository.
    Save {
        /// The workflow's name.
        name: String,
        /// The TOML file to save, or `-` for standard input.
        file: String,
    },
    /// Start a run by hand, giving the inputs the workflow's `manual` trigger names.
    Run {
        /// The workflow's name.
        name: String,
        /// The repository the run belongs to, by name; not needed when the trigger names an input of type
        /// `repository`.
        #[arg(long)]
        repository: Option<String>,
        /// One input the trigger names, as `name=value`; give the option again for each input. The text becomes the
        /// type the trigger declares the input to be, so `count=3` is the number 3 for a `number` and the text `3`
        /// for a `text`.
        #[arg(long = "input")]
        input: Vec<String>,
        /// Every input as a JSON object: the JSON itself, a file that holds it, or `-` for standard input. An
        /// alternative to `--input`.
        #[arg(long, conflicts_with = "input")]
        inputs: Option<String>,
    },
    /// Delete a workflow; a run of it queued or running refuses the deletion.
    Delete {
        /// The workflow's name.
        name: String,
    },
    /// Cancel a run and stop its nodes; a run that has ended refuses the cancel.
    Cancel {
        /// The run's id.
        id: i64,
    },
}

/// The request a workflow command stands for and how to show its answer. `get` reads a route, for the command that
/// types the inputs it is given.
pub fn workflow(
    command: &WorkflowCommand,
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
) -> Plan {
    Ok(match command {
        WorkflowCommand::List => (
            Request::get("/v1/workflows"),
            Show::Table(&["name", "description", "trigger_kind", "node_count"]),
        ),
        WorkflowCommand::Show { name } => {
            (Request::get(format!("/v1/workflows/{name}")), Show::Raw)
        }
        WorkflowCommand::Save { name, file } => {
            let text = read_input(file)?;
            (
                Request::put(format!("/v1/workflows/{name}"), json!({ "file": text })),
                Show::Object,
            )
        }
        WorkflowCommand::Run {
            name,
            repository,
            input,
            inputs,
        } => {
            let mut given = match inputs {
                Some(source) => match json_input(source)? {
                    Value::Object(fields) => fields,
                    _ => return Err(format!("{source} is not a JSON object of inputs")),
                },
                None => Map::new(),
            };
            if !input.is_empty() {
                let declared = get(&format!("/v1/workflows/{name}/run-inputs"))?;
                for flag in input {
                    let (input_name, text) = flag
                        .split_once('=')
                        .ok_or(format!("--input {flag:?} is not name=value"))?;
                    given.insert(input_name.to_string(), typed(&declared, input_name, text)?);
                }
            }
            let mut body = json!({ "inputs": given });
            if let Some(repository) = repository {
                body["repository"] = json!(repository);
            }
            (
                Request::post(format!("/v1/workflows/{name}/runs"), body),
                Show::Object,
            )
        }
        WorkflowCommand::Delete { name } => (
            Request::delete(format!("/v1/workflows/{name}")),
            Show::Object,
        ),
        WorkflowCommand::Cancel { id } => (
            Request::post(format!("/v1/workflows/runs/{id}/cancel"), json!({})),
            Show::Object,
        ),
    })
}

/// The value `--input name=value` stands for, taken as the type the workflow's `manual` trigger declares for that
/// input. A name the trigger does not give is sent as text, so the server refuses it and names it.
fn typed(declared: &Value, name: &str, text: &str) -> Result<Value, String> {
    let kind = declared["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|input| input["name"].as_str() == Some(name))
        .and_then(|input| input["type"].as_str());
    match kind {
        Some("number") => text
            .parse::<i64>()
            .map(|number| json!(number))
            .or_else(|_| {
                text.parse::<f64>()
                    .map(|number| json!(number))
                    .map_err(|_| ())
            })
            .map_err(|_| format!("input {name:?} is a number, and {text:?} is not one")),
        Some("boolean") => match text.to_ascii_lowercase().as_str() {
            "true" => Ok(json!(true)),
            "false" => Ok(json!(false)),
            _ => Err(format!(
                "input {name:?} is a boolean, and {text:?} is not true or false"
            )),
        },
        _ => Ok(json!(text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The inputs `workflow run` types its `--input` flags against.
    const DECLARED: &str = r#"{"trigger_kind":"manual","inputs":[
        {"name":"version","type":"text","required":true,"default":null,"choices":[]},
        {"name":"count","type":"number","required":false,"default":3,"choices":[]},
        {"name":"dry_run","type":"boolean","required":false,"default":true,"choices":[]},
        {"name":"environment","type":"choice","required":false,"default":"prod","choices":["staging","prod"]},
        {"name":"checkout","type":"repository","required":false,"default":null,"choices":[]}
    ]}"#;

    fn types(flags: &[&str]) -> Result<Value, String> {
        let declared: Value = serde_json::from_str(DECLARED).unwrap();
        let mut inputs = Map::new();
        for flag in flags {
            let (name, text) = flag.split_once('=').unwrap();
            inputs.insert(name.to_string(), typed(&declared, name, text)?);
        }
        Ok(Value::Object(inputs))
    }

    #[test]
    fn each_input_flag_becomes_the_type_the_trigger_declares() {
        assert_eq!(
            types(&[
                "version=1.2.3",
                "count=12",
                "dry_run=FALSE",
                "environment=staging",
                "checkout=radiance",
            ]),
            Ok(json!({
                "version": "1.2.3",
                "count": 12,
                "dry_run": false,
                "environment": "staging",
                "checkout": "radiance",
            }))
        );
        // A text that reads as a number stays text when the trigger says it is text.
        assert_eq!(types(&["version=3"]), Ok(json!({ "version": "3" })));
    }

    #[test]
    fn a_number_or_boolean_flag_that_is_not_one_is_refused_naming_the_input() {
        for (flag, name, words) in [
            ("count=three", "count", "number"),
            ("dry_run=yes", "dry_run", "boolean"),
        ] {
            let error = types(&[flag]).unwrap_err();
            assert!(error.contains(name), "{flag}: {error}");
            assert!(error.contains(words), "{flag}: {error}");
        }
    }

    #[test]
    fn a_number_flag_that_is_a_decimal_number_is_a_number() {
        assert_eq!(types(&["count=1.5"]), Ok(json!({ "count": 1.5 })));
    }

    #[test]
    fn a_flag_for_an_input_the_trigger_does_not_name_is_sent_as_text() {
        // The server refuses it and names it; the command line does not guess a type for it.
        assert_eq!(types(&["nope=7"]), Ok(json!({ "nope": "7" })));
    }
}
