//! The rules the guard decides by: `rules` writes the built-in patterns, the rules in prose with where they come from,
//! each repository's own rules and the recent refusals; `rules save` replaces the rules in prose.

use clap::Subcommand;
use serde_json::{Value, json};

use super::{Plan, Request, Show, cell, read_input};

#[derive(Debug, Subcommand)]
pub enum RulesCommand {
    /// Replace the rules in prose with the content of a file, or `-` for standard input.
    Save { file: String },
}

/// `rules` and `rules save`.
pub fn rules(command: Option<&RulesCommand>) -> Plan {
    let request = match command {
        None => Request::get("/v1/rules"),
        Some(RulesCommand::Save { file }) => {
            Request::put("/v1/rules", json!({ "text": read_input(file)? }))
        }
    };
    Ok((request, Show::Rules))
}

/// The rules page as plain text.
pub fn show_rules(view: &Value) -> String {
    let indented = |text: &Value| {
        text.as_str()
            .unwrap_or("")
            .lines()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
    };
    let mut out = vec!["built in:".to_string()];
    for pattern in view["patterns"].as_array().into_iter().flatten() {
        out.push(format!(
            "  {}  {}",
            cell(&pattern["name"]),
            cell(&pattern["description"])
        ));
    }
    let prose = &view["prose"];
    out.push(format!(
        "in prose (from {}, {}):",
        cell(&prose["source"]),
        if prose["editable"] == true {
            "editable"
        } else {
            "read only"
        }
    ));
    if let Some(error) = prose["error"].as_str() {
        out.push(format!("  error: {error}"));
    }
    out.extend(indented(&prose["text"]));
    // A repository without rules of its own is left out.
    let repositories = view["repositories"].as_array().into_iter().flatten();
    for repository in repositories.filter(|r| r["text"].as_str().is_some_and(|t| !t.is_empty())) {
        out.push(format!("in {}:", cell(&repository["repository"])));
        out.extend(indented(&repository["text"]));
    }
    if let Some(refusals) = view["refusals"].as_object().filter(|r| !r.is_empty()) {
        out.push("refusals in the last 7 days:".into());
        out.extend(refusals.iter().map(|(rule, n)| format!("  {rule}: {n}")));
    }
    if let Some(commit) = view["commit"].as_str() {
        out.push(format!("committed {commit}"));
    }
    out.join("\n")
}
