//! A research's brief, from draft to confirmed: stored as a draft, written anew while it is one, confirmed by a person
//! as a recorded decision, and a research stopped before it ends. What happens once it runs is in `rounds`.

use serde::Deserialize;

use super::{ResearchView, find, publish_status, repository_of, view};
use crate::app::{App, AppError, Result};
use crate::board::Runs;
use crate::decide::{Actor, Question};

/// The body of `POST /v1/research`.
#[derive(Debug, Clone, Deserialize)]
pub struct NewResearch {
    pub repository: i64,
    pub goal: String,
    pub measure: String,
    pub better: String,
    pub token_budget: i64,
}

/// The body of `PUT /v1/research/{id}`: the brief of a draft, written anew.
#[derive(Debug, Clone, Deserialize)]
pub struct Brief {
    pub goal: String,
    pub measure: String,
    pub better: String,
    pub token_budget: i64,
    /// Why the brief changes, kept with the decision.
    #[serde(default)]
    pub reason: String,
}

/// Refuses a brief without a goal or a measure, with a direction other than `lower` or `higher`, or with a budget
/// that is not more than 0.
fn check_brief(goal: &str, measure: &str, better: &str, token_budget: i64) -> Result<()> {
    let invalid = |why: &str| Err(AppError::Invalid(why.into()));
    if goal.trim().is_empty() {
        return invalid("a research needs a goal");
    }
    if measure.trim().is_empty() {
        return invalid("a research needs a measure command");
    }
    if !matches!(better, "lower" | "higher") {
        return invalid("better is lower or higher");
    }
    if token_budget <= 0 {
        return invalid("the token budget must be more than 0");
    }
    Ok(())
}

/// Refuses a research that is no longer a draft.
fn check_draft(id: i64, status: &str) -> Result<()> {
    if status == "draft" {
        return Ok(());
    }
    Err(AppError::Conflict(format!(
        "research {id} is {status}, not a draft"
    )))
}

/// The question of a decision of `kind` about a brief, with the options `choice` and `safe`, the one taken when no one
/// answers.
fn question<const N: usize>(
    kind: &str,
    [choice, safe]: [&str; 2],
    context: [(&str, String); N],
) -> Question {
    Question {
        kind: kind.into(),
        context: context.map(|(key, value)| (key.to_string(), value)).into(),
        options: vec![choice.into(), safe.into()],
        fail_closed: safe.into(),
        run_id: None,
        card_id: None,
    }
}

/// Stores a brief as a draft.
pub fn create(app: &App, new: NewResearch) -> Result<ResearchView> {
    check_brief(&new.goal, &new.measure, &new.better, new.token_budget)?;
    app.store
        .repository(new.repository)?
        .ok_or_else(|| AppError::NotFound(format!("repository {}", new.repository)))?;
    let research = app.store.add_research(
        new.repository,
        &new.goal,
        &new.measure,
        &new.better,
        new.token_budget,
    )?;
    publish_status(app, research.id, &research.status);
    view(app, research.id)
}

/// A person, or the Moebius agent, writes the brief of a draft anew: checked as a new brief is, recorded as an
/// `edit_research` decision and announced as `research_status`. A brief that changes nothing does neither.
pub fn edit(app: &App, by: &Actor, id: i64, brief: &Brief) -> Result<ResearchView> {
    let research = find(app, id)?;
    check_draft(id, &research.status)?;
    let (goal, measure) = (brief.goal.trim(), brief.measure.trim());
    check_brief(goal, measure, &brief.better, brief.token_budget)?;
    let unchanged = (goal, measure, brief.better.as_str(), brief.token_budget)
        == (
            research.goal.as_str(),
            research.measure.as_str(),
            research.better.as_str(),
            research.token_budget,
        );
    if unchanged {
        return view(app, id);
    }
    if !app
        .store
        .set_research_brief(id, goal, measure, &brief.better, brief.token_budget)?
    {
        return Err(AppError::Conflict(format!(
            "research {id} is not a draft any more"
        )));
    }
    let repository = app
        .store
        .repository(research.repository_id)?
        .map(|r| r.name)
        .unwrap_or_default();
    let asked = question(
        "edit_research",
        ["edit_research", "leave"],
        [
            ("repository", repository),
            ("goal", goal.to_string()),
            ("measure", measure.to_string()),
            ("better", brief.better.clone()),
            ("token_budget", brief.token_budget.to_string()),
            ("previous_goal", research.goal),
            ("previous_measure", research.measure),
            ("previous_better", research.better),
            ("previous_token_budget", research.token_budget.to_string()),
        ],
    );
    app.decisions
        .record_by(by, &asked, "edit_research", &brief.reason)?;
    publish_status(app, id, "draft");
    view(app, id)
}

/// A person confirms the brief of a draft: the decision is recorded and the research waits for its time.
pub fn confirm(app: &App, by: &Actor, id: i64) -> Result<ResearchView> {
    let research = find(app, id)?;
    let repository = repository_of(app, &research)?;
    check_draft(id, &research.status)?;
    let asked = question(
        "confirm_brief",
        ["confirm", "reject"],
        [
            ("repository", repository.name),
            ("goal", research.goal),
            ("measure", research.measure),
            ("better", research.better),
            ("token_budget", research.token_budget.to_string()),
        ],
    );
    app.decisions.record_by(by, &asked, "confirm", "")?;
    if !app.store.move_research(id, &["draft"], "confirmed", "")? {
        return Err(AppError::Conflict(format!(
            "research {id} is not a draft any more"
        )));
    }
    publish_status(app, id, "confirmed");
    view(app, id)
}

/// A person stops a research that has not ended; a research run that is going is stopped like a run is.
pub fn stop(app: &App, runs: &dyn Runs, id: i64) -> Result<ResearchView> {
    let research = find(app, id)?;
    let moved = app.store.move_research(
        id,
        &["draft", "confirmed", "running"],
        "stopped",
        "a person stopped it",
    )?;
    if !moved {
        return Err(AppError::Conflict(format!(
            "research {id} is {}, which has ended",
            research.status
        )));
    }
    publish_status(app, id, "stopped");
    for run in app.store.runs_of_research(id)? {
        if run.ended_at.is_none() {
            runs.stop(run.id);
        }
    }
    view(app, id)
}
