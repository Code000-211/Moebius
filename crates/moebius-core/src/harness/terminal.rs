//! A person's shell in a run's worktree or in the server's configuration directory, served over a WebSocket
//! (`GET /v1/runs/{id}/terminal` and `GET /v1/terminal`, see the desktop API reference). The shell runs on a pseudo-terminal; binary messages carry its bytes both ways and a text message
//! `{"type":"resize","cols":N,"rows":M}` sets its size. The shell is the person's own, so no check looks at what is
//! typed; Moebius records only that it was opened. The same serving carries Pi-Bolt's own terminal interface for a chat
//! (`GET /v1/chats/{id}/tui`), a program Moebius builds instead of the person's shell.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Deserialize;
use tokio::sync::{mpsc, watch};

/// The most terminals open on one run at a time, and on the server itself.
pub const MAX_PER_RUN: usize = 4;

/// The key of the terminals opened on the server rather than on a run; run ids start at 1.
pub const SERVER: i64 = 0;

/// The key under which the terminal interface of a chat is kept, apart from the shells of its run: at most one is open.
pub fn tui_key(run: i64) -> i64 {
    -run
}

/// The size of a terminal until the client says its own.
const SIZE: (u16, u16) = (80, 24);

#[derive(Debug, thiserror::Error)]
#[error("cannot start a shell: {0}")]
struct StartError(String);

/// The terminals open on one run: how many, and the signal that closes them all.
struct Open {
    count: usize,
    closed: watch::Sender<bool>,
    /// The processes the open terminals run, for the ports they listen on.
    pids: Vec<u32>,
}

/// The terminals each run has open.
#[derive(Default)]
pub struct Slots(Mutex<HashMap<i64, Open>>);

/// A terminal's place among the ones of its run, given back when dropped.
pub struct Slot {
    slots: Arc<Slots>,
    run: i64,
    closed: watch::Receiver<bool>,
    pid: Option<u32>,
}

impl Slots {
    /// A place for one more terminal of `run`, unless it has [`MAX_PER_RUN`] open.
    pub fn take(self: &Arc<Self>, run: i64) -> Option<Slot> {
        self.take_up_to(run, MAX_PER_RUN)
    }

    /// A place for one more terminal under `key`, unless `most` are open.
    pub fn take_up_to(self: &Arc<Self>, key: i64, most: usize) -> Option<Slot> {
        let mut open = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let open = open.entry(key).or_insert_with(|| Open {
            count: 0,
            closed: watch::channel(false).0,
            pids: Vec::new(),
        });
        if open.count >= most {
            return None;
        }
        open.count += 1;
        Some(Slot {
            slots: self.clone(),
            run: key,
            closed: open.closed.subscribe(),
            pid: None,
        })
    }

    /// The processes the open terminals run, each with the key its terminal was opened under.
    pub fn programs(&self) -> Vec<(i64, u32)> {
        let open = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut programs: Vec<_> = open
            .iter()
            .flat_map(|(key, open)| open.pids.iter().map(|pid| (*key, *pid)))
            .collect();
        programs.sort_unstable();
        programs
    }

    /// Closes the terminals open on `run`, its chat's terminal interface included, for when its worktree is gone:
    /// their programs are killed.
    pub fn close(&self, run: i64) {
        self.close_keys(&[run, tui_key(run)]);
    }

    /// Closes the terminal interface of the chat `run`, if it is open, for when the chat ends or stops.
    pub fn close_tui(&self, run: i64) {
        self.close_keys(&[tui_key(run)]);
    }

    fn close_keys(&self, keys: &[i64]) {
        let open = self.0.lock().unwrap_or_else(|e| e.into_inner());
        for key in keys {
            if let Some(open) = open.get(key) {
                open.closed.send_replace(true);
            }
        }
    }

    /// Whether a terminal is open under `key`.
    pub fn is_open(&self, key: i64) -> bool {
        let open = self.0.lock().unwrap_or_else(|e| e.into_inner());
        open.contains_key(&key)
    }
}

impl Slot {
    /// Records the process the terminal runs, until the place is given back.
    fn running(&mut self, pid: u32) {
        let mut open = self.slots.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = open.get_mut(&self.run) {
            entry.pids.retain(|known| Some(*known) != self.pid);
            entry.pids.push(pid);
            self.pid = Some(pid);
        }
    }

    /// Waits until the run's terminals are closed.
    async fn closed(&mut self) {
        let _ = self.closed.wait_for(|closed| *closed).await;
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut open = self.slots.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = open.get_mut(&self.run) {
            entry.pids.retain(|known| Some(*known) != self.pid);
            entry.count -= 1;
            if entry.count == 0 {
                open.remove(&self.run);
            }
        }
    }
}

/// What a client may send as text.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Control {
    Resize { cols: u16, rows: u16 },
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Runs the person's shell in `worktree` for as long as `socket` is open: until either the
/// shell exits, which closes the socket, or the socket closes or the run ends, which kills the shell and its process
/// group.
pub async fn serve(socket: WebSocket, worktree: &Path, mut slot: Slot) {
    serve_command(socket, shell(worktree), &mut slot).await;
}

/// Runs `command` on a pseudo-terminal for as long as `socket` is open, as [`serve`] does the shell. The caller gives
/// back `slot` when it is done with what the program leaves.
pub async fn serve_command(mut socket: WebSocket, command: CommandBuilder, slot: &mut Slot) {
    let (master, mut child, reader, writer) = match start(command) {
        Ok(started) => started,
        Err(e) => {
            let reason = e.to_string();
            let close = CloseFrame {
                code: 1011,
                reason: reason.into(),
            };
            let _ = socket.send(Message::Close(Some(close))).await;
            return;
        }
    };
    let pid = child.process_id();
    if let Some(pid) = pid {
        slot.running(pid);
    }
    let mut output = pump_output(reader);
    let input = pump_input(writer);
    loop {
        tokio::select! {
            bytes = output.recv() => match bytes {
                Some(bytes) => {
                    if socket.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                // The shell exited and everything it wrote has been sent.
                None => {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            },
            () = slot.closed() => {
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Binary(bytes))) => {
                    if input.send(bytes.to_vec()).is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Text(text))) => {
                    if let Ok(Control::Resize { cols, rows }) = serde_json::from_str(text.as_str()) {
                        let _ = master.resize(size(cols, rows));
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
        }
    }
    // Closing the master hangs the shell up; the kill is for one that ignores that. `wait` collects the shell.
    drop(master);
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(pid) = pid {
            crate::platform::kill_tree(pid);
        }
        let _ = child.kill();
        let _ = child.wait();
    })
    .await;
}

fn fail(e: impl std::fmt::Display) -> StartError {
    StartError(e.to_string())
}

type Started = (
    Box<dyn portable_pty::MasterPty + Send>,
    Box<dyn portable_pty::Child + Send + Sync>,
    Box<dyn Read + Send>,
    Box<dyn Write + Send>,
);

/// The person's shell (`$SHELL`, else `/bin/sh`; PowerShell on Windows) in `worktree`.
fn shell(worktree: &Path) -> CommandBuilder {
    let mut command = CommandBuilder::new(crate::platform::shell());
    command.cwd(worktree);
    command
}

/// `command` as a program on a terminal: its program, arguments, working directory and environment.
pub fn on_terminal(command: &std::process::Command) -> CommandBuilder {
    let mut builder = CommandBuilder::new(command.get_program());
    builder.args(command.get_args());
    if let Some(dir) = command.get_current_dir() {
        builder.cwd(dir);
    }
    for (name, value) in command.get_envs() {
        match value {
            Some(value) => builder.env(name, value),
            None => builder.env_remove(name),
        }
    }
    builder
}

/// The program on a new pseudo-terminal, and the two ends of it.
fn start(mut command: CommandBuilder) -> Result<Started, StartError> {
    let pair = native_pty_system()
        .openpty(size(SIZE.0, SIZE.1))
        .map_err(fail)?;
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    let child = pair.slave.spawn_command(command).map_err(fail)?;
    drop(pair.slave);
    let reader = pair.master.try_clone_reader().map_err(fail)?;
    let writer = pair.master.take_writer().map_err(fail)?;
    Ok((pair.master, child, reader, writer))
}

/// Reads what the shell writes on a thread of its own; the channel closes when the shell's side of the terminal does.
fn pump_output(mut reader: Box<dyn Read + Send>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel(64);
    tokio::task::spawn_blocking(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buffer) {
            if n == 0 || tx.blocking_send(buffer[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    rx
}

/// Writes what the client types on a thread of its own, so that a shell that does not read cannot hold up the socket.
fn pump_input(mut writer: Box<dyn Write + Send>) -> mpsc::UnboundedSender<Vec<u8>> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    tokio::task::spawn_blocking(move || {
        while let Some(bytes) = rx.blocking_recv() {
            if writer
                .write_all(&bytes)
                .and_then(|()| writer.flush())
                .is_err()
            {
                break;
            }
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_has_a_few_terminals_and_a_closed_one_makes_room() {
        let slots = Arc::new(Slots::default());
        let open: Vec<_> = (0..MAX_PER_RUN).map(|_| slots.take(1).unwrap()).collect();
        assert!(slots.take(1).is_none());
        assert!(slots.take(2).is_some(), "the limit is per run");
        drop(open);
        assert!(slots.take(1).is_some());
    }

    #[test]
    fn a_chat_has_one_terminal_interface_that_closing_the_run_closes() {
        let slots = Arc::new(Slots::default());
        let key = tui_key(1);
        let first = slots.take_up_to(key, 1).unwrap();
        assert!(slots.take_up_to(key, 1).is_none());
        assert!(slots.is_open(key));
        assert!(slots.take(1).is_some(), "shells are counted apart");
        slots.close(1);
        assert!(*first.closed.borrow());
        drop(first);
        assert!(!slots.is_open(key));
    }

    #[test]
    fn the_programs_of_open_terminals_are_listed_until_their_place_is_given_back() {
        let slots = Arc::new(Slots::default());
        let mut shell = slots.take(3).unwrap();
        let mut server = slots.take(SERVER).unwrap();
        shell.running(30);
        server.running(10);
        server.running(11);
        assert_eq!(slots.programs(), vec![(0, 11), (3, 30)]);
        drop(shell);
        assert_eq!(slots.programs(), vec![(0, 11)]);
        drop(server);
        assert!(slots.programs().is_empty());
    }

    #[test]
    fn a_resize_message_is_read() {
        let Control::Resize { cols, rows } =
            serde_json::from_str(r#"{"type":"resize","cols":100,"rows":30}"#).unwrap();
        assert_eq!((cols, rows), (100, 30));
        assert!(serde_json::from_str::<Control>(r#"{"type":"other"}"#).is_err());
    }
}
