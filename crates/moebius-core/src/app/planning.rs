//! A card's priority and due date. Both are kept as labels of the card (`priority:high`, `due:2026-12-01`, see
//! `store::planning`), so setting one is setting labels: it is written to the forge as they are, announced as
//! `card_labels_changed`, and a poll that brings the forge's labels brings the priority and the due date.

use super::{App, AppError, Result};
use crate::store::{Card, Priority, is_date, with_due, with_priority};

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
}
