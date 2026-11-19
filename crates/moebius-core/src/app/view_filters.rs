//! The parts of a saved view's filter that look at a run, at what a card waits for or at today: the agents that ran on
//! a card, its status and its due range. They are the board's own agent, status and due filters, so that a view made
//! from a board keeps them. `app::views` calls these for each card; they add nothing a person may not see, since the
//! cards they are asked about already passed `App::sight`.

use time::{Date, Duration, format_description::FormatItem, macros::format_description};

use super::{App, Result};
use crate::store::Card;

/// The statuses of a card, as the board names them: the first that applies.
pub const STATUSES: [&str; 7] = [
    "held",
    "live",
    "needs-you",
    "stuck",
    "blocked",
    "waiting",
    "idle",
];

/// The choices of a view's due range: `overdue`, `today`, `week` (today and the next seven days), `any` (has a due
/// date) and `none` (has none).
pub const DUE_RANGES: [&str; 5] = ["overdue", "today", "week", "any", "none"];

const DAY: &[FormatItem<'static>] = format_description!("[year]-[month]-[day]");

/// Whether `due` (`YYYY-MM-DD`, if the card has one) is in `range` as of `today`. An empty range asks nothing; a range
/// that is none of [`DUE_RANGES`] matches no card.
pub fn due_matches(due: Option<&str>, range: &str, today: &str) -> bool {
    match range {
        "" => true,
        "none" => due.is_none(),
        "any" => due.is_some(),
        "overdue" => due.is_some_and(|d| d < today),
        "today" => due == Some(today),
        "week" => {
            let Some(end) = Date::parse(today, DAY)
                .ok()
                .and_then(|d| d.checked_add(Duration::days(7)))
                .and_then(|d| d.format(DAY).ok())
            else {
                return false;
            };
            due.is_some_and(|d| d >= today && d <= end.as_str())
        }
        _ => false,
    }
}

impl App {
    /// What `card` is doing, as the board says it: held by a person or an agent, live (its latest run starts or runs),
    /// stuck or waiting for a person (an open inbox item), blocked by issues, waiting for a reason (`App::waiting`),
    /// or idle. One of [`STATUSES`].
    pub(super) fn card_status(&self, card: &Card) -> Result<&'static str> {
        if card.held.is_some() {
            return Ok("held");
        }
        let runs = self.store.runs_for_card(card.id)?;
        if runs
            .last()
            .is_some_and(|r| matches!(r.status.as_str(), "starting" | "running"))
        {
            return Ok("live");
        }
        let items = self.store.inbox_for_card(card.id)?;
        let open = || items.iter().filter(|i| i.resolved_at.is_none());
        if open().any(|i| i.kind == "stuck") {
            return Ok("stuck");
        }
        if open().next().is_some() {
            return Ok("needs-you");
        }
        if !self.blocked_by(card)?.is_empty() {
            return Ok("blocked");
        }
        if !self.waiting(std::slice::from_ref(card)).is_empty() {
            return Ok("waiting");
        }
        Ok("idle")
    }

    /// Whether an agent of `agents` (handles, in any case) ran on `card`: any of its runs, not only the latest.
    pub(super) fn ran_on(&self, card: &Card, agents: &[String]) -> Result<bool> {
        Ok(self
            .store
            .runs_for_card(card.id)?
            .iter()
            .any(|run| agents.iter().any(|a| a.eq_ignore_ascii_case(&run.agent))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_due_date_is_in_a_range_as_of_today() {
        let today = "2026-12-30";
        let matches = |due, range| due_matches(due, range, today);
        assert!(matches(Some("2026-12-29"), "overdue"));
        assert!(!matches(Some("2026-12-30"), "overdue"));
        assert!(!matches(None, "overdue"));
        assert!(matches(Some("2026-12-30"), "today"));
        assert!(!matches(Some("2026-12-31"), "today"));
        // A week is today and the seven days after it, across the end of the year.
        assert!(matches(Some("2026-12-30"), "week"));
        assert!(matches(Some("2027-01-06"), "week"));
        assert!(!matches(Some("2027-01-07"), "week"));
        assert!(!matches(Some("2026-12-29"), "week"));
        assert!(matches(Some("2020-01-01"), "any") && !matches(None, "any"));
        assert!(matches(None, "none") && !matches(Some("2026-12-30"), "none"));
        assert!(matches(None, "") && matches(Some("2026-12-30"), ""));
        assert!(!matches(Some("2026-12-30"), "soon"));
    }
}
