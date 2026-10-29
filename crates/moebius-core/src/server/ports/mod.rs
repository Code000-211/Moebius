//! The ports that the processes of runs, chats and terminals listen on, for the Ports view of the desktop app.
//!
//! [`detect`] finds the TCP ports something listens on that the server's loopback address reaches, and says whose they
//! are: the process that holds the socket belongs to a terminal when it, or a process that started it, is the shell
//! a person opened (`harness::terminal::Slots::programs`), and otherwise to the run or chat in whose worktree it, or a
//! process that started it, works. [`watch`] looks again every few seconds while a run or a terminal exists, and
//! publishes `port_opened` and `port_closed` for the difference. Detection reads `/proc`, so it finds nothing but on
//! Linux (see [`proc`]). The tunnel that carries a connection to a port is in `server/api/ports.rs`.

mod proc;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::App;
use crate::bus::AppEvent;
use crate::harness::terminal::SERVER;

/// How often [`watch`] looks for ports.
pub const PERIOD: Duration = Duration::from_secs(3);

/// A port something listens on, and what it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    pub port: u16,
    /// The process that listens, and the program it runs (empty when it ended while it was read).
    pub pid: u32,
    pub process: String,
    /// What the process belongs to: `run`, `chat` or `terminal`.
    pub kind: String,
    /// The run or chat; for a terminal, the run or chat it was opened on, none for the server's own terminal.
    pub run: Option<i64>,
}

/// What a process belongs to, before it is named.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Owner {
    Terminal(Option<i64>),
    Run { id: i64, chat: bool },
}

/// The roots a process is matched against: the shells people opened, and the worktrees of runs and chats.
struct Roots {
    terminals: HashMap<u32, Option<i64>>,
    worktrees: Vec<(PathBuf, Owner)>,
}

impl Roots {
    fn of(app: &App) -> Roots {
        let terminals = app
            .terminals
            .programs()
            .into_iter()
            .map(|(key, pid)| (pid, terminal_run(key)))
            .collect();
        let worktrees = app
            .store
            .active_runs()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|run| {
                let worktree = std::fs::canonicalize(run.worktree?).ok()?;
                let chat = run.purpose == "chat";
                Some((worktree, Owner::Run { id: run.id, chat }))
            })
            .collect();
        Roots {
            terminals,
            worktrees,
        }
    }

    fn is_empty(&self) -> bool {
        self.terminals.is_empty() && self.worktrees.is_empty()
    }

    /// Whose the process `pid` is: a terminal's if it or a process that started it is one's shell, else the run's
    /// whose worktree it or a process that started it works in.
    fn owner_of(&self, pid: u32) -> Option<Owner> {
        let mut chain = vec![pid];
        while let Some(parent) = proc::parent(*chain.last()?) {
            if chain.contains(&parent) || chain.len() > 64 {
                break;
            }
            chain.push(parent);
        }
        if let Some(run) = chain.iter().find_map(|pid| self.terminals.get(pid)) {
            return Some(Owner::Terminal(*run));
        }
        chain.iter().find_map(|pid| {
            let directory = proc::directory(*pid)?;
            self.worktrees
                .iter()
                .find(|(worktree, _)| directory.starts_with(worktree))
                .map(|(_, owner)| owner.clone())
        })
    }
}

/// The run a terminal opened under `key` belongs to: the key of a run's shell is the run, of a chat's terminal
/// interface the run negated, and of the server's own shell [`SERVER`].
fn terminal_run(key: i64) -> Option<i64> {
    (key != SERVER).then(|| key.abs())
}

/// The ports that processes of runs, chats and terminals listen on now, by port. Blocks: it reads `/proc`.
pub fn detect(app: &App) -> Vec<Port> {
    let roots = Roots::of(app);
    if roots.is_empty() {
        return Vec::new();
    }
    let mut found: BTreeMap<u16, Port> = BTreeMap::new();
    for (port, pid) in proc::listeners() {
        if found.contains_key(&port) {
            continue;
        }
        let Some(owner) = roots.owner_of(pid) else {
            continue;
        };
        let (kind, run) = match owner {
            Owner::Terminal(run) => ("terminal", run),
            Owner::Run { id, chat: true } => ("chat", Some(id)),
            Owner::Run { id, chat: false } => ("run", Some(id)),
        };
        found.insert(
            port,
            Port {
                port,
                pid,
                process: proc::name(pid).unwrap_or_default(),
                kind: kind.into(),
                run,
            },
        );
    }
    found.into_values().collect()
}

/// The events that turn the ports `before` into the ports `after`: `port_closed` for those gone, `port_opened` for
/// those new.
fn changes(before: &[Port], after: &[Port]) -> Vec<AppEvent> {
    let event = |kind: &str, port: &Port| {
        AppEvent::new(kind, serde_json::to_value(port).unwrap_or(json!({})))
    };
    let closed = before
        .iter()
        .filter(|b| !after.iter().any(|a| a.port == b.port));
    let opened = after
        .iter()
        .filter(|a| !before.iter().any(|b| b.port == a.port));
    closed
        .map(|port| event("port_closed", port))
        .chain(opened.map(|port| event("port_opened", port)))
        .collect()
}

/// Looks for ports every `period` and publishes what opened and closed, until the future is dropped. With no run and no
/// terminal there is nothing to read, so an idle server only asks the store.
pub async fn watch(app: Arc<App>, period: Duration) {
    let mut known: Vec<Port> = Vec::new();
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let a = app.clone();
        let Ok(now) = tokio::task::spawn_blocking(move || detect(&a)).await else {
            continue;
        };
        for event in changes(&known, &now) {
            app.bus.publish(event);
        }
        known = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(port: u16, pid: u32) -> Port {
        Port {
            port,
            pid,
            process: "node".into(),
            kind: "terminal".into(),
            run: Some(1),
        }
    }

    #[test]
    fn a_port_that_appears_is_opened_and_one_that_disappears_is_closed() {
        let a = port(3000, 10);
        let b = port(4000, 11);
        assert!(changes(std::slice::from_ref(&a), std::slice::from_ref(&a)).is_empty());
        let events = changes(std::slice::from_ref(&a), std::slice::from_ref(&b));
        let kinds: Vec<_> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["port_closed", "port_opened"]);
        assert_eq!(events[1].data["port"], 4000);
        assert_eq!(events[1].data["kind"], "terminal");
        assert_eq!(events[1].data["run"], 1);
    }

    #[test]
    fn the_key_of_a_terminal_names_the_run_it_was_opened_on() {
        assert_eq!(terminal_run(7), Some(7));
        assert_eq!(terminal_run(-7), Some(7), "a chat's terminal interface");
        assert_eq!(terminal_run(SERVER), None);
    }

    #[test]
    fn nothing_is_read_without_a_run_or_a_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::store::Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(moebius_plugin_api::Host::new(Duration::from_secs(1)));
        let app = App::new(store, host, dir.path());
        assert!(detect(&app).is_empty());
    }

    // Linux only: the ports are read from /proc, and the listener is a Python program.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn the_watch_publishes_a_port_when_it_opens_and_when_it_closes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::store::Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(moebius_plugin_api::Host::new(Duration::from_secs(1)));
        let app = Arc::new(App::new(store, host, dir.path()));
        let work = tempfile::tempdir().unwrap();
        let run = app.store.add_run(None, "Coder", "test", "chat").unwrap();
        app.store
            .set_run_worktree(run.id, &work.path().to_string_lossy())
            .unwrap();
        app.store.set_run_status(run.id, "running").unwrap();
        let mut events = app.bus.subscribe();
        let watching = tokio::spawn(watch(app.clone(), Duration::from_millis(20)));

        let mut child = std::process::Command::new("python3")
            .args([
                "-c",
                "import socket,sys;s=socket.socket();s.bind(('127.0.0.1',0));s.listen();\
                 print(s.getsockname()[1],flush=True);sys.stdin.read()",
            ])
            .current_dir(work.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(child.stdout.take().unwrap()),
            &mut line,
        )
        .unwrap();
        let port: u64 = line.trim().parse().unwrap();

        let mut next = async |kind: &str| loop {
            let event = tokio::time::timeout(Duration::from_secs(60), events.recv())
                .await
                .expect("no event in a minute")
                .unwrap();
            if event.kind == kind {
                return event;
            }
        };
        let opened = next("port_opened").await;
        assert_eq!(opened.data["port"], port);
        assert_eq!(opened.data["kind"], "chat");
        assert_eq!(opened.data["run"], run.id);
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(next("port_closed").await.data["port"], port);
        watching.abort();
    }
}
