//! What an import from another tracker made (issue #390): for each record of the source (an issue, a comment, a sprint,
//! an epic, a user, a link) the Moebius record it became and a note of what it looked like when it was read. The next
//! import finds the record by it, changes it only when the note differs and makes nothing twice. The notes hold no
//! secret: they are the mapped fields of a record, never the credentials the source was read with.

use rusqlite::{OptionalExtension, params};

use super::{Result, Store};

/// The table this module adds.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE imports (
    project_id INTEGER NOT NULL REFERENCES projects(id),
    source TEXT NOT NULL,
    kind TEXT NOT NULL,
    external_id TEXT NOT NULL,
    local_id INTEGER NOT NULL,
    note TEXT NOT NULL,
    PRIMARY KEY (project_id, source, kind, external_id)
);
CREATE INDEX imports_by_local ON imports (source, kind, local_id);
"#;

/// A record an import made: the id it has in Moebius (0 for one that has no record of its own, such as a link) and the
/// note of how the source showed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub local_id: i64,
    pub note: String,
}

impl Store {
    /// What the import from `source` into `project_id` made of the source's record `external_id` of `kind`.
    pub fn imported(
        &self,
        project_id: i64,
        source: &str,
        kind: &str,
        external_id: &str,
    ) -> Result<Option<Imported>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT local_id, note FROM imports \
                 WHERE project_id = ?1 AND source = ?2 AND kind = ?3 AND external_id = ?4",
                params![project_id, source, kind, external_id],
                |r| {
                    Ok(Imported {
                        local_id: r.get(0)?,
                        note: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    /// Remembers that the source's record became `local_id`, as it looked like in `note`; replaces what was kept.
    pub fn set_imported(
        &self,
        project_id: i64,
        source: &str,
        kind: &str,
        external_id: &str,
        local_id: i64,
        note: &str,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO imports (project_id, source, kind, external_id, local_id, note) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![project_id, source, kind, external_id, local_id, note],
        )?;
        Ok(())
    }

    /// Forgets the source's record, so that the next import makes it again.
    pub fn forget_imported(
        &self,
        project_id: i64,
        source: &str,
        kind: &str,
        external_id: &str,
    ) -> Result<()> {
        self.conn().execute(
            "DELETE FROM imports WHERE project_id = ?1 AND source = ?2 AND kind = ?3 AND external_id = ?4",
            params![project_id, source, kind, external_id],
        )?;
        Ok(())
    }

    /// The external ids and records of one `kind` the import into `project_id` made, in the order they were first read.
    pub fn imported_of_kind(
        &self,
        project_id: i64,
        source: &str,
        kind: &str,
    ) -> Result<Vec<(String, Imported)>> {
        self.rows(
            "SELECT external_id, local_id, note FROM imports \
             WHERE project_id = ?1 AND source = ?2 AND kind = ?3 ORDER BY rowid",
            params![project_id, source, kind],
            |r| {
                Ok((
                    r.get(0)?,
                    Imported {
                        local_id: r.get(1)?,
                        note: r.get(2)?,
                    },
                ))
            },
        )
    }

    /// Gives a comment the body it was edited to at its source.
    pub fn set_comment_body(&self, comment_id: i64, body: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE card_comments SET body = ?1 WHERE id = ?2",
            params![body, comment_id],
        )?;
        Ok(())
    }

    /// Says when a card was made, for a card that was made earlier somewhere else (Unix seconds).
    pub fn set_card_created_at(&self, card_id: i64, at: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET created_at = ?1 WHERE id = ?2",
            params![at, card_id],
        )?;
        Ok(())
    }
}
