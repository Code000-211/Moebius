//! Deleting a research: a brief that is running is stopped first, then its experiments, its runs, the changes offered for
//! its kept levers, its lever branches and its worktrees are taken away. The usage of its runs stays.

use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;

use super::rounds::lever_branches;
use super::{find, repository_of, stop};
use crate::app::{App, AppError, Result};
use crate::board::Runs;
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::Research;

/// A person deletes the research `id`. A research that is running is stopped first, and the deletion waits for its
/// runs and its thread to end. Recorded as a `delete_research` decision and announced as `research_deleted`.
pub fn delete(
    app: &Arc<App>,
    runs: &dyn Runs,
    by: &Actor,
    id: i64,
    reason: &str,
) -> Result<Research> {
    let research = find(app, id)?;
    let repository = repository_of(app, &research)?;
    // Stopping also keeps a start from taking the research up meanwhile; one that has ended in between is as good.
    match stop(app, runs, id) {
        Ok(_) | Err(AppError::Conflict(_)) => {}
        Err(e) => return Err(e),
    }
    let what = format!("research {id}");
    let ids: Vec<i64> = app
        .store
        .runs_of_research(id)?
        .iter()
        .map(|r| r.id)
        .collect();
    for run in app.store.runs_of_research(id)? {
        if run.ended_at.is_none() {
            runs.stop(run.id);
        }
    }
    // Holding the research keeps a start from beginning after this, so no driver writes to a deleted research.
    let held = RefCell::new(None);
    app.wait_until(&what, || {
        *held.borrow_mut() = app.researching(id);
        held.borrow().is_some()
    })?;
    app.wait_until_ended(&what, &ids)?;
    let clone = Path::new(&repository.path);
    let branches = lever_branches(&repository, id).unwrap_or_default();
    let research_runs = app.store.runs_of_research(id)?;
    let removed = app
        .store
        .delete_research(id)?
        .ok_or_else(|| AppError::NotFound(what))?;
    for run in &research_runs {
        app.remove_run_worktree(Some(clone), run);
    }
    for branch in &branches {
        let _ = app.repos.delete_branch(clone, branch);
    }
    let context = [
        ("research".to_string(), id.to_string()),
        ("repository".to_string(), repository.name.clone()),
        ("goal".to_string(), research.goal.clone()),
        ("status".to_string(), research.status.clone()),
        ("experiments".to_string(), removed.experiments.to_string()),
        ("runs".to_string(), crate::app::list(&removed.runs)),
        ("changes".to_string(), crate::app::list(&removed.changes)),
        ("branches".to_string(), branches.join(",")),
        ("usage_kept".to_string(), removed.usage.to_string()),
    ]
    .into();
    app.record_action(by, "delete_research", None, None, context, reason)?;
    drop(held);
    app.bus.publish(Event::ResearchDeleted { research: id });
    Ok(research)
}
