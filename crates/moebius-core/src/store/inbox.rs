//! The inbox: what waits for a person, about a card or about a change, grilling, proposal, takeover or workflow
//! approval, until it is resolved. Its title and data are redacted before they are stored.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};
use crate::blobs::{redact, redact_json};

/// The kind of an inbox item that carries the decision of a workflow run waiting for a person (see `workflows`).
pub const WORKFLOW_APPROVAL: &str = "workflow_approval";

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
        let title = redact(title);
        let data = redact_json(data);
        conn.execute(
            "INSERT INTO inbox_items (kind, card_id, title, data, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![kind, card_id, title, serde_json::to_string(&data)?, at],
        )?;
        Ok(InboxItem {
            id: conn.last_insert_rowid(),
            kind: kind.into(),
            card_id,
            title,
            data,
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
        let rows = stmt.query_map([param], |r| self.inbox_row(r))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn inbox_row(&self, r: &rusqlite::Row<'_>) -> rusqlite::Result<InboxItem> {
        let data: String = r.get(4)?;
        Ok(InboxItem {
            id: r.get(0)?,
            kind: r.get(1)?,
            card_id: r.get(2)?,
            title: r.get(3)?,
            data: serde_json::from_str(&data).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
            resolved_at: r.get(5)?,
            at: r.get(6)?,
        })
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

    /// The open inbox item that carries the decision of workflow run `run_id`, if one is open.
    pub fn inbox_for_workflow_run(&self, run_id: i64) -> Result<Option<InboxItem>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT id, kind, card_id, title, data, resolved_at, at FROM inbox_items \
                 WHERE card_id IS NULL AND kind = ?1 AND json_extract(data, '$.run') = ?2 \
                 AND resolved_at IS NULL ORDER BY id DESC LIMIT 1",
                params![WORKFLOW_APPROVAL, run_id],
                |r| self.inbox_row(r),
            )
            .optional()?)
    }

    /// Resolves the open inbox item that carries the decision of workflow run `run_id`, which has no card to resolve
    /// it through.
    pub fn resolve_inbox_for_workflow_run(&self, run_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 \
             WHERE kind = ?2 AND json_extract(data, '$.run') = ?3 AND resolved_at IS NULL",
            params![now(), WORKFLOW_APPROVAL, run_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::store::test_store;
    use serde_json::json;

    #[test]
    fn the_inbox_item_of_a_workflow_run_is_resolved_for_the_run_and_only() {
        let (_dir, store) = test_store();
        let repository = store
            .add_repository("cancel", "/tmp/cancel", "none", "main")
            .unwrap();
        let run = store
            .add_workflow_run("release", repository.id, "manual", &json!({}))
            .unwrap();
        let other = store
            .add_workflow_run("release", repository.id, "manual", &json!({}))
            .unwrap();
        let run_item = store
            .add_inbox_item(
                "workflow_approval",
                None,
                "The run waits",
                &json!({ "run": run.id }),
            )
            .unwrap();
        let other_item = store
            .add_inbox_item(
                "workflow_approval",
                None,
                "The other run waits",
                &json!({ "run": other.id }),
            )
            .unwrap();
        let not_a_workflow_item = store
            .add_inbox_item("chat", None, "A chat run", &json!({ "run": run.id }))
            .unwrap();

        store.resolve_inbox_for_workflow_run(run.id).unwrap();

        let open = store.inbox().unwrap();
        let open_ids = open.iter().map(|item| item.id).collect::<Vec<_>>();
        assert_eq!(open_ids.len(), 2);
        assert!(!open_ids.contains(&run_item.id));
        assert!(open_ids.contains(&other_item.id));
        assert!(open_ids.contains(&not_a_workflow_item.id));
    }
}
