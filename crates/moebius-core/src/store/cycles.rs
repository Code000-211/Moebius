//! The cycles (ADR 0019): time boxes of one project, each with a name, a start and an end, holding cards of that
//! project. A card is in at most one open cycle; a closed cycle keeps the cards it held, so its page still shows what
//! was planned and what got done, while the unfinished ones may have moved to the next cycle. This module keeps the
//! records and the sums a cycle's page needs; the rules (same project, one open cycle, closing) are in `app::cycles`.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, StoreError, now};

/// The tables of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE cycles (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    name TEXT NOT NULL COLLATE NOCASE,
    starts_on TEXT NOT NULL,
    ends_on TEXT NOT NULL,
    closed_at INTEGER,
    created_at INTEGER NOT NULL,
    UNIQUE (project_id, name)
);
CREATE TABLE cycle_cards (
    cycle_id INTEGER NOT NULL REFERENCES cycles(id),
    card_id INTEGER NOT NULL REFERENCES cards(id),
    added_at INTEGER NOT NULL,
    PRIMARY KEY (cycle_id, card_id)
);
CREATE INDEX cycle_cards_by_card ON cycle_cards (card_id);
"#;

/// A cycle of a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cycle {
    pub id: i64,
    pub project_id: i64,
    /// Unique in its project regardless of case.
    pub name: String,
    /// The first day, as `YYYY-MM-DD`.
    pub starts_on: String,
    /// The last day, as `YYYY-MM-DD`; never before the first.
    pub ends_on: String,
    /// When it was closed, in Unix seconds; none while it is open.
    pub closed_at: Option<i64>,
    pub created_at: i64,
}

/// A card of a cycle, as far as progress, the burn-up and the list need it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleCard {
    pub id: i64,
    pub repository_id: i64,
    pub number: i64,
    pub title: String,
    pub column: String,
    /// Whether the card is done: its issue is closed or it is in `Done`.
    pub done: bool,
    /// The card's estimate in points, none while it has none.
    pub estimate: Option<u32>,
    /// When the card was added to the cycle, in Unix seconds.
    pub added_at: i64,
    /// When the card got done, in Unix seconds; none while it is not done.
    pub done_at: Option<i64>,
    /// Whether an agent ran code for the card.
    pub by_agent: bool,
    /// Tokens the card's runs used.
    pub tokens: i64,
}

const COLUMNS: &str = "id, project_id, name, starts_on, ends_on, closed_at, created_at";

fn cycle(row: &Row) -> rusqlite::Result<Cycle> {
    Ok(Cycle {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        starts_on: row.get(3)?,
        ends_on: row.get(4)?,
        closed_at: row.get(5)?,
        created_at: row.get(6)?,
    })
}

impl Store {
    /// Adds an open cycle to a project. A name the project has in any case is an error; the caller checks the dates.
    pub fn add_cycle(
        &self,
        project_id: i64,
        name: &str,
        starts_on: &str,
        ends_on: &str,
    ) -> Result<Cycle> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO cycles (project_id, name, starts_on, ends_on, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![project_id, name, starts_on, ends_on, now()],
            )?;
            conn.last_insert_rowid()
        };
        self.cycle(id)?
            .ok_or(StoreError::Sql(rusqlite::Error::QueryReturnedNoRows))
    }

    /// Replaces a cycle's name and dates.
    pub fn edit_cycle(&self, id: i64, name: &str, starts_on: &str, ends_on: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE cycles SET name = ?2, starts_on = ?3, ends_on = ?4 WHERE id = ?1",
            params![id, name, starts_on, ends_on],
        )?;
        Ok(())
    }

    /// The cycle with `id`.
    pub fn cycle(&self, id: i64) -> Result<Option<Cycle>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM cycles WHERE id = ?1"),
                [id],
                cycle,
            )
            .optional()?)
    }

    /// The cycle of the project called `name`, in any case.
    pub fn cycle_by_name(&self, project_id: i64, name: &str) -> Result<Option<Cycle>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM cycles WHERE project_id = ?1 AND name = ?2"),
                params![project_id, name],
                cycle,
            )
            .optional()?)
    }

    /// The cycles of a project, the latest start first.
    pub fn cycles(&self, project_id: i64) -> Result<Vec<Cycle>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM cycles WHERE project_id = ?1 ORDER BY starts_on DESC, id DESC"
        ))?;
        Ok(stmt
            .query_map([project_id], cycle)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Marks an open cycle closed now and, when `carry_to` is an open cycle, puts the cards that are not done into it;
    /// the closed cycle keeps all its cards. Answers the cards carried, in the order of the cards, or none when the
    /// cycle was closed already or the target is not open (checked inside the transaction, so a close that races
    /// another changes nothing).
    pub fn close_cycle(&self, id: i64, carry_to: Option<i64>) -> Result<Option<Vec<i64>>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if let Some(to) = carry_to {
            let open: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM cycles WHERE id = ?1 AND closed_at IS NULL)",
                [to],
                |r| r.get(0),
            )?;
            if !open {
                return Ok(None);
            }
        }
        let closed = tx.execute(
            "UPDATE cycles SET closed_at = ?2 WHERE id = ?1 AND closed_at IS NULL",
            params![id, now()],
        )?;
        if closed == 0 {
            return Ok(None);
        }
        let mut carried = Vec::new();
        if let Some(to) = carry_to {
            let class = super::class_sql("c.column_name", "c.project_id");
            let mut stmt = tx.prepare(&format!(
                "SELECT c.id FROM cycle_cards m JOIN cards c ON c.id = m.card_id \
                 WHERE m.cycle_id = ?1 AND NOT (c.closed OR {class} = 'Done') ORDER BY c.id",
            ))?;
            let ids = stmt
                .query_map([id], |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(stmt);
            for card in ids {
                tx.execute(
                    "INSERT OR IGNORE INTO cycle_cards (cycle_id, card_id, added_at) VALUES (?1, ?2, ?3)",
                    params![to, card, now()],
                )?;
                carried.push(card);
            }
        }
        tx.commit()?;
        Ok(Some(carried))
    }

    /// Deletes a cycle and what it holds; its cards stay. Whether there was one.
    pub fn delete_cycle(&self, id: i64) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM cycle_cards WHERE cycle_id = ?1", [id])?;
        let deleted = tx.execute("DELETE FROM cycles WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(deleted > 0)
    }

    /// The open cycle the card is in, if any.
    pub fn card_cycle(&self, card_id: i64) -> Result<Option<Cycle>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT y.id, y.project_id, y.name, y.starts_on, y.ends_on, y.closed_at, y.created_at \
                 FROM cycle_cards m JOIN cycles y ON y.id = m.cycle_id \
                 WHERE m.card_id = ?1 AND y.closed_at IS NULL",
                [card_id],
                cycle,
            )
            .optional()?)
    }

    /// Puts the card into the open cycle `cycle_id`, or out of its open cycle when none. A card in another open cycle
    /// leaves it, so a card is in at most one. Cycles that are closed keep the card.
    pub fn set_card_cycle(&self, card_id: i64, cycle_id: Option<i64>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM cycle_cards WHERE card_id = ?1 \
               AND cycle_id IN (SELECT id FROM cycles WHERE closed_at IS NULL)",
            [card_id],
        )?;
        if let Some(cycle_id) = cycle_id {
            tx.execute(
                "INSERT OR IGNORE INTO cycle_cards (cycle_id, card_id, added_at) VALUES (?1, ?2, ?3)",
                params![cycle_id, card_id, now()],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Every card of the cycle, in the order of the cards.
    pub fn cycle_cards(&self, id: i64) -> Result<Vec<CycleCard>> {
        let conn = self.read();
        let class = super::class_sql("c.column_name", "c.project_id");
        let to_class = super::class_sql("m2.to_column", "c.project_id");
        let mut stmt = conn.prepare(&format!(
            "SELECT c.id, c.repository_id, c.number, c.title, c.column_name, \
                    (c.closed OR {class} = 'Done') AS done, (SELECT json_group_array(label) FROM card_labels WHERE card_id = c.id), m.added_at, \
                    CASE WHEN c.closed OR {class} = 'Done' THEN \
                      COALESCE((SELECT MAX(m2.at) FROM card_moves m2 WHERE m2.card_id = c.id AND {to_class} = 'Done'), c.moved_at) \
                    END, \
                    EXISTS (SELECT 1 FROM runs r WHERE r.card_id = c.id AND r.purpose = 'code'), \
                    COALESCE((SELECT SUM(u.input_tokens + u.output_tokens) FROM usage u JOIN runs r ON r.id = u.run_id \
                              WHERE r.card_id = c.id AND r.purpose <> 'trial'), 0) \
             FROM cycle_cards m JOIN cards c ON c.id = m.card_id WHERE m.cycle_id = ?1 ORDER BY c.id",
        ))?;
        let rows = stmt.query_map([id], |r| {
            let labels: Vec<String> =
                serde_json::from_str(&r.get::<_, String>(6)?).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
            Ok(CycleCard {
                id: r.get(0)?,
                repository_id: r.get(1)?,
                number: r.get(2)?,
                title: r.get(3)?,
                column: r.get(4)?,
                done: r.get(5)?,
                estimate: super::planning::estimate_of(&labels),
                added_at: r.get(7)?,
                done_at: r.get(8)?,
                by_agent: r.get(9)?,
                tokens: r.get(10)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use crate::store::test_store;

    #[test]
    fn a_card_is_in_one_open_cycle_and_a_closed_cycle_keeps_it() {
        let (_dir, s) = test_store();
        let repo = s.add_repository("web", "/w", "none", "main").unwrap();
        let project = repo.home_project_id;
        let a = s.add_card(repo.id, "a", "", "person", "Done").unwrap();
        let b = s
            .add_card(repo.id, "b", "", "person", "Not started")
            .unwrap();
        let one = s
            .add_cycle(project, "Cycle 1", "2026-11-02", "2026-11-15")
            .unwrap();
        let two = s
            .add_cycle(project, "Cycle 2", "2026-11-16", "2026-11-29")
            .unwrap();
        assert!(
            s.add_cycle(project, "CYCLE 1", "2026-12-01", "2026-12-14")
                .is_err()
        );
        assert_eq!(s.cycles(project).unwrap()[0].id, two.id);
        s.set_card_cycle(a.id, Some(one.id)).unwrap();
        s.set_card_cycle(b.id, Some(one.id)).unwrap();
        s.set_card_cycle(b.id, Some(two.id)).unwrap();
        assert_eq!(s.card_cycle(b.id).unwrap().unwrap().id, two.id);
        assert_eq!(s.cycle_cards(one.id).unwrap().len(), 1);
        assert_eq!(
            s.close_cycle(one.id, Some(two.id)).unwrap(),
            Some(Vec::<i64>::new())
        );
        // Closed twice, or carried to a closed cycle, changes nothing.
        assert_eq!(s.close_cycle(one.id, None).unwrap(), None);
        assert_eq!(s.close_cycle(two.id, Some(one.id)).unwrap(), None);
        assert!(s.cycle(two.id).unwrap().unwrap().closed_at.is_none());
        assert!(s.card_cycle(a.id).unwrap().is_none());
        assert_eq!(s.cycle_cards(one.id).unwrap()[0].id, a.id);
        assert!(s.cycle_cards(one.id).unwrap()[0].done);
        s.set_card_cycle(b.id, None).unwrap();
        assert!(s.card_cycle(b.id).unwrap().is_none());
        assert!(s.delete_cycle(two.id).unwrap());
        assert!(!s.delete_cycle(two.id).unwrap());
        s.delete_card(a.id, false).unwrap();
        assert!(s.cycle_cards(one.id).unwrap().is_empty());
    }

    #[test]
    fn deleting_the_last_repository_of_a_project_takes_its_cycles() {
        let (_dir, s) = test_store();
        let repo = s.add_repository("web", "/w", "none", "main").unwrap();
        let card = s.add_card(repo.id, "a", "", "person", "Done").unwrap();
        let cycle = s
            .add_cycle(repo.home_project_id, "One", "2026-11-02", "2026-11-15")
            .unwrap();
        s.set_card_cycle(card.id, Some(cycle.id)).unwrap();
        assert!(s.delete_repository(repo.id).unwrap());
        assert!(s.cycle(cycle.id).unwrap().is_none());
    }
}
