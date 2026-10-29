//! The API every Moebius plugin is written against, whether it is compiled in or runs as its own process
//! (see `docs/adr/0002-plugins-run-as-processes-or-compiled-in.md`).
//!
//! A plugin answers [`Call`]s. Every message is serialisable, so that the same calls travel as JSON Lines to a plugin
//! in its own process ([`Envelope`], and `process` which starts and looks after such a plugin). The core never calls a
//! plugin directly: it goes through the [`Host`], which gives every call a time limit and turns a timeout, a panic, a
//! crash or an error into a [`Refusal`] (fail closed).

mod process;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use process::{Process, ProcessSpec, ProcessStatus, State, serve};

/// What a plugin can be. One plugin may be several kinds at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Issues and changes: GitHub, plain git, later Codeberg and GitLab.
    Forge,
    /// Drives a coding agent harness such as Pi-Bolt.
    Harness,
    /// Where harnesses run: local worktrees, later GPU machines.
    Runner,
    /// Answers decision points.
    Decider,
    /// The model provider the harnesses talk to.
    Router,
    /// Describes a screen of the desktop app as data.
    View,
}

/// Who a plugin is and what it can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub kinds: Vec<Kind>,
}

/// One call into a plugin: a method of one of its kinds with its parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub kind: Kind,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl Call {
    pub fn new(kind: Kind, method: impl Into<String>, params: impl Serialize) -> Self {
        Self {
            kind,
            method: method.into(),
            params: serde_json::to_value(params).unwrap_or(Value::Null),
        }
    }
}

/// A call or its answer as one JSON line between Moebius and a plugin in its own process. The plugin's first line is
/// its manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Envelope {
    Manifest { manifest: Manifest },
    Call { id: u64, call: Call },
    Reply { id: u64, result: Value },
    Error { id: u64, message: String },
}

/// What a decider is asked: the method `decide` of [`Kind::Decider`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecideRequest {
    /// The kind of decision, such as `tool_call` or `move_card`.
    pub kind: String,
    /// The context, keyed by name, in full (secrets removed).
    pub context: BTreeMap<String, String>,
    /// The choices the decider must pick from; none for a kind whose answer is not one of a few choices, such as a
    /// ranked list of files, and the decider then answers with any text.
    pub options: Vec<String>,
}

/// A decider's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub choice: String,
    pub reason: String,
    /// Which part of the plugin decided, when it has several (such as `patterns` or `teacher` in `guard`). The core
    /// records the decider as `<plugin>/<by>`.
    #[serde(default)]
    pub by: Option<String>,
}

/// Why a plugin's own code refused or failed a call.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum PluginError {
    #[error("bad parameters: {0}")]
    BadParams(String),
    #[error("{0}")]
    Failed(String),
}

/// A plugin compiled in. A plugin in its own process is a [`Process`] instead, added with [`Host::add_process`].
pub trait Plugin: Send + Sync + 'static {
    fn manifest(&self) -> Manifest;
    fn call(&self, call: Call) -> Result<Value, PluginError>;
}

/// Why the host counts a call as refused. Every way a call can fail ends here, so the core never acts on a
/// half-answered call.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "refusal", content = "detail", rename_all = "snake_case")]
pub enum Refusal {
    #[error("no plugin named {0}")]
    UnknownPlugin(String),
    #[error("{plugin} is not a {kind:?} plugin")]
    WrongKind { plugin: String, kind: Kind },
    #[error("{plugin} did not answer within {millis} ms")]
    TimedOut { plugin: String, millis: u64 },
    #[error("{plugin} panicked: {message}")]
    Panicked { plugin: String, message: String },
    #[error("{plugin} failed: {error}")]
    Failed { plugin: String, error: PluginError },
    #[error("{plugin} answered something that is not a valid reply: {message}")]
    BadReply { plugin: String, message: String },
    #[error("{plugin} is not running: {message}")]
    NotRunning { plugin: String, message: String },
}

/// A plugin as the host holds it.
#[derive(Clone)]
enum Entry {
    Compiled(Arc<dyn Plugin>),
    Process(Arc<Process>),
}

impl Entry {
    fn manifest(&self) -> Manifest {
        match self {
            Entry::Compiled(plugin) => plugin.manifest(),
            Entry::Process(process) => process.manifest(),
        }
    }
}

/// Holds the plugins and makes every call into them.
#[derive(Clone)]
pub struct Host {
    plugins: BTreeMap<String, Entry>,
    limit: Duration,
}

impl Host {
    /// A host whose calls are cut off after `limit`.
    pub fn new(limit: Duration) -> Self {
        Self {
            plugins: BTreeMap::new(),
            limit,
        }
    }

    /// Adds a plugin under its manifest's name, replacing one with the same name.
    pub fn add(&mut self, plugin: impl Plugin) -> &mut Self {
        let plugin: Arc<dyn Plugin> = Arc::new(plugin);
        self.plugins
            .insert(plugin.manifest().name, Entry::Compiled(plugin));
        self
    }

    /// Adds a plugin in its own process under the name it was started with, replacing one with the same name.
    pub fn add_process(&mut self, process: Process) -> &mut Self {
        self.plugins
            .insert(process.manifest().name, Entry::Process(Arc::new(process)));
        self
    }

    /// How the plugin `plugin` is doing, if it runs in its own process.
    pub fn status(&self, plugin: &str) -> Option<ProcessStatus> {
        match self.plugins.get(plugin)? {
            Entry::Process(process) => Some(process.status()),
            Entry::Compiled(_) => None,
        }
    }

    /// The manifests of the plugins the host holds, by name.
    pub fn manifests(&self) -> Vec<Manifest> {
        self.plugins.values().map(|p| p.manifest()).collect()
    }

    /// How long a call into a plugin may take.
    pub fn limit(&self) -> Duration {
        self.limit
    }

    /// Calls `plugin`, cutting it off after the host's time limit. A plugin that hangs keeps its thread, but its
    /// answer is never used.
    pub fn call(&self, plugin: &str, call: Call) -> Result<Value, Refusal> {
        let limit = self.limit;
        let target = self
            .plugins
            .get(plugin)
            .ok_or_else(|| Refusal::UnknownPlugin(plugin.to_string()))?
            .clone();
        if let Entry::Process(process) = &target
            && let Some(refusal) = process.unavailable()
        {
            return Err(refusal);
        }
        if !target.manifest().kinds.contains(&call.kind) {
            return Err(Refusal::WrongKind {
                plugin: plugin.to_string(),
                kind: call.kind,
            });
        }
        let (tx, rx) = mpsc::channel();
        let name = plugin.to_string();
        std::thread::Builder::new()
            .name(format!("plugin {plugin}"))
            .spawn(move || {
                let result = match target {
                    Entry::Compiled(target) => catch_unwind(AssertUnwindSafe(|| target.call(call)))
                        .map(|answer| {
                            answer.map_err(|error| Refusal::Failed {
                                plugin: name,
                                error,
                            })
                        }),
                    Entry::Process(process) => Ok(process.request(call)),
                };
                let _ = tx.send(result);
            })
            .map_err(|e| Refusal::Failed {
                plugin: plugin.to_string(),
                error: PluginError::Failed(format!("cannot start the call: {e}")),
            })?;
        match rx.recv_timeout(limit) {
            Ok(Ok(answer)) => answer,
            Ok(Err(panic)) => Err(Refusal::Panicked {
                plugin: plugin.to_string(),
                message: panic_message(&*panic),
            }),
            Err(_) => Err(Refusal::TimedOut {
                plugin: plugin.to_string(),
                millis: limit.as_millis().try_into().unwrap_or(u64::MAX),
            }),
        }
    }

    /// Asks a decider plugin to decide; an answer outside the options is refused like any other failure, unless the
    /// request has no options.
    pub fn decide(&self, plugin: &str, request: &DecideRequest) -> Result<Decision, Refusal> {
        let value = self.call(plugin, Call::new(Kind::Decider, "decide", request))?;
        let decision: Decision = serde_json::from_value(value).map_err(|e| Refusal::BadReply {
            plugin: plugin.to_string(),
            message: e.to_string(),
        })?;
        if !request.options.is_empty() && !request.options.contains(&decision.choice) {
            return Err(Refusal::BadReply {
                plugin: plugin.to_string(),
                message: format!("{:?} is not one of the options", decision.choice),
            });
        }
        Ok(decision)
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A decider that behaves as its name says.
    struct Test(&'static str);

    impl Plugin for Test {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: self.0.to_string(),
                version: "0".into(),
                kinds: vec![Kind::Decider],
            }
        }

        fn call(&self, call: Call) -> Result<Value, PluginError> {
            match self.0 {
                "hangs" => loop {
                    std::thread::sleep(Duration::from_secs(60));
                },
                "panics" => panic!("boom"),
                "fails" => Err(PluginError::Failed("no".into())),
                "strays" => Ok(serde_json::json!({"choice": "neither", "reason": "?"})),
                _ => {
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
        }
    }

    /// A host whose limit is far beyond what an answering, failing or panicking plugin needs, even with backtraces on.
    fn host() -> Host {
        host_within(Duration::from_secs(30))
    }

    fn host_within(limit: Duration) -> Host {
        let mut host = Host::new(limit);
        for name in ["answers", "hangs", "panics", "fails", "strays"] {
            host.add(Test(name));
        }
        host
    }

    fn request() -> DecideRequest {
        DecideRequest {
            kind: "tool_call".into(),
            context: BTreeMap::from([("command".into(), "ls".into())]),
            options: vec!["allow".into(), "refuse".into()],
        }
    }

    #[test]
    fn an_answering_plugin_is_heard() {
        let d = host().decide("answers", &request()).unwrap();
        assert_eq!(d.choice, "allow");
    }

    #[test]
    fn a_hanging_plugin_is_cut_off_and_refused() {
        let started = std::time::Instant::now();
        let r = host_within(Duration::from_millis(200)).decide("hangs", &request());
        assert!(matches!(r, Err(Refusal::TimedOut { millis: 200, .. })));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_panicking_plugin_is_refused() {
        let r = host().decide("panics", &request());
        assert!(matches!(r, Err(Refusal::Panicked { ref message, .. }) if message == "boom"));
    }

    #[test]
    fn failures_wrong_kinds_and_stray_answers_are_refused() {
        let host = host();
        assert!(matches!(
            host.decide("fails", &request()),
            Err(Refusal::Failed { .. })
        ));
        assert!(matches!(
            host.decide("strays", &request()),
            Err(Refusal::BadReply { .. })
        ));
        assert!(matches!(
            host.decide("nobody", &request()),
            Err(Refusal::UnknownPlugin(_))
        ));
        assert!(matches!(
            host.call("answers", Call::new(Kind::Forge, "issues", ())),
            Err(Refusal::WrongKind { .. })
        ));
    }

    #[test]
    fn calls_travel_as_json_lines() {
        let e = Envelope::Call {
            id: 7,
            call: Call::new(Kind::Decider, "decide", request()),
        };
        let line = serde_json::to_string(&e).unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(serde_json::from_str::<Envelope>(&line).unwrap(), e);
        let refusal = Refusal::TimedOut {
            plugin: "x".into(),
            millis: 1,
        };
        let back: Refusal =
            serde_json::from_str(&serde_json::to_string(&refusal).unwrap()).unwrap();
        assert_eq!(back, refusal);
    }
}
