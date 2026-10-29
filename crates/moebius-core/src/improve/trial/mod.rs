//! Trials (spec 7, ADR 0001, ADR 0008): a proposal of the Retro (see `retro`) is tried on the tasks of the errors it
//! addresses before a person is asked to merge it. For each card its `Cards:` line names, the card's agent runs the
//! card's task again (a run with the purpose `trial`) in a fresh worktree of the card's repository, on a branch of its
//! own, with the agent's definition read from the proposal's branch instead of the default branch. Afterwards the
//! repository's own checks run on what the agent made. A trial is positive when the errors the proposal addresses are
//! gone and nothing got worse; the trial's branches and worktrees are removed and only the stored result stays.
//!
//! A proposal is `open` until it is tried, `trying` while it is, and then `positive` (every trial was), `negative`
//! (some trial was) or `untried` (the rest: some trial was skipped or failed, so nobody knows). A positive or untried
//! proposal waits in the inbox for a person to merge or dismiss it; a negative one stays on the Retro page. The
//! schedule asks `on_schedule` once a minute, and trials only start inside the trial window; `POST
//! /v1/proposals/{id}/trial` tries a proposal at any hour. Nothing here waits for a person.
//!
//! This module holds the trial window and what the parts share; `verdict` has what a trial measured and whether it is
//! positive, `card` one card's trial, `schedule` trying a proposal and when it starts, and `decision` a person's merge
//! or dismissal of a tried proposal.

mod card;
mod decision;
mod schedule;
mod verdict;

use serde_json::json;

use crate::app::{App, AppError, Result};
use crate::bus::AppEvent;
use crate::config::TimeOfDay;
use crate::store::Proposal;

pub use decision::{dismiss, merge};
pub use schedule::{on_schedule, start};
pub use verdict::{Measured, Original, compare};

/// Whether `now` is inside the trial window: from its start up to its end, which may be on the next day. A window that
/// ends when it starts is empty.
pub fn in_window(now: TimeOfDay, (start, end): (TimeOfDay, TimeOfDay)) -> bool {
    if start.0 <= end.0 {
        start.0 <= now.0 && now.0 < end.0
    } else {
        now.0 >= start.0 || now.0 < end.0
    }
}

/// The proposal `id`, or `NotFound`.
fn find(app: &App, id: i64) -> Result<Proposal> {
    app.store
        .proposal(id)?
        .ok_or_else(|| AppError::NotFound(format!("proposal {id}")))
}

fn publish_status(app: &App, proposal: i64, status: &str) {
    app.bus.publish(AppEvent::new(
        "proposal_status",
        json!({ "proposal": proposal, "status": status }),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u16, minute: u16) -> TimeOfDay {
        TimeOfDay(hour * 60 + minute)
    }

    #[test]
    fn the_trial_window_may_cross_midnight() {
        let day = (at(0, 0), at(6, 0));
        assert!(in_window(at(0, 0), day));
        assert!(in_window(at(5, 59), day));
        assert!(!in_window(at(6, 0), day));
        assert!(!in_window(at(23, 59), day));
        let night = (at(22, 0), at(4, 30));
        assert!(in_window(at(22, 0), night));
        assert!(in_window(at(23, 59), night));
        assert!(in_window(at(0, 0), night));
        assert!(in_window(at(4, 29), night));
        assert!(!in_window(at(4, 30), night));
        assert!(!in_window(at(12, 0), night));
        assert!(!in_window(at(21, 59), night));
        // A window that ends when it starts is empty.
        assert!(!in_window(at(3, 0), (at(3, 0), at(3, 0))));
    }
}
