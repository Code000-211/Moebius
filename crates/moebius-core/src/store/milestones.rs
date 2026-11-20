//! The milestones (ADR 0019): goals of the organisation, each with a name, a target date and a description, holding
//! cards of any projects. A card is in any number of milestones; a milestone is not in a project, so what a person may
//! see of one is decided above this module (see `app::milestones`), which gets every card of a milestone from here with
//! the project it is on. Deleting a card or a milestone takes the membership with it.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, StoreError, now};

/// The tables of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE milestones (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    target_date TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);
CREATE TABLE milestone_cards (
    milestone_id INTEGER NOT NULL REFERENCES milestones(id),
    card_id INTEGER NOT NULL REFERENCES cards(id),
    PRIMARY KEY (milestone_id, card_id)
);
CREATE INDEX milestone_cards_by_card ON milestone_cards (card_id);
"#;

/// A milestone: a goal with a target date, holding cards of any projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Milestone {
    pub id: i64,
    /// Unique regardless of case; a milestone can be renamed.
    pub name: String,
    /// The day the goal is due, as `YYYY-MM-DD`.
    pub target_date: String,
    pub description: String,
    pub created_at: i64,
}

/// A card of a milestone, as far as progress and the list need it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneCard {
    pub id: i64,
    pub project_id: i64,
    pub repository_id: i64,
    pub number: i64,
    pub title: String,
    pub column: String,
    /// Whether the card is done: its issue is closed or it is in `Done`.
    pub done: bool,
}

const COLUMNS: &str = "id, name, target_date, description, created_at";

fn milestone(row: &Row) -> rusqlite::Result<Milestone> {
    Ok(Milestone {
        id: row.get(0)?,
        name: row.get(1)?,
        target_date: row.get(2)?,
        description: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn card(row: &Row) -> rusqlite::Result<MilestoneCard> {
    Ok(MilestoneCard {
        id: row.get(0)?,
        project_id: row.get(1)?,
        repository_id: row.get(2)?,
        number: row.get(3)?,
        title: row.get(4)?,
        column: row.get(5)?,
        done: row.get(6)?,
    })
}

impl Store {
    /// Adds a milestone. A name taken in any case is an error; the caller checks the date.
    pub fn add_milestone(
        &self,
        name: &str,
        target_date: &str,
        description: &str,
    ) -> Result<Milestone> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO milestones (name, target_date, description, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![name, target_date, description, now()],
            )?;
            conn.last_insert_rowid()
        };
        self.milestone(id)?
            .ok_or(StoreError::Sql(rusqlite::Error::QueryReturnedNoRows))
    }

    /// Replaces a milestone's name, date and description.
    pub fn edit_milestone(
        &self,
        id: i64,
        name: &str,
        target_date: &str,
        description: &str,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE milestones SET name = ?2, target_date = ?3, description = ?4 WHERE id = ?1",
            params![id, name, target_date, description],
        )?;
        Ok(())
    }

    /// The milestone with `id`.
    pub fn milestone(&self, id: i64) -> Result<Option<Milestone>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM milestones WHERE id = ?1"),
                [id],
                milestone,
            )
            .optional()?)
    }

    /// The milestone whose name is `name`, in any case.
    pub fn milestone_by_name(&self, name: &str) -> Result<Option<Milestone>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM milestones WHERE name = ?1"),
                [name],
                milestone,
            )
            .optional()?)
    }

    /// Every milestone, the nearest target date first.
    pub fn milestones(&self) -> Result<Vec<Milestone>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM milestones ORDER BY target_date, id"
        ))?;
        Ok(stmt
            .query_map([], milestone)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Deletes a milestone and what it holds; its cards stay. Whether there was one.
    pub fn delete_milestone(&self, id: i64) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM milestone_cards WHERE milestone_id = ?1", [id])?;
        let deleted = tx.execute("DELETE FROM milestones WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(deleted > 0)
    }

    /// Every card of the milestone, in the order of the cards, with the project each is on.
    pub fn milestone_cards(&self, id: i64) -> Result<Vec<MilestoneCard>> {
        let conn = self.read();
        let mut stmt = conn.prepare(
            "SELECT c.id, c.project_id, c.repository_id, c.number, c.title, c.column_name, \
                    (c.closed OR c.column_name = 'Done') \
             FROM milestone_cards m JOIN cards c ON c.id = m.card_id WHERE m.milestone_id = ?1 ORDER BY c.id",
        )?;
        Ok(stmt
            .query_map([id], card)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// The milestones a card is in, in the order of their target dates.
    pub fn card_milestones(&self, card_id: i64) -> Result<Vec<Milestone>> {
        let conn = self.read();
        let mut stmt = conn.prepare(
            "SELECT m.id, m.name, m.target_date, m.description, m.created_at \
             FROM milestone_cards c JOIN milestones m ON m.id = c.milestone_id \
             WHERE c.card_id = ?1 ORDER BY m.target_date, m.id",
        )?;
        Ok(stmt
            .query_map([card_id], milestone)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Replaces the milestones the card is in with `milestones` (ids).
    pub fn set_card_milestones(&self, card_id: i64, milestones: &[i64]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM milestone_cards WHERE card_id = ?1", [card_id])?;
        for milestone in milestones {
            tx.execute(
                "INSERT OR IGNORE INTO milestone_cards (milestone_id, card_id) VALUES (?1, ?2)",
                params![milestone, card_id],
            )?;
        }
        Ok(tx.commit()?)
    }
}

#[cfg(test)]
mod tests {
    use crate::store::test_store;

    #[test]
    fn a_milestone_holds_cards_of_two_projects_and_goes_without_them() {
        let (_dir, s) = test_store();
        let web = s.add_repository("web", "/w", "none", "main").unwrap();
        let api = s.add_repository("api", "/a", "none", "main").unwrap();
        let a = s.add_card(web.id, "a", "", "person", "Done").unwrap();
        let b = s
            .add_card(api.id, "b", "", "person", "Not started")
            .unwrap();
        let launch = s.add_milestone("Launch", "2027-01-31", "Ship it").unwrap();
        let early = s.add_milestone("Early", "2026-12-01", "").unwrap();
        assert_eq!(
            s.milestones()
                .unwrap()
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            ["Early", "Launch"]
        );
        s.set_card_milestones(a.id, &[launch.id, early.id, launch.id])
            .unwrap();
        s.set_card_milestones(b.id, &[launch.id]).unwrap();
        let cards = s.milestone_cards(launch.id).unwrap();
        assert_eq!(
            cards.iter().map(|c| (c.id, c.done)).collect::<Vec<_>>(),
            [(a.id, true), (b.id, false)]
        );
        assert_ne!(cards[0].project_id, cards[1].project_id);
        assert_eq!(s.card_milestones(a.id).unwrap().len(), 2);
        assert_eq!(s.card_milestones(a.id).unwrap()[0].name, "Early");
        assert!(s.milestone_by_name("LAUNCH").unwrap().is_some());
        assert!(s.add_milestone("launch", "2027-02-01", "").is_err());

        // A card's deletion takes its membership; a milestone's, its cards' membership but not the cards.
        s.delete_card(a.id, false).unwrap();
        assert_eq!(s.milestone_cards(launch.id).unwrap().len(), 1);
        assert!(s.delete_milestone(launch.id).unwrap());
        assert!(!s.delete_milestone(launch.id).unwrap());
        assert!(s.card(b.id).unwrap().is_some());
        assert!(s.card_milestones(b.id).unwrap().is_empty());
    }
}
