//! The outgoing webhooks an owner added (`server::outgoing` delivers to them): the URL, the event kinds and the
//! projects each wants, and which repositories are private. A project is a repository today, so a hook names
//! repositories by id. The secret that signs a hook's deliveries is not here: it is kept in `secrets.toml`.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// The tables of this part, and the flag that makes a repository private.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE outgoing_webhooks (
    id INTEGER PRIMARY KEY,
    url TEXT NOT NULL,
    kinds TEXT NOT NULL,
    projects TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
ALTER TABLE repositories ADD COLUMN private INTEGER NOT NULL DEFAULT 0;
"#;

/// An outgoing webhook: where to post, which events, and of which projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingWebhook {
    pub id: i64,
    pub url: String,
    /// The event kinds it wants, such as `card_moved`; never empty.
    pub kinds: Vec<String>,
    /// The ids of the repositories it names; empty when it names none.
    pub projects: Vec<i64>,
    pub created_at: i64,
}

fn hook(row: &Row) -> rusqlite::Result<OutgoingWebhook> {
    let list = |index: usize| -> rusqlite::Result<String> { row.get(index) };
    let bad = |index: usize, e: serde_json::Error| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    };
    Ok(OutgoingWebhook {
        id: row.get(0)?,
        url: row.get(1)?,
        kinds: serde_json::from_str(&list(2)?).map_err(|e| bad(2, e))?,
        projects: serde_json::from_str(&list(3)?).map_err(|e| bad(3, e))?,
        created_at: row.get(4)?,
    })
}

impl Store {
    pub fn add_outgoing_webhook(
        &self,
        url: &str,
        kinds: &[String],
        projects: &[i64],
    ) -> Result<OutgoingWebhook> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO outgoing_webhooks (url, kinds, projects, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                url,
                serde_json::to_string(kinds)?,
                serde_json::to_string(projects)?,
                at
            ],
        )?;
        Ok(OutgoingWebhook {
            id: conn.last_insert_rowid(),
            url: url.into(),
            kinds: kinds.to_vec(),
            projects: projects.to_vec(),
            created_at: at,
        })
    }

    /// Every outgoing webhook, oldest first.
    pub fn outgoing_webhooks(&self) -> Result<Vec<OutgoingWebhook>> {
        self.rows(
            "SELECT id, url, kinds, projects, created_at FROM outgoing_webhooks ORDER BY id",
            [],
            hook,
        )
    }

    pub fn outgoing_webhook(&self, id: i64) -> Result<Option<OutgoingWebhook>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT id, url, kinds, projects, created_at FROM outgoing_webhooks WHERE id = ?1",
                [id],
                hook,
            )
            .optional()?)
    }

    /// Removes an outgoing webhook; false when there was none.
    pub fn delete_outgoing_webhook(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM outgoing_webhooks WHERE id = ?1", [id])?
            > 0)
    }

    /// Makes a repository private, or public again.
    pub fn set_repository_private(&self, id: i64, private: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE repositories SET private = ?2 WHERE id = ?1",
            params![id, private],
        )?;
        Ok(())
    }

    /// Whether the repository is private; a repository that does not exist is, so that nothing of it is sent.
    pub fn repository_is_private(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .query_row(
                "SELECT private FROM repositories WHERE id = ?1",
                [id],
                |r| r.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_are_kept_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        let a = store
            .add_outgoing_webhook("http://a.test/x", &["card_moved".into()], &[])
            .unwrap();
        let b = store
            .add_outgoing_webhook("http://b.test/x", &["card_added".into()], &[3, 4])
            .unwrap();
        assert_eq!(store.outgoing_webhooks().unwrap(), vec![a.clone(), b]);
        assert_eq!(store.outgoing_webhook(a.id).unwrap(), Some(a.clone()));
        assert!(store.delete_outgoing_webhook(a.id).unwrap());
        assert!(!store.delete_outgoing_webhook(a.id).unwrap());
        assert_eq!(store.outgoing_webhooks().unwrap().len(), 1);
    }

    #[test]
    fn a_repository_is_public_until_made_private() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        let repo = store.add_repository("demo", "/d", "none", "main").unwrap();
        assert!(!store.repository_is_private(repo.id).unwrap());
        store.set_repository_private(repo.id, true).unwrap();
        assert!(store.repository_is_private(repo.id).unwrap());
        assert!(store.repository_is_private(999).unwrap());
    }
}
