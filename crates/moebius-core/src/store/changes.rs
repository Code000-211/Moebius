//! Changes: a branch offered for merging, with its pull request and what its checks and reviews say.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

const CHANGE_COLUMNS: &str = "id, repository_id, card_id, branch, state, pull_request, checks, review, \
     head, published_at, checks_head, failed_checks, check_output, harness_repository";

fn change_from_row(r: &rusqlite::Row) -> rusqlite::Result<Change> {
    Ok(Change {
        id: r.get(0)?,
        repository_id: r.get(1)?,
        card_id: r.get(2)?,
        branch: r.get(3)?,
        state: r.get(4)?,
        pull_request: r.get(5)?,
        checks: r.get(6)?,
        review: r.get(7)?,
        head: r.get(8)?,
        published_at: r.get(9)?,
        checks_head: r.get(10)?,
        failed_checks: r.get(11)?,
        check_output: r.get(12)?,
        harness_repository: r.get(13)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub id: i64,
    /// The repository the change belongs to; none for a change of the harness configuration repository.
    pub repository_id: Option<i64>,
    pub card_id: Option<i64>,
    pub branch: String,
    pub state: String,
    /// The number of the change's pull request on GitHub.
    pub pull_request: Option<i64>,
    /// `pending`, `success` or `failure` once the checks are known, for the commit `checks_head`.
    pub checks: Option<String>,
    /// The state of the pull request's latest review, such as `approved`, once there is one.
    pub review: Option<String>,
    /// The commit of `branch` that was last published.
    pub head: Option<String>,
    /// When `head` was published, in seconds since the epoch.
    pub published_at: Option<i64>,
    /// The commit `checks` were read or run for.
    pub checks_head: Option<String>,
    /// The failing checks of the pull request, one per line.
    pub failed_checks: Option<String>,
    /// The end of the output of the repository's own checks, secrets removed.
    pub check_output: Option<String>,
    /// Whether the change belongs to the harness configuration repository, which is no repository of Moebius.
    pub harness_repository: bool,
}

impl Store {
    pub fn add_change(
        &self,
        repository_id: i64,
        card_id: Option<i64>,
        branch: &str,
        pull_request: Option<i64>,
        head: &str,
    ) -> Result<Change> {
        self.insert_change(Some(repository_id), card_id, branch, pull_request, head)
    }

    /// Adds a change of the harness configuration repository, which has no card and no pull request.
    pub fn add_harness_change(&self, branch: &str, head: &str) -> Result<Change> {
        self.insert_change(None, None, branch, None, head)
    }

    fn insert_change(
        &self,
        repository_id: Option<i64>,
        card_id: Option<i64>,
        branch: &str,
        pull_request: Option<i64>,
        head: &str,
    ) -> Result<Change> {
        let conn = self.conn();
        let at = now();
        let harness_repository = repository_id.is_none();
        conn.execute(
            "INSERT INTO changes (repository_id, card_id, branch, state, pull_request, head, published_at, created_at, \
             harness_repository) VALUES (?1, ?2, ?3, 'open', ?4, ?5, ?6, ?6, ?7)",
            params![repository_id, card_id, branch, pull_request, head, at, harness_repository],
        )?;
        Ok(Change {
            id: conn.last_insert_rowid(),
            repository_id,
            card_id,
            branch: branch.into(),
            state: "open".into(),
            pull_request,
            checks: None,
            review: None,
            head: Some(head.into()),
            published_at: Some(at),
            checks_head: None,
            failed_checks: None,
            check_output: None,
            harness_repository,
        })
    }

    /// The card's open change, if it has one.
    pub fn open_change(&self, card_id: i64) -> Result<Option<Change>> {
        Ok(self
            .conn()
            .query_row(
                &format!(
                    "SELECT {CHANGE_COLUMNS} FROM changes WHERE card_id = ?1 AND state = 'open' ORDER BY id DESC"
                ),
                [card_id],
                change_from_row,
            )
            .optional()?)
    }

    /// The latest change made of `branch`, open or not, if there is one.
    pub fn latest_change_on_branch(&self, branch: &str) -> Result<Option<Change>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {CHANGE_COLUMNS} FROM changes WHERE branch = ?1 ORDER BY id DESC"),
                [branch],
                change_from_row,
            )
            .optional()?)
    }

    /// Records that `head` of the change's branch was published just now.
    pub fn set_change_head(&self, id: i64, head: &str, published_at: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET head = ?1, published_at = ?2 WHERE id = ?3",
            params![head, published_at, id],
        )?;
        Ok(())
    }

    pub fn change(&self, id: i64) -> Result<Option<Change>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {CHANGE_COLUMNS} FROM changes WHERE id = ?1"),
                [id],
                change_from_row,
            )
            .optional()?)
    }

    /// The changes of a repository, oldest first.
    pub fn changes(&self, repository_id: i64) -> Result<Vec<Change>> {
        self.rows(
            &format!("SELECT {CHANGE_COLUMNS} FROM changes WHERE repository_id = ?1 ORDER BY id"),
            [repository_id],
            change_from_row,
        )
    }

    /// The changes of the harness configuration repository, oldest first.
    pub fn harness_changes(&self) -> Result<Vec<Change>> {
        self.rows(
            &format!(
                "SELECT {CHANGE_COLUMNS} FROM changes WHERE harness_repository = 1 ORDER BY id"
            ),
            [],
            change_from_row,
        )
    }

    /// The open changes of a repository that have a pull request, oldest first.
    pub fn open_pull_requests(&self, repository_id: i64) -> Result<Vec<Change>> {
        self.rows(
            &format!(
                "SELECT {CHANGE_COLUMNS} FROM changes \
             WHERE repository_id = ?1 AND state = 'open' AND pull_request IS NOT NULL ORDER BY id"
            ),
            [repository_id],
            change_from_row,
        )
    }

    /// Records what the forge says about the change's pull request: checks of the commit `checks_head`, the failing
    /// ones, and the latest review.
    pub fn set_change_status(
        &self,
        id: i64,
        checks: Option<&str>,
        checks_head: &str,
        failed_checks: Option<&str>,
        review: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET checks = ?1, checks_head = ?2, failed_checks = ?3, review = ?4 WHERE id = ?5",
            params![checks, checks_head, failed_checks, review, id],
        )?;
        Ok(())
    }

    /// Records the repository's own checks of the commit `head`: how they stand and the end of their output.
    pub fn set_local_checks(
        &self,
        id: i64,
        checks: &str,
        head: &str,
        output: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET checks = ?1, checks_head = ?2, check_output = ?3 WHERE id = ?4",
            params![checks, head, output, id],
        )?;
        Ok(())
    }

    pub fn set_change_state(&self, id: i64, state: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET state = ?1 WHERE id = ?2",
            params![state, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::store::{MIGRATIONS, Store};

    #[test]
    fn a_change_may_belong_to_no_repository_and_the_migration_keeps_the_changes_it_had() {
        let dir = tempfile::tempdir().unwrap();
        // Before the migration that lets a change belong to no repository, the one before the chats' origin.
        let before = MIGRATIONS.len() - 2;
        {
            let conn = Connection::open(dir.path().join("moebius.sqlite")).unwrap();
            conn.pragma_update(None, "foreign_keys", true).unwrap();
            for sql in &MIGRATIONS[..before] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", before as i64)
                .unwrap();
            conn.execute_batch(
                "INSERT INTO repositories (name, path, forge, default_branch) VALUES ('old', '/srv/old', 'none', 'main');
                 INSERT INTO changes (repository_id, branch, state, created_at, head) VALUES (1, 'moebius/chat-1', 'open', 1, 'abc');
                 INSERT INTO research (repository_id, goal, measure, better, token_budget, at) VALUES (1, 'g', 'm', 'lower', 1, 1);
                 INSERT INTO experiments (research_id, branch, title, kept, reason, change_id, at) VALUES (1, 'b', 't', 1, 'r', 1, 1);",
            )
            .unwrap();
        }
        let s = Store::open(dir.path()).unwrap();
        let old = s.change(1).unwrap().unwrap();
        assert_eq!(
            (
                old.repository_id,
                old.harness_repository,
                old.head.as_deref()
            ),
            (Some(1), false, Some("abc"))
        );
        let harness = s.add_harness_change("moebius/chat-2", "def").unwrap();
        assert_eq!(
            (harness.repository_id, harness.harness_repository),
            (None, true)
        );
        assert_eq!(s.harness_changes().unwrap(), vec![harness]);
        assert_eq!(s.changes(1).unwrap().len(), 1);
    }
}
