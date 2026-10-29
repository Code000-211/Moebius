//! What went wrong in a window of time, as the Retro collects it (see `retro`): failed and refused tool calls, failed
//! and stopped runs, steering, and cards sent back from review.

use rusqlite::params;

use super::{Result, Store};

/// One thing that went wrong in a window of time, as the Retro collects it. `text` is whatever the source has to say, in
/// full; the report shortens it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub kind: &'static str,
    pub run_id: Option<i64>,
    pub card_id: Option<i64>,
    pub agent: Option<String>,
    pub text: String,
    pub at: i64,
}

impl Store {
    /// The `Found` rows of `sql`, which takes the window as `?1` and `?2` and the run as `?3`.
    fn found(
        &self,
        kind: &'static str,
        sql: &str,
        from: i64,
        to: i64,
        run: Option<i64>,
    ) -> Result<Vec<Found>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![from, to, run], |r| {
            Ok(Found {
                kind: "",
                run_id: r.get(0)?,
                card_id: r.get(1)?,
                agent: r.get(2)?,
                text: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                at: r.get(4)?,
            })
        })?;
        rows.map(|row| Ok(Found { kind, ..row? })).collect()
    }

    /// The tool calls that failed in `[from, to)`: the tool and the start of what it said. A call Moebius refused is
    /// not among them; it is a refusal. With `run`, those of that run only, whatever its purpose; without, those of
    /// every run but the Retro's and the trials'. The same goes for the other sources below.
    pub fn failed_tool_calls(&self, from: i64, to: i64, run: Option<i64>) -> Result<Vec<Found>> {
        self.found(
            "tool_failed",
            "SELECT e.run_id, r.card_id, r.agent,
                    COALESCE(json_extract(e.data, '$.toolName'), 'tool') || ': ' || COALESCE(
                        json_extract(e.data, '$.result.content[0].text'), json_extract(e.data, '$.result'), ''),
                    e.at
             FROM events e JOIN runs r ON r.id = e.run_id
             WHERE e.kind = 'tool_execution_end' AND json_extract(e.data, '$.isError') = 1
               AND COALESCE(json_extract(e.data, '$.result.content[0].text'), '') NOT LIKE 'Moebius refused this call:%'
               AND (r.purpose NOT IN ('retro', 'trial', 'research') AND ?3 IS NULL OR r.id = ?3) AND e.at >= ?1 AND e.at < ?2
             ORDER BY e.at, e.id",
            from,
            to,
            run,
        )
    }

    /// The tool calls the rules refused in `[from, to)`: the tool and the reason.
    pub fn refused_tool_calls_between(
        &self,
        from: i64,
        to: i64,
        run: Option<i64>,
    ) -> Result<Vec<Found>> {
        self.found(
            "tool_refused",
            "SELECT e.run_id, r.card_id, r.agent,
                    COALESCE(json_extract(e.data, '$.tool'), 'tool') || ': ' || COALESCE(json_extract(e.data, '$.reason'), ''),
                    e.at
             FROM events e JOIN runs r ON r.id = e.run_id
             WHERE e.kind = 'tool_check' AND json_extract(e.data, '$.decision') = 'refuse'
               AND (r.purpose NOT IN ('retro', 'trial', 'research') AND ?3 IS NULL OR r.id = ?3) AND e.at >= ?1 AND e.at < ?2
             ORDER BY e.at, e.id",
            from,
            to,
            run,
        )
    }

    /// The runs that failed in `[from, to)`, with the latest `moebius_error` of each.
    pub fn failed_runs(&self, from: i64, to: i64, run: Option<i64>) -> Result<Vec<Found>> {
        self.ended_runs("run_failed", "failed", from, to, run)
    }

    /// The runs that were stopped in `[from, to)`.
    pub fn stopped_runs(&self, from: i64, to: i64, run: Option<i64>) -> Result<Vec<Found>> {
        self.ended_runs("run_stopped", "stopped", from, to, run)
    }

    fn ended_runs(
        &self,
        kind: &'static str,
        status: &str,
        from: i64,
        to: i64,
        run: Option<i64>,
    ) -> Result<Vec<Found>> {
        self.found(
            kind,
            &format!(
                "SELECT r.id, r.card_id, r.agent,
                        (SELECT json_extract(e.data, '$.message') FROM events e
                         WHERE e.run_id = r.id AND e.kind = 'moebius_error' ORDER BY e.seq DESC LIMIT 1),
                        r.ended_at
                 FROM runs r
                 WHERE r.status = '{status}' AND (r.purpose NOT IN ('retro', 'trial', 'research') AND ?3 IS NULL OR r.id = ?3)
                   AND r.ended_at >= ?1 AND r.ended_at < ?2
                 ORDER BY r.ended_at, r.id"
            ),
            from,
            to,
            run,
        )
    }

    /// The messages people sent to running runs in `[from, to)`.
    pub fn steering(&self, from: i64, to: i64, run: Option<i64>) -> Result<Vec<Found>> {
        self.found(
            "steered",
            "SELECT e.run_id, r.card_id, r.agent, json_extract(e.data, '$.message'), e.at
             FROM events e JOIN runs r ON r.id = e.run_id
             WHERE e.kind = 'steered' AND (r.purpose NOT IN ('trial', 'research') AND ?3 IS NULL OR r.id = ?3)
               AND e.at >= ?1 AND e.at < ?2
             ORDER BY e.at, e.id",
            from,
            to,
            run,
        )
    }

    /// The cards sent back from review to coding in `[from, to)`, with the note that said why. The note decides the
    /// kind: failed checks, a merge conflict, or else the Reviewer's findings.
    pub fn sent_back_cards(&self, from: i64, to: i64) -> Result<Vec<Found>> {
        self.rows("SELECT m.card_id, COALESCE(m.note, ''), m.at FROM card_moves m
             WHERE m.from_column = 'In review' AND m.to_column = 'In progress' AND m.at >= ?1 AND m.at < ?2
             ORDER BY m.at, m.id", params![from, to], |r| {
            let note: String = r.get(1)?;
            let kind = if note.starts_with("The checks failed") {
                "checks_failed"
            } else if note.starts_with("The change no longer merges") {
                "conflict"
            } else {
                "review_findings"
            };
            Ok(Found {
                kind,
                run_id: None,
                card_id: Some(r.get(0)?),
                agent: None,
                text: note,
                at: r.get(2)?,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::store::{Usage, test_store as store};

    #[test]
    fn a_retro_collects_the_errors_of_one_local_day_and_nothing_at_or_after_its_end() {
        use crate::improve::retro::report;
        let (_dir, s) = store();
        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        let card = s.add_card(repo.id, "Card", "", "leon", "Backlog").unwrap();
        let (from, to) = s.day_window("2026-03-10").unwrap().unwrap();
        // A local day is 23 to 25 hours long where the clocks change.
        assert!((23 * 3600..=25 * 3600).contains(&(to - from)));
        assert_eq!(s.day_window("2026-02-30").unwrap(), None);
        assert_eq!(s.day_window("today").unwrap(), None);

        let run = s
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        let result = |text: &str| json!({ "toolName": "bash", "isError": true, "result": { "content": [{ "type": "text", "text": text }] } });
        let at = |s: &Store, seq: i64, at: i64| {
            s.conn()
                .execute(
                    "UPDATE events SET at = ?1 WHERE run_id = ?2 AND seq = ?3",
                    params![at, run.id, seq],
                )
                .unwrap();
        };
        // seq 1..4: failed tool calls at the first moment, one second before, at the end, one second before it.
        for (seq, when) in [(1, from), (2, from - 1), (3, to), (4, to - 1)] {
            s.append_event(
                run.id,
                "tool_execution_end",
                &result(&format!("failed {seq}")),
            )
            .unwrap();
            at(&s, seq, when);
        }
        // A call that went well, and one that Moebius refused (which is a refusal, not a failure).
        s.append_event(
            run.id,
            "tool_execution_end",
            &json!({ "toolName": "bash", "isError": false }),
        )
        .unwrap();
        s.append_event(
            run.id,
            "tool_execution_end",
            &result("Moebius refused this call: no-rm"),
        )
        .unwrap();
        let refusal = |decision: &str| json!({ "tool": "bash", "decision": decision, "reason": "no-rm: rm -rf" });
        s.append_event(run.id, "tool_check", &refusal("refuse"))
            .unwrap();
        s.append_event(run.id, "tool_check", &refusal("allow"))
            .unwrap();
        s.append_event(
            run.id,
            "steered",
            &json!({ "message": "use the other file" }),
        )
        .unwrap();
        for seq in 5..=10 {
            at(&s, seq, from + 100 + seq);
        }

        // A run that failed with a long message, a stopped one at the end of the day (the next day's), and a retro run.
        let failed = s.add_run(None, "Reviewer", "pi-bolt", "review").unwrap();
        s.append_event(
            failed.id,
            "moebius_error",
            &json!({ "message": "x".repeat(600) }),
        )
        .unwrap();
        s.set_run_status(failed.id, "failed").unwrap();
        let stopped = s
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        s.set_run_status(stopped.id, "stopped").unwrap();
        let analyst = s
            .add_run(None, "Retro-analyst", "pi-bolt", "retro")
            .unwrap();
        s.set_run_status(analyst.id, "failed").unwrap();
        for (id, ended) in [
            (failed.id, from + 300),
            (stopped.id, to),
            (analyst.id, from + 301),
        ] {
            s.conn()
                .execute(
                    "UPDATE runs SET ended_at = ?1 WHERE id = ?2",
                    params![ended, id],
                )
                .unwrap();
        }

        // Cards sent back from review, one for each note, and moves that are not.
        for (note, when) in [
            ("The checks failed:\ncargo test", from + 400),
            ("The change no longer merges cleanly into main.", from + 401),
            ("The test is missing.", from + 402),
            ("Too late.", to),
        ] {
            s.set_card_note(card.id, note).unwrap();
            s.move_card(card.id, "In review").unwrap();
            s.move_card(card.id, "In progress").unwrap();
            s.conn()
                .execute(
                    "UPDATE card_moves SET at = ?1 WHERE note = ?2 AND to_column = 'In progress'",
                    params![when, note],
                )
                .unwrap();
            s.conn()
                .execute(
                    "UPDATE card_moves SET at = ?1 WHERE to_column = 'In review'",
                    params![from - 5],
                )
                .unwrap();
        }

        let first = report(&s, "2026-03-10").unwrap();
        let seen: Vec<_> = first
            .errors
            .iter()
            .map(|e| {
                (
                    e.kind.as_str(),
                    e.run,
                    e.card,
                    e.agent.as_deref(),
                    e.text.chars().take(20).collect(),
                )
            })
            .collect();
        let (r, c) = (Some(run.id), Some(card.id));
        assert_eq!(
            seen,
            [
                ("tool_failed", r, c, Some("Coder"), "bash: failed 1".into()),
                (
                    "tool_refused",
                    r,
                    c,
                    Some("Coder"),
                    "bash: no-rm: rm -rf".into()
                ),
                ("steered", r, c, Some("Coder"), "use the other file".into()),
                (
                    "run_failed",
                    Some(failed.id),
                    None,
                    Some("Reviewer"),
                    "x".repeat(20)
                ),
                (
                    "checks_failed",
                    None,
                    c,
                    None,
                    "The checks failed:\nc".into()
                ),
                ("conflict", None, c, None, "The change no longer".into()),
                (
                    "review_findings",
                    None,
                    c,
                    None,
                    "The test is missing.".into()
                ),
                ("tool_failed", r, c, Some("Coder"), "bash: failed 4".into()),
            ]
        );
        // The failed call at the first moment of the day is the first; the long text is shortened.
        assert_eq!(first.errors[3].text.chars().count(), 501);
        assert!(first.errors[3].text.ends_with('…'));
        assert!(first.errors.iter().all(|e| e.run != Some(analyst.id)));
        // The next day has a call that failed, the run stopped and the card sent back at its first moment.
        let next = report(&s, "2026-03-11").unwrap();
        let kinds: Vec<&str> = next.errors.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["tool_failed", "run_stopped", "review_findings"]);
        assert!(report(&s, "soon").is_err());
    }

    #[test]
    fn a_trial_run_is_no_part_of_its_cards_work_nor_of_the_days_report() {
        let (_dir, s) = store();
        let repository = s.add_repository("demo", "/demo", "none", "main").unwrap();
        let card = s
            .add_card(repository.id, "Task", "", "person", "Ready")
            .unwrap();
        let code = s.add_run(Some(card.id), "Coder", "pi", "code").unwrap();
        let trial = s.add_run(Some(card.id), "Coder", "pi", "trial").unwrap();
        for run in [code.id, trial.id] {
            s.set_run_status(run, "failed").unwrap();
            s.append_event(run, "moebius_error", &json!({ "message": "boom" }))
                .unwrap();
            s.add_usage(&Usage {
                run_id: Some(run),
                model: "m".into(),
                input_tokens: 10,
                output_tokens: 5,
                cached_tokens: 0,
                duration_ms: 1,
                tools: Vec::new(),
            })
            .unwrap();
        }
        let runs: Vec<i64> = s
            .runs_for_card(card.id)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(runs, [code.id]);
        assert_eq!(s.tokens_for_card(card.id).unwrap(), 15);
        // The day's report leaves the trial out; asked for the run, the same sources find it.
        let day = s.today().unwrap();
        let (from, to) = s.day_window(&day).unwrap().unwrap();
        let day_runs: Vec<_> = s.failed_runs(from, to + 86400, None).unwrap();
        assert_eq!(
            day_runs.iter().map(|f| f.run_id).collect::<Vec<_>>(),
            [Some(code.id)]
        );
        let of_trial = s.failed_runs(0, i64::MAX, Some(trial.id)).unwrap();
        assert_eq!(of_trial.len(), 1);
        assert_eq!(of_trial[0].run_id, Some(trial.id));
        assert_eq!(of_trial[0].text, "boom");
    }
}
