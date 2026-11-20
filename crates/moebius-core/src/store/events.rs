//! Run events: what a harness reported of a run, in order, with every secret redacted before it is stored.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// How long a streamed delta waits to be written with the others.
const FLUSH_AFTER: Duration = Duration::from_millis(100);

type Observer = Box<dyn Fn(&[Event]) + Send + Sync>;

/// The events waiting to be written, the last `seq` given to each run, and who is told of what was written.
#[derive(Default)]
pub(super) struct EventLog {
    state: Mutex<LogState>,
    /// Held while a batch is written and observed, so that batches are observed in order.
    flushing: Mutex<()>,
    observers: Mutex<Vec<Observer>>,
    /// Whether a flush of waiting deltas is already due.
    armed: AtomicBool,
}

#[derive(Default)]
struct LogState {
    pending: Vec<Event>,
    seqs: HashMap<i64, i64>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Replaces the stored deltas of the message that ended at `end` by as few as hold the same text: neighbouring
/// `text_delta`s, and neighbouring `thinking_delta`s, become the first of them with their pieces joined. The full
/// answer reads the same, with one row where there were hundreds, each of which repeated the message's `usage`.
fn merge_deltas(tx: &Transaction, run_id: i64, end: i64) -> Result<()> {
    let start: i64 = tx.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM events WHERE run_id = ?1 AND kind = 'message_start' AND seq < ?2",
        params![run_id, end],
        |r| r.get(0),
    )?;
    let rows: Vec<(i64, String)> = tx
        .prepare_cached(
            "SELECT seq, data FROM events WHERE run_id = ?1 AND kind = 'message_update' AND seq > ?2 AND seq < ?3 ORDER BY seq",
        )?
        .query_map(params![run_id, start, end], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    // The first row of the group so far: its seq, its data, the delta type and the last seq merged into it.
    let mut group: Option<(i64, serde_json::Value, String, i64)> = None;
    let mut done = Vec::new();
    let mut close = |group: &mut Option<(i64, serde_json::Value, String, i64)>| {
        if let Some((seq, data, _, last)) = group.take() {
            done.push((seq, data, last));
        }
    };
    for (seq, text) in rows {
        let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) else {
            close(&mut group);
            continue;
        };
        let update = &data["assistantMessageEvent"];
        let kind = update["type"].as_str().unwrap_or_default().to_string();
        let mergeable =
            matches!(kind.as_str(), "text_delta" | "thinking_delta") && update["delta"].is_string();
        match &mut group {
            Some((_, first, group_kind, last))
                if mergeable && *group_kind == kind && *last + 1 == seq =>
            {
                let joined = format!(
                    "{}{}",
                    first["assistantMessageEvent"]["delta"]
                        .as_str()
                        .unwrap_or_default(),
                    update["delta"].as_str().unwrap_or_default()
                );
                first["assistantMessageEvent"]["delta"] = joined.into();
                *last = seq;
            }
            _ => {
                close(&mut group);
                if mergeable {
                    group = Some((seq, data, kind, seq));
                }
            }
        }
    }
    close(&mut group);
    for (seq, data, last) in done.into_iter().filter(|(seq, _, last)| seq != last) {
        tx.execute(
            "UPDATE events SET data = ?3 WHERE run_id = ?1 AND seq = ?2",
            params![run_id, seq, serde_json::to_string(&data)?],
        )?;
        tx.execute(
            "DELETE FROM events WHERE run_id = ?1 AND seq > ?2 AND seq <= ?3 AND kind = 'message_update'",
            params![run_id, seq, last],
        )?;
    }
    Ok(())
}

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
        Ok(self.append_redacted_event(run_id, kind, data)?.0)
    }

    /// Like `append_event`, and returns the event's data as stored, so that it can be published without redacting it
    /// again. The event is written, together with every event that waits to be (see `queue_delta`), before this
    /// returns, and handed to the observers (see `observe_events`) after it was committed.
    pub fn append_redacted_event(
        &self,
        run_id: i64,
        kind: &str,
        data: &serde_json::Value,
    ) -> Result<(i64, serde_json::Value)> {
        let queued = self.queue(run_id, kind, data)?;
        self.flush_events()?;
        Ok((queued.seq, queued.data))
    }

    /// Queues a streamed delta of a message (`message_update`) of a run, redacted, to be written with the events
    /// around it: after `FLUSH_AFTER` at the latest, or with the next event that is written at once. A burst of
    /// deltas becomes one transaction instead of one per token. What the agent said is in the `message_end` that
    /// follows, so a crash loses at most `FLUSH_AFTER` of a stream that its end would have repeated.
    pub fn queue_delta(self: &Arc<Self>, run_id: i64, data: &serde_json::Value) -> Result<()> {
        self.queue(run_id, "message_update", data)?;
        if !self.events.armed.swap(true, Ordering::AcqRel) {
            let store = Arc::clone(self);
            std::thread::spawn(move || {
                std::thread::sleep(FLUSH_AFTER);
                store.events.armed.store(false, Ordering::Release);
                let _ = store.flush_events();
            });
        }
        Ok(())
    }

    /// Calls `observer` with the events of every batch that was committed, in the order of their `seq`, under the
    /// lock that orders the batches. How the bus is told of a run's events.
    pub fn observe_events(&self, observer: impl Fn(&[Event]) + Send + Sync + 'static) {
        lock(&self.events.observers).push(Box::new(observer));
    }

    fn queue(&self, run_id: i64, kind: &str, data: &serde_json::Value) -> Result<Event> {
        let data = crate::blobs::redact_json(data);
        let mut log = lock(&self.events.state);
        let next = match log.seqs.get(&run_id) {
            Some(last) => last + 1,
            None => self.conn().query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE run_id = ?1",
                [run_id],
                |r| r.get(0),
            )?,
        };
        log.seqs.insert(run_id, next);
        let event = Event {
            run_id,
            seq: next,
            kind: kind.to_string(),
            data,
            at: now(),
        };
        log.pending.push(event.clone());
        Ok(event)
    }

    /// Writes every queued event in one transaction, merges the deltas of each message that ended in it, and hands the
    /// events to the observers. A failed write leaves the events queued for the next flush.
    pub fn flush_events(&self) -> Result<()> {
        let _ordered = lock(&self.events.flushing);
        let batch = std::mem::take(&mut lock(&self.events.state).pending);
        if batch.is_empty() {
            return Ok(());
        }
        if let Err(e) = self.write_batch(&batch) {
            let mut log = lock(&self.events.state);
            let later = std::mem::take(&mut log.pending);
            log.pending = batch;
            log.pending.extend(later);
            return Err(e);
        }
        for observer in lock(&self.events.observers).iter() {
            observer(&batch);
        }
        Ok(())
    }

    fn write_batch(&self, batch: &[Event]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            // A run that was deleted meanwhile has no place for its events.
            let mut insert = tx.prepare_cached(
                "INSERT INTO events (run_id, seq, kind, data, at)
                 SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM runs WHERE id = ?1)",
            )?;
            for e in batch {
                insert.execute(params![
                    e.run_id,
                    e.seq,
                    e.kind,
                    serde_json::to_string(&e.data)?,
                    e.at
                ])?;
            }
        }
        for e in batch.iter().filter(|e| e.kind == "message_end") {
            merge_deltas(&tx, e.run_id, e.seq)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn events(&self, run_id: i64) -> Result<Vec<Event>> {
        self.events_after(run_id, 0, i64::MAX)
    }

    /// The first event of the run of kind `kind`, or with `last` the latest one; none without one.
    pub fn event_of_kind(&self, run_id: i64, kind: &str, last: bool) -> Result<Option<Event>> {
        let order = if last { "DESC" } else { "ASC" };
        let conn = self.read();
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
        let conn = self.read();
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

    /// The newest `limit` events of a run, oldest first: the end of its log, for a page that reads a long chat
    /// backwards from where it finished rather than from where it began.
    pub fn events_tail(&self, run_id: i64, limit: i64) -> Result<Vec<Event>> {
        self.events_before(run_id, i64::MAX, limit)
    }

    /// At most `limit` events of a run with a sequence number below `before`, in order: the piece of the log above
    /// the one a client holds already.
    pub fn events_before(&self, run_id: i64, before: i64, limit: i64) -> Result<Vec<Event>> {
        let conn = self.read();
        let mut stmt = conn.prepare(
            "SELECT run_id, seq, kind, data, at FROM events WHERE run_id = ?1 AND seq < ?2 ORDER BY seq DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![run_id, before, limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get(4)?,
            ))
        })?;
        let mut page: Vec<Event> = rows
            .map(|row| {
                let (run_id, seq, kind, data, at) = row?;
                Ok(Event {
                    run_id,
                    seq,
                    kind,
                    data: serde_json::from_str(&data)?,
                    at,
                })
            })
            .collect::<Result<_>>()?;
        // The newest were asked for first, so the page comes back with its end at the front; a chat is read from its
        // beginning whatever way the reader came.
        page.reverse();
        Ok(page)
    }

    /// How many tool calls of a run were refused, by the check or by the MCP gateway.
    pub fn refused_tool_calls(&self, run_id: i64) -> Result<i64> {
        Ok(self.read().query_row(
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
    fn a_long_log_is_read_backwards_from_its_end() {
        let (_dir, s) = store();
        let run = s.add_run(None, "Coder", "pi-bolt", "chat").unwrap();
        for piece in 1..=10 {
            s.append_event(run.id, "message_update", &json!({ "piece": piece }))
                .unwrap();
        }
        let seqs = |events: Vec<super::Event>| events.iter().map(|e| e.seq).collect::<Vec<_>>();
        assert_eq!(seqs(s.events_tail(run.id, 3).unwrap()), vec![8, 9, 10]);
        assert_eq!(seqs(s.events_before(run.id, 8, 3).unwrap()), vec![5, 6, 7]);
        // A page below the beginning is short, not empty of a log that has none left above it.
        assert_eq!(seqs(s.events_before(run.id, 3, 5).unwrap()), vec![1, 2]);
        // Another run's events are not a page of this one's.
        let other = s.add_run(None, "Coder", "pi-bolt", "chat").unwrap();
        s.append_event(other.id, "message_update", &json!({ "piece": 1 }))
            .unwrap();
        assert_eq!(seqs(s.events_tail(other.id, 500).unwrap()), vec![1]);
    }

    #[test]
    fn an_event_is_redacted_once_and_handed_back_as_stored() {
        let (_dir, s) = store();
        let run = s.add_run(None, "Coder", "pi-bolt", "code").unwrap();
        let key = format!("sk-ant-api03-{}", "x".repeat(40));
        let (seq, data) = s
            .append_redacted_event(run.id, "said", &json!({ "message": key }))
            .unwrap();
        assert_eq!(data["message"], "[redacted:anthropic-key]");
        assert_eq!(s.events(run.id).unwrap()[0].data, data);
        assert_eq!(s.events(run.id).unwrap()[0].seq, seq);
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

    /// Waits until `done` holds, without a time limit of its own: the test ends when the harness's does.
    fn until(done: impl Fn() -> bool) {
        while !done() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn streamed(piece: &str, kind: &str) -> serde_json::Value {
        json!({ "assistantMessageEvent": { "type": kind, "delta": piece }, "usage": { "input": 1 } })
    }

    #[test]
    fn a_burst_of_deltas_is_written_in_far_fewer_transactions_than_deltas() {
        let (_dir, s) = store();
        let s = std::sync::Arc::new(s);
        let run = s.add_run(None, "Coder", "pi-bolt", "code").unwrap();
        let batches = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Vec<i64>>::new()));
        let seen = batches.clone();
        s.observe_events(move |events| {
            seen.lock()
                .unwrap()
                .push(events.iter().map(|e| e.seq).collect());
        });
        for i in 0..500 {
            s.queue_delta(run.id, &streamed(&format!("t{i} "), "text_delta"))
                .unwrap();
        }
        until(|| batches.lock().unwrap().iter().map(Vec::len).sum::<usize>() == 500);
        let batches = batches.lock().unwrap().clone();
        assert!(
            batches.len() < 50,
            "{} transactions for 500 deltas",
            batches.len()
        );
        // Observed in order, none lost, none twice.
        let seqs: Vec<i64> = batches.into_iter().flatten().collect();
        assert_eq!(seqs, (1..=500).collect::<Vec<_>>());
        assert_eq!(s.events(run.id).unwrap().len(), 500);
    }

    #[test]
    fn an_event_written_at_once_carries_the_waiting_deltas_with_it_in_order() {
        let (_dir, s) = store();
        let run = s.add_run(None, "Coder", "pi-bolt", "code").unwrap();
        let s = std::sync::Arc::new(s);
        let published = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(i64, String)>::new()));
        let seen = published.clone();
        s.observe_events(move |events| {
            seen.lock()
                .unwrap()
                .extend(events.iter().map(|e| (e.seq, e.kind.clone())));
        });
        s.queue_delta(run.id, &streamed("a", "text_delta")).unwrap();
        s.queue_delta(run.id, &streamed("b", "text_delta")).unwrap();
        let seq = s.append_event(run.id, "agent_end", &json!({})).unwrap();
        assert_eq!(seq, 3);
        // Stored and published before `append_event` returned.
        assert_eq!(s.events(run.id).unwrap().len(), 3);
        assert_eq!(
            published
                .lock()
                .unwrap()
                .iter()
                .map(|e| e.0)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }

    #[test]
    fn the_deltas_of_a_finished_message_are_joined_and_the_answer_reads_the_same() {
        let (_dir, s) = store();
        let run = s.add_run(None, "Coder", "pi-bolt", "chat").unwrap();
        let s = std::sync::Arc::new(s);
        s.append_event(run.id, "message_start", &json!({})).unwrap();
        for piece in ["Hel", "lo ", "wor", "ld"] {
            s.queue_delta(run.id, &streamed(piece, "text_delta"))
                .unwrap();
        }
        s.queue_delta(run.id, &streamed("hmm", "thinking_delta"))
            .unwrap();
        s.append_event(run.id, "message_end", &json!({ "message": "Hello world" }))
            .unwrap();
        let events = s.events(run.id).unwrap();
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "message_start",
                "message_update",
                "message_update",
                "message_end"
            ]
        );
        assert_eq!(
            events[1].data["assistantMessageEvent"]["delta"],
            "Hello world"
        );
        assert_eq!(events[2].data["assistantMessageEvent"]["delta"], "hmm");
        // Later events keep counting from the highest seq; a client that read up to the end misses nothing.
        let next = s.append_event(run.id, "agent_end", &json!({})).unwrap();
        assert_eq!(next, 8);
        assert_eq!(s.events_after(run.id, 7, 10).unwrap().len(), 1);
    }

    #[test]
    fn a_reader_is_not_blocked_by_a_write_transaction_that_is_held() {
        let (_dir, s) = store();
        let run = s.add_run(None, "Coder", "pi-bolt", "code").unwrap();
        s.append_event(run.id, "said", &json!({ "message": "hi" }))
            .unwrap();
        let s = std::sync::Arc::new(s);
        let writer = s.conn();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        writer
            .execute("UPDATE events SET kind = 'changed'", [])
            .unwrap();
        let reading = {
            let s = s.clone();
            std::thread::spawn(move || s.events(run.id).unwrap())
        };
        // The thread finishes while the write transaction is still open, and reads the last commit.
        let events = reading.join().unwrap();
        assert_eq!(events[0].kind, "said");
        writer.execute_batch("COMMIT").unwrap();
    }

    #[test]
    fn events_of_a_deleted_run_are_dropped_not_fatal_to_the_batch() {
        let (_dir, s) = store();
        let kept = s.add_run(None, "Coder", "pi-bolt", "code").unwrap();
        let s = std::sync::Arc::new(s);
        s.queue_delta(9999, &streamed("x", "text_delta")).unwrap();
        s.queue_delta(kept.id, &streamed("y", "text_delta"))
            .unwrap();
        s.flush_events().unwrap();
        assert_eq!(s.events(kept.id).unwrap().len(), 1);
        assert!(s.events(9999).unwrap().is_empty());
    }

    #[test]
    fn the_store_syncs_normally_with_a_write_ahead_log() {
        let (_dir, s) = store();
        let sync: i64 = s
            .conn()
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sync, 1);
        let mode: String = s
            .conn()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }
}
