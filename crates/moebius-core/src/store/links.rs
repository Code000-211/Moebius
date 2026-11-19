//! The links between issues that are not a parent or a blocker: an issue that duplicates another and issues that are
//! related. A link names two cards by id. A duplicate is stored once, from the duplicate to the original; a relation
//! is symmetric and is stored in both directions, so that each card finds the other with one read. Moebius keeps them
//! (forges have no such link that all of them share); `app::issues::links` makes and announces them.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// The table this module adds.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE card_links (
    card_id INTEGER NOT NULL REFERENCES cards(id),
    other_id INTEGER NOT NULL REFERENCES cards(id),
    kind TEXT NOT NULL,
    at INTEGER NOT NULL,
    PRIMARY KEY (card_id, other_id, kind)
);
CREATE INDEX card_links_by_other ON card_links (other_id, kind);
"#;

/// The kind of a link from a duplicate to its original.
pub const DUPLICATE: &str = "duplicate";
/// The kind of a link between related issues, stored in both directions.
pub const RELATED: &str = "related";

/// A link as one card reads it: the card on the other end and its kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub other_id: i64,
    pub kind: String,
}

impl Store {
    /// Links `card_id` to `other_id` with `kind`. Linking again changes nothing.
    pub fn add_link(&self, card_id: i64, other_id: i64, kind: &str) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO card_links (card_id, other_id, kind, at) VALUES (?1, ?2, ?3, ?4)",
            params![card_id, other_id, kind, now()],
        )?;
        Ok(())
    }

    /// Removes the link of `kind` from `card_id` to `other_id`. Says whether there was one.
    pub fn remove_link(&self, card_id: i64, other_id: i64, kind: &str) -> Result<bool> {
        let removed = self.conn().execute(
            "DELETE FROM card_links WHERE card_id = ?1 AND other_id = ?2 AND kind = ?3",
            params![card_id, other_id, kind],
        )?;
        Ok(removed > 0)
    }

    /// The links from `card_id` of `kind`, oldest first.
    pub fn links_from(&self, card_id: i64, kind: &str) -> Result<Vec<Link>> {
        self.rows(
            "SELECT other_id, kind FROM card_links WHERE card_id = ?1 AND kind = ?2 ORDER BY at, other_id",
            params![card_id, kind],
            |r| {
                Ok(Link {
                    other_id: r.get(0)?,
                    kind: r.get(1)?,
                })
            },
        )
    }

    /// The links to `other_id` of `kind`, as the cards they start from, oldest first: the duplicates of an original.
    pub fn links_to(&self, other_id: i64, kind: &str) -> Result<Vec<i64>> {
        self.rows(
            "SELECT card_id FROM card_links WHERE other_id = ?1 AND kind = ?2 ORDER BY at, card_id",
            params![other_id, kind],
            |r| r.get(0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store as store;

    #[test]
    fn links_are_kept_once_and_read_from_either_end() {
        let (_dir, s) = store();
        let repo = s.add_repository("w", "/w", "none", "main").unwrap();
        let a = s.add_card(repo.id, "A", "", "p", "Backlog").unwrap();
        let b = s.add_card(repo.id, "B", "", "p", "Backlog").unwrap();

        s.add_link(b.id, a.id, DUPLICATE).unwrap();
        s.add_link(b.id, a.id, DUPLICATE).unwrap();
        s.add_link(a.id, b.id, RELATED).unwrap();
        s.add_link(b.id, a.id, RELATED).unwrap();
        assert_eq!(
            s.links_from(b.id, DUPLICATE).unwrap(),
            [Link {
                other_id: a.id,
                kind: DUPLICATE.into()
            }]
        );
        assert_eq!(s.links_to(a.id, DUPLICATE).unwrap(), [b.id]);
        assert!(s.links_from(a.id, DUPLICATE).unwrap().is_empty());
        assert_eq!(s.links_from(a.id, RELATED).unwrap()[0].other_id, b.id);

        assert!(s.remove_link(b.id, a.id, DUPLICATE).unwrap());
        assert!(!s.remove_link(b.id, a.id, DUPLICATE).unwrap());
        assert!(s.links_to(a.id, DUPLICATE).unwrap().is_empty());
    }
}
