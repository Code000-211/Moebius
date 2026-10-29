//! The regression set (spec 7): a proposal that fixes its own cards may still break what worked, so it is also tried on
//! cards merged in the past and is positive only when none of them fails where it passed before.
//!
//! The regression set of a repository is a fixed sample of its merged cards: the latest `size` of them, among the
//! latest `LOOKBACK` merged changes, those whose change adds or changes a reproduction test (`.moebius/reproduce`)
//! first. It changes only as new cards merge. A card is in it only when its first code run recorded the commit it
//! started at, which is where its trial starts, so that the task is the one the card had. The proposal's own cards are
//! left out. A regression trial is the card's task run by the agent of its last code run with the definition from the
//! proposal's branch, measured as any trial is, against the card's code runs together: it must succeed, the checks
//! must pass, and it may use no more than 1.5 times their tokens. Each proposal adds at most `size` runs for each
//! repository its cards belong to, one after the other; the first regression card that fails ends the rest.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use super::card::{Task, Tried, run_tokens, run_trial};
use super::verdict::Original;
use crate::app::{App, Result};
use crate::board::Runs;
use crate::harness::repo::HarnessRepo;
use crate::repos::git;
use crate::store::{Card, Proposal, Repository};

/// How many of a repository's latest merged changes the regression set is chosen from.
const LOOKBACK: usize = 50;

/// The file of a change that holds its reproduction test; a card whose change adds or changes it is preferred.
const REPRODUCE: &str = ".moebius/reproduce";

/// A card of the regression set, with what its trial needs.
struct Member {
    card: Card,
    /// The agent of the card's last code run.
    agent: String,
    /// The commit the card's first code run started at.
    base: String,
    /// Whether the card's change adds or changes its reproduction test.
    reproduce: bool,
    /// The card's code runs together, which passed.
    original: Original,
}

/// The card of a merged change as a member of the regression set, if it can be one.
fn member(
    app: &App,
    repository: &Repository,
    card_id: i64,
    head: Option<&str>,
) -> Result<Option<Member>> {
    let store = &app.store;
    let Some(card) = store.card(card_id)? else {
        return Ok(None);
    };
    let code_runs: Vec<_> = store
        .runs_for_card(card_id)?
        .into_iter()
        .filter(|r| r.purpose == "code")
        .collect();
    let (Some(first), Some(last)) = (code_runs.first(), code_runs.last()) else {
        return Ok(None);
    };
    let Some(base) = store.run_base(first.id)? else {
        return Ok(None);
    };
    let clone = Path::new(&repository.path);
    if git(clone, &["cat-file", "-e", &format!("{base}^{{commit}}")]).is_err() {
        return Ok(None);
    }
    let reproduce = head.is_some_and(|head| {
        git(
            clone,
            &["diff", "--name-only", &base, head, "--", REPRODUCE],
        )
        .is_ok_and(|changed| !changed.trim().is_empty())
    });
    let mut tokens = 0;
    for run in &code_runs {
        tokens += run_tokens(store, run.id)?;
    }
    Ok(Some(Member {
        agent: last.agent.clone(),
        original: Original {
            errors: vec![],
            checks_failed: false,
            rounds: code_runs.len(),
            tokens,
        },
        card,
        base,
        reproduce,
    }))
}

/// The regression set of `repository`: at most `size` of its latest merged cards, those with a reproduction test
/// first, without the cards in `leave_out`.
fn set(app: &App, repository: &Repository, size: usize, leave_out: &[i64]) -> Result<Vec<Member>> {
    let mut seen = BTreeSet::new();
    let mut members = Vec::new();
    for change in app.store.merged_card_changes(repository.id, LOOKBACK)? {
        let Some(card) = change.card_id else { continue };
        if leave_out.contains(&card) || !seen.insert(card) {
            continue;
        }
        if let Some(member) = member(app, repository, card, change.head.as_deref())? {
            members.push(member);
        }
    }
    // A stable sort keeps the latest first among equals.
    members.sort_by_key(|m| !m.reproduce);
    members.truncate(size);
    Ok(members)
}

/// The repositories of the proposal's cards, each once, in the order of the cards.
fn repositories(app: &App, proposal: &Proposal) -> Result<Vec<Repository>> {
    let mut repositories: Vec<Repository> = Vec::new();
    for &card in &proposal.cards {
        let Some(card) = app.store.card(card)? else {
            continue;
        };
        if repositories.iter().any(|r| r.id == card.repository_id) {
            continue;
        }
        if let Some(repository) = app.store.repository(card.repository_id)? {
            repositories.push(repository);
        }
    }
    Ok(repositories)
}

/// Tries the proposal on the regression set of each repository of its cards, handing each trial to `record` as it
/// ends. Once a card fails, the cards after it are skipped.
pub(super) fn try_set(
    app: &Arc<App>,
    runs: &Arc<dyn Runs>,
    repo: &HarnessRepo,
    proposal: &Proposal,
    size: usize,
    mut record: impl FnMut(i64, Tried) -> Result<()>,
) -> Result<()> {
    let mut failed: Option<i64> = None;
    for repository in repositories(app, proposal)? {
        for member in set(app, &repository, size, &proposal.cards)? {
            let card = member.card.id;
            let tried = match failed {
                Some(other) => Tried::skipped(format!(
                    "not tried: card {other} of the regression set already fails with the proposal"
                )),
                None => {
                    let task = Task {
                        card: &member.card,
                        repository: &repository,
                        agent: &member.agent,
                        base: &member.base,
                    };
                    let kinds = BTreeSet::new();
                    run_trial(app, runs, repo, proposal, task, (&kinds, &member.original))
                        .unwrap_or_else(|tried| tried)
                }
            };
            if tried.outcome == "negative" {
                failed = Some(card);
            }
            record(card, tried)?;
        }
    }
    Ok(())
}
