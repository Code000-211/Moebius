//! The workflow commands: list the workflows of the harness configuration repository, show one, save a file as one,
//! run one by hand with the inputs its `manual` trigger names, and delete one. Saving and deleting are commits on
//! that repository. A workflow belongs to a project: `--project` (a name or an id) addresses it through the
//! project's routes, and without it the commands use the aggregate routes, which resolve a name that only one
//! project has. A run of a workflow is cancelled by its id.

use clap::Subcommand;
use serde_json::{Map, Value, json};

use super::{Plan, Request, Show, json_input, read_input};

/// What `moebius workflow` does.
#[derive(Debug, Subcommand)]
pub enum WorkflowCommand {
    /// The workflows, with their trigger kind and how many nodes they have.
    List {
        /// Only the workflows of this project, by name or id.
        #[arg(long)]
        project: Option<String>,
    },
    /// One workflow: its file and its parsed form.
    Show {
        /// The workflow's name.
        name: String,
        /// The project it belongs to, by name or id; needed when more than one project has a workflow of the name.
        #[arg(long)]
        project: Option<String>,
    },
    /// Add a workflow or replace it, committed to the harness configuration repository.
    Save {
        /// The workflow's name.
        name: String,
        /// The TOML file to save, or `-` for standard input.
        file: String,
        /// The project it belongs to, by name or id; a new workflow goes to the oldest project without it.
        #[arg(long)]
        project: Option<String>,
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
        /// The project the workflow belongs to, by name or id; needed when more than one project has a workflow of
        /// the name.
        #[arg(long)]
        project: Option<String>,
    },
    /// Delete a workflow; a run of it queued or running refuses the deletion.
    Delete {
        /// The workflow's name.
        name: String,
        /// The project it belongs to, by name or id; needed when more than one project has a workflow of the name.
        #[arg(long)]
        project: Option<String>,
    },
    /// Cancel a run and stop its nodes; a run that has ended refuses the cancel.
    Cancel {
        /// The run's id.
        id: i64,
    },
}

/// The id of the project `given` names, a name (in any case) or an id, looked up with `get`.
pub fn project_id(
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
    given: &str,
) -> Result<i64, String> {
    let projects = get("/v1/projects")?;
    let projects: &[Value] = projects.as_array().map_or(&[], Vec::as_slice);
    let found = projects
        .iter()
        .find(|p| {
            p["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(given))
        })
        .or_else(|| {
            let id = given.parse::<i64>().ok()?;
            projects.iter().find(|p| p["id"].as_i64() == Some(id))
        });
    match found.and_then(|p| p["id"].as_i64()) {
        Some(id) => Ok(id),
        None => {
            let names: Vec<&str> = projects.iter().filter_map(|p| p["name"].as_str()).collect();
            Err(format!(
                "there is no project {given}; the projects are {}",
                names.join(", ")
            ))
        }
    }
}

/// The route of the workflows (`name` none) or of one workflow: the project's when `project` is given, the
/// aggregate's otherwise.
fn route(
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
    project: &Option<String>,
    name: Option<&str>,
) -> Result<String, String> {
    let base = match project {
        Some(project) => format!("/v1/projects/{}/workflows", project_id(get, project)?),
        None => "/v1/workflows".to_string(),
    };
    Ok(match name {
        Some(name) => format!("{base}/{name}"),
        None => base,
    })
}

/// The request a workflow command stands for and how to show its answer. `get` reads a route, for the command that
/// types the inputs it is given and for the project a name stands for.
pub fn workflow(
    command: &WorkflowCommand,
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
) -> Plan {
    Ok(match command {
        WorkflowCommand::List { project } => (
            Request::get(route(get, project, None)?),
            Show::Table(&[
                "project_name",
                "name",
                "description",
                "trigger_kind",
                "node_count",
            ]),
        ),
        WorkflowCommand::Show { name, project } => {
            (Request::get(route(get, project, Some(name))?), Show::Raw)
        }
        WorkflowCommand::Save {
            name,
            file,
            project,
        } => {
            let text = read_input(file)?;
            (
                Request::put(route(get, project, Some(name))?, json!({ "file": text })),
                Show::Object,
            )
        }
        WorkflowCommand::Run {
            name,
            repository,
            input,
            inputs,
            project,
        } => {
            let mut given = match inputs {
                Some(source) => match json_input(source)? {
                    Value::Object(fields) => fields,
                    _ => return Err(format!("{source} is not a JSON object of inputs")),
                },
                None => Map::new(),
            };
            let base = route(get, project, Some(name))?;
            if !input.is_empty() {
                let declared = get(&format!("{base}/run-inputs"))?;
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
            (Request::post(format!("{base}/runs"), body), Show::Object)
        }
        WorkflowCommand::Delete { name, project } => (
            Request::delete(route(get, project, Some(name))?),
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
    fn a_project_addresses_the_workflow_through_the_project_routes() {
        use crate::cli::tests::plan_with;
        let mut get = |route: &str| -> Result<Value, String> {
            match route {
                "/v1/projects" => Ok(json!([{"id": 3, "name": "Web"}, {"id": 5, "name": "API"}])),
                other => Err(format!("unexpected {other}")),
            }
        };
        let path = |args: &[&str], get: &mut dyn FnMut(&str) -> Result<Value, String>| {
            plan_with(args, get).map(|(request, _)| (request.method.to_string(), request.path))
        };
        assert_eq!(
            path(&["workflow", "list", "--project", "web"], &mut get),
            Ok(("GET".into(), "/v1/projects/3/workflows".into()))
        );
        assert_eq!(
            path(&["workflow", "show", "ship", "--project", "5"], &mut get),
            Ok(("GET".into(), "/v1/projects/5/workflows/ship".into()))
        );
        assert_eq!(
            path(
                &["workflow", "delete", "ship", "--project", "API"],
                &mut get
            ),
            Ok(("DELETE".into(), "/v1/projects/5/workflows/ship".into()))
        );
        assert_eq!(
            path(&["workflow", "run", "ship", "--project", "Web"], &mut get),
            Ok(("POST".into(), "/v1/projects/3/workflows/ship/runs".into()))
        );
        // Without a project the aggregate routes stay.
        assert_eq!(
            path(&["workflow", "show", "ship"], &mut get),
            Ok(("GET".into(), "/v1/workflows/ship".into()))
        );
        // A project that is not there is refused, naming the projects.
        let error = path(&["workflow", "list", "--project", "nope"], &mut get).unwrap_err();
        assert!(error.contains("Web") && error.contains("API"), "{error}");
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
