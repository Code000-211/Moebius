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
ALTER TABLE runs ADD COLUMN review_head TEXT;
ALTER TABLE runs ADD COLUMN review_base TEXT;
ALTER TABLE changes ADD COLUMN base_branch TEXT;
ALTER TABLE changes ADD COLUMN reviewed_base TEXT;
CREATE TABLE external_reviews (
    id INTEGER PRIMARY KEY,
    change_id INTEGER NOT NULL REFERENCES changes(id),
    head TEXT NOT NULL,
    base TEXT NOT NULL DEFAULT '',
    round INTEGER NOT NULL DEFAULT 0,
    run_id INTEGER,
    state TEXT NOT NULL,
    verdict TEXT,
    summary TEXT NOT NULL DEFAULT '',
    comments TEXT NOT NULL DEFAULT '[]',
    url TEXT,
    at INTEGER NOT NULL,
    kind TEXT NOT NULL DEFAULT 'author',
    moved_at INTEGER NOT NULL DEFAULT 0
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
    /// The tip of the default branch the round's head was read merged into; empty until the round starts.
    #[serde(skip)]
    pub base: String,
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
    /// `author` for a round the author asked for, `person` for a review a person asked for from the card, which
    /// writes nothing on the forge and counts for nothing, but holds the pull request like any round while it runs.
    #[serde(skip)]
    pub kind: String,
    /// When the round last changed state, which a sweep reads to end a round that never moved on.
    #[serde(skip)]
    pub moved_at: i64,
}

/// The rounds of the forge pull request the change `?1` follows, whatever change row followed it: a pull request that
/// was closed and reopened is followed again as a new change, and its rounds are still its own. Written for a table
/// `external_reviews e`.
const SAME_PULL_REQUEST: &str = "(e.change_id = ?1 OR e.change_id IN (SELECT o.id FROM changes o, changes me \
     WHERE me.id = ?1 AND o.repository_id = me.repository_id AND o.pull_request IS NOT NULL \
     AND o.pull_request = me.pull_request))";

/// The states in which a head counts as read, so that it is not read again: a round that ended with a verdict, or has
/// its run going.
const COUNTED: &str = "('starting', 'running', 'approved', 'posting', 'posted', 'merged')";

const COLUMNS: &str = "id, change_id, head, round, run_id, state, verdict, summary, comments, url, at, base, kind, moved_at";

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
        base: r.get(11)?,
        kind: r.get(12)?,
        moved_at: r.get(13)?,
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

    /// Records the branch the pull request is against, as the poll last read it.
    pub fn set_change_base_branch(&self, change_id: i64, branch: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET base_branch = ?1 WHERE id = ?2",
            params![branch, change_id],
        )?;
        Ok(())
    }

    /// The branch the pull request is against, as the poll last read it.
    pub fn change_base_branch(&self, change_id: i64) -> Result<Option<String>> {
        Ok(self
            .read()
            .query_row(
                "SELECT base_branch FROM changes WHERE id = ?1",
                [change_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    /// Records the commit of the default branch the review of the change is against, with the commit it reads.
    pub fn set_change_reviewed_base(&self, change_id: i64, base: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET reviewed_base = ?1 WHERE id = ?2",
            params![base, change_id],
        )?;
        Ok(())
    }

    /// The commit of the default branch the review of the change is against.
    pub fn change_reviewed_base(&self, change_id: i64) -> Result<Option<String>> {
        Ok(self
            .read()
            .query_row(
                "SELECT reviewed_base FROM changes WHERE id = ?1",
                [change_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    /// Records the two commits a restricted review run reads, as its round claimed them.
    pub fn set_run_review(&self, run_id: i64, head: &str, base: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE runs SET review_head = ?1, review_base = ?2 WHERE id = ?3",
            params![head, base, run_id],
        )?;
        Ok(())
    }

    /// The head and the default branch's tip a restricted review run reads, once they are recorded.
    pub fn run_review(&self, run_id: i64) -> Result<Option<(String, String)>> {
        Ok(self
            .read()
            .query_row(
                "SELECT review_head, review_base FROM runs WHERE id = ?1",
                [run_id],
                |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()?
            .and_then(|(head, base)| head.zip(base)))
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

    /// How many rounds of the pull request the change follows started a run, whatever came of them: one that went stale,
    /// failed or was never answered cost a run too, so no author can have the Reviewer read a pull request for ever, and
    /// closing and reopening it does not begin again. A round that is starting counts, since its run is being made.
    pub fn external_review_rounds(&self, change_id: i64) -> Result<i64> {
        Ok(self.read().query_row(
            &format!(
                "SELECT COUNT(*) FROM external_reviews e WHERE {SAME_PULL_REQUEST} \
                 AND e.kind = 'author' AND (e.run_id IS NOT NULL OR e.state = 'starting')"
            ),
            [change_id],
            |r| r.get(0),
        )?)
    }

    /// Whether a round on `head` of the pull request the change follows started and was not thrown away.
    pub fn external_review_of_head(&self, change_id: i64, head: &str) -> Result<bool> {
        Ok(self.read().query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM external_reviews e WHERE {SAME_PULL_REQUEST} \
                 AND e.kind = 'author' AND e.head = ?2 AND e.state IN {COUNTED})"
            ),
            params![change_id, head],
            |r| r.get(0),
        )?)
    }

    /// Claims the round `id` to start its run on the commits `head` and `base`, and says whether it did: false when
    /// another thread claimed it first, or when another round of the same pull request is starting, running or
    /// writing its review (rounds of a pull request never run side by side). The commits are written with the claim,
    /// in the same step, so that what the run reads is what the winner claimed and nothing a thread that lost wrote.
    pub fn claim_external_review(&self, id: i64, head: &str, base: &str) -> Result<bool> {
        let conn = self.conn();
        let change_id: i64 = conn.query_row(
            "SELECT change_id FROM external_reviews WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?;
        let busy: bool = conn.query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM external_reviews e WHERE {SAME_PULL_REQUEST} \
                 AND e.id <> ?2 AND e.state IN ('starting', 'running', 'posting'))"
            ),
            params![change_id, id],
            |r| r.get(0),
        )?;
        if busy {
            return Ok(false);
        }
        Ok(conn.execute(
            "UPDATE external_reviews SET state = 'starting', head = ?1, base = ?2, moved_at = ?4 \
             WHERE id = ?3 AND state = 'wanted'",
            params![head, base, id, now()],
        )? > 0)
    }

    /// Whether a round of the pull request the change follows is starting, running or writing its review.
    pub fn external_review_active(&self, change_id: i64) -> Result<bool> {
        Ok(self.read().query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM external_reviews e WHERE {SAME_PULL_REQUEST} \
                 AND e.state IN ('starting', 'running', 'posting'))"
            ),
            [change_id],
            |r| r.get(0),
        )?)
    }

    /// Claims the pull request the change follows for a review a person asked for, which is a round of its own kind
    /// started at once: the same claim as an author's round, in the same locked step, so that neither starts while the
    /// other is going. Gives its id, or none when a round of the pull request is going.
    pub fn claim_person_review(
        &self,
        change_id: i64,
        head: &str,
        base: &str,
    ) -> Result<Option<i64>> {
        let conn = self.conn();
        let busy: bool = conn.query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM external_reviews e WHERE {SAME_PULL_REQUEST} \
                 AND e.state IN ('starting', 'running', 'posting'))"
            ),
            [change_id],
            |r| r.get(0),
        )?;
        if busy {
            return Ok(None);
        }
        conn.execute(
            "INSERT INTO external_reviews (change_id, head, base, state, kind, at, moved_at) \
             VALUES (?1, ?2, ?3, 'starting', 'person', ?4, ?4)",
            params![change_id, head, base, now()],
        )?;
        Ok(Some(conn.last_insert_rowid()))
    }

    /// The ids of the change rows that follow the same forge pull request as the change, itself included.
    pub fn same_pull_request_changes(&self, change_id: i64) -> Result<Vec<i64>> {
        let mut ids = self.rows(
            "SELECT o.id FROM changes o, changes me WHERE me.id = ?1 AND o.repository_id = me.repository_id \
             AND o.pull_request IS NOT NULL AND o.pull_request = me.pull_request",
            [change_id],
            |r| r.get::<_, i64>(0),
        )?;
        if !ids.contains(&change_id) {
            ids.push(change_id);
        }
        Ok(ids)
    }

    /// The rounds of any pull request that are in one of `states`, oldest first.
    pub fn external_reviews_in(&self, states: &[&str]) -> Result<Vec<ExternalReview>> {
        let list = states
            .iter()
            .map(|s| format!("'{}'", s.replace('\'', "")))
            .collect::<Vec<_>>()
            .join(", ");
        self.rows(
            &format!("SELECT {COLUMNS} FROM external_reviews WHERE state IN ({list}) ORDER BY id"),
            [],
            read,
        )
    }

    /// The round of the pull request the change follows that is claimed and whose run is being made, with the commits
    /// its run reads.
    pub fn claimed_external_review(&self, change_id: i64) -> Result<Option<ExternalReview>> {
        Ok(self
            .read()
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM external_reviews e WHERE {SAME_PULL_REQUEST} \
                     AND e.state = 'starting' ORDER BY e.id DESC LIMIT 1"
                ),
                [change_id],
                read,
            )
            .optional()?)
    }

    /// Moves the round `id` from the state `from` to `to` and says whether it did: false when another thread moved it
    /// first. The poll and the engine both move the rounds along; what only one may do (start the run, write the
    /// review, merge) is done by the one that wins this move.
    pub fn move_external_review(&self, id: i64, from: &str, to: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE external_reviews SET state = ?1, moved_at = ?4 WHERE id = ?2 AND state = ?3",
            params![to, id, from, now()],
        )? > 0)
    }

    /// Records that the round `id` runs as `run_id`, as the `round`th of its change, on the commit `head` that the run
    /// reads, which is where the round counts from now on.
    pub fn start_external_review(
        &self,
        id: i64,
        run_id: i64,
        round: i64,
        (head, base): (&str, &str),
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE external_reviews SET state = 'running', run_id = ?1, round = ?2, head = ?3, base = ?4, moved_at = ?6 WHERE id = ?5",
            params![run_id, round, head, base, id, now()],
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
            "UPDATE external_reviews SET state = ?1, verdict = ?2, summary = ?3, comments = ?4, moved_at = ?6 WHERE id = ?5",
            params![state, verdict, summary, serde_json::to_string(comments)?, id, now()],
        )?;
        Ok(())
    }

    /// Moves the round `id` to `state`; `url` is where its review is on the forge, when it is posted.
    pub fn set_external_review_state(&self, id: i64, state: &str, url: Option<&str>) -> Result<()> {
        self.conn().execute(
            "UPDATE external_reviews SET state = ?1, url = COALESCE(?2, url), moved_at = ?4 WHERE id = ?3",
            params![state, url, id, now()],
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
        store
            .start_external_review(first, 5, 1, ("bbb", "base0"))
            .unwrap();
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
        // A thrown-away round has cost its run all the same, but its head may be read again.
        store
            .set_external_review_state(first, "stale", None)
            .unwrap();
        assert_eq!(store.external_review_rounds(change.id).unwrap(), 1);
        assert!(!store.external_review_of_head(change.id, "bbb").unwrap());
    }

    #[test]
    fn the_rounds_belong_to_the_pull_request_and_not_to_the_row_that_follows_it_and_never_run_together()
     {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        let repo = store.add_repository("r", "/r", "none", "main").unwrap();
        let first = store
            .add_change(repo.id, None, "moebius/pr-1", Some(1), "aaa")
            .unwrap();
        let round = store.add_external_review(first.id, "aaa").unwrap();
        assert!(store.claim_external_review(round, "aaa", "base0").unwrap());
        store
            .start_external_review(round, 5, 1, ("aaa", "base0"))
            .unwrap();
        store
            .set_external_review_state(round, "posted", None)
            .unwrap();
        // The pull request is closed and reopened: another change row follows it.
        let reopened = store
            .add_change(repo.id, None, "moebius/pr-1", Some(1), "aaa")
            .unwrap();
        assert_ne!(first.id, reopened.id);
        assert_eq!(store.external_review_rounds(reopened.id).unwrap(), 1);
        assert!(store.external_review_of_head(reopened.id, "aaa").unwrap());
        // Another pull request is another.
        let other = store
            .add_change(repo.id, None, "moebius/pr-2", Some(2), "ccc")
            .unwrap();
        assert_eq!(store.external_review_rounds(other.id).unwrap(), 0);
        // Two rounds of one pull request, on two change rows, never run together.
        let second = store.add_external_review(reopened.id, "bbb").unwrap();
        let third = store.add_external_review(first.id, "ccc").unwrap();
        assert!(store.claim_external_review(second, "bbb", "base1").unwrap());
        assert!(store.external_review_active(first.id).unwrap());
        assert!(
            !store.claim_external_review(third, "ccc", "base1").unwrap(),
            "another round of the pull request is starting"
        );
        store
            .start_external_review(second, 6, 2, ("bbb", "base1"))
            .unwrap();
        assert!(
            !store.claim_external_review(third, "ccc", "base1").unwrap(),
            "and while it runs"
        );
        store
            .set_external_review_state(second, "stale", None)
            .unwrap();
        assert!(store.claim_external_review(third, "ccc", "base1").unwrap());
        // The claim wrote its commits with it.
        let claimed = store.claimed_external_review(first.id).unwrap().unwrap();
        assert_eq!(
            (claimed.head.as_str(), claimed.base.as_str()),
            ("ccc", "base1")
        );
    }
}
