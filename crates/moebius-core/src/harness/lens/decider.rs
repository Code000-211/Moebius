//! The first decider of the lens's two decision points, a compiled-in plugin named `lens`.
//!
//! - `lint_report` (options `show`, `hide`): which of the new findings of an edit the agent sees. An error is always
//!   shown; a warning is shown when it is on a line the edit touched and fewer than `max_lines` findings of the
//!   edit are shown already; anything else is hidden.
//! - `deep_check` (options `run`, `skip`): whether the slow type check runs when the agent is done. It runs when a file
//!   of code was touched, other than by an edit that only changed comments; edits to documentation and configuration
//!   alone skip it.
//!
//! Both answer from the question's context alone, as every decider does, so that the decision model can be trained on
//! the same judgements and take them over. A question that lacks what the rule reads is an error, which `decide` turns
//! into the safe option (`show`, `run`).

use std::collections::BTreeMap;

use moebius_plugin_api::{Call, DecideRequest, Decision, Kind, Manifest, Plugin, PluginError};
use serde_json::Value;

/// The plugin's name, which `Decisions` routes the lens's kinds to.
pub const NAME: &str = "lens";

/// What kind of file a path is, by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Code,
    Docs,
    Config,
}

/// Documentation by extension.
const DOCS: [&str; 7] = ["md", "markdown", "txt", "rst", "adoc", "org", "rdoc"];
/// Configuration and data by extension.
const CONFIG: [&str; 14] = [
    "toml",
    "json",
    "jsonc",
    "yaml",
    "yml",
    "ini",
    "cfg",
    "conf",
    "lock",
    "env",
    "properties",
    "xml",
    "csv",
    "gitignore",
];

/// Whether `path` is code, documentation or configuration. A name that is not known is code, since the check runs
/// when in doubt.
pub fn file_kind(path: &str) -> FileKind {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match extension.as_deref() {
        Some(e) if DOCS.contains(&e) => FileKind::Docs,
        Some(e) if CONFIG.contains(&e) => FileKind::Config,
        None if matches!(name, "LICENSE" | "README" | "CHANGELOG" | "NOTICE") => FileKind::Docs,
        _ => FileKind::Code,
    }
}

fn decision(choice: &str, reason: impl Into<String>) -> Value {
    serde_json::to_value(Decision {
        choice: choice.into(),
        reason: reason.into(),
        by: Some("rules".into()),
    })
    .expect("a decision is JSON")
}

fn field<'a>(context: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, PluginError> {
    context
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| PluginError::BadParams(format!("the context has no {name:?}")))
}

fn number(context: &BTreeMap<String, String>, name: &str) -> Result<usize, PluginError> {
    field(context, name)?
        .parse()
        .map_err(|_| PluginError::BadParams(format!("{name:?} is not a number")))
}

fn lint_report(context: &BTreeMap<String, String>) -> Result<Value, PluginError> {
    let touched = field(context, "touched")? == "true";
    let shown = number(context, "shown_so_far")?;
    let max = number(context, "max_lines")?;
    Ok(match field(context, "severity")? {
        "error" => decision("show", "an error is always shown"),
        _ if !touched => decision("hide", "a warning off the lines the edit touched"),
        _ if shown >= max => decision("hide", format!("{max} findings are shown already")),
        _ => decision("show", "a warning on a line the edit touched"),
    })
}

fn deep_check(context: &BTreeMap<String, String>) -> Result<Value, PluginError> {
    let code = field(context, "code_files")?;
    Ok(if code.trim().is_empty() {
        decision(
            "skip",
            "only documentation, configuration or comments were edited",
        )
    } else {
        decision("run", "a file of code was edited")
    })
}

/// The compiled-in decider of the lens's kinds.
pub struct Lens;

impl Plugin for Lens {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: NAME.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kinds: vec![Kind::Decider],
        }
    }

    fn call(&self, call: Call) -> Result<Value, PluginError> {
        let request: DecideRequest = serde_json::from_value(call.params)
            .map_err(|e| PluginError::BadParams(e.to_string()))?;
        match request.kind.as_str() {
            "lint_report" => lint_report(&request.context),
            "deep_check" => deep_check(&request.context),
            other => Err(PluginError::Failed(format!(
                "the lens decides lint_report and deep_check, not {other:?}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::*;

    fn ask(kind: &str, pairs: &[(&str, &str)]) -> Result<Decision, String> {
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Lens);
        let options = if kind == "lint_report" {
            ["show", "hide"]
        } else {
            ["run", "skip"]
        };
        host.decide(
            NAME,
            &DecideRequest {
                kind: kind.into(),
                context: pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                options: options.map(String::from).to_vec(),
            },
        )
        .map_err(|e| e.to_string())
    }

    fn lint(severity: &str, touched: &str, shown: &str) -> String {
        ask(
            "lint_report",
            &[
                ("severity", severity),
                ("touched", touched),
                ("shown_so_far", shown),
                ("max_lines", "2"),
            ],
        )
        .unwrap()
        .choice
    }

    #[test]
    fn errors_are_always_shown_and_warnings_only_on_touched_lines_up_to_the_limit() {
        assert_eq!(lint("error", "false", "9"), "show");
        assert_eq!(lint("warning", "true", "0"), "show");
        assert_eq!(lint("warning", "true", "1"), "show");
        assert_eq!(lint("warning", "true", "2"), "hide");
        assert_eq!(lint("warning", "false", "0"), "hide");
        let hidden = ask(
            "lint_report",
            &[
                ("severity", "warning"),
                ("touched", "false"),
                ("shown_so_far", "0"),
                ("max_lines", "2"),
            ],
        )
        .unwrap();
        assert_eq!(hidden.by.as_deref(), Some("rules"));
    }

    #[test]
    fn a_question_without_what_the_rule_reads_is_an_error() {
        assert!(ask("lint_report", &[("severity", "error")]).is_err());
        assert!(ask("deep_check", &[]).is_err());
    }

    #[test]
    fn the_deep_check_runs_when_code_was_touched() {
        let code = |files: &str| ask("deep_check", &[("code_files", files)]).unwrap().choice;
        assert_eq!(code("src/a.rs"), "run");
        assert_eq!(code(""), "skip");
    }

    #[test]
    fn files_are_code_unless_they_are_documentation_or_configuration() {
        use FileKind::*;
        for (path, kind) in [
            ("src/a.rs", Code),
            ("a.py", Code),
            ("Makefile", Code),
            ("script.sh", Code),
            ("README.md", Docs),
            ("docs/guide.RST", Docs),
            ("LICENSE", Docs),
            ("Cargo.toml", Config),
            ("deploy/app.yaml", Config),
            ("package.json", Config),
            ("Cargo.lock", Config),
        ] {
            assert_eq!(file_kind(path), kind, "{path}");
        }
    }
}
