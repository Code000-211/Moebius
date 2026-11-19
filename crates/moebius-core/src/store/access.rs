//! How people prove who they are (ADR 0018): their access tokens, and the sessions of the browsers that signed in.
//!
//! An access token belongs to one person, has a name the person gave it, and lasts until it is revoked; the store
//! keeps when it was last used. A session is what a browser's cookie names: it belongs to a person, ends at its expiry,
//! and, when it was opened with an access token, ends with that token. Disabling a person ends their sessions and makes
//! their tokens refused (see `people`).
//!
//! Neither a token nor a session id is ever stored: only the SHA-256 of each ([`hash_secret`]), so whoever reads the
//! store cannot sign in with what is in it. The tokens and ids are made in the desktop API (`server::api::session`).

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::people::{COLUMNS as PERSON_COLUMNS, person};
use super::{Person, Result, Store, StoreError, now};

/// The tables of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE access_tokens (
    id INTEGER PRIMARY KEY,
    person_id INTEGER NOT NULL REFERENCES people(id),
    name TEXT NOT NULL,
    hash TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    revoked_at INTEGER
);
CREATE INDEX access_tokens_by_person ON access_tokens (person_id, id);
CREATE TABLE sessions (
    hash TEXT PRIMARY KEY,
    person_id INTEGER NOT NULL REFERENCES people(id),
    token_id INTEGER REFERENCES access_tokens(id),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
"#;

/// How many seconds pass before a token's last use is written again: a token used many times a minute is written once.
const LAST_USE_STEP: i64 = 60;

/// An access token as the store keeps it: everything but the token itself, which only its person was shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessToken {
    pub id: i64,
    pub person_id: i64,
    /// What the person calls it, such as `laptop`.
    pub name: String,
    pub created_at: i64,
    /// When it last signed a request or a browser in, to the minute; none when it never has.
    pub last_used_at: Option<i64>,
    /// When it was revoked; a revoked token is refused.
    pub revoked_at: Option<i64>,
}

/// The SHA-256 of a token or a session id, in hexadecimal: what the store keeps in its place.
pub fn hash_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

const TOKEN_COLUMNS: &str = "id, person_id, name, created_at, last_used_at, revoked_at";

fn access_token(row: &Row) -> rusqlite::Result<AccessToken> {
    Ok(AccessToken {
        id: row.get(0)?,
        person_id: row.get(1)?,
        name: row.get(2)?,
        created_at: row.get(3)?,
        last_used_at: row.get(4)?,
        revoked_at: row.get(5)?,
    })
}

/// The people columns of a query that joins `people`.
fn person_columns() -> String {
    PERSON_COLUMNS
        .split(", ")
        .map(|column| format!("people.{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Store {
    /// Keeps `token`, by its hash, as an access token of the person `person_id` named `name`.
    pub fn add_access_token(&self, person_id: i64, name: &str, token: &str) -> Result<AccessToken> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO access_tokens (person_id, name, hash, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![person_id, name, hash_secret(token), now()],
            )?;
            conn.last_insert_rowid()
        };
        self.access_token(id)?
            .ok_or(StoreError::Sql(rusqlite::Error::QueryReturnedNoRows))
    }

    /// The access token with `id`, revoked or not.
    pub fn access_token(&self, id: i64) -> Result<Option<AccessToken>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {TOKEN_COLUMNS} FROM access_tokens WHERE id = ?1"),
                [id],
                access_token,
            )
            .optional()?)
    }

    /// The access tokens of the person `person_id`, revoked ones included, in the order they were made.
    pub fn access_tokens(&self, person_id: i64) -> Result<Vec<AccessToken>> {
        self.rows(
            &format!("SELECT {TOKEN_COLUMNS} FROM access_tokens WHERE person_id = ?1 ORDER BY id"),
            [person_id],
            access_token,
        )
    }

    /// Revokes the access token `id` and ends the sessions it opened; false when there is none or it was revoked
    /// already.
    pub fn revoke_access_token(&self, id: i64) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let revoked = tx.execute(
            "UPDATE access_tokens SET revoked_at = ?1 WHERE id = ?2 AND revoked_at IS NULL",
            params![now(), id],
        )? == 1;
        tx.execute("DELETE FROM sessions WHERE token_id = ?1", [id])?;
        tx.commit()?;
        Ok(revoked)
    }

    /// The person `token` belongs to and the token's id, when it is an access token that is not revoked of a person who
    /// is not disabled. Notes the use, at `at`.
    pub fn person_of_token(&self, token: &str, at: i64) -> Result<Option<(Person, i64)>> {
        let conn = self.conn();
        let found = conn
            .query_row(
                &format!(
                    "SELECT {}, access_tokens.id FROM access_tokens
                     JOIN people ON people.id = access_tokens.person_id
                     WHERE access_tokens.hash = ?1 AND access_tokens.revoked_at IS NULL
                       AND people.disabled = 0",
                    person_columns()
                ),
                [hash_secret(token)],
                |row| Ok((person(row)?, row.get::<_, i64>(6)?)),
            )
            .optional()?;
        if let Some((_, id)) = &found {
            conn.execute(
                "UPDATE access_tokens SET last_used_at = ?1
                 WHERE id = ?2 AND (last_used_at IS NULL OR last_used_at <= ?1 - ?3)",
                params![at, id, LAST_USE_STEP],
            )?;
        }
        Ok(found)
    }

    /// Keeps the session `id`, by its hash, for the person `person_id` until `expires_at`; `token_id` is the access
    /// token it was opened with, whose revocation ends it. Sessions that have expired are forgotten.
    pub fn start_session(
        &self,
        id: &str,
        person_id: i64,
        token_id: Option<i64>,
        expires_at: i64,
    ) -> Result<()> {
        let at = now();
        let conn = self.conn();
        conn.execute("DELETE FROM sessions WHERE expires_at <= ?1", [at])?;
        conn.execute(
            "INSERT INTO sessions (hash, person_id, token_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![hash_secret(id), person_id, token_id, at, expires_at],
        )?;
        Ok(())
    }

    /// The person of the session `id` and the access token it was opened with, while it has not expired at `at`, its
    /// person is not disabled and its token is not revoked.
    pub fn person_of_session(&self, id: &str, at: i64) -> Result<Option<(Person, Option<i64>)>> {
        Ok(self
            .conn()
            .query_row(
                &format!(
                    "SELECT {}, sessions.token_id FROM sessions
                     JOIN people ON people.id = sessions.person_id
                     LEFT JOIN access_tokens ON access_tokens.id = sessions.token_id
                     WHERE sessions.hash = ?1 AND sessions.expires_at > ?2 AND people.disabled = 0
                       AND (sessions.token_id IS NULL OR access_tokens.revoked_at IS NULL)",
                    person_columns()
                ),
                params![hash_secret(id), at],
                |row| Ok((person(row)?, row.get::<_, Option<i64>>(6)?)),
            )
            .optional()?)
    }

    /// Ends the session `id`, if there is one.
    pub fn end_session(&self, id: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM sessions WHERE hash = ?1", [hash_secret(id)])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Role, test_store};

    #[test]
    fn a_token_names_its_person_until_it_is_revoked() {
        let (_dir, store) = test_store();
        let lea = store.add_person("lea", "Lea", Role::Member).unwrap();
        let made = store
            .add_access_token(lea.id, "laptop", "secret-a")
            .unwrap();
        assert_eq!((made.person_id, made.name.as_str()), (lea.id, "laptop"));
        assert_eq!(made.last_used_at, None);

        let (person, id) = store.person_of_token("secret-a", 1000).unwrap().unwrap();
        assert_eq!((person.handle.as_str(), id), ("lea", made.id));
        assert_eq!(store.person_of_token("secret-b", 1000).unwrap(), None);
        assert_eq!(
            store.access_token(made.id).unwrap().unwrap().last_used_at,
            Some(1000)
        );
        // A use within the minute is not written again; a later one is.
        store.person_of_token("secret-a", 1030).unwrap();
        assert_eq!(
            store.access_token(made.id).unwrap().unwrap().last_used_at,
            Some(1000)
        );
        store.person_of_token("secret-a", 1060).unwrap();
        assert_eq!(
            store.access_token(made.id).unwrap().unwrap().last_used_at,
            Some(1060)
        );

        assert!(store.revoke_access_token(made.id).unwrap());
        assert!(!store.revoke_access_token(made.id).unwrap());
        assert_eq!(store.person_of_token("secret-a", 2000).unwrap(), None);
        let listed = store.access_tokens(lea.id).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].revoked_at.is_some());
    }

    #[test]
    fn a_disabled_persons_tokens_and_sessions_are_refused() {
        let (_dir, store) = test_store();
        let lea = store.add_person("lea", "Lea", Role::Viewer).unwrap();
        let token = store.add_access_token(lea.id, "cli", "secret").unwrap();
        store
            .start_session("session", lea.id, Some(token.id), now() + 100)
            .unwrap();
        assert!(store.person_of_session("session", now()).unwrap().is_some());
        store.set_person_disabled(lea.id, true).unwrap();
        assert_eq!(store.person_of_token("secret", now()).unwrap(), None);
        assert_eq!(store.person_of_session("session", now()).unwrap(), None);
        // Enabling the person again does not bring the session back: disabling ended it.
        store.set_person_disabled(lea.id, false).unwrap();
        assert_eq!(store.person_of_session("session", now()).unwrap(), None);
        assert!(store.person_of_token("secret", now()).unwrap().is_some());
    }

    #[test]
    fn a_session_ends_at_its_expiry_with_its_token_or_when_signed_out() {
        let (_dir, store) = test_store();
        let lea = store.add_person("lea", "Lea", Role::Member).unwrap();
        let token = store.add_access_token(lea.id, "web", "secret").unwrap();
        let at = now();
        store
            .start_session("by-token", lea.id, Some(token.id), at + 100)
            .unwrap();
        store
            .start_session("plain", lea.id, None, at + 100)
            .unwrap();
        store.start_session("short", lea.id, None, at + 10).unwrap();

        let (person, from) = store.person_of_session("by-token", at).unwrap().unwrap();
        assert_eq!((person.id, from), (lea.id, Some(token.id)));
        assert!(store.person_of_session("short", at + 9).unwrap().is_some());
        assert_eq!(store.person_of_session("short", at + 10).unwrap(), None);
        assert_eq!(store.person_of_session("unknown", at).unwrap(), None);

        store.revoke_access_token(token.id).unwrap();
        assert_eq!(store.person_of_session("by-token", at).unwrap(), None);
        assert!(store.person_of_session("plain", at).unwrap().is_some());
        store.end_session("plain").unwrap();
        assert_eq!(store.person_of_session("plain", at).unwrap(), None);
    }

    #[test]
    fn neither_a_token_nor_a_session_id_is_kept_as_it_is() {
        let (_dir, store) = test_store();
        let lea = store.add_person("lea", "Lea", Role::Member).unwrap();
        store.add_access_token(lea.id, "cli", "the-token").unwrap();
        store
            .start_session("the-session", lea.id, None, now() + 100)
            .unwrap();
        let conn = store.conn();
        let kept: Vec<String> = conn
            .prepare("SELECT hash FROM access_tokens UNION ALL SELECT hash FROM sessions")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(kept, [hash_secret("the-token"), hash_secret("the-session")]);
        assert_eq!(kept[0].len(), 64);
    }
}
