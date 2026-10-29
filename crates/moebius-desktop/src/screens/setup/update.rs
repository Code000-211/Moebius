//! How a message changes the Setup pages: the answers read, an input edited, a model picked, an API key typed, a
//! save and the restart.

use iced::Task;

use crate::app::Message as AppMessage;
use crate::client::Client;
use crate::screens::live::say;
use crate::settings::Settings;
use crate::widgets::form::PROTOCOLS;

use super::fields::Kind;
use super::forms::{Forms, KeyChoice, Upstream};
use super::{KeyMode, Message, Notice, Setup};

impl Setup {
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
    pub(super) fn edit(&mut self, message: Message) {
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
    pub(super) fn save(&mut self, settings: Option<&Settings>, models: bool) -> Task<AppMessage> {
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
}
