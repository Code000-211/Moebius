//! The workflow file kept in the harness configuration repository. It names the workflow, says which one trigger starts
//! it, lists the nodes that do the work and the edges a value follows between them. Reading a file parses it whole and
//! validates it whole, so a bad file never leaves a partly usable graph behind.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use super::nodes::{Node, node};

/// The id an edge uses to start at the workflow's trigger.
pub const TRIGGER_ID: &str = "trigger";

/// The folder, under the harness configuration repository's root, that holds workflow files.
pub const DIRECTORY: &str = "workflows";

/// A workflow as its file writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowDefinition {
    /// The workflow's name for people.
    pub name: String,
    /// One sentence on what the workflow does.
    pub description: String,
    /// Exactly one trigger, written as `[[trigger]]`.
    #[serde(default)]
    pub trigger: Vec<Trigger>,
    /// The nodes that do the work, in the order the file lists them.
    #[serde(default)]
    pub nodes: Vec<NodeDefinition>,
    /// The values that flow from one node's port to another node.
    #[serde(default)]
    pub edges: Vec<Edge>,
}

/// What starts a workflow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    /// The trigger kind, resolved through the same registry as a node's kind.
    pub kind: String,
    /// The trigger kind's own settings.
    #[serde(default = "empty_params", skip_serializing_if = "is_empty_params")]
    pub params: Value,
}

/// One node in a workflow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeDefinition {
    /// The id edges use to name this node; unique in the workflow.
    pub id: String,
    /// The node kind, which the registry turns into a [`Node`].
    pub kind: String,
    /// The node kind's own settings.
    #[serde(default = "empty_params", skip_serializing_if = "is_empty_params")]
    pub params: Value,
    /// Where the node sits on the graph's canvas.
    #[serde(default)]
    pub position: Position,
}

/// A node's place on the graph's canvas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// A value leaving one node by one of its output ports and arriving at another node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    /// The source node's id, or [`TRIGGER_ID`].
    pub from: String,
    /// The target node's id.
    pub to: String,
    /// One of the source node's output ports.
    pub port: String,
}

/// Why a workflow file could not be read, parsed or used.
#[derive(Debug, Error)]
#[error("workflow {file}: {message}")]
pub struct Error {
    file: String,
    message: String,
}

impl Error {
    fn new(file: &str, message: impl Into<String>) -> Self {
        Self {
            file: file.to_string(),
            message: message.into(),
        }
    }

    /// The file the error is about.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// What is wrong with it, without the file name.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl WorkflowDefinition {
    /// Parse a workflow file's text. This checks its shape but not its graph; [`validate`](Self::validate) does that.
    pub fn parse(text: &str, file: &str) -> Result<Self, Error> {
        toml::from_str(text)
            .map_err(|error| Error::new(file, format!("is not a valid workflow file: {error}")))
    }

    /// Refuse a definition whose graph cannot be run whole.
    ///
    /// The checks run from the file's shape to its graph: exactly one trigger, unique node ids, edges that name nodes
    /// that exist, kinds the registry knows, ports those kinds have, no cycle, and every node reachable from the
    /// trigger.
    pub fn validate(&self, file: &str) -> Result<(), Error> {
        self.check_triggers(file)?;
        self.check_ids(file)?;
        self.check_edge_nodes(file)?;
        self.check_kinds_and_ports(file)?;
        self.check_no_cycle(file)?;
        self.check_reachable(file)
    }

    fn check_triggers(&self, file: &str) -> Result<(), Error> {
        match self.trigger.len() {
            1 => Ok(()),
            0 => Err(Error::new(
                file,
                "exactly one trigger is required, found none",
            )),
            count => Err(Error::new(
                file,
                format!("exactly one trigger is required, found {count}"),
            )),
        }
    }

    fn check_ids(&self, file: &str) -> Result<(), Error> {
        let mut seen = HashSet::new();
        for node in &self.nodes {
            if node.id == TRIGGER_ID {
                return Err(Error::new(
                    file,
                    format!(
                        "node {:?}: the id is reserved for the workflow's trigger",
                        node.id
                    ),
                ));
            }
            if !seen.insert(node.id.as_str()) {
                return Err(Error::new(
                    file,
                    format!("node {:?}: duplicate id", node.id),
                ));
            }
        }
        Ok(())
    }

    fn check_edge_nodes(&self, file: &str) -> Result<(), Error> {
        let ids: HashSet<&str> = self.nodes.iter().map(|node| node.id.as_str()).collect();
        for edge in &self.edges {
            if edge.from != TRIGGER_ID && !ids.contains(edge.from.as_str()) {
                return Err(Error::new(
                    file,
                    format!(
                        "edge {:?} -> {:?}: no node {:?}",
                        edge.from, edge.to, edge.from
                    ),
                ));
            }
            if !ids.contains(edge.to.as_str()) {
                return Err(Error::new(
                    file,
                    format!(
                        "edge {:?} -> {:?}: no node {:?}",
                        edge.from, edge.to, edge.to
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check_kinds_and_ports(&self, file: &str) -> Result<(), Error> {
        let trigger = &self.trigger[0];
        let trigger_node = node(&trigger.kind, &trigger.params)
            .map_err(|error| Error::new(file, format!("trigger: {error}")))?;

        let mut nodes: HashMap<&str, Box<dyn Node>> = HashMap::new();
        for definition in &self.nodes {
            let node = node(&definition.kind, &definition.params)
                .map_err(|error| Error::new(file, format!("node {:?}: {error}", definition.id)))?;
            nodes.insert(definition.id.as_str(), node);
        }

        for edge in &self.edges {
            let source = if edge.from == TRIGGER_ID {
                trigger_node.as_ref()
            } else {
                nodes
                    .get(edge.from.as_str())
                    .map(Box::as_ref)
                    .expect("edge sources were checked")
            };
            let ports = source.ports();
            if !ports.outputs.iter().any(|port| port == &edge.port) {
                return Err(Error::new(
                    file,
                    format!(
                        "node {:?}: edge to {:?} leaves by port {:?}, which is not one of its outputs",
                        edge.from, edge.to, edge.port
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check_no_cycle(&self, file: &str) -> Result<(), Error> {
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in &self.edges {
            if edge.from != TRIGGER_ID {
                adjacency
                    .entry(edge.from.as_str())
                    .or_default()
                    .push(edge.to.as_str());
            }
        }

        let mut state: HashMap<&str, Visit> = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), Visit::New))
            .collect();
        let mut stack = Vec::new();
        for node in &self.nodes {
            if let Some(cycle) = visit(node.id.as_str(), &adjacency, &mut state, &mut stack) {
                let through = cycle
                    .iter()
                    .map(|id| format!("{id:?}"))
                    .collect::<Vec<_>>()
                    .join(" -> ");
                return Err(Error::new(
                    file,
                    format!("node {:?}: is part of a cycle through {through}", cycle[0]),
                ));
            }
        }
        Ok(())
    }

    fn check_reachable(&self, file: &str) -> Result<(), Error> {
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in &self.edges {
            adjacency
                .entry(edge.from.as_str())
                .or_default()
                .push(edge.to.as_str());
        }

        let mut reachable = HashSet::new();
        let mut queue = VecDeque::new();
        if let Some(targets) = adjacency.get(TRIGGER_ID) {
            queue.extend(targets.iter().copied());
        }
        while let Some(id) = queue.pop_front() {
            if !reachable.insert(id) {
                continue;
            }
            if let Some(targets) = adjacency.get(id) {
                queue.extend(targets.iter().copied());
            }
        }

        for node in &self.nodes {
            if !reachable.contains(node.id.as_str()) {
                return Err(Error::new(
                    file,
                    format!("node {:?}: is unreachable from the trigger", node.id),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Visit {
    New,
    OnStack,
    Done,
}

fn visit<'a>(
    id: &'a str,
    adjacency: &HashMap<&'a str, Vec<&'a str>>,
    state: &mut HashMap<&'a str, Visit>,
    stack: &mut Vec<&'a str>,
) -> Option<Vec<String>> {
    match state.get(id).copied().unwrap_or(Visit::New) {
        Visit::Done => return None,
        Visit::OnStack => return Some(vec![id.to_string()]),
        Visit::New => {}
    }

    state.insert(id, Visit::OnStack);
    stack.push(id);
    if let Some(targets) = adjacency.get(id) {
        for target in targets {
            match state.get(target).copied().unwrap_or(Visit::New) {
                Visit::OnStack => {
                    let start = stack.iter().position(|seen| seen == target).unwrap_or(0);
                    return Some(stack[start..].iter().map(|seen| seen.to_string()).collect());
                }
                Visit::New => {
                    if let Some(cycle) = visit(target, adjacency, state, stack) {
                        return Some(cycle);
                    }
                }
                Visit::Done => {}
            }
        }
    }
    stack.pop();
    state.insert(id, Visit::Done);
    None
}

/// Read one workflow from the harness configuration repository, whose files are `workflows/<name>.toml` under `root`.
///
/// A name that cannot be a file name is refused before the path is built, so a workflow name never reaches outside
/// its folder.
pub fn read(root: &Path, name: &str) -> Result<WorkflowDefinition, Error> {
    let file = format!("{DIRECTORY}/{name}.toml");
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::new(
            &file,
            "workflow names may contain only letters, digits, '-' and '_'",
        ));
    }

    let path = root.join(DIRECTORY).join(format!("{name}.toml"));
    let text = std::fs::read_to_string(&path)
        .map_err(|error| Error::new(&file, format!("cannot be read: {error}")))?;
    let definition = WorkflowDefinition::parse(&text, &file)?;
    definition.validate(&file)?;
    Ok(definition)
}

impl crate::harness::repo::HarnessRepo {
    /// Read and validate one workflow from this harness configuration repository.
    pub fn workflow(&self, name: &str) -> crate::app::Result<WorkflowDefinition> {
        read(self.path(), name).map_err(|error| crate::app::AppError::Invalid(error.to_string()))
    }
}

fn empty_params() -> Value {
    Value::Object(Map::new())
}

fn is_empty_params(params: &Value) -> bool {
    params.as_object().is_some_and(Map::is_empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::nodes::TEST_KIND;

    const FILE: &str = "workflows/release.toml";

    const GOOD: &str = r#"
name = "release"
description = "Build the change and run its tests."

[[trigger]]
kind = "test"
params = { repository = "moebius" }

[[nodes]]
id = "build"
kind = "test"
params = { command = "cargo build" }
position = { x = 0.0, y = 1.0 }

[[nodes]]
id = "test"
kind = "test"
position = { x = 1.0, y = 2.0 }

[[edges]]
from = "trigger"
to = "build"
port = "out"

[[edges]]
from = "build"
to = "test"
port = "out"
"#;

    fn parse(text: &str) -> WorkflowDefinition {
        WorkflowDefinition::parse(text, FILE).unwrap()
    }

    fn refusal(text: &str) -> String {
        parse(text).validate(FILE).unwrap_err().to_string()
    }

    #[test]
    fn a_good_file_parses_and_round_trips() {
        let definition = parse(GOOD);
        assert_eq!(definition.name, "release");
        assert_eq!(definition.trigger.len(), 1);
        assert_eq!(definition.nodes[0].id, "build");
        assert_eq!(definition.nodes[1].position, Position { x: 1.0, y: 2.0 });
        definition.validate(FILE).unwrap();

        let written = toml::to_string(&definition).unwrap();
        assert_eq!(parse(&written), definition);
    }

    #[test]
    fn missing_parameters_and_position_take_their_defaults() {
        let definition = parse(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "test"
"#,
        );
        assert_eq!(definition.trigger[0].params, empty_params());
        assert!(definition.nodes.is_empty());
        definition.validate(FILE).unwrap();
    }

    #[test]
    fn a_file_that_is_not_toml_is_refused_with_its_name() {
        let error = WorkflowDefinition::parse("this is not toml =", FILE).unwrap_err();
        assert!(error.to_string().contains(FILE), "{error}");
        assert!(
            error.to_string().contains("is not a valid workflow file"),
            "{error}"
        );
    }

    #[test]
    fn no_trigger_is_refused() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(
            message.contains("exactly one trigger is required, found none"),
            "{message}"
        );
    }

    #[test]
    fn more_than_one_trigger_is_refused() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "test"
[[trigger]]
kind = "test"
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(
            message.contains("exactly one trigger is required, found 2"),
            "{message}"
        );
    }

    #[test]
    fn duplicate_node_ids_are_refused() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "test"
[[nodes]]
id = "build"
kind = "test"
[[nodes]]
id = "build"
kind = "test"
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(
            message.contains("node \"build\": duplicate id"),
            "{message}"
        );
    }

    #[test]
    fn the_trigger_id_cannot_be_a_node_id() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "test"
[[nodes]]
id = "trigger"
kind = "test"
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(
            message.contains("node \"trigger\": the id is reserved"),
            "{message}"
        );
    }

    #[test]
    fn an_unknown_kind_is_refused() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "test"
[[nodes]]
id = "build"
kind = "missing"
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(
            message.contains("node \"build\": unknown node kind \"missing\""),
            "{message}"
        );
    }

    #[test]
    fn an_unknown_trigger_kind_is_refused() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "missing"
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(
            message.contains("trigger: unknown node kind \"missing\""),
            "{message}"
        );
    }

    #[test]
    fn an_edge_to_a_missing_node_is_refused() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "test"
[[edges]]
from = "trigger"
to = "missing"
port = "out"
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(
            message.contains("edge \"trigger\" -> \"missing\": no node \"missing\""),
            "{message}"
        );
    }

    #[test]
    fn an_edge_from_a_missing_node_is_refused() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "test"
[[nodes]]
id = "build"
kind = "test"
[[edges]]
from = "missing"
to = "build"
port = "out"
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(
            message.contains("edge \"missing\" -> \"build\": no node \"missing\""),
            "{message}"
        );
    }

    #[test]
    fn an_edge_by_a_missing_port_is_refused() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "test"
[[nodes]]
id = "build"
kind = "test"
[[edges]]
from = "trigger"
to = "build"
port = "missing"
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(
            message.contains("node \"trigger\": edge to \"build\" leaves by port \"missing\""),
            "{message}"
        );
    }

    #[test]
    fn a_cycle_is_refused() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "test"
[[nodes]]
id = "a"
kind = "test"
[[nodes]]
id = "b"
kind = "test"
[[edges]]
from = "trigger"
to = "a"
port = "out"
[[edges]]
from = "a"
to = "b"
port = "out"
[[edges]]
from = "b"
to = "a"
port = "out"
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(message.contains("node \"a\""), "{message}");
        assert!(message.contains("cycle"), "{message}");
    }

    #[test]
    fn a_node_unreachable_from_the_trigger_is_refused() {
        let message = refusal(
            r#"
name = "release"
description = "Nothing to do yet."
[[trigger]]
kind = "test"
[[nodes]]
id = "build"
kind = "test"
[[nodes]]
id = "test"
kind = "test"
[[edges]]
from = "trigger"
to = "build"
port = "out"
"#,
        );
        assert!(message.contains(FILE), "{message}");
        assert!(
            message.contains("node \"test\": is unreachable from the trigger"),
            "{message}"
        );
    }

    #[test]
    fn a_workflow_is_read_from_the_harness_configuration_repository() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(DIRECTORY)).unwrap();
        std::fs::write(root.path().join(DIRECTORY).join("release.toml"), GOOD).unwrap();

        let definition = read(root.path(), "release").unwrap();
        assert_eq!(definition.name, "release");
        assert_eq!(definition.nodes.len(), 2);
    }

    #[test]
    fn a_bad_file_in_the_repository_is_refused_whole() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(DIRECTORY)).unwrap();
        std::fs::write(
            root.path().join(DIRECTORY).join("release.toml"),
            "not toml =",
        )
        .unwrap();

        let error = read(root.path(), "release").unwrap_err();
        assert_eq!(error.file(), "workflows/release.toml");
        assert!(
            error.to_string().contains("is not a valid workflow file"),
            "{error}"
        );
    }

    #[test]
    fn a_name_that_is_not_a_file_name_is_refused() {
        let error = read(Path::new("."), "../release").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("workflow names may contain only"),
            "{error}"
        );
    }

    #[test]
    fn the_test_kind_is_the_only_one_the_empty_registry_builds() {
        assert!(node(TEST_KIND, &empty_params()).is_ok());
    }
}
