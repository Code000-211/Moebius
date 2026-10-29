//! Run events: what a harness reported of a run, in order, with every secret redacted before it is stored.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub run_id: i64,
    pub seq: i64,
    pub kind: String,
    pub data: serde_json::Value,
    pub at: i64,
}

impl Store {
    /// Stores an event of a run, with every secret in it redacted, and returns its `seq`.
    pub fn append_event(&self, run_id: i64, kind: &str, data: &serde_json::Value) -> Result<i64> {
        let data = &crate::blobs::redact_json(data);
        let conn = self.conn();
        let seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO events (run_id, seq, kind, data, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![run_id, seq, kind, serde_json::to_string(data)?, now()],
        )?;
        Ok(seq)
    }

    pub fn events(&self, run_id: i64) -> Result<Vec<Event>> {
        self.events_after(run_id, 0, i64::MAX)
    }

    /// The first event of the run of kind `kind`, or with `last` the latest one; none without one.
    pub fn event_of_kind(&self, run_id: i64, kind: &str, last: bool) -> Result<Option<Event>> {
        let order = if last { "DESC" } else { "ASC" };
        let conn = self.conn();
        let found = conn
            .query_row(
                &format!(
                    "SELECT seq, data, at FROM events WHERE run_id = ?1 AND kind = ?2 ORDER BY seq {order} LIMIT 1"
                ),
                params![run_id, kind],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get(2)?)),
            )
            .optional()?;
        found
            .map(|(seq, data, at)| {
                Ok(Event {
                    run_id,
                    seq,
                    kind: kind.into(),
                    data: serde_json::from_str(&data)?,
                    at,
                })
            })
            .transpose()
    }

    /// At most `limit` events of a run with a sequence number above `after`, in order.
    pub fn events_after(&self, run_id: i64, after: i64, limit: i64) -> Result<Vec<Event>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT run_id, seq, kind, data, at FROM events WHERE run_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![run_id, after, limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get(4)?,
            ))
        })?;
        rows.map(|row| {
            let (run_id, seq, kind, data, at) = row?;
            Ok(Event {
                run_id,
                seq,
                kind,
                data: serde_json::from_str(&data)?,
                at,
            })
        })
        .collect()
    }

    /// How many tool calls of a run were refused, by the check or by the MCP gateway.
    pub fn refused_tool_calls(&self, run_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM events WHERE run_id = ?1 AND kind IN ('tool_check', 'mcp_call')
               AND json_extract(data, '$.decision') = 'refuse'",
            [run_id],
            |r| r.get(0),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::store::test_store as store;

    #[test]
    fn refused_mcp_calls_count_as_refused_tool_calls() {
        let (_dir, s) = store();
        let run = s.add_run(None, "Coder", "pi-bolt", "code").unwrap();
        let other = s.add_run(None, "Coder", "pi-bolt", "code").unwrap();
        for (kind, decision) in [
            ("tool_check", "refuse"),
            ("tool_check", "allow"),
            ("mcp_call", "refuse"),
            ("mcp_call", "allow"),
        ] {
            s.append_event(run.id, kind, &json!({ "decision": decision }))
                .unwrap();
        }
        s.append_event(other.id, "mcp_call", &json!({ "decision": "refuse" }))
            .unwrap();
        assert_eq!(s.refused_tool_calls(run.id).unwrap(), 2);
    }

    #[test]
    fn secrets_in_run_events_are_redacted_before_they_are_stored() {
        let (_dir, s) = store();
        let run = s.add_run(None, "Coder", "pi-bolt", "code").unwrap();
        let key = format!("sk-ant-api03-{}", "x".repeat(40));
        let event =
            json!({ "result": { "content": [{ "type": "text", "text": format!("KEY={key}") }] } });
        s.append_event(run.id, "tool_execution_end", &event)
            .unwrap();
        let stored = s.events(run.id).unwrap().pop().unwrap().data;
        assert_eq!(
            stored["result"]["content"][0]["text"],
            "KEY=[redacted:anthropic-key]"
        );
    }
}
