//! The inbox: what waits for a person, about a card or about a change, grilling, proposal or takeover, until it is
//! resolved.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboxItem {
    pub id: i64,
    pub kind: String,
    pub card_id: Option<i64>,
    pub title: String,
    pub data: serde_json::Value,
    pub resolved_at: Option<i64>,
    pub at: i64,
}

impl Store {
    pub fn add_inbox_item(
        &self,
        kind: &str,
        card_id: Option<i64>,
        title: &str,
        data: &serde_json::Value,
    ) -> Result<InboxItem> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO inbox_items (kind, card_id, title, data, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![kind, card_id, title, serde_json::to_string(data)?, at],
        )?;
        Ok(InboxItem {
            id: conn.last_insert_rowid(),
            kind: kind.into(),
            card_id,
            title: title.into(),
            data: data.clone(),
            resolved_at: None,
            at,
        })
    }

    /// The items not resolved yet, oldest first.
    pub fn inbox(&self) -> Result<Vec<InboxItem>> {
        self.inbox_where("resolved_at IS NULL AND ?1", 1.into())
    }

    /// All the items about a card, resolved or not, oldest first.
    pub fn inbox_for_card(&self, card_id: i64) -> Result<Vec<InboxItem>> {
        self.inbox_where("card_id = ?1", card_id.into())
    }

    fn inbox_where(&self, clause: &str, param: rusqlite::types::Value) -> Result<Vec<InboxItem>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, kind, card_id, title, data, resolved_at, at FROM inbox_items WHERE {clause} ORDER BY id"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })?;
        rows.map(|row| {
            let (id, kind, card_id, title, data, resolved_at, at) = row?;
            Ok(InboxItem {
                id,
                kind,
                card_id,
                title,
                data: serde_json::from_str(&data)?,
                resolved_at,
                at,
            })
        })
        .collect()
    }

    pub fn resolve_inbox_item(&self, id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 WHERE id = ?2",
            params![now(), id],
        )?;
        Ok(())
    }

    /// Whether the card has an unresolved item of `kind`.
    pub fn has_open_inbox_item(&self, card_id: i64, kind: &str) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM inbox_items WHERE card_id = ?1 AND kind = ?2 AND resolved_at IS NULL)",
            params![card_id, kind],
            |r| r.get(0),
        )?)
    }

    /// Resolves every open item about a card, such as its "ready to merge" once it is merged.
    pub fn resolve_inbox_for_card(&self, card_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 WHERE card_id = ?2 AND resolved_at IS NULL",
            params![now(), card_id],
        )?;
        Ok(())
    }

    /// Resolves the open "ready to merge" item of a change, which has no card to resolve it through.
    pub fn resolve_inbox_for_change(&self, change_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 \
             WHERE kind = 'ready_to_merge' AND json_extract(data, '$.change') = ?2 AND resolved_at IS NULL",
            params![now(), change_id],
        )?;
        Ok(())
    }

    /// Resolves the open inbox item of a grilling, which has no card to resolve it through.
    pub fn resolve_inbox_for_grilling(&self, grilling_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 \
             WHERE kind IN ('grilling_issues', 'grilling_report') \
             AND json_extract(data, '$.grilling') = ?2 AND resolved_at IS NULL",
            params![now(), grilling_id],
        )?;
        Ok(())
    }

    /// Resolves the open inbox item of a proposal, which has no card to resolve it through.
    pub fn resolve_inbox_for_proposal(&self, proposal_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 \
             WHERE kind = 'proposal' AND json_extract(data, '$.proposal') = ?2 AND resolved_at IS NULL",
            params![now(), proposal_id],
        )?;
        Ok(())
    }

    /// Resolves the open inbox item of a takeover, which has no card to resolve it through.
    pub fn resolve_inbox_for_takeover(&self, takeover_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 \
             WHERE kind = 'takeover' AND json_extract(data, '$.takeover') = ?2 AND resolved_at IS NULL",
            params![now(), takeover_id],
        )?;
        Ok(())
    }
}
