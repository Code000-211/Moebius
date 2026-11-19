//! Who a card or an inbox item is given to (ADR 0018): people of `people`, named in the records by their handle.
//! Assigning replaces the whole set, or adds or takes off one person; a card keeps its assignees until a person changes them, so a card in any column
//! and an inbox item until it is deleted. A person is never deleted, so an assignment never dangles; a disabled
//! person stays assigned until someone changes it.

use std::collections::BTreeSet;

use rusqlite::params;

use super::{Result, Store};

/// The tables of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE card_assignees (
    card_id INTEGER NOT NULL REFERENCES cards(id),
    person_id INTEGER NOT NULL REFERENCES people(id),
    PRIMARY KEY (card_id, person_id)
);
CREATE TABLE inbox_assignees (
    item_id INTEGER NOT NULL,
    person_id INTEGER NOT NULL REFERENCES people(id),
    PRIMARY KEY (item_id, person_id)
);
CREATE TRIGGER inbox_assignees_follow_their_item AFTER DELETE ON inbox_items
BEGIN
    DELETE FROM inbox_assignees WHERE item_id = old.id;
END;
"#;

/// The handles of the people a card is assigned to, as a JSON array in people's order, for a `SELECT`.
pub(super) const CARD_ASSIGNEES: &str = "(SELECT json_group_array(handle) FROM (SELECT p.handle FROM card_assignees a JOIN people p ON p.id = a.person_id WHERE a.card_id = cards.id ORDER BY p.id))";

/// The handles of the people an inbox item is assigned to, as a JSON array in people's order, for a `SELECT`.
pub(super) const INBOX_ASSIGNEES: &str = "(SELECT json_group_array(handle) FROM (SELECT p.handle FROM inbox_assignees a JOIN people p ON p.id = a.person_id WHERE a.item_id = inbox_items.id ORDER BY p.id))";

/// Reads the array a `*_ASSIGNEES` subquery gave, as column `index`.
pub(super) fn handles(r: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Vec<String>> {
    serde_json::from_str(&r.get::<_, String>(index)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, e.into())
    })
}

impl Store {
    /// Replaces the people the card is assigned to with `people` (ids of people).
    pub fn set_card_assignees(&self, card_id: i64, people: &BTreeSet<i64>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM card_assignees WHERE card_id = ?1", [card_id])?;
        for person in people {
            tx.execute(
                "INSERT INTO card_assignees (card_id, person_id) VALUES (?1, ?2)",
                params![card_id, person],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Replaces the people the inbox item is assigned to with `people` (ids of people).
    pub fn set_inbox_assignees(&self, item_id: i64, people: &BTreeSet<i64>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM inbox_assignees WHERE item_id = ?1", [item_id])?;
        for person in people {
            tx.execute(
                "INSERT INTO inbox_assignees (item_id, person_id) VALUES (?1, ?2)",
                params![item_id, person],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Gives the card to one more person; false when the card was already given to them. One statement, so two
    /// changes made at once never overwrite each other.
    pub fn add_card_assignee(&self, card_id: i64, person: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "INSERT OR IGNORE INTO card_assignees (card_id, person_id) VALUES (?1, ?2)",
            params![card_id, person],
        )? > 0)
    }

    /// Takes the card off one person; false when the card was not given to them.
    pub fn remove_card_assignee(&self, card_id: i64, person: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "DELETE FROM card_assignees WHERE card_id = ?1 AND person_id = ?2",
            params![card_id, person],
        )? > 0)
    }

    /// Gives the inbox item to one more person; false when it was already given to them.
    pub fn add_inbox_assignee(&self, item_id: i64, person: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "INSERT OR IGNORE INTO inbox_assignees (item_id, person_id) VALUES (?1, ?2)",
            params![item_id, person],
        )? > 0)
    }

    /// Takes the inbox item off one person; false when it was not given to them.
    pub fn remove_inbox_assignee(&self, item_id: i64, person: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "DELETE FROM inbox_assignees WHERE item_id = ?1 AND person_id = ?2",
            params![item_id, person],
        )? > 0)
    }

    /// The inbox item with `id`, resolved or not.
    pub fn inbox_item(&self, id: i64) -> Result<Option<super::InboxItem>> {
        Ok(self.inbox_where("id = ?1", id.into())?.pop())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Role, test_store};

    #[test]
    fn a_card_and_an_inbox_item_keep_the_people_they_were_given_to_in_the_order_people_were_added()
    {
        let (_dir, s) = test_store();
        let lea = s.add_person("lea", "Lea", Role::Member).unwrap();
        let tom = s.add_person("tom", "Tom", Role::Viewer).unwrap();
        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        let card = s
            .add_card(repo.id, "t", "b", "person", "Not started")
            .unwrap();
        assert!(card.assignees.is_empty());
        s.set_card_assignees(card.id, &BTreeSet::from([tom.id, lea.id]))
            .unwrap();
        assert_eq!(s.card(card.id).unwrap().unwrap().assignees, ["lea", "tom"]);
        s.set_card_assignees(card.id, &BTreeSet::from([tom.id]))
            .unwrap();
        assert_eq!(s.card(card.id).unwrap().unwrap().assignees, ["tom"]);
        s.set_card_assignees(card.id, &BTreeSet::new()).unwrap();
        assert!(s.card(card.id).unwrap().unwrap().assignees.is_empty());

        let item = s
            .add_inbox_item("stuck", Some(card.id), "t", &serde_json::json!({}))
            .unwrap();
        assert!(item.assignees.is_empty());
        s.set_inbox_assignees(item.id, &BTreeSet::from([lea.id]))
            .unwrap();
        assert_eq!(s.inbox().unwrap()[0].assignees, ["lea"]);
        assert_eq!(s.inbox_item(item.id).unwrap().unwrap().assignees, ["lea"]);
    }

    #[test]
    fn deleting_a_card_takes_its_assignments_and_its_inbox_items_with_theirs() {
        let (_dir, s) = test_store();
        let lea = s.add_person("lea", "Lea", Role::Member).unwrap();
        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        let card = s
            .add_card(repo.id, "t", "b", "person", "Not started")
            .unwrap();
        let item = s
            .add_inbox_item("stuck", Some(card.id), "t", &serde_json::json!({}))
            .unwrap();
        s.set_card_assignees(card.id, &BTreeSet::from([lea.id]))
            .unwrap();
        s.set_inbox_assignees(item.id, &BTreeSet::from([lea.id]))
            .unwrap();
        s.delete_card(card.id, false).unwrap();
        let conn = s.conn();
        for table in ["card_assignees", "inbox_assignees"] {
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{table}");
        }
    }
}
