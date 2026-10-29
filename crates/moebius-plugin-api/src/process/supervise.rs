//! Looking after a plugin process: starting it, reading its manifest and its replies, handing each reply to the call
//! that waits for it, logging its standard error, and starting it again after a pause when it stops.

use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{FIRST_PAUSE, LONGEST_PAUSE, Shared, State};
use crate::{Envelope, PluginError, Refusal};

/// Starts the process, again and again after a pause whenever it stops, until the [`Process`] is dropped.
pub(super) fn look_after(shared: &Shared, stop: &mpsc::Receiver<()>) {
    let mut pause = FIRST_PAUSE;
    loop {
        let started = Instant::now();
        let (why, refusal) = run(shared);
        let unanswered: Vec<_> = {
            let mut inner = shared.inner();
            shared.stdin().take();
            if let Some(mut child) = inner.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            inner.pending.drain().map(|(_, tx)| tx).collect()
        };
        for tx in unanswered {
            let _ = tx.send(Err(refusal.clone()));
        }
        let stopped = shared.stopped.load(Ordering::SeqCst);
        if started.elapsed() >= LONGEST_PAUSE {
            pause = FIRST_PAUSE;
        }
        if !stopped {
            (shared.log)(&format!(
                "moebius: {} stopped: {why}; it is started again in {} s",
                shared.spec.name,
                pause.as_secs()
            ));
        }
        {
            let mut inner = shared.inner();
            inner.status.state = if stopped {
                State::Stopped
            } else {
                State::Restarting
            };
            inner.status.error = Some(why);
            inner.tried = true;
        }
        shared.tried.notify_all();
        if stopped
            || !matches!(
                stop.recv_timeout(pause),
                Err(mpsc::RecvTimeoutError::Timeout)
            )
        {
            shared.inner().status.state = State::Stopped;
            return;
        }
        pause = (pause * 2).min(LONGEST_PAUSE);
        let mut inner = shared.inner();
        inner.status.restarts += 1;
        inner.status.state = State::Starting;
    }
}

/// Runs the process once, until it stops; returns why, and the refusal for the calls it did not answer.
fn run(shared: &Shared) -> (String, Refusal) {
    let spec = &shared.spec;
    let mut child = match Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(&spec.dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            let why = format!("cannot start {}: {e}", spec.program.display());
            return (why.clone(), shared.not_running(why));
        }
    };
    let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let errors = read_lines(stderr);
    let log = shared.log.clone();
    let _ = std::thread::Builder::new()
        .name(format!("plugin process {} errors", spec.name))
        .spawn(move || {
            for line in errors {
                log(&line);
            }
        });
    let lines = read_lines(stdout);
    {
        let mut inner = shared.inner();
        inner.child = Some(child);
        *shared.stdin() = stdin;
        if shared.stopped.load(Ordering::SeqCst)
            && let Some(child) = inner.child.as_mut()
        {
            let _ = child.kill();
        }
    }
    let manifest = match lines.recv_timeout(shared.limit) {
        Ok(line) => match serde_json::from_str::<Envelope>(&line) {
            Ok(Envelope::Manifest { manifest }) if manifest.name == spec.name => manifest,
            Ok(Envelope::Manifest { manifest }) => {
                let why = format!(
                    "its manifest names {:?}, not {:?}",
                    manifest.name, spec.name
                );
                return (why.clone(), shared.not_running(why));
            }
            _ => {
                let why = format!("its first line is not a manifest: {}", cut(&line));
                return (why.clone(), shared.not_running(why));
            }
        },
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let why = format!("it sent no manifest within {} ms", shared.limit.as_millis());
            return (why.clone(), shared.not_running(why));
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let why = format!("it exited before its manifest{}", exit_status(shared));
            return (why.clone(), shared.not_running(why));
        }
    };
    {
        let mut inner = shared.inner();
        inner.manifest = Some(manifest);
        inner.status.state = State::Running;
        inner.tried = true;
    }
    shared.tried.notify_all();
    for line in lines {
        let (id, answer) = match serde_json::from_str::<Envelope>(&line) {
            Ok(Envelope::Reply { id, result }) => (id, Ok(result)),
            Ok(Envelope::Error { id, message }) => (
                id,
                Err(Refusal::Failed {
                    plugin: spec.name.clone(),
                    error: PluginError::Failed(message),
                }),
            ),
            _ => {
                let why = format!("it wrote a line that is not a reply: {}", cut(&line));
                let refusal = Refusal::BadReply {
                    plugin: spec.name.clone(),
                    message: why.clone(),
                };
                return (why, refusal);
            }
        };
        // A reply nobody waits for answers a call that timed out; it is dropped.
        if let Some(tx) = shared.inner().pending.remove(&id) {
            let _ = tx.send(answer);
        }
    }
    let why = format!("it exited{}", exit_status(shared));
    (why.clone(), shared.not_running(why))
}

/// ` (<status>)` of the process, which has closed its output: it is given a second to exit before it is killed.
fn exit_status(shared: &Shared) -> String {
    let Some(mut child) = shared.inner().child.take() else {
        return String::new();
    };
    let closed = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if closed.elapsed() < Duration::from_secs(1) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                break child.wait();
            }
            Err(e) => break Err(e),
        }
    };
    match status {
        Ok(status) => format!(" ({status})"),
        Err(_) => String::new(),
    }
}

/// The lines of `output`, read on a thread of their own, until it ends.
fn read_lines(output: Option<impl Read + Send + 'static>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    if let Some(output) = output {
        let _ = std::thread::Builder::new()
            .name("plugin process output".into())
            .spawn(move || {
                let mut reader = BufReader::new(output);
                let mut line = Vec::new();
                while matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
                    let text = String::from_utf8_lossy(&line)
                        .trim_end_matches(['\n', '\r'])
                        .to_string();
                    if tx.send(text).is_err() {
                        return;
                    }
                    line.clear();
                }
            });
    }
    rx
}

/// At most the first 200 characters of `line`, for a message.
fn cut(line: &str) -> String {
    match line.char_indices().nth(200) {
        Some((end, _)) => format!("{}...", &line[..end]),
        None => line.to_string(),
    }
}
