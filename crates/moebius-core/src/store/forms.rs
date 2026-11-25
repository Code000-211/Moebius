//! What Moebius has written on a forge by its templates (ADR 0034), so that each thing is written once: the settings
//! column of `work_on_forge`, the draft state and the parsed fields of a change, and the log of posts (`forge_posts`) a
//! change has (its review of a round and head, a review answer, the reply that names a section a body lacks). Whatever
//! reads a post's text later, such as the next round's prompt, finds it here.

use rusqlite::{OptionalExtension, params};

use super::{Result, Store, now};

/// The tables and columns of this part.
pub(super) const SCHEMA: &str = r#"
ALTER TABLE flow_settings ADD COLUMN work_on_forge INTEGER NOT NULL DEFAULT 1;
ALTER TABLE changes ADD COLUMN draft INTEGER NOT NULL DEFAULT 0;
ALTER TABLE changes ADD COLUMN pull_fields TEXT;
CREATE TABLE forge_posts (
    change_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    key TEXT NOT NULL,
    text TEXT NOT NULL DEFAULT '',
    url TEXT,
    at INTEGER NOT NULL,
    PRIMARY KEY (change_id, kind, key)
);
"#;

/// A post of a change: its kind (`review`, `answer`, `missing`), the key that makes it the same post (a review's round
/// and head), what it said and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgePost {
    pub kind: String,
    pub key: String,
    pub text: String,
    pub url: Option<String>,
    pub at: i64,
}

impl Store {
    /// Whether the change's pull request is a draft, and what its body said.
    pub fn set_change_draft(&self, change_id: i64, draft: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET draft = ?2 WHERE id = ?1",
            params![change_id, draft],
        )?;
        Ok(())
    }

    /// Keeps what the change's pull request body said, as JSON.
    pub fn set_change_pull_fields(&self, change_id: i64, fields: Option<&str>) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET pull_fields = ?2 WHERE id = ?1",
            params![change_id, fields],
        )?;
        Ok(())
    }

    /// Claims the post `(kind, key)` of the change: true the first time, false when it was claimed before. The caller
    /// writes it only on true; a write that fails is given back with [`Store::release_forge_post`].
    pub fn claim_forge_post(
        &self,
        change_id: i64,
        kind: &str,
        key: &str,
        text: &str,
    ) -> Result<bool> {
        Ok(self.conn().execute(
            "INSERT OR IGNORE INTO forge_posts (change_id, kind, key, text, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![change_id, kind, key, text, now()],
        )? > 0)
    }

    /// Gives a claim back, because the write it was for failed, so that the next try writes it.
    pub fn release_forge_post(&self, change_id: i64, kind: &str, key: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM forge_posts WHERE change_id = ?1 AND kind = ?2 AND key = ?3",
            params![change_id, kind, key],
        )?;
        Ok(())
    }

    /// Says where a claimed post is on the forge.
    pub fn set_forge_post_url(
        &self,
        change_id: i64,
        kind: &str,
        key: &str,
        url: &str,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE forge_posts SET url = ?4 WHERE change_id = ?1 AND kind = ?2 AND key = ?3",
            params![change_id, kind, key, url],
        )?;
        Ok(())
    }

    /// The posts of the change of `kind`, oldest first.
    pub fn forge_posts(&self, change_id: i64, kind: &str) -> Result<Vec<ForgePost>> {
        self.rows(
            "SELECT kind, key, text, url, at FROM forge_posts WHERE change_id = ?1 AND kind = ?2 ORDER BY at, rowid",
            params![change_id, kind],
            |r| {
                Ok(ForgePost {
                    kind: r.get(0)?,
                    key: r.get(1)?,
                    text: r.get(2)?,
                    url: r.get(3)?,
                    at: r.get(4)?,
                })
            },
        )
    }

    /// The newest post of the change of `kind`.
    pub fn latest_forge_post(&self, change_id: i64, kind: &str) -> Result<Option<ForgePost>> {
        Ok(self.forge_posts(change_id, kind)?.pop())
    }

    /// Whether the change has a post of `kind` with `key`.
    pub fn has_forge_post(&self, change_id: i64, kind: &str, key: &str) -> Result<bool> {
        Ok(self
            .read()
            .query_row(
                "SELECT 1 FROM forge_posts WHERE change_id = ?1 AND kind = ?2 AND key = ?3",
                params![change_id, kind, key],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
}
