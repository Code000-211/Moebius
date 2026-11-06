//! What an issue has besides its text: a parent issue, labels, a claim, whether it was closed, and its comments. A
//! parent is the number of another issue of the same repository; the blocking edges live in `dependencies` (see
//! `cards`). Moebius keeps all of it, and on a forge the poll and the writes in `app::issues` keep the forge's copy in
//! line where the forge has one. For each of those fields it also keeps what the forge was last seen to say (the
//! `forge_seen` table), so that a poll acts on a change of the forge's and not on a snapshot that is older than
//! Moebius's own write.

use std::collections::BTreeSet;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// The columns and tables this module adds (the last migration of its kind).
pub(super) const SCHEMA: &str = r#"
ALTER TABLE cards ADD COLUMN parent INTEGER;
ALTER TABLE cards ADD COLUMN closed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cards ADD COLUMN claimed_by TEXT;
ALTER TABLE cards ADD COLUMN claimed_at INTEGER;
ALTER TABLE cards ADD COLUMN kept_open INTEGER NOT NULL DEFAULT 0;
CREATE TABLE card_labels (
    card_id INTEGER NOT NULL REFERENCES cards(id),
    label TEXT NOT NULL,
    PRIMARY KEY (card_id, label)
);
CREATE TABLE card_comments (
    id INTEGER PRIMARY KEY,
    card_id INTEGER NOT NULL REFERENCES cards(id),
    author TEXT NOT NULL,
    body TEXT NOT NULL,
    at INTEGER NOT NULL,
    forge_id INTEGER,
    UNIQUE (card_id, forge_id)
);
CREATE INDEX card_comments_by_card ON card_comments (card_id, id);
CREATE INDEX cards_by_parent ON cards (repository_id, parent);
CREATE TABLE forge_seen (
    repository_id INTEGER NOT NULL,
    number INTEGER NOT NULL,
    field TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (repository_id, number, field)
);
"#;

/// Tells the comments written through Moebius from the ones read from the forge, whose author may be a login that
/// looks like what Moebius writes (`person`). Older rows count as read from the forge.
pub(super) const COMMENT_ORIGIN: &str = r#"
ALTER TABLE card_comments ADD COLUMN local INTEGER NOT NULL DEFAULT 0;
"#;

/// A comment on an issue: written in Moebius or read from the forge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: i64,
    pub card_id: i64,
    /// The forge login of whoever wrote it, `person`, or `agent:<name>`.
    pub author: String,
    pub body: String,
    pub at: i64,
}

impl Store {
    /// Sets the issue number of the card's parent, or none.
    pub fn set_parent(&self, card_id: i64, parent: Option<i64>) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET parent = ?1 WHERE id = ?2",
            params![parent, card_id],
        )?;
        Ok(())
    }

    /// The cards of the issues whose parent is issue `parent` of the repository.
    pub fn children(&self, repository_id: i64, parent: i64) -> Result<Vec<super::Card>> {
        self.cards_where(
            "repository_id = ?1 AND parent = ?2",
            [repository_id, parent],
        )
    }

    /// Records that the issue was closed (or reopened) as an issue, which is not the same as its card being in Done: a
    /// card whose change was merged is in Done with its issue still open on a forge that does not close it.
    pub fn set_issue_closed(&self, card_id: i64, closed: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET closed = ?1 WHERE id = ?2",
            params![closed, card_id],
        )?;
        Ok(())
    }

    /// Sets or clears the mark of an issue that was reopened: the board does not close it for its sub-issues until one of
    /// them closes after that.
    pub fn set_kept_open(&self, card_id: i64, kept: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET kept_open = ?1 WHERE id = ?2",
            params![kept, card_id],
        )?;
        Ok(())
    }

    /// Whether the issue was reopened and no sub-issue has closed since.
    pub fn kept_open(&self, card_id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .query_row(
                "SELECT kept_open FROM cards WHERE id = ?1",
                [card_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false))
    }

    /// Whether the issue itself was closed.
    pub fn issue_closed(&self, card_id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .query_row("SELECT closed FROM cards WHERE id = ?1", [card_id], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or(false))
    }

    /// Replaces the card's labels; blank ones are dropped.
    pub fn set_labels(&self, card_id: i64, labels: &BTreeSet<String>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM card_labels WHERE card_id = ?1", [card_id])?;
        for label in labels.iter().filter(|l| !l.trim().is_empty()) {
            tx.execute(
                "INSERT INTO card_labels (card_id, label) VALUES (?1, ?2)",
                params![card_id, label],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Claims the card for `by`. Says whether it is claimed by `by` now: a card someone else claimed is left as it is.
    pub fn claim_card(&self, id: i64, by: &str) -> Result<bool> {
        let claimed = self.conn().execute(
            "UPDATE cards SET claimed_by = ?1, claimed_at = COALESCE(claimed_at, ?2) \
             WHERE id = ?3 AND (claimed_by IS NULL OR claimed_by = ?1)",
            params![by, now(), id],
        )?;
        Ok(claimed > 0)
    }

    /// Releases the card's claim. Says whether it was claimed.
    pub fn release_claim(&self, id: i64) -> Result<bool> {
        let released = self.conn().execute(
            "UPDATE cards SET claimed_by = NULL, claimed_at = NULL WHERE id = ?1 AND claimed_by IS NOT NULL",
            [id],
        )?;
        Ok(released > 0)
    }

    /// What the forge last said about `field` of issue `number` (`state`, `parent`, `labels`, `blockers`, `comments`, and the times the poll and Moebius's writes go by), as
    /// text; none before the poll first saw the issue.
    pub fn forge_seen(
        &self,
        repository_id: i64,
        number: i64,
        field: &str,
    ) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT value FROM forge_seen WHERE repository_id = ?1 AND number = ?2 AND field = ?3",
                params![repository_id, number, field],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Records what the forge says about `field` of issue `number`.
    pub fn set_forge_seen(
        &self,
        repository_id: i64,
        number: i64,
        field: &str,
        value: &str,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO forge_seen (repository_id, number, field, value) VALUES (?1, ?2, ?3, ?4)",
            params![repository_id, number, field, value],
        )?;
        Ok(())
    }

    /// Whether the card's change was merged.
    pub fn has_merged_change(&self, card_id: i64) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM changes WHERE card_id = ?1 AND state = 'merged')",
            [card_id],
            |r| r.get(0),
        )?)
    }

    /// Adds a blocker (an issue number) to a card.
    pub fn add_dependency(&self, card_id: i64, blocked_by: i64) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO dependencies (card_id, blocked_by) VALUES (?1, ?2)",
            [card_id, blocked_by],
        )?;
        Ok(())
    }

    /// Removes a blocker from a card. Says whether it was one.
    pub fn remove_dependency(&self, card_id: i64, blocked_by: i64) -> Result<bool> {
        let removed = self.conn().execute(
            "DELETE FROM dependencies WHERE card_id = ?1 AND blocked_by = ?2",
            [card_id, blocked_by],
        )?;
        Ok(removed > 0)
    }

    /// Every blocking edge in a repository, as (issue number, number of the issue that blocks it).
    pub fn dependency_edges(&self, repository_id: i64) -> Result<Vec<(i64, i64)>> {
        self.rows(
            "SELECT cards.number, dependencies.blocked_by FROM dependencies \
             JOIN cards ON cards.id = dependencies.card_id WHERE cards.repository_id = ?1",
            [repository_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    /// Adds a comment read from the forge. A comment of the forge is stored once however often it is read: `forge_id`
    /// is its id there. Says whether it was added.
    pub fn add_comment(
        &self,
        card_id: i64,
        author: &str,
        body: &str,
        at: Option<i64>,
        forge_id: Option<i64>,
    ) -> Result<Option<Comment>> {
        self.insert_comment(card_id, (author, false), body, at, forge_id)
    }

    /// Adds a comment written through Moebius by `author` (`person` or `agent:<name>`), which is also on the forge as
    /// `forge_id` when the repository has one. Says whether it was added. When the poll stored the comment from the
    /// forge first, that row is made the local one (its author is `author` and it counts as written through Moebius),
    /// and nothing is added.
    pub fn add_local_comment(
        &self,
        card_id: i64,
        author: &str,
        body: &str,
        forge_id: Option<i64>,
    ) -> Result<Option<Comment>> {
        let added = self.insert_comment(card_id, (author, true), body, None, forge_id)?;
        if let (None, Some(forge_id)) = (&added, forge_id) {
            self.conn().execute(
                "UPDATE card_comments SET local = 1, author = ?1 WHERE card_id = ?2 AND forge_id = ?3",
                params![author, card_id, forge_id],
            )?;
        }
        Ok(added)
    }

    fn insert_comment(
        &self,
        card_id: i64,
        (author, local): (&str, bool),
        body: &str,
        at: Option<i64>,
        forge_id: Option<i64>,
    ) -> Result<Option<Comment>> {
        let conn = self.conn();
        let at = at.unwrap_or_else(now);
        let added = conn.execute(
            "INSERT OR IGNORE INTO card_comments (card_id, author, body, at, forge_id, local) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![card_id, author, body, at, forge_id, local],
        )?;
        Ok((added > 0).then(|| Comment {
            id: conn.last_insert_rowid(),
            card_id,
            author: author.into(),
            body: body.into(),
            at,
        }))
    }

    /// The comment of the card that has the id `forge_id` on the forge.
    pub fn comment_by_forge_id(&self, card_id: i64, forge_id: i64) -> Result<Option<Comment>> {
        Ok(self
            .rows(
                "SELECT id, card_id, author, body, at FROM card_comments WHERE card_id = ?1 AND forge_id = ?2",
                [card_id, forge_id],
                |r| {
                    Ok(Comment {
                        id: r.get(0)?,
                        card_id: r.get(1)?,
                        author: r.get(2)?,
                        body: r.get(3)?,
                        at: r.get(4)?,
                    })
                },
            )?
            .pop())
    }

    /// The comments of a card, oldest first.
    pub fn comments(&self, card_id: i64) -> Result<Vec<Comment>> {
        self.rows(
            "SELECT id, card_id, author, body, at FROM card_comments WHERE card_id = ?1 ORDER BY at, id",
            [card_id],
            |r| {
                Ok(Comment {
                    id: r.get(0)?,
                    card_id: r.get(1)?,
                    author: r.get(2)?,
                    body: r.get(3)?,
                    at: r.get(4)?,
                })
            },
        )
    }

    /// The ids of the card's comments that were written through Moebius, not read from the forge.
    pub fn local_comment_ids(&self, card_id: i64) -> Result<std::collections::HashSet<i64>> {
        Ok(self
            .rows(
                "SELECT id FROM card_comments WHERE card_id = ?1 AND local = 1",
                [card_id],
                |r| r.get(0),
            )?
            .into_iter()
            .collect())
    }

    /// How many of the card's comments came from, or were written to, the forge.
    pub fn forge_comment_count(&self, card_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM card_comments WHERE card_id = ?1 AND forge_id IS NOT NULL",
            [card_id],
            |r| r.get(0),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store as store;

    #[test]
    fn parents_labels_claims_and_closing_are_kept_and_read_with_the_card() {
        let (_dir, s) = store();
        let repo = s.add_repository("w", "/w", "none", "main").unwrap();
        let map = s.add_card(repo.id, "Map", "", "p", "Backlog").unwrap();
        let ticket = s.add_card(repo.id, "Ticket", "", "p", "Backlog").unwrap();
        assert_eq!(
            (
                ticket.parent,
                ticket.closed,
                &ticket.labels,
                &ticket.claimed_by
            ),
            (None, false, &vec![], &None)
        );

        s.set_parent(ticket.id, Some(map.number)).unwrap();
        s.set_labels(
            ticket.id,
            &BTreeSet::from(["wayfinder:spike".to_string(), "a".into(), " ".into()]),
        )
        .unwrap();
        assert!(s.claim_card(ticket.id, "agent:x").unwrap());
        let stored = s.card(ticket.id).unwrap().unwrap();
        assert_eq!(stored.parent, Some(map.number));
        assert_eq!(stored.labels, ["a", "wayfinder:spike"]);
        assert_eq!(stored.claimed_by.as_deref(), Some("agent:x"));
        let children = s.children(repo.id, map.number).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, ticket.id);

        // Two sessions do not take the same issue; the holder may claim again, and a release frees it.
        assert!(!s.claim_card(ticket.id, "agent:y").unwrap());
        assert!(s.claim_card(ticket.id, "agent:x").unwrap());
        assert!(s.release_claim(ticket.id).unwrap());
        assert!(!s.release_claim(ticket.id).unwrap());
        assert!(s.claim_card(ticket.id, "agent:y").unwrap());

        // A card in Done is closed; so is an issue closed while its card is elsewhere.
        s.move_card(map.id, "Done").unwrap();
        assert!(s.card(map.id).unwrap().unwrap().closed);
        assert!(!s.issue_closed(map.id).unwrap());
        s.set_issue_closed(ticket.id, true).unwrap();
        assert!(s.card(ticket.id).unwrap().unwrap().closed);
    }

    #[test]
    fn comments_of_the_forge_are_stored_once_and_edges_are_read_per_repository() {
        let (_dir, s) = store();
        let repo = s.add_repository("w", "/w", "none", "main").unwrap();
        let other = s.add_repository("o", "/o", "none", "main").unwrap();
        let a = s.add_card(repo.id, "A", "", "p", "Backlog").unwrap();
        let b = s.add_card(repo.id, "B", "", "p", "Backlog").unwrap();
        let c = s.add_card(other.id, "C", "", "p", "Backlog").unwrap();

        assert!(
            s.add_comment(a.id, "leon", "one", Some(5), Some(100))
                .unwrap()
                .is_some()
        );
        assert!(
            s.add_comment(a.id, "leon", "one", Some(5), Some(100))
                .unwrap()
                .is_none()
        );
        s.add_comment(a.id, "person", "two", Some(4), None).unwrap();
        s.add_comment(a.id, "person", "three", Some(4), None)
            .unwrap();
        let bodies: Vec<String> = s
            .comments(a.id)
            .unwrap()
            .into_iter()
            .map(|c| c.body)
            .collect();
        assert_eq!(bodies, ["two", "three", "one"]);
        assert_eq!(s.forge_comment_count(a.id).unwrap(), 1);

        s.add_dependency(b.id, a.number).unwrap();
        s.add_dependency(b.id, a.number).unwrap();
        s.add_dependency(c.id, 9).unwrap();
        assert_eq!(s.dependency_edges(repo.id).unwrap(), [(b.number, a.number)]);
        assert!(s.remove_dependency(b.id, a.number).unwrap());
        assert!(!s.remove_dependency(b.id, a.number).unwrap());
    }

    /// A store from before issues were a tracker has merged cards in Done. They are closed to the board (the view says
    /// so) without the migration closing their issues, which the forge still has open: a poll that then sees an open
    /// issue must not take the card for a reopened one.
    #[test]
    fn a_card_in_done_before_the_migration_is_closed_in_the_view_and_not_as_an_issue() {
        let dir = tempfile::tempdir().unwrap();
        {
            let conn = rusqlite::Connection::open(dir.path().join("moebius.sqlite")).unwrap();
            let last = super::super::MIGRATIONS.len() - 1;
            for sql in &super::super::MIGRATIONS[..last] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", last as i64)
                .unwrap();
            conn.execute_batch(
                "INSERT INTO repositories (name, path, forge, default_branch) VALUES ('old', '/srv/old', 'none', 'main');
                 INSERT INTO cards (repository_id, number, title, body, author, column_name, created_at)
                 VALUES (1, 1, 'Merged', '', 'leon', 'Done', 1), (1, 2, 'Open', '', 'leon', 'Backlog', 1);",
            )
            .unwrap();
        }
        let s = Store::open(dir.path()).unwrap();
        let merged = s.card_by_number(1, 1).unwrap().unwrap();
        assert_eq!(merged.column, "Done");
        assert!(merged.closed);
        assert!(!s.issue_closed(merged.id).unwrap());
        assert!(!s.card_by_number(1, 2).unwrap().unwrap().closed);
    }
}
