//! Judgements: the choices recorded at decision points (ADR 0010), their outcomes, and the refusals counted by rule.

use std::collections::BTreeMap;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};
use crate::blobs::BlobId;

/// One recorded choice at a decision point (ADR 0010).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Judgement {
    pub id: i64,
    pub kind: String,
    pub context: Vec<BlobId>,
    pub options: Vec<String>,
    pub choice: String,
    pub reason: String,
    pub decider: String,
    pub run_id: Option<i64>,
    pub card_id: Option<i64>,
    pub outcome: Option<String>,
    pub at: i64,
}

/// What a judgement is about, before it is recorded.
#[derive(Debug, Clone, Default)]
pub struct NewJudgement {
    pub kind: String,
    pub context: Vec<BlobId>,
    pub options: Vec<String>,
    pub choice: String,
    pub reason: String,
    pub decider: String,
    pub run_id: Option<i64>,
    pub card_id: Option<i64>,
}

/// What follows `FROM judgements` in `open_judgements_of_run`.
const OPEN_OF_RUN_TAIL: &str = "WHERE run_id = ?1 AND kind = ?2 AND outcome IS NULL ORDER BY id";

impl Store {
    pub fn add_judgement(&self, j: &NewJudgement) -> Result<Judgement> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO judgements (kind, context, options, choice, reason, decider, run_id, card_id, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                j.kind,
                serde_json::to_string(&j.context)?,
                serde_json::to_string(&j.options)?,
                j.choice,
                j.reason,
                j.decider,
                j.run_id,
                j.card_id,
                at
            ],
        )?;
        Ok(Judgement {
            id: conn.last_insert_rowid(),
            kind: j.kind.clone(),
            context: j.context.clone(),
            options: j.options.clone(),
            choice: j.choice.clone(),
            reason: j.reason.clone(),
            decider: j.decider.clone(),
            run_id: j.run_id,
            card_id: j.card_id,
            outcome: None,
            at,
        })
    }

    fn judgements_where(
        &self,
        clause: &str,
        param: rusqlite::types::Value,
    ) -> Result<Vec<Judgement>> {
        self.judgements_query(&format!("WHERE {clause} ORDER BY id"), [param])
    }

    /// The judgements, newest first, of the card, of the run and of the kind if given; at most `limit`.
    pub fn judgements_page(
        &self,
        card_id: Option<i64>,
        run_id: Option<i64>,
        kind: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Judgement>> {
        self.judgements_query(
            "WHERE (?1 IS NULL OR card_id = ?1) AND (?2 IS NULL OR run_id = ?2) AND (?3 IS NULL OR kind = ?3)
             ORDER BY id DESC LIMIT ?4",
            params![card_id, run_id, kind, limit],
        )
    }

    /// The judgements of the kind made in the run that have no outcome yet, oldest first; served by `judgements_by_run`.
    pub fn open_judgements_of_run(&self, run_id: i64, kind: &str) -> Result<Vec<Judgement>> {
        self.judgements_query(OPEN_OF_RUN_TAIL, params![run_id, kind])
    }

    pub(super) fn judgements_query(
        &self,
        tail: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<Judgement>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, kind, context, options, choice, reason, decider, run_id, card_id, outcome, at
             FROM judgements {tail}"
        ))?;
        let rows = stmt.query_map(params, |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, i64>(10)?,
            ))
        })?;
        rows.map(|row| {
            let (id, kind, context, options, choice, reason, decider, run_id, card_id, outcome, at) = row?;
            Ok(Judgement {
                id,
                kind,
                context: serde_json::from_str(&context)?,
                options: serde_json::from_str(&options)?,
                choice,
                reason,
                decider,
                run_id,
                card_id,
                outcome,
                at,
            })
        })
        .collect()
    }

    /// The judgements of `kind` with an id above `after`, oldest first; at most `limit`.
    pub fn judgements_of_kind_after(
        &self,
        kind: &str,
        after: i64,
        limit: i64,
    ) -> Result<Vec<Judgement>> {
        self.judgements_query(
            "WHERE kind = ?1 AND id > ?2 ORDER BY id LIMIT ?3",
            params![kind, after, limit],
        )
    }

    /// The judgements of `kind` that have an outcome, newest first; at most `limit`.
    pub fn judgements_with_outcome(&self, kind: &str, limit: i64) -> Result<Vec<Judgement>> {
        self.judgements_query(
            "WHERE kind = ?1 AND outcome IS NOT NULL ORDER BY id DESC LIMIT ?2",
            params![kind, limit],
        )
    }

    /// Calls `each` with the kind, the id, whether there is an outcome and the decider of every judgement, oldest
    /// first, without reading them all into memory.
    pub fn each_judgement(&self, mut each: impl FnMut(&str, i64, bool, &str)) -> Result<()> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT kind, id, outcome IS NOT NULL, decider FROM judgements ORDER BY id")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            each(
                &row.get::<_, String>(0)?,
                row.get(1)?,
                row.get(2)?,
                &row.get::<_, String>(3)?,
            );
        }
        Ok(())
    }

    pub fn judgement(&self, id: i64) -> Result<Option<Judgement>> {
        Ok(self.judgements_where("id = ?1", id.into())?.pop())
    }

    pub fn judgements(&self, kind: &str) -> Result<Vec<Judgement>> {
        self.judgements_where("kind = ?1", kind.to_string().into())
    }

    pub fn judgements_for_card(&self, card_id: i64) -> Result<Vec<Judgement>> {
        self.judgements_where("card_id = ?1", card_id.into())
    }

    /// Whether a judgement made for the card still waits for the outcome that `set_missing_outcomes` sets. The hints
    /// of `localize` are left out: their outcome is the files a change touched, which a failed run never reports.
    pub fn has_pending_outcomes(&self, card_id: i64) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM judgements WHERE card_id = ?1 AND outcome IS NULL AND kind <> 'localize')",
            [card_id],
            |r| r.get(0),
        )?)
    }

    pub fn set_outcome(&self, judgement_id: i64, outcome: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE judgements SET outcome = ?1 WHERE id = ?2",
            params![outcome, judgement_id],
        )?;
        Ok(())
    }

    /// Sets `outcome` on every judgement made for the card that has none yet, except the hints of `localize`, whose
    /// outcome is the files the change touched; returns how many were set.
    pub fn set_missing_outcomes(&self, card_id: i64, outcome: &str) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE judgements SET outcome = ?1 WHERE card_id = ?2 AND outcome IS NULL AND kind <> 'localize'",
            params![outcome, card_id],
        )?)
    }

    /// Sets `outcome` on every judgement of `kind` made in the run that has none yet; returns how many were set.
    pub fn set_run_missing_outcomes(
        &self,
        run_id: i64,
        kind: &str,
        outcome: &str,
    ) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE judgements SET outcome = ?1 WHERE run_id = ?2 AND kind = ?3 AND outcome IS NULL",
            params![outcome, run_id, kind],
        )?)
    }

    /// How many tool calls the guard refused since `since` because of each rule, by the rule's name. A pattern and a
    /// rule in prose that the teacher names both write their reason as `<rule>: <why>`; a refusal without a rule
    /// (the teacher's own failures, a rules file that cannot be read, a missing teacher) is not counted.
    pub fn refusals_by_rule(&self, since: i64) -> Result<BTreeMap<String, i64>> {
        Ok(self.rows("SELECT substr(reason, 1, instr(reason, ': ') - 1) AS rule, COUNT(*) FROM judgements
             WHERE kind = 'tool_call' AND choice = 'refuse' AND decider IN ('guard/patterns', 'guard/teacher')
               AND at >= ?1 AND instr(reason, ': ') > 1 AND rule <> 'teacher'
               AND reason NOT LIKE 'the rules cannot be read%'
             GROUP BY rule", [since], |r| Ok((r.get(0)?, r.get(1)?)))?
            .into_iter()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store as store;

    #[test]
    fn an_unfinished_localize_hint_is_not_a_pending_outcome() {
        let (_dir, s) = store();
        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        let card = s.add_card(repo.id, "t", "b", "me", "Backlog").unwrap();
        let judge = |kind: &str| {
            s.add_judgement(&NewJudgement {
                kind: kind.into(),
                card_id: Some(card.id),
                ..Default::default()
            })
            .unwrap()
        };
        judge("localize");
        assert!(!s.has_pending_outcomes(card.id).unwrap());
        judge("tool_call");
        assert!(s.has_pending_outcomes(card.id).unwrap());
        s.set_missing_outcomes(card.id, "done").unwrap();
        assert!(!s.has_pending_outcomes(card.id).unwrap());
    }

    #[test]
    fn the_hot_queries_use_their_indexes() {
        let (_dir, s) = store();
        let plan = |sql: &str| -> String {
            let conn = s.conn();
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let rows: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(3))
                .unwrap()
                .map(|row| row.unwrap())
                .collect();
            rows.join("; ")
        };
        let open_of_run = format!("SELECT id FROM judgements {OPEN_OF_RUN_TAIL}")
            .replace("?1", "1")
            .replace("?2", "'x'");
        let window = format!(
            "SELECT id FROM usage WHERE at >= {} AND model = 'm'",
            crate::store::usage::WINDOW_START
        )
        .replace("?1", "1");
        for (sql, index) in [
            (open_of_run.as_str(), "judgements_by_run"),
            (window.as_str(), "usage_by_model"),
            (
                "SELECT id FROM judgements WHERE card_id = 1",
                "judgements_by_card",
            ),
            (
                "SELECT id FROM judgements WHERE run_id = 1 AND kind = 'x'",
                "judgements_by_run",
            ),
            ("SELECT id FROM runs WHERE card_id = 1", "runs_by_card"),
            ("SELECT id FROM usage WHERE at >= 5", "usage_by_at"),
            (
                "SELECT id FROM usage WHERE model = 'm' AND at >= 5",
                "usage_by_model",
            ),
            (
                "SELECT MAX(seq) FROM events WHERE run_id = 1 AND kind = 'x'",
                "events_by_kind",
            ),
        ] {
            assert!(plan(sql).contains(index), "{sql}: {}", plan(sql));
        }
    }

    #[test]
    fn refusals_are_counted_by_the_rule_named_in_the_reason() {
        let (_dir, s) = store();
        let judge = |kind: &str, choice: &str, reason: &str, decider: &str| {
            s.add_judgement(&NewJudgement {
                kind: kind.into(),
                choice: choice.into(),
                reason: reason.into(),
                decider: decider.into(),
                ..Default::default()
            })
            .unwrap();
        };
        judge(
            "tool_call",
            "refuse",
            "no-force-push: it rewrites",
            "guard/patterns",
        );
        judge(
            "tool_call",
            "refuse",
            "no-force-push: again",
            "guard/patterns",
        );
        judge(
            "tool_call",
            "refuse",
            "leave-ci-alone: not .github",
            "guard/teacher",
        );
        judge(
            "tool_call",
            "refuse",
            "teacher: the answer is not JSON",
            "guard/teacher",
        );
        judge(
            "tool_call",
            "refuse",
            "the rules cannot be read, so no call is allowed: x: y",
            "guard/patterns",
        );
        judge(
            "tool_call",
            "refuse",
            "no teacher is configured to check the rules in prose",
            "guard/teacher",
        );
        judge(
            "tool_call",
            "refuse",
            "no-force-push: by a model",
            "decision-model",
        );
        judge(
            "tool_call",
            "allow",
            "no-force-push: allowed",
            "guard/patterns",
        );
        judge(
            "other",
            "refuse",
            "no-force-push: other kind",
            "guard/patterns",
        );
        let counted = s.refusals_by_rule(0).unwrap();
        assert_eq!(
            counted,
            BTreeMap::from([
                ("leave-ci-alone".to_string(), 1),
                ("no-force-push".to_string(), 2)
            ])
        );
        assert!(s.refusals_by_rule(now() + 10).unwrap().is_empty());
    }
}
