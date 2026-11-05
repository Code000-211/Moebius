//! A person's decision about a proposal for Moebius's own configuration: merged, which saves a `settings` proposal
//! through the settings code and offers a `repository` proposal as a change of the harness configuration repository,
//! or dismissed, which changes nothing. Both are recorded as decisions of the kind `merge_proposal`.

use std::collections::BTreeMap;

use super::{REPOSITORY, SETTINGS};
use crate::app::{App, AppError, Result};
use crate::decide::{Actor, Question};
use crate::server::settings::{Changes, Live};
use crate::store::Proposal;

/// A person merges the proposal: a `settings` one is saved to `moebius.toml` as `PUT /v1/settings` saves a change, a
/// `repository` one is committed on the branch `moebius/proposal-<id>` of the harness configuration repository's
/// clone, in the files of `repos/<name>/`, and offered there as a change for a person to merge. What fails changes nothing and records nothing.
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

/// Commits the proposal's files, the project's files of `repos/<name>/`, on a new branch of the harness configuration
/// repository and offers the branch as a change there (see `HarnessRepo::offer_files`).
fn offer(app: &App, proposal: &Proposal) -> Result<()> {
    let id = proposal
        .repository
        .ok_or_else(|| AppError::Invalid(format!("proposal {} has no repository", proposal.id)))?;
    let repository = app.repository(id)?;
    let harness = app.harness_repo()?;
    let branch = format!("moebius/proposal-{}", proposal.id);
    let message = format!(
        "{}\n\n{}\n\nProposed by the Retro of {}.",
        proposal.title, proposal.body, proposal.retro_date
    );
    let change = harness.offer_files(app, &branch, message.trim(), |worktree| {
        for (path, text) in proposal.edits["files"].as_object().into_iter().flatten() {
            let file = worktree.join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file, text.as_str().unwrap_or_default())?;
        }
        Ok(())
    });
    let change = change.map_err(|e| match e {
        AppError::Invalid(_) => AppError::Invalid(format!(
            "proposal {} changes nothing in {} any more",
            proposal.id, repository.name
        )),
        other => other,
    })?;
    Ok(app.store.set_proposal_change(proposal.id, change.id)?)
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
