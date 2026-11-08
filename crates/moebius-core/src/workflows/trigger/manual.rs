//! The manual trigger: a person starts each of its runs — through `POST /v1/workflows/{name}/runs` or
//! `moebius workflow run` — giving it the inputs the trigger names. The inputs live in the trigger's parameters, are
//! checked when the workflow's file is read, and each is checked against its type when a run is started, so a run
//! never starts with an input the trigger does not know or a value its type does not take.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::workflows::nodes::Ports;

/// The kind name a workflow's file writes for the manual trigger.
pub const KIND: &str = "manual";

/// The type of an input of a manual trigger: what a value of the input must be when a run is started, and the refusal
/// a value that is not it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InputType {
    /// A JSON string.
    Text,
    /// A JSON number.
    Number,
    /// `true` or `false`.
    Boolean,
    /// One of the input's `choices`.
    Choice,
    /// A non-empty string that names a repository the server knows.
    Repository,
}

/// One input a manual trigger names.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Input {
    /// The name the inputs of a run give it by, and a refusal names it by.
    pub name: String,
    /// What a value of the input must be.
    #[serde(rename = "type")]
    pub ty: InputType,
    /// Whether a run must give it a value, or take it by default.
    pub required: bool,
    /// The value a run takes when it gives the input none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// The values a `choice` input may be; empty for the other types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
}

/// The manual trigger: the inputs it names, and what each of its runs starts with.
#[derive(Debug, Clone, PartialEq)]
pub struct ManualTrigger {
    /// The inputs in the order the trigger's file names them.
    pub inputs: Vec<Input>,
}

impl ManualTrigger {
    /// The ports a value leaves the trigger by: the edges of a workflow name its `out`.
    pub fn ports(&self) -> Ports {
        Ports::new([], ["out"])
    }

    /// The trigger the parameters of a `manual` trigger in a workflow's file name, refused with a message that names
    /// the input that is not one: the inputs must be objects that name each other uniquely, with a known type and —
    /// for a `choice` — the choices it may be, and a default that is a value of its own type.
    pub fn parse(params: &Value) -> Result<Self, String> {
        let table = params
            .as_object()
            .ok_or_else(|| "the manual trigger takes an object of parameters".to_string())?;
        let unknown: Vec<&str> = table
            .keys()
            .filter(|key| key.as_str() != "inputs")
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            return Err(format!("unknown trigger parameters {unknown:?}"));
        }
        let list = match table.get("inputs") {
            None => return Ok(Self { inputs: vec![] }),
            Some(value) => value
                .as_array()
                .ok_or_else(|| "the trigger's \"inputs\" must be a list of inputs".to_string())?,
        };
        let mut inputs = Vec::with_capacity(list.len());
        for (index, item) in list.iter().enumerate() {
            inputs.push(input(index, item)?);
        }
        let mut seen = std::collections::BTreeSet::new();
        for input in &inputs {
            if !seen.insert(&input.name) {
                return Err(format!("input \"{}\": duplicate input name", input.name));
            }
        }
        Ok(Self { inputs })
    }

    /// The value a run starts with, from the inputs a person gave: the value of an input is checked against its type,
    /// a default stands in for an input that was not given, an input that was not given and has no default is left
    /// out of the value, and a name the trigger does not give is refused. Every refusal names the input.
    pub fn resolve(&self, given: &Map<String, Value>) -> Result<Value, String> {
        for name in given.keys() {
            if !self.inputs.iter().any(|input| input.name == *name) {
                return Err(format!("input \"{name}\": the trigger has no input named \"{name}\""));
            }
        }
        let mut value = Map::new();
        for input in &self.inputs {
            let input_value = match given.get(&input.name) {
                Some(value) => value.clone(),
                None => match &input.default {
                    Some(default) => default.clone(),
                    None => {
                        if input.required {
                            return Err(format!("input \"{}\": is required", input.name));
                        }
                        continue;
                    }
                },
            };
            check_value(&label(&input.name), input.ty, &input_value, &input.choices)?;
            value.insert(input.name.clone(), input_value);
        }
        Ok(Value::Object(value))
    }
}

/// A refusal that names its input, as `input "<name>"`.
fn label(name: &str) -> String {
    format!("input \"{name}\"")
}

/// The input at `index` of the trigger's `inputs`, refused when it is not an object with the input's fields.
fn input(index: usize, item: &Value) -> Result<Input, String> {
    let table = item
        .as_object()
        .ok_or_else(|| format!("input {index}: an input is an object of its fields"))?;
    let name = table
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("input {index}: \"name\" is a non-empty string"))?
        .to_string();
    let label = label(&name);
    for key in table.keys() {
        if !["name", "type", "required", "default", "choices"].contains(key.as_str()) {
            return Err(format!("{label}: unknown key \"{key}\""));
        }
    }
    let ty = match table.get("type") {
        Some(value) => input_type(&label, value)?,
        None => return Err(format!("{label}: \"type\" is one of the input's types")),
    };
    let required = table
        .get("required")
        .map(|value| value.as_bool().ok_or_else(|| format!("{label}: \"required\" is true or false")))
        .transpose()?
        .unwrap_or(false);
    let mut choices = Vec::new();
    if let Some(value) = table.get("choices") {
        choices = choices_list(&label, value)?;
    }
    if ty == InputType::Choice {
        if choices.is_empty() {
            return Err(format!("{label}: \"choices\" is a non-empty list of strings"));
        }
    } else if !choices.is_empty() {
        return Err(format!("{label}: \"choices\" is only for a choice input"));
    }
    let mut default = None;
    if let Some(value) = table.get("default") {
        check_value(&format!("{label}: \"default\""), ty, value, &choices)?;
        default = Some(value.clone());
    }
    Ok(Input {
        name,
        ty,
        required,
        default,
        choices,
    })
}

/// The type an input's `"type"` field writes, refused with a message that names the input.
fn input_type(input: &str, value: &Value) -> Result<InputType, String> {
    match value.as_str() {
        Some("text") => Ok(InputType::Text),
        Some("number") => Ok(InputType::Number),
        Some("boolean") => Ok(InputType::Boolean),
        Some("choice") => Ok(InputType::Choice),
        Some("repository") => Ok(InputType::Repository),
        _ => Err(format!("{input}: \"type\" is one of \"text\", \"number\", \"boolean\", \"choice\", \"repository\"")),
    }
}

/// The list of strings an input's `"choices"` field must hold, refused with a message that names the input.
fn choices_list(input: &str, value: &Value) -> Result<Vec<String>, String> {
    let list = value
        .as_array()
        .ok_or_else(|| format!("{input}: \"choices\" is a list of strings"))?;
    let mut choices = Vec::with_capacity(list.len());
    for item in list {
        let choice = item
            .as_str()
            .filter(|choice| !choice.is_empty())
            .ok_or_else(|| format!("{input}: \"choices\" is a list of strings"))?
            .to_string();
        if choices.contains(&choice) {
            return Err(format!("{input}: \"choices\" lists \"{choice}\" twice"));
        }
        choices.push(choice);
    }
    Ok(choices)
}

/// Whether `value` is a value of the input's type, the message that names what it is not when it is not.
fn check_value(what: &str, ty: InputType, value: &Value, choices: &[String]) -> Result<(), String> {
    let takes = match ty {
        InputType::Text => value.is_string(),
        InputType::Number => value.is_number(),
        InputType::Boolean => value.is_boolean(),
        InputType::Choice => value.as_str().is_some_and(|choice| choices.contains(choice)),
        InputType::Repository => value.as_str().is_some_and(|name| !name.is_empty()),
    };
    if takes {
        return Ok(());
    }
    Err(match ty {
        InputType::Text => format!("{what}: must be a JSON string"),
        InputType::Number => format!("{what}: must be a JSON number"),
        InputType::Boolean => format!("{what}: must be true or false"),
        InputType::Choice => format!("{what}: must be one of the input's choices"),
        InputType::Repository => format!("{what}: must be the name of a repository"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn trigger(params: Value) -> Result<ManualTrigger, String> {
        ManualTrigger::parse(&params)
    }

    fn parse(params: Value) -> ManualTrigger {
        trigger(params).expect("a valid trigger")
    }

    #[test]
    fn a_trigger_with_no_inputs_is_a_trigger() {
        let manual = parse(json!({}));
        assert!(manual.inputs.is_empty());
        assert_eq!(manual.resolve(&Map::new()).expect("no inputs to check"), json!({}));
    }

    #[test]
    fn a_parameter_that_is_not_an_object_is_refused() {
        assert!(trigger(json!([])).is_err());
    }

    #[test]
    fn a_parameter_that_is_not_inputs_is_refused() {
        let error = trigger(json!({ "foo": [] })).expect_err("refused");
        assert!(error.contains("\"foo\""), "{error}");
    }

    #[test]
    fn an_input_that_is_not_a_list_is_refused() {
        assert!(trigger(json!({ "inputs": {} })).is_err());
    }

    #[test]
    fn each_type_is_accepted() {
        let manual = parse(json!({ "inputs": [
            { "name": "text", "type": "text" },
            { "name": "number", "type": "number" },
            { "name": "boolean", "type": "boolean" },
            { "name": "choice", "type": "choice", "choices": ["main", "staging"] },
            { "name": "repository", "type": "repository" },
        ] }));
        let value = manual
            .resolve(&Map::from_iter([
                ("text".to_string(), json!("a string")),
                ("number".to_string(), json!(3)),
                ("boolean".to_string(), json!(true)),
                ("choice".to_string(), json!("main")),
                ("repository".to_string(), json!("radiance")),
            ]))
            .expect("a value of every type");
        assert_eq!(
            value,
            json!({
                "text": "a string",
                "number": 3,
                "boolean": true,
                "choice": "main",
                "repository": "radiance"
            })
        );
    }

    #[test]
    fn a_value_that_is_not_the_type_of_its_input_is_refused_and_the_input_is_named() {
        let cases = [
            (json!({ "name": "text", "type": "text" }), json!(3)),
            (json!({ "name": "number", "type": "number" }), json!("3")),
            (json!({ "name": "boolean", "type": "boolean" }), json!("true")),
            (
                json!({ "name": "choice", "type": "choice", "choices": ["main"] }),
                json!("staging"),
            ),
            (json!({ "name": "repository", "type": "repository" }), json!(3)),
            (json!({ "name": "repository", "type": "repository" }), json!("")),
        ];
        for (input, value) in cases {
            let manual = parse(json!({ "inputs": [input] }));
            let name = manual.inputs[0].name.clone();
            let given = Map::from_iter([(name.clone(), value)]);
            let error = manual.resolve(&given).expect_err("refused");
            assert!(error.contains(&format!("\"{name}\"")), "{error}");
        }
    }

    #[test]
    fn a_type_that_is_not_one_of_the_types_is_refused_and_the_input_is_named() {
        let error = trigger(json!({ "inputs": [{ "name": "count", "type": "date" }] }))
            .expect_err("refused");
        assert!(error.contains("\"count\""), "{error}");
        assert!(error.contains("text"), "{error}");
    }

    #[test]
    fn a_choice_without_its_choices_is_refused_and_the_input_is_named() {
        let error = trigger(json!({ "inputs": [{ "name": "env", "type": "choice" }] }))
            .expect_err("refused");
        assert!(error.contains("\"env\""), "{error}");
        assert!(error.contains("choices"), "{error}");
    }

    #[test]
    fn choices_that_are_not_a_list_of_strings_are_refused_and_the_input_is_named() {
        for choices in [json!("main"), json!(["main", 3]), json!(["main", "main"]), json!([])].into_iter() {
            let error = trigger(json!({ "inputs": [{ "name": "env", "type": "choice", "choices": choices }] }))
                .expect_err("refused");
            assert!(error.contains("\"env\""), "{error}");
        }
    }

    #[test]
    fn choices_on_an_input_that_is_not_a_choice_are_refused() {
        let error =
            trigger(json!({ "inputs": [{ "name": "text", "type": "text", "choices": ["a"] }] }))
                .expect_err("refused");
        assert!(error.contains("\"text\""), "{error}");
        assert!(error.contains("choices"), "{error}");
    }

    #[test]
    fn a_key_that_is_not_one_of_the_inputs_fields_is_refused_and_the_input_is_named() {
        let error = trigger(json!({ "inputs": [{ "name": "count", "type": "number", "bogus": 1 }] }))
            .expect_err("refused");
        assert!(error.contains("\"count\""), "{error}");
        assert!(error.contains("bogus"), "{error}");
    }

    #[test]
    fn an_input_without_a_name_is_refused_at_its_place() {
        let error = trigger(json!({ "inputs": [{ "type": "text" }] })).expect_err("refused");
        assert!(error.contains("input 0"), "{error}");
    }

    #[test]
    fn a_default_that_is_not_the_type_of_its_input_is_refused_and_the_input_is_named() {
        let error = trigger(json!({ "inputs": [{ "name": "count", "type": "number", "default": "3" }] }))
            .expect_err("refused");
        assert!(error.contains("\"count\""), "{error}");
        assert!(error.contains("default"), "{error}");
    }

    #[test]
    fn a_default_that_is_not_one_of_the_choices_is_refused() {
        let error = trigger(json!({
            "inputs": [{ "name": "env", "type": "choice", "choices": ["main"], "default": "staging" }]
        }))
        .expect_err("refused");
        assert!(error.contains("\"env\""), "{error}");
        assert!(error.contains("default"), "{error}");
    }

    #[test]
    fn a_required_that_is_not_true_or_false_is_refused() {
        let error =
            trigger(json!({ "inputs": [{ "name": "text", "type": "text", "required": "yes" }] }))
                .expect_err("refused");
        assert!(error.contains("\"text\""), "{error}");
        assert!(error.contains("required"), "{error}");
    }

    #[test]
    fn a_twice_named_input_is_refused_and_named() {
        let error = trigger(json!({
            "inputs": [
                { "name": "version", "type": "text" },
                { "name": "version", "type": "text" }
            ]
        }))
        .expect_err("refused");
        assert!(error.contains("\"version\""), "{error}");
        assert!(error.contains("duplicate"), "{error}");
    }

    #[test]
    fn a_missing_required_input_is_refused_and_named() {
        let manual = parse(json!({ "inputs": [{ "name": "version", "type": "text", "required": true }] }));
        let error = manual.resolve(&Map::new()).expect_err("refused");
        assert!(error.contains("\"version\""), "{error}");
        assert!(error.contains("required"), "{error}");
    }

    #[test]
    fn a_default_stands_in_for_an_input_that_was_not_given() {
        let manual = parse(json!({
            "inputs": [
                { "name": "version", "type": "text", "required": true },
                { "name": "count", "type": "number", "default": 3 }
            ]
        }));
        let value = manual
            .resolve(&Map::from_iter([("version".to_string(), json!("1.0.0"))]))
            .expect("the default stands in");
        assert_eq!(value, json!({ "version": "1.0.0", "count": 3 }));
    }

    #[test]
    fn a_required_input_with_a_default_needs_no_value() {
        let manual = parse(json!({ "inputs": [{ "name": "count", "type": "number", "required": true, "default": 3 }] }));
        assert_eq!(manual.resolve(&Map::new()).expect("the default stands in"), json!({ "count": 3 }));
    }

    #[test]
    fn a_given_value_wins_over_a_default() {
        let manual = parse(json!({ "inputs": [{ "name": "count", "type": "number", "default": 3 }] }));
        let value = manual
            .resolve(&Map::from_iter([("count".to_string(), json!(7))]))
            .expect("a value of its type");
        assert_eq!(value, json!({ "count": 7 }));
    }

    #[test]
    fn an_input_that_was_not_given_and_has_no_default_is_left_out() {
        let manual = parse(json!({ "inputs": [{ "name": "note", "type": "text" }] }));
        assert_eq!(manual.resolve(&Map::new()).expect("no value for it"), json!({}));
    }

    #[test]
    fn an_input_the_trigger_does_not_give_is_refused_and_named() {
        let manual = parse(json!({ "inputs": [{ "name": "version", "type": "text" }] }));
        let given = Map::from_iter([("bogus".to_string(), json!("1.0.0"))]);
        let error = manual.resolve(&given).expect_err("refused");
        assert!(error.contains("\"bogus\""), "{error}");
    }

    #[test]
    fn a_trigger_leaves_its_value_by_out() {
        let manual = parse(json!({}));
        assert_eq!(manual.ports().output, &["out"]);
    }

    #[test]
    fn the_inputs_serialize_as_the_form_builds_them_from() {
        let manual = parse(json!({
            "inputs": [
                { "name": "version", "type": "text", "required": true },
                { "name": "env", "type": "choice", "choices": ["main", "staging"] }
            ]
        }));
        let inputs: Vec<Value> = manual
            .inputs
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .expect("plain values");
        assert_eq!(
            inputs.expect("plain values"),
            vec![
                json!({ "name": "version", "type": "text", "required": true }),
                json!({ "name": "env", "type": "choice", "required": false, "choices": ["main", "staging"] })
            ]
        );
    }
}
