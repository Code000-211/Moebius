//! The reference documentation covers the code: every route, script function, `card` field, configuration key and event
//! kind appears in `docs/reference/`, and `AGENTS.md` mirrors `CLAUDE.md`. Each test scans the source for the names and
//! checks that the page mentions each one, so a change to a contract fails here until its reference is updated.

use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> String {
    let file = repository_root().join(path);
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

/// The source of a module without its tests: one file, or every `.rs` file in a folder and the folders in it.
fn source(path: &str) -> String {
    let full = repository_root().join(path);
    if full.is_dir() {
        let mut entries: Vec<String> = std::fs::read_dir(&full)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        return entries
            .iter()
            .filter(|name| name.ends_with(".rs") || full.join(name).is_dir())
            .map(|name| source(&format!("{path}/{name}")))
            .collect();
    }
    let text = read(path);
    match text.split_once("#[cfg(test)]\nmod tests") {
        Some((code, _)) => code.to_string(),
        None => text,
    }
}

/// Fails, naming `page`, for every name in `names` that the page does not contain.
fn assert_documented(page: &str, names: &[String]) {
    let text = read(page);
    let missing: Vec<&String> = names.iter().filter(|name| !text.contains(*name)).collect();
    assert!(missing.is_empty(), "{page} does not mention {missing:?}");
}

/// The strings that follow each occurrence of `before` in `code`, up to the closing quote. Whitespace in `code` is
/// ignored, so a call split over lines is found.
fn quoted_after(code: &str, before: &str) -> Vec<String> {
    let squeezed: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let before = format!("{before}\"");
    squeezed
        .split(&before)
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_string)
        .collect()
}

#[test]
fn every_route_is_in_the_reference() {
    for (module, page) in [
        ("server/api", "docs/reference/desktop-api.md"),
        ("repos/check.rs", "docs/reference/bridge-protocol.md"),
        ("router", "docs/reference/router.md"),
    ] {
        let routes = quoted_after(
            &source(&format!("crates/moebius-core/src/{module}")),
            ".route(",
        );
        assert!(!routes.is_empty(), "no routes found in {module}");
        assert_documented(page, &routes);
    }
}

#[test]
fn every_script_function_constant_and_card_field_is_in_the_reference() {
    let board = source("crates/moebius-core/src/board");
    // `m` is the `moebius` table, `t` the `card` table and `r` its `run` table.
    let moebius: Vec<String> = quoted_after(&board, "m.set(")
        .into_iter()
        .map(|name| format!("moebius.{name}"))
        .collect();
    let card: Vec<String> = quoted_after(&board, "t.set(")
        .into_iter()
        .map(|name| format!("card.{name}"))
        .collect();
    let run: Vec<String> = quoted_after(&board, "r.set(")
        .into_iter()
        .map(|name| format!("card.run.{name}"))
        .collect();
    assert!(moebius.len() >= 10 && card.len() >= 10 && run.len() >= 4);
    let page = "docs/reference/column-scripts.md";
    assert_documented(page, &moebius);
    assert_documented(page, &card);
    assert_documented(page, &run);
}

#[test]
fn every_default_script_is_in_the_reference() {
    let mut scripts = Vec::new();
    for entry in std::fs::read_dir(repository_root().join("crates/moebius-core/workflow")).unwrap()
    {
        scripts.push(entry.unwrap().file_name().to_string_lossy().into_owned());
    }
    assert_eq!(scripts.len(), 6);
    assert_documented("docs/reference/column-scripts.md", &scripts);
}

/// The names of the keys in the configuration example, commented out or not, as the reference writes them: a key of a
/// table as `table.key`, a key of an agent as `agents.<name>.key`.
fn configuration_keys() -> Vec<String> {
    let mut keys = Vec::new();
    let mut table = String::new();
    for line in moebius_core::config::EXAMPLE.lines() {
        let line = line.trim_start_matches('#').trim();
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = header.trim_matches(['[', ']']).to_string();
            if table.starts_with("agents.") {
                table = "agents.<name>".to_string();
            }
        } else if let Some((key, _)) = line.split_once(" = ")
            && !key.is_empty()
            && key.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        {
            keys.push(if table.is_empty() {
                key.to_string()
            } else {
                format!("{table}.{key}")
            });
        }
    }
    keys
}

#[test]
fn every_configuration_key_is_in_the_reference() {
    let keys = configuration_keys();
    // A few the scan must find, so that a change to the example's layout is noticed here.
    for expected in [
        "data_dir",
        "rules_file",
        "trial_window.start",
        "board.tick_ms",
        "router.upstreams.api_key_env",
        "teacher.timeout_seconds",
        "github.poll_seconds",
        "agents.<name>.token_budget",
    ] {
        assert!(
            keys.iter().any(|key| key == expected),
            "{expected} in {keys:?}"
        );
    }
    assert_documented("docs/reference/configuration.md", &keys);
}

#[test]
fn every_event_kind_is_in_the_reference() {
    let mut kinds = Vec::new();
    kinds.extend(quoted_after(
        &source("crates/moebius-core/src"),
        "AppEvent::new(",
    ));
    kinds.sort();
    kinds.dedup();
    assert!(kinds.len() >= 14, "{kinds:?}");
    let kinds: Vec<String> = kinds.iter().map(|kind| format!("`{kind}`")).collect();
    assert_documented("docs/reference/desktop-api.md", &kinds);
}

#[test]
fn agents_md_mirrors_claude_md() {
    let agents = read("AGENTS.md");
    let (first, rest) = agents.split_once('\n').expect("AGENTS.md has a first line");
    assert!(first.contains("mirrors CLAUDE.md"), "{first}");
    assert_eq!(
        rest,
        read("CLAUDE.md"),
        "AGENTS.md differs from CLAUDE.md below its first line"
    );
}

#[test]
fn the_documentation_the_readme_and_claude_md_point_to_exists() {
    for path in [
        "README.md",
        "docs/architecture.md",
        "docs/reference/column-scripts.md",
        "docs/reference/rules.md",
        "docs/reference/configuration.md",
        "docs/reference/harness-configuration.md",
        "docs/reference/plugin-api.md",
        "docs/reference/bridge-protocol.md",
        "docs/reference/router.md",
        "docs/reference/desktop-api.md",
        "docs/guide/README.md",
        "examples/README.md",
        "examples/rules.md",
        "examples/scripts/ready-large-cards-alone.luau",
        "examples/pi-extension/log-tool-calls.ts",
        "crates/moebius-plugin-api/examples/allow_reads.rs",
    ] {
        assert!(repository_root().join(path).is_file(), "{path} is missing");
    }
}
