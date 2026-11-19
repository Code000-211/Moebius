//! A card's priority, due date and estimate. All are kept as labels of the card (`priority:high`, `due:2026-12-01`,
//! `estimate:5`, see `store::planning`), so setting one is setting labels: it is written to the forge as they are, announced as
//! `card_labels_changed`, and a poll that brings the forge's labels brings them. The Moebius agent proposes an estimate
//! from the cards that look like the card and have one, and names them.

use serde::{Deserialize, Serialize};

use super::issues::likeness;
use super::{App, AppError, Result};
use crate::store::{
    Card, ESTIMATE_LIMIT, Priority, is_date, with_due, with_estimate, with_priority,
};

/// The most cards an estimate is proposed from.
const COMPARED: usize = 5;

/// A card an estimate was proposed from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Compared {
    pub id: i64,
    pub number: i64,
    pub title: String,
    /// The estimate that card has, in points.
    pub estimate: u32,
    /// From 0.5 to 1: the share of their words the two cards have in common.
    pub score: f64,
}

/// An estimate the Moebius agent proposes for a card: the median of the estimates of the cards it compared, which it
/// names. Nothing is set by it; a person sets the estimate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstimateProposal {
    /// The proposed points; none when no card with an estimate looks like this one.
    pub estimate: Option<u32>,
    /// The cards compared, the most alike first.
    pub compared: Vec<Compared>,
}

impl App {
    /// Gives the card the priority called `name` (`urgent`, `high`, `medium`, `low` or `none`, in any case), replacing
    /// its `priority:` labels.
    pub fn set_priority(&self, card_id: i64, name: &str) -> Result<Card> {
        let priority = Priority::parse(name).ok_or_else(|| {
            AppError::Invalid(format!(
                "{name:?} is not a priority (urgent, high, medium, low or none)"
            ))
        })?;
        let card = self.card(card_id)?;
        self.set_labels(card_id, &with_priority(&card.labels, priority))
    }

    /// Gives the card the due date `due` (`YYYY-MM-DD`), or none, replacing its `due:` labels.
    pub fn set_due(&self, card_id: i64, due: Option<&str>) -> Result<Card> {
        let due = due.map(str::trim).filter(|d| !d.is_empty());
        if let Some(due) = due
            && !is_date(due)
        {
            return Err(AppError::Invalid(format!(
                "{due:?} is not a date (YYYY-MM-DD)"
            )));
        }
        let card = self.card(card_id)?;
        self.set_labels(card_id, &with_due(&card.labels, due))
    }

    /// Gives the card an estimate of `points` (1 to [`ESTIMATE_LIMIT`]), or none, replacing its `estimate:` labels.
    pub fn set_estimate(&self, card_id: i64, points: Option<u32>) -> Result<Card> {
        if let Some(points) = points
            && !(1..=ESTIMATE_LIMIT).contains(&points)
        {
            return Err(AppError::Invalid(format!(
                "{points} is not an estimate (1 to {ESTIMATE_LIMIT} points)"
            )));
        }
        let card = self.card(card_id)?;
        self.set_labels(card_id, &with_estimate(&card.labels, points))
    }

    /// Proposes an estimate for the card from the cards of its repository that look like it and have an estimate: the
    /// median of up to five of them, the most alike first (a middle pair gives the higher of the two, as a
    /// proposal should not undershoot).
    pub fn propose_estimate(&self, card_id: i64) -> Result<EstimateProposal> {
        let card = self.card(card_id)?;
        let mut compared: Vec<Compared> = self
            .store
            .cards(card.repository_id)?
            .into_iter()
            .filter(|other| other.id != card.id)
            .filter_map(|other| {
                let estimate = other.estimate?;
                let score = likeness((&card.title, &card.body), (&other.title, &other.body))?;
                Some(Compared {
                    id: other.id,
                    number: other.number,
                    title: other.title,
                    estimate,
                    score: (score * 100.0).round() / 100.0,
                })
            })
            .collect();
        compared.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.number.cmp(&b.number)));
        compared.truncate(COMPARED);
        let mut points: Vec<u32> = compared.iter().map(|c| c.estimate).collect();
        points.sort_unstable();
        Ok(EstimateProposal {
            estimate: points.get(points.len() / 2).copied(),
            compared,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::app::OWNER_AUTHOR;
    use crate::board::testing::setup;

    #[test]
    fn an_estimate_is_set_as_a_label_and_checked() {
        let (_dir, app, _engine, repo) = setup();
        let card = app.create_issue(repo, "Cache", "", OWNER_AUTHOR).unwrap();
        let set = app.set_estimate(card.id, Some(5)).unwrap();
        assert_eq!(set.estimate, Some(5));
        assert!(set.labels.contains(&"estimate:5".to_string()));
        assert_eq!(app.set_estimate(card.id, None).unwrap().estimate, None);
        assert!(app.set_estimate(card.id, Some(0)).is_err());
        assert!(app.set_estimate(card.id, Some(1001)).is_err());
    }

    #[test]
    fn the_proposal_is_the_median_of_the_like_cards_and_names_them() {
        let (_dir, app, _engine, repo) = setup();
        let new = app
            .create_issue(repo, "Cache model downloads", "", OWNER_AUTHOR)
            .unwrap();
        let mut named = Vec::new();
        for (title, points) in [
            ("Cache the model downloads", Some(3)),
            ("Cache model downloads on disk", Some(8)),
            ("Cache model downloads again", Some(5)),
            ("Cache model downloads, no estimate", None),
            ("Dark theme for the settings page", Some(13)),
        ] {
            let card = app.create_issue(repo, title, "", OWNER_AUTHOR).unwrap();
            if let Some(points) = points {
                app.set_estimate(card.id, Some(points)).unwrap();
                named.push(card.id);
            }
        }
        let proposal = app.propose_estimate(new.id).unwrap();
        assert_eq!(proposal.estimate, Some(5));
        let ids: Vec<i64> = proposal.compared.iter().map(|c| c.id).collect();
        // The unlike card and the one without an estimate are not compared.
        assert_eq!(ids.len(), 3);
        assert!(ids.iter().all(|id| named[..3].contains(id)), "{ids:?}");

        let alone = app
            .create_issue(repo, "Something else entirely", "", OWNER_AUTHOR)
            .unwrap();
        let none = app.propose_estimate(alone.id).unwrap();
        assert_eq!((none.estimate, none.compared.len()), (None, 0));
    }
}
