//! The `condition` node kind: it sends the value that reached it down exactly one of its named output ports. Each port
//! has one Luau expression, they are evaluated in the order the node lists them, and the first that is true names the
//! port the value leaves by. If none is true the node fails closed, unless a fallback port is declared, which is then
//! taken. The edges of the workflow already carry their port, so the executor follows the edge that leaves by the port
//! [`Node::run_routed`] names; validation needs an edge leaving by every port the node reports, and a run that names a
//! port with no edge fails closed.

use serde::Deserialize;
use serde_json::Value;

use super::{Metadata, Node, NodeError, Ports, Routed};
use crate::workflows::decorate::{self, Scope};

/// The kind name a workflow file writes.
pub const KIND: &str = "condition";

/// The id the expressions' errors name the node by, since the node does not know the id the workflow gave it.
const NODE: &str = "condition";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    ports: Vec<PortParams>,
    #[serde(default)]
    fallback: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortParams {
    name: String,
    when: String,
}

/// A node that routes its input down the first port whose expression is true.
#[derive(Debug)]
pub struct Condition {
    /// The ports and their expressions, in the order they are tried.
    ports: Vec<(String, String)>,
    fallback: Option<String>,
}

impl Condition {
    /// Build the node from its `params`: `ports`, an array of `{ name, when }` tried in order, and an optional
    /// `fallback` port name taken when no `when` is true.
    pub fn new(params: &Value) -> Result<Self, NodeError> {
        let invalid = |message: String| NodeError::Parameters {
            kind: KIND.to_string(),
            message,
        };
        let params: Params =
            serde_json::from_value(params.clone()).map_err(|error| invalid(error.to_string()))?;
        if params.ports.is_empty() {
            return Err(invalid("at least one port is required".to_string()));
        }
        let mut names: Vec<&str> = Vec::new();
        for port in &params.ports {
            if port.name.trim().is_empty() {
                return Err(invalid("a port needs a name".to_string()));
            }
            if port.when.trim().is_empty() {
                return Err(invalid(format!("port {:?} needs an expression", port.name)));
            }
            if port.when.contains("}}") {
                return Err(invalid(format!(
                    "the expression of port {:?} may not contain \"}}}}\"",
                    port.name
                )));
            }
            if names.contains(&port.name.as_str()) {
                return Err(invalid(format!("port {:?} is declared twice", port.name)));
            }
            names.push(&port.name);
        }
        if let Some(fallback) = &params.fallback {
            if fallback.trim().is_empty() {
                return Err(invalid("the fallback needs a name".to_string()));
            }
            if names.contains(&fallback.as_str()) {
                return Err(invalid(format!(
                    "the fallback {fallback:?} is also a port with an expression"
                )));
            }
        }
        Ok(Self {
            ports: params
                .ports
                .into_iter()
                .map(|port| (port.name, port.when))
                .collect(),
            fallback: params.fallback,
        })
    }

    /// The name of the port the value leaves by: the first whose expression is true in `scope`, else the fallback.
    /// An expression that fails, or no true expression and no fallback, is an error: the node fails closed.
    pub fn route(&self, scope: &Scope) -> Result<&str, NodeError> {
        for (name, when) in &self.ports {
            let expression = Value::String(format!("{{{{ {when} }}}}"));
            let truth = decorate::fill(NODE, &expression, scope)
                .map_err(|error| NodeError::Failed(format!("port {name:?}: {}", error.message)))?;
            // Luau's truth: only `nil` and `false` are false.
            if !matches!(truth, Value::Null | Value::Bool(false)) {
                return Ok(name);
            }
        }
        self.fallback.as_deref().ok_or_else(|| {
            NodeError::Failed("no port's expression was true and no fallback is declared".into())
        })
    }
}

impl Node for Condition {
    /// Checks that a port is taken, with no trigger and no earlier nodes to read, and returns the input unchanged.
    /// The executor calls [`Node::run_routed`], which does read them.
    fn run(&self, input: Value) -> Result<Value, NodeError> {
        let scope = Scope {
            trigger: Value::Null,
            input,
            nodes: Default::default(),
        };
        Ok(self.run_routed(&scope)?.value)
    }

    /// Names the port the value leaves by and passes the input on unchanged: the value goes down the port as it came.
    fn run_routed(&self, scope: &Scope) -> Result<Routed, NodeError> {
        Ok(Routed {
            value: scope.input.clone(),
            port: Some(self.route(scope)?.to_string()),
        })
    }

    fn ports(&self) -> Ports {
        let outputs = self
            .ports
            .iter()
            .map(|(name, _)| name.clone())
            .chain(self.fallback.clone());
        Ports::new(["in"], outputs)
    }

    /// The value always leaves by one of the ports this node names, so it routes.
    fn routes(&self) -> bool {
        true
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            kind: KIND,
            title: "Condition",
            description: "Sends the value down the first port whose expression is true, or the fallback port.",
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

    fn scope(input: Value) -> Scope {
        Scope {
            trigger: json!({ "branch": "main" }),
            input,
            nodes: BTreeMap::new(),
        }
    }

    fn condition(params: Value) -> Condition {
        Condition::new(&params).unwrap()
    }

    fn two_ports() -> Condition {
        condition(json!({ "ports": [
            { "name": "pass", "when": "input.ok" },
            { "name": "retry", "when": "input.attempts < 3" },
        ] }))
    }

    #[test]
    fn the_first_true_port_is_taken() {
        let node = two_ports();
        assert_eq!(
            node.route(&scope(json!({ "ok": true, "attempts": 1 })))
                .unwrap(),
            "pass"
        );
        assert_eq!(
            node.route(&scope(json!({ "ok": false, "attempts": 1 })))
                .unwrap(),
            "retry"
        );
    }

    #[test]
    fn expressions_see_the_trigger_and_the_input_and_run_returns_the_input() {
        let node = condition(json!({ "ports": [
            { "name": "main", "when": "trigger.branch == \"main\" and input.n > 1" },
        ] }));
        assert_eq!(node.route(&scope(json!({ "n": 2 }))).unwrap(), "main");
        assert!(node.run(json!({ "n": 1 })).is_err());
    }

    #[test]
    fn run_passes_the_value_on_unchanged() {
        let node = condition(json!({ "ports": [{ "name": "any", "when": "true" }] }));
        assert_eq!(node.run(json!({ "n": 1 })).unwrap(), json!({ "n": 1 }));
    }

    #[test]
    fn the_fallback_is_taken_when_no_port_is_true() {
        let node = condition(json!({
            "ports": [{ "name": "pass", "when": "input.ok" }],
            "fallback": "other",
        }));
        assert_eq!(node.route(&scope(json!({ "ok": false }))).unwrap(), "other");
        assert_eq!(node.route(&scope(json!({ "ok": true }))).unwrap(), "pass");
        assert_eq!(node.ports().outputs, ["pass", "other"]);
        assert!(node.run(json!({ "ok": false })).is_ok());
    }

    #[test]
    fn no_true_port_and_no_fallback_fails_closed() {
        let node = condition(json!({ "ports": [{ "name": "pass", "when": "input.ok" }] }));
        let error = node.run(json!({ "ok": false })).unwrap_err();
        assert!(matches!(error, NodeError::Failed(_)), "{error}");
        assert!(error.to_string().contains("no fallback"), "{error}");
    }

    #[test]
    fn a_failing_expression_fails_closed_even_with_a_fallback() {
        let node = condition(json!({
            "ports": [{ "name": "pass", "when": "input.a.b" }],
            "fallback": "other",
        }));
        let error = node.route(&scope(json!({}))).unwrap_err();
        assert!(error.to_string().contains("port \"pass\""), "{error}");
    }

    #[test]
    fn a_nil_or_false_value_is_false_and_anything_else_is_true() {
        let node = condition(json!({ "ports": [{ "name": "set", "when": "input.x" }] }));
        assert!(node.route(&scope(json!({ "x": 0 }))).is_ok());
        assert!(node.route(&scope(json!({ "x": "" }))).is_ok());
        assert!(node.route(&scope(json!({ "x": null }))).is_err());
        assert!(node.route(&scope(json!({ "x": false }))).is_err());
    }

    #[test]
    fn bad_parameters_are_refused() {
        for params in [
            json!({}),
            json!({ "ports": [] }),
            json!({ "ports": [{ "name": "a" }] }),
            json!({ "ports": [{ "name": "", "when": "true" }] }),
            json!({ "ports": [{ "name": "a", "when": " " }] }),
            json!({ "ports": [{ "name": "a", "when": "{b={c=1}}" }] }),
            json!({ "ports": [{ "name": "a", "when": "true" }, { "name": "a", "when": "true" }] }),
            json!({ "ports": [{ "name": "a", "when": "true" }], "fallback": "a" }),
            json!({ "ports": [{ "name": "a", "when": "true" }], "fallback": "" }),
            json!({ "ports": [{ "name": "a", "when": "true" }], "extra": 1 }),
        ] {
            let error = Condition::new(&params).unwrap_err();
            assert!(matches!(error, NodeError::Parameters { .. }), "{params}");
        }
    }

    #[test]
    fn the_registry_builds_it_and_reports_its_ports() {
        let node = super::super::node(
            KIND,
            &json!({
                "ports": [{ "name": "pass", "when": "true" }],
                "fallback": "other",
            }),
        )
        .unwrap();
        assert_eq!(node.ports().inputs, ["in"]);
        assert_eq!(node.ports().outputs, ["pass", "other"]);
        assert_eq!(node.metadata().kind, "condition");
    }

    const WORKFLOW: &str = r#"
name = "route"
description = "Route by the trigger."
[[trigger]]
kind = "test"
[[nodes]]
id = "check"
kind = "condition"
params = { ports = [{ name = "pass", when = "input.ok" }], fallback = "other" }
[[nodes]]
id = "ship"
kind = "test"
[[nodes]]
id = "fix"
kind = "test"
[[edges]]
from = "trigger"
to = "check"
port = "out"
[[edges]]
from = "check"
to = "ship"
port = "pass"
[[edges]]
from = "check"
to = "fix"
port = "other"
"#;

    /// Runs nodes through the registry, as the server does.
    struct Registry;

    impl Runner for Registry {
        fn run(&self, node: &NodeDefinition, input: &Value) -> Result<Value, NodeError> {
            super::super::node(&node.kind, &node.params)?.run(input.clone())
        }

        fn run_routed(&self, node: &NodeDefinition, scope: &Scope) -> Result<Routed, NodeError> {
            super::super::node(&node.kind, &node.params)?.run_routed(scope)
        }
    }

    /// Records the ids of the nodes the executor starts.
    #[derive(Default)]
    struct Started(Vec<String>);

    impl Observer for Started {
        fn started(&mut self, node: &NodeDefinition, _input: &Value) -> Result<(), run::Error> {
            self.0.push(node.id.clone());
            Ok(())
        }

        fn finished(
            &mut self,
            _node: &NodeDefinition,
            _result: &StepResult,
        ) -> Result<(), run::Error> {
            Ok(())
        }
    }

    /// Runs `text` with the real executor and returns the nodes that ran, or the error that ended the run.
    fn execute_nodes(text: &str, start: Value) -> Result<Vec<String>, run::Error> {
        let definition = WorkflowDefinition::parse(text, "route.toml").unwrap();
        let runner: Arc<dyn Runner> = Arc::new(Registry);
        let mut started = Started::default();
        run::execute(
            &definition,
            start,
            run::Limits::default(),
            &runner,
            &mut started,
        )
        .map(|_| started.0)
    }

    #[test]
    fn the_executor_runs_the_taken_path_and_not_the_other() {
        let first = execute_nodes(WORKFLOW, json!({ "ok": true })).unwrap();
        assert_eq!(first, ["check", "ship"]);
        let second = execute_nodes(WORKFLOW, json!({ "ok": false })).unwrap();
        assert_eq!(second, ["check", "fix"]);
    }

    #[test]
    fn the_executor_gives_expressions_the_trigger_and_earlier_nodes() {
        let text = r#"
name = "reads"
description = "Route by the trigger and an earlier node."
[[trigger]]
kind = "test"
[[nodes]]
id = "first"
kind = "test"
[[nodes]]
id = "check"
kind = "condition"
params = { ports = [{ name = "pass", when = "trigger.ok and node('first').seen" }] }
[[nodes]]
id = "ship"
kind = "test"
[[edges]]
from = "trigger"
to = "first"
port = "out"
[[edges]]
from = "first"
to = "check"
port = "out"
[[edges]]
from = "check"
to = "ship"
port = "pass"
"#;
        let ran = execute_nodes(text, json!({ "ok": true, "seen": true }));
        assert_eq!(ran.unwrap(), ["first", "check", "ship"]);
    }

    #[test]
    fn no_true_port_and_no_fallback_fails_the_run_closed() {
        let text = WORKFLOW.replace(", fallback = \"other\"", "");
        let text = text.replace("port = \"other\"", "port = \"pass\"");
        let error = execute_nodes(&text, json!({ "ok": false })).unwrap_err();
        assert!(
            error.to_string().contains("no fallback is declared"),
            "{error}"
        );
        // Nothing after the condition ran: the observer saw only the condition and the run failed there.
        assert!(matches!(error, run::Error::Node(_)));
    }

    #[test]
    fn an_edge_by_a_port_the_condition_does_not_have_is_refused() {
        let text = WORKFLOW.replace("port = \"other\"", "port = \"nowhere\"");
        let definition = WorkflowDefinition::parse(&text, "route.toml").unwrap();
        let message = definition.validate("route.toml").unwrap_err().to_string();
        assert!(
            message.contains("node \"check\": edge to \"fix\" leaves by port \"nowhere\""),
            "{message}"
        );
    }

    #[test]
    fn a_port_with_no_edge_is_refused_by_validation() {
        let no_edge = r#"
[[edges]]
from = "check"
to = "fix"
port = "other"
"#;

        // The fallback keeps its edge, and a declared port has none.
        let text = WORKFLOW
            .replace(
                "}], fallback = \"other\"",
                "}, { name = \"retry\", when = \"input.ok\" }]",
            )
            .replace(no_edge, "");
        let definition = WorkflowDefinition::parse(&text, "route.toml").unwrap();
        let message = definition.validate("route.toml").unwrap_err().to_string();
        assert!(
            message.contains("node \"check\": port \"retry\" has no edge leaving by it"),
            "{message}"
        );

        // And a `fallback` with no edge is refused the same way.
        let text = WORKFLOW.replace(no_edge, "");
        let definition = WorkflowDefinition::parse(&text, "route.toml").unwrap();
        let message = definition.validate("route.toml").unwrap_err().to_string();
        assert!(
            message.contains("node \"check\": port \"other\" has no edge leaving by it"),
            "{message}"
        );
    }
}
