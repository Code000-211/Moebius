//! The raw traces the Retro analyst reads besides the report: for each run the report names, its events as JSON Lines
//! and the diff of what it made, as files under `.retro/` of the analyst's worktree (`Runs::start_retro` writes them,
//! and git leaves them out). The summary of an error says what went wrong; the trace shows how, which is what makes a
//! proposal fit. Every secret is redacted (`blobs::redact`), and the files are bounded: at most `RUN_LIMIT` runs, each
//! file at most `FILE_LIMIT` bytes.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;

use super::Report;
use crate::app::App;
use crate::blobs::{redact, redact_json};
use crate::repos::git;

/// The directory of the worktree the traces are written to.
pub const DIR: &str = ".retro";

/// How many runs' traces are written; the report's first runs are taken.
const RUN_LIMIT: usize = 20;

/// How many bytes a trace file holds at most; the rest is left out with a line saying so.
const FILE_LIMIT: usize = 200_000;

/// The runs of the report, in the order of their first error, each once.
fn runs_of(report: &Report) -> Vec<i64> {
    let mut runs = Vec::new();
    for run in report.errors.iter().filter_map(|e| e.run) {
        if !runs.contains(&run) {
            runs.push(run);
        }
    }
    runs
}

/// The events of `run`, one JSON object a line (`seq`, `kind`, `data`, `at`), redacted and cut at `FILE_LIMIT`.
fn events(app: &App, run: i64) -> String {
    let events = app.store.events(run).unwrap_or_default();
    let mut text = String::new();
    for (i, event) in events.iter().enumerate() {
        let line = json!({
            "seq": event.seq, "kind": event.kind, "data": redact_json(&event.data), "at": event.at,
        })
        .to_string();
        if text.len() + line.len() >= FILE_LIMIT {
            let left = events.len() - i;
            text += &json!({ "kind": "moebius_cut", "data": { "left_out": left } }).to_string();
            text.push('\n');
            break;
        }
        text += &line;
        text.push('\n');
    }
    text
}

/// What the run of a card changed, from the commit it started at to its branch, redacted and cut at `FILE_LIMIT`; none
/// when the run has no card, its branch is gone, or it changed nothing.
fn diff(app: &App, run: i64) -> Option<String> {
    let run = app.store.run(run).ok()??;
    let card = app.store.card(run.card_id?).ok()??;
    let repository = app.store.repository(card.repository_id).ok()??;
    let base = app
        .store
        .run_base(run.id)
        .ok()
        .flatten()
        .unwrap_or_else(|| repository.base_ref());
    let diff = git(Path::new(&repository.path), &["diff", &base, &run.branch()]).ok()?;
    if diff.trim().is_empty() {
        return None;
    }
    let mut diff = redact(&diff);
    if diff.len() > FILE_LIMIT {
        let mut end = FILE_LIMIT;
        while !diff.is_char_boundary(end) {
            end -= 1;
        }
        diff.truncate(end);
        diff += "\n… the rest of the diff is left out\n";
    }
    Some(diff)
}

/// The trace files of the report's runs, by their path in the worktree.
pub fn files(app: &App, report: &Report) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for run in runs_of(report).into_iter().take(RUN_LIMIT) {
        files.insert(format!("{DIR}/run-{run}.events.jsonl"), events(app, run));
        if let Some(diff) = diff(app, run) {
            files.insert(format!("{DIR}/run-{run}.diff"), diff);
        }
    }
    files
}

/// What the analyst is told about the trace files.
pub fn brief(report: &Report, files: &BTreeMap<String, String>) -> String {
    if files.is_empty() {
        return String::new();
    }
    let runs = runs_of(report).len();
    let left_out = if runs > RUN_LIMIT {
        format!(" Only the first {RUN_LIMIT} of the {runs} runs are there.")
    } else {
        String::new()
    };
    let list: Vec<&str> = files.keys().map(String::as_str).collect();
    format!(
        "The raw traces of the runs the report names are in {DIR}/ of your worktree: run-<id>.events.jsonl holds a \
         run's events in order (what the agent said, its tool calls with their checks and results, and the errors), \
         and run-<id>.diff what the run of a card changed, when its branch is still there. Read them before you \
         propose: the report only summarises what went wrong, the traces show how. Secrets in them are redacted. They \
         are not harness configuration: never commit them.{left_out}\n\nThe trace files: {}\n",
        list.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::improve::retro::ReportError;

    fn error(run: Option<i64>) -> ReportError {
        ReportError {
            kind: "run_failed".into(),
            run,
            card: None,
            agent: None,
            text: String::new(),
        }
    }

    #[test]
    fn each_run_of_the_report_is_named_once_in_the_order_of_its_first_error() {
        let report = Report {
            date: "2026-03-10".into(),
            errors: vec![error(Some(3)), error(None), error(Some(1)), error(Some(3))],
            merged: vec![],
        };
        assert_eq!(runs_of(&report), [3, 1]);
        let files = BTreeMap::from([(format!("{DIR}/run-3.events.jsonl"), String::new())]);
        let told = brief(&report, &files);
        assert!(told.contains(".retro/run-3.events.jsonl"), "{told}");
        assert!(!told.contains("Only the first"), "{told}");
        assert_eq!(brief(&report, &BTreeMap::new()), "");
    }
}
