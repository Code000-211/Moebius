//! Working with people on other servers through the forge (ADR 0027): what the store keeps so that a server knows who
//! took an issue. A person tells the server their login on each forge (`person_logins`); the poll records the
//! logins an issue is assigned to on its forge (`cards.forge_assignees`); a card is *taken by* the first of those that
//! is none of this server's people. A repository says whether the server marks the issues that are ready for agents
//! with a label and whether its agents take every approved issue or only the labelled ones nobody took (columns of
//! `repositories`). A card also keeps why the server could not assign its person on the forge (`assign_problem`).

use std::collections::BTreeMap;

use rusqlite::params;

use super::{Result, Store};

/// The tables and columns of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE person_logins (
    person_id INTEGER NOT NULL REFERENCES people(id),
    forge TEXT NOT NULL,
    login TEXT NOT NULL,
    PRIMARY KEY (person_id, forge)
);
ALTER TABLE cards ADD COLUMN forge_assignees TEXT NOT NULL DEFAULT '[]';
ALTER TABLE cards ADD COLUMN assign_problem TEXT;
ALTER TABLE repositories ADD COLUMN mark_ready INTEGER NOT NULL DEFAULT 0;
ALTER TABLE repositories ADD COLUMN ready_label TEXT NOT NULL DEFAULT 'Ready for agent';
ALTER TABLE repositories ADD COLUMN agents_take TEXT NOT NULL DEFAULT 'all';
ALTER TABLE changes ADD COLUMN outside_author TEXT;
ALTER TABLE changes ADD COLUMN taken_from TEXT;
ALTER TABLE changes ADD COLUMN reviewed_head TEXT;
"#;

/// The label a repository marks ready issues with until it is given another.
pub const DEFAULT_READY_LABEL: &str = "Ready for agent";

/// The forges a person has a login on, in the order the Profile page lists them.
pub const LOGIN_FORGES: [&str; 3] = ["github", "gitlab", "forgejo"];

/// The login a card is taken by, for a `SELECT`: the first login the card's issue is assigned to on its forge that is no
/// person's login there, none when there is none or when a login of this server's people came before it: the forge
/// lists assignees in the order they were added, and the one listed first took the issue first.
pub(super) const CARD_TAKEN_BY: &str = "(SELECT j.value FROM json_each(cards.forge_assignees) j \
     WHERE NOT EXISTS (SELECT 1 FROM person_logins l JOIN repositories r ON r.id = cards.repository_id \
     WHERE l.forge = r.forge AND l.login = j.value COLLATE NOCASE) \
     AND NOT EXISTS (SELECT 1 FROM json_each(cards.forge_assignees) k WHERE k.key < j.key AND EXISTS \
     (SELECT 1 FROM person_logins l2 JOIN repositories r2 ON r2.id = cards.repository_id \
     WHERE l2.forge = r2.forge AND l2.login = k.value COLLATE NOCASE)) \
     ORDER BY j.key LIMIT 1)";

impl Store {
    /// The logins `person` has on forges, by forge name.
    pub fn person_logins(&self, person: i64) -> Result<BTreeMap<String, String>> {
        Ok(self
            .rows(
                "SELECT forge, login FROM person_logins WHERE person_id = ?1 ORDER BY forge",
                [person],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )?
            .into_iter()
            .collect())
    }

    /// Replaces the logins of `person`: a forge with a blank login has none.
    pub fn set_person_logins(&self, person: i64, logins: &BTreeMap<String, String>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM person_logins WHERE person_id = ?1", [person])?;
        for (forge, login) in logins.iter().filter(|(_, login)| !login.trim().is_empty()) {
            tx.execute(
                "INSERT INTO person_logins (person_id, forge, login) VALUES (?1, ?2, ?3)",
                params![person, forge, login.trim()],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Every login anyone of this server has on `forge`, with the handle of the person it is.
    pub fn logins_on(&self, forge: &str) -> Result<Vec<(String, String)>> {
        self.rows(
            "SELECT p.handle, l.login FROM person_logins l JOIN people p ON p.id = l.person_id \
             WHERE l.forge = ?1 ORDER BY p.id",
            [forge],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    /// The logins the issue of `card_id` is assigned to on its forge, as the poll last read them.
    pub fn forge_assignees(&self, card_id: i64) -> Result<Vec<String>> {
        let text: String = self.conn().query_row(
            "SELECT forge_assignees FROM cards WHERE id = ?1",
            [card_id],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Records the logins the card's issue is assigned to on its forge. Says whether that changed anything.
    pub fn set_forge_assignees(&self, card_id: i64, logins: &[String]) -> Result<bool> {
        let text = serde_json::to_string(logins)?;
        Ok(self.conn().execute(
            "UPDATE cards SET forge_assignees = ?1 WHERE id = ?2 AND forge_assignees != ?1",
            params![text, card_id],
        )? > 0)
    }

    /// Records why the card's person could not be assigned on the forge, or that nothing is wrong any more.
    pub fn set_assign_problem(&self, card_id: i64, problem: Option<&str>) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE cards SET assign_problem = ?1 WHERE id = ?2 AND assign_problem IS NOT ?1",
            params![problem, card_id],
        )? > 0)
    }

    /// Marks the change as a pull request someone else opened: by `author`, for a card that was in `taken_from`.
    pub fn set_change_outside(&self, change_id: i64, author: &str, taken_from: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET outside_author = ?1, taken_from = ?2 WHERE id = ?3",
            params![author, taken_from, change_id],
        )?;
        Ok(())
    }

    /// Records the commit the Reviewer read when it was asked to review the pull request the change follows: a review
    /// counts only for that commit.
    pub fn set_change_reviewed_head(&self, change_id: i64, head: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET reviewed_head = ?1 WHERE id = ?2",
            params![head, change_id],
        )?;
        Ok(())
    }

    /// Sets how a repository works with others: whether the server marks ready issues with a label, which label, and
    /// whether its agents take every approved issue (`all`) or only the labelled ones nobody took (`labelled`).
    pub fn set_repository_working_with_others(
        &self,
        id: i64,
        mark_ready: bool,
        ready_label: &str,
        agents_take: &str,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE repositories SET mark_ready = ?1, ready_label = ?2, agents_take = ?3 WHERE id = ?4",
            params![mark_ready, ready_label, agents_take, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (Store::in_memory(dir.path()).unwrap(), dir)
    }

    #[test]
    fn a_card_is_taken_by_the_first_assignee_that_is_nobodys_person() {
        let (s, _dir) = store();
        let repo = s
            .add_github_repository("demo", "/tmp/demo", "main", "o/demo", &[])
            .unwrap();
        let card = s.add_card(repo.id, "t", "b", "someone", "Backlog").unwrap();
        assert_eq!(s.card(card.id).unwrap().unwrap().taken_by, None);

        s.set_forge_assignees(card.id, &["Alice".into(), "bob".into()])
            .unwrap();
        assert_eq!(
            s.card(card.id).unwrap().unwrap().taken_by.as_deref(),
            Some("Alice")
        );

        // A login of one of this server's people, in any case, is not an outsider.
        let owner = s.person_by_handle("owner").unwrap().unwrap();
        s.set_person_logins(
            owner.id,
            &BTreeMap::from([("github".into(), "alice".into())]),
        )
        .unwrap();
        // It came first, so the issue is this server's and bob, listed after it, does not take it.
        assert_eq!(s.card(card.id).unwrap().unwrap().taken_by, None);
        s.set_forge_assignees(card.id, &["bob".into(), "Alice".into()])
            .unwrap();
        assert_eq!(
            s.card(card.id).unwrap().unwrap().taken_by.as_deref(),
            Some("bob")
        );
        // On another forge the same login is nobody's.
        s.set_person_logins(
            owner.id,
            &BTreeMap::from([("gitlab".into(), "alice".into())]),
        )
        .unwrap();
        assert_eq!(
            s.card(card.id).unwrap().unwrap().taken_by.as_deref(),
            Some("bob")
        );
    }

    #[test]
    fn blank_logins_are_dropped_and_the_rest_are_trimmed() {
        let (s, _dir) = store();
        let owner = s.person_by_handle("owner").unwrap().unwrap();
        s.set_person_logins(
            owner.id,
            &BTreeMap::from([
                ("github".into(), " leon ".into()),
                ("gitlab".into(), "  ".into()),
            ]),
        )
        .unwrap();
        assert_eq!(
            s.person_logins(owner.id).unwrap(),
            BTreeMap::from([("github".to_string(), "leon".to_string())])
        );
    }
}
