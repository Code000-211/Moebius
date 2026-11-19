//! The people of the organisation (ADR 0018): each with an id, a handle such as `lea` (written `@lea`), a display
//! name, a role and whether they are disabled. A person is never deleted, only disabled, so that what they did keeps
//! its author: the table refuses a delete.
//!
//! The migration that adds the table makes the first owner (id 1, handle [`FIRST_OWNER`]) and gives them every record
//! from before that a person made: an issue or a comment written through Moebius by `person`, a judgement decided by
//! `person`, and a hold or a claim by `person`, which now say `person:owner`. A card or a comment read from a forge
//! keeps its author even when that login is `person`. A person's access tokens and sessions are in `access`. The actor's form of a person is in `decide`; the author and
//! decider text is `app::person_author`.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, StoreError, now};

/// The handle of the first owner, whom the migration makes.
pub const FIRST_OWNER: &str = "owner";

/// The table of this part, the first owner and the earlier records given to them.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE people (
    id INTEGER PRIMARY KEY,
    handle TEXT NOT NULL UNIQUE COLLATE NOCASE,
    name TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('owner', 'member', 'viewer')),
    disabled INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE TRIGGER people_are_never_deleted BEFORE DELETE ON people
BEGIN
    SELECT RAISE(ABORT, 'a person is never deleted, only disabled');
END;
INSERT INTO people (id, handle, name, role, created_at)
    VALUES (1, 'owner', 'Owner', 'owner', CAST(strftime('%s', 'now') AS INTEGER));
UPDATE cards SET author = 'person:owner'
    WHERE author = 'person'
      AND (confirmed = 1 OR repository_id IN (SELECT id FROM repositories WHERE forge = 'none'));
UPDATE card_comments SET author = 'person:owner' WHERE author = 'person' AND local = 1;
UPDATE judgements SET decider = 'person:owner' WHERE decider = 'person';
UPDATE cards SET held_by = 'person:owner' WHERE held_by = 'person';
UPDATE cards SET claimed_by = 'person:owner' WHERE claimed_by = 'person';
"#;

/// What a person may do in the organisation (ADR 0018).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Everything, including the configuration, the repositories and the people.
    Owner,
    /// Issues and cards, comments, chats and merging a change.
    Member,
    /// Reads, comments, and writes issues that wait in the inbox.
    Viewer,
}

impl Role {
    /// The role as it is stored and shown: `owner`, `member` or `viewer`.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Member => "member",
            Role::Viewer => "viewer",
        }
    }

    /// The role named `name`, if it is one.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "owner" => Some(Role::Owner),
            "member" => Some(Role::Member),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }
}

/// A person of the organisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: i64,
    /// What the person is named by in authors and mentions, without the `@`; unique regardless of case.
    pub handle: String,
    pub name: String,
    pub role: Role,
    /// A disabled person can no longer act, and stays the author of what they did.
    pub disabled: bool,
    pub created_at: i64,
}

impl Person {
    /// The person as an author or decider: `person:<handle>`.
    pub fn author(&self) -> String {
        crate::app::person_author(Some(&self.handle))
    }
}

pub(super) const COLUMNS: &str = "id, handle, name, role, disabled, created_at";

pub(super) fn person(row: &Row) -> rusqlite::Result<Person> {
    let role: String = row.get(3)?;
    Ok(Person {
        id: row.get(0)?,
        handle: row.get(1)?,
        name: row.get(2)?,
        // The table's check allows only the three roles.
        role: Role::parse(&role).unwrap_or(Role::Viewer),
        disabled: row.get(4)?,
        created_at: row.get(5)?,
    })
}

impl Store {
    /// Adds a person with `handle`, `name` and `role`. A handle that is taken, in any case, is an error.
    pub fn add_person(&self, handle: &str, name: &str, role: Role) -> Result<Person> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO people (handle, name, role, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![handle, name, role.as_str(), now()],
            )?;
            conn.last_insert_rowid()
        };
        self.person(id)?
            .ok_or(StoreError::Sql(rusqlite::Error::QueryReturnedNoRows))
    }

    /// The person with `id`, disabled or not.
    pub fn person(&self, id: i64) -> Result<Option<Person>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM people WHERE id = ?1"),
                [id],
                person,
            )
            .optional()?)
    }

    /// The person with `handle`, in any case.
    pub fn person_by_handle(&self, handle: &str) -> Result<Option<Person>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM people WHERE handle = ?1"),
                [handle],
                person,
            )
            .optional()?)
    }

    /// Every person, disabled ones included, in the order they were added.
    pub fn people(&self) -> Result<Vec<Person>> {
        self.rows(
            &format!("SELECT {COLUMNS} FROM people ORDER BY id"),
            [],
            person,
        )
    }

    /// Disables or enables the person with `id`; false when there is none. Disabling ends the person's sessions.
    pub fn set_person_disabled(&self, id: i64, disabled: bool) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE people SET disabled = ?1 WHERE id = ?2",
            params![disabled, id],
        )? == 1;
        if disabled {
            tx.execute("DELETE FROM sessions WHERE person_id = ?1", [id])?;
        }
        tx.commit()?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MIGRATIONS, test_store};

    /// A store from before people: the issues, comments, judgements, holds and claims of `person` belong to the first
    /// owner after the migration; what a forge user named `person` wrote on the forge stays theirs.
    #[test]
    fn a_store_from_before_gives_what_the_person_did_to_the_first_owner() {
        let dir = tempfile::tempdir().unwrap();
        {
            let conn = rusqlite::Connection::open(dir.path().join("moebius.sqlite")).unwrap();
            let people = MIGRATIONS
                .iter()
                .position(|sql| *sql == SCHEMA)
                .expect("a migration adds people");
            for sql in &MIGRATIONS[..people] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", people as i64)
                .unwrap();
            conn.execute_batch(
                "INSERT INTO repositories (id, name, path, forge, default_branch) VALUES (1, 'local', '/srv/local', 'none', 'main');
                 INSERT INTO repositories (id, name, path, forge, default_branch) VALUES (2, 'remote', '/srv/remote', 'github', 'main');
                 INSERT INTO cards (id, repository_id, number, title, body, author, column_name, created_at, held_by, held_at, claimed_by)
                     VALUES (1, 1, 1, 'Mine', '', 'person', 'Backlog', 1, 'person', 1, 'person');
                 INSERT INTO cards (id, repository_id, number, title, body, author, column_name, created_at, confirmed)
                     VALUES (2, 2, 1, 'Opened in Moebius', '', 'person', 'Backlog', 1, 1);
                 INSERT INTO cards (id, repository_id, number, title, body, author, column_name, created_at)
                     VALUES (3, 2, 2, 'From the forge', '', 'person', 'Backlog', 1);
                 INSERT INTO card_comments (card_id, author, body, at, local) VALUES (1, 'person', 'mine', 1, 1);
                 INSERT INTO card_comments (card_id, author, body, at, forge_id, local) VALUES (3, 'person', 'theirs', 1, 9, 0);
                 INSERT INTO judgements (kind, context, options, choice, reason, decider, card_id, at)
                     VALUES ('approve_issue', '[]', '[]', 'approve', '', 'person', 1, 1);
                 INSERT INTO judgements (kind, context, options, choice, reason, decider, at)
                     VALUES ('tool_call', '[]', '[]', 'allow', '', 'guard', 1);",
            )
            .unwrap();
        }
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len());
        let owner = store.person_by_handle(FIRST_OWNER).unwrap().unwrap();
        assert_eq!(owner.role, Role::Owner);
        let mine = store.card(1).unwrap().unwrap();
        assert_eq!(mine.author, owner.author());
        assert_eq!(mine.held.unwrap().by, owner.author());
        assert_eq!(mine.claimed_by, Some(owner.author()));
        assert_eq!(store.card(2).unwrap().unwrap().author, owner.author());
        assert_eq!(
            store.card(3).unwrap().unwrap().author,
            "person",
            "a forge user named person is not the owner"
        );
        let authors = |card| -> Vec<String> {
            store
                .comments(card)
                .unwrap()
                .into_iter()
                .map(|c| c.author)
                .collect()
        };
        assert_eq!(authors(1), [owner.author()]);
        assert_eq!(authors(3), ["person"]);
        let deciders: Vec<String> = store
            .judgements_of_kind_after("approve_issue", 0, 10)
            .unwrap()
            .into_iter()
            .chain(store.judgements_of_kind_after("tool_call", 0, 10).unwrap())
            .map(|j| j.decider)
            .collect();
        assert_eq!(deciders, [owner.author(), "guard".to_string()]);
    }

    #[test]
    fn a_second_persons_issue_is_theirs_and_stays_theirs_when_they_are_disabled() {
        let (_dir, store) = test_store();
        let repository = store.add_repository("w", "/w", "none", "main").unwrap();
        let lea = store.add_person("lea", "Lea", Role::Member).unwrap();
        let card = store
            .add_card(repository.id, "Lea's issue", "", &lea.author(), "Backlog")
            .unwrap();
        assert_eq!(card.author, "person:lea");
        assert!(store.set_person_disabled(lea.id, true).unwrap());
        assert_eq!(store.card(card.id).unwrap().unwrap().author, "person:lea");
        let lea = store.person_by_handle("lea").unwrap().unwrap();
        assert!(lea.disabled);
        assert_eq!(lea.author(), "person:lea");
    }

    #[test]
    fn a_new_store_has_the_first_owner() {
        let (_dir, store) = test_store();
        let people = store.people().unwrap();
        assert_eq!(people.len(), 1);
        let owner = &people[0];
        assert_eq!(
            (owner.id, owner.handle.as_str(), owner.role, owner.disabled),
            (1, FIRST_OWNER, Role::Owner, false)
        );
        assert_eq!(owner.author(), "person:owner");
    }

    #[test]
    fn a_person_is_added_and_found_by_id_and_handle() {
        let (_dir, store) = test_store();
        let lea = store.add_person("lea", "Lea", Role::Member).unwrap();
        assert_eq!(store.person(lea.id).unwrap().as_ref(), Some(&lea));
        assert_eq!(store.person_by_handle("LEA").unwrap().as_ref(), Some(&lea));
        assert_eq!(store.person_by_handle("ada").unwrap(), None);
        assert!(
            store
                .add_person("Lea", "Another Lea", Role::Viewer)
                .is_err()
        );
    }

    #[test]
    fn a_person_is_disabled_and_never_deleted() {
        let (_dir, store) = test_store();
        let lea = store.add_person("lea", "Lea", Role::Viewer).unwrap();
        assert!(store.set_person_disabled(lea.id, true).unwrap());
        assert!(store.person(lea.id).unwrap().unwrap().disabled);
        assert!(!store.set_person_disabled(99, true).unwrap());
        let deleted = store
            .conn()
            .execute("DELETE FROM people WHERE id = ?1", [lea.id]);
        assert!(deleted.is_err());
        assert_eq!(store.people().unwrap().len(), 2);
    }

    #[test]
    fn a_role_reads_back_from_its_name() {
        for role in [Role::Owner, Role::Member, Role::Viewer] {
            assert_eq!(Role::parse(role.as_str()), Some(role));
        }
        assert_eq!(Role::parse("admin"), None);
    }
}
