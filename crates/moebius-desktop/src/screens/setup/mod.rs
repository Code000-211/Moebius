//! The Setup pages: Models, Plugins and Settings. Models and Settings edit `moebius.toml` through
//! `PUT /v1/settings`: a form of the keys as `GET /v1/settings` tells them, whose Save sends only the keys that were
//! changed. An API key typed here is sent to `PUT /v1/secrets/{name}` first and the file names it; it is not kept in
//! the app after it is sent. A key that the running server uses only after a restart makes a banner with a Restart
//! button. The Plugins page only shows. The Settings page also shows where this app is connected and leads to the
//! connect screen to change that.
//!
//! `fields` has the keys and one key as an input, `forms` the forms made of them and what a save sends, `update` how a
//! message changes the pages, `view` how they are drawn and `inputs` the inputs of the forms.

mod fields;
mod forms;
mod inputs;
mod update;
mod view;

use iced::Task;
use serde_json::{Map, Value};

use crate::app::Message as AppMessage;
use crate::client::{
    Available, AvailableModel, Client, ClientError, Models, Plugins, SavedSettings, ServerSettings,
    unauthorized,
};
use crate::settings::Settings;

use forms::Forms;

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
}
