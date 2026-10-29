//! A person's decision about a tried proposal: merged into the harness configuration or dismissed, while it is
//! positive or untried, and its inbox item resolved.

use super::{find, publish_status};
use crate::app::{App, AppError, Result};
use crate::decide::Actor;
use crate::harness::repo::HarnessRepo;
use crate::store::Proposal;

/// The proposal `id`, if a person can still decide about it: it was tried, and it was not negative.
fn to_decide(app: &App, id: i64) -> Result<Proposal> {
    let proposal = find(app, id)?;
    if !matches!(proposal.status.as_str(), "positive" | "untried") {
        return Err(AppError::Conflict(format!(
            "proposal {id} is {}, not positive or untried",
            proposal.status
        )));
    }
    Ok(proposal)
}

/// Closes the matter of a proposal: its inbox item is resolved and it has `status`.
fn decided(app: &App, id: i64, status: &str) -> Result<Proposal> {
    app.store.resolve_inbox_for_proposal(id)?;
    app.store.set_proposal_status(id, status)?;
    publish_status(app, id, status);
    find(app, id)
}

/// A person merges a positive or untried proposal into the harness configuration. A conflict changes nothing.
pub fn merge(app: &App, by: &Actor, repo: &HarnessRepo, id: i64, reason: &str) -> Result<Proposal> {
    let proposal = to_decide(app, id)?;
    repo.merge_proposal(app, by, &proposal.branch, &proposal.title, reason)?;
    decided(app, id, "merged")
}

/// A person dismisses a positive or untried proposal.
pub fn dismiss(
    app: &App,
    by: &Actor,
    repo: &HarnessRepo,
    id: i64,
    reason: &str,
) -> Result<Proposal> {
    let proposal = to_decide(app, id)?;
    repo.dismiss_proposal(app, by, &proposal.branch, &proposal.title, reason)?;
    decided(app, id, "dismissed")
}
