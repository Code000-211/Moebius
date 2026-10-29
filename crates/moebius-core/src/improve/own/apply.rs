//! A person's decision about a proposal for Moebius's own configuration: merged, which saves a `settings` proposal
//! through the settings code and offers a `repository` proposal as a change on the repository, or dismissed, which
//! changes nothing. Both are recorded as decisions of the kind `merge_proposal`.

use std::collections::BTreeMap;
use std::path::Path;

use super::{REPOSITORY, SETTINGS};
use crate::app::{App, AppError, Result};
use crate::decide::{Actor, Question};
use crate::repos::{commit_all, git};
use crate::server::settings::{Changes, Live};
use crate::store::Proposal;

/// A person merges the proposal: a `settings` one is saved to `moebius.toml` as `PUT /v1/settings` saves a change, a
/// `repository` one is committed on the branch `moebius/proposal-<id>` of the repository's clone and offered as a
/// change through its forge. What fails changes nothing and records nothing.
pub fn merge(app: &App, by: &Actor, live: &Live, proposal: &Proposal, reason: &str) -> Result<()> {
    match proposal.kind.as_str() {
        SETTINGS => {
            let changes: Changes = serde_json::from_value(proposal.edits.clone())
                .map_err(|e| AppError::Invalid(format!("the proposal's change is broken: {e}")))?;
            live.save(app, by, &changes)?;
        }
        REPOSITORY => offer(app, proposal)?,
        other => {
            return Err(AppError::Invalid(format!(
                "proposal {} is of the kind {other}, not of Moebius's own configuration",
                proposal.id
            )));
        }
    }
    record(app, by, proposal, "merge_proposal", reason)
}

/// A person dismisses the proposal; nothing changes.
pub fn dismiss(app: &App, by: &Actor, proposal: &Proposal, reason: &str) -> Result<()> {
    record(app, by, proposal, "dismiss", reason)
}

/// Commits the proposal's files on a new branch from the repository's default branch and offers the branch as a
/// change. The branch is removed again when that fails.
fn offer(app: &App, proposal: &Proposal) -> Result<()> {
    let id = proposal
        .repository
        .ok_or_else(|| AppError::Invalid(format!("proposal {} has no repository", proposal.id)))?;
    let repository = app.repository(id)?;
    // Best effort: the change should start from the forge's latest default branch.
    if let Ok(forge) = app.forge(&repository) {
        let _ = forge.refresh(&repository);
    }
    let clone = Path::new(&repository.path);
    let name = format!("proposal-{}", proposal.id);
    let branch = format!("moebius/{name}");
    let worktree =
        app.repos
            .add_detached_worktree(&repository.name, clone, &name, &repository.base_ref())?;
    let committed = (|| -> Result<String> {
        for (path, text) in proposal.edits["files"].as_object().into_iter().flatten() {
            let file = worktree.join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file, text.as_str().unwrap_or_default())?;
        }
        let message = format!("{}\n\n{}", proposal.title, proposal.body);
        if !commit_all(&worktree, message.trim())? {
            return Err(AppError::Invalid(format!(
                "proposal {} changes nothing in {} any more",
                proposal.id, repository.name
            )));
        }
        Ok(git(&worktree, &["rev-parse", "HEAD"])?)
    })();
    let _ = app.repos.remove_worktree(clone, &worktree);
    let head = committed?;
    app.repos.point_branch(clone, &branch, &head)?;
    let body = format!(
        "{}\n\nProposed by the Retro of {}.\n\nMade by Moebius.",
        proposal.body, proposal.retro_date
    );
    match app.offer_branch_change(&repository, &branch, &proposal.title, body.trim_start()) {
        Ok(change) => Ok(app.store.set_proposal_change(proposal.id, change.id)?),
        Err(e) => {
            let _ = git(clone, &["branch", "-D", "--quiet", &branch]);
            Err(e)
        }
    }
}

fn record(app: &App, by: &Actor, proposal: &Proposal, choice: &str, reason: &str) -> Result<()> {
    app.decisions.record_by(
        by,
        &Question {
            kind: "merge_proposal".into(),
            context: BTreeMap::from([
                ("branch".to_string(), proposal.branch.clone()),
                ("proposal".to_string(), proposal.title.clone()),
                ("kind".to_string(), proposal.kind.clone()),
                ("diff".to_string(), proposal.diff.clone()),
            ]),
            options: vec!["merge_proposal".into(), "dismiss".into()],
            fail_closed: "dismiss".into(),
            run_id: None,
            card_id: None,
        },
        choice,
        reason,
    )?;
    Ok(())
}
