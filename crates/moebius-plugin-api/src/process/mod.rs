//! A plugin that runs as its own program and speaks [`Envelope`]s as JSON Lines on its standard input and output.
//!
//! [`Process::start`] starts the program and reads its manifest; the [`Host`](crate::Host) then sends it calls and waits
//! for the replies with its time limit, as it does for a compiled-in plugin. A process that exits, writes a line that is
//! not a reply or sends no manifest in time is started again after a pause that doubles from one second to a minute, and
//! every call it has not answered is refused. Each line of its standard error, and each time it stops, is handed to the
//! log function it was started with.
//!
//! `supervise` is the thread that looks after one process, and `serve` the other side: a Rust plugin run as a process.

mod serve;
mod supervise;

pub use serve::serve;

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Call, Envelope, Manifest, PluginError, Refusal};

/// The first pause before a process that stopped is started again.
const FIRST_PAUSE: Duration = Duration::from_secs(1);

/// The longest pause; a process that ran at least this long starts again after [`FIRST_PAUSE`].
const LONGEST_PAUSE: Duration = Duration::from_secs(60);

/// The program to run as a plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    /// The name the plugin is added under; its manifest must say the same.
    pub name: String,
    pub program: PathBuf,
    pub args: Vec<String>,
    /// The directory the program runs in.
    pub dir: PathBuf,
}

/// Where a plugin process is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Started, its manifest not read yet.
    Starting,
    /// Its manifest was read, and it takes calls.
    Running,
    /// It stopped and waits to be started again; calls are refused meanwhile.
    Restarting,
    /// Moebius stopped it for good.
    Stopped,
}

/// How a plugin process is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessStatus {
    /// The program that runs.
    pub program: PathBuf,
    pub state: State,
    /// How many times it was started again after it stopped.
    pub restarts: u64,
    /// Why it stopped the last time, if it ever did.
    pub error: Option<String>,
}

type Log = Arc<dyn Fn(&str) + Send + Sync>;

struct Inner {
    status: ProcessStatus,
    manifest: Option<Manifest>,
    child: Option<Child>,
    pending: HashMap<u64, mpsc::Sender<Result<Value, Refusal>>>,
    next_id: u64,
    /// Whether the first start has ended, with the manifest read or not.
    tried: bool,
}

struct Shared {
    spec: ProcessSpec,
    limit: Duration,
    log: Log,
    inner: Mutex<Inner>,
    /// Kept apart from `inner`, since a write can wait on a plugin that does not read.
    stdin: Mutex<Option<ChildStdin>>,
    tried: Condvar,
    stopped: AtomicBool,
}

impl Shared {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn stdin(&self) -> MutexGuard<'_, Option<ChildStdin>> {
        self.stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn not_running(&self, message: impl Into<String>) -> Refusal {
        Refusal::NotRunning {
            plugin: self.spec.name.clone(),
            message: message.into(),
        }
    }
}

/// A plugin in its own process, kept running until it is dropped.
pub struct Process {
    shared: Arc<Shared>,
    /// Dropped to wake the thread that looks after the process.
    _stop: mpsc::Sender<()>,
}

impl Process {
    /// Starts `spec` and waits until its manifest is read or the first start failed, so that the process takes calls
    /// as soon as this returns unless it is broken. `limit` is how long the manifest and every call may take; `log`
    /// gets each line of the process's standard error and a line each time it stops.
    pub fn start(
        spec: ProcessSpec,
        limit: Duration,
        log: impl Fn(&str) + Send + Sync + 'static,
    ) -> Process {
        let program = spec.program.clone();
        let shared = Arc::new(Shared {
            spec,
            limit,
            log: Arc::new(log),
            inner: Mutex::new(Inner {
                status: ProcessStatus {
                    program,
                    state: State::Starting,
                    restarts: 0,
                    error: None,
                },
                manifest: None,
                child: None,
                pending: HashMap::new(),
                next_id: 1,
                tried: false,
            }),
            stdin: Mutex::new(None),
            tried: Condvar::new(),
            stopped: AtomicBool::new(false),
        });
        let (stop, stopped) = mpsc::channel();
        let looked_after = shared.clone();
        let started = std::thread::Builder::new()
            .name(format!("plugin process {}", shared.spec.name))
            .spawn(move || supervise::look_after(&looked_after, &stopped));
        match started {
            Ok(_) => {
                let mut inner = shared.inner();
                while !inner.tried {
                    inner = shared
                        .tried
                        .wait(inner)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
            }
            Err(e) => {
                let mut inner = shared.inner();
                inner.status.state = State::Stopped;
                inner.status.error = Some(format!("cannot start a thread for it: {e}"));
            }
        }
        Process {
            shared,
            _stop: stop,
        }
    }

    pub fn status(&self) -> ProcessStatus {
        self.shared.inner().status.clone()
    }

    /// The manifest the process sent, or its name with no kinds before it sent one.
    pub(crate) fn manifest(&self) -> Manifest {
        self.shared
            .inner()
            .manifest
            .clone()
            .unwrap_or_else(|| Manifest {
                name: self.shared.spec.name.clone(),
                version: String::new(),
                kinds: Vec::new(),
            })
    }

    /// Why the process cannot take a call now, if it cannot.
    pub(crate) fn unavailable(&self) -> Option<Refusal> {
        let inner = self.shared.inner();
        match inner.status.state {
            State::Running => None,
            State::Starting => Some(self.shared.not_running("it is starting")),
            State::Restarting | State::Stopped => Some(
                self.shared.not_running(
                    inner
                        .status
                        .error
                        .clone()
                        .unwrap_or_else(|| "it stopped".into()),
                ),
            ),
        }
    }

    /// Sends `call` and waits for its answer, at most the time limit.
    pub(crate) fn request(&self, call: Call) -> Result<Value, Refusal> {
        if let Some(refusal) = self.unavailable() {
            return Err(refusal);
        }
        let (tx, rx) = mpsc::channel();
        let id = {
            let mut inner = self.shared.inner();
            let id = inner.next_id;
            inner.next_id += 1;
            inner.pending.insert(id, tx);
            id
        };
        let forget = || {
            self.shared.inner().pending.remove(&id);
        };
        let mut line = serde_json::to_string(&Envelope::Call { id, call }).map_err(|e| {
            forget();
            Refusal::Failed {
                plugin: self.shared.spec.name.clone(),
                error: PluginError::Failed(format!("cannot write the call: {e}")),
            }
        })?;
        line.push('\n');
        let written = match self.shared.stdin().as_mut() {
            Some(stdin) => stdin
                .write_all(line.as_bytes())
                .and_then(|()| stdin.flush())
                .map_err(|e| format!("cannot write to it: {e}")),
            None => Err("it is not running".to_string()),
        };
        if let Err(message) = written {
            forget();
            return Err(self.shared.not_running(message));
        }
        match rx.recv_timeout(self.shared.limit) {
            Ok(answer) => answer,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                forget();
                Err(Refusal::TimedOut {
                    plugin: self.shared.spec.name.clone(),
                    millis: self.shared.limit.as_millis().try_into().unwrap_or(u64::MAX),
                })
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(self.shared.not_running("it stopped before it answered"))
            }
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        self.shared.stdin().take();
        if let Some(child) = self.shared.inner().child.as_mut() {
            let _ = child.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::time::Instant;

    use super::*;
    use crate::{DecideRequest, Decision, Host, Kind, Plugin};

    /// Far beyond what an answering plugin needs on a slow machine.
    const LIMIT: Duration = Duration::from_secs(30);

    /// The test plugin in `mode`: started by its `#!` line, or on Windows, which has none, by `python`.
    fn spec(mode: &str, args: &[&str]) -> ProcessSpec {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let script = dir.join("plugin.py").display().to_string();
        let (program, script) = if cfg!(windows) {
            ("python".into(), Some(script))
        } else {
            (dir.join("plugin.py"), None)
        };
        ProcessSpec {
            name: "test".into(),
            program,
            args: script
                .into_iter()
                .chain(
                    std::iter::once(mode)
                        .chain(args.iter().copied())
                        .map(String::from),
                )
                .collect(),
            dir,
        }
    }

    /// A host with the test plugin in `mode`, and what it logged.
    fn host(mode: &str, limit: Duration) -> (Host, Arc<Mutex<Vec<String>>>) {
        let logged = Arc::new(Mutex::new(Vec::new()));
        let log = logged.clone();
        let process = Process::start(spec(mode, &[]), limit, move |line| {
            log.lock().unwrap().push(line.to_string());
        });
        let mut host = Host::new(limit);
        host.add_process(process);
        (host, logged)
    }

    fn request() -> DecideRequest {
        DecideRequest {
            kind: "tool_call".into(),
            context: BTreeMap::from([("tool".into(), "ls".into())]),
            options: vec!["allow".into(), "refuse".into()],
        }
    }

    /// Waits, at most a minute, until `done` holds.
    fn eventually(mut done: impl FnMut() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "never happened"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_process_sends_its_manifest_and_answers_calls_and_its_errors_are_logged() {
        let (host, logged) = host("answers", LIMIT);
        let status = host.status("test").unwrap();
        assert_eq!(status.state, State::Running);
        assert_eq!(status.restarts, 0);
        let manifest = &host.manifests()[0];
        assert_eq!(manifest.version, "1.2.3");
        assert_eq!(manifest.kinds, vec![Kind::Decider]);
        let calls: Vec<_> = (0..4)
            .map(|_| {
                let host = host.clone();
                std::thread::spawn(move || host.decide("test", &request()))
            })
            .collect();
        for call in calls {
            assert_eq!(call.join().unwrap().unwrap().choice, "allow");
        }
        assert!(matches!(
            host.call("test", Call::new(Kind::Forge, "issues", ())),
            Err(Refusal::WrongKind { .. })
        ));
        eventually(|| logged.lock().unwrap().iter().any(|line| line == "started"));
    }

    #[test]
    fn an_error_a_bad_line_and_a_hang_are_refused() {
        let (errors, _) = host("errors", LIMIT);
        assert!(matches!(
            errors.decide("test", &request()),
            Err(Refusal::Failed { error: PluginError::Failed(ref m), .. }) if m == "no"
        ));
        let (garbage, logged) = host("garbage", LIMIT);
        let refused = garbage.decide("test", &request());
        assert!(
            matches!(refused, Err(Refusal::BadReply { ref message, .. }) if message.contains("not json")),
            "{refused:?}"
        );
        eventually(|| {
            logged.lock().unwrap().iter().any(|line| {
                line.starts_with("moebius: test stopped: it wrote a line that is not a reply")
            })
        });
        let (hangs, _) = host("hangs", Duration::from_millis(300));
        assert!(matches!(
            hangs.decide("test", &request()),
            Err(Refusal::TimedOut { millis: 300, .. })
        ));
    }

    #[test]
    fn a_crash_refuses_the_call_and_the_process_is_started_again() {
        let (host, _) = host("crashes", LIMIT);
        let refused = host.decide("test", &request());
        assert!(
            matches!(refused, Err(Refusal::NotRunning { ref message, .. }) if message.contains("exited")),
            "{refused:?}"
        );
        eventually(|| {
            let status = host.status("test").unwrap();
            status.restarts >= 1 && status.state == State::Running
        });
        assert!(
            host.status("test")
                .unwrap()
                .error
                .unwrap()
                .contains("exited")
        );
    }

    #[test]
    fn a_process_that_does_not_start_refuses_every_call() {
        for (mode, args, why) in [
            ("exits", &[][..], "exited before its manifest"),
            ("answers", &["other"][..], "its manifest names \"other\""),
            ("silent", &[][..], "no manifest within 2000 ms"),
        ] {
            // Long enough for Python to start on a slow Windows machine.
            let process = Process::start(spec(mode, args), Duration::from_secs(2), |_| {});
            let status = process.status();
            assert_eq!(status.state, State::Restarting, "{mode}");
            assert!(status.error.as_deref().unwrap().contains(why), "{status:?}");
            let mut host = Host::new(LIMIT);
            host.add_process(process);
            let refused = host.decide("test", &request());
            assert!(
                matches!(refused, Err(Refusal::NotRunning { ref message, .. }) if message.contains(why)),
                "{refused:?}"
            );
        }
        let missing = ProcessSpec {
            program: "/nonexistent/plugin".into(),
            ..spec("answers", &[])
        };
        let status = Process::start(missing, LIMIT, |_| {}).status();
        assert!(
            status
                .error
                .unwrap()
                .contains("cannot start /nonexistent/plugin")
        );
    }

    struct First;

    impl Plugin for First {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: "first".into(),
                version: "0".into(),
                kinds: vec![Kind::Decider],
            }
        }

        fn call(&self, call: Call) -> Result<Value, PluginError> {
            let request: DecideRequest = serde_json::from_value(call.params)
                .map_err(|e| PluginError::BadParams(e.to_string()))?;
            Ok(serde_json::to_value(Decision {
                choice: request.options[0].clone(),
                reason: "first".into(),
                by: None,
            })
            .unwrap())
        }
    }

    #[test]
    fn serve_speaks_the_plugins_side_of_the_protocol() {
        let call = Envelope::Call {
            id: 4,
            call: Call::new(Kind::Decider, "decide", request()),
        };
        let input = format!(
            "{}\nnot a call\n{}\n",
            serde_json::to_string(&call).unwrap(),
            serde_json::to_string(&Envelope::Call {
                id: 5,
                call: Call::new(Kind::Decider, "decide", "nonsense"),
            })
            .unwrap()
        );
        let mut output = Vec::new();
        serve(&First, input.as_bytes(), &mut output).unwrap();
        let lines: Vec<Envelope> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            lines[0],
            Envelope::Manifest {
                manifest: First.manifest()
            }
        );
        assert!(
            matches!(&lines[1], Envelope::Reply { id: 4, result } if result["choice"] == "allow")
        );
        assert!(
            matches!(&lines[2], Envelope::Error { id: 5, message } if message.starts_with("bad parameters"))
        );
        assert_eq!(lines.len(), 3);
    }
}
