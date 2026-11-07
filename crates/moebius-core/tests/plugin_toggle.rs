//! Turning a plugin off and on while the server runs: a decider that is off gives its kinds back to the guard and its
//! process ends, with its entry and settings still in the file and the store; a launcher that is off is absent for the
//! rules of the launchers, and turning off the last one needs a confirmation; only a person does it; every client is told.

// Unix only: the plugins are Python scripts started by their `#!` line.
#![cfg(unix)]

use std::path::Path;
use std::time::{Duration, Instant};

use moebius_core::app::AppError;
use moebius_core::config::Config;
use moebius_core::decide::{Actor, Question};
use moebius_core::router::Grant;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use moebius_core::server::plugins::Switches;
use reqwest::Method;
use serde_json::{Value, json};

/// A decider that answers its first option, writes its process id to `<dir>/<name>.pid` and a line to
/// `<dir>/starts.txt` when it starts. Started as `plugin.py <dir> <name>`.
const DECIDER: &str = r#"#!/usr/bin/env python3
import json, os, sys
directory, name = sys.argv[1], sys.argv[2]
def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()
with open(os.path.join(directory, name + ".pid"), "w") as f:
    f.write(str(os.getpid()))
with open(os.path.join(directory, "starts.txt"), "a") as f:
    f.write(name + "\n")
send({"type": "manifest", "manifest": {"name": name, "version": "1", "kinds": ["decider", "launcher"],
      "app_settings": [{"key": "registry", "label": "Registry"}]}})
for line in sys.stdin:
    message = json.loads(line)
    if message.get("type") != "call":
        continue
    params = message["call"]["params"]
    if message["call"]["method"] == "launch":
        result = {"answer": "unchanged"}
    elif message["call"]["method"] == "leftovers":
        with open(os.path.join(directory, "leftovers.txt"), "a") as f:
            f.write(json.dumps(params["running"]) + "\n")
        result = {"cleanup": [], "remove": []}
    else:
        result = {"choice": params["options"][0], "reason": "first"}
    send({"type": "reply", "id": message["id"], "result": result})
"#;

struct Fixture {
    server: Server,
    api: String,
    dir: tempfile::TempDir,
    client: reqwest::Client,
}

impl Fixture {
    /// A server whose file is `extra` after the router section and `[agents.Coder]`; every `[[plugins]]` in it runs the
    /// script above under its own name, given as `@name` in `path`.
    async fn start(extra: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("plugin.py");
        std::fs::write(&script, DECIDER).unwrap();
        crate::moebius_core_make_executable(&script);
        let text = format!(
            "[router]\nlisten = \"127.0.0.1:0\"\n\n[agents.Coder]\nmodel = \"m\"\n\n{extra}"
        )
        .replace("{script}", &script.display().to_string())
        .replace("{dir}", &dir.path().display().to_string());
        std::fs::write(dir.path().join("moebius.toml"), text).unwrap();
        Self::open(dir).await
    }

    async fn open(dir: tempfile::TempDir) -> Self {
        let config = Config::load(&dir.path().join("moebius.toml")).unwrap();
        let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            sessions: Default::default(),
            app: server.app.clone(),
            runs: server.harness.clone(),
            harness_repo: server.harness_repo.clone(),
            config: server.live.clone(),
            token: server.token.clone(),
            router: server.router.clone(),
        };
        tokio::spawn(api::serve(listener, state));
        Self {
            server,
            api,
            dir,
            client: reqwest::Client::new(),
        }
    }

    fn file(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("moebius.toml")).unwrap()
    }

    fn pid(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(format!("{name}.pid"))).unwrap()
    }

    /// The `running` of each `leftovers` call the launchers got, one line each.
    fn leftovers(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("leftovers.txt"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn plugin_dir(&self) -> std::path::PathBuf {
        self.server.config.data_dir.join("plugins")
    }

    fn starts(&self) -> usize {
        std::fs::read_to_string(self.dir.path().join("starts.txt"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    fn set(
        &self,
        plugin: &str,
        enabled: bool,
        unwrap_runs: bool,
    ) -> Result<moebius_core::server::settings::PluginEnabled, AppError> {
        self.server.live.set_plugin_enabled(
            &self.server.app,
            &Actor::Person,
            plugin,
            enabled,
            unwrap_runs,
            false,
        )
    }

    fn decider_of(&self, kind: &str) -> String {
        self.server
            .app
            .decisions
            .decide(&Question {
                kind: kind.into(),
                options: vec!["allow".into(), "refuse".into()],
                fail_closed: "refuse".into(),
                ..Question::default()
            })
            .unwrap()
            .decider
    }

    async fn call(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.api))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    fn agent_token(&self) -> String {
        let run = self
            .server
            .app
            .store
            .add_run(None, "Moebius", "pi-bolt", "chat")
            .unwrap();
        self.server.router.issue_token(Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        })
    }
}

fn moebius_core_make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Whether the process `pid` exists.
fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

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

const GATE: &str = "# the gate decides the tool calls\n[[plugins]]\nname = \"gate\"\npath = \"{script}\"\n\
                    args = [\"{dir}\", \"gate\"]\ndecides = [\"tool_call\"]\n";

#[tokio::test(flavor = "multi_thread")]
async fn a_decider_is_turned_off_and_on_at_once_with_its_process_entry_and_settings() {
    let f = Fixture::start(GATE).await;
    let mut events = f.server.app.bus.subscribe();
    assert_eq!(f.decider_of("tool_call"), "gate");
    let first = f.pid("gate");
    assert!(alive(&first));
    // Its app settings are in the store.
    f.server
        .app
        .save_plugin_settings(
            &Actor::Person,
            moebius_core::app::plugin_settings::Scope::App,
            "gate",
            &[("registry".to_string(), "r.example".to_string())].into(),
            "",
        )
        .unwrap();

    let off = f.set("gate", false, false).unwrap();
    assert!(off.changed && !off.enabled && !off.unwrapped_runs);
    // The process ends, the guard decides the kind again, and the file keeps the entry, its keys and its comment.
    eventually(|| !alive(&first));
    assert!(f.decider_of("tool_call").starts_with("guard"));
    let file = f.file();
    assert!(file.contains("# the gate decides the tool calls"), "{file}");
    assert!(file.contains("enabled = false") && file.contains("decides = [\"tool_call\"]"));
    assert!(f.server.live.view().unwrap().restart_keys.is_empty());
    // It is still listed, as off, with its settings box and its value.
    let plugins = moebius_core::server::setup::plugins(
        &f.server.app.decisions,
        &f.server.config.data_dir,
        &Switches::of(&f.server.live.get()),
    );
    let gate = plugins.plugins.iter().find(|p| p.name == "gate").unwrap();
    assert!(!gate.enabled && gate.process.is_none());
    let settings = f.server.app.app_settings().unwrap();
    let box_ = settings
        .plugins
        .iter()
        .find(|p| p.plugin == "gate")
        .unwrap();
    assert!(!box_.enabled);
    assert_eq!(box_.settings[0].value, "r.example");
    // Nothing is started or asked meanwhile, and the event told every client.
    assert_eq!(f.starts(), 1);
    let seen = std::iter::from_fn(|| events.try_recv().ok())
        .map(|e| serde_json::to_value(&e).unwrap())
        .find(|e| e["kind"] == "plugin_enabled_changed")
        .unwrap();
    assert_eq!(seen["data"], json!({"name": "gate", "enabled": false}));
    // The same again changes nothing.
    assert!(!f.set("gate", false, false).unwrap().changed);

    let on = f.set("gate", true, false).unwrap();
    assert!(on.changed && on.enabled);
    assert_eq!(f.starts(), 2);
    assert_ne!(f.pid("gate"), first);
    assert!(alive(&f.pid("gate")));
    assert_eq!(f.decider_of("tool_call"), "gate");
    assert!(!f.file().contains("enabled"));
    let plugins = moebius_core::server::setup::plugins(
        &f.server.app.decisions,
        &f.server.config.data_dir,
        &Switches::of(&f.server.live.get()),
    );
    assert!(
        plugins
            .plugins
            .iter()
            .find(|p| p.name == "gate")
            .unwrap()
            .enabled
    );
    assert!(f.server.app.app_settings().unwrap().plugins[0].enabled);
    assert!(f.server.live.view().unwrap().restart_keys.is_empty());
    assert!(matches!(
        f.set("nobody", false, false),
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_that_is_off_in_the_file_is_not_started_but_keeps_its_settings_box_after_a_restart()
 {
    let f = Fixture::start(GATE).await;
    f.set("gate", false, false).unwrap();
    let pid = f.pid("gate");
    eventually(|| !alive(&pid));
    let dir = f.dir;
    drop(f.server);
    let g = Fixture::open(dir).await;
    // Not started at the start, not asked, and listed from the manifest it last sent.
    assert_eq!(g.starts(), 1);
    assert!(g.decider_of("tool_call").starts_with("guard"));
    let settings = g.server.app.app_settings().unwrap();
    assert_eq!(settings.plugins.len(), 1);
    assert!(!settings.plugins[0].enabled);
    assert_eq!(settings.plugins[0].version, "1");
    g.set("gate", true, false).unwrap();
    assert_eq!(g.starts(), 2);
    assert_eq!(g.decider_of("tool_call"), "gate");
}

const LAUNCHERS: &str = "[[plugins]]\nname = \"narrow\"\npath = \"{script}\"\nargs = [\"{dir}\", \"narrow\"]\n\
    launches = true\nlaunch_agents = [\"Moebius\"]\n\
    [[plugins]]\nname = \"wide\"\npath = \"{script}\"\nargs = [\"{dir}\", \"wide\"]\nlaunches = true\n";

#[tokio::test(flavor = "multi_thread")]
async fn launchers_that_are_off_are_absent_and_turning_off_the_last_one_is_confirmed() {
    let f = Fixture::start(LAUNCHERS).await;
    let launcher = || f.server.harness.pi_bolt.launcher.get();
    let who = |agent: &str| {
        launcher()
            .and_then(|l| l.plugin_for(agent).map(String::from))
            .unwrap_or_default()
    };
    assert_eq!(
        (who("Moebius").as_str(), who("Coder").as_str()),
        ("narrow", "wide")
    );
    let before = f.file();
    // The launcher for the rest cannot go while the one for the Moebius agent stays: Coder would run unwrapped.
    let AppError::Invalid(message) = f.set("wide", false, false).unwrap_err() else {
        panic!("not refused as invalid");
    };
    assert!(
        message.contains("Coder") && message.contains("Moebius"),
        "{message}"
    );
    assert_eq!(f.file(), before);
    assert_eq!(who("Coder"), "wide");
    assert!(f.starts() == 2);

    // The one for the Moebius agent can go: the other takes every agent.
    let run_held = launcher().unwrap();
    f.set("narrow", false, false).unwrap();
    assert_eq!(
        (who("Moebius").as_str(), who("Coder").as_str()),
        ("wide", "wide")
    );
    // A run that started before keeps the launchers it started with.
    assert_eq!(run_held.plugin_for("Moebius"), Some("narrow"));

    // Turning off the last launcher unwraps every run, and a person says so.
    let AppError::Conflict(message) = f.set("wide", false, false).unwrap_err() else {
        panic!("not refused as a conflict");
    };
    assert!(message.contains("unwrap_runs"), "{message}");
    assert!(
        launcher().is_some()
            && !f
                .file()
                .contains("name = \"wide\"\npath = \"{script}\"\nenabled")
    );
    let off = f.set("wide", false, true).unwrap();
    assert!(off.unwrapped_runs && off.note.unwrap().contains("unwrapped"));
    assert!(launcher().is_none());
    // Claude Code runs start again, since no launcher is configured.
    assert!(!f.server.harness.claude_code.launcher.is_set());

    // Turning one on again wraps the runs that start from now on; it needs no confirmation.
    f.set("wide", true, false).unwrap();
    assert_eq!(who("Coder"), "wide");
    // Turning a launcher for single agents on after it is allowed only where the rules hold: wide is last and takes all.
    f.set("narrow", true, false).unwrap();
    assert_eq!(
        (who("Moebius").as_str(), who("Coder").as_str()),
        ("narrow", "wide")
    );
    assert!(f.server.harness.claude_code.launcher.is_set());
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_person_turns_a_plugin_off_and_the_route_and_the_settings_say_so() {
    let f = Fixture::start(GATE).await;
    let person = f.server.token.to_string();
    let agent = f.agent_token();
    let off = json!({ "enabled": false });
    let (status, _) = f
        .call(
            &agent,
            Method::PUT,
            "/v1/plugins/gate/enabled",
            Some(off.clone()),
        )
        .await;
    assert_eq!(status, 403);
    // Neither does the settings route let an agent turn one off, nor leave a plugin out of its changes.
    let edit = json!({ "set": { "plugins.0.enabled": false } });
    let (status, _) = f
        .call(&agent, Method::PUT, "/v1/settings", Some(edit.clone()))
        .await;
    assert_eq!(status, 403);
    assert!(!f.file().contains("enabled"));
    assert_eq!(f.decider_of("tool_call"), "gate");

    let (status, body) = f
        .call(&person, Method::PUT, "/v1/plugins/gate/enabled", Some(off))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({"name": "gate", "enabled": false, "changed": true, "unwrapped_runs": false, "unchecked_calls": false, "note": null})
    );
    let (_, plugins) = f.call(&person, Method::GET, "/v1/plugins", None).await;
    let gate = plugins["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "gate")
        .unwrap();
    assert_eq!(
        (&gate["enabled"], &gate["process"]),
        (&json!(false), &Value::Null)
    );
    let (status, _) = f
        .call(
            &person,
            Method::PUT,
            "/v1/plugins/nobody/enabled",
            Some(json!({"enabled": true})),
        )
        .await;
    assert_eq!(status, 404);
    let (status, _) = f
        .call(
            &person,
            Method::PUT,
            "/v1/plugins/gate/enabled",
            Some(json!({"enabled": true, "x": 1})),
        )
        .await;
    assert_eq!(status, 422);
    // A person may also turn it on through the settings route.
    let (status, saved) = f
        .call(
            &person,
            Method::PUT,
            "/v1/settings",
            Some(json!({ "remove": ["plugins.0.enabled"] })),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["restart_needed"], false);
    assert_eq!(f.decider_of("tool_call"), "gate");
}

const GATE_OF_START: &str = "[[plugins]]\nname = \"gate\"\npath = \"{script}\"\n\
                             args = [\"{dir}\", \"gate\"]\ndecides = [\"start_card\"]\n";

fn start_card(f: &Fixture) -> moebius_core::decide::Decision {
    f.server
        .app
        .decisions
        .decide(&Question {
            kind: "start_card".into(),
            options: vec!["start".into(), "hold".into()],
            fail_closed: "hold".into(),
            ..Question::default()
        })
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_decider_of_a_custom_kind_that_is_off_is_unrouted_and_its_decisions_fail_closed() {
    let f = Fixture::start(GATE_OF_START).await;
    assert!(f.server.app.decisions.decides("start_card"));
    assert_eq!(start_card(&f).decider, "gate");
    f.set("gate", false, false).unwrap();
    // No built-in decider takes the kind over: nothing answers it, and the fail-closed option is taken and recorded.
    assert!(!f.server.app.decisions.decides("start_card"));
    let held = start_card(&f);
    assert!(held.refused && held.choice == "hold" && held.decider == "none");
    assert!(held.reason.contains("no decider"), "{}", held.reason);
    f.set("gate", true, false).unwrap();
    let again = start_card(&f);
    assert!(!again.refused && again.decider == "gate");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_that_cannot_be_turned_on_is_an_error_and_the_file_stays_as_it_was() {
    let f = Fixture::start(GATE_OF_START).await;
    f.set("gate", false, false).unwrap();
    let off = f.file();
    // The log of the plugin cannot be opened: the plugin cannot be started at all.
    let log = f.plugin_dir().join("gate.log");
    std::fs::remove_file(&log).unwrap();
    std::fs::create_dir(&log).unwrap();
    let mut events = f.server.app.bus.subscribe();
    let person = f.server.token.to_string();
    let (status, body) = f
        .call(
            &person,
            Method::PUT,
            "/v1/plugins/gate/enabled",
            Some(json!({"enabled": true})),
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(
        body.to_string().contains("cannot turn gate on") && body.to_string().contains("gate.log"),
        "{body}"
    );
    assert_eq!(f.file(), off);
    assert!(f.server.app.decisions.host().is_off("gate"));
    assert!(!f.server.app.decisions.decides("start_card"));
    assert_eq!(f.starts(), 1);
    // Through the settings route as well, and nothing else of that change was applied.
    let (status, body) = f
        .call(
            &person,
            Method::PUT,
            "/v1/settings",
            Some(json!({"set": {"run_limit": 9}, "remove": ["plugins.0.enabled"]})),
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(f.file(), off);
    assert_ne!(f.server.live.get().run_limit, 9);
    assert!(
        std::iter::from_fn(|| events.try_recv().ok())
            .all(|e| !format!("{e:?}").contains("PluginEnabledChanged"))
    );
    // Once it can be started, it is turned on.
    std::fs::remove_dir(&log).unwrap();
    assert!(f.set("gate", true, false).unwrap().changed);
    assert_eq!(start_card(&f).decider, "gate");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_that_does_not_exist_is_on_and_refused_and_its_decisions_fail_closed() {
    let f = Fixture::start(
        "[[plugins]]\nname = \"gone\"\npath = \"/nonexistent/program\"\ndecides = [\"start_card\"]\n",
    )
    .await;
    f.set("gone", false, false).unwrap();
    assert!(f.set("gone", true, false).unwrap().changed);
    // It is on in the file and in the host, and fails: the host refuses every call, so the decision is the closed one.
    assert!(!f.file().contains("enabled"));
    assert!(!f.server.app.decisions.host().is_off("gone"));
    let held = start_card(&f);
    assert!(held.refused && held.choice == "hold" && held.decider == "gone");
}

/// One launcher, for every agent.
const LAUNCHER: &str = "[[plugins]]\nname = \"wide\"\npath = \"{script}\"\nargs = [\"{dir}\", \"wide\"]\nlaunches = true\n";

#[tokio::test(flavor = "multi_thread")]
async fn a_hand_edit_that_turns_off_the_last_launcher_does_not_block_other_saves_and_waits_for_a_confirmation()
 {
    let f = Fixture::start(LAUNCHER).await;
    let person = f.server.token.to_string();
    let launcher = || f.server.harness.pi_bolt.launcher.is_set();
    let mut events = f.server.app.bus.subscribe();
    // By hand, outside the server: the last launcher is turned off.
    std::fs::write(
        f.dir.path().join("moebius.toml"),
        f.file()
            .replace("launches = true", "launches = true\nenabled = false"),
    )
    .unwrap();
    let hand = f.file();
    assert!(hand.contains("enabled = false"));

    // An unrelated save goes through, as many times as wanted; the launcher stays on and the file keeps the edit.
    for limit in [7, 8] {
        let (status, saved) = f
            .call(
                &person,
                Method::PUT,
                "/v1/settings",
                Some(json!({"set": {"run_limit": limit}})),
            )
            .await;
        assert_eq!(status, 200, "{saved}");
        assert_eq!(f.server.live.get().run_limit, limit);
        assert!(launcher() && !f.server.app.decisions.host().is_off("wide"));
        assert!(f.file().contains("enabled = false"));
    }
    let seen: Vec<Value> = std::iter::from_fn(|| events.try_recv().ok())
        .map(|e| serde_json::to_value(&e).unwrap())
        .collect();
    let refused = seen
        .iter()
        .find(|e| e["kind"] == "plugin_enable_refused")
        .unwrap();
    assert_eq!(refused["data"]["name"], "wide");
    assert!(
        refused["data"]["message"]
            .as_str()
            .unwrap()
            .contains("unwrap_runs")
    );
    assert!(seen.iter().all(|e| e["kind"] != "plugin_enabled_changed"));

    // Turning it on says the file is in step with what runs, with nothing to start.
    let (status, on) = f
        .call(
            &person,
            Method::PUT,
            "/v1/plugins/wide/enabled",
            Some(json!({"enabled": true})),
        )
        .await;
    assert_eq!(status, 200, "{on}");
    assert_eq!(on["changed"], true);
    assert!(!f.file().contains("enabled"));
    assert_eq!(f.starts(), 1);

    // The same edit again is confirmed through the plugin route: without unwrap_runs it is a conflict.
    std::fs::write(f.dir.path().join("moebius.toml"), &hand).unwrap();
    let off = json!({"enabled": false});
    let (status, _) = f
        .call(
            &person,
            Method::PUT,
            "/v1/plugins/wide/enabled",
            Some(off.clone()),
        )
        .await;
    assert_eq!(status, 409);
    assert!(launcher());
    let (status, done) = f
        .call(
            &person,
            Method::PUT,
            "/v1/plugins/wide/enabled",
            Some(json!({"enabled": false, "unwrap_runs": true})),
        )
        .await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(
        (&done["changed"], &done["unwrapped_runs"]),
        (&json!(true), &json!(true))
    );
    assert!(!launcher() && f.server.app.decisions.host().is_off("wide"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_settings_route_confirms_unwrapping_runs_with_unwrap_runs() {
    let f = Fixture::start(LAUNCHER).await;
    let person = f.server.token.to_string();
    let off = json!({"set": {"plugins.0.enabled": false}});
    let (status, body) = f
        .call(&person, Method::PUT, "/v1/settings", Some(off))
        .await;
    assert_eq!(status, 409, "{body}");
    assert!(body.to_string().contains("unwrap_runs"), "{body}");
    assert!(!f.file().contains("enabled") && f.server.harness.pi_bolt.launcher.is_set());
    let confirmed = json!({"set": {"plugins.0.enabled": false}, "unwrap_runs": true});
    let (status, body) = f
        .call(&person, Method::PUT, "/v1/settings", Some(confirmed))
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(f.file().contains("enabled = false"));
    assert!(!f.server.harness.pi_bolt.launcher.is_set());
    // The flag is no part of the file or of a later change.
    assert!(!f.file().contains("unwrap_runs"));
    let again = json!({"set": {"run_limit": 3}});
    assert_eq!(
        f.call(&person, Method::PUT, "/v1/settings", Some(again))
            .await
            .0,
        200
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launcher_turned_on_is_asked_for_its_leftovers_beside_the_runs_that_exist() {
    let f = Fixture::start(LAUNCHER).await;
    // Asked at the start, when no run exists.
    assert_eq!(f.leftovers(), ["[]"]);
    f.set("wide", false, true).unwrap();
    let run = f
        .server
        .app
        .store
        .add_run(None, "Moebius", "pi-bolt", "chat")
        .unwrap();
    f.set("wide", true, false).unwrap();
    assert_eq!(f.leftovers(), ["[]".to_string(), format!("[{}]", run.id)]);
    // A launcher that is off when the server starts is not asked until it is turned on.
    f.set("wide", false, true).unwrap();
    let dir = f.dir;
    drop(f.server);
    let g = Fixture::open(dir).await;
    assert_eq!(g.leftovers().len(), 2);
    g.set("wide", true, false).unwrap();
    assert_eq!(g.leftovers().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_manifest_kept_of_a_plugin_is_private_and_read_back_only_when_it_is_a_valid_one() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::start(GATE).await;
    f.set("gate", false, false).unwrap();
    let kept = f.plugin_dir().join("gate.manifest.json");
    assert_eq!(
        std::fs::metadata(&kept).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let good = std::fs::read_to_string(&kept).unwrap();
    let dir = f.dir;
    drop(f.server);
    let big = "x".repeat(300 * 1024);
    let manifests = [
        // Over the size a manifest may have, the name of another plugin, and no kind at all.
        json!({"name": "gate", "version": "evil", "kinds": ["decider"], "settings_ui": big}),
        json!({"name": "other", "version": "evil", "kinds": ["decider"]}),
        json!({"name": "gate", "version": "evil", "kinds": []}),
    ];
    let mut dir = dir;
    for manifest in manifests {
        std::fs::write(
            dir.path().join(".moebius/plugins/gate.manifest.json"),
            manifest.to_string(),
        )
        .unwrap();
        let g = Fixture::open(dir).await;
        // It is listed from the entry, not from the kept file.
        let settings = g.server.app.app_settings().unwrap();
        assert!(settings.plugins.is_empty() || settings.plugins[0].version.is_empty());
        let listed = g.server.app.decisions.host().manifests();
        let gate = listed.iter().find(|m| m.name == "gate").unwrap();
        assert!(gate.version.is_empty() && gate.settings_ui.is_none());
        dir = g.dir;
        drop(g.server);
    }
    std::fs::write(dir.path().join(".moebius/plugins/gate.manifest.json"), good).unwrap();
    let g = Fixture::open(dir).await;
    let listed = g.server.app.decisions.host().manifests();
    assert_eq!(
        listed.iter().find(|m| m.name == "gate").unwrap().version,
        "1"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launcher_turned_off_while_a_run_goes_is_not_called_again_by_it() {
    let f = Fixture::start(LAUNCHER).await;
    // A run that started holds the launchers it started with.
    let held = f.server.harness.pi_bolt.launcher.get().unwrap();
    assert_eq!(held.plugin_for("Coder"), Some("wide"));
    held.clean_leftovers(&[]).unwrap();
    f.set("wide", false, true).unwrap();
    assert_eq!(held.plugin_for("Coder"), Some("wide"));
    // It is not asked again: the host refuses the call, so that whatever would be launched through it fails. (The stop
    // and cleanup commands a launch answered with are run by the core without the launcher, so a run that goes is
    // still stopped and cleaned up; `harness::launch` tests that.)
    let error = held.clean_leftovers(&[]).unwrap_err();
    assert!(error.contains("turned off"), "{error}");
}

/// What a decision of `kind` is, with the options a tool call has.
fn decision_of(f: &Fixture, kind: &str) -> moebius_core::decide::Decision {
    f.server
        .app
        .decisions
        .decide(&Question {
            kind: kind.into(),
            options: vec!["allow".into(), "refuse".into()],
            fail_closed: "refuse".into(),
            ..Question::default()
        })
        .unwrap()
}

/// What `GET /v1/plugins` says of the plugin `name`: whether it is on, and the kinds it answers.
async fn listed(f: &Fixture, name: &str) -> (bool, Vec<String>) {
    let token = f.server.token.to_string();
    let (_, plugins) = f.call(&token, Method::GET, "/v1/plugins", None).await;
    let plugin = plugins["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("{name} is not listed: {plugins}"));
    (
        plugin["enabled"].as_bool().unwrap(),
        plugin["decides"]
            .as_array()
            .unwrap()
            .iter()
            .map(|kind| kind.as_str().unwrap().to_string())
            .collect(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn the_localizer_is_turned_off_and_on_at_once_and_the_file_says_so() {
    let f = Fixture::start("").await;
    let person = f.server.token.to_string();
    let mut events = f.server.app.bus.subscribe();
    for (name, kinds) in [("localize", vec!["localize"])] {
        assert_eq!(
            listed(&f, name).await,
            (true, kinds.iter().map(|k| k.to_string()).collect())
        );
        for kind in &kinds {
            assert!(f.server.app.decisions.decides(kind));
        }
        let (status, body) = f
            .call(
                &person,
                Method::PUT,
                &format!("/v1/plugins/{name}/enabled"),
                Some(json!({"enabled": false})),
            )
            .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            body,
            json!({"name": name, "enabled": false, "changed": true, "unwrapped_runs": false, "unchecked_calls": false, "note": null})
        );
        // At once: nothing decides the kinds any more, the page says so, the file has the key, no restart is needed.
        for kind in &kinds {
            assert!(!f.server.app.decisions.decides(kind));
        }
        assert_eq!(listed(&f, name).await, (false, vec![]));
        assert!(
            f.file().contains(&format!("[{name}]\nenabled = false")),
            "{}",
            f.file()
        );
        assert!(f.server.live.view().unwrap().restart_keys.is_empty());
        assert!(f.server.live.view().unwrap().values[&format!("{name}.enabled")]["value"] == false);
        // The same again changes nothing.
        assert!(!f.set(name, false, false).unwrap().changed);
        // On again: the key is removed and the kinds are decided again.
        let (status, body) = f
            .call(
                &person,
                Method::PUT,
                &format!("/v1/plugins/{name}/enabled"),
                Some(json!({"enabled": true})),
            )
            .await;
        assert_eq!(status, 200, "{body}");
        assert!(!f.file().contains("enabled"), "{}", f.file());
        for kind in &kinds {
            assert!(f.server.app.decisions.decides(kind));
        }
        assert!(listed(&f, name).await.0);
    }
    let seen: Vec<Value> = std::iter::from_fn(|| events.try_recv().ok())
        .map(|e| serde_json::to_value(&e).unwrap())
        .filter(|e| e["kind"] == "plugin_enabled_changed")
        .map(|e| e["data"].clone())
        .collect();
    assert_eq!(
        seen,
        [
            json!({"name": "localize", "enabled": false}),
            json!({"name": "localize", "enabled": true}),
        ]
    );
    // The decision is recorded like any other plugin's.
    let kinds: Vec<String> = f
        .server
        .app
        .store
        .judgements("disable_plugin")
        .unwrap()
        .iter()
        .map(|j| j.decider.clone())
        .collect();
    assert_eq!(kinds, ["person"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_guard_is_turned_off_only_with_allow_unchecked_and_then_tool_calls_are_allowed_and_say_so()
 {
    let f = Fixture::start("").await;
    let person = f.server.token.to_string();
    let path = "/v1/plugins/guard/enabled";
    assert_eq!(
        listed(&f, "guard").await,
        (true, vec!["tool_call".to_string()])
    );
    // Refused without the confirmation, in words, and nothing changes.
    let (status, body) = f
        .call(&person, Method::PUT, path, Some(json!({"enabled": false})))
        .await;
    assert_eq!(status, 409, "{body}");
    assert!(body.to_string().contains("allow_unchecked"), "{body}");
    assert!(f.server.live.get().guard_enabled && !f.file().contains("guard"));
    assert!(decision_of(&f, "tool_call").decider.starts_with("guard"));

    // Only a person confirms it.
    let agent = f.agent_token();
    let (status, _) = f
        .call(
            &agent,
            Method::PUT,
            path,
            Some(json!({"enabled": false, "allow_unchecked": true})),
        )
        .await;
    assert_eq!(status, 403);

    let (status, body) = f
        .call(
            &person,
            Method::PUT,
            path,
            Some(json!({"enabled": false, "allow_unchecked": true})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        (
            &body["changed"],
            &body["unchecked_calls"],
            &body["unwrapped_runs"]
        ),
        (&json!(true), &json!(true), &json!(false))
    );
    assert!(body["note"].as_str().unwrap().contains("no longer checked"));
    assert!(
        f.file().contains("[guard]\nenabled = false"),
        "{}",
        f.file()
    );
    assert!(!f.server.live.get().guard_enabled);
    assert_eq!(listed(&f, "guard").await, (false, vec![]));

    // A call that the guard would refuse is allowed, and the judgement says that the guard is off.
    let call = decision_of(&f, "tool_call");
    assert!(!call.refused && call.choice == "allow" && call.decider == "none");
    assert!(call.reason.contains("the guard is off"), "{}", call.reason);
    let judged = f.server.app.store.judgements("tool_call").unwrap();
    assert!(judged.last().unwrap().reason.contains("the guard is off"));
    // Other kinds still fail closed with no decider.
    assert!(decision_of(&f, "nobody_decides").refused);

    // On again: no confirmation, the key is gone, the guard decides.
    let (status, body) = f
        .call(&person, Method::PUT, path, Some(json!({"enabled": true})))
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(!f.file().contains("guard") && f.server.live.get().guard_enabled);
    assert!(decision_of(&f, "tool_call").decider.starts_with("guard"));
    assert!(listed(&f, "guard").await.0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_that_decides_tool_call_keeps_it_and_gives_it_back_to_what_is_on() {
    let f = Fixture::start(GATE).await;
    f.set("guard", false, false).unwrap_err();
    f.server
        .live
        .set_plugin_enabled(&f.server.app, &Actor::Person, "guard", false, false, true)
        .unwrap();
    // The plugin decides it; the guard being off changes nothing for the calls it checks.
    assert_eq!(decision_of(&f, "tool_call").decider, "gate");
    // The plugin off with the guard off: calls are allowed with the note, never left without a decider.
    f.set("gate", false, false).unwrap();
    let open = decision_of(&f, "tool_call");
    assert!(!open.refused && open.choice == "allow" && open.decider == "none");
    assert!(open.reason.contains("the guard is off"));
    // The plugin on again takes the kind back from the note.
    f.set("gate", true, false).unwrap();
    assert_eq!(decision_of(&f, "tool_call").decider, "gate");
    // The guard on, then the plugin off: the kind goes to the guard.
    f.set("guard", true, false).unwrap();
    assert_eq!(decision_of(&f, "tool_call").decider, "gate");
    f.set("gate", false, false).unwrap();
    assert!(decision_of(&f, "tool_call").decider.starts_with("guard"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_settings_route_turns_the_guard_off_only_with_allow_unchecked() {
    let f = Fixture::start("").await;
    let person = f.server.token.to_string();
    let off = json!({"set": {"guard.enabled": false}});
    let (status, body) = f
        .call(&person, Method::PUT, "/v1/settings", Some(off))
        .await;
    assert_eq!(status, 409, "{body}");
    assert!(body.to_string().contains("allow_unchecked"), "{body}");
    assert!(!f.file().contains("guard") && f.server.live.get().guard_enabled);
    let confirmed = json!({"set": {"guard.enabled": false}, "allow_unchecked": true});
    let (status, body) = f
        .call(&person, Method::PUT, "/v1/settings", Some(confirmed))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["restart_needed"], false);
    assert!(f.file().contains("enabled = false") && !f.file().contains("allow_unchecked"));
    assert!(!f.server.live.get().guard_enabled);
    assert_eq!(decision_of(&f, "tool_call").decider, "none");
    // An agent cannot do it through the settings either.
    let agent = f.agent_token();
    f.set("guard", true, false).unwrap();
    let (status, _) = f
        .call(
            &agent,
            Method::PUT,
            "/v1/settings",
            Some(json!({"set": {"localize.enabled": false}})),
        )
        .await;
    assert_eq!(status, 403);
    // The localizer needs no confirmation through the settings route.
    let (status, body) = f
        .call(
            &person,
            Method::PUT,
            "/v1/settings",
            Some(json!({"set": {"localize.enabled": false}})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(!f.server.app.decisions.decides("localize"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hand_edit_that_turns_the_guard_off_is_honoured_at_the_start_but_refused_while_it_runs() {
    // At the start the file is honoured. (Its `[lens]` table is one of the removed lens, which is ignored.)
    let f = Fixture::start("[guard]\nenabled = false\n[lens]\nenabled = false\n").await;
    assert!(f.server.config.has_lens_table);
    assert!(!f.server.live.get().guard_enabled);
    assert_eq!(decision_of(&f, "tool_call").decider, "none");
    assert_eq!(listed(&f, "guard").await, (false, vec![]));
    // Turned on, the key goes and the guard decides.
    f.set("guard", true, false).unwrap();
    assert!(decision_of(&f, "tool_call").decider.starts_with("guard"));

    // While it runs, a hand edit is not applied by the next save: nobody confirmed it.
    let g = Fixture::start("").await;
    let person = g.server.token.to_string();
    let mut events = g.server.app.bus.subscribe();
    let hand = format!("{}\n[guard]\nenabled = false\n", g.file());
    std::fs::write(g.dir.path().join("moebius.toml"), &hand).unwrap();
    let (status, saved) = g
        .call(
            &person,
            Method::PUT,
            "/v1/settings",
            Some(json!({"set": {"run_limit": 7}})),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(g.server.live.get().run_limit, 7);
    assert!(g.server.live.get().guard_enabled);
    assert!(decision_of(&g, "tool_call").decider.starts_with("guard"));
    assert!(g.file().contains("enabled = false"));
    let seen: Vec<Value> = std::iter::from_fn(|| events.try_recv().ok())
        .map(|e| serde_json::to_value(&e).unwrap())
        .collect();
    let refused = seen
        .iter()
        .find(|e| e["kind"] == "plugin_enable_refused")
        .unwrap();
    assert_eq!(refused["data"]["name"], "guard");
    assert!(
        refused["data"]["message"]
            .as_str()
            .unwrap()
            .contains("allow_unchecked")
    );
    assert!(seen.iter().all(|e| e["kind"] != "plugin_enabled_changed"));
    // A person confirms it with the route.
    let (status, _) = g
        .call(
            &person,
            Method::PUT,
            "/v1/plugins/guard/enabled",
            Some(json!({"enabled": false})),
        )
        .await;
    assert_eq!(status, 409);
    let (status, done) = g
        .call(
            &person,
            Method::PUT,
            "/v1/plugins/guard/enabled",
            Some(json!({"enabled": false, "allow_unchecked": true})),
        )
        .await;
    assert_eq!(status, 200, "{done}");
    assert!(!g.server.live.get().guard_enabled);
    assert_eq!(decision_of(&g, "tool_call").decider, "none");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_switch_that_is_off_turns_the_feature_off_for_a_plugin_that_decides_its_kind_too() {
    let f = Fixture::start(
        "[[plugins]]\nname = \"gate\"\npath = \"{script}\"\nargs = [\"{dir}\", \"gate\"]\n\
         decides = [\"start_card\", \"localize\"]\n",
    )
    .await;
    assert!(f.server.app.decisions.decides("localize"));
    assert_eq!(decision_of(&f, "localize").decider, "gate");
    f.set("localize", false, false).unwrap();
    assert!(!f.server.app.decisions.decides("localize"));
    // The plugin is still on and keeps its other kind.
    assert!(f.server.app.decisions.decides("start_card"));
    assert_eq!(
        listed(&f, "gate").await,
        (true, vec!["start_card".to_string()])
    );
    // Turning the plugin off and on meanwhile does not bring the kind back.
    f.set("gate", false, false).unwrap();
    f.set("gate", true, false).unwrap();
    assert!(!f.server.app.decisions.decides("localize"));
    f.set("localize", true, false).unwrap();
    assert_eq!(decision_of(&f, "localize").decider, "gate");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_with_the_table_of_the_removed_lens_still_runs_and_its_old_judgements_stay_readable()
{
    let f = Fixture::start("[lens]\nenabled = false\n").await;
    let person = f.server.token.to_string();
    // Edits through the app do not trip over the table.
    let (status, saved) = f
        .call(
            &person,
            Method::PUT,
            "/v1/settings",
            Some(json!({"set": {"run_limit": 7}})),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(f.server.live.get().run_limit, 7);
    // The lens is no plugin and no setting any more.
    let (status, _) = f
        .call(
            &person,
            Method::PUT,
            "/v1/plugins/lens/enabled",
            Some(json!({"enabled": false})),
        )
        .await;
    assert_eq!(status, 404);
    let (status, settings) = f.call(&person, Method::GET, "/v1/settings", None).await;
    assert_eq!(status, 200, "{settings}");
    assert!(!settings.to_string().contains("lens"), "{settings}");
    // A judgement of a kind the lens decided, recorded by an earlier version, is shown like any other.
    for kind in ["lint_report", "deep_check"] {
        f.server
            .app
            .store
            .add_judgement(&moebius_core::store::NewJudgement {
                kind: kind.into(),
                options: vec!["show".into(), "hide".into()],
                choice: "show".into(),
                decider: "lens".into(),
                ..Default::default()
            })
            .unwrap();
    }
    for kind in ["lint_report", "deep_check"] {
        let (status, shown) = f
            .call(
                &person,
                Method::GET,
                &format!("/v1/judgements?kind={kind}"),
                None,
            )
            .await;
        assert_eq!(status, 200, "{shown}");
        assert_eq!(shown.as_array().unwrap().len(), 1, "{shown}");
    }
    let (status, kinds) = f
        .call(&person, Method::GET, "/v1/judgements/kinds", None)
        .await;
    assert_eq!(status, 200, "{kinds}");
    let (status, usage) = f.call(&person, Method::GET, "/v1/usage", None).await;
    assert_eq!(status, 200, "{usage}");
}
