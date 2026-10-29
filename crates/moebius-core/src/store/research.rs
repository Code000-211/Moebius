//! Research (see `research`): a brief a person confirms, the runs that carry it out, and the levers they try.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Result, Run, Store, now};

/// A research (see `research`): a brief a person confirms, and what became of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Research {
    pub id: i64,
    pub repository_id: i64,
    pub goal: String,
    /// The shell command that measures; its result is the last number it prints.
    pub measure: String,
    /// `lower` or `higher`: which way the measurement should move.
    pub better: String,
    /// The tokens its research runs may use, checked before each round.
    pub token_budget: i64,
    /// `draft`, `confirmed`, `running`, `done`, `spent`, `stopped` or `failed`.
    pub status: String,
    /// The measurement of the repository as it is; none until it was measured.
    pub baseline: Option<f64>,
    /// Why it ended, once it did.
    pub reason: String,
    pub at: i64,
}

/// One lever tried (see `research`): the measurement of its branch and whether it was kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Experiment {
    pub id: i64,
    pub research_id: i64,
    /// The lever's branch, `research/<research id>-<short name>`.
    pub branch: String,
    /// The subject of the lever's commit.
    pub title: String,
    /// What the measure printed on the branch; none when the measurement failed.
    pub value: Option<f64>,
    /// Whether the repository's own checks passed on the branch; none when it has none or the measurement failed.
    pub checks: Option<bool>,
    pub kept: bool,
    pub reason: String,
    /// The change offered for a kept lever.
    pub change_id: Option<i64>,
    pub at: i64,
}

impl Store {
    pub fn add_research(
        &self,
        repository_id: i64,
        goal: &str,
        measure: &str,
        better: &str,
        token_budget: i64,
    ) -> Result<Research> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO research (repository_id, goal, measure, better, token_budget, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![repository_id, goal, measure, better, token_budget, now()],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        Ok(self.research(id)?.expect("the research was just stored"))
    }

    fn research_where(&self, clause: &str, param: rusqlite::types::Value) -> Result<Vec<Research>> {
        self.rows(&format!(
            "SELECT id, repository_id, goal, measure, better, token_budget, status, baseline, reason, at
             FROM research WHERE {clause} ORDER BY id DESC"
        ), [param], |r| {
            Ok(Research {
                id: r.get(0)?,
                repository_id: r.get(1)?,
                goal: r.get(2)?,
                measure: r.get(3)?,
                better: r.get(4)?,
                token_budget: r.get(5)?,
                status: r.get(6)?,
                baseline: r.get(7)?,
                reason: r.get(8)?,
                at: r.get(9)?,
            })
        })
    }

    pub fn research(&self, id: i64) -> Result<Option<Research>> {
        Ok(self.research_where("id = ?1", id.into())?.pop())
    }

    /// All research, the newest first.
    pub fn researches(&self) -> Result<Vec<Research>> {
        self.research_where("?1", 1.into())
    }

    /// Gives a research that has one of the statuses `from` the status `to` and the reason it got it; false, and
    /// nothing changed, when it has another status.
    pub fn move_research(&self, id: i64, from: &[&str], to: &str, reason: &str) -> Result<bool> {
        self.move_status(
            "research",
            "status = ?, reason = ?",
            vec![to.to_string().into(), reason.to_string().into()],
            id,
            from,
        )
    }

    /// Replaces the brief of a draft research; false, and nothing changed, when it is not a draft.
    pub fn set_research_brief(
        &self,
        id: i64,
        goal: &str,
        measure: &str,
        better: &str,
        token_budget: i64,
    ) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE research SET goal = ?1, measure = ?2, better = ?3, token_budget = ?4
             WHERE id = ?5 AND status = 'draft'",
            params![goal, measure, better, token_budget, id],
        )? == 1)
    }

    /// Starts a confirmed research; false, and nothing changed, when it is not confirmed or another one is running,
    /// since only one runs at a time.
    pub fn start_research(&self, id: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE research SET status = 'running'
             WHERE id = ?1 AND status = 'confirmed' AND NOT EXISTS (SELECT 1 FROM research WHERE status = 'running')",
            [id],
        )? == 1)
    }

    /// Sends the research the last server was running back to confirmed.
    pub fn reconfirm_running_research(&self) -> Result<()> {
        self.conn().execute(
            "UPDATE research SET status = 'confirmed' WHERE status = 'running'",
            [],
        )?;
        Ok(())
    }

    pub fn set_baseline(&self, id: i64, baseline: f64) -> Result<()> {
        self.conn().execute(
            "UPDATE research SET baseline = ?1 WHERE id = ?2",
            params![baseline, id],
        )?;
        Ok(())
    }

    /// Records that a run belongs to a research, so that its tokens count against the budget.
    pub fn add_research_run(&self, research_id: i64, run_id: i64) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO research_runs (research_id, run_id) VALUES (?1, ?2)",
            params![research_id, run_id],
        )?;
        Ok(())
    }

    /// The runs of a research, oldest first.
    pub fn runs_of_research(&self, research_id: i64) -> Result<Vec<Run>> {
        self.runs_where(
            "id IN (SELECT run_id FROM research_runs WHERE research_id = ?1)",
            research_id.into(),
        )
    }

    /// The tokens the runs of a research used.
    pub fn research_tokens(&self, research_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(SUM(u.input_tokens + u.output_tokens), 0) FROM usage u
             JOIN research_runs x ON x.run_id = u.run_id WHERE x.research_id = ?1",
            [research_id],
            |r| r.get(0),
        )?)
    }

    pub fn add_experiment(&self, e: &Experiment) -> Result<Experiment> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO experiments (research_id, branch, title, value, checks, kept, reason, change_id, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                e.research_id,
                e.branch,
                e.title,
                e.value,
                e.checks,
                e.kept,
                e.reason,
                e.change_id,
                now()
            ],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        Ok(self
            .experiments(e.research_id)?
            .into_iter()
            .find(|x| x.id == id)
            .expect("the experiment was just stored"))
    }

    /// The experiments of a research, the first first.
    pub fn experiments(&self, research_id: i64) -> Result<Vec<Experiment>> {
        self.rows(
            "SELECT id, research_id, branch, title, value, checks, kept, reason, change_id, at
             FROM experiments WHERE research_id = ?1 ORDER BY id",
            [research_id],
            |r| {
                Ok(Experiment {
                    id: r.get(0)?,
                    research_id: r.get(1)?,
                    branch: r.get(2)?,
                    title: r.get(3)?,
                    value: r.get(4)?,
                    checks: r.get(5)?,
                    kept: r.get(6)?,
                    reason: r.get(7)?,
                    change_id: r.get(8)?,
                    at: r.get(9)?,
                })
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::store::test_store as store;

    #[test]
    fn one_research_runs_at_a_time_and_only_a_confirmed_one_starts() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/tmp/demo", "main", "none")
            .unwrap();
        let [a, b] = [1, 2].map(|_| {
            s.add_research(repo.id, "goal", "echo 1", "lower", 10)
                .unwrap()
                .id
        });
        assert!(!s.start_research(a).unwrap());
        for id in [a, b] {
            assert!(s.move_research(id, &["draft"], "confirmed", "").unwrap());
        }
        assert!(!s.move_research(a, &["draft"], "confirmed", "").unwrap());
        assert!(s.start_research(a).unwrap());
        assert!(!s.start_research(b).unwrap());
        assert!(
            s.move_research(a, &["running"], "spent", "used up")
                .unwrap()
        );
        assert!(s.start_research(b).unwrap());
        let spent = s.research(a).unwrap().unwrap();
        assert_eq!(
            (spent.status.as_str(), spent.reason.as_str()),
            ("spent", "used up")
        );
        // The server that was running b died.
        s.reconfirm_running_research().unwrap();
        assert_eq!(s.research(b).unwrap().unwrap().status, "confirmed");
        assert_eq!(s.research(a).unwrap().unwrap().status, "spent");
    }
}
