//! What concerns a person (ADR 0018): the notices of their Inbox's Mentions tab, and the cards they follow.
//!
//! A notice is one line for one person about a card or an inbox item: they were given it (`assigned`), mentioned in
//! a comment (`mentioned`) or a card they follow changed (`changed`). It stays until it is deleted with its card, and
//! carries a read mark that is the person's own: reading it changes nothing for anyone else. Redacted before it is
//! stored.
//!
//! A person follows the cards they wrote and the cards they are given, unless they stopped following; any other card
//! they follow by choice. `card_follows` holds only what a person chose (`following` 1 or 0); a card with no row for
//! a person is followed by them when they wrote it or are assigned to it.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Person, Result, Store, now};
use crate::blobs::redact;

/// The tables of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE notices (
    id INTEGER PRIMARY KEY,
    person_id INTEGER NOT NULL REFERENCES people(id),
    kind TEXT NOT NULL,
    by TEXT,
    card_id INTEGER,
    inbox_item_id INTEGER,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    at INTEGER NOT NULL,
    read_at INTEGER
);
CREATE INDEX notices_of_a_person ON notices (person_id, id);
CREATE TABLE card_follows (
    card_id INTEGER NOT NULL REFERENCES cards(id),
    person_id INTEGER NOT NULL REFERENCES people(id),
    following INTEGER NOT NULL,
    PRIMARY KEY (card_id, person_id)
);
"#;

/// The kinds of notice, as stored and as the web app names them.
pub const ASSIGNED: &str = "assigned";
pub const MENTIONED: &str = "mentioned";
pub const CHANGED: &str = "changed";

/// How long a notice that was read is kept, in seconds (30 days).
pub const KEPT_AFTER_READ: i64 = 30 * 24 * 60 * 60;

/// How many notices of a person are listed, newest first.
pub const LISTED: usize = 200;

/// One line of a person's Inbox under Mentions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub id: i64,
    /// `assigned`, `mentioned` or `changed`.
    pub kind: String,
    /// The handle of the person who caused it, without the `@`, when it is known and is a person.
    pub by: Option<String>,
    pub card_id: Option<i64>,
    pub inbox_item_id: Option<i64>,
    /// The title of the card or inbox item.
    pub title: String,
    /// What happened: the start of the comment, or the change.
    pub body: String,
    pub at: i64,
    pub read: bool,
}

/// What a notice is made of, before it has an id.
pub struct NewNotice<'a> {
    pub kind: &'a str,
    pub by: Option<&'a str>,
    pub card_id: Option<i64>,
    pub inbox_item_id: Option<i64>,
    pub title: &'a str,
    pub body: &'a str,
}

const COLUMNS: &str = "id, kind, by, card_id, inbox_item_id, title, body, at, read_at IS NOT NULL";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Notice> {
    Ok(Notice {
        id: r.get(0)?,
        kind: r.get(1)?,
        by: r.get(2)?,
        card_id: r.get(3)?,
        inbox_item_id: r.get(4)?,
        title: r.get(5)?,
        body: r.get(6)?,
        at: r.get(7)?,
        read: r.get(8)?,
    })
}

impl Store {
    /// Tells the person: one new unread notice.
    pub fn add_notice(&self, person_id: i64, new: &NewNotice<'_>) -> Result<Notice> {
        let conn = self.conn();
        let (title, body) = (redact(new.title), redact(new.body));
        let at = now();
        conn.execute(
            "INSERT INTO notices (person_id, kind, by, card_id, inbox_item_id, title, body, at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                person_id,
                new.kind,
                new.by,
                new.card_id,
                new.inbox_item_id,
                title,
                body,
                at
            ],
        )?;
        Ok(Notice {
            id: conn.last_insert_rowid(),
            kind: new.kind.into(),
            by: new.by.map(Into::into),
            card_id: new.card_id,
            inbox_item_id: new.inbox_item_id,
            title,
            body,
            at,
            read: false,
        })
    }

    /// The person's notices, newest first, at most [`LISTED`].
    pub fn notices(&self, person_id: i64) -> Result<Vec<Notice>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM notices WHERE person_id = ?1 ORDER BY id DESC LIMIT {LISTED}"
        ))?;
        let rows = stmt.query_map([person_id], row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Deletes the notices that were read and are older than [`KEPT_AFTER_READ`] at `now`; those not read yet stay.
    /// Answers how many it deleted.
    pub fn prune_notices(&self, now: i64) -> Result<usize> {
        Ok(self.conn().execute(
            "DELETE FROM notices WHERE read_at IS NOT NULL AND at < ?1",
            [now - KEPT_AFTER_READ],
        )?)
    }

    /// Marks the person's notice read (once; the first time stays). `None` when the person has no such notice, which
    /// is the answer for another person's too.
    pub fn read_notice(&self, person_id: i64, id: i64) -> Result<Option<Notice>> {
        let conn = self.conn();
        conn.execute(
            "UPDATE notices SET read_at = COALESCE(read_at, ?3) WHERE id = ?1 AND person_id = ?2",
            params![id, person_id, now()],
        )?;
        Ok(conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM notices WHERE id = ?1 AND person_id = ?2"),
                params![id, person_id],
                row,
            )
            .optional()?)
    }

    /// Records that the person follows the card, or stopped following it.
    pub fn set_following(&self, card_id: i64, person_id: i64, following: bool) -> Result<()> {
        self.conn().execute(
            "INSERT INTO card_follows (card_id, person_id, following) VALUES (?1, ?2, ?3) \
             ON CONFLICT (card_id, person_id) DO UPDATE SET following = ?3",
            params![card_id, person_id, following],
        )?;
        Ok(())
    }

    /// The people who follow the card, in the order they were added: those who chose to, and, unless they chose not
    /// to, the one who wrote it (`author_handle`, without the `@`) and those it is given to. Disabled people do not.
    pub fn followers(&self, card_id: i64, author_handle: Option<&str>) -> Result<Vec<Person>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM people p WHERE p.disabled = 0 AND COALESCE(\
                (SELECT f.following FROM card_follows f WHERE f.card_id = ?1 AND f.person_id = p.id), \
                p.handle = ?2 COLLATE NOCASE \
                OR EXISTS (SELECT 1 FROM card_assignees a WHERE a.card_id = ?1 AND a.person_id = p.id)) = 1 \
             ORDER BY p.id",
            super::people::COLUMNS
                .split(", ")
                .map(|c| format!("p.{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))?;
        let rows = stmt.query_map(params![card_id, author_handle], super::people::person)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Role, test_store};

    fn new(kind: &str) -> NewNotice<'_> {
        NewNotice {
            kind,
            by: Some("lea"),
            card_id: None,
            inbox_item_id: None,
            title: "t",
            body: "b",
        }
    }

    #[test]
    fn a_notice_is_read_by_its_person_alone_and_listed_newest_first() {
        let (_dir, s) = test_store();
        let lea = s.add_person("lea", "Lea", Role::Member).unwrap();
        let tom = s.add_person("tom", "Tom", Role::Member).unwrap();
        let first = s.add_notice(lea.id, &new(ASSIGNED)).unwrap();
        let second = s.add_notice(lea.id, &new(MENTIONED)).unwrap();
        let toms = s.add_notice(tom.id, &new(MENTIONED)).unwrap();
        let listed = s.notices(lea.id).unwrap();
        assert_eq!(
            listed.iter().map(|n| n.id).collect::<Vec<_>>(),
            [second.id, first.id]
        );
        assert!(listed.iter().all(|n| !n.read));

        assert_eq!(s.read_notice(tom.id, first.id).unwrap(), None, "not Tom's");
        assert!(!s.notices(lea.id).unwrap()[1].read);
        assert!(s.read_notice(lea.id, first.id).unwrap().unwrap().read);
        assert!(s.notices(lea.id).unwrap()[1].read);
        assert!(!s.notices(lea.id).unwrap()[0].read);
        assert!(!s.notices(tom.id).unwrap()[0].read, "Tom's own is unread");
        assert_eq!(toms.kind, MENTIONED);
    }

    #[test]
    fn a_card_is_followed_by_its_author_and_assignees_unless_they_stopped_and_by_whoever_chose_to()
    {
        let (_dir, s) = test_store();
        let lea = s.add_person("lea", "Lea", Role::Member).unwrap();
        let tom = s.add_person("tom", "Tom", Role::Member).unwrap();
        let ada = s.add_person("ada", "Ada", Role::Viewer).unwrap();
        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        let card = s
            .add_card(repo.id, "t", "b", "person:lea", "Not started")
            .unwrap();
        let handles = |s: &Store| -> Vec<String> {
            s.followers(card.id, Some("lea"))
                .unwrap()
                .into_iter()
                .map(|p| p.handle)
                .collect()
        };
        assert_eq!(handles(&s), ["lea"], "the author");
        s.set_card_assignees(card.id, &std::collections::BTreeSet::from([tom.id]))
            .unwrap();
        assert_eq!(handles(&s), ["lea", "tom"], "and who it is given to");
        s.set_following(card.id, ada.id, true).unwrap();
        s.set_following(card.id, lea.id, false).unwrap();
        assert_eq!(handles(&s), ["tom", "ada"], "a choice beats the author");
        s.set_person_disabled(tom.id, true).unwrap();
        assert_eq!(handles(&s), ["ada"], "a disabled person is not told");
    }

    #[test]
    fn notices_that_were_read_long_ago_are_pruned_and_the_others_stay() {
        let (_dir, s) = test_store();
        let lea = s.add_person("lea", "Lea", Role::Member).unwrap();
        let old_read = s.add_notice(lea.id, &new(ASSIGNED)).unwrap();
        let old_unread = s.add_notice(lea.id, &new(ASSIGNED)).unwrap();
        let new_read = s.add_notice(lea.id, &new(MENTIONED)).unwrap();
        s.read_notice(lea.id, old_read.id).unwrap();
        s.read_notice(lea.id, new_read.id).unwrap();
        let day = 24 * 60 * 60;
        s.conn()
            .execute(
                "UPDATE notices SET at = ?1 WHERE id IN (?2, ?3)",
                rusqlite::params![100, old_read.id, old_unread.id],
            )
            .unwrap();
        assert_eq!(s.prune_notices(100 + KEPT_AFTER_READ - day).unwrap(), 0);
        assert_eq!(s.prune_notices(100 + KEPT_AFTER_READ + day).unwrap(), 1);
        let ids: Vec<i64> = s.notices(lea.id).unwrap().iter().map(|n| n.id).collect();
        assert_eq!(ids, [new_read.id, old_unread.id]);
    }
}
