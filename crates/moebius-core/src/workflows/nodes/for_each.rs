//! The `for_each` node: takes a list from its input, runs the sub-path that leaves its `each` port once per element
//! with that element as the value, and emits the gathered outputs, in order, from its `done` port.
//!
//! The node does not walk the graph itself. The executor gives [`ForEach::gather`] a function that runs the sub-path
//! once, from the nodes the `each` edges lead to up to the node the definition names as `rejoin`, and returns what
//! that node produced. The definition names the rejoin node in the node's parameters, and [`check`] refuses a graph
//! whose sub-path does not reach it.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;

use super::{Metadata, Node, NodeError, Ports};
use crate::workflows::definition::WorkflowDefinition;

/// The kind name a workflow file writes.
pub const KIND: &str = "for_each";

/// The port the sub-path leaves by, once per element.
pub const EACH: &str = "each";

/// The port the gathered outputs leave by.
pub const DONE: &str = "done";

/// How many elements one run of the node may fan out over when its parameters set no ceiling.
pub const DEFAULT_MAX_ELEMENTS: u64 = 100;

/// A node that fans a list out over a sub-path and gathers the results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForEach {
    rejoin: String,
    max_elements: u64,
}

impl ForEach {
    /// Read the node's parameters: `rejoin`, the id of the node where the sub-path rejoins (required), and
    /// `max_elements`, the most elements one run may fan out over (optional).
    pub fn from_params(params: &Value) -> Result<Self, NodeError> {
        let invalid = |message: &str| NodeError::Parameters {
            kind: KIND.to_string(),
            message: message.to_string(),
        };
        let rejoin = match params.get("rejoin") {
            Some(Value::String(id)) if !id.is_empty() => id.clone(),
            _ => {
                return Err(invalid(
                    "\"rejoin\" must name the node where the sub-path rejoins",
                ));
            }
        };
        let max_elements = match params.get("max_elements") {
            None => DEFAULT_MAX_ELEMENTS,
            Some(value) => value
                .as_u64()
                .ok_or_else(|| invalid("\"max_elements\" must be a non-negative integer"))?,
        };
        Ok(Self {
            rejoin,
            max_elements,
        })
    }

    /// The id of the node where the sub-path rejoins.
    pub fn rejoin(&self) -> &str {
        &self.rejoin
    }

    /// Run `sub_path` once per element of `input`, in order, and return the list of what it returned.
    ///
    /// `input` must be a list. A list longer than the ceiling fails before any element runs, and the first element
    /// whose sub-path fails fails the node, so the later elements do not run. An empty list never calls `sub_path`.
    pub fn gather(
        &self,
        input: Value,
        mut sub_path: impl FnMut(Value) -> Result<Value, NodeError>,
    ) -> Result<Value, NodeError> {
        let Value::Array(elements) = input else {
            return Err(NodeError::Failed(
                "for_each needs a list as its input".to_string(),
            ));
        };
        if elements.len() as u64 > self.max_elements {
            return Err(NodeError::Failed(format!(
                "for_each got {} elements, over its ceiling of {}",
                elements.len(),
                self.max_elements
            )));
        }
        let mut gathered = Vec::with_capacity(elements.len());
        for (index, element) in elements.into_iter().enumerate() {
            let output = sub_path(element).map_err(|error| {
                NodeError::Failed(format!("for_each element {index} failed: {error}"))
            })?;
            gathered.push(output);
        }
        Ok(Value::Array(gathered))
    }
}

impl Node for ForEach {
    /// Fails closed: the sub-path belongs to the executor, which calls [`ForEach::gather`] instead.
    fn run(&self, _input: Value) -> Result<Value, NodeError> {
        Err(NodeError::Failed(
            "for_each needs an executor that runs its sub-path".to_string(),
        ))
    }

    fn ports(&self) -> Ports {
        Ports::new(["in"], [EACH, DONE])
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            kind: KIND,
            title: "For each",
            description: "Runs a sub-path once per element of a list and gathers the outputs in order.",
        }
    }
}

/// Refuse a graph in which a `for_each` node's sub-path does not rejoin where the node says.
///
/// For each `for_each` node the `rejoin` node must exist and not be the node itself, at least one edge must leave the
/// node by `each`, and the rejoin node must be reachable from where those edges lead. The error is the text after the
/// file name.
pub(crate) fn check(definition: &WorkflowDefinition) -> Result<(), String> {
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &definition.edges {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    let ids: HashSet<&str> = definition.nodes.iter().map(|n| n.id.as_str()).collect();

    for node in definition.nodes.iter().filter(|node| node.kind == KIND) {
        let for_each = ForEach::from_params(&node.params)
            .map_err(|error| format!("node {:?}: {error}", node.id))?;
        let rejoin = for_each.rejoin();
        if rejoin == node.id || !ids.contains(rejoin) {
            return Err(format!(
                "node {:?}: rejoin names {rejoin:?}, which is not another node of the workflow",
                node.id
            ));
        }
        let starts: Vec<&str> = definition
            .edges
            .iter()
            .filter(|edge| edge.from == node.id && edge.port == EACH)
            .map(|edge| edge.to.as_str())
            .collect();
        if starts.is_empty() {
            return Err(format!(
                "node {:?}: no edge leaves by port \"{EACH}\", so there is no sub-path",
                node.id
            ));
        }
        let mut seen = HashSet::new();
        let mut queue: VecDeque<&str> = starts.into_iter().collect();
        while let Some(id) = queue.pop_front() {
            if seen.insert(id) {
                queue.extend(adjacency.get(id).into_iter().flatten().copied());
            }
        }
        if !seen.contains(rejoin) {
            return Err(format!(
                "node {:?}: the sub-path leaving \"{EACH}\" does not rejoin at {rejoin:?}",
                node.id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn for_each(params: Value) -> ForEach {
        ForEach::from_params(&params).unwrap()
    }

    #[test]
    fn three_elements_give_three_sub_runs_in_order_and_one_gathered_list() {
        let node = for_each(json!({ "rejoin": "join" }));
        let mut seen = Vec::new();
        let gathered = node
            .gather(json!([1, 2, 3]), |element| {
                seen.push(element.clone());
                Ok(json!(element.as_i64().unwrap() * 10))
            })
            .unwrap();
        assert_eq!(seen, [json!(1), json!(2), json!(3)]);
        assert_eq!(gathered, json!([10, 20, 30]));
    }

    #[test]
    fn an_empty_list_skips_the_sub_path_and_emits_an_empty_list() {
        let node = for_each(json!({ "rejoin": "join" }));
        let gathered = node
            .gather(json!([]), |_| panic!("the sub-path ran for an empty list"))
            .unwrap();
        assert_eq!(gathered, json!([]));
    }

    #[test]
    fn a_list_over_the_ceiling_fails_closed_before_any_element_runs() {
        let node = for_each(json!({ "rejoin": "join", "max_elements": 2 }));
        let mut runs = 0;
        let error = node
            .gather(json!([1, 2, 3]), |element| {
                runs += 1;
                Ok(element)
            })
            .unwrap_err();
        assert_eq!(runs, 0);
        assert_eq!(
            error.to_string(),
            "for_each got 3 elements, over its ceiling of 2"
        );
        // At the ceiling is allowed.
        assert!(node.gather(json!([1, 2]), Ok).is_ok());
    }

    #[test]
    fn a_sub_path_error_fails_the_node_and_stops_the_later_elements() {
        let node = for_each(json!({ "rejoin": "join" }));
        let mut runs = 0;
        let error = node
            .gather(json!([1, 2, 3]), |element| {
                runs += 1;
                if element == json!(2) {
                    Err(NodeError::Failed("boom".to_string()))
                } else {
                    Ok(element)
                }
            })
            .unwrap_err();
        assert_eq!(runs, 2);
        assert_eq!(error.to_string(), "for_each element 1 failed: boom");
    }

    #[test]
    fn an_input_that_is_not_a_list_fails() {
        let node = for_each(json!({ "rejoin": "join" }));
        let error = node.gather(json!({ "a": 1 }), Ok).unwrap_err();
        assert_eq!(error.to_string(), "for_each needs a list as its input");
    }

    #[test]
    fn parameters_need_a_rejoin_and_a_whole_ceiling() {
        for params in [
            json!({}),
            json!({ "rejoin": "" }),
            json!({ "rejoin": 3 }),
            json!({ "rejoin": "join", "max_elements": -1 }),
            json!({ "rejoin": "join", "max_elements": "many" }),
        ] {
            assert!(
                matches!(
                    ForEach::from_params(&params),
                    Err(NodeError::Parameters { .. })
                ),
                "{params} was accepted"
            );
        }
    }

    #[test]
    fn it_is_in_the_registry_with_each_and_done_ports() {
        let node = super::super::node(KIND, &json!({ "rejoin": "join" })).unwrap();
        assert_eq!(node.ports().outputs, ["each", "done"]);
        assert_eq!(node.metadata().kind, "for_each");
        assert!(node.run(json!([1])).is_err());
    }

    mod validation {
        use super::*;

        const FILE: &str = "workflows/fan.toml";

        fn workflow(rejoin: &str, edges: &str) -> String {
            format!(
                r#"
name = "fan"
description = "Fan out."

[[trigger]]
kind = "test"

[[nodes]]
id = "loop"
kind = "for_each"
params = {{ rejoin = "{rejoin}" }}

[[nodes]]
id = "step"
kind = "test"

[[nodes]]
id = "join"
kind = "test"

[[nodes]]
id = "after"
kind = "test"

[[edges]]
from = "trigger"
to = "loop"
port = "out"

[[edges]]
from = "loop"
to = "after"
port = "done"
{edges}
"#
            )
        }

        const REJOINING: &str = r#"
[[edges]]
from = "loop"
to = "step"
port = "each"

[[edges]]
from = "step"
to = "join"
port = "out"
"#;

        fn validate(text: &str) -> Result<(), String> {
            let definition = WorkflowDefinition::parse(text, FILE).unwrap();
            definition
                .validate(FILE)
                .map_err(|error| error.message().to_string())
        }

        #[test]
        fn a_sub_path_that_rejoins_is_accepted() {
            // `join` must be reachable from the trigger, which the `each` edges do.
            validate(&workflow("join", REJOINING)).unwrap();
        }

        #[test]
        fn a_sub_path_that_does_not_rejoin_is_refused() {
            let edges = r#"
[[edges]]
from = "loop"
to = "step"
port = "each"

[[edges]]
from = "trigger"
to = "join"
port = "out"
"#;
            let message = validate(&workflow("join", edges)).unwrap_err();
            assert_eq!(
                message,
                "node \"loop\": the sub-path leaving \"each\" does not rejoin at \"join\""
            );
        }

        #[test]
        fn a_missing_each_edge_is_refused() {
            let edges = r#"
[[edges]]
from = "trigger"
to = "join"
port = "out"

[[edges]]
from = "trigger"
to = "step"
port = "out"
"#;
            let message = validate(&workflow("join", edges)).unwrap_err();
            assert!(
                message.contains("no edge leaves by port \"each\""),
                "{message}"
            );
        }

        #[test]
        fn a_rejoin_that_names_no_other_node_is_refused() {
            for rejoin in ["nowhere", "loop"] {
                let message = validate(&workflow(rejoin, REJOINING)).unwrap_err();
                assert!(message.contains("not another node"), "{message}");
            }
        }
    }
}
