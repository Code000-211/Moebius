//! Editing the server's configuration while it runs: the desktop app changes `moebius.toml` through
//! `PUT /v1/settings` and keeps API keys it is given in `<data_dir>/secrets.toml`.
//!
//! A change is applied to the text of the file with `toml_edit`, so comments and order stay; the result must pass the
//! same `Config::parse` the server uses at start, else nothing is written. [`Live`] is the configuration the running
//! server reads: the keys marked live in [`KEYS`] take effect at once (the board engine, the router, the schedule
//! and the agents read them from it), every other key is written and used after a restart, which the answer says.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Notify;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use crate::app::{App, AppError, Result};
use crate::blobs::redact;
use crate::board::Engine;
use crate::config::{AgentSet, Config, valid_agent_name};
use crate::decide::Question;
use crate::router::{Router, Upstream};
use crate::setup::without_credentials;

/// A key of the configuration as the reference names it, with its default (JSON text, empty for none) and whether a
/// change applies to the running server.
struct Key {
    name: &'static str,
    default: &'static str,
    live: bool,
}

const fn key(name: &'static str, default: &'static str, live: bool) -> Key {
    Key {
        name,
        default,
        live,
    }
}

/// Every key of `docs/reference/configuration.md`, in its order. `router.upstreams` and `agents` stand for the whole
/// array of tables and the whole table of agents.
const KEYS: &[Key] = &[
    key("data_dir", "\".moebius\"", false),
    key("listen", "\"127.0.0.1:7171\"", false),
    key("token_file", "\"token\"", false),
    key("run_limit", "3", true),
    key("retro_at", "\"23:30\"", true),
    key("rules_file", "", false),
    key("harness.mcp_env", "[]", false),
    key("harness.url", "", false),
    key("trial_window.start", "\"00:00\"", true),
    key("trial_window.end", "\"06:00\"", true),
    key("board.tick_ms", "2000", true),
    key("board.round_limit", "5", true),
    key("budgets.day_tokens", "", true),
    key("checks.timeout_seconds", "1800", false),
    key("router.listen", "\"127.0.0.1:7172\"", false),
    key("router.upstreams", "[]", true),
    key("pi_bolt.program", "\"pi-bolt\"", false),
    key("teacher.model", "", false),
    key("teacher.protocol", "\"openai\"", false),
    key("teacher.timeout_seconds", "20", false),
    key("decision_model.url", "", false),
    key("decision_model.model", "", false),
    key("decision_model.api_key_env", "", false),
    key("decision_model.api_key_secret", "", false),
    key("decision_model.timeout_seconds", "20", false),
    key("decision_model.shadow", "", false),
    key("decision_model.agreement", "0.95", false),
    key("decision_model.min_judgements", "100", false),
    key("github.app_id", "", false),
    key("github.private_key_file", "", false),
    key("github.api_url", "\"https://api.github.com\"", false),
    key("github.poll_seconds", "30", false),
    key("agents", "{}", true),
];

/// The keys whose value is, or holds, a URL that may have a user name and password in it.
const URLS: &[&str] = &["harness.url", "decision_model.url", "router.upstreams"];

/// What `PUT /v1/settings` takes: values to set and keys or tables to remove, by their dotted names.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Changes {
    /// A dotted key and its new value. A table is a JSON object, an array of tables a JSON array of objects.
    #[serde(default)]
    pub set: BTreeMap<String, Value>,
    /// Dotted keys or tables to remove; one that is not in the file is ignored.
    #[serde(default)]
    pub remove: Vec<String>,
}

/// What a saved change tells the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Saved {
    /// Whether some key changed that the running server only uses after it restarts.
    pub restart_needed: bool,
    /// Those keys, by their dotted names.
    pub restart_keys: Vec<String>,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

/// The parts of a dotted key, which are made of letters, digits, `-` and `_`.
fn parts(key: &str) -> Result<Vec<&str>> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.iter().all(|part| valid_agent_name(part)) {
        Ok(parts)
    } else {
        Err(invalid(format!(
            "{key:?} is not a key of the configuration"
        )))
    }
}

fn index(key: &str, part: &str) -> Result<usize> {
    part.parse()
        .map_err(|_| invalid(format!("{key}: {part:?} is not a position in the list")))
}

fn value_from_json(key: &str, value: &Value) -> Result<toml_edit::Value> {
    Ok(match value {
        Value::Bool(b) => (*b).into(),
        Value::String(s) => s.as_str().into(),
        Value::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => i.into(),
            (None, Some(f)) if n.is_f64() => f.into(),
            _ => return Err(invalid(format!("{key}: {n} is not a number TOML holds"))),
        },
        Value::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(value_from_json(key, item)?);
            }
            array.into()
        }
        Value::Null => {
            return Err(invalid(format!(
                "{key}: null is no value; remove the key instead"
            )));
        }
        Value::Object(_) => return Err(invalid(format!("{key}: a table is not allowed here"))),
    })
}

fn table_from_json(key: &str, fields: &serde_json::Map<String, Value>) -> Result<Table> {
    let mut table = Table::new();
    for (name, value) in fields {
        if !valid_agent_name(name) {
            return Err(invalid(format!("{key}: {name:?} is not a key")));
        }
        table.insert(name, item_from_json(&format!("{key}.{name}"), value)?);
    }
    Ok(table)
}

/// A value, a table or an array of tables for `value`.
fn item_from_json(key: &str, value: &Value) -> Result<Item> {
    match value {
        Value::Object(fields) => Ok(Item::Table(table_from_json(key, fields)?)),
        Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object) => {
            let mut tables = ArrayOfTables::new();
            for item in items {
                if let Value::Object(fields) = item {
                    tables.push(table_from_json(key, fields)?);
                }
            }
            Ok(Item::ArrayOfTables(tables))
        }
        other => Ok(Item::Value(value_from_json(key, other)?)),
    }
}

/// An empty table that shows no header of its own until it holds a key.
fn implicit() -> Table {
    let mut table = Table::new();
    table.set_implicit(true);
    table
}

/// Sets `name` of `table` to `new`, keeping the comments around what it replaces.
fn put(table: &mut Table, name: &str, new: Item) {
    match table.get_mut(name) {
        Some(old) => {
            let mut new = new;
            match (&*old, &mut new) {
                (Item::Value(before), Item::Value(after)) => {
                    *after.decor_mut() = before.decor().clone();
                }
                (Item::Table(before), Item::Table(after)) => {
                    *after.decor_mut() = before.decor().clone();
                    after.set_position(before.position());
                }
                _ => {}
            }
            *old = new;
        }
        None => {
            table.insert(name, new);
        }
    }
}

fn set_in(table: &mut Table, key: &str, path: &[&str], new: Item) -> Result<()> {
    let Some((first, rest)) = path.split_first() else {
        return Err(invalid("a key is needed"));
    };
    if rest.is_empty() {
        put(table, first, new);
        return Ok(());
    }
    if !table.contains_key(first) {
        // A position in the list of tables, such as `router.upstreams.0`, makes the list.
        let made = if index(key, rest[0]).is_ok() {
            Item::ArrayOfTables(ArrayOfTables::new())
        } else {
            Item::Table(implicit())
        };
        table.insert(first, made);
    }
    match table.get_mut(first) {
        Some(Item::Table(child)) => set_in(child, key, rest, new),
        Some(Item::ArrayOfTables(list)) => {
            let at = index(key, rest[0])?;
            if rest.len() > 1 {
                let child = list
                    .get_mut(at)
                    .ok_or_else(|| invalid(format!("{key}: there is no entry {at}")))?;
                return set_in(child, key, &rest[1..], new);
            }
            let Item::Table(entry) = new else {
                return Err(invalid(format!("{key}: an entry is a table")));
            };
            match at.cmp(&list.len()) {
                std::cmp::Ordering::Less => {
                    if let Some(old) = list.get_mut(at) {
                        *old = entry;
                    }
                }
                std::cmp::Ordering::Equal => list.push(entry),
                std::cmp::Ordering::Greater => {
                    return Err(invalid(format!(
                        "{key}: the list has {} entries, so the next is {}",
                        list.len(),
                        list.len()
                    )));
                }
            }
            Ok(())
        }
        _ => Err(invalid(format!("{key}: {first} is not a table"))),
    }
}

fn remove_in(table: &mut Table, key: &str, path: &[&str]) -> Result<()> {
    let Some((first, rest)) = path.split_first() else {
        return Ok(());
    };
    if rest.is_empty() {
        table.remove(first);
        return Ok(());
    }
    match table.get_mut(first) {
        Some(Item::Table(child)) => remove_in(child, key, rest),
        Some(Item::ArrayOfTables(list)) => {
            let at = index(key, rest[0])?;
            if at >= list.len() {
                return Ok(());
            }
            if rest.len() > 1 {
                if let Some(child) = list.get_mut(at) {
                    remove_in(child, key, &rest[1..])?;
                }
            } else {
                list.remove(at);
            }
            if list.is_empty() {
                table.remove(first);
            }
            Ok(())
        }
        Some(_) => Err(invalid(format!("{key}: {first} is not a table"))),
        None => Ok(()),
    }
}

/// Orders dotted keys so that a position in a list counts as a number: `router.upstreams.2` before
/// `router.upstreams.10`, which is what adding several entries in one change needs.
fn by_position(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (a.split('.'), b.split('.'));
    loop {
        match (a.next(), b.next()) {
            (Some(x), Some(y)) => {
                let order = match (x.parse::<usize>(), y.parse::<usize>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    _ => x.cmp(y),
                };
                if order.is_ne() {
                    return order;
                }
            }
            (x, y) => return x.is_some().cmp(&y.is_some()),
        }
    }
}

/// `text` with `changes` made to it. Comments and order stay; the removals come first, so that a table can be
/// replaced by removing and setting it.
pub fn edit(text: &str, changes: &Changes) -> Result<String> {
    let mut document: DocumentMut = text
        .parse()
        .map_err(|e| invalid(format!("moebius.toml cannot be edited: {e}")))?;
    for key in &changes.remove {
        remove_in(document.as_table_mut(), key, &parts(key)?)?;
    }
    let mut set: Vec<(&String, &Value)> = changes.set.iter().collect();
    set.sort_by(|(a, _), (b, _)| by_position(a, b));
    for (key, value) in set {
        let item = item_from_json(key, value)?;
        set_in(document.as_table_mut(), key, &parts(key)?, item)?;
    }
    Ok(document.to_string())
}

/// The value at the dotted `name` of the parsed file.
fn lookup<'a>(raw: &'a toml::Table, name: &str) -> Option<&'a toml::Value> {
    let (first, rest) = name.split_once('.').unwrap_or((name, ""));
    let value = raw.get(first)?;
    if rest.is_empty() {
        return Some(value);
    }
    lookup(value.as_table()?, rest)
}

fn default_of(key: &Key) -> Value {
    if key.default.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(key.default).unwrap_or(Value::Null)
    }
}

/// The value of `key` in the file, or its default.
fn effective(raw: &toml::Table, key: &Key) -> Value {
    match lookup(raw, key.name).map(serde_json::to_value) {
        Some(Ok(value)) => value,
        _ => default_of(key),
    }
}

/// The keys the running server uses only after a restart whose value in `raw` is not the one it started with.
fn restart_keys(started: &toml::Table, raw: &toml::Table) -> Vec<String> {
    KEYS.iter()
        .filter(|key| !key.live && effective(started, key) != effective(raw, key))
        .map(|key| key.name.to_string())
        .collect()
}

/// Strips the user name and password from the URLs of `value`.
fn without_passwords(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(without_credentials(text)),
        Value::Array(items) => Value::Array(items.iter().map(without_passwords).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), without_passwords(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Every key of the reference with its value in the file, or its default: `{"value": ..., "default": bool}`.
/// Secrets are not in the file, so they cannot be here; a URL has no user name and password.
pub fn values(raw: &toml::Table) -> BTreeMap<String, Value> {
    KEYS.iter()
        .map(|key| {
            let mut value = effective(raw, key);
            if URLS.contains(&key.name) {
                value = without_passwords(&value);
            }
            let default = lookup(raw, key.name).is_none();
            (
                key.name.to_string(),
                json!({ "value": value, "default": default }),
            )
        })
        .collect()
}

/// Reads and parses the configuration file; a file that is not there is an empty one.
pub fn read_raw(file: &Path) -> Result<(String, toml::Table)> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let raw = toml::from_str(&text).map_err(|e| {
        invalid(format!(
            "{} is not valid TOML: {}",
            file.display(),
            redact(&e.to_string())
        ))
    })?;
    Ok((text, raw))
}

/// Writes `bytes` to `path` through a temporary file in the same directory, which replaces it in one step. The file
/// is readable by its owner only when `private`.
fn write_atomically(path: &Path, bytes: &[u8], private: bool) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    if private {
        options.mode(0o600);
    }
    let written = options.open(&temporary).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    match written.and_then(|()| std::fs::rename(&temporary, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temporary);
            Err(e)
        }
    }
}

/// The API keys typed in the app: `<data_dir>/secrets.toml`, a table of name and key, readable by the owner only.
/// A key leaves this file only to be given to the router or the decision model, never to a client, the store or a log.
#[derive(Debug, Clone)]
pub struct Secrets {
    path: PathBuf,
}

impl Secrets {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("secrets.toml"),
        }
    }

    fn read(&self) -> Result<toml::Table> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|_| invalid(format!("{} is not valid TOML", self.path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// The names of the stored keys, never the keys.
    pub fn names(&self) -> Result<Vec<String>> {
        Ok(self.read()?.keys().cloned().collect())
    }

    /// The key stored under `name`.
    pub fn get(&self, name: &str) -> Result<Option<String>> {
        Ok(self
            .read()?
            .get(name)
            .and_then(toml::Value::as_str)
            .map(str::to_string))
    }

    /// Stores `value` under `name`, replacing a key stored before.
    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        if !valid_agent_name(name) {
            return Err(invalid(format!(
                "{name:?} is not a name (letters, digits, - and _)"
            )));
        }
        if value.trim().is_empty() {
            return Err(invalid("the key is empty"));
        }
        let mut secrets = self.read()?;
        secrets.insert(
            name.to_string(),
            toml::Value::String(value.trim().to_string()),
        );
        self.write(&secrets)
    }

    /// Removes the key stored under `name`; false when there was none.
    pub fn remove(&self, name: &str) -> Result<bool> {
        let mut secrets = self.read()?;
        let had = secrets.remove(name).is_some();
        if had {
            self.write(&secrets)?;
        }
        Ok(had)
    }

    fn write(&self, secrets: &toml::Table) -> Result<()> {
        let text = toml::to_string(secrets).map_err(|e| invalid(e.to_string()))?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Ok(write_atomically(&self.path, text.as_bytes(), true)?)
    }

    /// The key an upstream or the decision model is given: from the environment variable `env`, or from the secret
    /// `secret`, or none. A variable that is not set or a secret that is not stored is an error.
    pub fn resolve(
        &self,
        env: Option<&str>,
        secret: Option<&str>,
    ) -> std::result::Result<Option<String>, String> {
        match (env, secret) {
            (Some(var), _) => std::env::var(var)
                .map(Some)
                .map_err(|_| format!("the environment variable {var} is not set")),
            (None, Some(name)) => match self.get(name) {
                Ok(Some(key)) => Ok(Some(key)),
                Ok(None) => Err(format!("the secret {name} is not stored")),
                Err(e) => Err(e.to_string()),
            },
            (None, None) => Ok(None),
        }
    }

    /// The router's upstreams of `config`, each with its key.
    pub fn upstreams(&self, config: &Config) -> std::result::Result<Vec<Upstream>, String> {
        config
            .upstreams
            .iter()
            .map(|upstream| {
                Ok(Upstream {
                    protocol: upstream.protocol,
                    base_url: upstream.base_url.clone(),
                    api_key: self.resolve(
                        upstream.api_key_env.as_deref(),
                        upstream.api_key_secret.as_deref(),
                    )?,
                })
            })
            .collect()
    }
}

/// The parts of the running server that take live keys.
struct Targets {
    engine: Arc<Engine>,
    router: Arc<Router>,
    agents: Arc<RwLock<AgentSet>>,
}

/// The configuration the running server reads, and what edits it. The keys that are live (see [`KEYS`]) are read from
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

    /// Applies `changes` to the file: refused with the message of the parse when the result is not a valid
    /// configuration, and then nothing is written. Otherwise the file is replaced in one step, the person's decision
    /// is recorded and the live keys are applied.
    pub fn save(&self, app: &App, changes: &Changes) -> Result<Saved> {
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
        app.decisions.record_person(
            &Question {
                kind: "edit_settings".into(),
                context,
                options: vec!["edit_settings".into(), "leave".into()],
                fail_closed: "leave".into(),
                run_id: None,
                card_id: None,
            },
            "edit_settings",
            "",
        )?;
        self.apply(config, upstreams);
        let raw = toml::from_str(&edited).unwrap_or_default();
        let restart_keys = restart_keys(&self.started_raw, &raw);
        Ok(Saved {
            restart_needed: !restart_keys.is_empty(),
            restart_keys,
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
        running.board_tick = new.board_tick;
        running.upstreams = new.upstreams.clone();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn changes(set: Value, remove: &[&str]) -> Changes {
        Changes {
            set: serde_json::from_value(set).unwrap(),
            remove: remove.iter().map(|key| key.to_string()).collect(),
        }
    }

    fn parse(text: &str) -> std::result::Result<Config, crate::config::ConfigError> {
        Config::parse(text, Path::new("/srv"), Path::new("/srv/moebius.toml"))
    }

    #[test]
    fn a_change_keeps_the_comments_and_the_order_of_the_file() {
        let text = "# The server.\nrun_limit = 3 # at a time\n\n# The board.\n[board]\ntick_ms = 2000\n# Rounds.\nround_limit = 5 # per card\n\n[checks]\ntimeout_seconds = 60\n";
        let edited = edit(
            text,
            &changes(json!({"board.round_limit": 9, "run_limit": 4}), &[]),
        )
        .unwrap();
        assert_eq!(
            edited,
            "# The server.\nrun_limit = 4 # at a time\n\n# The board.\n[board]\ntick_ms = 2000\n# Rounds.\nround_limit = 9 # per card\n\n[checks]\ntimeout_seconds = 60\n"
        );
    }

    #[test]
    fn the_example_stays_the_example_but_for_the_change() {
        let edited = edit(
            crate::config::EXAMPLE,
            &changes(json!({"board.round_limit": 7}), &[]),
        )
        .unwrap();
        assert_eq!(
            edited,
            crate::config::EXAMPLE.replace("round_limit = 5", "round_limit = 7")
        );
        assert_eq!(parse(&edited).unwrap().round_limit, 7);
    }

    #[test]
    fn keys_and_tables_that_are_not_in_the_file_are_made() {
        let edited = edit(
            "run_limit = 2\n\n[board]\ntick_ms = 100\n",
            &changes(
                json!({
                    "budgets.day_tokens": 5000,
                    "rules_file": "rules.md",
                    "teacher": {"model": "m", "timeout_seconds": 9},
                    "agents.Coder": {"model": "c", "skills": ["a"]},
                }),
                &[],
            ),
        )
        .unwrap();
        let config = parse(&edited).unwrap();
        assert_eq!(config.day_tokens, Some(5000));
        assert_eq!(config.rules_file, Some(PathBuf::from("/srv/rules.md")));
        assert_eq!(config.teacher.unwrap().model, "m");
        assert_eq!(config.agents["Coder"].model, "c");
        assert!(edited.starts_with("run_limit = 2\nrules_file = \"rules.md\"\n"));
        assert!(!edited.contains("[agents]"), "{edited}");
        assert!(edited.contains("[board]\ntick_ms = 100\n"));
    }

    #[test]
    fn a_removed_table_or_key_is_gone_and_one_that_is_not_there_is_ignored() {
        let text = "run_limit = 2\n\n[teacher]\nmodel = \"m\"\n\n[board]\ntick_ms = 100\nround_limit = 2\n\n[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"http://a\"\n\n[[router.upstreams]]\nprotocol = \"anthropic\"\nbase_url = \"http://b\"\n";
        let edited = edit(
            text,
            &changes(
                json!({}),
                &[
                    "teacher",
                    "board.tick_ms",
                    "router.upstreams.0",
                    "checks.timeout_seconds",
                    "github",
                ],
            ),
        )
        .unwrap();
        let config = parse(&edited).unwrap();
        assert!(config.teacher.is_none());
        assert_eq!(config.board_tick, std::time::Duration::from_millis(2000));
        assert_eq!(config.round_limit, 2);
        assert_eq!(config.upstreams.len(), 1);
        assert_eq!(config.upstreams[0].base_url, "http://b");
        assert!(!edited.contains("[teacher]"));
        let none = edit(&edited, &changes(json!({}), &["router.upstreams.0"])).unwrap();
        assert!(!none.contains("upstreams"), "{none}");
    }

    #[test]
    fn upstreams_are_set_by_position_or_whole() {
        let text = "[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"http://a\"\n";
        let edited = edit(
            text,
            &changes(
                json!({
                    "router.upstreams.1": {"protocol": "anthropic", "base_url": "http://b", "api_key_secret": "k"},
                    "router.upstreams.0": {"protocol": "openai", "base_url": "http://c"},
                }),
                &[],
            ),
        )
        .unwrap();
        let config = parse(&edited).unwrap();
        let urls: Vec<_> = config
            .upstreams
            .iter()
            .map(|u| u.base_url.as_str())
            .collect();
        assert_eq!(urls, ["http://c", "http://b"]);
        assert_eq!(config.upstreams[1].api_key_secret.as_deref(), Some("k"));
        let whole = edit(
            text,
            &changes(
                json!({"router.upstreams": [{"protocol": "anthropic", "base_url": "http://z"}]}),
                &[],
            ),
        )
        .unwrap();
        assert_eq!(parse(&whole).unwrap().upstreams[0].base_url, "http://z");
        assert!(
            edit(
                text,
                &changes(
                    json!({"router.upstreams.5": {"protocol": "openai", "base_url": "x"}}),
                    &[]
                )
            )
            .is_err()
        );
    }

    #[test]
    fn entries_added_in_one_change_are_added_in_order() {
        let entries: Vec<Value> = (0..12)
            .map(|n| json!({"protocol": "openai", "base_url": format!("http://h{n}")}))
            .collect();
        let set: serde_json::Map<String, Value> = entries
            .iter()
            .enumerate()
            .map(|(n, entry)| (format!("router.upstreams.{n}"), entry.clone()))
            .collect();
        let edited = edit("", &changes(Value::Object(set), &[])).unwrap();
        let urls: Vec<String> = parse(&edited)
            .unwrap()
            .upstreams
            .into_iter()
            .map(|u| u.base_url)
            .collect();
        assert_eq!(urls[0], "http://h0");
        assert_eq!(urls[11], "http://h11");
        assert_eq!(urls.len(), 12);
    }

    #[test]
    fn what_is_not_a_key_or_a_value_is_refused() {
        for bad in [
            changes(json!({"run limit": 1}), &[]),
            changes(json!({"a..b": 1}), &[]),
            changes(json!({"run_limit": null}), &[]),
            changes(json!({}), &["../x"]),
        ] {
            assert!(
                matches!(edit("", &bad), Err(AppError::Invalid(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn every_live_key_is_read_and_every_other_key_is_told_to_need_a_restart() {
        let started: toml::Table =
            toml::from_str("run_limit = 2\n[teacher]\nmodel = \"a\"\n").unwrap();
        let raw: toml::Table = toml::from_str(
            "run_limit = 9\nlisten = \"127.0.0.1:7171\"\n[teacher]\nmodel = \"b\"\n[board]\nround_limit = 1\n[github]\napp_id = 1\n",
        )
        .unwrap();
        // `listen` is the default as written, so it did not change; `run_limit` and `board` are live.
        assert_eq!(
            restart_keys(&started, &raw),
            ["teacher.model", "github.app_id"]
        );
        for key in KEYS {
            assert!(key.default.is_empty() || serde_json::from_str::<Value>(key.default).is_ok());
        }
    }

    #[test]
    fn the_values_tell_the_file_and_the_defaults_and_hold_no_password() {
        let raw: toml::Table = toml::from_str(
            "run_limit = 8\n[harness]\nurl = \"https://u:hunter2@example.com/h.git\"\n[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"http://a:hunter2@127.0.0.1:1\"\napi_key_secret = \"k\"\n",
        )
        .unwrap();
        let values = values(&raw);
        assert_eq!(values["run_limit"], json!({"value": 8, "default": false}));
        assert_eq!(
            values["board.round_limit"],
            json!({"value": 5, "default": true})
        );
        assert_eq!(
            values["budgets.day_tokens"],
            json!({"value": null, "default": true})
        );
        assert_eq!(values["harness.url"]["value"], "https://example.com/h.git");
        assert_eq!(
            values["router.upstreams"]["value"][0]["base_url"],
            "http://127.0.0.1:1"
        );
        assert_eq!(
            values["router.upstreams"]["value"][0]["api_key_secret"],
            "k"
        );
        assert!(!serde_json::to_string(&values).unwrap().contains("hunter2"));
        assert_eq!(values.len(), KEYS.len());
    }

    #[test]
    fn a_secret_is_stored_privately_replaced_and_removed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::new(dir.path());
        assert_eq!(secrets.names().unwrap(), Vec::<String>::new());
        secrets.set("anthropic", "sk-first").unwrap();
        secrets.set("local", "second").unwrap();
        secrets.set("anthropic", "sk-third").unwrap();
        let path = dir.path().join("secrets.toml");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(secrets.names().unwrap(), ["anthropic", "local"]);
        assert_eq!(
            secrets.get("anthropic").unwrap().as_deref(),
            Some("sk-third")
        );
        assert!(secrets.remove("local").unwrap());
        assert!(!secrets.remove("local").unwrap());
        assert_eq!(secrets.names().unwrap(), ["anthropic"]);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(matches!(secrets.set("a b", "x"), Err(AppError::Invalid(_))));
        assert!(matches!(secrets.set("ok", "  "), Err(AppError::Invalid(_))));
        // No temporary file is left.
        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn a_key_comes_from_the_environment_or_a_secret_and_is_missing_with_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::new(dir.path());
        secrets.set("k", "typed").unwrap();
        assert_eq!(
            secrets.resolve(None, Some("k")).unwrap().as_deref(),
            Some("typed")
        );
        assert_eq!(secrets.resolve(None, None).unwrap(), None);
        assert!(
            secrets
                .resolve(None, Some("nope"))
                .unwrap_err()
                .contains("not stored")
        );
        assert!(
            secrets
                .resolve(Some("MOEBIUS_SURELY_UNSET_VARIABLE"), None)
                .unwrap_err()
                .contains("not set")
        );
    }

    #[test]
    fn a_key_may_not_be_given_twice() {
        assert!(matches!(
            parse(
                "[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"x\"\napi_key_env = \"A\"\napi_key_secret = \"b\"\n"
            ),
            Err(crate::config::ConfigError::KeyTwice { .. })
        ));
        assert!(matches!(
            parse("[decision_model]\nurl = \"x\"\nmodel = \"m\"\napi_key_secret = \"a b\"\n"),
            Err(crate::config::ConfigError::KeySecret { .. })
        ));
    }
}
