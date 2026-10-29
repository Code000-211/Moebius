//! Runs: an agent's work in a harness, for a card or not, from its start to its end.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: i64,
    pub card_id: Option<i64>,
    pub agent: String,
    pub harness: String,
    pub purpose: String,
    pub status: String,
    pub worktree: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    /// What the agent answered last, once the harness has reported it.
    pub summary: Option<String>,
    /// The repository a chat works in; none for a run of a card, and for a chat in an empty directory.
    pub repository_id: Option<i64>,
    /// Whether a chat works in the harness configuration repository.
    pub harness_repository: bool,
    /// Where a chat was started: `chat`, on the Chat page or by a client, `assistant`, in the desktop app's floating
    /// window, or `automation`, by an automation's schedule. `chat` for every other run.
    pub origin: String,
}

impl Run {
    /// The branch the run's work is committed on.
    pub fn branch(&self) -> String {
        let kind = if self.purpose == "chat" {
            "chat"
        } else {
            "run"
        };
        format!("moebius/{kind}-{}", self.id)
    }
}

impl Store {
    pub fn add_run(
        &self,
        card_id: Option<i64>,
        agent: &str,
        harness: &str,
        purpose: &str,
    ) -> Result<Run> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO runs (card_id, agent, harness, purpose, status, started_at) VALUES (?1, ?2, ?3, ?4, 'starting', ?5)",
            params![card_id, agent, harness, purpose, at],
        )?;
        Ok(Run {
            id: conn.last_insert_rowid(),
            card_id,
            agent: agent.into(),
            harness: harness.into(),
            purpose: purpose.into(),
            status: "starting".into(),
            worktree: None,
            started_at: at,
            ended_at: None,
            summary: None,
            repository_id: None,
            harness_repository: false,
            origin: "chat".into(),
        })
    }

    pub(super) fn runs_where(
        &self,
        clause: &str,
        param: rusqlite::types::Value,
    ) -> Result<Vec<Run>> {
        self.rows(&format!(
            "SELECT id, card_id, agent, harness, purpose, status, worktree, started_at, ended_at, summary, repository_id,
                harness_repository, origin
             FROM runs WHERE {clause} ORDER BY id"
        ), [param], |r| {
            Ok(Run {
                id: r.get(0)?,
                card_id: r.get(1)?,
                agent: r.get(2)?,
                harness: r.get(3)?,
                purpose: r.get(4)?,
                status: r.get(5)?,
                worktree: r.get(6)?,
                started_at: r.get(7)?,
                ended_at: r.get(8)?,
                summary: r.get(9)?,
                repository_id: r.get(10)?,
                harness_repository: r.get(11)?,
                origin: r.get(12)?,
            })
        })
    }

    pub fn run(&self, id: i64) -> Result<Option<Run>> {
        Ok(self.runs_where("id = ?1", id.into())?.pop())
    }

    /// The runs of a card, oldest first. Its trials are not among them: they are no part of the card's work.
    pub fn runs_for_card(&self, card_id: i64) -> Result<Vec<Run>> {
        self.runs_where("card_id = ?1 AND purpose <> 'trial'", card_id.into())
    }

    /// The runs that have not ended.
    pub fn active_runs(&self) -> Result<Vec<Run>> {
        self.runs_where("ended_at IS NULL AND ?1", 1.into())
    }

    /// Ends every run that has not ended as failed, with an event saying why; returns them. For a server that
    /// starts again, since no run outlives the server that drove it.
    pub fn fail_unended_runs(&self, why: &str) -> Result<Vec<Run>> {
        let runs = self.active_runs()?;
        for run in &runs {
            self.append_event(
                run.id,
                "moebius_error",
                &serde_json::json!({ "message": why }),
            )?;
            self.set_run_status(run.id, "failed")?;
        }
        Ok(runs)
    }

    pub fn set_run_status(&self, id: i64, status: &str) -> Result<()> {
        let ended = matches!(status, "succeeded" | "failed" | "stopped").then(now);
        let conn = self.conn();
        conn.execute(
            "UPDATE runs SET status = ?1, ended_at = COALESCE(?2, ended_at) WHERE id = ?3",
            params![status, ended, id],
        )?;
        if status == "succeeded" {
            // The note the run was started with has been dealt with.
            conn.execute(
                "UPDATE cards SET note = NULL WHERE id = (SELECT card_id FROM runs WHERE id = ?1 AND purpose = 'code')",
                [id],
            )?;
        }
        Ok(())
    }

    pub fn set_run_summary(&self, id: i64, summary: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE runs SET summary = ?1 WHERE id = ?2",
            params![summary, id],
        )?;
        Ok(())
    }

    /// The branch of the card's latest successful `code` run, where its work stands.
    pub fn code_branch(&self, card_id: i64) -> Result<Option<String>> {
        Ok(self
            .runs_for_card(card_id)?
            .iter()
            .rev()
            .find(|r| r.purpose == "code" && r.status == "succeeded")
            .map(Run::branch))
    }

    /// Records the commit a run of a card started from.
    pub fn set_run_base(&self, id: i64, commit: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE runs SET base = ?1 WHERE id = ?2",
            params![commit, id],
        )?;
        Ok(())
    }

    /// The commit a run of a card started from, if it was recorded.
    pub fn run_base(&self, id: i64) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row("SELECT base FROM runs WHERE id = ?1", [id], |r| r.get(0))
            .optional()?
            .flatten())
    }

    pub fn set_run_worktree(&self, id: i64, worktree: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE runs SET worktree = ?1 WHERE id = ?2",
            params![worktree, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::store::test_store as store;

    #[test]
    fn a_note_stays_on_the_card_until_a_code_run_succeeds() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("radiance", "/srv/radiance", "none", "main")
            .unwrap();
        let card = s.add_card(repo.id, "First", "", "leon", "Backlog").unwrap();
        assert_eq!(card.note, None);
        s.set_card_note(card.id, "Fix the lint.").unwrap();
        let note = || s.card(card.id).unwrap().unwrap().note;
        assert_eq!(note().as_deref(), Some("Fix the lint."));

        let review = s.add_run(Some(card.id), "Reviewer", "x", "review").unwrap();
        s.set_run_status(review.id, "succeeded").unwrap();
        let failed = s.add_run(Some(card.id), "Coder", "x", "code").unwrap();
        s.set_run_status(failed.id, "failed").unwrap();
        assert_eq!(note().as_deref(), Some("Fix the lint."));

        let coded = s.add_run(Some(card.id), "Coder", "x", "code").unwrap();
        s.set_run_status(coded.id, "succeeded").unwrap();
        assert_eq!(note(), None);
    }

    #[test]
    fn a_run_keeps_the_commit_it_started_from_and_merged_changes_are_listed_latest_first() {
        let (_dir, s) = store();
        let repo = s.add_repository("r", "/srv/r", "none", "main").unwrap();
        let card = s.add_card(repo.id, "First", "", "leon", "Backlog").unwrap();
        let run = s.add_run(Some(card.id), "Coder", "x", "code").unwrap();
        assert_eq!(s.run_base(run.id).unwrap(), None);
        s.set_run_base(run.id, "abc").unwrap();
        assert_eq!(s.run_base(run.id).unwrap().as_deref(), Some("abc"));
        assert_eq!(s.run_base(99).unwrap(), None);

        let other = s
            .add_card(repo.id, "Second", "", "leon", "Backlog")
            .unwrap();
        let first = s
            .add_change(repo.id, Some(card.id), "a", None, "h1")
            .unwrap();
        let open = s
            .add_change(repo.id, Some(other.id), "b", None, "h2")
            .unwrap();
        let second = s
            .add_change(repo.id, Some(other.id), "c", None, "h3")
            .unwrap();
        let chat = s.add_change(repo.id, None, "d", None, "h4").unwrap();
        for change in [&first, &second, &chat] {
            s.set_change_state(change.id, "merged").unwrap();
        }
        let ids = |limit| -> Vec<i64> {
            s.merged_card_changes(repo.id, limit)
                .unwrap()
                .iter()
                .map(|c| c.id)
                .collect()
        };
        assert_eq!(ids(10), [second.id, first.id]);
        assert_eq!(ids(1), [second.id]);
        assert_ne!(open.id, second.id);
    }
}
