//! A person's decision about a tried proposal: merged into the harness configuration or dismissed, while it is
//! positive or untried, and its inbox item resolved. A proposal for Moebius's own configuration, which is never tried,
//! is decided the same way through `own`.

use super::{find, publish_status};
use crate::app::{App, AppError, Result};
use crate::decide::Actor;
use crate::harness::repo::HarnessRepo;
use crate::improve::own;
use crate::server::settings::Live;
use crate::store::Proposal;

/// The harness configuration repository a proposal of the kind `harness` needs.
fn harness(repo: Option<&HarnessRepo>) -> Result<&HarnessRepo> {
    repo.ok_or_else(|| {
        AppError::Invalid(
            "agents are configured in moebius.toml, not in a harness repository".into(),
        )
    })
}

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

/// A person merges a positive or untried proposal into the harness configuration, or into Moebius's own configuration
/// (see `own::merge`). A conflict changes nothing.
pub fn merge(
    app: &App,
    by: &Actor,
    (repo, live): (Option<&HarnessRepo>, &Live),
    id: i64,
    reason: &str,
) -> Result<Proposal> {
    let proposal = to_decide(app, id)?;
    match proposal.kind.as_str() {
        "harness" => {
            harness(repo)?.merge_proposal(app, by, &proposal.branch, &proposal.title, reason)?;
        }
        _ => own::merge(app, by, live, &proposal, reason)?,
    }
    decided(app, id, "merged")
}

/// A person dismisses a positive or untried proposal.
pub fn dismiss(
    app: &App,
    by: &Actor,
    repo: Option<&HarnessRepo>,
    id: i64,
    reason: &str,
) -> Result<Proposal> {
    let proposal = to_decide(app, id)?;
    match proposal.kind.as_str() {
        "harness" => {
            harness(repo)?.dismiss_proposal(app, by, &proposal.branch, &proposal.title, reason)?
        }
        _ => own::dismiss(app, by, &proposal, reason)?,
    }
    decided(app, id, "dismissed")
}
