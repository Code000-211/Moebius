//! Reading a run's Pi: every record it writes is stored as a run event and published, and the run ends when Pi
//! settles (a chat's Pi only when it is aborted) or exits. Ending a run commits what its agent left (a chat's work
//! is offered as a change, a grilling's result read), removes the worktree and revokes the run's token.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{Value, json};

use super::runs::RunningRun;
use super::{Closing, GRILLING_AGENT, Live, lock, shut_down};
use crate::app::App;
use crate::bus::AppEvent;
use crate::router::Router;

/// The text of the last assistant message in an `agent_end` record, if it has any.
pub(crate) fn final_answer(agent_end: &Value) -> Option<String> {
    let messages = agent_end["messages"].as_array()?;
    let last = messages.iter().rev().find(|m| m["role"] == "assistant")?;
    let text: String = last["content"]
        .as_array()?
        .iter()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    Some(text)
}

/// Reads one run's records and ends the run.
pub(super) struct Reader {
    pub(super) app: Arc<App>,
    pub(super) run: RunningRun,
    pub(super) live: Live,
    pub(super) router: Arc<Router>,
    pub(super) lives: Arc<Mutex<HashMap<i64, Live>>>,
}

impl Reader {
    /// Reads Pi's records from `stdout` until the run ends, then ends it.
    pub(super) fn read(self, stdout: impl Read) {
        let mut lines = BufReader::new(stdout);
        let mut record = Vec::new();
        let mut ending = Err("pi-bolt exited before it settled".to_string());
        loop {
            record.clear();
            match lines.read_until(b'\n', &mut record) {
                Ok(0) => {
                    match *lock(&self.live.closing) {
                        // The chat goes on in its terminal interface, which `close_tui` ends.
                        Some(Closing::Paused) => return,
                        Some(closing) => ending = Ok(closing == Closing::Stopped),
                        None => {}
                    }
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    ending = Err(format!("cannot read from pi-bolt: {e}"));
                    break;
                }
            }
            while matches!(record.last(), Some(b'\n' | b'\r')) {
                record.pop();
            }
            if record.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_slice(&record).unwrap_or_else(|e| {
                json!({ "type": "unparsed", "error": e.to_string(), "text": String::from_utf8_lossy(&record) })
            });
            let kind = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let chat = self.run.purpose == "chat";
            let aborted = value["aborted"] == true;
            // Before the chat's status, which says the same, so that a chat that reads as waiting has waited.
            if chat && kind == "agent_start" {
                *lock(&self.live.waiting_since) = None;
            }
            if chat && kind == "agent_settled" && !aborted {
                *lock(&self.live.waiting_since) = Some(Instant::now());
            }
            keep(&self.app, self.run.id, chat, &kind, &value);
            if kind == "response" && value["command"] == "prompt" && value["success"] == false {
                ending = Err(format!("pi-bolt refused the prompt: {}", value["error"]));
                break;
            }
            if kind == "agent_settled" && !(chat && !aborted) {
                ending = Ok(aborted);
                break;
            }
        }
        self.end(ending);
    }

    /// Ends Pi, then the run: `Ok(aborted)` when Pi settled, `Err(why)` when it failed.
    fn end(self, ending: Result<bool, String>) {
        shut_down(&self.live);
        finish(&self.app, self.run, &self.router, &self.lives, ending);
    }
}

/// Stores what a record of Pi says about the run (the final answer as its summary; for a chat, whether the agent works
/// or waits for the person), then the record as a run event. The run is updated before the event is published, so
/// that a client acting on the event reads the run as it is now.
pub(in crate::harness) fn keep(app: &App, run_id: i64, chat: bool, kind: &str, value: &Value) {
    if kind == "agent_end"
        && let Some(answer) = final_answer(value)
    {
        let _ = app.store.set_run_summary(run_id, &answer);
    }
    if chat && kind == "agent_start" {
        let _ = app.store.set_run_status(run_id, "running");
    }
    // A chat waits for the person's next message; only an abort ends it.
    if chat && kind == "agent_settled" && value["aborted"] != true {
        let _ = app.store.set_run_status(run_id, "waiting");
    }
    let _ = app.record_run_event(run_id, kind, value);
}

/// Ends a run whose Pi is gone: `Ok(aborted)` when it settled, `Err(why)` when it failed. What the agent left is
/// committed (and a chat's work offered as a change), the worktree removed, the token revoked and the end published.
pub(in crate::harness) fn finish<L>(
    app: &Arc<App>,
    run: RunningRun,
    router: &Router,
    lives: &Mutex<HashMap<i64, L>>,
    ending: Result<bool, String>,
) {
    router.revoke_token(&run.token);
    let failed = |why: &str| {
        let _ = app.record_run_event(run.id, "moebius_error", &json!({ "message": why }));
        "failed"
    };
    // A grilling leaves its result for Moebius to read, not a change to offer.
    let grilling =
        run.purpose == "chat" && app.run(run.id).is_ok_and(|run| run.agent == GRILLING_AGENT);
    let status = match ending {
        Ok(aborted) if grilling => {
            if aborted {
                "stopped"
            } else {
                "succeeded"
            }
        }
        Ok(aborted) if run.purpose == "chat" => match run.commit() {
            // A chat that changed nothing has still succeeded.
            Ok(_) if aborted => "stopped",
            Ok(_) => "succeeded",
            Err(e) => failed(&e.to_string()),
        },
        Ok(true) => "stopped",
        Ok(false) if !matches!(run.purpose.as_str(), "code" | "trial") => "succeeded",
        Ok(false) => match run.commit() {
            Ok(true) => "succeeded",
            Ok(false) => failed("the agent changed nothing"),
            Err(e) => failed(&e.to_string()),
        },
        Err(why) => failed(&why),
    };
    if grilling && status != "failed" {
        // Before the worktree is removed, with the result in it.
        if let Err(e) = crate::harness::grilling::conclude(app, run.id, &run.worktree) {
            failed(&format!("cannot read the result of the grilling: {e}"));
        }
    }
    run.remove_worktree(app);
    if run.purpose == "chat" && !grilling && status != "failed" {
        // Before the status is set, so that an ended chat has offered what it made.
        if let Err(e) = app.offer_chat_change(run.id) {
            failed(&format!("cannot offer the chat's work as a change: {e}"));
        }
    }
    let _ = app.store.set_run_status(run.id, status);
    lock(lives).remove(&run.id);
    app.bus.publish(AppEvent::new(
        "run_ended",
        json!({ "run": run.id, "status": status }),
    ));
}
