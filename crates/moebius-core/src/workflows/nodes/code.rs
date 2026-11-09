//! The `code` node: Luau that reshapes the value that reached the node. The code runs in the sandbox expressions
//! use, with the same scope — `trigger`, `input` and `node("<id>")` — and what it returns is the node's output. A
//! limit of the sandbox stops it and the node fails closed, naming the limit.
//!
//! `source` is a parameter like any other, so it is filled from expressions before it runs; a table of tables in the
//! code is written `{ {1}, {2} }`, since `{{` starts an expression. The code may read the run through its scope as
//! well, so what an expression would have substituted into the text is also reachable by name.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{Lua, Value, VmState};
use serde_json::{Map, Value as Json};

use super::{Metadata, Node, NodeError, Ports, Routed};
use crate::board::Limits;
use crate::workflows::decorate::{self, Scope, json_to_lua, lua_to_json};

/// The kind name a workflow file writes.
pub const KIND: &str = "code";

/// How many times the Luau interrupt may fire for one run of the code, beside the time and memory limits.
const INSTRUCTIONS: u64 = 1_000_000;

/// Which limit of the sandbox stopped the code. The interrupt names the limit as it fires it, so a failure says
/// which limit was reached without reading that back out of the words of an error message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Limit {
    Time,
    Instructions,
}

impl Limit {
    fn message(self) -> &'static str {
        match self {
            Self::Time => "the code ran out of time",
            Self::Instructions => "the code ran out of instructions",
        }
    }
}

/// A node that runs Luau and outputs what it returns.
#[derive(Debug, Clone)]
pub struct CodeNode {
    source: String,
    time: Duration,
    memory: usize,
    instructions: u64,
}

impl CodeNode {
    /// Builds the node from its parameters: `source`, the Luau to run, as a string.
    pub fn from_params(params: &Json) -> Result<Self, NodeError> {
        let invalid = |message: String| NodeError::Parameters {
            kind: KIND.to_string(),
            message,
        };
        let Some(map) = params.as_object() else {
            return Err(invalid("the parameters must be a table".to_string()));
        };
        if let Some(unknown) = map.keys().find(|key| key.as_str() != "source") {
            return Err(invalid(format!("unknown parameter \"{unknown}\"")));
        }
        let source = match map.get("source") {
            Some(Json::String(source)) if !source.trim().is_empty() => source.clone(),
            Some(Json::String(_)) => return Err(invalid("\"source\" is empty".to_string())),
            Some(_) => return Err(invalid("\"source\" must be a string".to_string())),
            None => return Err(invalid("\"source\" is required".to_string())),
        };
        let limits = Limits::default();
        Ok(Self {
            source,
            time: limits.script_time,
            memory: limits.script_memory,
            instructions: INSTRUCTIONS,
        })
    }

    /// The source with its expressions filled from `scope`, the way every node kind's parameters are filled. The
    /// source may be one expression whose value is the code, or text with expressions in it.
    fn source(&self, scope: &Scope) -> Result<String, NodeError> {
        let params = Json::Object(Map::from_iter([(
            "source".to_string(),
            Json::String(self.source.clone()),
        )]));
        let filled = decorate::fill(KIND, &params, scope)
            .map_err(|error| NodeError::Failed(format!("the source: {}", error.message)))?;
        match filled.get("source") {
            Some(Json::String(source)) if !source.trim().is_empty() => Ok(source.clone()),
            Some(Json::String(_)) => Err(NodeError::Failed(
                "the source is empty once its expressions are filled".to_string(),
            )),
            Some(_) => Err(NodeError::Failed(
                "the source is not a string once its expressions are filled".to_string(),
            )),
            None => unreachable!("filling the source kept the parameter's name"),
        }
    }

    /// Runs the code with `scope` around it and makes JSON of what it returned. A limit of the sandbox is a failure
    /// that names the limit; so is anything else the code, or its returned value, got wrong.
    fn evaluate(&self, source: &str, scope: &Scope) -> Result<Json, NodeError> {
        let stopped_at = Rc::new(Cell::new(None));
        self.run_sandboxed(source, scope, &stopped_at)
            .map_err(|error| NodeError::Failed(why(&error, stopped_at.get())))
    }

    /// Runs the chunk in a sandbox with nothing reachable but its scope, and turns what it returned into JSON while
    /// the sandbox is still there to read the value through. `stopped_at` records which limit the interrupt fired,
    /// when a limit fired.
    fn run_sandboxed(
        &self,
        source: &str,
        scope: &Scope,
        stopped_at: &Rc<Cell<Option<Limit>>>,
    ) -> mlua::Result<Json> {
        let lua = Lua::new();
        lua.set_memory_limit(self.memory)?;
        let environment = lua.create_table()?;
        environment.raw_set("trigger", json_to_lua(&lua, &scope.trigger)?)?;
        environment.raw_set("input", json_to_lua(&lua, &scope.input)?)?;
        let nodes = lua.create_table()?;
        for (id, value) in &scope.nodes {
            nodes.raw_set(id.as_str(), json_to_lua(&lua, value)?)?;
        }
        let node_function = lua.create_function(move |_, id: String| {
            match nodes.raw_get::<Value>(id.as_str())? {
                Value::Nil => Err(mlua::Error::runtime(format!(
                    "no earlier node is named {id:?}"
                ))),
                value => Ok(value),
            }
        })?;
        environment.raw_set("node", node_function)?;
        lua.sandbox(true)?;

        let deadline = Instant::now() + self.time;
        let used = Rc::new(Cell::new(0u64));
        let limit = self.instructions;
        let interrupt = Rc::clone(stopped_at);
        lua.set_interrupt(move |_| {
            if Instant::now() > deadline {
                interrupt.set(Some(Limit::Time));
                return Err(mlua::Error::runtime(Limit::Time.message()));
            }
            used.set(used.get() + 1);
            if used.get() > limit {
                interrupt.set(Some(Limit::Instructions));
                return Err(mlua::Error::runtime(Limit::Instructions.message()));
            }
            Ok(VmState::Continue)
        });

        let value = lua
            .load(source)
            .set_name("code")
            .set_environment(environment)
            .call::<Value>(())?;
        lua_to_json(value, "the code")
    }
}

/// Why the code did not produce a value: the limit that stopped it, when the sandbox stopped it, and otherwise what
/// Luau or the conversion said. The kind of the error says which limit it was — the memory limit is the one Luau
/// reports itself, and it reports it as an error of its own — so nothing is read out of the words of a message the
/// code could have written.
fn why(error: &mlua::Error, stopped_at: Option<Limit>) -> String {
    if let Some(limit) = stopped_at {
        return limit.message().to_string();
    }
    match error {
        mlua::Error::MemoryError(_) => "the code ran out of memory".to_string(),
        // An error of Luau's own, which already says what went wrong in its own words.
        mlua::Error::RuntimeError(message) => message.clone(),
        other => other.to_string(),
    }
}

impl Node for CodeNode {
    /// Runs the code with `input` in scope. Without a run around it there is no trigger (`nil`) and no earlier node;
    /// the executor calls [`Node::run_routed`], which gives the code the run's whole scope.
    fn run(&self, input: Json) -> Result<Json, NodeError> {
        let scope = Scope {
            trigger: Json::Null,
            input,
            nodes: Default::default(),
        };
        Ok(self.run_routed(&scope)?.value)
    }

    /// Runs the code against `scope` and outputs what it returned, leaving by `out`: the code reshapes the value,
    /// it does not choose a port.
    fn run_routed(&self, scope: &Scope) -> Result<Routed, NodeError> {
        let source = self.source(scope)?;
        Ok(Routed {
            value: self.evaluate(&source, scope)?,
            port: None,
        })
    }

    fn ports(&self) -> Ports {
        Ports::new(["in"], ["out"])
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            kind: KIND,
            title: "Code",
            description: "Runs Luau that reshapes the value and outputs what it returns.",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;
    use crate::workflows::decorate::MAX_DEPTH;

    fn node(source: &str) -> CodeNode {
        CodeNode::from_params(&json!({ "source": source })).unwrap()
    }

    fn scope() -> Scope {
        Scope {
            trigger: json!({"repository": "moebius", "snippet": "return 7"}),
            input: json!({"n": 1}),
            nodes: BTreeMap::from([("build".to_string(), json!({"count": 3}))]),
        }
    }

    #[test]
    fn json_in_transformed_json_out() {
        let output = node(
            "local names = {} for _, user in input.users do names[#names + 1] = user.name .. '!' end \
             return { names = names, count = #names }",
        )
        .run(json!({"users": [{"name": "ada"}, {"name": "lin"}]}))
        .unwrap();
        assert_eq!(output, json!({"names": ["ada!", "lin!"], "count": 2}));
    }

    #[test]
    fn the_trigger_and_earlier_nodes_are_in_scope_of_a_run() {
        let output = node("return trigger.repository .. ':' .. (input.n + node('build').count)")
            .run_routed(&scope())
            .unwrap()
            .value;
        assert_eq!(output, json!("moebius:4"));
        let error = node("return node('missing')")
            .run_routed(&scope())
            .unwrap_err();
        assert!(error.to_string().contains("no earlier node"), "{error}");
    }

    /// The source is a parameter like every other, so an expression in it is filled from the run before the code
    /// runs: text around it has its value substituted, and a source that is exactly one expression becomes that
    /// expression's value.
    #[test]
    fn the_source_is_filled_from_expressions_like_every_other_parameter() {
        let output = node(
            "return { repository = '{{ trigger.repository }}', count = {{ node('build').count }} }",
        )
        .run_routed(&scope())
        .unwrap()
        .value;
        assert_eq!(output, json!({"repository": "moebius", "count": 3}));

        // A source that is exactly one expression becomes the string the expression returned, and that code runs.
        assert_eq!(
            node("{{ trigger.snippet }}")
                .run_routed(&scope())
                .unwrap()
                .value,
            json!(7)
        );

        // Luau writes a table of tables with a space, so `{{` is never mistaken for a table.
        assert_eq!(
            node("return { {1}, {2} }").run(json!({})).unwrap(),
            json!([[1], [2]])
        );
    }

    #[test]
    fn an_expression_in_the_source_that_fails_fails_the_node() {
        let error = node("return '{{ trigger.missing.name }}'")
            .run_routed(&scope())
            .unwrap_err();
        assert!(matches!(error, NodeError::Failed(_)), "{error}");
        assert!(error.to_string().contains("the source"), "{error}");

        let error = node("return '{{ input.n }'")
            .run_routed(&scope())
            .unwrap_err();
        assert!(error.to_string().contains("not closed"), "{error}");

        let error = node("{{ '' }}").run_routed(&scope()).unwrap_err();
        assert!(error.to_string().contains("empty"), "{error}");

        let error = node("{{ 1 }}").run_routed(&scope()).unwrap_err();
        assert!(error.to_string().contains("not a string"), "{error}");
    }

    #[test]
    fn code_that_returns_nothing_outputs_null() {
        assert_eq!(node("local x = 1").run(json!(1)).unwrap(), Json::Null);
    }

    #[test]
    fn an_error_in_the_code_fails_the_node() {
        let error = node("return input.missing.name")
            .run(json!({}))
            .unwrap_err();
        assert!(matches!(error, NodeError::Failed(_)));
        assert!(error.to_string().contains("index nil"), "{error}");
        let error = node("return input.a.b").run(json!({})).unwrap_err();
        assert!(matches!(error, NodeError::Failed(_)), "{error}");
        let error = node("return (").run(json!({})).unwrap_err();
        assert!(matches!(error, NodeError::Failed(_)), "{error}");
    }

    /// An ordinary error whose text happens to name a limit is still the error it is, and not a report that the
    /// limit was reached.
    #[test]
    fn an_error_naming_a_limit_in_its_own_words_is_not_reported_as_that_limit() {
        for source in [
            "local t = nil return t.memory",
            "local t = nil return t.low_memory_mode",
            "local t = nil return t.instructions",
        ] {
            let error = node(source).run(json!({})).unwrap_err();
            let message = error.to_string();
            assert!(
                !message.contains("ran out of"),
                "{source} was reported as a limit: {message}"
            );
            assert!(message.contains("index nil"), "{source}: {message}");
        }
    }

    /// Everything the node says about its own value names the code, since it is the code, not an expression, that
    /// returned it.
    #[test]
    fn a_value_the_node_cannot_return_names_the_code() {
        let error = node("return function() end").run(json!({})).unwrap_err();
        assert_eq!(
            error,
            NodeError::Failed("the code returned a function JSON cannot hold".to_string())
        );
        let error = node("local t = {} t.t = t return t")
            .run(json!({}))
            .unwrap_err();
        assert_eq!(
            error,
            NodeError::Failed(format!(
                "the code returned a value nested deeper than {MAX_DEPTH} levels"
            ))
        );
    }

    #[test]
    fn a_sandbox_limit_fails_the_node_and_names_it() {
        let mut instructions = node("while true do end");
        instructions.instructions = 10;
        let error = instructions.run(json!({})).unwrap_err();
        assert_eq!(
            error,
            NodeError::Failed("the code ran out of instructions".into())
        );

        let mut time = node("while true do end");
        time.time = Duration::from_millis(50);
        time.instructions = u64::MAX;
        let error = time.run(json!({})).unwrap_err();
        assert_eq!(error, NodeError::Failed("the code ran out of time".into()));

        let mut memory = node("local t = {} for _ = 1, 100000 do t[_] = { t } end return t");
        memory.time = Duration::from_secs(60);
        memory.instructions = u64::MAX;
        memory.memory = 1 << 20;
        let error = memory.run(json!({})).unwrap_err();
        assert_eq!(
            error,
            NodeError::Failed("the code ran out of memory".into())
        );
    }

    #[test]
    fn a_value_that_holds_itself_fails_the_node_rather_than_the_process() {
        let error = node("local t = {} t.t = t return t")
            .run(json!({}))
            .unwrap_err();
        assert!(error.to_string().contains("nested deeper than"), "{error}");
    }

    #[test]
    fn a_value_nested_past_the_depth_a_node_may_return_fails_it() {
        let deep = node("local t = {} for _ = 1, 200 do t = { below = t } end return t")
            .run(json!({}))
            .unwrap_err();
        assert!(
            deep.to_string()
                .contains(&format!("nested deeper than {MAX_DEPTH}")),
            "{deep}"
        );
        // The same code well within the depth goes through, so the limit is the depth and not the tables.
        let within = node("local t = {} for _ = 1, 20 do t = { below = t } end return t")
            .run(json!({}))
            .unwrap();
        let mut depth = 0;
        let mut value = &within;
        while let Some(below) = value.get("below") {
            depth += 1;
            value = below;
        }
        assert_eq!(depth, 20);
    }

    #[test]
    fn the_code_cannot_reach_files_network_or_processes() {
        let output = node(
            "return { io == nil, os == nil, require == nil, package == nil, debug == nil, load == nil, loadfile == nil, \
             dofile == nil, socket == nil, http == nil, _G == nil, getfenv == nil, setfenv == nil, print == nil }",
        )
        .run(json!({}))
        .unwrap();
        let probe = output.as_array().expect("the probe is an array");
        assert_eq!(probe.len(), 14);
        assert!(probe.iter().all(|value| value == &json!(true)), "{probe:?}");
    }

    #[test]
    fn the_parameters_are_checked() {
        for params in [
            json!({}),
            json!({"source": 1}),
            json!({"source": "  "}),
            json!({"source": "return 1", "extra": true}),
            json!("return 1"),
        ] {
            assert!(
                matches!(
                    CodeNode::from_params(&params),
                    Err(NodeError::Parameters { .. })
                ),
                "{params}"
            );
        }
    }

    #[test]
    fn the_registry_builds_the_node_and_it_reports_itself() {
        let built =
            crate::workflows::nodes::node(KIND, &json!({"source": "return input"})).unwrap();
        assert_eq!(built.metadata().kind, "code");
        assert_eq!(built.ports(), Ports::new(["in"], ["out"]));
        assert!(!built.routes(), "the code does not choose a port");
        assert_eq!(built.run(json!([1, 2])).unwrap(), json!([1, 2]));
        assert!(matches!(
            crate::workflows::nodes::node(KIND, &json!({})),
            Err(NodeError::Parameters { .. })
        ));
    }

    /// A run reaches a node through the registry alone, so the run's scope arrives there and not by a kind name.
    #[test]
    fn the_registry_hands_the_node_the_scope_of_the_run() {
        let built = crate::workflows::nodes::node(
            KIND,
            &json!({"source": "return { here = trigger.repository, before = node('build').count, n = input.n }"}),
        )
        .unwrap();
        let output = built.run_routed(&scope()).unwrap();
        assert_eq!(
            output.value,
            json!({"here": "moebius", "before": 3, "n": 1})
        );
        assert_eq!(output.port, None);
    }
}
