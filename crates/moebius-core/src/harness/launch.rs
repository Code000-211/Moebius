//! The launcher of harness processes: the plugin of kind `launcher` that may wrap how a harness process starts, such
//! as inside a sandbox (see `docs/reference/plugin-api.md`). A harness describes the process it would start as an
//! [`Ask`] and its `Command`; [`Launcher::start`] returns the command to start instead and a [`Launched`], the handle
//! that stops the wrapped process tree and cleans up after it. A launcher that does not answer, fails or answers
//! something the core cannot use is an error, so the run fails: a configured launcher is never skipped, and neither is
//! a run: the configuration makes the last launcher take every agent, and a run that no launcher takes (a
//! [`Launcher`] made by hand without one) fails instead of starting bare. Without a launcher the harnesses do not
//! come here at all.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use moebius_plugin_api::{
    CommandLine, Host, LaunchAnswer, LaunchRequest, LaunchRun, LeftoversRequest, Urls, Wrapped,
};

use crate::blobs::redact;

/// How long a stop or cleanup command of a launcher may take before it is killed.
const COMMAND_LIMIT: Duration = Duration::from_secs(30);

/// The launcher plugins of the plugin host, in the order of the configuration: a run goes to the first whose agents
/// include the run's agent (or that names none). The configuration makes the last one take every agent, so every run
/// has a launcher; a run that none takes fails, it never starts unwrapped.
pub struct Launcher {
    host: Arc<Host>,
    plugins: Vec<Scoped>,
}

/// The launchers the harnesses start runs through, which can change while the server runs: a person turns a launcher
/// plugin off or on. A run reads the launchers once, as it starts, and keeps them to its end; the slot is shared by
/// the harnesses.
#[derive(Clone, Default)]
pub struct LauncherSlot(Arc<RwLock<Option<Arc<Launcher>>>>);

impl LauncherSlot {
    /// The launchers now; none when runs start without one.
    pub fn get(&self) -> Option<Arc<Launcher>> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Whether a launcher wraps the runs that start now.
    pub fn is_set(&self) -> bool {
        self.get().is_some()
    }

    /// Makes `launcher` take the runs that start from now on.
    pub fn set(&self, launcher: Option<Arc<Launcher>>) {
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = launcher;
    }
}

/// One launcher plugin and the agents it is asked about; empty, every agent.
struct Scoped {
    plugin: String,
    agents: Vec<String>,
}

/// The repository a run works in, by name, and the launcher's project settings for it, by key.
pub(crate) type RunRepository = (String, BTreeMap<String, String>);

/// What a harness tells the launcher about the process it is about to start.
pub(crate) struct Ask<'a> {
    pub(crate) run: LaunchRun,
    pub(crate) dir: &'a Path,
    pub(crate) read_only: Vec<PathBuf>,
    pub(crate) read_write: Vec<PathBuf>,
    pub(crate) urls: Urls,
    pub(crate) clone: Option<&'a Path>,
    /// The runner when the run is not on the server.
    pub(crate) runner: Option<&'a str>,
    /// The repository the run works in, by name, and the launcher's project settings for it.
    pub(crate) repository: Option<RunRepository>,
    /// The launcher's app settings, by key.
    pub(crate) app_settings: BTreeMap<String, String>,
    /// The file the process's standard error goes to, which also gets a line when a stop or cleanup command fails.
    pub(crate) log: &'a Path,
}

/// A launch the launcher wrapped: start `command` instead; `launched` ends it and cleans up after it, and `urls` are the
/// addresses the process sees where they differ from the server's.
pub(crate) struct Wrapping {
    pub(crate) command: Command,
    pub(crate) launched: Arc<Launched>,
    pub(crate) urls: moebius_plugin_api::UrlsInside,
}

/// A wrapped process: ends its tree with the launcher's stop command and cleans up after it once.
pub(crate) struct Launched {
    log: PathBuf,
    stop: Option<CommandLine>,
    cleanup: Mutex<Option<(Option<CommandLine>, Vec<String>)>>,
}

/// Why a run of `agent` fails when no launcher takes it: it is never started without the launcher.
pub(crate) fn no_launcher(agent: &str) -> String {
    format!("no launcher takes the agent {agent}, so its run does not start unwrapped")
}

impl Launcher {
    /// A launcher that is asked about every run.
    pub fn new(host: Arc<Host>, plugin: impl Into<String>) -> Self {
        Self::scoped(host, [(plugin.into(), Vec::new())])
    }

    /// Launchers in order, each with the agents it is asked about (empty: every agent).
    pub fn scoped(
        host: Arc<Host>,
        plugins: impl IntoIterator<Item = (String, Vec<String>)>,
    ) -> Self {
        Self {
            host,
            plugins: plugins
                .into_iter()
                .map(|(plugin, agents)| Scoped { plugin, agents })
                .collect(),
        }
    }

    /// The launcher that wraps the runs of `agent`, which its settings are kept under. None only for a launcher list
    /// that the configuration would have refused (its last one names agents); the caller fails the run then.
    pub fn plugin_for(&self, agent: &str) -> Option<&str> {
        self.plugins
            .iter()
            .find(|each| each.agents.is_empty() || each.agents.iter().any(|name| name == agent))
            .map(|each| each.plugin.as_str())
    }

    /// Asks the launcher how to start `command` (the process as the harness would start it). `Err` with why when the
    /// launcher refuses, fails, is too slow or wraps a run that cannot be wrapped; the caller fails the run and never
    /// starts `command` then. The first of the result is the launcher's note about the launch, if it gave one.
    pub(crate) fn start(
        &self,
        ask: Ask,
        command: &Command,
    ) -> Result<(Option<String>, Option<Wrapping>), String> {
        let Some(plugin) = self.plugin_for(&ask.run.agent) else {
            return Err(no_launcher(&ask.run.agent));
        };
        let log = ask.log.to_path_buf();
        let request = request(ask, command);
        let remote = request.runner.is_some();
        let answer = self.host.launch(plugin, &request).map_err(|refusal| {
            format!(
                "the launcher {plugin} did not start the run: {}",
                redact(&refusal.to_string())
            )
        })?;
        let wrapped = match answer {
            LaunchAnswer::Unchanged { note } => return Ok((note, None)),
            LaunchAnswer::Wrapped(wrapped) => *wrapped,
        };
        let Wrapped {
            program,
            args,
            env,
            urls,
            stop,
            cleanup,
            remove,
            note,
        } = wrapped;
        // The launcher may have prepared things already, so whatever fails from here on cleans up after it.
        let launched = Arc::new(Launched {
            log,
            stop,
            cleanup: Mutex::new(Some((cleanup, remove))),
        });
        if remote {
            launched.finish();
            return Err(format!(
                "the launcher {plugin} wrapped a run on a runner, which it cannot"
            ));
        }
        if program.is_empty() {
            launched.finish();
            return Err(format!(
                "the launcher {plugin} answered a launch without a program"
            ));
        }
        let mut wrapped = Command::new(program);
        wrapped.args(args).env_clear().envs(env);
        if let Some(dir) = command.get_current_dir() {
            wrapped.current_dir(dir);
        }
        Ok((
            note,
            Some(Wrapping {
                command: wrapped,
                launched,
                urls,
            }),
        ))
    }

    /// Asks every launcher what earlier runs left and removes it. `running` are the ids of the runs that exist. A
    /// launcher that does not answer is an error, so that the server does not start with a sandbox it cannot tidy.
    pub fn clean_leftovers(&self, running: &[i64]) -> Result<(), String> {
        for each in &self.plugins {
            self.clean_leftovers_of(&each.plugin, running)?;
        }
        Ok(())
    }

    fn clean_leftovers_of(&self, plugin: &str, running: &[i64]) -> Result<(), String> {
        let request = LeftoversRequest {
            running: running.to_vec(),
        };
        let left = self.host.leftovers(plugin, &request).map_err(|refusal| {
            format!(
                "the launcher {plugin} did not name its leftovers: {}",
                redact(&refusal.to_string())
            )
        })?;
        let mut failed = Vec::new();
        for command in &left.cleanup {
            if let Err(e) = run_command(command, COMMAND_LIMIT) {
                failed.push(e);
            }
        }
        remove_files(&left.remove);
        match failed.is_empty() {
            true => Ok(()),
            false => Err(format!(
                "the launcher {plugin} left what it could not clean up: {}",
                failed.join("; ")
            )),
        }
    }
}

/// Cleans up after a wrapped process when dropped, unless it was started: for a start that fails after the launcher
/// answered.
pub(crate) struct Unstarted(Option<Arc<Launched>>);

impl Unstarted {
    pub(crate) fn new(launched: Option<&Arc<Launched>>) -> Self {
        Self(launched.cloned())
    }

    /// The process started: its reader cleans up from now on.
    pub(crate) fn started(mut self) {
        self.0 = None;
    }
}

impl Drop for Unstarted {
    fn drop(&mut self) {
        if let Some(launched) = &self.0 {
            launched.finish();
        }
    }
}

impl Launched {
    /// Ends the wrapped process tree with the launcher's stop command, if it gave one. The caller still kills the
    /// wrapper's own process group. A stop command that fails is noted in the run's log.
    pub(crate) fn stop(&self) {
        if let Some(stop) = &self.stop
            && let Err(e) = run_command(stop, COMMAND_LIMIT)
        {
            self.note("stop", &e);
        }
    }

    /// Runs the cleanup command and removes the files, the first time only. A cleanup that fails is noted in the
    /// run's log.
    pub(crate) fn finish(&self) {
        let taken = self
            .cleanup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let Some((cleanup, remove)) = taken else {
            return;
        };
        if let Some(cleanup) = &cleanup
            && let Err(e) = run_command(cleanup, COMMAND_LIMIT)
        {
            self.note("cleanup", &e);
        }
        remove_files(&remove);
    }

    fn note(&self, what: &str, why: &str) {
        use std::io::Write;
        if let Ok(mut log) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
        {
            let _ = writeln!(
                log,
                "moebius: the launcher's {what} command failed: {}",
                redact(why)
            );
        }
    }
}

/// The request for the process `ask` describes, which `command` would start.
fn request(ask: Ask, command: &Command) -> LaunchRequest {
    let text = |path: &Path| path.to_string_lossy().into_owned();
    let repository = ask.repository;
    LaunchRequest {
        run: ask.run,
        program: command.get_program().to_string_lossy().into_owned(),
        args: command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect(),
        env: command
            .get_envs()
            .filter_map(|(name, value)| {
                Some((
                    name.to_string_lossy().into_owned(),
                    value?.to_string_lossy().into_owned(),
                ))
            })
            .collect(),
        dir: text(ask.dir),
        read_only: ask.read_only.iter().map(|p| text(p)).collect(),
        read_write: ask.read_write.iter().map(|p| text(p)).collect(),
        urls: ask.urls,
        clone: ask.clone.map(text),
        runner: ask.runner.map(String::from),
        repository: repository.as_ref().map(|(name, _)| name.clone()),
        project_settings: repository.map(|(_, values)| values).unwrap_or_default(),
        app_settings: ask.app_settings,
    }
}

/// Runs a command of the launcher in its own process group and waits for it, at most `limit`; it is killed
/// with what it started after that.
fn run_command(line: &CommandLine, limit: Duration) -> Result<(), String> {
    let mut command = Command::new(&line.program);
    crate::harness::clear_environment(&mut command);
    command.args(&line.args).envs(&line.env);
    let mut child = crate::platform::own_group(&mut command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", line.program))?;
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("{} failed ({status})", line.program)),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                crate::platform::kill_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{} took too long", line.program));
            }
            Err(e) => return Err(format!("cannot wait for {}: {e}", line.program)),
        }
    }
}

/// Removes the files and directories the launcher created.
fn remove_files(paths: &[String]) {
    for path in paths {
        let path = Path::new(path);
        let _ = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
    }
}

// Unix only: the stop and cleanup commands are run with `sh`.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use moebius_plugin_api::{Call, Kind, Manifest, Plugin, PluginError};
    use serde_json::{Value, json};

    /// A launcher that wraps every launch with a cleanup command that appends to `marker`, and names a leftover whose
    /// cleanup command fails when `broken`.
    struct Wraps {
        marker: PathBuf,
        broken: bool,
        program: String,
    }

    impl Plugin for Wraps {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: "box".into(),
                version: "0".into(),
                kinds: vec![Kind::Launcher],
                ..Default::default()
            }
        }

        fn call(&self, call: Call) -> Result<Value, PluginError> {
            let marker = self.marker.to_string_lossy().into_owned();
            let append = |text: &str| json!({"program": "sh", "args": ["-c", "echo \"$0\" >> \"$1\"", text, marker]});
            Ok(match call.method.as_str() {
                "leftovers" if self.broken => json!({"cleanup": [{"program": "false"}]}),
                "leftovers" => {
                    json!({"cleanup": [append("swept")], "remove": [marker.clone() + ".old"]})
                }
                _ => json!({
                    "answer": "wrapped", "program": self.program, "args": ["--", "pi"],
                    "env": {"ONLY": "this"}, "cleanup": append("cleaned"), "remove": [marker.clone() + ".made"]
                }),
            })
        }
    }

    fn launcher(dir: &Path, broken: bool) -> Launcher {
        launcher_for(dir, broken, "wrap")
    }

    fn launcher_for(dir: &Path, broken: bool, program: &str) -> Launcher {
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Wraps {
            marker: dir.join("marker"),
            broken,
            program: program.into(),
        });
        Launcher::new(Arc::new(host), "box")
    }

    fn ask<'a>(dir: &'a Path, runner: Option<&'a str>, log: &'a Path) -> Ask<'a> {
        Ask {
            run: LaunchRun {
                id: 1,
                purpose: "code".into(),
                agent: "Coder".into(),
                chat: false,
                card: None,
            },
            dir,
            read_only: vec![],
            read_write: vec![],
            urls: Urls {
                router: "r".into(),
                gateway: "g".into(),
                check: "c".into(),
            },
            clone: None,
            runner,
            repository: None,
            app_settings: BTreeMap::new(),
            log,
        }
    }

    #[test]
    fn a_wrapped_command_has_only_the_environment_the_launcher_gave_and_is_cleaned_up_once() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let mut original = Command::new("pi");
        original.env("TOKEN", "t").current_dir(dir.path());
        std::fs::write(dir.path().join("marker.made"), "").unwrap();
        let Wrapping {
            command, launched, ..
        } = launcher(dir.path(), false)
            .start(ask(dir.path(), None, &log), &original)
            .unwrap()
            .1
            .expect("the launch is wrapped");
        assert_eq!(command.get_program(), "wrap");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["--", "pi"]);
        let envs: Vec<_> = command.get_envs().collect();
        assert_eq!(envs.len(), 1, "{envs:?}");
        assert_eq!(command.get_current_dir(), Some(dir.path()));
        launched.finish();
        launched.finish();
        let marker = std::fs::read_to_string(dir.path().join("marker")).unwrap();
        assert_eq!(marker, "cleaned\n");
        assert!(!dir.path().join("marker.made").exists());
    }

    #[test]
    fn a_run_goes_to_the_first_launcher_that_takes_its_agent_and_a_run_no_launcher_takes_fails() {
        let host = Arc::new(Host::new(Duration::from_secs(30)));
        let named = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        let both = Launcher::scoped(
            host.clone(),
            [
                ("nono".to_string(), named(&["Moebius"])),
                ("podman".to_string(), Vec::new()),
            ],
        );
        assert_eq!(both.plugin_for("Moebius"), Some("nono"));
        assert_eq!(both.plugin_for("Coder"), Some("podman"));
        let only = Launcher::scoped(host, [("nono".to_string(), named(&["Moebius", "Retro"]))]);
        assert_eq!(only.plugin_for("Retro"), Some("nono"));
        assert_eq!(only.plugin_for("Coder"), None);

        // A run no launcher takes fails, and the launcher is not asked: it never starts bare.
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let error = only
            .start(ask(dir.path(), None, &log), &Command::new("pi"))
            .err()
            .expect("the run fails");
        assert!(
            error.contains("no launcher takes the agent Coder"),
            "{error}"
        );
        only.clean_leftovers(&[]).unwrap_err();
    }

    #[test]
    fn a_launcher_turned_off_is_not_asked_again_by_a_launcher_that_holds_it_but_a_run_that_goes_is_cleaned_up()
     {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Wraps {
            marker: dir.path().join("marker"),
            broken: false,
            program: "wrap".into(),
        });
        let host = Arc::new(host);
        let held = Launcher::new(host.clone(), "box");
        std::fs::write(dir.path().join("marker.made"), "").unwrap();
        let Wrapping { launched, .. } = held
            .start(ask(dir.path(), None, &log), &Command::new("pi"))
            .unwrap()
            .1
            .expect("the launch is wrapped");
        // The launcher is turned off while the run goes.
        host.add_off(Manifest {
            name: "box".into(),
            kinds: vec![Kind::Launcher],
            ..Default::default()
        });
        // A launch through the launchers the run holds fails closed, and so do its leftovers.
        let error = held
            .start(ask(dir.path(), None, &log), &Command::new("pi"))
            .err()
            .expect("the launch fails");
        assert!(error.contains("turned off"), "{error}");
        assert!(
            held.clean_leftovers(&[1])
                .unwrap_err()
                .contains("turned off")
        );
        // The run that goes is cleaned up by the commands the launcher gave it: they need no call into the launcher.
        launched.finish();
        let marker = std::fs::read_to_string(dir.path().join("marker")).unwrap();
        assert_eq!(marker, "cleaned\n");
        assert!(!dir.path().join("marker.made").exists());
    }

    #[test]
    fn a_launcher_cannot_wrap_a_run_on_a_runner() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let result = launcher(dir.path(), false)
            .start(ask(dir.path(), Some("gpu"), &log), &Command::new("pi"));
        assert!(result.is_err_and(|e| e.contains("runner")));
    }

    #[test]
    fn leftovers_are_cleaned_up_and_a_cleanup_that_fails_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("marker.old"), "").unwrap();
        launcher(dir.path(), false).clean_leftovers(&[]).unwrap();
        assert!(!dir.path().join("marker.old").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("marker")).unwrap(),
            "swept\n"
        );
        let error = launcher(dir.path(), true).clean_leftovers(&[]).unwrap_err();
        assert!(error.contains("could not clean up"), "{error}");
    }

    #[test]
    fn a_wrapped_answer_that_cannot_be_started_is_cleaned_up_after() {
        let dir = tempfile::tempdir().unwrap();
        let (log, made) = (dir.path().join("log"), dir.path().join("marker.made"));
        // A wrapped answer without a program, and one for a run on a runner: both fail, and neither leaves what the
        // launcher made.
        for (program, runner) in [("", None), ("wrap", Some("gpu"))] {
            std::fs::write(&made, "").unwrap();
            let result = launcher_for(dir.path(), false, program)
                .start(ask(dir.path(), runner, &log), &Command::new("pi"));
            assert!(result.is_err(), "{program:?} {runner:?}");
            assert!(!made.exists(), "{program:?} {runner:?}");
            let marker = std::fs::read_to_string(dir.path().join("marker")).unwrap();
            assert_eq!(marker, "cleaned\n", "{program:?} {runner:?}");
            std::fs::remove_file(dir.path().join("marker")).unwrap();
        }
    }

    #[test]
    fn a_launch_that_does_not_start_is_cleaned_up_by_its_guard() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let (_, wrapping) = launcher(dir.path(), false)
            .start(ask(dir.path(), None, &log), &Command::new("pi"))
            .unwrap();
        let launched = wrapping.unwrap().launched;
        Unstarted::new(Some(&launched)).started();
        assert!(!dir.path().join("marker").exists());
        drop(Unstarted::new(Some(&launched)));
        let marker = std::fs::read_to_string(dir.path().join("marker")).unwrap();
        assert_eq!(marker, "cleaned\n");
    }

    #[test]
    fn a_stop_command_that_takes_too_long_is_killed() {
        let started = Instant::now();
        let result = run_command(
            &CommandLine {
                program: "sleep".into(),
                args: vec!["300".into()],
                env: Default::default(),
            },
            Duration::from_millis(300),
        );
        assert!(started.elapsed() < Duration::from_secs(60));
        assert!(result.is_err_and(|e| e.contains("too long")));
    }

    #[test]
    fn a_failing_stop_command_is_noted_in_the_log() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let launched = Launched {
            log: log.clone(),
            stop: Some(CommandLine {
                program: "false".into(),
                args: vec![],
                env: Default::default(),
            }),
            cleanup: Mutex::new(None),
        };
        launched.stop();
        let text = std::fs::read_to_string(log).unwrap();
        assert!(text.contains("stop command failed"), "{text}");
    }
}
