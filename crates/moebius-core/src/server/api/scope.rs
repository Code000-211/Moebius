//! What the run token of a Grilling partner may reach in the issue tracker. A person and the Moebius agent are not
//! limited; the Grilling partner keeps a wayfinder map and nothing else, so it is held to the repository of its chat
//! and to the cards that carry a `wayfinder:*` label (the map and its tickets), and it creates only such cards. A card
//! or a repository outside its chat's repository is answered as if it did not exist (404), so that the answer does not
//! tell what exists elsewhere; a card of its repository that is no part of a wayfinder map is refused with 403.

use crate::app::{App, AppError, Result};
use crate::decide::Actor;
use crate::harness::pibolt::GRILLING_AGENT;

/// Whether a label marks a card as part of a wayfinder map: `wayfinder:` followed by anything, in any case.
pub(super) fn is_wayfinder(label: &str) -> bool {
    label.to_lowercase().starts_with("wayfinder:")
}

fn outside_wayfinder(what: &str) -> AppError {
    AppError::Forbidden(format!(
        "{what}: a Grilling partner works on wayfinder cards only (a card with a `wayfinder:*` label)"
    ))
}

/// The repository a Grilling partner's run is held to; none for anyone else. A run without a repository is refused.
fn held_to(app: &App, by: &Actor) -> Result<Option<i64>> {
    match by {
        Actor::Agent { name, run } if name == GRILLING_AGENT => app
            .store
            .run(*run)?
            .and_then(|run| run.repository_id)
            .map(Some)
            .ok_or_else(|| {
                AppError::Forbidden("this run works in no repository, so it has no issues".into())
            }),
        _ => Ok(None),
    }
}

/// Lets `by` reach the issues of repository `id`.
pub(super) fn repository(app: &App, by: &Actor, id: i64) -> Result<()> {
    match held_to(app, by)? {
        Some(own) if own != id => Err(AppError::NotFound(format!("repository {id}"))),
        _ => Ok(()),
    }
}

/// Lets `by` reach card `id`: a read when `writes` is false, otherwise a change, which only a wayfinder card takes.
pub(super) fn card(app: &App, by: &Actor, id: i64, writes: bool) -> Result<()> {
    let Some(own) = held_to(app, by)? else {
        return Ok(());
    };
    let card = app
        .store
        .card(id)?
        .filter(|card| card.repository_id == own)
        .ok_or_else(|| AppError::NotFound(format!("card {id}")))?;
    if writes && !card.labels.iter().any(|l| is_wayfinder(l)) {
        return Err(outside_wayfinder(&format!("card {id}")));
    }
    Ok(())
}

/// Lets `by` relate a card of repository `repository` to the issue with `number`: it has to be a wayfinder card.
pub(super) fn related(app: &App, by: &Actor, repository: i64, number: i64) -> Result<()> {
    if held_to(app, by)?.is_none() {
        return Ok(());
    }
    match app.store.card_by_number(repository, number)? {
        Some(card) if card.labels.iter().any(|l| is_wayfinder(l)) => Ok(()),
        Some(_) => Err(outside_wayfinder(&format!("issue {number}"))),
        None => Err(AppError::NotFound(format!("issue {number}"))),
    }
}

/// Lets `by` give a card the labels `labels` (when it creates one or replaces them): a Grilling partner's cards keep a
/// `wayfinder:*` label.
pub(super) fn labels(app: &App, by: &Actor, labels: &[String]) -> Result<()> {
    if held_to(app, by)?.is_some() && !labels.iter().any(|l| is_wayfinder(l)) {
        return Err(outside_wayfinder(
            "the labels must include a `wayfinder:*` label",
        ));
    }
    Ok(())
}

/// The repository of card `id`, which the issue numbers in a request about it are numbers of.
pub(super) fn repository_of_card(app: &App, id: i64) -> Result<i64> {
    app.store
        .card(id)?
        .map(|card| card.repository_id)
        .ok_or_else(|| AppError::NotFound(format!("card {id}")))
}
