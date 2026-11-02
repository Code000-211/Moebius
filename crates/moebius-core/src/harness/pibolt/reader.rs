//! Reading a run's Pi: every record it writes is stored as a run event and published, and the run ends when Pi
//! settles (a chat's Pi only when it is aborted) or exits. Ending a run commits what its agent left (a chat's work
//! is offered as a change, a grilling's result read), removes the worktree and revokes the run's token.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{Value, json};

use super::{Closing, Live, lock, shut_down};
use crate::app::App;
use crate::harness::run::{RunningRun, finish, keep};
use crate::router::Router;

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
            if kind == "agent_settled" && (!chat || aborted) {
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
