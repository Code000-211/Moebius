//! The Retro of each day (see `retro`), the changes to the harness configuration and to Moebius's own configuration it
//! proposes (see `own`), and the trials of them (see `trial`).

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// A day's Retro: its report (see `retro`), how far it got and what the analyst left out.
#[derive(Debug, Clone, PartialEq)]
pub struct Retro {
    /// The local day it is about, as `YYYY-MM-DD`.
    pub date: String,
    pub report: serde_json::Value,
    /// `reported`, `analysing`, `proposed` or `failed`.
    pub status: String,
    /// The analyst's run, if there was one.
    pub run_id: Option<i64>,
    pub warnings: Vec<String>,
    pub at: i64,
}

/// A change that a Retro proposes: to the harness configuration, on a branch of its own, or to Moebius's own
/// configuration (see `own`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: i64,
    /// `harness` (a branch of the harness configuration repository), `settings` (a change to `moebius.toml`) or
    /// `repository` (a change to a repository's `.moebius/` files).
    pub kind: String,
    pub retro_date: String,
    pub branch: String,
    /// The subject of the proposal's commit.
    pub title: String,
    /// The rest of the commit's message: what changes and which errors it addresses.
    pub body: String,
    /// The agents whose files the commit changes.
    pub agents: Vec<String>,
    /// The cards whose errors the proposal addresses, as the `Cards:` line ending its commit message names them.
    pub cards: Vec<i64>,
    /// `open`, `trying`, `positive`, `negative`, `untried`, `merged` or `dismissed`.
    pub status: String,
    /// The trials made of it, oldest first.
    pub trials: Vec<Trial>,
    /// What a `settings` or `repository` proposal changes, as a unified diff; empty for `harness`.
    pub diff: String,
    /// What a `settings` proposal sets and removes (`{"set", "remove"}`, as `PUT /v1/settings` takes it), or the new
    /// text of each file a `repository` proposal changes (`{"files": {path: text}}`); `{}` for `harness`.
    pub edits: serde_json::Value,
    /// The repository a `repository` proposal changes.
    pub repository: Option<i64>,
    /// The change a merged `repository` proposal was offered as.
    pub change: Option<i64>,
    pub at: i64,
}

/// What a proposal for Moebius's own configuration adds to a proposal (see `own`).
#[derive(Debug, Clone, PartialEq)]
pub struct OwnEdits {
    /// `settings` or `repository`.
    pub kind: String,
    pub diff: String,
    pub edits: serde_json::Value,
    pub repository: Option<i64>,
}

/// A proposal tried on one card (see `trial`): how it went and what was measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trial {
    pub id: i64,
    pub proposal_id: i64,
    /// The card it was tried on; none for the one trial of a proposal that names no cards.
    pub card_id: Option<i64>,
    /// The trial's run, if one was started.
    pub run_id: Option<i64>,
    /// `positive`, `negative`, `skipped` or `failed`.
    pub outcome: String,
    pub reason: String,
    pub measurements: serde_json::Value,
    pub at: i64,
}

impl Store {
    /// Stores the Retro of a day with `status`; false, and nothing stored, when the day has one already.
    pub fn add_retro(&self, date: &str, report: &serde_json::Value, status: &str) -> Result<bool> {
        let added = self.conn().execute(
            "INSERT INTO retros (date, report, status, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (date) DO NOTHING",
            params![date, serde_json::to_string(report)?, status, now()],
        )?;
        Ok(added == 1)
    }

    pub fn set_retro_run(&self, date: &str, run_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE retros SET run_id = ?1 WHERE date = ?2",
            params![run_id, date],
        )?;
        Ok(())
    }

    /// Ends the analysis of a day's Retro: its status, and what was left out or went wrong.
    pub fn finish_retro(&self, date: &str, status: &str, warnings: &[String]) -> Result<()> {
        self.conn().execute(
            "UPDATE retros SET status = ?1, warnings = ?2 WHERE date = ?3",
            params![status, serde_json::to_string(warnings)?, date],
        )?;
        Ok(())
    }

    /// Fails the Retros whose analysis the last server did not see through.
    pub fn fail_analysing_retros(&self, why: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE retros SET status = 'failed', warnings = json_array(?1) WHERE status = 'analysing'",
            [why],
        )?;
        Ok(())
    }

    fn retros_where(&self, clause: &str, param: &str) -> Result<Vec<Retro>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT date, report, status, run_id, warnings, at FROM retros WHERE {clause} ORDER BY date DESC"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get::<_, String>(4)?,
                r.get(5)?,
            ))
        })?;
        rows.map(|row| {
            let (date, report, status, run_id, warnings, at) = row?;
            Ok(Retro {
                date,
                report: serde_json::from_str(&report)?,
                status,
                run_id,
                warnings: serde_json::from_str(&warnings)?,
                at,
            })
        })
        .collect()
    }

    /// The Retro of a day, if it has one.
    pub fn retro(&self, date: &str) -> Result<Option<Retro>> {
        Ok(self.retros_where("date = ?1", date)?.pop())
    }

    /// All Retros, the latest day first.
    pub fn retros(&self) -> Result<Vec<Retro>> {
        self.retros_where("?1 = ?1", "")
    }

    pub fn add_proposal(
        &self,
        retro_date: &str,
        branch: &str,
        title: &str,
        body: &str,
        agents: &[String],
        cards: &[i64],
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO proposals (retro_date, branch, title, body, agents, cards, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                retro_date,
                branch,
                title,
                body,
                serde_json::to_string(agents)?,
                serde_json::to_string(cards)?,
                now()
            ],
        )?;
        Ok(())
    }

    /// Stores a proposal for Moebius's own configuration, which is never tried, so it is `untried` from the start, and
    /// returns its id.
    pub fn add_own_proposal(
        &self,
        retro_date: &str,
        branch: &str,
        title: &str,
        body: &str,
        cards: &[i64],
        own: &OwnEdits,
    ) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO proposals (retro_date, branch, title, body, cards, status, kind, diff, edits, repository_id, at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'untried', ?6, ?7, ?8, ?9, ?10)",
            params![
                retro_date,
                branch,
                title,
                body,
                serde_json::to_string(cards)?,
                own.kind,
                own.diff,
                serde_json::to_string(&own.edits)?,
                own.repository,
                now()
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Records the change a `repository` proposal was offered as.
    pub fn set_proposal_change(&self, id: i64, change_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE proposals SET change_id = ?1 WHERE id = ?2",
            params![change_id, id],
        )?;
        Ok(())
    }

    fn proposals_where(
        &self,
        clause: &str,
        param: rusqlite::types::Value,
    ) -> Result<Vec<Proposal>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, retro_date, branch, title, body, agents, cards, status, at, kind, diff, edits, repository_id,
             change_id FROM proposals WHERE {clause} ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok((
                Proposal {
                    id: r.get(0)?,
                    kind: r.get(9)?,
                    retro_date: r.get(1)?,
                    branch: r.get(2)?,
                    title: r.get(3)?,
                    body: r.get(4)?,
                    agents: vec![],
                    cards: vec![],
                    status: r.get(7)?,
                    trials: vec![],
                    diff: r.get(10)?,
                    edits: serde_json::Value::Null,
                    repository: r.get(12)?,
                    change: r.get(13)?,
                    at: r.get(8)?,
                },
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(11)?,
            ))
        })?;
        let mut stmt = conn.prepare(
            "SELECT id, proposal_id, card_id, run_id, outcome, reason, measurements, at FROM trials
             WHERE proposal_id = ?1 ORDER BY id",
        )?;
        rows.map(|row| {
            let (proposal, agents, cards, edits) = row?;
            let trials = stmt.query_map([proposal.id], |r| {
                Ok((
                    Trial {
                        id: r.get(0)?,
                        proposal_id: r.get(1)?,
                        card_id: r.get(2)?,
                        run_id: r.get(3)?,
                        outcome: r.get(4)?,
                        reason: r.get(5)?,
                        measurements: serde_json::Value::Null,
                        at: r.get(7)?,
                    },
                    r.get::<_, String>(6)?,
                ))
            })?;
            let trials = trials
                .map(|row| {
                    let (trial, measurements) = row?;
                    Ok(Trial {
                        measurements: serde_json::from_str(&measurements)?,
                        ..trial
                    })
                })
                .collect::<Result<_>>()?;
            Ok(Proposal {
                agents: serde_json::from_str(&agents)?,
                cards: serde_json::from_str(&cards)?,
                edits: serde_json::from_str(&edits)?,
                trials,
                ..proposal
            })
        })
        .collect()
    }

    /// The proposals of one Retro (`Some(date)`) or of all, the newest first, with their trials.
    pub fn proposals(&self, retro_date: Option<&str>) -> Result<Vec<Proposal>> {
        match retro_date {
            Some(date) => self.proposals_where("retro_date = ?1", date.to_string().into()),
            None => self.proposals_where("?1", 1.into()),
        }
    }

    pub fn proposal(&self, id: i64) -> Result<Option<Proposal>> {
        Ok(self.proposals_where("id = ?1", id.into())?.pop())
    }

    /// Sets the status of a proposal.
    pub fn set_proposal_status(&self, id: i64, status: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE proposals SET status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    /// Starts the trying of an open proposal; false, and nothing changed, when it is not open or another proposal is
    /// being tried, since only one is at a time.
    pub fn start_trying(&self, id: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE proposals SET status = 'trying'
             WHERE id = ?1 AND status = 'open' AND NOT EXISTS (SELECT 1 FROM proposals WHERE status = 'trying')",
            [id],
        )? == 1)
    }

    /// Opens the proposals the last server was trying, without the trials it had made of them.
    pub fn reopen_trying_proposals(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM trials WHERE proposal_id IN (SELECT id FROM proposals WHERE status = 'trying')",
            [],
        )?;
        conn.execute(
            "UPDATE proposals SET status = 'open' WHERE status = 'trying'",
            [],
        )?;
        Ok(())
    }

    pub fn add_trial(
        &self,
        proposal_id: i64,
        card_id: Option<i64>,
        run_id: Option<i64>,
        outcome: &str,
        reason: &str,
        measurements: &serde_json::Value,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO trials (proposal_id, card_id, run_id, outcome, reason, measurements, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                proposal_id,
                card_id,
                run_id,
                outcome,
                reason,
                serde_json::to_string(measurements)?,
                now()
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::store::test_store as store;

    #[test]
    fn retros_and_their_proposals_round_trip_and_a_day_has_one_retro() {
        let (_dir, s) = store();
        let report = json!({ "date": "2026-03-10", "errors": [] });
        assert!(s.add_retro("2026-03-10", &report, "analysing").unwrap());
        assert!(!s.add_retro("2026-03-10", &json!({}), "reported").unwrap());
        assert!(s.add_retro("2026-03-11", &report, "reported").unwrap());
        s.finish_retro("2026-03-10", "proposed", &["a warning".to_string()])
            .unwrap();
        s.add_proposal(
            "2026-03-10",
            "proposal/2026-03-10-x",
            "Title",
            "Body",
            &["Coder".to_string()],
            &[48, 50],
        )
        .unwrap();
        let retro = s.retro("2026-03-10").unwrap().unwrap();
        assert_eq!(
            (retro.status.as_str(), retro.warnings),
            ("proposed", vec!["a warning".to_string()])
        );
        assert_eq!(retro.report, report);
        let dates: Vec<String> = s.retros().unwrap().into_iter().map(|r| r.date).collect();
        assert_eq!(dates, ["2026-03-11", "2026-03-10"]);
        let proposals = s.proposals(None).unwrap();
        assert_eq!(proposals, s.proposals(Some("2026-03-10")).unwrap());
        assert_eq!(
            (proposals[0].status.as_str(), &proposals[0].agents[..]),
            ("open", &["Coder".to_string()][..])
        );
        assert_eq!(proposals[0].cards, [48, 50]);
        assert!(proposals[0].trials.is_empty());
        assert!(s.proposals(Some("2026-03-11")).unwrap().is_empty());
        s.add_retro("2026-03-12", &report, "analysing").unwrap();
        s.fail_analysing_retros("stopped").unwrap();
        assert_eq!(s.retro("2026-03-12").unwrap().unwrap().status, "failed");
        assert_eq!(s.retro("2026-03-11").unwrap().unwrap().status, "reported");
    }

    #[test]
    fn a_proposal_for_moebius_itself_keeps_its_diff_and_edits_and_is_untried() {
        let (_dir, s) = store();
        s.add_retro("2026-03-10", &json!({}), "analysing").unwrap();
        s.add_proposal("2026-03-10", "proposal/2026-03-10-h", "H", "", &[], &[])
            .unwrap();
        let own = super::OwnEdits {
            kind: "settings".into(),
            diff: "-a\n+b\n".into(),
            edits: json!({ "set": { "board.round_limit": 7 }, "remove": [] }),
            repository: None,
        };
        let id = s
            .add_own_proposal(
                "2026-03-10",
                "proposal/2026-03-10-s",
                "S",
                "Why.",
                &[3],
                &own,
            )
            .unwrap();
        let proposal = s.proposal(id).unwrap().unwrap();
        assert_eq!(
            (proposal.kind.as_str(), proposal.status.as_str()),
            ("settings", "untried")
        );
        assert_eq!(
            (proposal.diff.as_str(), &proposal.edits),
            (own.diff.as_str(), &own.edits)
        );
        assert_eq!((proposal.cards, proposal.change), (vec![3], None));
        // A proposal of the harness configuration is what it was.
        let harness = s.proposal(id - 1).unwrap().unwrap();
        assert_eq!(
            (harness.kind.as_str(), harness.diff.as_str(), &harness.edits),
            ("harness", "", &json!({}))
        );
    }

    #[test]
    fn one_proposal_is_tried_at_a_time_and_a_trial_that_was_not_seen_through_is_forgotten() {
        let (_dir, s) = store();
        s.add_retro("2026-03-10", &json!({}), "proposed").unwrap();
        for branch in ["a", "b"] {
            s.add_proposal("2026-03-10", branch, branch, "", &[], &[])
                .unwrap();
        }
        assert!(s.start_trying(1).unwrap());
        // Another one is being tried, and this one is not open any more.
        assert!(!s.start_trying(2).unwrap());
        assert!(!s.start_trying(1).unwrap());
        s.add_trial(1, None, None, "skipped", "no cards", &json!({ "a": 1 }))
            .unwrap();
        let trials = s.proposal(1).unwrap().unwrap().trials;
        assert_eq!(
            (trials[0].outcome.as_str(), &trials[0].measurements),
            ("skipped", &json!({ "a": 1 }))
        );
        s.reopen_trying_proposals().unwrap();
        let proposal = s.proposal(1).unwrap().unwrap();
        assert_eq!(proposal.status, "open");
        assert!(proposal.trials.is_empty());
        assert!(s.start_trying(2).unwrap());
        assert!(s.proposal(3).unwrap().is_none());
    }
}
