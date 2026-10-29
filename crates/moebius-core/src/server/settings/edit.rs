//! A change applied to the text of `moebius.toml` with `toml_edit`: dotted keys set to JSON values, and keys or
//! tables removed, with the file's comments and order kept.

use serde_json::Value;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use super::{Changes, invalid};
use crate::app::Result;
use crate::config::valid_agent_name;

/// The parts of a dotted key, which are made of letters, digits, `-` and `_`.
fn parts(key: &str) -> Result<Vec<&str>> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.iter().all(|part| valid_agent_name(part)) {
        Ok(parts)
    } else {
        Err(invalid(format!(
            "{key:?} is not a key of the configuration"
        )))
    }
}

fn index(key: &str, part: &str) -> Result<usize> {
    part.parse()
        .map_err(|_| invalid(format!("{key}: {part:?} is not a position in the list")))
}

fn value_from_json(key: &str, value: &Value) -> Result<toml_edit::Value> {
    Ok(match value {
        Value::Bool(b) => (*b).into(),
        Value::String(s) => s.as_str().into(),
        Value::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => i.into(),
            (None, Some(f)) if n.is_f64() => f.into(),
            _ => return Err(invalid(format!("{key}: {n} is not a number TOML holds"))),
        },
        Value::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(value_from_json(key, item)?);
            }
            array.into()
        }
        Value::Null => {
            return Err(invalid(format!(
                "{key}: null is no value; remove the key instead"
            )));
        }
        Value::Object(_) => return Err(invalid(format!("{key}: a table is not allowed here"))),
    })
}

fn table_from_json(key: &str, fields: &serde_json::Map<String, Value>) -> Result<Table> {
    let mut table = Table::new();
    for (name, value) in fields {
        if !valid_agent_name(name) {
            return Err(invalid(format!("{key}: {name:?} is not a key")));
        }
        table.insert(name, item_from_json(&format!("{key}.{name}"), value)?);
    }
    Ok(table)
}

/// A value, a table or an array of tables for `value`.
fn item_from_json(key: &str, value: &Value) -> Result<Item> {
    match value {
        Value::Object(fields) => Ok(Item::Table(table_from_json(key, fields)?)),
        Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object) => {
            let mut tables = ArrayOfTables::new();
            for item in items {
                if let Value::Object(fields) = item {
                    tables.push(table_from_json(key, fields)?);
                }
            }
            Ok(Item::ArrayOfTables(tables))
        }
        other => Ok(Item::Value(value_from_json(key, other)?)),
    }
}

/// An empty table that shows no header of its own until it holds a key.
fn implicit() -> Table {
    let mut table = Table::new();
    table.set_implicit(true);
    table
}

/// Sets `name` of `table` to `new`, keeping the comments around what it replaces.
fn put(table: &mut Table, name: &str, new: Item) {
    match table.get_mut(name) {
        Some(old) => {
            let mut new = new;
            match (&*old, &mut new) {
                (Item::Value(before), Item::Value(after)) => {
                    *after.decor_mut() = before.decor().clone();
                }
                (Item::Table(before), Item::Table(after)) => {
                    *after.decor_mut() = before.decor().clone();
                    after.set_position(before.position());
                }
                _ => {}
            }
            *old = new;
        }
        None => {
            table.insert(name, new);
        }
    }
}

fn set_in(table: &mut Table, key: &str, path: &[&str], new: Item) -> Result<()> {
    let Some((first, rest)) = path.split_first() else {
        return Err(invalid("a key is needed"));
    };
    if rest.is_empty() {
        put(table, first, new);
        return Ok(());
    }
    if !table.contains_key(first) {
        // A position in the list of tables, such as `router.upstreams.0`, makes the list.
        let made = if index(key, rest[0]).is_ok() {
            Item::ArrayOfTables(ArrayOfTables::new())
        } else {
            Item::Table(implicit())
        };
        table.insert(first, made);
    }
    match table.get_mut(first) {
        Some(Item::Table(child)) => set_in(child, key, rest, new),
        Some(Item::ArrayOfTables(list)) => {
            let at = index(key, rest[0])?;
            if rest.len() > 1 {
                let child = list
                    .get_mut(at)
                    .ok_or_else(|| invalid(format!("{key}: there is no entry {at}")))?;
                return set_in(child, key, &rest[1..], new);
            }
            let Item::Table(entry) = new else {
                return Err(invalid(format!("{key}: an entry is a table")));
            };
            match at.cmp(&list.len()) {
                std::cmp::Ordering::Less => {
                    if let Some(old) = list.get_mut(at) {
                        *old = entry;
                    }
                }
                std::cmp::Ordering::Equal => list.push(entry),
                std::cmp::Ordering::Greater => {
                    return Err(invalid(format!(
                        "{key}: the list has {} entries, so the next is {}",
                        list.len(),
                        list.len()
                    )));
                }
            }
            Ok(())
        }
        _ => Err(invalid(format!("{key}: {first} is not a table"))),
    }
}

fn remove_in(table: &mut Table, key: &str, path: &[&str]) -> Result<()> {
    let Some((first, rest)) = path.split_first() else {
        return Ok(());
    };
    if rest.is_empty() {
        table.remove(first);
        return Ok(());
    }
    match table.get_mut(first) {
        Some(Item::Table(child)) => remove_in(child, key, rest),
        Some(Item::ArrayOfTables(list)) => {
            let at = index(key, rest[0])?;
            if at >= list.len() {
                return Ok(());
            }
            if rest.len() > 1 {
                if let Some(child) = list.get_mut(at) {
                    remove_in(child, key, &rest[1..])?;
                }
            } else {
                list.remove(at);
            }
            if list.is_empty() {
                table.remove(first);
            }
            Ok(())
        }
        Some(_) => Err(invalid(format!("{key}: {first} is not a table"))),
        None => Ok(()),
    }
}

/// Orders dotted keys so that a position in a list counts as a number: `router.upstreams.2` before
/// `router.upstreams.10`, which is what adding several entries in one change needs.
fn by_position(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (a.split('.'), b.split('.'));
    loop {
        match (a.next(), b.next()) {
            (Some(x), Some(y)) => {
                let order = match (x.parse::<usize>(), y.parse::<usize>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    _ => x.cmp(y),
                };
                if order.is_ne() {
                    return order;
                }
            }
            (x, y) => return x.is_some().cmp(&y.is_some()),
        }
    }
}

/// `text` with `changes` made to it. Comments and order stay; the removals come first, so that a table can be
/// replaced by removing and setting it.
pub fn edit(text: &str, changes: &Changes) -> Result<String> {
    let mut document: DocumentMut = text
        .parse()
        .map_err(|e| invalid(format!("moebius.toml cannot be edited: {e}")))?;
    for key in &changes.remove {
        remove_in(document.as_table_mut(), key, &parts(key)?)?;
    }
    let mut set: Vec<(&String, &Value)> = changes.set.iter().collect();
    set.sort_by(|(a, _), (b, _)| by_position(a, b));
    for (key, value) in set {
        let item = item_from_json(key, value)?;
        set_in(document.as_table_mut(), key, &parts(key)?, item)?;
    }
    Ok(document.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::json;

    use super::*;
    use crate::app::AppError;
    use crate::config::Config;

    fn changes(set: Value, remove: &[&str]) -> Changes {
        Changes {
            set: serde_json::from_value(set).unwrap(),
            remove: remove.iter().map(|key| key.to_string()).collect(),
        }
    }

    fn parse(text: &str) -> std::result::Result<Config, crate::config::ConfigError> {
        Config::parse(text, Path::new("/srv"), Path::new("/srv/moebius.toml"))
    }

    #[test]
    fn a_change_keeps_the_comments_and_the_order_of_the_file() {
        let text = "# The server.\nrun_limit = 3 # at a time\n\n# The board.\n[board]\ntick_ms = 2000\n# Rounds.\nround_limit = 5 # per card\n\n[checks]\ntimeout_seconds = 60\n";
        let edited = edit(
            text,
            &changes(json!({"board.round_limit": 9, "run_limit": 4}), &[]),
        )
        .unwrap();
        assert_eq!(
            edited,
            "# The server.\nrun_limit = 4 # at a time\n\n# The board.\n[board]\ntick_ms = 2000\n# Rounds.\nround_limit = 9 # per card\n\n[checks]\ntimeout_seconds = 60\n"
        );
    }

    #[test]
    fn the_example_stays_the_example_but_for_the_change() {
        let edited = edit(
            crate::config::EXAMPLE,
            &changes(json!({"board.round_limit": 7}), &[]),
        )
        .unwrap();
        assert_eq!(
            edited,
            crate::config::EXAMPLE.replace("round_limit = 5", "round_limit = 7")
        );
        assert_eq!(parse(&edited).unwrap().round_limit, 7);
    }

    #[test]
    fn keys_and_tables_that_are_not_in_the_file_are_made() {
        let edited = edit(
            "run_limit = 2\n\n[board]\ntick_ms = 100\n",
            &changes(
                json!({
                    "budgets.day_tokens": 5000,
                    "rules_file": "rules.md",
                    "teacher": {"model": "m", "timeout_seconds": 9},
                    "agents.Coder": {"model": "c", "skills": ["a"]},
                }),
                &[],
            ),
        )
        .unwrap();
        let config = parse(&edited).unwrap();
        assert_eq!(config.day_tokens, Some(5000));
        assert_eq!(config.rules_file, Some(PathBuf::from("/srv/rules.md")));
        assert_eq!(config.teacher.unwrap().model, "m");
        assert_eq!(config.agents["Coder"].model, "c");
        assert!(edited.starts_with("run_limit = 2\nrules_file = \"rules.md\"\n"));
        assert!(!edited.contains("[agents]"), "{edited}");
        assert!(edited.contains("[board]\ntick_ms = 100\n"));
    }

    #[test]
    fn a_removed_table_or_key_is_gone_and_one_that_is_not_there_is_ignored() {
        let text = "run_limit = 2\n\n[teacher]\nmodel = \"m\"\n\n[board]\ntick_ms = 100\nround_limit = 2\n\n[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"http://a\"\n\n[[router.upstreams]]\nprotocol = \"anthropic\"\nbase_url = \"http://b\"\n";
        let edited = edit(
            text,
            &changes(
                json!({}),
                &[
                    "teacher",
                    "board.tick_ms",
                    "router.upstreams.0",
                    "checks.timeout_seconds",
                    "github",
                ],
            ),
        )
        .unwrap();
        let config = parse(&edited).unwrap();
        assert!(config.teacher.is_none());
        assert_eq!(config.board_tick, std::time::Duration::from_millis(2000));
        assert_eq!(config.round_limit, 2);
        assert_eq!(config.upstreams.len(), 1);
        assert_eq!(config.upstreams[0].base_url, "http://b");
        assert!(!edited.contains("[teacher]"));
        let none = edit(&edited, &changes(json!({}), &["router.upstreams.0"])).unwrap();
        assert!(!none.contains("upstreams"), "{none}");
    }

    #[test]
    fn upstreams_are_set_by_position_or_whole() {
        let text = "[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"http://a\"\n";
        let edited = edit(
            text,
            &changes(
                json!({
                    "router.upstreams.1": {"protocol": "anthropic", "base_url": "http://b", "api_key_secret": "k"},
                    "router.upstreams.0": {"protocol": "openai", "base_url": "http://c"},
                }),
                &[],
            ),
        )
        .unwrap();
        let config = parse(&edited).unwrap();
        let urls: Vec<_> = config
            .upstreams
            .iter()
            .map(|u| u.base_url.as_str())
            .collect();
        assert_eq!(urls, ["http://c", "http://b"]);
        assert_eq!(config.upstreams[1].api_key_secret.as_deref(), Some("k"));
        let whole = edit(
            text,
            &changes(
                json!({"router.upstreams": [{"protocol": "anthropic", "base_url": "http://z"}]}),
                &[],
            ),
        )
        .unwrap();
        assert_eq!(parse(&whole).unwrap().upstreams[0].base_url, "http://z");
        assert!(
            edit(
                text,
                &changes(
                    json!({"router.upstreams.5": {"protocol": "openai", "base_url": "x"}}),
                    &[]
                )
            )
            .is_err()
        );
    }

    #[test]
    fn entries_added_in_one_change_are_added_in_order() {
        let entries: Vec<Value> = (0..12)
            .map(|n| json!({"protocol": "openai", "base_url": format!("http://h{n}")}))
            .collect();
        let set: serde_json::Map<String, Value> = entries
            .iter()
            .enumerate()
            .map(|(n, entry)| (format!("router.upstreams.{n}"), entry.clone()))
            .collect();
        let edited = edit("", &changes(Value::Object(set), &[])).unwrap();
        let urls: Vec<String> = parse(&edited)
            .unwrap()
            .upstreams
            .into_iter()
            .map(|u| u.base_url)
            .collect();
        assert_eq!(urls[0], "http://h0");
        assert_eq!(urls[11], "http://h11");
        assert_eq!(urls.len(), 12);
    }

    #[test]
    fn what_is_not_a_key_or_a_value_is_refused() {
        for bad in [
            changes(json!({"run limit": 1}), &[]),
            changes(json!({"a..b": 1}), &[]),
            changes(json!({"run_limit": null}), &[]),
            changes(json!({}), &["../x"]),
        ] {
            assert!(
                matches!(edit("", &bad), Err(AppError::Invalid(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_key_may_not_be_given_twice() {
        assert!(matches!(
            parse(
                "[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"x\"\napi_key_env = \"A\"\napi_key_secret = \"b\"\n"
            ),
            Err(crate::config::ConfigError::KeyTwice { .. })
        ));
        assert!(matches!(
            parse("[decision_model]\nurl = \"x\"\nmodel = \"m\"\napi_key_secret = \"a b\"\n"),
            Err(crate::config::ConfigError::KeySecret { .. })
        ));
    }
}
