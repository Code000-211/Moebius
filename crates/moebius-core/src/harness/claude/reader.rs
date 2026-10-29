//! Reading a run's Claude Code: every record it writes is mapped to the run events a Pi-Bolt run has and stored and
//! published, and the run ends when Claude Code exits. A run that is no chat is told it is done at its first `result`
//! (its stdin is closed); its last `result` says whether it succeeded. Ending a run is the same as for Pi-Bolt (see
//! `pibolt`): what the agent left is committed, the worktree removed and the token revoked.
//!
//! The mapping: an `assistant` record is `message_start`, a `message_update` for each text (`text_delta`) and thinking
//! (`thinking_delta`) block, a `tool_execution_start` for each `tool_use` block, then `message_end`; each `tool_result`
//! of a `user` record is a `tool_execution_end`; a `result` is `agent_end`, with its text as the final answer, then
//! `agent_settled`. Any other record is kept as `claude_<type>`, such as `claude_system`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::{Closing, Live, lock};
use crate::app::App;
use crate::harness::pibolt::{finish, keep};
use crate::router::Router;

/// Reads one run's records and ends the run.
pub(super) struct Reader {
    pub(super) app: Arc<App>,
    pub(super) live: Live,
    pub(super) router: Arc<Router>,
    pub(super) lives: Arc<Mutex<HashMap<i64, Live>>>,
}

impl Reader {
    /// Reads Claude Code's records from `stdout` until it exits, then ends the run.
    pub(super) fn read(self, stdout: impl Read) {
        let mut lines = BufReader::new(stdout);
        let mut record = Vec::new();
        let mut tools = HashMap::new();
        let mut outcome = Err("claude exited before it answered".to_string());
        loop {
            record.clear();
            match lines.read_until(b'\n', &mut record) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => {
                    outcome = Err(format!("cannot read from claude: {e}"));
                    break;
                }
            }
            let Ok(value) = serde_json::from_slice::<Value>(&record) else {
                let text = String::from_utf8_lossy(&record).trim().to_string();
                if !text.is_empty() {
                    keep_all(
                        &self.app,
                        &self.live,
                        vec![("claude_unparsed".into(), json!({ "text": text }))],
                    );
                }
                continue;
            };
            keep_all(&self.app, &self.live, events_of(&value, &mut tools));
            if value["type"] == "result" {
                outcome = match value["is_error"].as_bool().unwrap_or(false) {
                    false => Ok(()),
                    true => Err(format!(
                        "claude ended with an error: {}",
                        value["result"].as_str().unwrap_or("unknown")
                    )),
                };
                // A run that is no chat is done: Claude Code answers what it was sent meanwhile, then exits.
                if !self.live.chat {
                    lock(&self.live.stdin).take();
                }
            }
        }
        let closing = *lock(&self.live.closing);
        let ending = match closing {
            // The chat goes on in a new process.
            Some(Closing::Restarting) => return,
            Some(Closing::Stopped) => Ok(true),
            Some(Closing::Ended) => Ok(false),
            None if self.live.chat => Err("claude exited while the chat was going".to_string()),
            None => outcome.map(|()| false),
        };
        let _ = lock(&self.live.child).wait();
        finish(&self.app, self.live.run, &self.router, &self.lives, ending);
    }
}

/// Stores and publishes `events` of the run, as Pi-Bolt's records are (see `pibolt`).
fn keep_all(app: &App, live: &Live, events: Vec<(String, Value)>) {
    for (kind, data) in events {
        keep(app, live.run.id, live.chat, &kind, &data);
    }
}

/// The run events of one of Claude Code's records (see the module's documentation). `tools` holds the name of each
/// tool call by its id, for the results.
fn events_of(record: &Value, tools: &mut HashMap<String, String>) -> Vec<(String, Value)> {
    let blocks = record["message"]["content"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    match record["type"].as_str().unwrap_or("unknown") {
        "assistant" => {
            let mut events = vec![("message_start".to_string(), json!({}))];
            for (index, block) in blocks.iter().enumerate() {
                let delta = |kind: &str, text: &Value| {
                    let update = json!({ "type": kind, "contentIndex": index, "delta": text });
                    (
                        "message_update".to_string(),
                        json!({ "assistantMessageEvent": update }),
                    )
                };
                match block["type"].as_str() {
                    Some("text") => events.push(delta("text_delta", &block["text"])),
                    Some("thinking") => events.push(delta("thinking_delta", &block["thinking"])),
                    Some("tool_use") => {
                        let id = block["id"].as_str().unwrap_or_default().to_string();
                        let name = block["name"].as_str().unwrap_or_default().to_string();
                        tools.insert(id.clone(), name.clone());
                        events.push((
                            "tool_execution_start".into(),
                            json!({ "toolCallId": id, "toolName": name, "args": block["input"] }),
                        ));
                    }
                    _ => {}
                }
            }
            events.push(("message_end".into(), json!({})));
            events
        }
        "user" => blocks
            .iter()
            .filter(|block| block["type"] == "tool_result")
            .map(|block| {
                let id = block["tool_use_id"].as_str().unwrap_or_default();
                let content = match &block["content"] {
                    Value::String(text) => json!([{ "type": "text", "text": text }]),
                    other => other.clone(),
                };
                let end = json!({
                    "toolCallId": id,
                    "toolName": tools.get(id).cloned().unwrap_or_default(),
                    "result": { "content": content },
                    "isError": block["is_error"].as_bool().unwrap_or(false),
                });
                ("tool_execution_end".to_string(), end)
            })
            .collect(),
        "result" => {
            let answer = record["result"].as_str().unwrap_or_default();
            let message =
                json!({ "role": "assistant", "content": [{ "type": "text", "text": answer }] });
            let mut end = json!({ "messages": [message], "willRetry": false });
            for key in ["subtype", "is_error", "num_turns", "duration_ms"] {
                if !record[key].is_null() {
                    end[key] = record[key].clone();
                }
            }
            vec![
                ("agent_end".into(), end),
                ("agent_settled".into(), json!({ "aborted": false })),
            ]
        }
        other => vec![(format!("claude_{other}"), record.clone())],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::pibolt::final_answer;

    #[test]
    fn claude_codes_records_become_the_events_of_a_run() {
        let mut tools = HashMap::new();
        let assistant = json!({"type": "assistant", "message": {"content": [
            {"type": "thinking", "thinking": "Hm."}, {"type": "text", "text": "Reading."},
            {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "a"}}]}});
        let events = events_of(&assistant, &mut tools);
        let kinds: Vec<&str> = events.iter().map(|(kind, _)| kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "message_start",
                "message_update",
                "message_update",
                "tool_execution_start",
                "message_end"
            ]
        );
        assert_eq!(
            events[1].1["assistantMessageEvent"]["type"],
            "thinking_delta"
        );
        assert_eq!(events[2].1["assistantMessageEvent"]["delta"], "Reading.");
        assert_eq!(
            events[3].1,
            json!({"toolCallId": "t1", "toolName": "Read", "args": {"file_path": "a"}})
        );

        let user = json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": "no such file", "is_error": true}]}});
        let events = events_of(&user, &mut tools);
        assert_eq!(events[0].0, "tool_execution_end");
        assert_eq!(
            events[0].1,
            json!({"toolCallId": "t1", "toolName": "Read", "isError": true,
                   "result": {"content": [{"type": "text", "text": "no such file"}]}})
        );

        let result =
            json!({"type": "result", "subtype": "success", "is_error": false, "result": "Done."});
        let events = events_of(&result, &mut tools);
        assert_eq!(
            (events[0].0.as_str(), events[1].0.as_str()),
            ("agent_end", "agent_settled")
        );
        assert_eq!(final_answer(&events[0].1).as_deref(), Some("Done."));

        let system = json!({"type": "system", "subtype": "init"});
        assert_eq!(events_of(&system, &mut tools)[0].0, "claude_system");
    }
}
