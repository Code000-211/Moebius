//! The launcher kind: a plugin that may wrap how a harness process is started, such as inside a sandbox. Before the
//! core starts a harness process for a run it sends the launcher a [`LaunchRequest`] (the method `launch` of
//! [`Kind::Launcher`](crate::Kind::Launcher)); the launcher answers [`LaunchAnswer::Unchanged`] or a
//! [`Wrapped`] launch, which the core starts instead, speaking the harness's protocol over its standard input and
//! output as before. At the server's start the core asks for [`Leftovers`] (the method `leftovers`) of earlier runs.
//!
//! The core runs the stop and cleanup commands of a wrapped launch itself, so a launcher that is a process plugin
//! need not stay in the way of a run once it answered. A launcher that fails, does not answer within the host's time
//! limit or answers nonsense fails the run: the core never starts it unwrapped then (see
//! `docs/reference/plugin-api.md`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Call, Host, Kind, Refusal};

/// The method of [`Kind::Launcher`] that is asked before a harness process starts.
pub const LAUNCH: &str = "launch";

/// The method of [`Kind::Launcher`] that is asked when the server starts, for what earlier runs left.
pub const LEFTOVERS: &str = "leftovers";

/// The run a harness process is started for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchRun {
    pub id: i64,
    /// What the run is for, such as `code`, `review`, `chat`, `retro` or `trial`.
    pub purpose: String,
    /// The name of the agent that runs.
    pub agent: String,
    /// Whether the run is a chat (its process stays up between messages) and not a run of a card or of the
    /// improvement loop.
    pub chat: bool,
    /// The card the run works on; none for a chat, a Retro and the like.
    #[serde(default)]
    pub card: Option<i64>,
}

/// The addresses the process must reach, as the server reaches them. All three are on the server's one listener.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Urls {
    /// The model router, such as `http://127.0.0.1:7172`.
    pub router: String,
    /// The MCP gateway.
    pub gateway: String,
    /// The tool-call check the bridge asks (`MOEBIUS_CHECK_URL`).
    pub check: String,
}

/// What a launcher is asked before a harness process starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchRequest {
    pub run: LaunchRun,
    /// The harness's executable, as the core would start it.
    pub program: String,
    pub args: Vec<String>,
    /// The process's complete environment, run token included. Nothing else is inherited.
    pub env: BTreeMap<String, String>,
    /// The working directory: the run's worktree.
    pub dir: String,
    /// Paths the process needs to read: the bridge, and the agent's own extensions and skills.
    pub read_only: Vec<String>,
    /// Paths the process needs to read and write: the worktree, its agent directory, its sessions directory and the
    /// run's temporary directory.
    pub read_write: Vec<String>,
    pub urls: Urls,
    /// The clone of the repository that the worktree belongs to, whose git directory the worktree's git uses; none
    /// for a run in an empty directory.
    #[serde(default)]
    pub clone: Option<String>,
    /// The runner when the run goes on another machine over SSH, which a launcher on the server cannot wrap: it
    /// answers [`LaunchAnswer::Unchanged`] (an answer that wraps it fails the run). None for a run on the server.
    #[serde(default)]
    pub runner: Option<String>,
    /// The name in Moebius of the repository the run works in; none for a chat in an empty directory and for a run
    /// in the harness configuration repository.
    #[serde(default)]
    pub repository: Option<String>,
    /// The launcher's own project settings (`project_settings` of its manifest) as a person set them for that
    /// repository, by key. A setting left empty is not in it, and without a repository it is empty.
    #[serde(default)]
    pub project_settings: BTreeMap<String, String>,
    /// The launcher's own app settings (`app_settings` of its manifest) as a person set them for the server, by key.
    /// A setting left empty is not in it.
    #[serde(default)]
    pub app_settings: BTreeMap<String, String>,
}

/// A command the core runs on the server: to stop a wrapped process, clean up after it, or remove leftovers. It runs
/// with an environment cleared but for what `env` gives and the few variables every program needs (`PATH`, `HOME`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandLine {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// The addresses as the wrapped process sees them, where they differ from the server's. A missing one stays.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlsInside {
    #[serde(default)]
    pub router: Option<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub check: Option<String>,
}

/// A launch the launcher wrapped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wrapped {
    /// The program the core starts instead of the harness. It must keep the harness's standard input and output
    /// connected, since the core speaks to the harness over them.
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// The wrapped program's complete environment. The core adds nothing to it but the addresses in `urls`.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The addresses as the harness sees them from inside, such as the router at another port. The core writes them
    /// into the agent directory and `MOEBIUS_CHECK_URL`.
    #[serde(default)]
    pub urls: UrlsInside,
    /// Ends the whole wrapped process tree. The core runs it, and kills the wrapper's own process group as well,
    /// every time the process is ended: when the run is stopped, killed, restarted or the server restarts, and after
    /// a normal end too. It can run more than once for one process, so it must succeed when nothing is left.
    #[serde(default)]
    pub stop: Option<CommandLine>,
    /// Run once when the wrapped process has ended, for whatever reason.
    #[serde(default)]
    pub cleanup: Option<CommandLine>,
    /// Files the launcher created that the core removes after `cleanup`.
    #[serde(default)]
    pub remove: Vec<String>,
    /// Something to say about the launch, which the core records as a `launcher_note` event of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A launcher's answer to a [`LaunchRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum LaunchAnswer {
    /// Start the process as asked. `note` says why, such as that the run is on a runner or its agent is excluded; the
    /// core records it as a `launcher_note` event of the run.
    Unchanged {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// Start this instead.
    Wrapped(Box<Wrapped>),
}

/// What the core asks for at the server's start.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeftoversRequest {
    /// The ids of the runs that still exist. A launcher keeps what belongs to them and answers the rest; at the
    /// server's start the runs of the last start have been failed, so the list is empty.
    pub running: Vec<i64>,
}

/// What earlier runs left, which the core removes at the server's start.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leftovers {
    /// Commands the core runs, such as one that deletes a sandbox's profile.
    #[serde(default)]
    pub cleanup: Vec<CommandLine>,
    /// Files and directories the core removes, after the commands.
    #[serde(default)]
    pub remove: Vec<String>,
}

impl Host {
    /// Asks the launcher `plugin` how to start a harness process. Any [`Refusal`] means the run must not start.
    pub fn launch(&self, plugin: &str, request: &LaunchRequest) -> Result<LaunchAnswer, Refusal> {
        self.typed(plugin, LAUNCH, request)
    }

    /// Asks the launcher `plugin` what earlier runs left behind.
    pub fn leftovers(
        &self,
        plugin: &str,
        request: &LeftoversRequest,
    ) -> Result<Leftovers, Refusal> {
        self.typed(plugin, LEFTOVERS, request)
    }

    /// A call of the launcher kind whose reply must parse as `T`.
    fn typed<T: serde::de::DeserializeOwned>(
        &self,
        plugin: &str,
        method: &str,
        params: &impl Serialize,
    ) -> Result<T, Refusal> {
        let value = self.call(plugin, Call::new(Kind::Launcher, method, params))?;
        serde_json::from_value(value).map_err(|e| Refusal::BadReply {
            plugin: plugin.to_string(),
            message: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{Manifest, Plugin, PluginError};

    /// A launcher that answers as its name says.
    struct Fake(&'static str);

    impl Plugin for Fake {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: self.0.into(),
                version: "0".into(),
                kinds: vec![Kind::Launcher],
                ..Default::default()
            }
        }

        fn call(&self, call: Call) -> Result<serde_json::Value, PluginError> {
            match (self.0, call.method.as_str()) {
                ("hangs", _) => loop {
                    std::thread::sleep(Duration::from_secs(60));
                },
                ("fails", _) => Err(PluginError::Failed("no sandbox".into())),
                ("garbles", _) => Ok(serde_json::json!({"answer": "maybe"})),
                (_, LEFTOVERS) => Ok(serde_json::json!({"remove": ["/x"], "cleanup": []})),
                (_, _) => {
                    let request: LaunchRequest = serde_json::from_value(call.params)
                        .map_err(|e| PluginError::BadParams(e.to_string()))?;
                    let answer = if request.runner.is_some() {
                        LaunchAnswer::Unchanged { note: None }
                    } else {
                        LaunchAnswer::Wrapped(Box::new(Wrapped {
                            program: "wrap".into(),
                            args: vec!["--".into(), request.program],
                            env: BTreeMap::new(),
                            urls: UrlsInside::default(),
                            stop: None,
                            cleanup: None,
                            remove: vec![],
                            note: None,
                        }))
                    };
                    Ok(serde_json::to_value(answer).unwrap())
                }
            }
        }
    }

    pub(crate) fn request(runner: Option<&str>) -> LaunchRequest {
        LaunchRequest {
            run: LaunchRun {
                id: 3,
                purpose: "code".into(),
                agent: "Coder".into(),
                chat: false,
                card: Some(9),
            },
            program: "pi-bolt".into(),
            args: vec!["--mode".into(), "rpc".into()],
            env: BTreeMap::from([("PATH".to_string(), "/bin".to_string())]),
            dir: "/work".into(),
            read_only: vec![],
            read_write: vec!["/work".into(), "/agent".into()],
            urls: Urls {
                router: "http://127.0.0.1:1".into(),
                gateway: "http://127.0.0.1:1/mcp".into(),
                check: "http://127.0.0.1:1".into(),
            },
            clone: Some("/clone".into()),
            runner: runner.map(String::from),
            repository: None,
            project_settings: BTreeMap::new(),
            app_settings: BTreeMap::new(),
        }
    }

    fn host(limit: Duration) -> Host {
        let mut host = Host::new(limit);
        for name in ["wraps", "hangs", "fails", "garbles"] {
            host.add(Fake(name));
        }
        host
    }

    #[test]
    fn an_answer_is_read_as_unchanged_or_wrapped() {
        let host = host(Duration::from_secs(30));
        let wrapped = host.launch("wraps", &request(None)).unwrap();
        assert!(matches!(wrapped, LaunchAnswer::Wrapped(w) if w.program == "wrap"));
        let unchanged = host.launch("wraps", &request(Some("box"))).unwrap();
        assert_eq!(unchanged, LaunchAnswer::Unchanged { note: None });
        let left = host
            .leftovers("wraps", &LeftoversRequest::default())
            .unwrap();
        assert_eq!(left.remove, ["/x"]);
    }

    #[test]
    fn a_wrapped_answer_may_leave_out_everything_but_the_program() {
        let answer: LaunchAnswer =
            serde_json::from_value(serde_json::json!({ "answer": "wrapped", "program": "wrap" }))
                .unwrap();
        assert!(
            matches!(answer, LaunchAnswer::Wrapped(w) if w.args.is_empty() && w.note.is_none())
        );
        let noted: LaunchAnswer = serde_json::from_value(
            serde_json::json!({ "answer": "unchanged", "note": "a runner" }),
        )
        .unwrap();
        assert_eq!(
            noted,
            LaunchAnswer::Unchanged {
                note: Some("a runner".into())
            }
        );
    }

    #[test]
    fn every_way_a_launcher_can_fail_is_a_refusal() {
        let slow = host(Duration::from_millis(200));
        assert!(matches!(
            slow.launch("hangs", &request(None)),
            Err(Refusal::TimedOut { .. })
        ));
        let host = host(Duration::from_secs(30));
        assert!(matches!(
            host.launch("fails", &request(None)),
            Err(Refusal::Failed { .. })
        ));
        assert!(matches!(
            host.launch("garbles", &request(None)),
            Err(Refusal::BadReply { .. })
        ));
        assert!(matches!(
            host.launch("nobody", &request(None)),
            Err(Refusal::UnknownPlugin(_))
        ));
        // A plugin of another kind is not a launcher.
        let mut other = Host::new(Duration::from_secs(30));
        other.add(crate::tests::Test("answers"));
        assert!(matches!(
            other.launch("answers", &request(None)),
            Err(Refusal::WrongKind { .. })
        ));
    }

    #[test]
    fn messages_travel_as_json_with_the_answer_named() {
        let answer = LaunchAnswer::Unchanged { note: None };
        assert_eq!(
            serde_json::to_string(&answer).unwrap(),
            r#"{"answer":"unchanged"}"#
        );
        let line = r#"{"answer":"wrapped","program":"w","args":[]}"#;
        let LaunchAnswer::Wrapped(w) = serde_json::from_str(line).unwrap() else {
            panic!("not wrapped")
        };
        assert!(w.stop.is_none() && w.env.is_empty() && w.urls == UrlsInside::default());
    }
}
