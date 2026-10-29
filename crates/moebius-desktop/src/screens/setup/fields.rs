//! The keys of `moebius.toml` the pages edit, by box, and one key as an input: what it holds, what it held when read,
//! and the value a save sends.

use serde_json::{Map, Value, json};

use crate::client::{AvailableModel, ServerSettings};
use crate::widgets::form::PROTOCOLS;

/// How a key of the configuration is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Text,
    /// A model's name: a dropdown of the models the upstreams serve, or an input for another one.
    Model,
    /// A path, a URL or an address, set in the code font.
    Code,
    Whole,
    Decimal,
    /// Names separated by commas.
    List,
    Pick(&'static [&'static str]),
}

/// The keys of the Settings page by box: the dotted key, its label and how it is typed.
/// A key of the configuration as a form shows it: the dotted key, its label and how it is typed.
pub(super) type Spec = (&'static str, &'static str, Kind);

pub(super) const GROUPS: &[(&str, &[Spec])] = &[
    (
        "Server",
        &[
            ("data_dir", "Data directory", Kind::Code),
            ("listen", "Desktop API listens on", Kind::Code),
            ("token_file", "Token file", Kind::Code),
            ("router.listen", "Router listens on", Kind::Code),
            ("run_limit", "Runs at a time", Kind::Whole),
        ],
    ),
    (
        "Board and budgets",
        &[
            ("board.tick_ms", "Board tick (ms)", Kind::Whole),
            ("board.round_limit", "Coding rounds a card", Kind::Whole),
            ("budgets.day_tokens", "Tokens a day", Kind::Whole),
            (
                "checks.timeout_seconds",
                "Checks time limit (s)",
                Kind::Whole,
            ),
        ],
    ),
    (
        "Nightly schedule",
        &[
            ("retro_at", "Retro at (HH:MM)", Kind::Text),
            ("trial_window.start", "Trial window starts", Kind::Text),
            ("trial_window.end", "Trial window ends", Kind::Text),
            ("regression.size", "Regression cards", Kind::Whole),
            ("regression.gate", "Regression gate", Kind::Pick(&YES_NO)),
        ],
    ),
    (
        "Harness",
        &[
            ("harness.url", "Repository URL", Kind::Code),
            ("harness.mcp_env", "MCP variables", Kind::List),
            ("rules_file", "Rules file", Kind::Code),
        ],
    ),
    ("Pi-Bolt", &[("pi_bolt.program", "Program", Kind::Code)]),
    (
        "GitHub",
        &[
            ("github.app_id", "App id", Kind::Whole),
            ("github.private_key_file", "Private key file", Kind::Code),
            ("github.api_url", "API URL", Kind::Code),
            ("github.poll_seconds", "Poll every (s)", Kind::Whole),
        ],
    ),
    (
        "GitLab",
        &[
            ("gitlab.url", "URL", Kind::Code),
            ("gitlab.token_file", "Token file", Kind::Code),
            ("gitlab.token_secret", "Token secret", Kind::Code),
            ("gitlab.poll_seconds", "Poll every (s)", Kind::Whole),
        ],
    ),
];

/// The tables of the file that are there or not: when every key of one is blank the table is removed.
pub(super) const TABLES: &[(&str, &[&str])] = &[
    (
        "github",
        &[
            "github.app_id",
            "github.private_key_file",
            "github.api_url",
            "github.poll_seconds",
        ],
    ),
    (
        "gitlab",
        &[
            "gitlab.url",
            "gitlab.token_file",
            "gitlab.token_secret",
            "gitlab.poll_seconds",
        ],
    ),
    ("harness", &["harness.url", "harness.mcp_env"]),
];

pub(super) const TEACHER: &[Spec] = &[
    ("teacher.model", "Model", Kind::Model),
    ("teacher.protocol", "Protocol", Kind::Pick(&PROTOCOLS)),
    ("teacher.timeout_seconds", "Time limit (s)", Kind::Whole),
];

pub(super) const DECISION_MODEL: &[Spec] = &[
    ("decision_model.url", "URL", Kind::Code),
    ("decision_model.model", "Model", Kind::Model),
    (
        "decision_model.timeout_seconds",
        "Time limit (s)",
        Kind::Whole,
    ),
    ("decision_model.shadow", "Kinds to shadow", Kind::List),
    (
        "decision_model.agreement",
        "Agreement needed",
        Kind::Decimal,
    ),
    (
        "decision_model.min_judgements",
        "Judgements needed",
        Kind::Whole,
    ),
];

/// The choices of a key that is true or false, sent as a JSON boolean.
const YES_NO: [&str; 2] = ["true", "false"];

/// One key of the configuration as an input.
#[derive(Debug, Clone)]
pub(super) struct Field {
    pub(super) key: &'static str,
    pub(super) label: &'static str,
    pub(super) kind: Kind,
    /// What the input holds; blank when the file does not set the key.
    pub(super) text: String,
    /// What it held when the settings were read.
    pub(super) original: String,
    /// What the server uses when the file does not set the key.
    pub(super) default: Option<String>,
    /// What a Model field offers.
    pub(super) models: Vec<AvailableModel>,
    /// Whether a Model field is an input rather than the list.
    pub(super) typing: bool,
}

/// A value as the text of an input.
fn text_of(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(text_of).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

impl Field {
    pub(super) fn read(spec: &(&'static str, &'static str, Kind), server: &ServerSettings) -> Self {
        let (key, label, kind) = *spec;
        let shown = text_of(server.value(key));
        let set = server.is_set(key);
        // A picker always shows a choice, so it holds the default when the file does not set the key.
        let text = if set || matches!(kind, Kind::Pick(_)) {
            shown.clone()
        } else {
            String::new()
        };
        Field {
            key,
            label,
            kind,
            original: text.clone(),
            text,
            default: (!set && !shown.is_empty()).then_some(shown),
            models: vec![],
            typing: false,
        }
    }

    /// The new value of the key, none when the input is blank; or what is wrong with the input.
    pub(super) fn value(&self) -> Result<Option<Value>, String> {
        let text = self.text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        let wrong = |what: &str| format!("{} is {what}.", self.label);
        Ok(Some(match self.kind {
            Kind::Pick(options) if options == YES_NO => json!(text == "true"),
            Kind::Text | Kind::Code | Kind::Model | Kind::Pick(_) => json!(text),
            Kind::Whole => json!(text.parse::<i64>().map_err(|_| wrong("a whole number"))?),
            Kind::Decimal => json!(text.parse::<f64>().map_err(|_| wrong("a number"))?),
            Kind::List => json!(
                text.split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .collect::<Vec<_>>()
            ),
        }))
    }

    pub(super) fn changed(&self) -> bool {
        self.text.trim() != self.original.trim()
    }

    /// Whether the file set this key when it was read.
    pub(super) fn was_set(&self) -> bool {
        !self.original.trim().is_empty()
    }
}

/// The inputs of some keys.
#[derive(Debug, Clone, Default)]
pub(super) struct Form {
    pub(super) fields: Vec<Field>,
}

impl Form {
    pub(super) fn read(specs: &[Spec], server: &ServerSettings) -> Self {
        Form {
            fields: specs.iter().map(|spec| Field::read(spec, server)).collect(),
        }
    }

    pub(super) fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.key == key)
    }

    /// Adds the keys that were changed to `set` and `remove`.
    pub(super) fn changes(
        &self,
        set: &mut Map<String, Value>,
        remove: &mut Vec<String>,
    ) -> Result<(), String> {
        for field in self.fields.iter().filter(|field| field.changed()) {
            match field.value()? {
                Some(value) => {
                    set.insert(field.key.to_string(), value);
                }
                None => remove.push(field.key.to_string()),
            }
        }
        Ok(())
    }

    /// Every key as the input shows it, for a table that is new.
    pub(super) fn all(&self, set: &mut Map<String, Value>) -> Result<(), String> {
        for field in &self.fields {
            if let Some(value) = field.value()? {
                set.insert(field.key.to_string(), value);
            }
        }
        Ok(())
    }

    pub(super) fn dirty(&self) -> bool {
        self.fields.iter().any(Field::changed)
    }
}
