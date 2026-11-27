//! Measurements an author commits with the change (issue #781). In a repository with `trust_author_measurement` on, the
//! authors measure their own changes and commit the record under one of the repository's `author_measurement_paths`,
//! and the Reviewer checks it against the review brief. A round of the Reviewer (not one a person asked for) that ends
//! `approve` for a head whose merge adds or changes such a file is recorded as an accepted measurement of that head,
//! with the source `author (reviewed)` and the author as measurer, before the approval's merge goes through the gate. A
//! head without such a file is not measured by this, and a push makes a new head that needs its own round. When the
//! merge is asked, the latest such round of the head is weighed again (issue #792), so a round approved before the
//! repository trusted the author counts once it does.

use super::short;
use crate::app::{App, Result};
use crate::store::{AUTHOR_REVIEWED, Card, Change, ExternalReview, Repository};

impl App {
    /// Accepts the author's committed measurement of the change's current head on the latest finished round of the
    /// Reviewer for that head, as [`App::accept_author_measurement`] does when the verdict is written: the merge gate
    /// asks for it, so a setting turned on after the round was approved applies to it.
    pub(in crate::app) fn accept_approved_author_measurement(
        &self,
        repository: &Repository,
        card: &Card,
        change: &Change,
    ) -> Result<()> {
        if !repository.trust_author_measurement {
            return Ok(());
        }
        let Some(head) = change.head.as_deref() else {
            return Ok(());
        };
        let rounds = self.store.external_reviews(change.id)?;
        let Some(round) = rounds.iter().rev().find(|round| {
            round.head == head
                && round.verdict.is_some()
                && matches!(round.kind.as_str(), "author" | "measurement")
        }) else {
            return Ok(());
        };
        let verdict = round.verdict.as_deref().unwrap_or_default();
        self.accept_author_measurement(repository, card, change, round, verdict)
    }

    /// Accepts the author's committed measurement of the head `round` read, when the repository trusts one, the round is
    /// the Reviewer's own and approved it, and its merge writes a file under `author_measurement_paths`. What cannot be
    /// read accepts nothing.
    pub(in crate::app) fn accept_author_measurement(
        &self,
        repository: &Repository,
        card: &Card,
        change: &Change,
        round: &ExternalReview,
        verdict: &str,
    ) -> Result<()> {
        if !repository.trust_author_measurement
            || repository.author_measurement_paths.is_empty()
            || verdict != "approve"
            || !matches!(round.kind.as_str(), "author" | "measurement")
            || round.base.is_empty()
            || self.store.has_measurement(change.id, &round.head)?
        {
            return Ok(());
        }
        let files =
            match crate::app::review_context::merged_writes(repository, &round.base, &round.head) {
                Ok(files) => files,
                Err(e) => {
                    eprintln!(
                        "moebius: cannot look for the author's measurement of change {}: {e}",
                        change.id
                    );
                    return Ok(());
                }
            };
        let paths = repository
            .author_measurement_paths
            .iter()
            .map(String::as_str);
        let Some(file) = files
            .iter()
            .find(|file| crate::app::factory::paths::locked_by(paths.clone(), file).is_some())
        else {
            return Ok(());
        };
        let author = change.outside_author.as_deref().unwrap_or("author");
        self.store
            .add_measurement(change.id, &round.head, author, AUTHOR_REVIEWED)?;
        self.store.note_activity(
            "measurement",
            Some(card.repository_id),
            Some(card.number),
            Some(author),
            &format!(
                "the author's measurement of {} ({file}) accepted, reviewed",
                short(&round.head)
            ),
            &serde_json::json!({
                "change": change.id,
                "head": round.head,
                "card": card.id,
                "source": AUTHOR_REVIEWED,
            }),
        )?;
        Ok(())
    }
}
