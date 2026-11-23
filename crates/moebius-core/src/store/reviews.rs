//! Reviews of pull requests someone else opened (ADR 0029): what the store keeps so that the author's `/done` starts one
//! review of one head, a verdict counts for that head only, and a pull request is reviewed at most a few times. Each
//! row of `external_reviews` is one round of one change; the repository's `merge_reviewed` says whether an approved
//! pull request is merged by the server.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// The tables and columns of this part.
pub(super) const SCHEMA: &str = r#"
ALTER TABLE repositories ADD COLUMN merge_reviewed INTEGER NOT NULL DEFAULT 1;
ALTER TABLE runs ADD COLUMN restricted INTEGER NOT NULL DEFAULT 0;
CREATE TABLE external_reviews (
    id INTEGER PRIMARY KEY,
    change_id INTEGER NOT NULL REFERENCES changes(id),
    head TEXT NOT NULL,
    round INTEGER NOT NULL DEFAULT 0,
    run_id INTEGER,
    state TEXT NOT NULL,
    verdict TEXT,
    summary TEXT NOT NULL DEFAULT '',
    comments TEXT NOT NULL DEFAULT '[]',
    url TEXT,
    at INTEGER NOT NULL
);
CREATE INDEX external_reviews_of_a_change ON external_reviews (change_id, id);
"#;

/// A comment of a review on a line of the pull request's diff, in the file as the pull request leaves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment {
    pub path: String,
    pub line: i64,
    pub body: String,
}

/// One round of the Reviewer reading a followed pull request at one head. Its `state` is `wanted` (the author said
/// `/done` and the checks of the head are not known yet), `running`, `approved` (approved, waiting for the checks of the
/// head to be green before it is merged), `starting` and `posting` (a thread has taken the round to start its run or to
/// write its review, which only one does: see [`Store::move_external_review`]), `posted` (the review is on the forge), `merged`, `stale` (the head moved
/// before the verdict could be used: nothing was posted) or `failed` (the run ended without a verdict).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalReview {
    pub id: i64,
    pub change_id: i64,
    pub head: String,
    pub round: i64,
    pub run_id: Option<i64>,
    pub state: String,
    /// `approve` or `changes` once the run ended with one.
    pub verdict: Option<String>,
    pub summary: String,
    pub comments: Vec<ReviewComment>,
    /// Where the review is on the forge, once it is posted.
    pub url: Option<String>,
    pub at: i64,
}

/// The states in which a round counts: it started, and its head was read.
const COUNTED: &str = "('starting', 'running', 'approved', 'posting', 'posted', 'merged')";

const COLUMNS: &str =
    "id, change_id, head, round, run_id, state, verdict, summary, comments, url, at";

fn read(r: &rusqlite::Row) -> rusqlite::Result<ExternalReview> {
    let comments: String = r.get(8)?;
    Ok(ExternalReview {
        id: r.get(0)?,
        change_id: r.get(1)?,
        head: r.get(2)?,
        round: r.get(3)?,
        run_id: r.get(4)?,
        state: r.get(5)?,
        verdict: r.get(6)?,
        summary: r.get(7)?,
        comments: serde_json::from_str(&comments).unwrap_or_default(),
        url: r.get(9)?,
        at: r.get(10)?,
    })
}

impl Store {
    /// Sets whether the server merges a followed pull request the Reviewer approved.
    pub fn set_repository_merge_reviewed(&self, id: i64, merge: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE repositories SET merge_reviewed = ?1 WHERE id = ?2",
            params![merge, id],
        )?;
        Ok(())
    }

    /// Marks the run as restricted, as it is started: it reads a pull request someone else opened, has no tools, and
    /// nothing changes that later.
    pub fn set_run_restricted(&self, run_id: i64) -> Result<()> {
        self.conn()
            .execute("UPDATE runs SET restricted = 1 WHERE id = ?1", [run_id])?;
        Ok(())
    }

    /// Whether the run was started restricted. A run that is not there counts as restricted: nothing it asks is allowed.
    pub fn run_restricted(&self, run_id: i64) -> Result<bool> {
        Ok(self
            .read()
            .query_row("SELECT restricted FROM runs WHERE id = ?1", [run_id], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or(true))
    }

    /// Asks for a round on `head` of the change, which starts once the checks of the head are known and green.
    pub fn add_external_review(&self, change_id: i64, head: &str) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO external_reviews (change_id, head, state, at) VALUES (?1, ?2, 'wanted', ?3)",
            params![change_id, head, now()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// The rounds of the change, oldest first.
    pub fn external_reviews(&self, change_id: i64) -> Result<Vec<ExternalReview>> {
        self.rows(
            &format!("SELECT {COLUMNS} FROM external_reviews WHERE change_id = ?1 ORDER BY id"),
            [change_id],
            read,
        )
    }

    /// The round `id`.
    pub fn external_review(&self, id: i64) -> Result<Option<ExternalReview>> {
        Ok(self
            .read()
            .query_row(
                &format!("SELECT {COLUMNS} FROM external_reviews WHERE id = ?1"),
                [id],
                read,
            )
            .optional()?)
    }

    /// How many rounds of the change started and were not thrown away.
    pub fn external_review_rounds(&self, change_id: i64) -> Result<i64> {
        Ok(self.read().query_row(
            &format!(
                "SELECT COUNT(*) FROM external_reviews WHERE change_id = ?1 AND state IN {COUNTED}"
            ),
            [change_id],
            |r| r.get(0),
        )?)
    }

    /// Whether a round on `head` of the change started and was not thrown away.
    pub fn external_review_of_head(&self, change_id: i64, head: &str) -> Result<bool> {
        Ok(self.read().query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM external_reviews WHERE change_id = ?1 AND head = ?2 \
                 AND state IN {COUNTED})"
            ),
            params![change_id, head],
            |r| r.get(0),
        )?)
    }

    /// Moves the round `id` from the state `from` to `to` and says whether it did: false when another thread moved it
    /// first. The poll and the engine both move the rounds along; what only one may do (start the run, write the
    /// review, merge) is done by the one that wins this move.
    pub fn move_external_review(&self, id: i64, from: &str, to: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE external_reviews SET state = ?1 WHERE id = ?2 AND state = ?3",
            params![to, id, from],
        )? > 0)
    }

    /// Records that the round `id` runs as `run_id`, as the `round`th of its change, on the commit `head` that the run
    /// reads, which is where the round counts from now on.
    pub fn start_external_review(
        &self,
        id: i64,
        run_id: i64,
        round: i64,
        head: &str,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE external_reviews SET state = 'running', run_id = ?1, round = ?2, head = ?3 WHERE id = ?4",
            params![run_id, round, head, id],
        )?;
        Ok(())
    }

    /// Records what the run of round `id` concluded, with the round in `state`.
    pub fn set_external_review_verdict(
        &self,
        id: i64,
        state: &str,
        verdict: &str,
        summary: &str,
        comments: &[ReviewComment],
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE external_reviews SET state = ?1, verdict = ?2, summary = ?3, comments = ?4 WHERE id = ?5",
            params![state, verdict, summary, serde_json::to_string(comments)?, id],
        )?;
        Ok(())
    }

    /// Moves the round `id` to `state`; `url` is where its review is on the forge, when it is posted.
    pub fn set_external_review_state(&self, id: i64, state: &str, url: Option<&str>) -> Result<()> {
        self.conn().execute(
            "UPDATE external_reviews SET state = ?1, url = COALESCE(?2, url) WHERE id = ?3",
            params![state, url, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_count_once_started_and_a_head_is_read_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        let repo = store.add_repository("r", "/r", "none", "main").unwrap();
        let change = store
            .add_change(repo.id, None, "moebius/pr-1", Some(1), "aaa")
            .unwrap();
        let first = store.add_external_review(change.id, "aaa").unwrap();
        assert_eq!(store.external_review_rounds(change.id).unwrap(), 0);
        assert!(!store.external_review_of_head(change.id, "aaa").unwrap());
        assert!(
            store
                .move_external_review(first, "wanted", "starting")
                .unwrap()
        );
        assert!(
            !store
                .move_external_review(first, "wanted", "starting")
                .unwrap(),
            "only one thread wins"
        );
        store.start_external_review(first, 5, 1, "bbb").unwrap();
        assert!(
            store.external_review_of_head(change.id, "bbb").unwrap(),
            "the head the run reads"
        );
        assert!(!store.external_review_of_head(change.id, "aaa").unwrap());
        assert_eq!(store.external_review_rounds(change.id).unwrap(), 1);
        store
            .set_external_review_verdict(
                first,
                "posted",
                "changes",
                "No.",
                &[ReviewComment {
                    path: "a.rs".into(),
                    line: 3,
                    body: "Why?".into(),
                }],
            )
            .unwrap();
        let read = store.external_review(first).unwrap().unwrap();
        assert_eq!(read.comments[0].line, 3);
        // A thrown-away round counts for nothing.
        store
            .set_external_review_state(first, "stale", None)
            .unwrap();
        assert_eq!(store.external_review_rounds(change.id).unwrap(), 0);
        assert!(!store.external_review_of_head(change.id, "bbb").unwrap());
    }
}
