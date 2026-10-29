//! Attempts: the runs a card gets in one go when it is hard (`moebius.start_runs`), each in a worktree and on a branch
//! of its own, and what became of them: whether each passed the card's tests and checks, and which one was picked.
//! An attempt is a run with the purpose `attempt`; the one that is picked becomes a `code` run, so that everything
//! that builds on the card's latest `code` run builds on it. The others keep their branches.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// One attempt of a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub run_id: i64,
    pub card_id: i64,
    /// The attempts started together share this: the id of the first of them.
    pub group_id: i64,
    /// The model or model class the attempt was started with in place of its agent's, if one was given.
    pub model: Option<String>,
    /// `passed` or `failed` once the attempt was judged by the card's tests and checks.
    pub result: Option<String>,
    /// Why the attempt failed, as the end of the output of its tests or checks; empty for one that passed.
    pub output: String,
    /// Whether the attempt was picked.
    pub picked: bool,
    /// Why it was picked: what the selection run said, or that it was the only one that passed.
    pub reason: Option<String>,
    /// Whether the group has been settled: one picked, or none passed. A settled group is history.
    pub settled: bool,
    pub judged_at: Option<i64>,
    pub settled_at: Option<i64>,
}

const COLUMNS: &str = "run_id, card_id, group_id, model, result, output, picked, reason, settled, judged_at, settled_at";

fn attempt(r: &Row) -> rusqlite::Result<Attempt> {
    Ok(Attempt {
        run_id: r.get(0)?,
        card_id: r.get(1)?,
        group_id: r.get(2)?,
        model: r.get(3)?,
        result: r.get(4)?,
        output: r.get(5)?,
        picked: r.get(6)?,
        reason: r.get(7)?,
        settled: r.get(8)?,
        judged_at: r.get(9)?,
        settled_at: r.get(10)?,
    })
}

/// The schema of attempts.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE attempts (
    run_id INTEGER PRIMARY KEY REFERENCES runs(id),
    card_id INTEGER NOT NULL REFERENCES cards(id),
    group_id INTEGER NOT NULL,
    model TEXT,
    result TEXT,
    output TEXT NOT NULL DEFAULT '',
    picked INTEGER NOT NULL DEFAULT 0,
    reason TEXT,
    settled INTEGER NOT NULL DEFAULT 0,
    judged_at INTEGER,
    settled_at INTEGER
);
CREATE INDEX attempts_by_card ON attempts (card_id, group_id);
"#;

impl Store {
    /// Records that the run `run_id` is an attempt of the card, in the group `group_id`.
    pub fn add_attempt(
        &self,
        run_id: i64,
        card_id: i64,
        group_id: i64,
        model: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO attempts (run_id, card_id, group_id, model) VALUES (?1, ?2, ?3, ?4)",
            params![run_id, card_id, group_id, model],
        )?;
        Ok(())
    }

    pub fn attempt(&self, run_id: i64) -> Result<Option<Attempt>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM attempts WHERE run_id = ?1"),
                [run_id],
                attempt,
            )
            .optional()?)
    }

    /// Every attempt of the card, oldest first.
    pub fn attempts_for_card(&self, card_id: i64) -> Result<Vec<Attempt>> {
        self.rows(
            &format!("SELECT {COLUMNS} FROM attempts WHERE card_id = ?1 ORDER BY run_id"),
            [card_id],
            attempt,
        )
    }

    /// The attempts of the card that have not been settled, oldest first.
    pub fn open_attempts(&self, card_id: i64) -> Result<Vec<Attempt>> {
        self.rows(
            &format!(
                "SELECT {COLUMNS} FROM attempts WHERE card_id = ?1 AND settled = 0 ORDER BY run_id"
            ),
            [card_id],
            attempt,
        )
    }

    /// Records how the attempt fared against the card's tests and checks: `passed` or `failed`, and why.
    pub fn set_attempt_result(&self, run_id: i64, result: &str, output: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE attempts SET result = ?1, output = ?2, judged_at = ?3 WHERE run_id = ?4",
            params![result, output, now(), run_id],
        )?;
        Ok(())
    }

    /// Settles the card's open attempts. The attempt `picked` is picked for the reason `text` and becomes a `code`
    /// run, and the card's note, which its round was started with, is dealt with. Without one none passed, and `text`
    /// becomes the card's note, so that the next round is told. Returns the runs settled.
    pub fn settle_attempts(
        &self,
        card_id: i64,
        picked: Option<i64>,
        text: &str,
    ) -> Result<Vec<i64>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let at = now();
        let open: Vec<i64> = {
            let mut stmt =
                tx.prepare("SELECT run_id FROM attempts WHERE card_id = ?1 AND settled = 0")?;
            stmt.query_map([card_id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        tx.execute(
            "UPDATE attempts SET settled = 1, settled_at = ?1 WHERE card_id = ?2 AND settled = 0",
            params![at, card_id],
        )?;
        if let Some(run_id) = picked {
            tx.execute(
                "UPDATE attempts SET picked = 1, reason = ?1 WHERE run_id = ?2",
                params![text, run_id],
            )?;
            tx.execute(
                "UPDATE runs SET purpose = 'code' WHERE id = ?1 AND purpose = 'attempt'",
                [run_id],
            )?;
            tx.execute("UPDATE cards SET note = NULL WHERE id = ?1", [card_id])?;
        } else {
            tx.execute(
                "UPDATE cards SET note = ?1 WHERE id = ?2",
                params![text, card_id],
            )?;
        }
        tx.commit()?;
        Ok(open)
    }

    /// How many groups of attempts of the card since it was last retried were settled with none picked: rounds that
    /// failed, as a `code` run that failed is.
    pub fn failed_attempt_rounds(&self, card_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(DISTINCT group_id) FROM attempts
             WHERE card_id = ?1 AND settled = 1
               AND group_id > COALESCE((SELECT retried_after FROM cards WHERE id = ?1), 0)
               AND group_id NOT IN (SELECT group_id FROM attempts WHERE card_id = ?1 AND picked = 1)",
            [card_id],
            |r| r.get(0),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use crate::store::test_store as store;

    #[test]
    fn attempts_are_judged_and_settled_and_the_picked_one_becomes_a_code_run() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let card = s.add_card(repo.id, "Hard", "", "leon", "Backlog").unwrap();
        s.set_card_note(card.id, "Try again.").unwrap();
        let mut runs = Vec::new();
        for model in [Some("fast"), None, Some("strong")] {
            let run = s.add_run(Some(card.id), "Coder", "x", "attempt").unwrap();
            s.add_attempt(
                run.id,
                card.id,
                runs.first().unwrap_or(&run.id).to_owned(),
                model,
            )
            .unwrap();
            s.set_run_status(run.id, "succeeded").unwrap();
            runs.push(run.id);
        }
        assert_eq!(s.open_attempts(card.id).unwrap().len(), 3);
        assert_eq!(s.attempt(runs[1]).unwrap().unwrap().model, None);
        assert_eq!(
            s.code_branch(card.id).unwrap(),
            None,
            "attempts are no code run"
        );

        s.set_attempt_result(runs[0], "failed", "the test fails")
            .unwrap();
        s.set_attempt_result(runs[2], "passed", "").unwrap();
        let settled = s
            .settle_attempts(card.id, Some(runs[2]), "the only one")
            .unwrap();
        assert_eq!(settled, runs);
        assert!(s.open_attempts(card.id).unwrap().is_empty());
        let all = s.attempts_for_card(card.id).unwrap();
        assert!(all.iter().all(|a| a.settled && a.settled_at.is_some()));
        assert_eq!(all.iter().filter(|a| a.picked).count(), 1);
        assert_eq!(all[2].reason.as_deref(), Some("the only one"));
        assert_eq!(s.run(runs[2]).unwrap().unwrap().purpose, "code");
        assert_eq!(s.run(runs[0]).unwrap().unwrap().purpose, "attempt");
        assert_eq!(
            s.code_branch(card.id).unwrap(),
            Some(format!("moebius/run-{}", runs[2]))
        );
        assert_eq!(s.card(card.id).unwrap().unwrap().note, None);
        assert_eq!(s.failed_attempt_rounds(card.id).unwrap(), 0);
    }

    #[test]
    fn a_group_settled_with_none_picked_is_a_failed_round() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let card = s.add_card(repo.id, "Hard", "", "leon", "Backlog").unwrap();
        let run = s.add_run(Some(card.id), "Coder", "x", "attempt").unwrap();
        s.add_attempt(run.id, card.id, run.id, None).unwrap();
        s.settle_attempts(card.id, None, "None passed.").unwrap();
        assert_eq!(
            s.card(card.id).unwrap().unwrap().note.as_deref(),
            Some("None passed.")
        );
        assert_eq!(s.failed_attempt_rounds(card.id).unwrap(), 1);
        s.set_card_retried(card.id).unwrap();
        assert_eq!(s.failed_attempt_rounds(card.id).unwrap(), 0);
    }
}
