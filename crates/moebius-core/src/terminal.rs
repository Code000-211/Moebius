//! A person's shell in a run's worktree or in the server's configuration directory, served over a WebSocket
//! (`GET /v1/runs/{id}/terminal` and `GET /v1/terminal`, see the desktop API reference). The shell runs on a pseudo-terminal; binary messages carry its bytes both ways and a text message
//! `{"type":"resize","cols":N,"rows":M}` sets its size. The shell is the person's own, so no check looks at what is
//! typed; Moebius records only that it was opened.

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

/// The size of a terminal until the client says its own.
const SIZE: (u16, u16) = (80, 24);

#[derive(Debug, thiserror::Error)]
#[error("cannot start a shell: {0}")]
struct StartError(String);

/// The terminals open on one run: how many, and the signal that closes them all.
struct Open {
    count: usize,
    closed: watch::Sender<bool>,
}

/// The terminals each run has open.
#[derive(Default)]
pub struct Slots(Mutex<HashMap<i64, Open>>);

/// A terminal's place among the ones of its run, given back when dropped.
pub struct Slot {
    slots: Arc<Slots>,
    run: i64,
    closed: watch::Receiver<bool>,
}

impl Slots {
    /// A place for one more terminal of `run`, unless it has [`MAX_PER_RUN`] open.
    pub fn take(self: &Arc<Self>, run: i64) -> Option<Slot> {
        let mut open = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let open = open.entry(run).or_insert_with(|| Open {
            count: 0,
            closed: watch::channel(false).0,
        });
        if open.count >= MAX_PER_RUN {
            return None;
        }
        open.count += 1;
        Some(Slot {
            slots: self.clone(),
            run,
            closed: open.closed.subscribe(),
        })
    }

    /// Closes the terminals open on `run`, for when its worktree is gone: their shells are killed.
    pub fn close(&self, run: i64) {
        let open = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(open) = open.get(&run) {
            open.closed.send_replace(true);
        }
    }
}

impl Slot {
    /// Waits until the run's terminals are closed.
    async fn closed(&mut self) {
        let _ = self.closed.wait_for(|closed| *closed).await;
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut open = self.slots.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = open.get_mut(&self.run) {
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

/// Runs the person's shell (`$SHELL`, else `/bin/sh`) in `worktree` for as long as `socket` is open: until either the
/// shell exits, which closes the socket, or the socket closes or the run ends, which kills the shell and its process
/// group.
pub async fn serve(mut socket: WebSocket, worktree: &Path, mut slot: Slot) {
    let (master, mut child, reader, writer) = match start(worktree) {
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
            let _ = std::process::Command::new("kill")
                .args(["-KILL", "--", &format!("-{pid}")])
                .output();
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

/// The shell on a new pseudo-terminal, and the two ends of it.
fn start(worktree: &Path) -> Result<Started, StartError> {
    let pair = native_pty_system()
        .openpty(size(SIZE.0, SIZE.1))
        .map_err(fail)?;
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_string());
    let mut command = CommandBuilder::new(shell);
    command.cwd(worktree);
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
    fn a_resize_message_is_read() {
        let Control::Resize { cols, rows } =
            serde_json::from_str(r#"{"type":"resize","cols":100,"rows":30}"#).unwrap();
        assert_eq!((cols, rows), (100, 30));
        assert!(serde_json::from_str::<Control>(r#"{"type":"other"}"#).is_err());
    }
}
