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
use crate::config::{AgentSet, Config};
use crate::decide::Actor;
use crate::harness::repo;
use crate::router::{Router, Upstream};

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
    /// Held while the file is read, changed and written, so that two edits do not overwrite each other.
    writing: Mutex<()>,
    targets: Option<Targets>,
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
            current: RwLock::new(Arc::new(config.clone())),
            started: config,
            started_raw,
            writing: Mutex::new(()),
            targets: None,
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
        let (text, _) = read_raw(&self.started.file)?;
        let edited = edit(&text, changes)?;
        let base = self.started.file.parent().unwrap_or(Path::new("."));
        let config = Config::parse(&edited, base, &self.started.file)
            .map_err(|e| invalid(redact(&e.to_string())))?;
        let upstreams = self.secrets.upstreams(&config).map_err(invalid)?;
        if let Some(model) = &config.decision_model {
            self.secrets
                .resolve(
                    model.api_key_env.as_deref(),
                    model.api_key_secret.as_deref(),
                )
                .map_err(invalid)?;
        }
        write_atomically(&self.started.file, edited.as_bytes(), false)?;
        let context = BTreeMap::from([
            ("file".to_string(), self.started.file.display().to_string()),
            (
                "set".to_string(),
                without_passwords(&serde_json::to_value(&changes.set).unwrap_or_default())
                    .to_string(),
            ),
            ("remove".to_string(), changes.remove.join(", ")),
        ]);
        app.record_action(by, "edit_settings", None, None, context, "")?;
        self.apply(config, upstreams);
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
    /// the file is written leaves the file and the directory as they were. The running server is not changed: it uses
    /// the repository after it restarts.
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
        };
        let edited = edit(&text, &changes)?;
        Config::parse(&edited, base, file).map_err(|e| invalid(redact(&e.to_string())))?;
        let existed = dir.exists();
        let moved = repo::create(&dir, &config.agents, config.rules_file.as_deref())?;
        if let Err(e) = write_atomically(file, edited.as_bytes(), false) {
            let _ = std::fs::remove_dir_all(&dir);
            if existed {
                let _ = std::fs::create_dir(&dir);
            }
            return Err(e.into());
        }
        let moved = moved
            .iter()
            .map(|(from, to)| format!("{} -> {to}", from.display()))
            .collect::<Vec<_>>();
        let context = BTreeMap::from([
            ("file".to_string(), file.display().to_string()),
            ("repository".to_string(), url),
            (
                "agents".to_string(),
                config.agents.keys().cloned().collect::<Vec<_>>().join(", "),
            ),
            ("copied".to_string(), moved.join("\n")),
        ]);
        app.record_action(by, "setup_harness", None, None, context, "")?;
        let raw = toml::from_str(&edited).unwrap_or_default();
        Ok(Saved {
            restart_needed: true,
            restart_keys: restart_keys(&self.started_raw, &raw),
        })
    }

    /// Takes the live keys of `new` into the running configuration and gives them to the parts that use them.
    fn apply(&self, new: Config, upstreams: Vec<Upstream>) {
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
        // The agents of a server with a harness configuration repository come from the repository.
        let own_agents = self.started.harness_url.is_none() && new.harness_url.is_none();
        if own_agents {
            running.agents = new.agents.clone();
        }
        if let Some(targets) = &self.targets {
            targets
                .engine
                .set_limits(new.run_limit, new.round_limit, new.day_tokens);
            targets.router.set_day_budget(new.day_tokens);
            targets.router.set_upstreams(upstreams);
            targets.router.set_classes(new.classes.clone());
            if own_agents {
                targets
                    .agents
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .agents = new.agents;
            }
        }
        *self.current.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(running);
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
