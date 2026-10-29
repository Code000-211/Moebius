//! The Setup pages: Models, Plugins and Settings. Models and Settings edit `moebius.toml` through
//! `PUT /v1/settings`: a form of the keys as `GET /v1/settings` tells them, whose Save sends only the keys that were
//! changed. An API key typed here is sent to `PUT /v1/secrets/{name}` first and the file names it; it is not kept in
//! the app after it is sent. A key that the running server uses only after a restart makes a banner with a Restart
//! button. The Plugins page only shows. The Settings page also shows where this app is connected and leads to the
//! connect screen to change that.

use std::collections::BTreeSet;

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, pick_list, row, scrollable, space, text, text_input};
use iced::{Alignment, Element, Length, Padding, Task};
use serde_json::{Map, Value, json};

use crate::app::Message as AppMessage;
use crate::client::{
    Available, AvailableModel, Client, ClientError, Models, Plugins, SavedSettings, ServerSettings,
};
use crate::screens::agents::{menu_style, pick_style};
use crate::screens::board::tokens;
use crate::screens::connect;
use crate::screens::live::say;
use crate::settings::{self, Settings};
use crate::theme;
use crate::widgets::model_picker;
use crate::widgets::page;

const PROTOCOLS: [&str; 2] = ["openai", "anthropic"];

/// How a key of the configuration is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    /// A model's name: a dropdown of the models the upstreams serve, or an input for another one.
    Model,
    /// A path, a URL or an address, set in the code font.
    Code,
    Whole,
    Decimal,
    /// Names separated by commas.
    List,
    Pick(&'static [&'static str]),
}

/// The keys of the Settings page by box: the dotted key, its label and how it is typed.
/// A key of the configuration as a form shows it: the dotted key, its label and how it is typed.
type Spec = (&'static str, &'static str, Kind);

const GROUPS: &[(&str, &[Spec])] = &[
    (
        "Server",
        &[
            ("data_dir", "Data directory", Kind::Code),
            ("listen", "Desktop API listens on", Kind::Code),
            ("token_file", "Token file", Kind::Code),
            ("router.listen", "Router listens on", Kind::Code),
            ("run_limit", "Runs at a time", Kind::Whole),
        ],
    ),
    (
        "Board and budgets",
        &[
            ("board.tick_ms", "Board tick (ms)", Kind::Whole),
            ("board.round_limit", "Coding rounds a card", Kind::Whole),
            ("budgets.day_tokens", "Tokens a day", Kind::Whole),
            (
                "checks.timeout_seconds",
                "Checks time limit (s)",
                Kind::Whole,
            ),
        ],
    ),
    (
        "Nightly schedule",
        &[
            ("retro_at", "Retro at (HH:MM)", Kind::Text),
            ("trial_window.start", "Trial window starts", Kind::Text),
            ("trial_window.end", "Trial window ends", Kind::Text),
        ],
    ),
    (
        "Harness",
        &[
            ("harness.url", "Repository URL", Kind::Code),
            ("harness.mcp_env", "MCP variables", Kind::List),
            ("rules_file", "Rules file", Kind::Code),
        ],
    ),
    ("Pi-Bolt", &[("pi_bolt.program", "Program", Kind::Code)]),
    (
        "GitHub",
        &[
            ("github.app_id", "App id", Kind::Whole),
            ("github.private_key_file", "Private key file", Kind::Code),
            ("github.api_url", "API URL", Kind::Code),
            ("github.poll_seconds", "Poll every (s)", Kind::Whole),
        ],
    ),
];

/// The tables of the file that are there or not: when every key of one is blank the table is removed.
const TABLES: &[(&str, &[&str])] = &[
    (
        "github",
        &[
            "github.app_id",
            "github.private_key_file",
            "github.api_url",
            "github.poll_seconds",
        ],
    ),
    ("harness", &["harness.url", "harness.mcp_env"]),
];

const TEACHER: &[Spec] = &[
    ("teacher.model", "Model", Kind::Model),
    ("teacher.protocol", "Protocol", Kind::Pick(&PROTOCOLS)),
    ("teacher.timeout_seconds", "Time limit (s)", Kind::Whole),
];

const DECISION_MODEL: &[Spec] = &[
    ("decision_model.url", "URL", Kind::Code),
    ("decision_model.model", "Model", Kind::Model),
    (
        "decision_model.timeout_seconds",
        "Time limit (s)",
        Kind::Whole,
    ),
    ("decision_model.shadow", "Kinds to shadow", Kind::List),
    (
        "decision_model.agreement",
        "Agreement needed",
        Kind::Decimal,
    ),
    (
        "decision_model.min_judgements",
        "Judgements needed",
        Kind::Whole,
    ),
];

#[derive(Debug, Clone)]
pub enum Message {
    Models(Result<Models, ClientError>),
    Plugins(Result<Plugins, ClientError>),
    /// The models the upstreams serve.
    Available(Result<Available, ClientError>),
    /// The link under a Model field was pressed: ask the upstreams again.
    RefreshAvailable,
    /// A model was picked from the list of the field with this key.
    PickModel(&'static str, AvailableModel),
    /// "Another model…" was chosen in the Model field with this key: it becomes an input.
    TypeModel(&'static str),
    Server(Result<ServerSettings, ClientError>),
    /// The input of a key of the configuration was edited.
    Edit(&'static str, String),
    SaveSettings,
    SaveModels,
    /// The server answered a save: whether a restart is needed, or why not; `None` when nothing was sent.
    Saved(Result<Option<SavedSettings>, ClientError>),
    AddUpstream,
    RemoveUpstream(usize),
    UpstreamProtocol(usize, String),
    UpstreamUrl(usize, String),
    KeyMode(Owner, KeyMode),
    KeyEnv(Owner, String),
    KeyTyped(Owner, String),
    ToggleTeacher,
    ToggleDecisionModel,
    Restart,
    /// The server answered the request to restart.
    Restarting(Result<(), ClientError>),
}

/// What an API key belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Upstream(usize),
    DecisionModel,
}

/// Where an API key comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMode {
    None,
    Environment,
    Typed,
}

impl KeyMode {
    const ALL: [KeyMode; 3] = [KeyMode::None, KeyMode::Environment, KeyMode::Typed];

    fn label(self) -> &'static str {
        match self {
            KeyMode::None => "No key",
            KeyMode::Environment => "From the environment",
            KeyMode::Typed => "Typed key",
        }
    }
}

/// One key of the configuration as an input.
#[derive(Debug, Clone)]
struct Field {
    key: &'static str,
    label: &'static str,
    kind: Kind,
    /// What the input holds; blank when the file does not set the key.
    text: String,
    /// What it held when the settings were read.
    original: String,
    /// What the server uses when the file does not set the key.
    default: Option<String>,
    /// What a Model field offers.
    models: Vec<AvailableModel>,
    /// Whether a Model field is an input rather than the list.
    typing: bool,
}

/// A value as the text of an input.
fn text_of(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(text_of).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

impl Field {
    fn read(spec: &(&'static str, &'static str, Kind), server: &ServerSettings) -> Self {
        let (key, label, kind) = *spec;
        let shown = text_of(server.value(key));
        let set = server.is_set(key);
        // A picker always shows a choice, so it holds the default when the file does not set the key.
        let text = if set || matches!(kind, Kind::Pick(_)) {
            shown.clone()
        } else {
            String::new()
        };
        Field {
            key,
            label,
            kind,
            original: text.clone(),
            text,
            default: (!set && !shown.is_empty()).then_some(shown),
            models: vec![],
            typing: false,
        }
    }

    /// The new value of the key, none when the input is blank; or what is wrong with the input.
    fn value(&self) -> Result<Option<Value>, String> {
        let text = self.text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        let wrong = |what: &str| format!("{} is {what}.", self.label);
        Ok(Some(match self.kind {
            Kind::Text | Kind::Code | Kind::Model | Kind::Pick(_) => json!(text),
            Kind::Whole => json!(text.parse::<i64>().map_err(|_| wrong("a whole number"))?),
            Kind::Decimal => json!(text.parse::<f64>().map_err(|_| wrong("a number"))?),
            Kind::List => json!(
                text.split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .collect::<Vec<_>>()
            ),
        }))
    }

    fn changed(&self) -> bool {
        self.text.trim() != self.original.trim()
    }

    /// Whether the file set this key when it was read.
    fn was_set(&self) -> bool {
        !self.original.trim().is_empty()
    }
}

/// The inputs of some keys.
#[derive(Debug, Clone, Default)]
struct Form {
    fields: Vec<Field>,
}

impl Form {
    fn read(specs: &[Spec], server: &ServerSettings) -> Self {
        Form {
            fields: specs.iter().map(|spec| Field::read(spec, server)).collect(),
        }
    }

    fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.key == key)
    }

    /// Adds the keys that were changed to `set` and `remove`.
    fn changes(
        &self,
        set: &mut Map<String, Value>,
        remove: &mut Vec<String>,
    ) -> Result<(), String> {
        for field in self.fields.iter().filter(|field| field.changed()) {
            match field.value()? {
                Some(value) => {
                    set.insert(field.key.to_string(), value);
                }
                None => remove.push(field.key.to_string()),
            }
        }
        Ok(())
    }

    /// Every key as the input shows it, for a table that is new.
    fn all(&self, set: &mut Map<String, Value>) -> Result<(), String> {
        for field in &self.fields {
            if let Some(value) = field.value()? {
                set.insert(field.key.to_string(), value);
            }
        }
        Ok(())
    }

    fn dirty(&self) -> bool {
        self.fields.iter().any(Field::changed)
    }
}

/// An API key to store by name.
type Stored = (String, String);

/// Where the key of an upstream or of the decision model comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyChoice {
    mode: KeyMode,
    /// The environment variable.
    env: String,
    /// The name of the key the server stores, if it has one for this.
    stored: Option<String>,
    /// The key being typed; never read back from the server.
    typed: String,
}

impl KeyChoice {
    fn read(env: Option<&Value>, secret: Option<&Value>) -> Self {
        let name = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_string);
        let (env, stored) = (name(env), name(secret));
        KeyChoice {
            mode: match (&env, &stored) {
                (Some(_), _) => KeyMode::Environment,
                (None, Some(_)) => KeyMode::Typed,
                (None, None) => KeyMode::None,
            },
            env: env.unwrap_or_default(),
            stored,
            typed: String::new(),
        }
    }

    /// The key to send: the environment variable or the name of the stored key, which is `fresh` when none is stored
    /// yet; the key to store under that name, if one was typed.
    fn plan(
        &self,
        what: &str,
        fresh: impl FnOnce() -> String,
    ) -> Result<(Map<String, Value>, Option<Stored>), String> {
        let mut keys = Map::new();
        let mut store = None;
        match self.mode {
            KeyMode::None => {}
            KeyMode::Environment => {
                if self.env.trim().is_empty() {
                    return Err(format!(
                        "Enter the environment variable of the key of {what}."
                    ));
                }
                keys.insert("api_key_env".into(), json!(self.env.trim()));
            }
            KeyMode::Typed => {
                let typed = self.typed.trim();
                if typed.is_empty() && self.stored.is_none() {
                    return Err(format!("Type the key of {what}."));
                }
                let name = self.stored.clone().unwrap_or_else(fresh);
                if !typed.is_empty() {
                    store = Some((name.clone(), typed.to_string()));
                }
                keys.insert("api_key_secret".into(), json!(name));
            }
        }
        Ok((keys, store))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Upstream {
    /// The place in the list the file had it, none for one added here.
    origin: Option<usize>,
    protocol: String,
    base_url: String,
    key: KeyChoice,
}

impl Upstream {
    fn read(value: &Value, origin: usize) -> Self {
        let text = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Upstream {
            origin: Some(origin),
            protocol: text("protocol"),
            base_url: text("base_url"),
            key: KeyChoice::read(value.get("api_key_env"), value.get("api_key_secret")),
        }
    }
}

/// The teacher or the decision model: a table of the file that can be turned off.
#[derive(Debug, Clone)]
struct Optional {
    was_on: bool,
    on: bool,
    form: Form,
    /// Only the decision model has a key.
    key: Option<(KeyChoice, KeyChoice)>,
}

/// The forms of the Models and Settings pages, made from what the server told.
#[derive(Debug, Clone, Default)]
struct Forms {
    settings: Vec<(&'static str, Form)>,
    upstreams: Vec<Upstream>,
    original_upstreams: Vec<Upstream>,
    teacher: Option<Optional>,
    decision_model: Option<Optional>,
}

impl Forms {
    fn read(server: &ServerSettings) -> Self {
        let upstreams: Vec<Upstream> = server
            .value("router.upstreams")
            .as_array()
            .map(|list| {
                list.iter()
                    .enumerate()
                    .map(|(n, u)| Upstream::read(u, n))
                    .collect()
            })
            .unwrap_or_default();
        let optional = |specs: &[Spec], keyed: bool| {
            let was_on = specs.iter().any(|(key, _, _)| server.is_set(key));
            let key = keyed.then(|| {
                let choice = KeyChoice::read(
                    server
                        .is_set("decision_model.api_key_env")
                        .then(|| server.value("decision_model.api_key_env")),
                    server
                        .is_set("decision_model.api_key_secret")
                        .then(|| server.value("decision_model.api_key_secret")),
                );
                (choice.clone(), choice)
            });
            Optional {
                was_on,
                on: was_on,
                form: Form::read(specs, server),
                key,
            }
        };
        Forms {
            settings: GROUPS
                .iter()
                .map(|(title, specs)| (*title, Form::read(specs, server)))
                .collect(),
            original_upstreams: upstreams.clone(),
            upstreams,
            teacher: Some(optional(TEACHER, false)),
            decision_model: Some(optional(DECISION_MODEL, true)),
        }
    }

    fn field_mut(&mut self, key: &str) -> Option<&mut Field> {
        let optional = self
            .teacher
            .iter_mut()
            .chain(self.decision_model.iter_mut())
            .map(|optional| &mut optional.form);
        let settings = self.settings.iter_mut().map(|(_, form)| form);
        for form in settings.chain(optional) {
            if let Some(field) = form.fields.iter_mut().find(|field| field.key == key) {
                return Some(field);
            }
        }
        None
    }

    fn settings_dirty(&self) -> bool {
        self.settings.iter().any(|(_, form)| form.dirty())
    }

    /// The keys the Settings page changed.
    fn settings_changes(&self) -> Result<(Map<String, Value>, Vec<String>), String> {
        let (mut set, mut remove) = (Map::new(), Vec::new());
        for (_, form) in &self.settings {
            form.changes(&mut set, &mut remove)?;
        }
        // A table whose every input is blank is taken out of the file as a whole.
        for (table, keys) in TABLES {
            let fields: Vec<&Field> = self
                .settings
                .iter()
                .flat_map(|(_, form)| &form.fields)
                .filter(|field| keys.contains(&field.key))
                .collect();
            if fields.iter().all(|field| field.text.trim().is_empty())
                && fields.iter().any(|field| field.was_set())
            {
                remove.retain(|key| !keys.contains(&key.as_str()));
                remove.push((*table).to_string());
            }
        }
        Ok((set, remove))
    }

    /// What the Models page changed: the keys to store, to set and to remove.
    fn models_changes(&self, secrets: &[String]) -> Result<Plan, String> {
        let mut plan = Plan::default();
        let mut taken: BTreeSet<String> = secrets.iter().cloned().collect();
        taken.extend(self.upstreams.iter().filter_map(|u| u.key.stored.clone()));
        let mut fresh = |base: &str| {
            let mut n = 1;
            loop {
                let name = if base == "decision-model" && n == 1 {
                    base.to_string()
                } else {
                    format!("{base}-{n}")
                };
                if taken.insert(name.clone()) {
                    return name;
                }
                n += 1;
            }
        };

        // Rows that are gone are removed from the last to the first, so that the places of the others hold; the rows
        // that changed, and the ones added, are set at the place they have after that.
        let kept: BTreeSet<usize> = self.upstreams.iter().filter_map(|u| u.origin).collect();
        for gone in (0..self.original_upstreams.len())
            .rev()
            .filter(|n| !kept.contains(n))
        {
            plan.remove.push(format!("router.upstreams.{gone}"));
        }
        for (place, upstream) in self.upstreams.iter().enumerate() {
            let changed = upstream
                .origin
                .and_then(|origin| self.original_upstreams.get(origin))
                .is_none_or(|original| original != upstream);
            if !changed {
                continue;
            }
            if upstream.base_url.trim().is_empty() {
                return Err("Enter the base URL of each upstream.".into());
            }
            let (keys, store) = upstream
                .key
                .plan(&format!("the upstream {}", place + 1), || fresh("upstream"))?;
            plan.store.extend(store);
            let mut table = Map::new();
            table.insert("protocol".into(), json!(upstream.protocol));
            table.insert("base_url".into(), json!(upstream.base_url.trim()));
            table.extend(keys);
            plan.set
                .insert(format!("router.upstreams.{place}"), Value::Object(table));
        }

        if let Some(teacher) = &self.teacher {
            teacher.changes("teacher", &mut plan, String::new)?;
        }
        if let Some(model) = &self.decision_model {
            model.changes("decision_model", &mut plan, || fresh("decision-model"))?;
        }
        Ok(plan)
    }

    fn models_dirty(&self) -> bool {
        self.upstreams != self.original_upstreams
            || [&self.teacher, &self.decision_model]
                .into_iter()
                .flatten()
                .any(|optional| {
                    optional.was_on != optional.on
                        || optional.form.dirty()
                        || optional
                            .key
                            .as_ref()
                            .is_some_and(|(now, before)| now != before)
                })
    }
}

impl Optional {
    /// Adds what the person changed in the table `table` to `plan`.
    fn changes(
        &self,
        table: &str,
        plan: &mut Plan,
        fresh: impl FnOnce() -> String,
    ) -> Result<(), String> {
        if self.was_on && !self.on {
            plan.remove.push(table.to_string());
            return Ok(());
        }
        if !self.on {
            return Ok(());
        }
        let new = !self.was_on;
        let key_changed = self
            .key
            .as_ref()
            .is_some_and(|(now, before)| new || now != before || !now.typed.is_empty());
        if !new && !self.form.dirty() && !key_changed {
            return Ok(());
        }
        let model = self.form.field(&format!("{table}.model"));
        if model.is_some_and(|field| field.text.trim().is_empty()) {
            return Err(format!(
                "Enter the model of the {}.",
                table.replace('_', " ")
            ));
        }
        if new {
            self.form.all(&mut plan.set)?;
        } else {
            self.form.changes(&mut plan.set, &mut plan.remove)?;
        }
        if let (Some((now, _)), true) = (&self.key, key_changed) {
            let (keys, store) = now.plan("the decision model", fresh)?;
            plan.store.extend(store);
            for name in ["api_key_env", "api_key_secret"] {
                match keys.get(name) {
                    Some(value) => {
                        plan.set.insert(format!("{table}.{name}"), value.clone());
                    }
                    None if !new => plan.remove.push(format!("{table}.{name}")),
                    None => {}
                }
            }
        }
        Ok(())
    }
}

/// What a save sends: the API keys to store by name, then the dotted keys to set and to remove.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub store: Vec<(String, String)>,
    pub set: Map<String, Value>,
    pub remove: Vec<String>,
}

/// What the page says of its last save.
enum Notice {
    Saved,
    NothingChanged,
    Failed(String),
}

/// What the server told of its setup, as far as it has, and the forms made from it.
#[derive(Default)]
pub struct Setup {
    models: Option<Models>,
    plugins: Option<Plugins>,
    /// What the upstreams serve, once they have been asked.
    available: Option<Available>,
    server: Option<ServerSettings>,
    forms: Option<Forms>,
    /// Set after a save, so that the next answer of the server replaces the forms.
    reset: bool,
    notice: Option<Notice>,
    /// Whether the server was asked to restart and has not been heard from since.
    restarting: bool,
}

/// Whether an answer says the token is not accepted.
fn unauthorized<T>(result: &Result<T, ClientError>) -> bool {
    matches!(result, Err(ClientError::Unauthorized))
}

impl Setup {
    /// Reads the three answers again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let (models, plugins, server) = (
            Client::new(settings.clone()),
            Client::new(settings.clone()),
            Client::new(settings.clone()),
        );
        Task::batch([
            self.ask_available(settings, false),
            Task::perform(async move { models.models().await }, |result| {
                AppMessage::Setup(Message::Models(result))
            }),
            Task::perform(async move { plugins.plugins().await }, |result| {
                AppMessage::Setup(Message::Plugins(result))
            }),
            Task::perform(async move { server.server_settings().await }, |result| {
                AppMessage::Setup(Message::Server(result))
            }),
        ])
    }

    /// Asks the upstreams which models they serve; `refresh` makes the server ask them again.
    fn ask_available(&self, settings: &Settings, refresh: bool) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        Task::perform(
            async move { client.available_models(refresh).await },
            |result| AppMessage::Setup(Message::Available(result)),
        )
    }

    /// What a save of the Models page (`models`) or of the Settings page would send, or what is wrong with the inputs.
    pub fn save_request(&self, models: bool) -> Result<Plan, String> {
        let (Some(forms), Some(server)) = (&self.forms, &self.server) else {
            return Err("The settings are not read yet.".into());
        };
        if models {
            forms.models_changes(&server.secrets)
        } else {
            forms.settings_changes().map(|(set, remove)| Plan {
                set,
                remove,
                ..Plan::default()
            })
        }
    }

    /// Whether the server was asked to restart and the app waits for it to come back.
    pub fn restarting(&self) -> bool {
        self.restarting
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        match message {
            Message::Models(result) => unauthorized(result),
            Message::Plugins(result) => unauthorized(result),
            Message::Available(result) => unauthorized(result),
            Message::Server(result) => unauthorized(result),
            Message::Saved(result) => unauthorized(result),
            Message::Restarting(result) => unauthorized(result),
            _ => false,
        }
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Models(Ok(models)) => self.models = Some(models),
            Message::Plugins(Ok(plugins)) => self.plugins = Some(plugins),
            Message::Available(Ok(available)) => {
                self.available = Some(available);
                self.offer_models();
            }
            // Without the list the fields are inputs.
            Message::Available(Err(_)) => {}
            Message::RefreshAvailable => {
                if let Some(settings) = settings {
                    return self.ask_available(settings, true);
                }
            }
            Message::Server(Ok(server)) => {
                // What is being edited is kept when the server tells the same again.
                let editing = self
                    .forms
                    .as_ref()
                    .is_some_and(|forms| forms.settings_dirty() || forms.models_dirty());
                if self.forms.is_none() || self.reset || !editing {
                    self.forms = Some(Forms::read(&server));
                    self.reset = false;
                    self.offer_models();
                }
                self.restarting = false;
                self.server = Some(server);
            }
            Message::Models(Err(_)) | Message::Plugins(Err(_)) | Message::Server(Err(_)) => {}
            Message::SaveSettings => return self.save(settings, false),
            Message::SaveModels => return self.save(settings, true),
            Message::Saved(Ok(_)) => {
                self.notice = Some(Notice::Saved);
                self.reset = true;
                return settings.map_or_else(Task::none, |settings| self.reload(settings));
            }
            Message::Saved(Err(e)) => self.notice = Some(Notice::Failed(say(&e))),
            Message::Restart => {
                if let Some(settings) = settings {
                    self.restarting = true;
                    let client = Client::new(settings.clone());
                    return Task::perform(async move { client.restart_server().await }, |result| {
                        AppMessage::Setup(Message::Restarting(result))
                    });
                }
            }
            Message::Restarting(Err(e)) => {
                self.restarting = false;
                self.notice = Some(Notice::Failed(say(&e)));
            }
            Message::Restarting(Ok(())) => {}
            edit => self.edit(edit),
        }
        Task::none()
    }

    /// Gives the Model fields the models that were listed.
    fn offer_models(&mut self) {
        if let Some(forms) = &mut self.forms {
            for optional in forms
                .teacher
                .iter_mut()
                .chain(forms.decision_model.iter_mut())
            {
                for field in &mut optional.form.fields {
                    if field.kind == Kind::Model {
                        field.models = self.available.as_ref().map_or(vec![], |a| a.models.clone());
                    }
                }
            }
        }
    }

    /// The change of an input of one of the forms.
    fn edit(&mut self, message: Message) {
        let Some(forms) = &mut self.forms else {
            return;
        };
        self.notice = None;
        match message {
            Message::Edit(key, text) => {
                if let Some(field) = forms.field_mut(key) {
                    field.text = text;
                }
            }
            Message::TypeModel(key) => {
                if let Some(field) = forms.field_mut(key) {
                    field.typing = true;
                }
            }
            Message::PickModel(key, model) => {
                if let Some(field) = forms.field_mut(key) {
                    field.text = model.id;
                }
                // The teacher's protocol follows its model; the decision model speaks the protocol of its URL.
                if key == "teacher.model"
                    && let Some(field) = forms.field_mut("teacher.protocol")
                    && PROTOCOLS.contains(&model.protocol.as_str())
                {
                    field.text = model.protocol;
                }
            }
            Message::AddUpstream => forms.upstreams.push(Upstream {
                origin: None,
                protocol: PROTOCOLS[0].to_string(),
                base_url: String::new(),
                key: KeyChoice {
                    mode: KeyMode::None,
                    env: String::new(),
                    stored: None,
                    typed: String::new(),
                },
            }),
            Message::RemoveUpstream(place) => {
                if place < forms.upstreams.len() {
                    forms.upstreams.remove(place);
                }
            }
            Message::UpstreamProtocol(place, protocol) => {
                if let Some(upstream) = forms.upstreams.get_mut(place) {
                    upstream.protocol = protocol;
                }
            }
            Message::UpstreamUrl(place, url) => {
                if let Some(upstream) = forms.upstreams.get_mut(place) {
                    upstream.base_url = url;
                }
            }
            Message::KeyMode(owner, mode) => {
                if let Some(key) = forms.key_mut(owner) {
                    key.mode = mode;
                }
            }
            Message::KeyEnv(owner, env) => {
                if let Some(key) = forms.key_mut(owner) {
                    key.env = env;
                }
            }
            Message::KeyTyped(owner, typed) => {
                if let Some(key) = forms.key_mut(owner) {
                    key.typed = typed;
                }
            }
            Message::ToggleTeacher => {
                if let Some(teacher) = &mut forms.teacher {
                    teacher.on = !teacher.on;
                }
            }
            Message::ToggleDecisionModel => {
                if let Some(model) = &mut forms.decision_model {
                    model.on = !model.on;
                }
            }
            _ => {}
        }
    }

    /// Sends what the Settings page, or the Models page, changed.
    fn save(&mut self, settings: Option<&Settings>, models: bool) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        let plan = match self.save_request(models) {
            Ok(plan) => plan,
            Err(problem) => {
                self.notice = Some(Notice::Failed(problem));
                return Task::none();
            }
        };
        if plan.set.is_empty() && plan.remove.is_empty() && plan.store.is_empty() {
            self.notice = Some(Notice::NothingChanged);
            return Task::none();
        }
        self.notice = None;
        let client = Client::new(settings.clone());
        Task::perform(
            async move {
                // The keys go first, since the file names them and the server refuses a name it does not have.
                for (name, key) in &plan.store {
                    client.store_secret(name, key).await?;
                }
                if plan.set.is_empty() && plan.remove.is_empty() {
                    return Ok(None);
                }
                client.save_settings(plan.set, plan.remove).await.map(Some)
            },
            |result| AppMessage::Setup(Message::Saved(result)),
        )
    }

    pub fn models_page(&self) -> Element<'_, AppMessage> {
        let (Some(models), Some(forms)) = (&self.models, &self.forms) else {
            return opened(
                "Models",
                "Reading the models of the server.",
                text("").into(),
                column![],
            );
        };
        let secrets = self.server.as_ref().map_or(&[][..], |s| &s.secrets[..]);
        let upstreams: Element<AppMessage> = if forms.upstreams.is_empty() {
            page::body("No upstream is configured; every model request fails.")
        } else {
            column(
                forms
                    .upstreams
                    .iter()
                    .enumerate()
                    .map(|(place, upstream)| upstream_row(place, upstream, secrets)),
            )
            .spacing(12)
            .into()
        };
        let upstreams = page::boxed(
            "Upstreams",
            column![
                upstreams,
                page::white_button(
                    "Add upstream",
                    Some(AppMessage::Setup(Message::AddUpstream))
                ),
            ]
            .spacing(12),
        );
        let agents: Element<AppMessage> = if models.agents.is_empty() {
            page::body("The server has no agent.")
        } else {
            table(
                &[
                    ("Agent", 3),
                    ("Model", 4),
                    ("Protocol", 2),
                    ("Thinking", 2),
                    ("Budget", 2),
                ],
                models
                    .agents
                    .iter()
                    .map(|agent| {
                        let (model, protocol) = match &agent.error {
                            Some(_) => ("Cannot run".to_string(), String::new()),
                            None => (agent.model.clone(), agent.protocol.clone()),
                        };
                        vec![
                            strong(&agent.name),
                            plain(model),
                            plain(protocol),
                            plain(agent.thinking.clone().unwrap_or_else(|| "–".into())),
                            plain(agent.token_budget.map_or_else(|| "–".into(), tokens)),
                        ]
                    })
                    .collect(),
            )
        };
        let agents = page::boxed(
            "Agents",
            column![agents, page::body("Agents are edited on the Agents page.")].spacing(10),
        );
        // What the upstreams said is shown once, under the first Model field on the page.
        let note = |first: bool| {
            let available = self.available.as_ref().filter(|_| first)?;
            Some(model_picker::note(
                available,
                AppMessage::Setup(Message::RefreshAvailable),
            ))
        };
        let teacher_on = forms.teacher.as_ref().is_some_and(|t| t.on);
        let teacher = forms.teacher.as_ref().map(|teacher| {
            optional_box(
                "Teacher",
                "The teacher model checks the rules written in prose.",
                "No teacher is configured; a call is refused while a rule in prose applies.",
                teacher,
                Message::ToggleTeacher,
                None,
                note(true),
            )
        });
        let decision_model = forms.decision_model.as_ref().map(|model| {
            optional_box(
                "Decision model",
                "The decision model shadows the teacher and can take kinds of decision over.",
                "No decision model is configured.",
                model,
                Message::ToggleDecisionModel,
                Some((Owner::DecisionModel, secrets)),
                note(!teacher_on),
            )
        });
        let mut content = column![self.status(), upstreams, agents].spacing(theme::PAGE_GAP);
        let mut boxes = row![].spacing(theme::PAGE_GAP).align_y(Alignment::Start);
        for element in [teacher, decision_model].into_iter().flatten() {
            boxes = boxes.push(container(element).width(Length::FillPortion(1)));
        }
        content = content.push(boxes);
        opened(
            "Models",
            "Where model requests go, and the models that check and decide. Keys are never shown.",
            page::primary_button("Save", Some(AppMessage::Setup(Message::SaveModels))),
            content,
        )
    }

    pub fn plugins_page(&self) -> Element<'_, AppMessage> {
        let Some(plugins) = &self.plugins else {
            return opened(
                "Plugins",
                "Reading the plugins of the server.",
                text("").into(),
                column![],
            );
        };
        let list: Element<AppMessage> = if plugins.plugins.is_empty() {
            page::body("No plugin is loaded.")
        } else {
            table(
                &[
                    ("Plugin", 3),
                    ("Version", 2),
                    ("Kinds", 2),
                    ("Time limit", 2),
                    ("Decides", 4),
                ],
                plugins
                    .plugins
                    .iter()
                    .map(|plugin| {
                        vec![
                            mono(plugin.name.clone()),
                            plain(plugin.version.clone()),
                            plain(plugin.kinds.join(", ")),
                            plain(format!("{} s", plugins.time_limit_seconds)),
                            if plugin.decides.is_empty() {
                                plain("No kind of decision".to_string())
                            } else {
                                mono(plugin.decides.join(", "))
                            },
                        ]
                    })
                    .collect(),
            )
        };
        opened(
            "Plugins",
            &format!(
                "{} plugins loaded; a call into one is refused after {} s.",
                plugins.plugins.len(),
                plugins.time_limit_seconds
            ),
            text("").into(),
            column![page::boxed("Loaded plugins", list)],
        )
    }

    /// The Settings page: every key of `moebius.toml` as an input, and where this app is connected, `saved` being the
    /// connection.
    pub fn settings_page(&self, saved: Option<&Settings>) -> Element<'_, AppMessage> {
        let (address, token) = match saved {
            Some(saved) => (saved.address.clone(), settings::mask(&saved.token)),
            None => ("none".to_string(), "none".to_string()),
        };
        let app = page::boxed(
            "This app",
            column![
                facts(vec![
                    ("Server address", mono(address)),
                    ("Token", mono(token))
                ]),
                page::white_button(
                    "Connect to another server",
                    Some(AppMessage::ChangeConnection)
                ),
            ]
            .spacing(14),
        );
        let (Some(server), Some(forms)) = (&self.server, &self.forms) else {
            return opened(
                "Settings",
                "Reading the settings of the server.",
                text("").into(),
                column![page::boxed(
                    "Server",
                    page::body("Reading the settings of the server.")
                )]
                .push(app)
                .spacing(theme::PAGE_GAP),
            );
        };
        let mut boxes = forms.settings.iter().map(|(title, form)| {
            page::boxed(
                title,
                column(form.fields.iter().map(|field| field_view(field, None))).spacing(12),
            )
        });
        let mut content = column![self.status()].spacing(theme::PAGE_GAP);
        while let Some(first) = boxes.next() {
            let mut line = row![container(first).width(Length::FillPortion(1))]
                .spacing(theme::PAGE_GAP)
                .align_y(Alignment::Start);
            if let Some(second) = boxes.next() {
                line = line.push(container(second).width(Length::FillPortion(1)));
            }
            content = content.push(line);
        }
        content = content.push(app);
        opened(
            "Settings",
            &format!(
                "The keys of {}. Save writes only what you changed.",
                server.file
            ),
            page::primary_button("Save", Some(AppMessage::Setup(Message::SaveSettings))),
            content,
        )
    }

    /// The banner that says a restart is needed, and what a save said.
    fn status(&self) -> Element<'_, AppMessage> {
        let mut status = column![].spacing(10);
        if self.restarting {
            status = status.push(banner(
                "Restarting the server. The app connects again by itself.".to_string(),
                None,
            ));
        } else if let Some(server) = self.server.as_ref().filter(|s| s.restart_needed) {
            status = status.push(banner(
                format!(
                    "Restart the server to use: {}",
                    server.restart_keys.join(", ")
                ),
                Some(AppMessage::Setup(Message::Restart)),
            ));
        }
        match &self.notice {
            Some(Notice::Saved) => status = status.push(notice_line("Saved", false)),
            Some(Notice::NothingChanged) => {
                status = status.push(notice_line("Nothing changed, so nothing was saved.", false));
            }
            Some(Notice::Failed(problem)) => status = status.push(notice_line(problem, true)),
            None => {}
        }
        status.into()
    }
}

impl Forms {
    fn key_mut(&mut self, owner: Owner) -> Option<&mut KeyChoice> {
        match owner {
            Owner::Upstream(place) => self.upstreams.get_mut(place).map(|u| &mut u.key),
            Owner::DecisionModel => self
                .decision_model
                .as_mut()
                .and_then(|model| model.key.as_mut())
                .map(|(now, _)| now),
        }
    }
}

/// A box that holds the keys of a table, which can be turned off.
fn optional_box<'a>(
    heading: &'a str,
    about: &'a str,
    off: &'a str,
    optional: &'a Optional,
    toggle: Message,
    key: Option<(Owner, &'a [String])>,
    models_note: Option<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    let toggle = AppMessage::Setup(toggle);
    if !optional.on {
        let line = if optional.was_on {
            "Turned off; Save takes it out of the file."
        } else {
            off
        };
        return page::boxed(
            heading,
            column![
                page::body(line),
                page::white_button("Turn on", Some(toggle))
            ]
            .spacing(12),
        );
    }
    let mut models_note = models_note;
    let mut fields = column(optional.form.fields.iter().map(|field| {
        let note = if field.kind == Kind::Model {
            models_note.take()
        } else {
            None
        };
        field_view(field, note)
    }))
    .spacing(12);
    if let (Some((owner, secrets)), Some((now, _))) = (key, &optional.key) {
        fields = fields.push(key_view(owner, now, secrets));
    }
    page::boxed(
        heading,
        column![
            page::body(about),
            fields,
            page::white_button("Turn off", Some(toggle)),
        ]
        .spacing(12),
    )
}

/// An input with its label and, when the file does not set the key, its default.
fn field_view<'a>(
    field: &'a Field,
    note: Option<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    let key = field.key;
    let on_input = move |text| AppMessage::Setup(Message::Edit(key, text));
    let input: Element<AppMessage> = match field.kind {
        Kind::Pick(options) => {
            let options: Vec<String> = options.iter().map(|o| o.to_string()).collect();
            pick_list(options, Some(field.text.clone()), on_input)
                .text_size(theme::BODY)
                .padding([9, 12])
                .width(Length::Fill)
                .style(pick_style)
                .menu_style(menu_style)
                .into()
        }
        Kind::Model => model_picker::field(
            &field.models,
            field.typing,
            field.default.as_deref().unwrap_or(""),
            &field.text,
            on_input,
            move |model| AppMessage::Setup(Message::PickModel(key, model)),
            AppMessage::Setup(Message::TypeModel(key)),
        ),
        kind => text_input(field.default.as_deref().unwrap_or(""), &field.text)
            .on_input(on_input)
            .font(if kind == Kind::Code {
                theme::MONO
            } else {
                theme::MANROPE
            })
            .size(if kind == Kind::Code {
                theme::CODE
            } else {
                theme::BODY
            })
            .padding([9, 12])
            .style(connect::input_style)
            .into(),
    };
    let mut head = row![
        text(field.label)
            .size(13)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::TEXT_SECONDARY),
        space::horizontal(),
    ]
    .align_y(Alignment::Center);
    if let Some(default) = &field.default
        && field.text.trim().is_empty()
    {
        head = head.push(
            text(format!("Default {default}"))
                .size(theme::LABEL)
                .color(theme::TEXT_MUTED),
        );
    }
    let mut shown = column![head, input].spacing(5);
    if let Some(note) = note {
        shown = shown.push(note);
    }
    shown.into()
}

/// One upstream as a row of inputs.
fn upstream_row<'a>(
    place: usize,
    upstream: &'a Upstream,
    secrets: &'a [String],
) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Setup(message);
    let protocols: Vec<String> = PROTOCOLS.iter().map(|p| p.to_string()).collect();
    let protocol = pick_list(protocols, Some(upstream.protocol.clone()), move |p| {
        message(Message::UpstreamProtocol(place, p))
    })
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let url = text_input("http://127.0.0.1:8080", &upstream.base_url)
        .on_input(move |url| message(Message::UpstreamUrl(place, url)))
        .font(theme::MONO)
        .size(theme::CODE)
        .padding([9, 12])
        .style(connect::input_style);
    row![
        container(labelled("Protocol", protocol.into())).width(Length::FillPortion(2)),
        container(labelled("Base URL", url.into())).width(Length::FillPortion(6)),
        container(key_view(Owner::Upstream(place), &upstream.key, secrets))
            .width(Length::FillPortion(7)),
        container(page::white_button(
            "Remove",
            Some(message(Message::RemoveUpstream(place)))
        ))
        .padding(Padding::default().top(21)),
    ]
    .spacing(12)
    .align_y(Alignment::Start)
    .into()
}

/// The inputs for the key of an upstream or of the decision model: where it comes from, and the variable or the key.
fn key_view<'a>(
    owner: Owner,
    key: &'a KeyChoice,
    secrets: &'a [String],
) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Setup(message);
    let modes: Vec<String> = KeyMode::ALL.iter().map(|m| m.label().to_string()).collect();
    let chosen = key.mode;
    let mode = pick_list(
        modes,
        Some(chosen.label().to_string()),
        move |label: String| {
            let mode = KeyMode::ALL
                .into_iter()
                .find(|m| m.label() == label)
                .unwrap_or(KeyMode::None);
            message(Message::KeyMode(owner, mode))
        },
    )
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let detail: Element<AppMessage> = match key.mode {
        KeyMode::None => space().width(Length::Fill).into(),
        KeyMode::Environment => text_input("ANTHROPIC_API_KEY", &key.env)
            .on_input(move |env| message(Message::KeyEnv(owner, env)))
            .font(theme::MONO)
            .size(theme::CODE)
            .padding([9, 12])
            .style(connect::input_style)
            .into(),
        KeyMode::Typed => {
            let stored = key
                .stored
                .as_ref()
                .is_some_and(|name| secrets.contains(name));
            text_input(
                if stored {
                    "Stored; type to replace it"
                } else {
                    "Paste the key"
                },
                &key.typed,
            )
            .on_input(move |typed| message(Message::KeyTyped(owner, typed)))
            .secure(true)
            .size(theme::BODY)
            .padding([9, 12])
            .style(connect::input_style)
            .into()
        }
    };
    row![
        container(labelled("Key", mode.into())).width(Length::FillPortion(1)),
        container(labelled(" ", detail)).width(Length::FillPortion(1)),
    ]
    .spacing(12)
    .into()
}

fn labelled<'a>(label: &'a str, input: Element<'a, AppMessage>) -> Element<'a, AppMessage> {
    column![
        text(label)
            .size(13)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::TEXT_SECONDARY),
        input,
    ]
    .spacing(5)
    .into()
}

/// The amber line that says the server has to restart for some keys to be used, with the button that restarts it.
fn banner<'a>(line: String, restart: Option<AppMessage>) -> Element<'a, AppMessage> {
    let mut content = row![
        text(line)
            .size(theme::CONTEXT)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::AMBER_TEXT)
            .width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if restart.is_some() {
        content = content.push(page::primary_button("Restart", restart));
    }
    container(content)
        .padding([12, 16])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::AMBER_FILL.into()),
            border: iced::Border {
                color: theme::AMBER_BORDER,
                width: 1.0,
                radius: theme::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// One line about what happened to a save, red when it failed.
fn notice_line<'a>(line: &str, red: bool) -> Element<'a, AppMessage> {
    text(line.to_string())
        .size(theme::CONTEXT)
        .font(theme::manrope(Weight::Semibold))
        .color(if red {
            theme::RED_TEXT
        } else {
            theme::ONLINE_TEXT
        })
        .into()
}

/// A page of the Setup group: the header with its `actions`, and `content` that scrolls.
fn opened<'a>(
    title: &'a str,
    context: &str,
    actions: Element<'a, AppMessage>,
    content: iced::widget::Column<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    column![
        page::header(
            "Setup",
            title,
            None,
            page::context(context.to_string()),
            actions,
        ),
        scrollable(container(content).padding(Padding::default().right(12))).height(Length::Fill),
    ]
    .spacing(theme::PAGE_GAP)
    .width(Length::Fill)
    .into()
}

/// A table with a row of headings; each of `rows` has one element for each column, of the portion of the heading.
fn table<'a>(
    columns: &[(&'static str, u16)],
    rows: Vec<Vec<Element<'a, AppMessage>>>,
) -> Element<'a, AppMessage> {
    let heads = columns
        .iter()
        .fold(row![].spacing(10), |heads, (label, portion)| {
            heads.push(cell(
                *portion,
                text(*label)
                    .size(theme::LABEL)
                    .font(theme::manrope(Weight::Semibold))
                    .color(theme::TEXT_MUTED),
            ))
        });
    let mut table = column![heads, page::rule()].spacing(8);
    for cells in rows {
        let line = cells
            .into_iter()
            .zip(columns)
            .fold(row![].spacing(10), |line, (content, (_, portion))| {
                line.push(cell(*portion, content))
            });
        table = table.push(line).push(page::rule());
    }
    table.into()
}

/// Labelled values, one to a line.
fn facts<'a>(lines: Vec<(&'static str, Element<'a, AppMessage>)>) -> Element<'a, AppMessage> {
    let mut facts = column![].spacing(10);
    for (label, value) in lines {
        facts = facts.push(
            row![
                container(
                    text(label)
                        .size(theme::BODY)
                        .font(theme::manrope(Weight::Semibold))
                        .color(theme::TEXT_MUTED)
                )
                .width(Length::Fixed(170.0)),
                container(value).width(Length::Fill),
            ]
            .spacing(10),
        );
    }
    facts.into()
}

fn cell<'a>(portion: u16, content: impl Into<Element<'a, AppMessage>>) -> Element<'a, AppMessage> {
    container(content.into())
        .width(Length::FillPortion(portion))
        .into()
}

fn strong<'a>(line: &'a str) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::BODY)
        .font(theme::manrope(Weight::Semibold))
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

fn plain<'a>(line: String) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::BODY)
        .color(theme::TEXT_SECONDARY)
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

fn mono<'a>(line: String) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::CODE_SMALL)
        .font(theme::MONO)
        .color(theme::TEXT)
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}
