//! Expressions in workflow parameters: a parameter string may hold `{{ ... }}`, which is evaluated as Luau in the
//! sandbox column scripts use. The trigger, the value that reached the node and the values of earlier nodes are in
//! scope; nothing else is. This is what fills every node kind's parameters from the nodes before it, so no node kind
//! has to know about expressions itself.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{Lua, Value, VmState};
use serde_json::{Map, Number, Value as Json};
use thiserror::Error;

use crate::board::Limits;

/// How many times the Luau interrupt may fire for one expression before it is stopped. It is a bound on the work an
/// expression may do, beside the time and memory limits the sandbox already has.
const INSTRUCTIONS: u64 = 100_000;

/// What an expression may read: the trigger's value, the value that reached the node, and the values of earlier
/// nodes by their id.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    /// The value the workflow's trigger produced.
    pub trigger: Json,
    /// The value that reached the node through its input edge.
    pub input: Json,
    /// The values earlier nodes produced, by node id.
    pub nodes: BTreeMap<String, Json>,
}

/// An expression that could not be evaluated. It names the node and parameter it was in, and fails closed: the
/// workflow does not get a value the expression did not produce.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("node {node:?} parameter {parameter:?}: {message}")]
pub struct Error {
    /// The id of the node whose parameter held the expression.
    pub node: String,
    /// The parameter's name, with `.key` and `[n]` for a value nested in a table or array.
    pub parameter: String,
    /// What went wrong, including the name of a sandbox limit that stopped the expression.
    pub message: String,
}

impl Error {
    fn new(node: &str, parameter: &str, message: impl Into<String>) -> Self {
        Self {
            node: node.to_string(),
            parameter: parameter.to_string(),
            message: message.into(),
        }
    }
}

/// Fills every parameter of `params` with the expressions it holds. Non-string values are kept, strings without
/// `{{` are kept, and a string that is one expression becomes that expression's value with its type kept.
pub fn fill(node: &str, params: &Json, scope: &Scope) -> Result<Json, Error> {
    fill_with_limits(node, params, scope, &Limits::default(), INSTRUCTIONS)
}

fn fill_with_limits(
    node: &str,
    params: &Json,
    scope: &Scope,
    limits: &Limits,
    instructions: u64,
) -> Result<Json, Error> {
    let sandbox = Sandbox {
        time: limits.script_time,
        memory: limits.script_memory,
        instructions,
    };
    match params {
        Json::Object(map) => {
            let mut filled = Map::new();
            for (name, value) in map {
                filled.insert(
                    name.clone(),
                    evaluate_value(node, name, value, scope, &sandbox)?,
                );
            }
            Ok(Json::Object(filled))
        }
        other => evaluate_value(node, "params", other, scope, &sandbox),
    }
}

fn evaluate_value(
    node: &str,
    parameter: &str,
    value: &Json,
    scope: &Scope,
    sandbox: &Sandbox,
) -> Result<Json, Error> {
    match value {
        Json::String(text) => evaluate_text(node, parameter, text, scope, sandbox),
        Json::Array(items) => {
            let mut filled = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                let name = format!("{parameter}[{index}]");
                filled.push(evaluate_value(node, &name, item, scope, sandbox)?);
            }
            Ok(Json::Array(filled))
        }
        Json::Object(map) => {
            let mut filled = Map::new();
            for (key, item) in map {
                let name = if parameter.is_empty() {
                    key.clone()
                } else {
                    format!("{parameter}.{key}")
                };
                filled.insert(
                    key.clone(),
                    evaluate_value(node, &name, item, scope, sandbox)?,
                );
            }
            Ok(Json::Object(filled))
        }
        other => Ok(other.clone()),
    }
}

fn evaluate_text(
    node: &str,
    parameter: &str,
    text: &str,
    scope: &Scope,
    sandbox: &Sandbox,
) -> Result<Json, Error> {
    let parts = parts(text).map_err(|message| Error::new(node, parameter, message))?;
    if parts.is_empty() {
        return Ok(Json::String(text.to_string()));
    }
    if let [Part::Expression(expression)] = parts.as_slice() {
        return evaluate_expression(node, parameter, expression, scope, sandbox);
    }
    if parts.iter().all(|part| matches!(part, Part::Text(_))) {
        return Ok(Json::String(text.to_string()));
    }
    let mut rendered = String::new();
    for part in parts {
        match part {
            Part::Text(text) => rendered.push_str(text),
            Part::Expression(expression) => {
                let value = evaluate_expression(node, parameter, expression, scope, sandbox)?;
                match value {
                    Json::String(text) => rendered.push_str(&text),
                    other => rendered.push_str(&other.to_string()),
                }
            }
        }
    }
    Ok(Json::String(rendered))
}

enum Part<'a> {
    Text(&'a str),
    Expression(&'a str),
}

/// Splits `text` at `{{` and `}}`. A `{{` with no closing `}}` is an error rather than literal text, so a mistake
/// never silently becomes a parameter value.
fn parts(text: &str) -> Result<Vec<Part<'_>>, String> {
    let mut parts = Vec::new();
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find("{{").map(|at| cursor + at) {
        if start > cursor {
            parts.push(Part::Text(&text[cursor..start]));
        }
        let inner = start + 2;
        let Some(end) = text[inner..].find("}}").map(|at| inner + at) else {
            return Err("the expression is not closed with }}".to_string());
        };
        parts.push(Part::Expression(&text[inner..end]));
        cursor = end + 2;
    }
    if cursor < text.len() {
        parts.push(Part::Text(&text[cursor..]));
    }
    Ok(parts)
}

struct Sandbox {
    time: Duration,
    memory: usize,
    instructions: u64,
}

fn evaluate_expression(
    node: &str,
    parameter: &str,
    expression: &str,
    scope: &Scope,
    sandbox: &Sandbox,
) -> Result<Json, Error> {
    let lua = Lua::new();
    lua.set_memory_limit(sandbox.memory)
        .map_err(|error| Error::new(node, parameter, describe(&error)))?;
    let environment = lua
        .create_table()
        .map_err(|error| Error::new(node, parameter, describe(&error)))?;
    let trigger = json_to_lua(&lua, &scope.trigger)
        .map_err(|error| Error::new(node, parameter, describe(&error)))?;
    environment
        .raw_set("trigger", trigger)
        .map_err(|error| Error::new(node, parameter, describe(&error)))?;
    let input = json_to_lua(&lua, &scope.input)
        .map_err(|error| Error::new(node, parameter, describe(&error)))?;
    environment
        .raw_set("input", input)
        .map_err(|error| Error::new(node, parameter, describe(&error)))?;
    let nodes = lua
        .create_table()
        .map_err(|error| Error::new(node, parameter, describe(&error)))?;
    for (id, value) in &scope.nodes {
        let value = json_to_lua(&lua, value)
            .map_err(|error| Error::new(node, parameter, describe(&error)))?;
        nodes
            .raw_set(id.as_str(), value)
            .map_err(|error| Error::new(node, parameter, describe(&error)))?;
    }
    let earlier = nodes.clone();
    let node_function = lua
        .create_function(
            move |_, id: String| match earlier.raw_get::<Value>(id.as_str())? {
                Value::Nil => Err(mlua::Error::runtime(format!(
                    "no earlier node is named {id:?}"
                ))),
                value => Ok(value),
            },
        )
        .map_err(|error| Error::new(node, parameter, describe(&error)))?;
    environment
        .raw_set("node", node_function)
        .map_err(|error| Error::new(node, parameter, describe(&error)))?;
    lua.sandbox(true)
        .map_err(|error| Error::new(node, parameter, describe(&error)))?;

    let deadline = Instant::now() + sandbox.time;
    let instructions = Rc::new(Cell::new(0));
    let limit = sandbox.instructions;
    lua.set_interrupt(move |_| {
        if Instant::now() > deadline {
            return Err(mlua::Error::runtime("the expression ran out of time"));
        }
        let used = instructions.get() + 1;
        instructions.set(used);
        if used > limit {
            return Err(mlua::Error::runtime(
                "the expression ran out of instructions",
            ));
        }
        Ok(VmState::Continue)
    });

    let source = format!("return ({expression})");
    let result = lua
        .load(source)
        .set_name(format!("{node}:{parameter}"))
        .set_environment(environment)
        .eval::<Value>();
    match result {
        Ok(value) => {
            lua_to_json(value).map_err(|error| Error::new(node, parameter, describe(&error)))
        }
        Err(error) => Err(Error::new(node, parameter, describe(&error))),
    }
}

fn describe(error: &mlua::Error) -> String {
    let message = error.to_string();
    if message.contains("ran out of time") {
        "the expression ran out of time".to_string()
    } else if message.contains("ran out of instructions") {
        "the expression ran out of instructions".to_string()
    } else if message.to_ascii_lowercase().contains("memory") {
        "the expression ran out of memory".to_string()
    } else {
        message
    }
}

fn json_to_lua(lua: &Lua, value: &Json) -> mlua::Result<Value> {
    Ok(match value {
        Json::Null => Value::Nil,
        Json::Bool(value) => Value::Boolean(*value),
        Json::Number(number) => {
            if let Some(value) = number.as_i64() {
                Value::Integer(value)
            } else if let Some(value) = number.as_f64() {
                Value::Number(value)
            } else {
                return Err(mlua::Error::runtime(
                    "a number cannot be represented in Luau",
                ));
            }
        }
        Json::String(value) => Value::String(lua.create_string(value)?),
        Json::Array(items) => {
            let table = lua.create_table()?;
            for (index, item) in items.iter().enumerate() {
                table.raw_set(index + 1, json_to_lua(lua, item)?)?;
            }
            Value::Table(table)
        }
        Json::Object(map) => {
            let table = lua.create_table()?;
            for (key, item) in map {
                table.raw_set(key.as_str(), json_to_lua(lua, item)?)?;
            }
            Value::Table(table)
        }
    })
}

fn lua_to_json(value: Value) -> mlua::Result<Json> {
    Ok(match value {
        Value::Nil => Json::Null,
        Value::Boolean(value) => Json::Bool(value),
        Value::Integer(value) => Json::Number(Number::from(value)),
        Value::Number(value) => Number::from_f64(value).map(Json::Number).ok_or_else(|| {
            mlua::Error::runtime("the expression returned a number JSON cannot hold")
        })?,
        Value::String(value) => Json::String(value.to_string_lossy()),
        Value::Table(table) => table_to_json(table)?,
        other => {
            return Err(mlua::Error::runtime(format!(
                "the expression returned a {} JSON cannot hold",
                other.type_name()
            )));
        }
    })
}

fn table_to_json(table: mlua::Table) -> mlua::Result<Json> {
    let entries: Vec<(Value, Value)> =
        table.pairs::<Value, Value>().collect::<mlua::Result<_>>()?;
    if entries.is_empty() {
        return Ok(Json::Array(Vec::new()));
    }
    let len = entries.len();
    let mut slots: Vec<Option<Json>> = vec![None; len];
    let mut sequence = true;
    for (key, value) in &entries {
        let index = match key {
            Value::Integer(index) if *index >= 1 && (*index as usize) <= len => {
                Some(*index as usize - 1)
            }
            _ => None,
        };
        if let Some(index) = index
            && slots[index].is_none()
        {
            slots[index] = Some(lua_to_json(value.clone())?);
            continue;
        }
        sequence = false;
    }
    if sequence && slots.iter().all(Option::is_some) {
        return Ok(Json::Array(
            slots.into_iter().flatten().collect::<Vec<Json>>(),
        ));
    }
    let mut map = Map::new();
    for (key, value) in entries {
        let key = match key {
            Value::String(key) => key.to_string_lossy(),
            Value::Integer(key) => key.to_string(),
            Value::Number(key) => key.to_string(),
            Value::Boolean(key) => key.to_string(),
            _ => return Err(mlua::Error::runtime("a table key is not a string")),
        };
        map.insert(key, lua_to_json(value)?);
    }
    Ok(Json::Object(map))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use serde_json::json;

    use super::*;

    fn scope() -> Scope {
        Scope {
            trigger: json!({"repository": "moebius", "number": 7}),
            input: json!({"branch": "main"}),
            nodes: BTreeMap::from([(
                "build".to_string(),
                json!({"command": "cargo build", "count": 3}),
            )]),
        }
    }

    #[test]
    fn an_expression_reads_the_trigger_and_the_input() {
        let filled = fill(
            "test",
            &json!({
                "repository": "{{ trigger.repository }}",
                "branch": "{{ input.branch }}",
            }),
            &scope(),
        )
        .unwrap();
        assert_eq!(filled, json!({"repository": "moebius", "branch": "main"}));
    }

    #[test]
    fn an_expression_reads_an_earlier_node() {
        let filled = fill(
            "test",
            &json!({
                "command": "{{ node(\"build\").command }}",
                "count": "{{ node(\"build\").count + 1 }}",
                "summary": "building with {{ node('build').command }}",
            }),
            &scope(),
        )
        .unwrap();
        assert_eq!(
            filled,
            json!({
                "command": "cargo build",
                "count": 4,
                "summary": "building with cargo build",
            })
        );
    }

    #[test]
    fn text_without_an_expression_is_left_alone() {
        let params = json!({
            "command": "cargo build",
            "retries": 3,
            "options": {"release": true},
            "args": ["--locked"],
        });
        assert_eq!(fill("test", &params, &scope()).unwrap(), params);
    }

    #[test]
    fn an_error_names_the_node_and_parameter() {
        let error = fill("test", &json!({"command": "{{ nil + 1 }}"}), &scope()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("\"test\""), "{message}");
        assert!(message.contains("\"command\""), "{message}");
    }

    #[test]
    fn an_unknown_earlier_node_is_an_error() {
        let error = fill(
            "test",
            &json!({"command": "{{ node(\"missing\").command }}"}),
            &scope(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("no earlier node"), "{error}");
    }

    #[test]
    fn an_expression_that_never_finishes_is_stopped_by_a_limit() {
        let loop_forever = json!({"loop": "{{ (function() while true do end end)() }}"});
        let by_instructions =
            fill_with_limits("test", &loop_forever, &scope(), &Limits::default(), 10).unwrap_err();
        assert!(
            by_instructions
                .to_string()
                .contains("ran out of instructions"),
            "{by_instructions}"
        );
        let by_time = fill_with_limits(
            "test",
            &loop_forever,
            &scope(),
            &Limits {
                script_time: Duration::from_millis(50),
                ..Limits::default()
            },
            u64::MAX,
        )
        .unwrap_err();
        assert!(by_time.to_string().contains("ran out of time"), "{by_time}");
    }

    #[test]
    fn an_expression_cannot_reach_files_network_or_processes() {
        let filled = fill(
            "test",
            &json!({
                "probe": "{{ { io == nil, os == nil, require == nil, package == nil, debug == nil, load == nil, loadfile == nil, dofile == nil, socket == nil, http == nil, _G == nil, getfenv == nil, setfenv == nil, string == nil, table == nil, math == nil, coroutine == nil, utf8 == nil, bit32 == nil, buffer == nil, vector == nil, print == nil, pairs == nil, ipairs == nil, tostring == nil, type == nil, next == nil, pcall == nil, error == nil, assert == nil, select == nil, tonumber == nil, rawget == nil, rawset == nil, rawequal == nil, rawlen == nil, unpack == nil } }}",
            }),
            &scope(),
        )
        .unwrap();
        let probe = filled["probe"].as_array().expect("the probe is an array");
        assert!(!probe.is_empty());
        assert!(probe.iter().all(|value| value == &json!(true)), "{probe:?}");
    }
}
