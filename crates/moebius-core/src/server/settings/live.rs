//! The configuration the running server reads, and its edits: saving a change to `moebius.toml`, moving the agents
//! and rules into a new harness configuration repository, applying the live keys to the engine, the router and the
//! agents, and asking for a restart.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::Notify;

use super::keys::{restart_keys, values, without_passwords};
use super::{Changes, Saved, Secrets, edit, invalid, read_raw, write_atomically};
use crate::app::{App, AppError, Result};
use crate::blobs::redact;
use crate::board::Engine;
use crate::bus::Event;
use crate::config::{AgentSet, Config, ConfigError, PluginConfig};
use crate::decide::Actor;
use crate::harness::launch::LauncherSlot;
use crate::harness::repo;
use crate::router::{Router, Upstream};
use crate::server::webhooks::Webhooks;

/// Why a change that leaves no launcher on is refused unless a person confirms it.
const UNWRAP_NEEDS_CONFIRMING: &str = "this change leaves no launcher on, so runs would start unwrapped; confirm that with unwrap_runs";

/// The parts of the running server that take live keys.
struct Targets {
    engine: Arc<Engine>,
    router: Arc<Router>,
    agents: Arc<RwLock<AgentSet>>,
}

/// The configuration the running server reads, and what edits it. The keys that are live (see `keys::KEYS`) are read from
/// here by the schedule and the API, and pushed to the engine, the router and the agents when they change.
pub struct Live {
    /// What the server started with; the keys that need a restart stay as they were here.
    started: Config,
    started_raw: toml::Table,
    current: RwLock<Arc<Config>>,
    secrets: Secrets,
    /// The polls the forges' webhooks wake.
    webhooks: Webhooks,
    /// Held while the file is read, changed and written, so that two edits do not overwrite each other.
    writing: Mutex<()>,
    targets: Option<Targets>,
    /// The launchers runs start through.
    launchers: LauncherSlot,
    restart: Notify,
    restarting: AtomicBool,
}

impl Live {
    pub fn new(config: Config) -> Self {
        let started_raw = read_raw(&config.file)
            .map(|(_, raw)| raw)
            .unwrap_or_default();
        Self {
            secrets: Secrets::new(&config.data_dir),
            webhooks: Webhooks::default(),
            current: RwLock::new(Arc::new(config.clone())),
            started: config,
            started_raw,
            writing: Mutex::new(()),
            targets: None,
            launchers: LauncherSlot::default(),
            restart: Notify::new(),
            restarting: AtomicBool::new(false),
        }
    }

    /// Lets edits of live keys reach the engine, the router and the agents.
    pub fn with_targets(
        mut self,
        engine: Arc<Engine>,
        router: Arc<Router>,
        agents: Arc<RwLock<AgentSet>>,
    ) -> Self {
        self.targets = Some(Targets {
            engine,
            router,
            agents,
        });
        self
    }

    /// Lets turning a launcher plugin off or on build the launchers `launchers` holds again.
    pub fn with_launchers(mut self, launchers: LauncherSlot) -> Self {
        self.launchers = launchers;
        self
    }

    /// The configuration the server runs with now.
    pub fn get(&self) -> Arc<Config> {
        self.current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn secrets(&self) -> &Secrets {
        &self.secrets
    }

    pub fn webhooks(&self) -> &Webhooks {
        &self.webhooks
    }

    /// What `GET /v1/settings` answers: the file and the value of every key in it.
    pub fn view(&self) -> Result<SettingsView> {
        let (_, raw) = read_raw(&self.started.file)?;
        let restart_keys = restart_keys(&self.started_raw, &raw);
        Ok(SettingsView {
            file: self.started.file.display().to_string(),
            values: values(&raw),
            secrets: self.secrets.names()?,
            restart_needed: !restart_keys.is_empty(),
            restart_keys,
        })
    }

    /// The file's text now and as `changes` would leave it, without writing anything: refused as `save` refuses a
    /// change whose result is not a valid configuration. The Retro shows its proposals to change the file this way.
    pub fn preview(&self, changes: &Changes) -> Result<(String, String)> {
        let (text, _) = read_raw(&self.started.file)?;
        let edited = edit(&text, changes)?;
        let base = self.started.file.parent().unwrap_or(Path::new("."));
        Config::parse(&edited, base, &self.started.file)
            .map_err(|e| invalid(redact(&e.to_string())))?;
        Ok((text, edited))
    }

    /// Applies `changes` to the file: refused with the message of the parse when the result is not a valid
    /// configuration, and then nothing is written. Otherwise the file is replaced in one step, the person's decision
    /// is recorded and the live keys are applied.
    pub fn save(&self, app: &App, by: &Actor, changes: &Changes) -> Result<Saved> {
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        self.write_changes(app, by, changes, ("edit_settings", false), || {
            Ok(BTreeMap::from([
                (
                    "set".to_string(),
                    without_passwords(&serde_json::to_value(&changes.set).unwrap_or_default())
                        .to_string(),
                ),
                ("remove".to_string(), changes.remove.join(", ")),
            ]))
        })
    }

    /// The one path every edit of the file takes, with `writing` held: `changes` are applied to the text and the
    /// result validated, `prepare` runs (it may refuse, and then nothing is written; what it returns is added to the
    /// context of the recorded action), the file is replaced in one step, `action` is recorded and the live keys are
    /// applied.
    fn write_changes(
        &self,
        app: &App,
        by: &Actor,
        changes: &Changes,
        (action, unwrap_runs): (&str, bool),
        prepare: impl FnOnce() -> Result<BTreeMap<String, String>>,
    ) -> Result<Saved> {
        let (text, _) = read_raw(&self.started.file)?;
        let edited = edit(&text, changes)?;
        let base = self.started.file.parent().unwrap_or(Path::new("."));
        let config = Config::parse(&edited, base, &self.started.file)
            .map_err(|e| invalid(redact(&e.to_string())))?;
        let unwrap_runs = unwrap_runs || changes.unwrap_runs;
        let upstreams = self.secrets.upstreams(&config).map_err(invalid)?;
        if let Some(model) = &config.decision_model {
            self.secrets
                .resolve(
                    model.api_key_env.as_deref(),
                    model.api_key_secret.as_deref(),
                )
                .map_err(invalid)?;
        }
        crate::server::check_forges(&config, &self.secrets).map_err(invalid)?;
        if !matches!(by, Actor::Person) && !turned(&self.get().plugins, &config.plugins).is_empty()
        {
            return Err(AppError::Forbidden(
                "only a person turns a plugin off or on".into(),
            ));
        }
        // A file that a hand edit left with no launcher on does not refuse every later change: the change is saved and
        // the launcher stays on until a person confirms (see `turn_plugins`).
        if !unwrap_runs && self.unwraps(&config) && !self.file_unwraps(&text, base) {
            return Err(AppError::Conflict(UNWRAP_NEEDS_CONFIRMING.into()));
        }
        let mut context = prepare()?;
        write_atomically(&self.started.file, edited.as_bytes(), false)?;
        let turned = match self.turn_plugins(app, &config, unwrap_runs) {
            Ok(turned) => turned,
            Err(e) => {
                // A plugin that cannot be turned on leaves the file as it was.
                let _ = write_atomically(&self.started.file, text.as_bytes(), false);
                return Err(invalid(redact(&e)));
            }
        };
        context.insert("file".to_string(), self.started.file.display().to_string());
        app.record_action(by, action, None, None, context, "")?;
        self.apply(app, config, upstreams, turned);
        let raw = toml::from_str(&edited).unwrap_or_default();
        let restart_keys = restart_keys(&self.started_raw, &raw);
        Ok(Saved {
            restart_needed: !restart_keys.is_empty(),
            restart_keys,
        })
    }

    /// A person moves the agents and rules of `moebius.toml` into a new harness configuration repository: `harness/`
    /// next to the file (see `repo::create`). The file then gets `[harness] url` naming it and loses
    /// `[agents.*]` and `rules_file`, with its other keys and comments kept; the result must parse. Refused with a
    /// conflict when the file has `[harness]` already, or the directory is there and not empty. Whatever fails before
    /// the file is written leaves the file and the directory as they were. It is saved as `save` saves, so the live keys
    /// are applied; the server uses the repository after it restarts.
    pub fn set_up_harness(&self, app: &App, by: &Actor) -> Result<Saved> {
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        let file = &self.started.file;
        let (text, raw) = read_raw(file)?;
        if raw.contains_key("harness") || self.started.harness_url.is_some() {
            return Err(AppError::Conflict(
                "the server has a harness configuration repository already".into(),
            ));
        }
        let base = file.parent().unwrap_or(Path::new("."));
        let config =
            Config::parse(&text, base, file).map_err(|e| invalid(redact(&e.to_string())))?;
        let dir = base.join("harness");
        let url = dir.display().to_string();
        let changes = Changes {
            set: BTreeMap::from([("harness.url".to_string(), Value::String(url.clone()))]),
            remove: vec!["agents".into(), "rules_file".into()],
            ..Changes::default()
        };
        self.write_changes(app, by, &changes, ("setup_harness", false), || {
            let moved = repo::create(&dir, &config.agents, config.rules_file.as_deref())?;
            let moved = moved
                .iter()
                .map(|(from, to)| format!("{} -> {to}", from.display()))
                .collect::<Vec<_>>();
            Ok(BTreeMap::from([
                ("repository".to_string(), url.clone()),
                (
                    "agents".to_string(),
                    config.agents.keys().cloned().collect::<Vec<_>>().join(", "),
                ),
                ("copied".to_string(), moved.join("\n")),
            ]))
        })
    }

    /// Takes the live keys of `new` into the running configuration and gives them to the parts that use them.
    fn apply(&self, app: &App, new: Config, upstreams: Vec<Upstream>, turned: Turned) {
        let mut running = (*self.get()).clone();
        running.run_limit = new.run_limit;
        running.round_limit = new.round_limit;
        running.day_tokens = new.day_tokens;
        running.retro_at = new.retro_at;
        running.trial_window = new.trial_window;
        running.regression_size = new.regression_size;
        running.regression_gate = new.regression_gate;
        running.board_tick = new.board_tick;
        running.upstreams = new.upstreams.clone();
        running.classes = new.classes.clone();
        running.model_thinking = new.model_thinking.clone();
        // The agents of a server with a harness configuration repository come from the repository.
        let own_agents = self.started.harness_url.is_none() && new.harness_url.is_none();
        if own_agents {
            running.agents = new.agents.clone();
        }
        running.plugins = turned.plugins;
        if let Some(targets) = &self.targets {
            targets
                .engine
                .set_limits(new.run_limit, new.round_limit, new.day_tokens);
            targets.router.set_day_budget(new.day_tokens);
            targets.router.set_upstreams(upstreams);
            targets.router.set_classes(new.classes.clone());
            targets
                .router
                .set_model_thinking(new.model_thinking.clone());
            if own_agents {
                targets
                    .agents
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .agents = new.agents;
            }
        }
        *self.current.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(running);
        for event in turned.events {
            app.bus.publish(event);
        }
    }

    /// Turns the plugins off or on that `new` has otherwise than the running configuration: stops or starts them,
    /// gives their kinds back or takes them, and builds the launchers again. The plugins in the answer are those of
    /// `new` that run or that the plugin host holds (those a package installed since the start), as `new` has them,
    /// except that a plugin whose change was not applied has the state it runs in.
    ///
    /// Not applied is a change that turns off the launchers that are on, when `confirmed` is false: a hand edit of the
    /// file that nobody confirmed. The launchers stay on, the server logs why and the event `plugin_enable_refused`
    /// says so; a person confirms it with `unwrap_runs`. `Err` with why when a plugin cannot be turned on; the ones
    /// turned before it are put back then, and nothing is published.
    fn turn_plugins(
        &self,
        app: &App,
        new: &Config,
        confirmed: bool,
    ) -> std::result::Result<Turned, String> {
        let running = self.get();
        let before = known_plugins(app, &running.plugins, &new.plugins);
        let mut wanted = turned(&before, &new.plugins);
        // The plugins as they run now; each takes its new state as it is turned.
        let mut plugins: Vec<PluginConfig> = before.clone();
        for entry in &mut plugins {
            if let Some(entry_new) = new.plugins.iter().find(|n| n.name == entry.name) {
                *entry = PluginConfig {
                    enabled: entry.enabled,
                    ..entry_new.clone()
                };
            }
        }
        let mut events = Vec::new();
        let on = |plugins: &[PluginConfig]| plugins.iter().any(|p| p.launches && p.enabled);
        let mut after = before.clone();
        for (name, enabled) in &wanted {
            if let Some(each) = after.iter_mut().find(|each| &each.name == name) {
                each.enabled = *enabled;
            }
        }
        if !confirmed && on(&before) && !on(&after) {
            wanted.retain(|(name, enabled)| {
                let launcher = before.iter().any(|p| &p.name == name && p.launches);
                if *enabled || !launcher {
                    return true;
                }
                let message = format!(
                    "{name} stays on: moebius.toml turns it off, which leaves no launcher on, so runs would start unwrapped; \
                     confirm that with unwrap_runs (PUT /v1/plugins/{name}/enabled)"
                );
                eprintln!("moebius: {message}");
                events.push(Event::PluginEnableRefused {
                    name: name.clone(),
                    enabled: false,
                    message,
                });
                false
            });
        }
        let mut done: Vec<(String, bool)> = Vec::new();
        for (name, enabled) in wanted {
            if let Some(each) = plugins.iter_mut().find(|each| each.name == name) {
                each.enabled = enabled;
            }
            match crate::server::plugins::set_enabled(
                app,
                &running,
                &plugins,
                &name,
                &self.launchers,
            ) {
                Ok(()) => {
                    events.push(Event::PluginEnabledChanged {
                        name: name.clone(),
                        enabled,
                    });
                    done.push((name, enabled));
                }
                Err(e) => {
                    if let Some(each) = plugins.iter_mut().find(|each| each.name == name) {
                        each.enabled = !enabled;
                    }
                    for (back, was) in done.into_iter().rev() {
                        if let Some(each) = plugins.iter_mut().find(|each| each.name == back) {
                            each.enabled = !was;
                        }
                        let _ = crate::server::plugins::set_enabled(
                            app,
                            &running,
                            &plugins,
                            &back,
                            &self.launchers,
                        );
                    }
                    let verb = if enabled { "on" } else { "off" };
                    return Err(format!("cannot turn {name} {verb}: {e}"));
                }
            }
        }
        Ok(Turned { plugins, events })
    }

    /// The plugins as they run, with the states `new` gives those that run; a plugin that `new` adds is not among them
    /// until the server restarts.
    fn plugins_after(&self, new: &Config) -> Vec<PluginConfig> {
        self.get()
            .plugins
            .iter()
            .map(|each| PluginConfig {
                enabled: new
                    .plugins
                    .iter()
                    .find(|n| n.name == each.name)
                    .map_or(each.enabled, |n| n.enabled),
                ..each.clone()
            })
            .collect()
    }

    /// Whether `new` leaves no launcher on where one is: runs would start unwrapped.
    fn unwraps(&self, new: &Config) -> bool {
        let on = |plugins: &[PluginConfig]| plugins.iter().any(|p| p.launches && p.enabled);
        on(&self.get().plugins) && !on(&self.plugins_after(new))
    }

    /// Whether the file as it is, the text `text`, leaves no launcher on where one is: a hand edit did that.
    fn file_unwraps(&self, text: &str, base: &Path) -> bool {
        Config::parse(text, base, &self.started.file).is_ok_and(|config| self.unwraps(&config))
    }

    /// A person turns the plugin `plugin` off or on: its `enabled` is saved in `moebius.toml` (the entry and the settings
    /// stay), and the running server stops or starts it at once, takes its kinds and builds the launchers again. A
    /// run that started keeps the launcher it started with. Refused when the launchers left would wrap some agents
    /// and not others (naming them), and, without `unwrap_runs`, when no launcher would be left, so that runs would
    /// start unwrapped.
    pub fn set_plugin_enabled(
        &self,
        app: &App,
        by: &Actor,
        plugin: &str,
        enabled: bool,
        unwrap_runs: bool,
    ) -> Result<PluginEnabled> {
        if !matches!(by, Actor::Person) {
            return Err(AppError::Forbidden(
                "only a person turns a plugin off or on".into(),
            ));
        }
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        let file = &self.started.file;
        let (text, raw) = read_raw(file)?;
        let entries = raw.get("plugins").and_then(|plugins| plugins.as_array());
        let found = entries
            .into_iter()
            .flatten()
            .enumerate()
            .find(|(_, each)| each.get("name").and_then(|name| name.as_str()) == Some(plugin));
        let Some((index, entry)) = found else {
            return Err(AppError::NotFound(format!("a plugin {plugin}")));
        };
        let in_file = entry
            .get("enabled")
            .and_then(|enabled| enabled.as_bool())
            .unwrap_or(true);
        // What runs counts, not the file alone: a hand edit that the server has not applied (it refused to leave no
        // launcher on) can still be applied or confirmed here, and the file brought in step with what runs.
        let host = app.decisions.host();
        let was = host.holds(plugin).then(|| !host.is_off(plugin));
        let answer = |changed, unwrapped_runs| PluginEnabled {
            name: plugin.to_string(),
            enabled,
            changed,
            unwrapped_runs,
            note: unwrapped_runs.then(|| {
                format!("runs now start unwrapped: no launcher is on after {plugin} was turned off")
            }),
        };
        if was.unwrap_or(in_file) == enabled && in_file == enabled {
            return Ok(answer(false, false));
        }
        let key = format!("plugins.{index}.enabled");
        let changes = if enabled {
            Changes {
                remove: vec![key],
                ..Changes::default()
            }
        } else {
            Changes {
                set: BTreeMap::from([(key, Value::Bool(false))]),
                ..Changes::default()
            }
        };
        let edited = edit(&text, &changes)?;
        let base = file.parent().unwrap_or(Path::new("."));
        let after = Config::parse(&edited, base, file).map_err(|e| match e {
            ConfigError::LauncherLeavesAgents { .. } => {
                invalid(self.leaves_agents(plugin, enabled, &edited))
            }
            other => invalid(redact(&other.to_string())),
        })?;
        let unwrapped = was != Some(enabled) && {
            let before = known_plugins(app, &self.get().plugins, &after.plugins);
            let on = |plugins: &[PluginConfig]| plugins.iter().any(|p| p.launches && p.enabled);
            let mut then = before.clone();
            for each in then.iter_mut().filter(|each| each.name == plugin) {
                each.enabled = enabled;
            }
            on(&before) && !on(&then)
        };
        if unwrapped && !unwrap_runs {
            return Err(AppError::Conflict(UNWRAP_NEEDS_CONFIRMING.into()));
        }
        self.write_changes(
            app,
            by,
            &changes,
            (
                if enabled {
                    "enable_plugin"
                } else {
                    "disable_plugin"
                },
                unwrap_runs,
            ),
            || {
                Ok(BTreeMap::from([
                    ("plugin".to_string(), plugin.to_string()),
                    ("unwrapped_runs".to_string(), unwrapped.to_string()),
                ]))
            },
        )?;
        if unwrapped {
            eprintln!("moebius: {}", answer(true, true).note.unwrap_or_default());
        }
        Ok(answer(true, unwrapped))
    }

    /// Why turning `plugin` off or on leaves the launchers wrapping some agents and not others, naming them; `edited`
    /// is the file as it would be.
    fn leaves_agents(&self, plugin: &str, enabled: bool, edited: &str) -> String {
        let verb = if enabled { "on" } else { "off" };
        let table: toml::Table = toml::from_str(edited).unwrap_or_default();
        let named: std::collections::BTreeSet<String> = table
            .get("plugins")
            .and_then(|plugins| plugins.as_array())
            .into_iter()
            .flatten()
            .filter(|each| {
                each.get("launches").and_then(|v| v.as_bool()) == Some(true)
                    && each.get("enabled").and_then(|v| v.as_bool()) != Some(false)
            })
            .filter_map(|each| each.get("launch_agents").and_then(|v| v.as_array()))
            .flatten()
            .filter_map(|agent| agent.as_str().map(str::to_string))
            .collect();
        let known: std::collections::BTreeSet<String> = match &self.targets {
            Some(targets) => targets
                .agents
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .agents
                .keys()
                .cloned()
                .collect(),
            None => self.get().agents.keys().cloned().collect(),
        };
        let bare: Vec<String> = known.difference(&named).cloned().collect();
        let bare = if bare.is_empty() {
            "every other agent".to_string()
        } else {
            bare.join(", ")
        };
        let wrapped = named.iter().cloned().collect::<Vec<_>>().join(", ");
        format!(
            "{plugin} stays {} because turning it {verb} would wrap the runs of {wrapped} and leave those of {bare} unwrapped; \
             turn off the launchers for single agents too, or let the last launcher take every agent (no launch_agents)",
            if enabled { "off" } else { "on" },
        )
    }

    /// Asks the server to restart: `Server::serve` returns, and the `moebius` binary starts the same program again.
    pub fn request_restart(&self) {
        self.restarting.store(true, Ordering::SeqCst);
        self.restart.notify_waiters();
    }

    /// Whether a restart was asked for.
    pub fn restart_requested(&self) -> bool {
        self.restarting.load(Ordering::SeqCst)
    }

    /// Completes once a restart is asked for.
    pub async fn restart_asked(&self) {
        loop {
            let asked = self.restart.notified();
            if self.restart_requested() {
                return;
            }
            asked.await;
        }
    }
}

/// The plugins of `new` that run, as they run now: those of `running`, and those a package installed since the start,
/// which are in the plugin host, on unless turned off since.
fn known_plugins(app: &App, running: &[PluginConfig], new: &[PluginConfig]) -> Vec<PluginConfig> {
    let host = app.decisions.host();
    new.iter()
        .filter_map(
            |entry| match running.iter().find(|each| each.name == entry.name) {
                Some(each) => Some(each.clone()),
                None => host.holds(&entry.name).then(|| PluginConfig {
                    enabled: !host.is_off(&entry.name),
                    ..entry.clone()
                }),
            },
        )
        .collect()
}

/// The plugins of `running`, by name, whose `enabled` in `new` is not what it is, with the state `new` wants.
fn turned(running: &[PluginConfig], new: &[PluginConfig]) -> Vec<(String, bool)> {
    running
        .iter()
        .filter_map(|each| {
            let wanted = new.iter().find(|n| n.name == each.name)?.enabled;
            (wanted != each.enabled).then(|| (each.name.clone(), wanted))
        })
        .collect()
}

/// What turning the plugins that a change turned off or on left: the plugins as they run, and the events to publish.
struct Turned {
    plugins: Vec<PluginConfig>,
    events: Vec<Event>,
}

/// What turning a plugin off or on answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginEnabled {
    pub name: String,
    /// Whether the plugin is on now.
    pub enabled: bool,
    /// False when it was in that state already and nothing was written.
    pub changed: bool,
    /// Whether runs start unwrapped from now on: the plugin was the last launcher that was on.
    pub unwrapped_runs: bool,
    /// Says so in words when `unwrapped_runs`.
    pub note: Option<String>,
}

/// The answer of `GET /v1/settings`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingsView {
    /// The configuration file, an absolute path.
    pub file: String,
    /// Every key of the reference: `{"value": ..., "default": bool}`, the default being used when the file does not
    /// set the key.
    pub values: BTreeMap<String, Value>,
    /// The names of the stored API keys, never the keys.
    pub secrets: Vec<String>,
    /// Whether the file has keys the running server uses only after it restarts, and which.
    pub restart_needed: bool,
    pub restart_keys: Vec<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::server::Server;

    fn actor() -> Actor {
        Actor::Person
    }

    /// A change to the live key `name`, and what the running server shows of it.
    fn sample(name: &str) -> (Value, fn(&Server) -> String) {
        fn config(server: &Server, f: fn(&Config) -> String) -> String {
            f(&server.live.get())
        }
        macro_rules! field {
            ($($field:tt)+) => {
                |s| config(s, |c| format!("{:?}", c.$($field)+))
            };
        }
        match name {
            "run_limit" => (json!(7), field!(run_limit)),
            "retro_at" => (json!("04:15"), field!(retro_at)),
            "trial_window.start" => (json!("01:10"), field!(trial_window.0)),
            "trial_window.end" => (json!("05:20"), field!(trial_window.1)),
            "regression.size" => (json!(11), field!(regression_size)),
            "regression.gate" => (json!(false), field!(regression_gate)),
            "board.tick_ms" => (json!(123), field!(board_tick)),
            "board.round_limit" => (json!(8), field!(round_limit)),
            "budgets.day_tokens" => (json!(4321), field!(day_tokens)),
            "router.upstreams" => (
                json!([{"protocol": "openai", "base_url": "http://127.0.0.1:1"}]),
                field!(upstreams),
            ),
            "router.classes" => (json!({"fast": ["a", "b"]}), |s| {
                format!("{:?} {:?}", s.live.get().classes, s.router.classes())
            }),
            "router.model_thinking" => (json!({"qwen-local": {"high": "xhigh"}}), |s| {
                format!(
                    "{:?} {:?}",
                    s.live.get().model_thinking,
                    s.router.thinking_map("qwen-local")
                )
            }),
            "agents" => (json!({"Coder": {"model": "live-model"}}), |s| {
                format!("{:?}", s.harness.agents.read().unwrap().agents)
            }),
            other => panic!(
                "the live key {other} has no sample: add one, and make `Live::apply` apply it"
            ),
        }
    }

    #[test]
    fn every_live_key_is_applied_to_the_running_server_by_save() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("moebius.toml");
        std::fs::write(&file, "[router]\nlisten = \"127.0.0.1:0\"\n").unwrap();
        let server =
            Server::new(Config::load(&file).unwrap(), "127.0.0.1:1".parse().unwrap()).unwrap();
        let live_keys = super::super::keys::live_keys();
        assert!(live_keys.len() > 10);
        for name in live_keys {
            let (value, seen) = sample(name);
            let before = seen(&server);
            let changes = Changes {
                set: BTreeMap::from([(name.to_string(), value)]),
                ..Changes::default()
            };
            let saved = server.live.save(&server.app, &actor(), &changes).unwrap();
            assert!(!saved.restart_needed, "{name} needs a restart");
            assert_ne!(
                before,
                seen(&server),
                "{name} is marked live but not applied"
            );
        }
    }

    #[test]
    fn a_change_that_serve_would_refuse_for_a_forge_is_refused_and_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("moebius.toml");
        let original = "[router]\nlisten = \"127.0.0.1:0\"\n";
        std::fs::write(&file, original).unwrap();
        let server =
            Server::new(Config::load(&file).unwrap(), "127.0.0.1:1".parse().unwrap()).unwrap();
        let attempt = |sets: &[(&str, Value)]| {
            let changes = Changes {
                set: sets
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
                ..Changes::default()
            };
            server.live.save(&server.app, &actor(), &changes)
        };
        std::fs::write(dir.path().join("not-a-key.pem"), "this is not a key").unwrap();
        let refused = [
            vec![
                ("github.app_id", json!(1)),
                ("github.private_key_file", json!("not-a-key.pem")),
            ],
            vec![
                ("github.app_id", json!(1)),
                ("github.private_key_file", json!("missing.pem")),
            ],
            vec![("github.token_file", json!("missing-token"))],
            vec![("github.token_secret", json!("github"))],
            vec![("gitlab.token_file", json!("missing-token"))],
            vec![("gitlab.token_secret", json!("gitlab"))],
            vec![("forgejo.token_file", json!("missing-token"))],
            vec![("forgejo.token_secret", json!("codeberg"))],
        ];
        for sets in &refused {
            let error = attempt(sets);
            assert!(matches!(error, Err(AppError::Invalid(_))), "{sets:?}");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
        }
        std::fs::write(dir.path().join("github-token"), "ghp_token\n").unwrap();
        attempt(&[("github.token_file", json!("github-token"))]).unwrap();
        std::fs::write(dir.path().join("gitlab-token"), "glpat\n").unwrap();
        attempt(&[("gitlab.token_file", json!("gitlab-token"))]).unwrap();
    }
}
