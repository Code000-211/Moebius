//! The decision model's record (see `decision_model`): its shadows of held-out judgements, how well they agree, and the
//! takeovers proposed from them.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// A decision model's answer to a held-out judgement's question (see `decision_model`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewShadow {
    pub judgement_id: i64,
    pub kind: String,
    /// The model's choice; none when it failed.
    pub choice: Option<String>,
    /// The model's reason, or why it failed.
    pub reason: String,
    pub failed: bool,
}

/// How well the decision model agrees with the recorded choices of one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowStats {
    pub kind: String,
    pub shadows: i64,
    /// The shadows whose choice is the recorded one.
    pub agreed: i64,
    /// The shadows where the model failed; they count as disagreement.
    pub failed: i64,
}

impl ShadowStats {
    /// The share of the shadows that agreed; none before there is one.
    pub fn agreement(&self) -> Option<f64> {
        (self.shadows > 0).then(|| self.agreed as f64 / self.shadows as f64)
    }
}

/// A proposal that the decision model take over a kind of decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Takeover {
    pub id: i64,
    pub kind: String,
    /// The agreement measured when it was proposed, between 0 and 1.
    pub agreement: f64,
    /// How many shadows it was measured on.
    pub count: i64,
    /// `open`, `merged`, `dismissed` or `released`.
    pub status: String,
    pub at: i64,
}

impl Store {
    pub fn add_shadow(&self, shadow: &NewShadow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO shadows (judgement_id, kind, choice, reason, failed, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                shadow.judgement_id,
                shadow.kind,
                shadow.choice,
                shadow.reason,
                shadow.failed,
                now()
            ],
        )?;
        Ok(())
    }

    /// What the shadows of each kind, or only of `kind`, say about the model, kinds in alphabetical order.
    pub fn shadow_stats(&self, kind: Option<&str>) -> Result<Vec<ShadowStats>> {
        self.rows("SELECT s.kind, COUNT(*), COALESCE(SUM(s.failed = 0 AND s.choice = j.choice), 0), COALESCE(SUM(s.failed), 0)
             FROM shadows s JOIN judgements j ON j.id = s.judgement_id
             WHERE ?1 IS NULL OR s.kind = ?1 GROUP BY s.kind ORDER BY s.kind", [kind], |r| {
            Ok(ShadowStats {
                kind: r.get(0)?,
                shadows: r.get(1)?,
                agreed: r.get(2)?,
                failed: r.get(3)?,
            })
        })
    }

    pub fn add_takeover(&self, kind: &str, agreement: f64, count: i64) -> Result<Takeover> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO takeovers (kind, agreement, count, at) VALUES (?1, ?2, ?3, ?4)",
            params![kind, agreement, count, at],
        )?;
        Ok(Takeover {
            id: conn.last_insert_rowid(),
            kind: kind.into(),
            agreement,
            count,
            status: "open".into(),
            at,
        })
    }

    /// The takeovers, oldest first.
    pub fn takeovers(&self) -> Result<Vec<Takeover>> {
        self.rows(
            "SELECT id, kind, agreement, count, status, at FROM takeovers ORDER BY id",
            [],
            |r| {
                Ok(Takeover {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    agreement: r.get(2)?,
                    count: r.get(3)?,
                    status: r.get(4)?,
                    at: r.get(5)?,
                })
            },
        )
    }

    pub fn takeover(&self, id: i64) -> Result<Option<Takeover>> {
        Ok(self.takeovers()?.into_iter().find(|t| t.id == id))
    }

    /// Gives a takeover that has the status `from` the status `to`; false, and nothing changed, when it has another.
    pub fn move_takeover(&self, id: i64, from: &str, to: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE takeovers SET status = ?1 WHERE id = ?2 AND status = ?3",
            params![to, id, from],
        )? == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{NewJudgement, test_store as store};

    #[test]
    fn shadows_are_counted_per_kind_and_a_failure_disagrees() {
        let (_dir, s) = store();
        let judge = |kind: &str, choice: &str| {
            s.add_judgement(&NewJudgement {
                kind: kind.into(),
                choice: choice.into(),
                ..Default::default()
            })
            .unwrap()
            .id
        };
        let shadow = |id: i64, kind: &str, choice: Option<&str>| {
            s.add_shadow(&NewShadow {
                judgement_id: id,
                kind: kind.into(),
                choice: choice.map(String::from),
                reason: String::new(),
                failed: choice.is_none(),
            })
            .unwrap();
        };
        let (a, b, c, d) = (
            judge("a", "yes"),
            judge("a", "no"),
            judge("a", "yes"),
            judge("b", "yes"),
        );
        shadow(a, "a", Some("yes"));
        shadow(b, "a", Some("yes"));
        shadow(c, "a", None);
        shadow(d, "b", Some("yes"));
        let stats = s.shadow_stats(None).unwrap();
        assert_eq!(
            stats
                .iter()
                .map(|k| (k.kind.as_str(), k.shadows, k.agreed, k.failed))
                .collect::<Vec<_>>(),
            [("a", 3, 1, 1), ("b", 1, 1, 0)]
        );
        assert_eq!(s.shadow_stats(Some("b")).unwrap().len(), 1);
        assert!(s.shadow_stats(Some("c")).unwrap().is_empty());
    }

    #[test]
    fn a_takeover_changes_status_from_the_one_it_has() {
        let (_dir, s) = store();
        let t = s.add_takeover("triage", 0.97, 120).unwrap();
        assert_eq!(
            (t.status.as_str(), s.takeovers().unwrap().len()),
            ("open", 1)
        );
        assert!(!s.move_takeover(t.id, "merged", "released").unwrap());
        assert!(s.move_takeover(t.id, "open", "merged").unwrap());
        assert!(!s.move_takeover(t.id, "open", "dismissed").unwrap());
        assert_eq!(s.takeover(t.id).unwrap().unwrap().status, "merged");
        assert_eq!(s.takeover(t.id + 1).unwrap(), None);
    }
}
