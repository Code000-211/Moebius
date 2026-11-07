//! The plugins in their own process that `[[plugins]]` names: each that is on is started at the server's start, added to
//! the plugin host and kept running by it (see `moebius_plugin_api::Process`). Its standard error, and a line each time
//! it stops, go with secrets redacted to its log, `<data_dir>/plugins/<name>.log`.
//!
//! A plugin that is off (`enabled = false`) is not started: the host holds it as off with the manifest it last sent,
//! kept in `<data_dir>/plugins/<name>.manifest.json`, so that its settings can still be shown. [`set_enabled`] turns a
//! plugin off or on while the server runs and builds the launchers again. The manifest kept for it is private to the
//! owner and read back only when valid.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use moebius_plugin_api::{Kind, MAX_SETTINGS_UI, Manifest, Process, ProcessSpec, State};

use crate::app::App;
use crate::blobs::redact;
use crate::config::{Config, PluginConfig};
use crate::decide::{Decisions, guard, localize};
use crate::harness::launch::{Launcher, LauncherSlot};
use crate::harness::lens;

/// The log of the plugin `name`.
pub fn log_path(data_dir: &Path, name: &str) -> PathBuf {
    data_dir.join("plugins").join(format!("{name}.log"))
}

/// The file the manifest of the plugin `name` is kept in, for as long as it is off.
fn manifest_path(data_dir: &Path, name: &str) -> PathBuf {
    data_dir
        .join("plugins")
        .join(format!("{name}.manifest.json"))
}

/// Keeps the manifest a plugin that runs sent, best effort and readable by the owner only: it is all that is known of
/// the plugin while it is off, and after a restart.
fn remember_manifest(data_dir: &Path, manifest: &Manifest) {
    if manifest.kinds.is_empty() {
        return;
    }
    if let Ok(text) = serde_json::to_vec(manifest) {
        let _ = crate::server::settings::write_atomically(
            &manifest_path(data_dir, &manifest.name),
            &text,
            true,
        );
    }
}

/// Drops the kept manifest of the plugin `name`, such as when a new version of it is installed while it is off: the
/// old version's manifest must not stand for the new one.
pub fn forget_manifest(data_dir: &Path, name: &str) {
    let _ = std::fs::remove_file(manifest_path(data_dir, name));
}

/// The manifest kept for `plugin`, if it is one a plugin of that name could have sent: it names the plugin, lists a
/// kind, and its `settings_ui` is not over [`MAX_SETTINGS_UI`], as a manifest sent by the process must be. The file is
/// not read at all when it is far larger than a manifest can be.
fn kept_manifest(data_dir: &Path, plugin: &PluginConfig) -> Option<Manifest> {
    let path = manifest_path(data_dir, &plugin.name);
    if std::fs::metadata(&path).ok()?.len() > 4 * MAX_SETTINGS_UI as u64 {
        return None;
    }
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let fits = manifest
        .settings_ui
        .as_ref()
        .is_none_or(|ui| ui.len() <= MAX_SETTINGS_UI);
    (manifest.name == plugin.name && !manifest.kinds.is_empty() && fits).then_some(manifest)
}

/// What is known of the plugin `plugin` while it does not run: the manifest it last sent, else what its entry says.
fn manifest_of_off(config: &Config, plugin: &PluginConfig) -> Manifest {
    kept_manifest(&config.data_dir, plugin).unwrap_or_else(|| manifest_of_entry(plugin, ""))
}

/// What the entry of `plugin` says of it: its name, `version` and the kinds it decides or launches.
pub fn manifest_of_entry(plugin: &PluginConfig, version: &str) -> Manifest {
    let mut kinds = Vec::new();
    if !plugin.decides.is_empty() {
        kinds.push(Kind::Decider);
    }
    if plugin.launches {
        kinds.push(Kind::Launcher);
    }
    Manifest {
        name: plugin.name.clone(),
        version: version.to_string(),
        kinds,
        ..Manifest::default()
    }
}

/// Puts `plugin` in `host`: started when it is on, else held as off. Not routed.
pub fn put_in_host(
    host: &moebius_plugin_api::Host,
    config: &Config,
    plugin: &PluginConfig,
) -> Result<(), String> {
    if plugin.enabled {
        let process = start(config, plugin, host.limit())?;
        if process.status().state == State::Running {
            remember_manifest(&config.data_dir, &process.manifest());
        }
        host.set_process(process);
    } else {
        host.add_off(manifest_of_off(config, plugin));
    }
    Ok(())
}

/// The built-in decider of a kind, which decides it when no plugin does.
pub fn built_in_decider(kind: &str) -> Option<&'static str> {
    match kind {
        "tool_call" => Some(guard::NAME),
        localize::KIND => Some(localize::KIND),
        "lint_report" | "deep_check" => Some(lens::NAME),
        _ => None,
    }
}

/// Lets the built-in deciders decide their kinds, and the deciders of the plugins that are on decide theirs.
pub fn route_deciders(decisions: &Decisions, plugins: &[PluginConfig]) {
    for kind in ["tool_call", localize::KIND, "lint_report", "deep_check"] {
        if let Some(decider) = built_in_decider(kind) {
            decisions.route(kind, decider);
        }
    }
    for plugin in plugins.iter().filter(|plugin| plugin.enabled) {
        for kind in &plugin.decides {
            decisions.route(kind, &plugin.name);
        }
    }
}

/// The launchers of the plugins that are on, in the order of the file; none when no plugin is.
pub fn launchers_of(
    host: &Arc<moebius_plugin_api::Host>,
    plugins: &[PluginConfig],
) -> Option<Arc<Launcher>> {
    let launchers: Vec<(String, Vec<String>)> = plugins
        .iter()
        .filter(|plugin| plugin.launches && plugin.enabled)
        .map(|plugin| (plugin.name.clone(), plugin.launch_agents.clone()))
        .collect();
    (!launchers.is_empty()).then(|| Arc::new(Launcher::scoped(host.clone(), launchers)))
}

/// Turns the plugin `plugin` (as `plugins`, the running configuration with the change made, has it) off or on while the
/// server runs, and builds the launchers again from `plugins`. Off: no run starts through it from now on, the deciders
/// of its kinds give them back to what decides them without it, and its process is stopped; the manifest it sent is
/// kept first. On: its process is started and the kinds it decides are its again; a launcher is also asked what it left
/// (the runs that exist are not touched), as at the server's start. A run that started already keeps the launcher it
/// started with. `Err` with why when the plugin cannot be started at all or a launcher cannot name its leftovers; then
/// it stays off. One that starts and fails is on and refused.
pub fn set_enabled(
    app: &App,
    config: &Config,
    plugins: &[PluginConfig],
    plugin: &str,
    launchers: &LauncherSlot,
) -> Result<(), String> {
    let Some(entry) = plugins.iter().find(|each| each.name == plugin) else {
        return Ok(());
    };
    let decisions = &app.decisions;
    let host = app.decisions.host_arc();
    if entry.enabled {
        put_in_host(&host, config, entry)?;
        if entry.launches
            && let Err(e) = clean_leftovers(app, &host, plugin)
        {
            host.turn_off(plugin, manifest_of_off(config, entry));
            return Err(e);
        }
        decisions.unroute(plugin);
        for kind in &entry.decides {
            decisions.route(kind, plugin);
        }
        launchers.set(launchers_of(&host, plugins));
    } else {
        // No run is given to it any more before its process ends.
        launchers.set(launchers_of(&host, plugins));
        decisions.unroute(plugin);
        for kind in &entry.decides {
            if let Some(decider) = built_in_decider(kind) {
                decisions.route(kind, decider);
            }
        }
        // What the process sent is all that is known of it from now on, also after a restart.
        if host
            .status(plugin)
            .is_some_and(|s| s.state == State::Running)
            && let Some(sent) = host.manifests().into_iter().find(|m| m.name == plugin)
        {
            remember_manifest(&config.data_dir, &sent);
        }
        if !host.turn_off(plugin, manifest_of_off(config, entry)) {
            host.add_off(manifest_of_off(config, entry));
        }
    }
    Ok(())
}

/// Asks the launcher `plugin` what earlier runs left and removes it, as at the server's start, but with the ids of the
/// runs that exist, whose sandboxes it must keep.
fn clean_leftovers(
    app: &App,
    host: &Arc<moebius_plugin_api::Host>,
    plugin: &str,
) -> Result<(), String> {
    let running: Vec<i64> = app
        .store
        .active_runs()
        .map_err(|e| format!("cannot list the runs that exist: {e}"))?
        .iter()
        .map(|run| run.id)
        .collect();
    Launcher::new(host.clone(), plugin).clean_leftovers(&running)
}

/// Starts `plugin` and waits until it sent its manifest or failed to, at most `limit`. A plugin that fails is still
/// returned: the host refuses calls into it while it is started again.
pub fn start(config: &Config, plugin: &PluginConfig, limit: Duration) -> Result<Process, String> {
    let path = log_path(&config.data_dir, &plugin.name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let file = Mutex::new(file);
    let spec = ProcessSpec {
        name: plugin.name.clone(),
        program: plugin.path.clone(),
        args: plugin.args.clone(),
        dir: config.dir().to_path_buf(),
    };
    Ok(Process::start(spec, limit, move |line| {
        let stamp = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        let mut file = file.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = writeln!(file, "{stamp} {}", redact(line));
    }))
}

/// The `launch_agents` of the launchers that name no agent in `known`, each as "plugin: agent", for a warning at the
/// server's start: the match is by exact name, so a misspelt agent is wrapped by no launcher but the last.
pub fn unknown_launch_agents(
    plugins: &[PluginConfig],
    known: &std::collections::BTreeSet<String>,
) -> Vec<String> {
    plugins
        .iter()
        .flat_map(|plugin| {
            plugin
                .launch_agents
                .iter()
                .filter(|agent| !known.contains(*agent))
                .map(|agent| format!("{}: {agent}", plugin.name))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launcher(name: &str, agents: &[&str]) -> PluginConfig {
        PluginConfig {
            name: name.into(),
            path: "p".into(),
            args: vec![],
            decides: vec![],
            launches: true,
            launch_agents: agents.iter().map(|agent| agent.to_string()).collect(),
            enabled: true,
        }
    }

    #[test]
    fn a_launch_agent_that_is_no_agent_is_named_exactly_as_it_is_written() {
        let known = ["Moebius", "Coder"].map(String::from).into();
        let plugins = [
            launcher("nono", &["Moebius", "moebius"]),
            launcher("podman", &[]),
        ];
        assert_eq!(unknown_launch_agents(&plugins, &known), ["nono: moebius"]);
        assert!(unknown_launch_agents(&plugins[1..], &known).is_empty());
    }
}
