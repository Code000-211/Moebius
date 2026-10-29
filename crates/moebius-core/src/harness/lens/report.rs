//! The lens's endpoints on the router's listener, which the extension calls with the run's token (see `mod`): what it
//! found after an edit (`/moebius/lens/report`), whether to run the slow check (`/moebius/lens/deep`) and what that
//! found (`/moebius/lens/deep-result`). The linters themselves run in the extension; here everything that is a
//! decision goes through `decide`, and the time each check took is stored as `lens_check` run events.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use axum::Json;
use axum::Router as Routes;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::decider::{FileKind, file_kind};
use crate::app::App;
use crate::decide::Question;
use crate::decide::check::{parse, unauthorized};
use crate::router::{Grant, Router};

/// The most findings of one edit that are put to the decision point; the rest are not shown.
const DECIDED_MAX: usize = 40;
/// The most lines of the edit's new text that go into a question as the edit.
const EXCERPT_LINES: usize = 40;

#[derive(Clone)]
struct Served {
    app: Arc<App>,
    router: Arc<Router>,
}

/// The lens's endpoints, authenticated by the tokens `router` issued.
pub fn routes(app: Arc<App>, router: Arc<Router>) -> Routes {
    Routes::new()
        .route("/moebius/lens/report", post(report))
        .route("/moebius/lens/deep", post(deep))
        .route("/moebius/lens/deep-result", post(deep_result))
        .with_state(Served { app, router })
}

/// One finding of a linter, a parser or a type checker.
#[derive(Debug, Clone, Deserialize)]
struct Finding {
    tool: String,
    /// `error` or `warning`.
    severity: String,
    line: u32,
    #[serde(default)]
    column: Option<u32>,
    #[serde(default)]
    rule: Option<String>,
    message: String,
    /// Whether the line is one the edit touched.
    touched: bool,
    /// What identifies the problem across edits, without its line: the tool, the rule and the message.
    key: String,
}

/// The time one check took, and whether it ran.
#[derive(Debug, Clone, Deserialize)]
struct Timing {
    /// `syntax`, `lint` or `deep`.
    phase: String,
    tool: String,
    ms: u64,
    /// Why the check did not run (`over budget`, `not installed`, ...), when it did not.
    #[serde(default)]
    skipped: Option<String>,
    #[serde(default)]
    findings: u64,
    /// For the end-of-turn check: how many findings it told the agent about.
    #[serde(default)]
    shown: u64,
}

#[derive(Debug, Deserialize)]
struct Edit {
    tool: String,
    first_line: u32,
    last_line: u32,
    /// The new text of the touched lines.
    #[serde(default)]
    excerpt: String,
}

#[derive(Debug, Deserialize)]
struct Report {
    #[serde(default)]
    tool_call_id: Option<String>,
    file: String,
    edit: Edit,
    /// The most findings the agent is shown for one edit.
    max_lines: usize,
    /// The new findings, after the edit.
    findings: Vec<Finding>,
    /// The keys of every finding the file has now, new or old.
    present: Vec<String>,
    checks: Vec<Timing>,
}

#[derive(Debug, Serialize)]
struct ReportReply {
    /// The positions in `findings` of the ones to show.
    shown: Vec<usize>,
    hidden: usize,
}

async fn report(State(state): State<Served>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(grant) = state.router.grant(&headers) else {
        return unauthorized();
    };
    let request: Report = match parse(&body) {
        Ok(request) => request,
        Err(rejection) => return rejection.into_response(),
    };
    match tokio::task::spawn_blocking(move || judge_report(&state.app, &grant, &request)).await {
        Ok(reply) => Json(reply).into_response(),
        Err(_) => cannot("cannot judge the findings"),
    }
}

fn cannot(why: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": why })),
    )
        .into_response()
}

/// Puts each new finding to `decide("lint_report")` and stores the checks' times. Findings that were shown or hidden
/// before and are gone now are fixed; that is their outcome.
fn judge_report(app: &App, grant: &Grant, report: &Report) -> ReportReply {
    close_fixed(app, grant.run_id, &report.file, &report.present);
    let mut order: Vec<usize> = (0..report.findings.len()).collect();
    // Errors first, then by line, so that the limit cuts the least important.
    order.sort_by_key(|&i| {
        let f = &report.findings[i];
        (f.severity != "error", f.line, i)
    });
    let max = report.max_lines.clamp(1, 200);
    let excerpt: String = report
        .edit
        .excerpt
        .lines()
        .take(EXCERPT_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let mut shown = Vec::new();
    for &i in order.iter().take(DECIDED_MAX) {
        let finding = &report.findings[i];
        let mut context = BTreeMap::from([
            ("file".to_string(), report.file.clone()),
            ("tool".to_string(), finding.tool.clone()),
            ("severity".to_string(), finding.severity.clone()),
            ("line".to_string(), finding.line.to_string()),
            ("message".to_string(), finding.message.clone()),
            ("touched".to_string(), finding.touched.to_string()),
            ("key".to_string(), finding.key.clone()),
            ("edit_tool".to_string(), report.edit.tool.clone()),
            (
                "edit_lines".to_string(),
                format!("{}-{}", report.edit.first_line, report.edit.last_line),
            ),
            ("edit".to_string(), excerpt.clone()),
            ("shown_so_far".to_string(), shown.len().to_string()),
            ("max_lines".to_string(), max.to_string()),
            ("run_id".to_string(), grant.run_id.to_string()),
        ]);
        if let Some(column) = finding.column {
            context.insert("column".into(), column.to_string());
        }
        if let Some(rule) = &finding.rule {
            context.insert("rule".into(), rule.clone());
        }
        let question = Question {
            kind: "lint_report".into(),
            context,
            options: vec!["show".into(), "hide".into()],
            // The safe side is to show the finding.
            fail_closed: "show".into(),
            run_id: Some(grant.run_id),
            card_id: grant.card_id,
        };
        let show = app
            .decisions
            .decide(&question)
            .map_or(true, |decision| decision.choice == "show");
        if show {
            shown.push(i);
        }
    }
    let hidden = report.findings.len() - shown.len();
    for timing in &report.checks {
        let shown_by_tool = shown
            .iter()
            .filter(|&&i| report.findings[i].tool == timing.tool)
            .count();
        let event = json!({
            "file": report.file,
            "tool_call_id": report.tool_call_id,
            "phase": timing.phase,
            "tool": timing.tool,
            "ms": timing.ms,
            "skipped": timing.skipped,
            "findings": timing.findings,
            "shown": shown_by_tool,
        });
        let _ = app.record_run_event(grant.run_id, "lens_check", &event);
    }
    shown.sort_unstable();
    ReportReply { shown, hidden }
}

/// Sets the outcome `fixed` on the open findings of `file` in the run whose key the file no longer has.
fn close_fixed(app: &App, run_id: i64, file: &str, present: &[String]) {
    let present: HashSet<&str> = present.iter().map(String::as_str).collect();
    let Ok(open) = app
        .store
        .judgements_page(None, Some(run_id), Some("lint_report"), 2000)
    else {
        return;
    };
    for judgement in open.into_iter().filter(|j| j.outcome.is_none()) {
        let Some(context) = judgement
            .context
            .first()
            .and_then(|blob| app.store.blobs().get(blob).ok())
            .and_then(|text| serde_json::from_str::<BTreeMap<String, String>>(&text).ok())
        else {
            continue;
        };
        if context.get("file").map(String::as_str) == Some(file)
            && context
                .get("key")
                .is_some_and(|key| !present.contains(key.as_str()))
        {
            let _ = app.store.set_outcome(judgement.id, "fixed");
        }
    }
}

/// Ends the lens's bookkeeping of a run: the findings still open were ignored.
pub fn close_run(app: &App, run_id: i64) {
    let _ = app
        .store
        .set_run_missing_outcomes(run_id, "lint_report", "ignored");
}

#[derive(Debug, Deserialize)]
struct Touched {
    path: String,
    /// Whether every edit of the file in this run changed comments only.
    #[serde(default)]
    comment_only: bool,
}

#[derive(Debug, Deserialize)]
struct Deep {
    files: Vec<Touched>,
    /// Which time this run asks, from 1.
    #[serde(default)]
    round: u32,
    /// The names of the commands that would run.
    #[serde(default)]
    tools: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DeepReply {
    run: bool,
    judgement_id: Option<i64>,
}

async fn deep(State(state): State<Served>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(grant) = state.router.grant(&headers) else {
        return unauthorized();
    };
    let request: Deep = match parse(&body) {
        Ok(request) => request,
        Err(rejection) => return rejection.into_response(),
    };
    match tokio::task::spawn_blocking(move || judge_deep(&state.app, &grant, &request)).await {
        Ok(reply) => Json(reply).into_response(),
        Err(_) => cannot("cannot decide"),
    }
}

/// Puts the end-of-turn check to `decide("deep_check")`. Anything that fails answers `run`.
fn judge_deep(app: &App, grant: &Grant, request: &Deep) -> DeepReply {
    let mut code = Vec::new();
    let mut comments = Vec::new();
    let mut docs = Vec::new();
    let mut config = Vec::new();
    for file in &request.files {
        match file_kind(&file.path) {
            FileKind::Docs => docs.push(file.path.as_str()),
            FileKind::Config => config.push(file.path.as_str()),
            FileKind::Code if file.comment_only => comments.push(file.path.as_str()),
            FileKind::Code => code.push(file.path.as_str()),
        }
    }
    let all: Vec<&str> = request.files.iter().map(|f| f.path.as_str()).collect();
    let question = Question {
        kind: "deep_check".into(),
        context: BTreeMap::from([
            ("files".to_string(), all.join("\n")),
            ("code_files".to_string(), code.join("\n")),
            ("comment_only_files".to_string(), comments.join("\n")),
            ("docs_files".to_string(), docs.join("\n")),
            ("config_files".to_string(), config.join("\n")),
            ("tools".to_string(), request.tools.join(", ")),
            ("round".to_string(), request.round.to_string()),
            ("run_id".to_string(), grant.run_id.to_string()),
        ]),
        options: vec!["run".into(), "skip".into()],
        // The safe side is to run the check.
        fail_closed: "run".into(),
        run_id: Some(grant.run_id),
        card_id: grant.card_id,
    };
    match app.decisions.decide(&question) {
        Ok(decision) => DeepReply {
            run: decision.choice == "run",
            judgement_id: Some(decision.judgement_id),
        },
        Err(_) => DeepReply {
            run: true,
            judgement_id: None,
        },
    }
}

#[derive(Debug, Deserialize)]
struct DeepResult {
    #[serde(default)]
    judgement_id: Option<i64>,
    /// How many findings there were that the agent was not told about before.
    new_findings: u64,
    checks: Vec<Timing>,
}

async fn deep_result(State(state): State<Served>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(grant) = state.router.grant(&headers) else {
        return unauthorized();
    };
    let request: DeepResult = match parse(&body) {
        Ok(request) => request,
        Err(rejection) => return rejection.into_response(),
    };
    match tokio::task::spawn_blocking(move || keep_deep(&state.app, &grant, &request)).await {
        Ok(()) => Json(json!({})).into_response(),
        Err(_) => cannot("cannot store the result"),
    }
}

/// Stores the times of the end-of-turn check and the outcome of the decision to run it.
fn keep_deep(app: &App, grant: &Grant, result: &DeepResult) {
    let ran = result.checks.iter().any(|c| c.skipped.is_none());
    if let Some(id) = result.judgement_id
        && let Ok(Some(judgement)) = app.store.judgement(id)
        && judgement.run_id == Some(grant.run_id)
        && judgement.kind == "deep_check"
    {
        let outcome = match (ran, result.new_findings) {
            (false, _) => "not run",
            (true, 0) => "nothing new",
            (true, _) => "found something new",
        };
        let _ = app.store.set_outcome(id, outcome);
    }
    for timing in &result.checks {
        let event: Value = json!({
            "file": null,
            "tool_call_id": null,
            "phase": timing.phase,
            "tool": timing.tool,
            "ms": timing.ms,
            "skipped": timing.skipped,
            "findings": timing.findings,
            "shown": timing.shown,
        });
        let _ = app.record_run_event(grant.run_id, "lens_check", &event);
    }
}
