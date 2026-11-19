//! What a person keeps for themselves in the sidebar, on the server so that it follows them to every browser: the
//! places they pinned, in the order they put them, and the places they opened lately, newest first.
//!
//! A place is a project, a view, a card or a page, named by its kind, the repository it belongs to and a key (a
//! card's id; a view's or a page's slug; empty for a project) together with the title it had when it was kept. The
//! same place is kept once in each list. Whether a person may still see a place is the API's to say
//! (`server::api::favourites`), which forgets the ones they may not.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// The tables of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE favourites (
    id INTEGER PRIMARY KEY,
    person_id INTEGER NOT NULL REFERENCES people(id),
    kind TEXT NOT NULL,
    repository_id INTEGER NOT NULL,
    key TEXT NOT NULL,
    title TEXT NOT NULL,
    position INTEGER NOT NULL,
    UNIQUE (person_id, kind, repository_id, key)
);
CREATE TABLE recent_places (
    id INTEGER PRIMARY KEY,
    person_id INTEGER NOT NULL REFERENCES people(id),
    kind TEXT NOT NULL,
    repository_id INTEGER NOT NULL,
    key TEXT NOT NULL,
    title TEXT NOT NULL,
    seen_at INTEGER NOT NULL,
    UNIQUE (person_id, kind, repository_id, key)
);
"#;

/// How many recent places a person keeps; the oldest go first.
pub const RECENT_LIMIT: usize = 20;

/// The kinds of place that can be kept.
pub const PLACE_KINDS: [&str; 4] = ["project", "view", "card", "page"];

/// A place as a person keeps it, in either list. `id` is the row of the list it is read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    pub id: i64,
    /// `project`, `view`, `card` or `page`.
    pub kind: String,
    pub repository_id: i64,
    /// A card's id, a view's or a page's slug; empty for a project.
    pub key: String,
    pub title: String,
}

fn place(row: &Row) -> rusqlite::Result<Place> {
    Ok(Place {
        id: row.get(0)?,
        kind: row.get(1)?,
        repository_id: row.get(2)?,
        key: row.get(3)?,
        title: row.get(4)?,
    })
}

impl Store {
    /// The places `person` pinned, in their order.
    pub fn favourites(&self, person: i64) -> Result<Vec<Place>> {
        self.rows(
            "SELECT id, kind, repository_id, key, title FROM favourites WHERE person_id = ?1
             ORDER BY position, id",
            [person],
            place,
        )
    }

    /// Pins a place at the end of `person`'s list. A place already pinned keeps its position and takes the new title.
    pub fn pin(
        &self,
        person: i64,
        kind: &str,
        repository_id: i64,
        key: &str,
        title: &str,
    ) -> Result<Place> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO favourites (person_id, kind, repository_id, key, title, position)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     (SELECT COALESCE(MAX(position) + 1, 0) FROM favourites WHERE person_id = ?1))
             ON CONFLICT (person_id, kind, repository_id, key) DO UPDATE SET title = excluded.title",
            params![person, kind, repository_id, key, title],
        )?;
        Ok(conn.query_row(
            "SELECT id, kind, repository_id, key, title FROM favourites
             WHERE person_id = ?1 AND kind = ?2 AND repository_id = ?3 AND key = ?4",
            params![person, kind, repository_id, key],
            place,
        )?)
    }

    /// Removes a pin of `person`; false when they have none with `id`.
    pub fn unpin(&self, person: i64, id: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "DELETE FROM favourites WHERE id = ?1 AND person_id = ?2",
            params![id, person],
        )? == 1)
    }

    /// Puts `person`'s pins in the order of `ids`. A pin the list leaves out follows the ones it names, in its old
    /// order; an id that is not one of their pins is ignored.
    pub fn reorder_favourites(&self, person: i64, ids: &[i64]) -> Result<()> {
        let mut order: Vec<i64> = Vec::new();
        for id in ids {
            if !order.contains(id) {
                order.push(*id);
            }
        }
        let own = self.favourites(person)?;
        let mut sorted: Vec<i64> = order
            .into_iter()
            .filter(|id| own.iter().any(|p| p.id == *id))
            .collect();
        let rest: Vec<i64> = own
            .iter()
            .map(|p| p.id)
            .filter(|id| !sorted.contains(id))
            .collect();
        sorted.extend(rest);
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (position, id) in sorted.iter().enumerate() {
            tx.execute(
                "UPDATE favourites SET position = ?1 WHERE id = ?2 AND person_id = ?3",
                params![position as i64, id, person],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The places `person` opened lately, newest first.
    pub fn recent_places(&self, person: i64) -> Result<Vec<Place>> {
        self.rows(
            "SELECT id, kind, repository_id, key, title FROM recent_places WHERE person_id = ?1
             ORDER BY seen_at DESC, id DESC",
            [person],
            place,
        )
    }

    /// Notes that `person` opened a place now: it moves to the top of their recent places, which keep only
    /// [`RECENT_LIMIT`].
    pub fn see(
        &self,
        person: i64,
        kind: &str,
        repository_id: i64,
        key: &str,
        title: &str,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        // The newest one is always after the newest before it, even within the same second.
        let latest: Option<i64> = tx
            .query_row(
                "SELECT MAX(seen_at) FROM recent_places WHERE person_id = ?1",
                [person],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let at = now().max(latest.unwrap_or(0) + 1);
        tx.execute(
            "INSERT INTO recent_places (person_id, kind, repository_id, key, title, seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (person_id, kind, repository_id, key)
             DO UPDATE SET title = excluded.title, seen_at = excluded.seen_at",
            params![person, kind, repository_id, key, title, at],
        )?;
        tx.execute(
            "DELETE FROM recent_places WHERE person_id = ?1 AND id NOT IN
             (SELECT id FROM recent_places WHERE person_id = ?1 ORDER BY seen_at DESC, id DESC LIMIT ?2)",
            params![person, RECENT_LIMIT as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Forgets a recent place of `person`; false when they have none with `id`.
    pub fn forget_recent(&self, person: i64, id: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "DELETE FROM recent_places WHERE id = ?1 AND person_id = ?2",
            params![id, person],
        )? == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Role;
    use super::*;

    fn store() -> (tempfile::TempDir, Store, i64, i64) {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::in_memory(&dir.path().join("blobs")).unwrap();
        let a = s.add_person("ann", "Ann", Role::Member).unwrap().id;
        let b = s.add_person("bo", "Bo", Role::Member).unwrap().id;
        (dir, s, a, b)
    }

    #[test]
    fn pins_keep_their_order_and_belong_to_one_person() {
        let (_d, s, a, b) = store();
        let one = s.pin(a, "project", 1, "", "One").unwrap();
        let two = s.pin(a, "card", 1, "7", "Seven").unwrap();
        let three = s.pin(a, "page", 1, "intro", "Intro").unwrap();
        s.pin(b, "project", 1, "", "One").unwrap();
        let ids = |s: &Store| {
            s.favourites(a)
                .unwrap()
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&s), vec![one.id, two.id, three.id]);
        s.reorder_favourites(a, &[three.id, one.id]).unwrap();
        assert_eq!(ids(&s), vec![three.id, one.id, two.id]);
        assert!(s.unpin(a, one.id).unwrap());
        assert!(!s.unpin(b, two.id).unwrap(), "not b's to remove");
        assert_eq!(ids(&s), vec![three.id, two.id]);
        assert_eq!(s.favourites(b).unwrap().len(), 1);
    }

    #[test]
    fn pinning_twice_keeps_one_pin_and_its_place() {
        let (_d, s, a, _) = store();
        let first = s.pin(a, "project", 1, "", "Old").unwrap();
        s.pin(a, "card", 1, "2", "Card").unwrap();
        let again = s.pin(a, "project", 1, "", "New").unwrap();
        assert_eq!(first.id, again.id);
        let all = s.favourites(a).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].title, "New");
    }

    #[test]
    fn recent_places_are_newest_first_without_repeats_and_limited() {
        let (_d, s, a, _) = store();
        s.see(a, "card", 1, "1", "One").unwrap();
        s.see(a, "card", 1, "2", "Two").unwrap();
        s.see(a, "card", 1, "1", "One again").unwrap();
        let titles: Vec<String> = s
            .recent_places(a)
            .unwrap()
            .into_iter()
            .map(|p| p.title)
            .collect();
        assert_eq!(titles, vec!["One again", "Two"]);
        for n in 0..RECENT_LIMIT + 5 {
            s.see(a, "card", 2, &n.to_string(), "x").unwrap();
        }
        let all = s.recent_places(a).unwrap();
        assert_eq!(all.len(), RECENT_LIMIT);
        assert_eq!(all[0].key, (RECENT_LIMIT + 4).to_string());
    }
}
