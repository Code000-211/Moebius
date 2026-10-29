//! A waiting chat in Pi-Bolt's own interactive mode: the chat's Pi is ended, the interactive mode continues its
//! session with the chat's agent directory and run token, and what it adds to the session file is recorded as the
//! chat's events (see `session_events`). When the interface closes, the chat's Pi starts again on the same session.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use super::process::Place;
use super::reader::{finish, keep};
use super::{Closing, MOEBIUS_AGENT, Paused, PiBolt, lock, run_dir};
use crate::app::App;

impl PiBolt {
    /// Ends the chat's idle Pi and returns Pi's interactive mode on the chat's own agent directory, run token and
    /// sessions, continuing the chat's session; what it adds to the session file is recorded as the chat's events
    /// until `take_back_from_tui`.
    pub(super) fn hand_to_tui(&self, app: &Arc<App>, run_id: i64) -> Result<Command, String> {
        let stored = app.run(run_id).map_err(|e| e.to_string())?;
        let config = self.chat_config(app, run_id, &stored.agent)?;
        let mut paused = lock(&self.paused);
        let ended = || format!("chat {run_id} has ended");
        let run_dir = run_dir(app, run_id);
        // A chat that waited long has no Pi to end.
        let run = if paused.get(&run_id).is_some_and(Paused::is_idle) {
            paused.remove(&run_id).expect("the chat is paused").run
        } else {
            let live = self.live_chat(run_id).ok_or_else(ended)?;
            if !self.end_waiting_pi(&live) {
                return Err(ended());
            }
            live.run
        };
        let agent_dir = self.write_agent_dir(run_dir.join("agent"), &config, &run.token)?;
        // A chat from before the server was started again may lack the lens.
        std::fs::write(run_dir.join("moebius-lens.ts"), super::LENS)
            .map_err(|e| format!("cannot write the lens extension: {e}"))?;
        let sessions = run_dir.join("sessions");
        let place = Place {
            worktree: &run.worktree,
            agent_dir: &agent_dir,
            sessions: &sessions,
            continues: true,
            lens: &run_dir.join("moebius-lens.ts"),
            bridge: &run_dir.join("moebius-bridge.ts"),
            token: &run.token,
            operates: stored.agent == MOEBIUS_AGENT,
        };
        let command = self.command(&config, &place, true);
        let follower = Follower::start(app.clone(), run_id, sessions);
        paused.insert(
            run_id,
            Paused {
                run,
                follower: Some(follower),
                closing: None,
                app: app.clone(),
            },
        );
        Ok(command)
    }

    /// Stops following the chat's session file and starts the chat's Pi again on the same session, waiting for the
    /// next message; a chat that was ended or stopped meanwhile ends instead.
    pub(super) fn take_back_from_tui(&self, app: &Arc<App>, run_id: i64) {
        let mut paused = lock(&self.paused);
        if paused.get(&run_id).is_none_or(|p| p.follower.is_none()) {
            return;
        }
        let Some(Paused {
            run,
            follower,
            closing,
            ..
        }) = paused.remove(&run_id)
        else {
            return;
        };
        if let Some(follower) = follower {
            follower.stop();
        }
        if let Some(closing) = closing {
            return finish(
                app,
                run,
                &self.router,
                &self.live,
                Ok(closing == Closing::Stopped),
            );
        }
        if let Err(e) = self.relaunch(app, &run) {
            let why = format!("cannot start the chat's pi-bolt again after its terminal: {e}");
            finish(app, run, &self.router, &self.live, Err(why));
        }
    }
}

/// How often the session file of a chat in its terminal interface is read.
const FOLLOW_EVERY: Duration = Duration::from_millis(500);

/// Follows the Pi session files of a chat while its terminal interface is open: every [`FOLLOW_EVERY`], the complete
/// lines added to the newest file are recorded as the chat's events (see `session_events`). What the files held when
/// it started is not recorded again.
pub(super) struct Follower {
    stop: std::sync::mpsc::Sender<()>,
    thread: std::thread::JoinHandle<()>,
}

impl Follower {
    fn start(app: Arc<App>, run_id: i64, sessions: PathBuf) -> Follower {
        // How far each file has been read: those there now, to their end.
        let mut read: HashMap<PathBuf, u64> = session_files(&sessions)
            .into_iter()
            .map(|(path, len, _)| (path, len))
            .collect();
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name(format!("pi-bolt terminal {run_id}"))
            .spawn(move || {
                loop {
                    let last = !matches!(
                        stopped.recv_timeout(FOLLOW_EVERY),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                    );
                    for line in new_lines(&sessions, &mut read) {
                        let Ok(entry) = serde_json::from_slice::<Value>(&line) else {
                            continue;
                        };
                        for (kind, data) in session_events(&entry) {
                            keep(&app, run_id, true, kind, &data);
                        }
                    }
                    if last {
                        break;
                    }
                }
            })
            .expect("a thread can be started");
        Follower { stop, thread }
    }

    /// Reads what was added last, then stops.
    fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.thread.join();
    }
}

/// The session files in `sessions`: each one's path, length and when it was last written.
fn session_files(sessions: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let Ok(entries) = std::fs::read_dir(sessions) else {
        return vec![];
    };
    entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some((entry.path(), meta.len(), meta.modified().ok()?))
        })
        .collect()
}

/// The complete lines added to the newest session file in `sessions` since `read` says it was read up to, which then
/// says how far they go. A file that is new is read from its start.
fn new_lines(sessions: &Path, read: &mut HashMap<PathBuf, u64>) -> Vec<Vec<u8>> {
    use std::io::{Seek, SeekFrom};
    let newest = session_files(sessions)
        .into_iter()
        .max_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
    let Some((path, _, _)) = newest else {
        return vec![];
    };
    let from = read.get(&path).copied().unwrap_or(0);
    let mut added = Vec::new();
    let opened = std::fs::File::open(&path).and_then(|mut file| {
        file.seek(SeekFrom::Start(from))?;
        file.read_to_end(&mut added)
    });
    if opened.is_err() {
        return vec![];
    }
    // A line Pi is still writing is read the next time.
    let complete = added.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    added.truncate(complete);
    read.insert(path, from + complete as u64);
    added
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(<[u8]>::to_vec)
        .collect()
}

/// The chat's events for one entry of a Pi session file, in the shapes of the records Pi's RPC mode writes for the
/// same, so the chat reads the same either way: a person's message is a `said` event and starts the agent
/// (`agent_start`); an assistant message is a `message_update` with a `thinking_delta` or `text_delta` for each
/// thinking and text part and a `tool_execution_start` for each tool call, and, unless it ends in a tool call, the
/// end of the agent's turn (`agent_end` with the message, then `agent_settled`); a tool's result is a
/// `tool_execution_end`. Other entries (the header, model changes, the system prompt, ...) are none.
fn session_events(entry: &Value) -> Vec<(&'static str, Value)> {
    if entry["type"] != "message" {
        return vec![];
    }
    let message = &entry["message"];
    let parts = message["content"].as_array().map_or(&[][..], Vec::as_slice);
    match message["role"].as_str() {
        Some("user") => {
            let text = match &message["content"] {
                Value::String(text) => text.clone(),
                _ => parts
                    .iter()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            vec![
                ("said", json!({ "message": text })),
                ("agent_start", json!({})),
            ]
        }
        Some("assistant") => {
            let mut events = Vec::new();
            for (index, part) in parts.iter().enumerate() {
                let delta = |kind: &str, text: &Value| {
                    let update = json!({ "type": kind, "contentIndex": index, "delta": text });
                    ("message_update", json!({ "assistantMessageEvent": update }))
                };
                match part["type"].as_str() {
                    Some("text") => events.push(delta("text_delta", &part["text"])),
                    Some("thinking") => events.push(delta("thinking_delta", &part["thinking"])),
                    Some("toolCall") => events.push((
                        "tool_execution_start",
                        json!({ "toolCallId": part["id"], "toolName": part["name"], "args": part["arguments"] }),
                    )),
                    _ => {}
                }
            }
            if message["stopReason"] != "toolUse" {
                events.push(("agent_end", json!({ "messages": [message] })));
                // Not `aborted` even when the person interrupted it: the chat still waits for them.
                events.push(("agent_settled", json!({ "aborted": false })));
            }
            events
        }
        Some("toolResult") => {
            let mut end = json!({
                "toolCallId": message["toolCallId"],
                "toolName": message["toolName"],
                "result": { "content": message["content"] },
                "isError": message["isError"].as_bool().unwrap_or(false),
            });
            if message["durationMs"].is_number() {
                end["durationMs"] = message["durationMs"].clone();
            }
            vec![("tool_execution_end", end)]
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::harness::pibolt::final_answer;

    #[test]
    fn only_complete_lines_added_to_the_newest_session_file_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old.jsonl");
        std::fs::write(&old, "{\"type\":\"session\"}\n").unwrap();
        let mut read: HashMap<PathBuf, u64> = session_files(dir.path())
            .into_iter()
            .map(|(path, len, _)| (path, len))
            .collect();
        assert!(
            new_lines(dir.path(), &mut read).is_empty(),
            "what was there is not read again"
        );

        let mut file = std::fs::OpenOptions::new().append(true).open(&old).unwrap();
        file.write_all(b"{\"a\":1}\n{\"b\":").unwrap();
        assert_eq!(new_lines(dir.path(), &mut read), [b"{\"a\":1}".to_vec()]);
        file.write_all(b"2}\n").unwrap();
        assert_eq!(new_lines(dir.path(), &mut read), [b"{\"b\":2}".to_vec()]);

        // A session started later is read from its start.
        let new = dir.path().join("new.jsonl");
        std::fs::write(&new, "{\"c\":3}\n").unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(60);
        let opened = std::fs::File::options().write(true).open(&new).unwrap();
        opened.set_modified(later).unwrap();
        assert_eq!(new_lines(dir.path(), &mut read), [b"{\"c\":3}".to_vec()]);
    }

    #[test]
    fn a_session_entry_is_recorded_as_pis_records_of_the_same() {
        let entry = |message: Value| json!({ "type": "message", "id": "a1", "parentId": null, "message": message });
        let kinds = |events: Vec<(&'static str, Value)>| -> Vec<&'static str> {
            events.into_iter().map(|(kind, _)| kind).collect()
        };
        assert!(session_events(&json!({ "type": "session", "version": 3 })).is_empty());
        assert!(session_events(&entry(json!({ "role": "system", "content": "" }))).is_empty());
        let said = session_events(&entry(json!({ "role": "user", "content": "Hi" })));
        assert_eq!(said[0], ("said", json!({ "message": "Hi" })));
        let working = json!({ "role": "assistant", "stopReason": "toolUse", "content": [
            { "type": "text", "text": "Looking." },
            { "type": "toolCall", "id": "c1", "name": "bash", "arguments": { "command": "ls" } }
        ]});
        assert_eq!(
            kinds(session_events(&entry(working))),
            ["message_update", "tool_execution_start"]
        );
        let done = json!({ "role": "assistant", "stopReason": "stop", "content": [{ "type": "text", "text": "Done." }] });
        let events = session_events(&entry(done));
        assert_eq!(
            kinds(events.clone()),
            ["message_update", "agent_end", "agent_settled"]
        );
        assert_eq!(final_answer(&events[1].1).as_deref(), Some("Done."));
    }
}
