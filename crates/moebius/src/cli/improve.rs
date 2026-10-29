//! Learning from what was recorded: `retro` shows and runs the nightly Retros, `proposals` and `proposal` list and
//! settle what they propose, `research` drafts, confirms, starts and stops autoresearch, `decision-model` writes what
//! the decision model has measured, and `takeovers` and `takeover` list and settle what it earned. The training
//! export works on the store itself and is `moebius export`, not a command of a running server.

use clap::Subcommand;
use serde_json::{Value, json};

use super::{Plan, Request, Show, cell, table};

#[derive(Debug, Subcommand)]
pub enum RetroCommand {
    /// Run the Retro of a day now.
    Run {
        /// The day, `YYYY-MM-DD`; today without it.
        #[arg(long)]
        date: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProposalCommand {
    Merge {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
    Dismiss {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ResearchCommand {
    /// Store a brief as a draft.
    New {
        /// The id of the repository.
        #[arg(long)]
        repository: i64,
        #[arg(long)]
        goal: String,
        /// The shell command that measures the goal.
        #[arg(long)]
        measure: String,
        /// `lower` or `higher`.
        #[arg(long)]
        better: String,
        #[arg(long)]
        token_budget: i64,
    },
    /// Confirm a draft.
    Confirm { id: i64 },
    /// Start a confirmed research now.
    Start { id: i64 },
    /// Stop a research.
    Stop { id: i64 },
}

#[derive(Debug, Subcommand)]
pub enum TakeoverCommand {
    Merge {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
    Dismiss {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
    Release {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
}

/// The columns of a list of proposals, alone and in a Retro.
const PROPOSAL_COLUMNS: &[&str] = &["id", "kind", "status", "branch", "title"];

/// The columns of a list of takeovers, alone and in what the decision model measured.
const TAKEOVER_COLUMNS: &[&str] = &["id", "kind", "status", "agreement", "count"];

/// `retro`, `retro <date>` and `retro run`.
pub fn retro(date: Option<&str>, command: Option<&RetroCommand>) -> Plan {
    Ok(match (date, command) {
        (_, Some(RetroCommand::Run { date })) => (
            Request::post(
                "/v1/retros",
                date.as_ref().map_or(json!({}), |d| json!({ "date": d })),
            ),
            Show::Retro,
        ),
        (Some(date), None) => (Request::get(format!("/v1/retros/{date}")), Show::Retro),
        (None, None) => (
            Request::get("/v1/retros"),
            Show::Table(&["date", "status", "errors", "proposals"]),
        ),
    })
}

/// `proposals`.
pub fn proposals() -> Plan {
    Ok((Request::get("/v1/proposals"), Show::Table(PROPOSAL_COLUMNS)))
}

/// `proposal merge` and `dismiss`.
pub fn proposal(command: &ProposalCommand) -> Plan {
    let (action, id, reason) = match command {
        ProposalCommand::Merge { id, reason } => ("merge", id, reason),
        ProposalCommand::Dismiss { id, reason } => ("dismiss", id, reason),
    };
    Ok((
        Request::post(
            format!("/v1/proposals/{id}/{action}"),
            json!({ "reason": reason }),
        ),
        Show::Object,
    ))
}

/// `research`, `research <id>`, and `research new`, `confirm`, `start` and `stop`.
pub fn research(id: Option<i64>, command: Option<&ResearchCommand>) -> Plan {
    let (id, action) = match command {
        None => {
            return Ok(match id {
                None => (
                    Request::get("/v1/research"),
                    Show::Table(&["id", "status", "repository_id", "goal"]),
                ),
                Some(id) => (Request::get(format!("/v1/research/{id}")), Show::Object),
            });
        }
        Some(ResearchCommand::New {
            repository,
            goal,
            measure,
            better,
            token_budget,
        }) => {
            let brief = json!({
                "repository": repository,
                "goal": goal,
                "measure": measure,
                "better": better,
                "token_budget": token_budget,
            });
            return Ok((Request::post("/v1/research", brief), Show::Object));
        }
        Some(ResearchCommand::Confirm { id }) => (id, "confirm"),
        Some(ResearchCommand::Start { id }) => (id, "start"),
        Some(ResearchCommand::Stop { id }) => (id, "stop"),
    };
    Ok((
        Request::post(format!("/v1/research/{id}/{action}"), json!({})),
        Show::Object,
    ))
}

/// `decision-model`.
pub fn decision_model() -> Plan {
    Ok((Request::get("/v1/decision-model"), Show::DecisionModel))
}

/// `takeovers`.
pub fn takeovers() -> Plan {
    Ok((Request::get("/v1/takeovers"), Show::Table(TAKEOVER_COLUMNS)))
}

/// `takeover merge`, `dismiss` and `release`.
pub fn takeover(command: &TakeoverCommand) -> Plan {
    let (action, id, reason) = match command {
        TakeoverCommand::Merge { id, reason } => ("merge", id, reason),
        TakeoverCommand::Dismiss { id, reason } => ("dismiss", id, reason),
        TakeoverCommand::Release { id, reason } => ("release", id, reason),
    };
    Ok((
        Request::post(
            format!("/v1/takeovers/{id}/{action}"),
            json!({ "reason": reason }),
        ),
        Show::Object,
    ))
}

/// A Retro: its day and status, its warnings and errors, and its proposals.
pub fn show_retro(view: &Value) -> String {
    let mut out = vec![format!(
        "retro {} {}",
        cell(&view["date"]),
        cell(&view["status"])
    )];
    for warning in view["warnings"].as_array().into_iter().flatten() {
        out.push(format!("warning: {}", cell(warning)));
    }
    let errors = &view["report"]["errors"];
    out.push(format!(
        "errors ({}):",
        errors.as_array().map_or(0, Vec::len)
    ));
    for error in errors.as_array().into_iter().flatten() {
        out.push(format!(
            "  {}  {}",
            cell(&error["kind"]),
            cell(&error["text"])
        ));
    }
    let proposals = &view["proposals"];
    if proposals.as_array().is_some_and(|p| !p.is_empty()) {
        out.push("proposals:".into());
        out.push(
            table(proposals, PROPOSAL_COLUMNS)
                .lines()
                .map(|l| format!("  {l}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    out.join("\n")
}

/// What the decision model has measured: its thresholds, each kind's shadows and the takeovers.
pub fn show_decision_model(view: &Value) -> String {
    if view["configured"] != true {
        return "no decision model is configured".into();
    }
    [
        format!(
            "model {}: takes a kind over at {} agreement with {} judgements",
            cell(&view["model"]),
            cell(&view["agreement"]),
            cell(&view["min_judgements"])
        ),
        "kinds:".into(),
        table(
            &view["kinds"],
            &[
                "kind",
                "shadows",
                "agreement",
                "failed",
                "dropped",
                "routed",
            ],
        ),
        "takeovers:".into(),
        table(&view["takeovers"], TAKEOVER_COLUMNS),
    ]
    .join("\n")
}
