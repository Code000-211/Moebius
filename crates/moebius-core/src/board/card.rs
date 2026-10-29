//! The `card` table a script is given, and what it says of the card's review and change:
//!
//! - `card.blocked_by`: the numbers of the issues blocking the card that are not done yet (an issue with no card counts
//!   as not done); empty when nothing blocks.
//! - `card.review`: the verdict and findings of the review since the card's latest `code` run, if there is one.
//! - `card.note`: why the card went back to coding, if it did and no `code` run has succeeded since.
//! - `card.change`: the card's open change: its checks, the failing ones and whether it still merges cleanly.
//! - `card.stopped`: whether a person stopped the card's latest run and has not retried the card since.
//! - `card.rounds`, `card.reviews`, `card.tokens`: the `code` runs, `review` runs and tokens of the card since it was
//!   last retried.

use mlua::{Lua, Table};

use super::Engine;
use crate::app::{App, runs_since_retry};
use crate::store::{Card, Run};

/// What a review run concluded: `verdict` is `approve` or `changes`, and `findings` is what it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    pub verdict: &'static str,
    pub findings: String,
}

/// Reads the verdict from the end of a review run's summary: a last line `VERDICT: approve` or `VERDICT: changes`,
/// with the findings before it. A summary without a verdict, or with another one, asks for changes, because work that
/// was not reviewed is not approved.
fn verdict(summary: Option<&str>) -> Review {
    let text = summary.unwrap_or("").trim_end();
    let (findings, last) = text.rsplit_once('\n').unwrap_or(("", text));
    let said = last
        .trim()
        .strip_prefix("VERDICT:")
        .map(|v| v.trim().to_ascii_lowercase());
    let (verdict, findings) = match said.as_deref() {
        Some("approve") => ("approve", findings.trim()),
        Some("changes") if !findings.trim().is_empty() => ("changes", findings.trim()),
        Some("changes") => (
            "changes",
            "The review asked for changes but gave no findings.",
        ),
        _ => ("changes", "The review gave no verdict."),
    };
    Review {
        verdict,
        findings: findings.into(),
    }
}

/// The review that counts: the latest finished one after the latest `code` run.
pub fn latest_review(runs: &[Run]) -> Option<Review> {
    let mut since_code = runs.iter().rev().take_while(|r| r.purpose != "code");
    let run = since_code.find(|r| r.purpose == "review" && r.status == "succeeded")?;
    Some(verdict(run.summary.as_deref()))
}

impl Engine {
    /// The `card` table of `card`, as its script sees it.
    pub(super) fn card_table(&self, lua: &Lua, card: &Card) -> mlua::Result<Table> {
        let runs = self
            .app
            .store
            .runs_for_card(card.id)
            .map_err(mlua::Error::external)?;
        let t = lua.create_table()?;
        t.set("id", card.id)?;
        t.set("number", card.number)?;
        t.set("title", card.title.as_str())?;
        t.set("body", card.body.as_str())?;
        t.set("author", card.author.as_str())?;
        t.set("column", card.column.as_str())?;
        t.set("note", card.note.as_deref())?;
        t.set("change", change_table(&self.app, lua, card.id)?)?;
        t.set("trusted", self.app.is_trusted(card))?;
        t.set("confirmed", card.confirmed)?;
        let blocked_by = self.app.blocked_by(card).map_err(mlua::Error::external)?;
        t.set("blocked_by", blocked_by)?;
        let since_retry = || runs_since_retry(&runs, card);
        let count = |purpose: &str| since_retry().filter(|r| r.purpose == purpose).count();
        t.set("rounds", count("code"))?;
        t.set("reviews", count("review"))?;
        t.set(
            "tokens",
            self.app
                .store
                .tokens_for_card(card.id)
                .map_err(mlua::Error::external)?,
        )?;
        t.set(
            "stopped",
            runs.last()
                .is_some_and(|r| r.status == "stopped" && Some(r.id) > card.retried_after),
        )?;
        if let Some(run) = runs.last() {
            let r = lua.create_table()?;
            r.set("id", run.id)?;
            r.set("agent", run.agent.as_str())?;
            r.set("purpose", run.purpose.as_str())?;
            r.set("status", run.status.as_str())?;
            r.set("summary", run.summary.as_deref())?;
            t.set("run", r)?;
        }
        if let Some(review) = latest_review(&runs) {
            let v = lua.create_table()?;
            v.set("verdict", review.verdict)?;
            v.set("findings", review.findings)?;
            t.set("review", v)?;
        }
        Ok(t)
    }
}

/// The card's open change as scripts see it, or nothing when the card has none. How its checks stand is for its current
/// commit; what failed is left out when that is of an older one.
pub(super) fn change_table(app: &App, lua: &Lua, card_id: i64) -> mlua::Result<Option<Table>> {
    let Some(change) = app
        .store
        .open_change(card_id)
        .map_err(mlua::Error::external)?
    else {
        return Ok(None);
    };
    let repository = change
        .repository_id
        .and_then(|id| app.store.repository(id).transpose())
        .ok_or_else(|| mlua::Error::runtime("the change has no repository"))?
        .map_err(mlua::Error::external)?;
    let current = change.checks_head == change.head;
    let c = lua.create_table()?;
    c.set("branch", change.branch.as_str())?;
    c.set("base", repository.base_ref())?;
    c.set("pull_request", change.pull_request)?;
    c.set(
        "checks",
        app.checks_of(&change).map_err(mlua::Error::external)?,
    )?;
    c.set(
        "failed_checks",
        change.failed_checks.as_deref().filter(|_| current),
    )?;
    c.set(
        "check_output",
        change.check_output.as_deref().filter(|_| current),
    )?;
    c.set(
        "conflict",
        app.conflict_of(&change).map_err(mlua::Error::external)?,
    )?;
    Ok(Some(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_review_run_is_read_for_its_verdict() {
        let review = |text: &str| {
            let r = verdict(Some(text));
            (r.verdict, r.findings)
        };
        assert_eq!(
            review("Good.\nVERDICT: approve\n"),
            ("approve", "Good.".into())
        );
        assert_eq!(review("VERDICT: approve"), ("approve", "".into()));
        assert_eq!(
            review("Line one\nLine two\n  VERDICT:  Changes  "),
            ("changes", "Line one\nLine two".into())
        );
        assert_eq!(
            review("VERDICT: changes").1,
            "The review asked for changes but gave no findings."
        );
        for text in [
            "",
            "All good",
            "VERDICT: approve\nand more",
            "VERDICT: maybe",
        ] {
            assert_eq!(
                review(text),
                ("changes", "The review gave no verdict.".into()),
                "{text:?}"
            );
        }
        assert_eq!(verdict(None).verdict, "changes");
    }
}
