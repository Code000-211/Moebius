//! The class of each column of each project: the built-in column it counts as (ADR 0030). A project's custom
//! columns are defined in a file of the harness configuration repository, which SQL cannot read, so the app writes
//! what the file says here whenever it reads it (`app::columns`), and a query asks for the class of a card's column
//! with [`class_sql`]. A built-in column has no row: it counts as itself.
//!
//! A column the file no longer has but cards are still in keeps its row with `present` off, so those cards keep
//! counting as the class they had.

use rusqlite::params;

use super::{Result, Store};

/// The table of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE column_classes (
    project_id INTEGER NOT NULL,
    name TEXT NOT NULL COLLATE NOCASE,
    class TEXT NOT NULL,
    present INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (project_id, name)
);
"#;

/// An SQL expression for the class of the column `column` of a card of the project `project`: the class a custom
/// column counts as, and the column's own name for any other. Both arguments are SQL expressions, such as
/// `c.column_name` and `c.project_id`; `AND <class_sql> = 'Done'` reads the way `AND column_name = 'Done'` did.
pub fn class_sql(column: &str, project: &str) -> String {
    format!(
        "COALESCE((SELECT cc.class FROM column_classes cc WHERE cc.project_id = {project} AND cc.name = {column}), {column})"
    )
}

impl Store {
    /// The custom columns recorded for `project`: name, class and whether the file still has it.
    pub fn column_classes(&self, project: i64) -> Result<Vec<(String, String, bool)>> {
        self.rows(
            "SELECT name, class, present FROM column_classes WHERE project_id = ?1 ORDER BY rowid",
            [project],
            |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0)),
        )
    }

    /// Makes the rows of `project` those of `present` (name and class), and keeps the rows of `kept` (columns the
    /// file lost, with the class they had) with `present` off; any other row goes. Does nothing when the rows are
    /// those already.
    pub fn set_column_classes(
        &self,
        project: i64,
        present: &[(String, String)],
        kept: &[(String, String)],
    ) -> Result<()> {
        let mut wanted: Vec<(&str, &str, i64)> = present
            .iter()
            .map(|(n, c)| (n.as_str(), c.as_str(), 1))
            .collect();
        wanted.extend(kept.iter().map(|(n, c)| (n.as_str(), c.as_str(), 0)));
        let have = self.column_classes(project)?;
        let same = have.len() == wanted.len()
            && have
                .iter()
                .zip(&wanted)
                .all(|((n, c, p), (wn, wc, wp))| n == wn && c == wc && i64::from(*p) == *wp);
        if same {
            return Ok(());
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM column_classes WHERE project_id = ?1",
            [project],
        )?;
        for (name, class, present) in wanted {
            tx.execute(
                "INSERT INTO column_classes (project_id, name, class, present) VALUES (?1, ?2, ?3, ?4)",
                params![project, name, class, present],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The numbers of the cards in project `project` that are in the column `column`, in any case.
    pub fn cards_in_column(&self, project: i64, column: &str) -> Result<Vec<i64>> {
        self.rows(
            "SELECT number FROM cards WHERE project_id = ?1 AND column_name = ?2 COLLATE NOCASE ORDER BY id",
            params![project, column],
            |r| r.get(0),
        )
    }

    /// The numbers of the cards that are in each of the project's columns that are not in `allowed`.
    pub fn cards_outside(
        &self,
        project: i64,
        allowed: &[String],
    ) -> Result<Vec<(String, Vec<i64>)>> {
        let rows: Vec<(String, i64)> = self.rows(
            "SELECT column_name, number FROM cards WHERE project_id = ?1 ORDER BY id",
            [project],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut out: Vec<(String, Vec<i64>)> = Vec::new();
        for (column, number) in rows {
            if allowed.iter().any(|a| a.eq_ignore_ascii_case(&column)) {
                continue;
            }
            match out.iter_mut().find(|(c, _)| *c == column) {
                Some((_, numbers)) => numbers.push(number),
                None => out.push((column, vec![number])),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_replaced_and_a_class_is_looked_up_with_the_expression() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        store
            .set_column_classes(
                1,
                &[("Ready for testing".into(), "In review".into())],
                &[("Old".into(), "Done".into())],
            )
            .unwrap();
        let classes = store.column_classes(1).unwrap();
        assert_eq!(classes.len(), 2);
        assert!(classes[0].2 && !classes[1].2);
        let sql = format!("SELECT {}", class_sql("'ready for testing'", "1"));
        let class: String = store.conn().query_row(&sql, [], |r| r.get(0)).unwrap();
        assert_eq!(class, "In review");
        let sql = format!("SELECT {}", class_sql("'Done'", "1"));
        let class: String = store.conn().query_row(&sql, [], |r| r.get(0)).unwrap();
        assert_eq!(class, "Done");
        let sql = format!("SELECT {}", class_sql("'Ready for testing'", "2"));
        let class: String = store.conn().query_row(&sql, [], |r| r.get(0)).unwrap();
        assert_eq!(class, "Ready for testing");
        store.set_column_classes(1, &[], &[]).unwrap();
        assert!(store.column_classes(1).unwrap().is_empty());
    }
}

/// What the queries decide by the class of a column, with cards in custom columns.
#[cfg(test)]
mod decisions {
    use rusqlite::params;

    use super::*;
    use crate::store::test_store as store;

    /// A repository whose project has the columns Building (In progress), Testing (In review) and Shipped (Done).
    fn board() -> (tempfile::TempDir, Store, crate::store::Repository) {
        let (dir, s) = store();
        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        s.set_column_classes(
            repo.home_project_id,
            &[
                ("Building".into(), "In progress".into()),
                ("Testing".into(), "In review".into()),
                ("Shipped".into(), "Done".into()),
            ],
            &[],
        )
        .unwrap();
        (dir, s, repo)
    }

    #[test]
    fn the_closed_flag_and_the_open_count_follow_the_class() {
        let (_dir, s, repo) = board();
        let shipped = s.add_card(repo.id, "S", "", "leon", "Shipped").unwrap();
        let testing = s.add_card(repo.id, "T", "", "leon", "Testing").unwrap();
        let done = s.add_card(repo.id, "D", "", "leon", "Done").unwrap();
        assert!(s.card(shipped.id).unwrap().unwrap().closed);
        assert!(s.card(done.id).unwrap().unwrap().closed);
        assert!(!s.card(testing.id).unwrap().unwrap().closed);
        assert_eq!(s.open_cards(repo.home_project_id).unwrap(), 1);
        // The same column of another project counts as itself.
        let other = s.add_repository("o", "/o", "none", "main").unwrap();
        let elsewhere = s.add_card(other.id, "E", "", "leon", "Shipped").unwrap();
        assert!(!s.card(elsewhere.id).unwrap().unwrap().closed);
    }

    #[test]
    fn what_goes_wrong_in_a_window_is_found_by_class() {
        let (_dir, s, repo) = board();
        let (from, to) = (1_000, 2_000);
        let card = |title: &str, runs: usize, sent_back: bool| {
            let card = s
                .add_card(repo.id, title, "the issue", "leon", "Backlog")
                .unwrap();
            for _ in 0..runs {
                s.add_run(Some(card.id), "Coder", "pi-bolt", "code")
                    .unwrap();
            }
            if sent_back {
                s.move_card(card.id, "Testing").unwrap();
                s.move_card(card.id, "Building").unwrap();
            }
            s.move_card(card.id, "Shipped").unwrap();
            s.conn()
                .execute(
                    "UPDATE card_moves SET at = ?1 WHERE card_id = ?2",
                    params![from + 10, card.id],
                )
                .unwrap();
            card
        };
        let once = card("Once", 1, false);
        let back = card("Sent back", 1, true);
        assert_eq!(
            s.merged_at_once(from, to).unwrap(),
            [(once.id, "Once".to_string(), "the issue".to_string())]
        );
        let sent = s.sent_back_cards(from, to).unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].card_id, Some(back.id));
        // Both are in a column that counts as Done now, and moved into one in the window.
        let costs = s.done_card_costs(None).unwrap();
        assert_eq!(
            costs.iter().map(|c| c.0).collect::<Vec<_>>(),
            [once.id, back.id]
        );
        assert_eq!(s.done_card_costs(Some((from, to))).unwrap().len(), 2);
        s.move_card(once.id, "Building").unwrap();
        assert_eq!(s.done_card_costs(None).unwrap().len(), 1);
    }

    #[test]
    fn a_board_reads_the_runs_of_the_cards_that_do_not_count_as_done() {
        let (_dir, s, repo) = board();
        let open = s.add_card(repo.id, "Open", "", "leon", "Building").unwrap();
        let shipped = s
            .add_card(repo.id, "Shipped", "", "leon", "Shipped")
            .unwrap();
        for card in [&open, &shipped] {
            s.add_run(Some(card.id), "Coder", "pi-bolt", "code")
                .unwrap();
        }
        let runs = s.runs_for_board(repo.id, "Done").unwrap();
        assert_eq!(
            runs.iter().filter_map(|r| r.card_id).collect::<Vec<_>>(),
            [open.id]
        );
        assert!(s.attempts_for_board(repo.id, "Done").unwrap().is_empty());
        assert!(s.tokens_for_board(repo.id, "Done").unwrap().is_empty());
    }

    #[test]
    fn the_cards_a_person_took_count_until_they_are_in_a_column_that_counts_as_done() {
        let (_dir, s, repo) = board();
        let card = s
            .add_card(repo.id, "Taken", "", "leon", "Building")
            .unwrap();
        s.conn()
            .execute(
                "UPDATE cards SET command_taker = 'lea', forge_assignees = '[\"lea\"]' WHERE id = ?1",
                [card.id],
            )
            .unwrap();
        assert_eq!(s.command_takes(repo.id, "lea").unwrap(), 1);
        s.move_card(card.id, "Shipped").unwrap();
        assert_eq!(s.command_takes(repo.id, "lea").unwrap(), 0);
    }

    #[test]
    fn the_classes_of_a_project_go_when_the_project_does() {
        let (_dir, s, repo) = board();
        let project = repo.home_project_id;
        let kept = s.add_repository("k", "/k", "none", "main").unwrap();
        s.set_column_classes(
            kept.home_project_id,
            &[("Shipped".into(), "Done".into())],
            &[],
        )
        .unwrap();
        assert!(s.delete_repository(repo.id).unwrap());
        assert!(s.column_classes(project).unwrap().is_empty());
        assert_eq!(s.column_classes(kept.home_project_id).unwrap().len(), 1);
    }
}
