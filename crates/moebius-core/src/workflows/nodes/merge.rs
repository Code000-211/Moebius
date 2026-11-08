//! The `merge` node: joins the two branches that reach it and emits what arrived, naming the side each value
//! arrived by.
//!
//! A workflow's edges carry the port their *source* leaves by, so the graph alone does not tell a merge which of its
//! two sides a value came from; its parameters name the node whose value arrives by each side. A side has arrived by
//! the time the merge runs when that node ran earlier in the run. The merge runs once, when a branch reaches it, and
//! a side whose branch never ran is simply not an arrival, so a branch that can never arrive does not hold the run up.

use serde_json::{Map, Value};

use super::super::decorate::Scope;
use super::super::definition::TRIGGER_ID;
use super::{Metadata, Node, NodeError, Ports, Routed};

/// The kind name a workflow file writes.
pub const KIND: &str = "merge";

/// The incoming port the first side arrives by.
pub const A: &str = "a";

/// The incoming port the second side arrives by.
pub const B: &str = "b";

/// The port the merged value leaves by.
pub const OUT: &str = "out";

/// A node that joins two branches and emits what arrived, by the name of the side it arrived by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merge {
    /// The node whose value arrives by port `a`.
    a: String,
    /// The node whose value arrives by port `b`.
    b: String,
    /// Whether both sides may arrive, in which case the node emits the list of arrivals.
    both: bool,
}

impl Merge {
    /// Read the node's parameters: `a` and `b`, the ids of the two nodes whose values arrive by the ports of those
    /// names, and `both`, whether both sides may arrive (optional, default `false`).
    pub fn from_params(params: &Value) -> Result<Self, NodeError> {
        let invalid = |message: &str| NodeError::Parameters {
            kind: KIND.to_string(),
            message: message.to_string(),
        };
        let Value::Object(params) = params else {
            return Err(invalid("parameters must be a table"));
        };
        for key in params.keys() {
            if !(A == key.as_str() || B == key.as_str() || key == "both") {
                return Err(invalid(&format!(
                    "unknown parameter {key:?}; a merge takes \"{A}\", \"{B}\" and \"both\""
                )));
            }
        }
        let side = |port: &str| match params.get(port) {
            Some(Value::String(id)) if !id.trim().is_empty() => Ok(id.clone()),
            _ => Err(invalid(&format!(
                "\"{port}\" must name the node whose value arrives by port \"{port}\""
            ))),
        };
        let a = side(A)?;
        let b = side(B)?;
        if a == TRIGGER_ID || b == TRIGGER_ID {
            return Err(invalid(&format!(
                "a side cannot name {TRIGGER_ID:?}, which is not a branch the merge joins"
            )));
        }
        if a == b {
            return Err(invalid("\"a\" and \"b\" must name different nodes"));
        }
        let both = match params.get("both") {
            None => false,
            Some(Value::Bool(both)) => *both,
            Some(_) => return Err(invalid("\"both\" must be true or false")),
        };
        Ok(Self { a, b, both })
    }

    /// The value the merge emits for `arrivals`, the sides that reached it as `(port, value)` pairs in the order they
    /// arrived. One arrival is the object naming its side, `{ "a": value }` or `{ "b": value }`; several are the
    /// ordered list of those objects. Nothing arriving, or two arrivals at a merge whose parameters wait for one,
    /// fails the node.
    ///
    /// An executor that runs several branches at once gathers the arrivals and calls this. The executor that follows
    /// one edge at a time gives the node to [`Node::run_routed`], which calls it with the sides that ran.
    pub fn join(&self, arrivals: &[(&str, Value)]) -> Result<Value, NodeError> {
        let arrival = |side: &str, value: Value| {
            let mut arrived = Map::new();
            arrived.insert(side.to_string(), value);
            Value::Object(arrived)
        };
        if arrivals.is_empty() {
            return Err(NodeError::Failed(format!(
                "no branch reached the merge: neither {:?} nor {:?} ran",
                self.a, self.b
            )));
        }
        if arrivals.len() > 1 && !self.both {
            return Err(NodeError::Failed(format!(
                "both sides reached the merge, whose parameters wait for one: {:?} and {:?}",
                self.a, self.b
            )));
        }
        if self.both {
            return Ok(Value::Array(
                arrivals
                    .iter()
                    .map(|(side, value)| arrival(side, value.clone()))
                    .collect(),
            ));
        }
        let (side, value) = arrivals[0].clone();
        Ok(arrival(side, value))
    }

    /// The sides that had arrived when the node runs: of the two nodes the definition names, those that ran earlier
    /// in the run, with what each produced, in the order the definition names the sides.
    fn arrived(&self, scope: &Scope) -> Vec<(&str, Value)> {
        [(A, &self.a), (B, &self.b)]
            .into_iter()
            .filter_map(|(side, id)| {
                scope
                    .nodes
                    .get(id.as_str())
                    .map(|value| (side, value.clone()))
            })
            .collect()
    }
}

impl Node for Merge {
    /// Fails closed: which side a value arrived by belongs to a run, and the executor calls [`Node::run_routed`]
    /// with the nodes that ran before.
    fn run(&self, _input: Value) -> Result<Value, NodeError> {
        Err(NodeError::Failed(
            "merge needs an executor that gives it the nodes that ran before".to_string(),
        ))
    }

    /// Emits what arrived: the sides whose nodes ran, in the order the definition names them. A side whose branch
    /// never ran is left out, so a branch that can never arrive does not hold the run up.
    fn run_routed(&self, scope: &Scope) -> Result<Routed, NodeError> {
        let arrivals = self.arrived(scope);
        Ok(Routed {
            value: self.join(&arrivals)?,
            port: None,
        })
    }

    fn ports(&self) -> Ports {
        Ports::new([A, B], [OUT])
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            kind: KIND,
            title: "Merge",
            description: "Joins two branches and emits what arrived by each side.",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::workflows::definition::{NodeDefinition, WorkflowDefinition};
    use crate::workflows::run::{self, Observer, Runner, StepResult};

    /// A run's scope: the nodes that ran before with what they produced, and the value the last of them left for
    /// the node about to run.
    fn scope(ran: &[(&str, Value)]) -> Scope {
        Scope {
            trigger: Value::Null,
            input: ran
                .last()
                .map(|(_, value)| value.clone())
                .unwrap_or(Value::Null),
            nodes: ran
                .iter()
                .map(|(id, value)| (id.to_string(), value.clone()))
                .collect::<BTreeMap<String, Value>>(),
        }
    }

    fn merge(params: Value) -> Merge {
        Merge::from_params(&params).unwrap_or_else(|error| panic!("{params} refused: {error}"))
    }

    /// Records which nodes ran and what each of them produced.
    #[derive(Default)]
    struct Steps {
        started: Vec<String>,
        outputs: Vec<(String, Value)>,
    }

    impl Steps {
        /// What the node called `id` produced, if it ran and succeeded.
        fn output(&self, id: &str) -> Option<Value> {
            self.outputs
                .iter()
                .find(|(ran, _)| ran == id)
                .map(|(_, value)| value.clone())
        }
    }

    impl Observer for Steps {
        fn started(&mut self, node: &NodeDefinition, _input: &Value) -> Result<(), run::Error> {
            self.started.push(node.id.clone());
            Ok(())
        }

        fn finished(
            &mut self,
            node: &NodeDefinition,
            result: &StepResult,
        ) -> Result<(), run::Error> {
            if let StepResult::Succeeded(value) = result {
                self.outputs.push((node.id.clone(), value.clone()));
            }
            Ok(())
        }
    }

    /// The diamond a `condition` makes: `check` sends the value to `left` or to `right`, and an edge from each of
    /// them reaches the `join` node, which joins those two sides. The line `# other side` is where a test writes
    /// what the `b` side runs with.
    const DIAMOND: &str = r#"
name = "join"
description = "Join the branch a condition took."
[[trigger]]
kind = "test"
[[nodes]]
id = "check"
kind = "condition"
params = { ports = [{ name = "pass", when = "input.ok" }], fallback = "fail" }
[[nodes]]
id = "left"
kind = "test"
[[nodes]]
id = "right"
kind = "test"
# other side
[[nodes]]
id = "join"
kind = "merge"
params = { a = "left", b = "right", both = true }
[[nodes]]
id = "after"
kind = "test"
[[edges]]
from = "trigger"
to = "check"
port = "out"
[[edges]]
from = "check"
to = "left"
port = "pass"
[[edges]]
from = "check"
to = "right"
port = "fail"
[[edges]]
from = "left"
to = "join"
port = "out"
[[edges]]
from = "right"
to = "join"
port = "out"
[[edges]]
from = "join"
to = "after"
port = "out"
"#;

    /// The diamond with `other_side` as the parameters of `right`, which a test makes a node that never finishes.
    fn diamond(other_side: &str) -> String {
        DIAMOND.replace("# other side", other_side)
    }

    /// Runs a workflow and hands back the run's value and what it recorded.
    fn run_workflow(text: &str, start: Value) -> (Result<Value, run::Error>, Steps) {
        let definition = WorkflowDefinition::parse(text, "test").unwrap();
        definition.validate("test").unwrap();
        let runner: Arc<dyn Runner> = Arc::new(Registry);
        let mut steps = Steps::default();
        let result = run::execute(
            &definition,
            start,
            run::Limits::from_definition(&definition),
            &runner,
            &mut steps,
        );
        (result, steps)
    }

    /// Runs nodes as the server's runner does: from the registry, through the node's own `run_routed`.
    struct Registry;

    impl Runner for Registry {
        fn run(&self, node: &NodeDefinition, input: &Value) -> Result<Value, NodeError> {
            super::super::node(&node.kind, &node.params)?.run(input.clone())
        }

        fn run_routed(&self, node: &NodeDefinition, scope: &Scope) -> Result<Routed, NodeError> {
            super::super::node(&node.kind, &node.params)?.run_routed(scope)
        }
    }

    #[test]
    fn one_side_arriving_names_the_side_it_arrived_by() {
        let merge = merge(json!({ "a": "left", "b": "right" }));
        let arrived = merge
            .run_routed(&scope(&[
                ("check", json!(true)),
                ("left", json!({ "note": "passed" })),
            ]))
            .unwrap();
        assert_eq!(arrived.port, None);
        assert_eq!(arrived.value, json!({ "a": { "note": "passed" } }));

        let arrived = merge
            .run_routed(&scope(&[("right", json!("late"))]))
            .unwrap();
        assert_eq!(arrived.value, json!({ "b": "late" }));
    }

    #[test]
    fn both_sides_arriving_are_the_ordered_list_of_arrivals() {
        // `zeta` is the `a` side although the earlier nodes are kept by id, where `alpha` sorts first.
        let merge = merge(json!({ "a": "zeta", "b": "alpha", "both": true }));
        let arrived = merge
            .run_routed(&scope(&[("alpha", json!(2)), ("zeta", json!(1))]))
            .unwrap();
        assert_eq!(arrived.value, json!([{ "a": 1 }, { "b": 2 }]));
    }

    #[test]
    fn a_side_that_never_arrived_is_left_out_of_the_list() {
        let merge = merge(json!({ "a": "left", "b": "right", "both": true }));
        let arrived = merge.run_routed(&scope(&[("left", json!(1))])).unwrap();
        assert_eq!(arrived.value, json!([{ "a": 1 }]));
    }

    #[test]
    fn a_merge_with_no_arrival_fails_closed() {
        let merge = merge(json!({ "a": "left", "b": "right", "both": true }));
        let error = merge
            .run_routed(&scope(&[("check", json!(true))]))
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "no branch reached the merge: neither \"left\" nor \"right\" ran"
        );
    }

    #[test]
    fn two_arrivals_at_a_merge_that_waits_for_one_fail_closed() {
        let merge = merge(json!({ "a": "left", "b": "right" }));
        let error = match merge.run_routed(&scope(&[("left", json!(1)), ("right", json!(2))])) {
            Ok(routed) => panic!("two arrivals at a merge waiting for one gave {routed:?}"),
            Err(error) => error,
        };
        assert_eq!(
            error.to_string(),
            "both sides reached the merge, whose parameters wait for one: \"left\" and \"right\""
        );
    }

    #[test]
    fn a_merge_without_a_run_fails_closed() {
        let merge = merge(json!({ "a": "left", "b": "right" }));
        assert_eq!(
            merge.run(json!({ "ok": true })).unwrap_err().to_string(),
            "merge needs an executor that gives it the nodes that ran before"
        );
    }

    #[test]
    fn the_two_sides_are_the_only_parameters() {
        for params in [
            json!({}),
            json!({ "a": "left" }),
            json!({ "a": "left", "b": "" }),
            json!({ "a": "left", "b": "left" }),
            json!({ "a": "trigger", "b": "right" }),
            json!({ "a": 1, "b": "right" }),
            json!({ "a": "left", "b": "right", "both": "yes" }),
            json!({ "a": "left", "b": "right", "wait_for": "both" }),
        ] {
            let error = match Merge::from_params(&params) {
                Ok(node) => panic!("{params} built a merge: {node:?}"),
                Err(error) => error,
            };
            assert!(
                matches!(error, NodeError::Parameters { ref kind, .. } if kind == KIND),
                "{params} gave {error}",
            );
        }
        assert_eq!(
            Merge::from_params(&json!({ "a": "left", "b": 2 }))
                .unwrap_err()
                .to_string(),
            "invalid parameters for node kind \"merge\": \"b\" must name the node whose value arrives by port \"b\""
        );
    }

    #[test]
    fn the_registry_builds_a_merge_with_two_incoming_ports() {
        let node = super::super::node(KIND, &json!({ "a": "left", "b": "right" })).unwrap();
        assert_eq!(node.ports().inputs, [A, B]);
        assert_eq!(node.ports().outputs, [OUT]);
        assert_eq!(node.metadata().kind, KIND);
        assert!(!node.routes(), "a merge leaves by one port");
    }

    #[test]
    fn a_merge_after_a_condition_runs_once_and_names_the_branch_that_ran() {
        let graph = diamond("");
        let (value, steps) = run_workflow(&graph, json!({ "ok": true }));
        assert_eq!(value.unwrap(), json!([{ "a": { "ok": true } }]));
        assert_eq!(steps.started, ["check", "left", "join", "after"]);
        assert_eq!(
            steps.output("join"),
            Some(json!([{ "a": { "ok": true } }])),
            "the join did not emit the value the run carries on",
        );

        let (value, steps) = run_workflow(&graph, json!({ "ok": false }));
        assert_eq!(value.unwrap(), json!([{ "b": { "ok": false } }]));
        assert_eq!(steps.started, ["check", "right", "join", "after"]);
    }

    #[test]
    fn a_side_that_can_never_arrive_does_not_hold_the_run_up() {
        // The `b` side waits on a gate nobody opens, and its branch is not the one `check` takes: nothing waits for
        // it, so the run ends with the one arrival it did get.
        let graph = diamond(r#"params = { wait_for_gate = "never opened" }"#);
        let (value, steps) = run_workflow(&graph, json!({ "ok": true }));
        assert_eq!(value.unwrap(), json!([{ "a": { "ok": true } }]));
        assert!(
            !steps.started.iter().any(|id| id == "right"),
            "the branch that cannot arrive ran: {:?}",
            steps.started
        );
    }
}
