//! Every key of the configuration as the reference names it, with its default and whether a change applies to the
//! running server, and the value of each in the file, without the passwords of its URLs.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::server::setup::without_credentials;

/// A key of the configuration as the reference names it, with its default (JSON text, empty for none) and whether a
/// change applies to the running server.
struct Key {
    name: &'static str,
    default: &'static str,
    live: bool,
}

const fn key(name: &'static str, default: &'static str, live: bool) -> Key {
    Key {
        name,
        default,
        live,
    }
}

/// Every key of `docs/reference/configuration.md`, in its order. `router.upstreams`, `plugins` and `agents` stand for
/// the whole array of tables and the whole table of agents.
const KEYS: &[Key] = &[
    key("data_dir", "\".moebius\"", false),
    key("listen", "\"127.0.0.1:7171\"", false),
    key("token_file", "\"token\"", false),
    key("run_limit", "3", true),
    key("retro_at", "\"23:30\"", true),
    key("rules_file", "\"rules.md\"", false),
    key("harness.mcp_env", "[]", false),
    key("harness.url", "", false),
    key("trial_window.start", "\"00:00\"", true),
    key("trial_window.end", "\"06:00\"", true),
    key("board.tick_ms", "2000", true),
    key("board.round_limit", "5", true),
    key("budgets.day_tokens", "", true),
    key("checks.timeout_seconds", "1800", false),
    key("router.listen", "\"127.0.0.1:7172\"", false),
    key("router.upstreams", "[]", true),
    key("pi_bolt.program", "\"pi-bolt\"", false),
    key("teacher.model", "", false),
    key("teacher.protocol", "\"openai\"", false),
    key("teacher.timeout_seconds", "20", false),
    key("decision_model.url", "", false),
    key("decision_model.model", "", false),
    key("decision_model.api_key_env", "", false),
    key("decision_model.api_key_secret", "", false),
    key("decision_model.timeout_seconds", "20", false),
    key("decision_model.shadow", "", false),
    key("decision_model.agreement", "0.95", false),
    key("decision_model.min_judgements", "100", false),
    key("plugins", "[]", false),
    key("github.app_id", "", false),
    key("github.private_key_file", "", false),
    key("github.api_url", "\"https://api.github.com\"", false),
    key("github.poll_seconds", "30", false),
    key("agents", "{}", true),
];

/// The keys whose value is, or holds, a URL that may have a user name and password in it.
const URLS: &[&str] = &["harness.url", "decision_model.url", "router.upstreams"];

/// The value at the dotted `name` of the parsed file.
fn lookup<'a>(raw: &'a toml::Table, name: &str) -> Option<&'a toml::Value> {
    let (first, rest) = name.split_once('.').unwrap_or((name, ""));
    let value = raw.get(first)?;
    if rest.is_empty() {
        return Some(value);
    }
    lookup(value.as_table()?, rest)
}

fn default_of(key: &Key) -> Value {
    if key.default.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(key.default).unwrap_or(Value::Null)
    }
}

/// The value of `key` in the file, or its default.
fn effective(raw: &toml::Table, key: &Key) -> Value {
    match lookup(raw, key.name).map(serde_json::to_value) {
        Some(Ok(value)) => value,
        _ => default_of(key),
    }
}

/// The keys the running server uses only after a restart whose value in `raw` is not the one it started with.
pub(super) fn restart_keys(started: &toml::Table, raw: &toml::Table) -> Vec<String> {
    KEYS.iter()
        .filter(|key| !key.live && effective(started, key) != effective(raw, key))
        .map(|key| key.name.to_string())
        .collect()
}

/// Strips the user name and password from the URLs of `value`.
pub(super) fn without_passwords(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(without_credentials(text)),
        Value::Array(items) => Value::Array(items.iter().map(without_passwords).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), without_passwords(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Every key of the reference with its value in the file, or its default: `{"value": ..., "default": bool}`.
/// Secrets are not in the file, so they cannot be here; a URL has no user name and password.
pub fn values(raw: &toml::Table) -> BTreeMap<String, Value> {
    KEYS.iter()
        .map(|key| {
            let mut value = effective(raw, key);
            if URLS.contains(&key.name) {
                value = without_passwords(&value);
            }
            let default = lookup(raw, key.name).is_none();
            (
                key.name.to_string(),
                json!({ "value": value, "default": default }),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_live_key_is_read_and_every_other_key_is_told_to_need_a_restart() {
        let started: toml::Table =
            toml::from_str("run_limit = 2\n[teacher]\nmodel = \"a\"\n").unwrap();
        let raw: toml::Table = toml::from_str(
            "run_limit = 9\nlisten = \"127.0.0.1:7171\"\n[teacher]\nmodel = \"b\"\n[board]\nround_limit = 1\n[github]\napp_id = 1\n",
        )
        .unwrap();
        // `listen` is the default as written, so it did not change; `run_limit` and `board` are live.
        assert_eq!(
            restart_keys(&started, &raw),
            ["teacher.model", "github.app_id"]
        );
        for key in KEYS {
            assert!(key.default.is_empty() || serde_json::from_str::<Value>(key.default).is_ok());
        }
    }

    #[test]
    fn the_values_tell_the_file_and_the_defaults_and_hold_no_password() {
        let raw: toml::Table = toml::from_str(
            "run_limit = 8\n[harness]\nurl = \"https://u:hunter2@example.com/h.git\"\n[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"http://a:hunter2@127.0.0.1:1\"\napi_key_secret = \"k\"\n",
        )
        .unwrap();
        let values = values(&raw);
        assert_eq!(values["run_limit"], json!({"value": 8, "default": false}));
        assert_eq!(
            values["board.round_limit"],
            json!({"value": 5, "default": true})
        );
        assert_eq!(
            values["budgets.day_tokens"],
            json!({"value": null, "default": true})
        );
        assert_eq!(values["harness.url"]["value"], "https://example.com/h.git");
        assert_eq!(
            values["router.upstreams"]["value"][0]["base_url"],
            "http://127.0.0.1:1"
        );
        assert_eq!(
            values["router.upstreams"]["value"][0]["api_key_secret"],
            "k"
        );
        assert!(!serde_json::to_string(&values).unwrap().contains("hunter2"));
        assert_eq!(values.len(), KEYS.len());
    }
}
