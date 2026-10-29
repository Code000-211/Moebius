//! The Automations page: recurring work that runs on its own. The list of automations on the left; on the right the
//! editor of the one chosen (its name, its schedule with a picker of common ones and its next firing, its agent, where
//! its chats work, its prompt, whether it is on, and "Run now"), and its past firings, each of which opens its chat.
//! A save is `POST /v1/automations` or `PUT /v1/automations/{id}`.
//!
//! This module holds the state and how a message changes it; `view` draws the page.

mod view;

use iced::Task;
use iced::widget::text_editor;

use crate::app::Message as AppMessage;
use crate::client::{Automation, AutomationSettings, Client, ClientError, Firing};
use crate::screens::live::say;
use crate::settings::Settings;

/// The schedules the picker offers, with what they are called.
pub const PRESETS: [(&str, &str); 6] = [
    ("Every hour", "0 * * * *"),
    ("Every morning at 8", "0 8 * * *"),
    ("Weekdays at 8", "0 8 * * 1-5"),
    ("Every Friday at 9", "0 9 * * 5"),
    ("Every night at 2", "0 2 * * *"),
    ("Every Monday at 9", "0 9 * * 1"),
];

/// What a schedule is called: the name of its preset, or the expression itself.
pub fn schedule_name(schedule: &str) -> &str {
    let schedule = schedule.trim();
    PRESETS
        .iter()
        .find(|(_, cron)| *cron == schedule)
        .map_or(schedule, |(name, _)| name)
}

/// How long until `at`, at `now` (both Unix seconds): `in 6 min`, `in 3 h`, `in 2 d`.
pub fn until(now: i64, at: i64) -> String {
    match (at - now).max(0) {
        0..60 => "in under a minute".to_string(),
        seconds @ 60..3600 => format!("in {} min", seconds / 60),
        seconds @ 3600..86400 => format!("in {} h", seconds / 3600),
        seconds => format!("in {} d", seconds / 86400),
    }
}

/// Where an automation's chats work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// An empty directory of each chat's own.
    Nowhere,
    /// The repository with this id.
    Repository(i64),
    /// The harness configuration repository.
    Harness,
}

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Vec<Automation>, ClientError>),
    /// The agents were listed, which the agent picker offers.
    Agents(Result<Vec<crate::client::Agent>, ClientError>),
    /// An automation of the list was chosen.
    Select(i64),
    NewAutomation,
    NameEdited(String),
    ScheduleEdited(String),
    /// A schedule of the picker was chosen: its cron expression.
    PresetChosen(String),
    AgentChosen(String),
    PlaceChosen(Place),
    Prompt(text_editor::Action),
    /// On or off.
    Switched(bool),
    Save,
    Saved(Result<Automation, ClientError>),
    RunNow,
    Ran(Result<Firing, ClientError>),
    /// The firings of the automation with this id were read.
    Firings(i64, Result<Vec<Firing>, ClientError>),
    AskDelete,
    Keep,
    Delete,
    Deleted(Result<(), ClientError>),
    /// The clock, which the next firing and the firings are told against.
    Tick(i64),
}

enum Notice {
    Saved,
    /// "Run now" fired it: what came of it.
    Ran(Firing),
    Failed(String),
}

/// The automation in the editor, as the person has it.
struct Editor {
    /// The automation it edits; none for a new one.
    id: Option<i64>,
    name: String,
    schedule: String,
    agent: String,
    place: Place,
    prompt: text_editor::Content,
    enabled: bool,
    /// Its firings, newest first, once read.
    firings: Option<Vec<Firing>>,
    notice: Option<Notice>,
    /// Whether "Delete" asks to be confirmed.
    confirming: bool,
}

impl Editor {
    fn new() -> Self {
        Editor {
            id: None,
            name: String::new(),
            schedule: PRESETS[1].1.to_string(),
            agent: "Moebius".to_string(),
            place: Place::Nowhere,
            prompt: text_editor::Content::new(),
            enabled: true,
            firings: Some(Vec::new()),
            notice: None,
            confirming: false,
        }
    }

    fn of(automation: &Automation) -> Self {
        Editor {
            id: Some(automation.id),
            name: automation.name.clone(),
            schedule: automation.schedule.clone(),
            agent: automation.agent.clone(),
            place: match automation.repository_id {
                Some(id) => Place::Repository(id),
                None if automation.harness_repository => Place::Harness,
                None => Place::Nowhere,
            },
            prompt: text_editor::Content::with_text(&automation.prompt),
            enabled: automation.enabled,
            firings: None,
            notice: None,
            confirming: false,
        }
    }

    /// What a save sends, or what is wrong with the fields.
    fn request(&self) -> Result<AutomationSettings, String> {
        let prompt = self.prompt.text().trim().to_string();
        if self.name.trim().is_empty() {
            return Err("Give the automation a name.".into());
        }
        if self.schedule.split_whitespace().count() != 5 {
            return Err(
                "A schedule has five fields: minute, hour, day of the month, month, day of the week."
                    .into(),
            );
        }
        if prompt.is_empty() {
            return Err("Write the prompt its chats start with.".into());
        }
        Ok(AutomationSettings {
            name: self.name.trim().to_string(),
            schedule: self.schedule.trim().to_string(),
            agent: self.agent.clone(),
            prompt,
            repository: match self.place {
                Place::Repository(id) => Some(id),
                _ => None,
            },
            harness: self.place == Place::Harness,
            enabled: self.enabled,
        })
    }
}

pub struct Automations {
    list: Vec<Automation>,
    /// The names of the agents a chat can be with.
    agents: Vec<String>,
    editor: Option<Editor>,
    now: i64,
}

impl Automations {
    pub fn new(now: i64) -> Self {
        Automations {
            list: Vec::new(),
            agents: Vec::new(),
            editor: None,
            now,
        }
    }

    /// Reads the automations and the agents again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let (list, agents) = (Client::new(settings.clone()), Client::new(settings.clone()));
        Task::batch([
            Task::perform(async move { list.automations().await }, |result| {
                AppMessage::Automations(Message::Listed(result))
            }),
            Task::perform(async move { agents.agents().await }, |result| {
                AppMessage::Automations(Message::Agents(result))
            }),
        ])
    }

    /// Reads the firings of the automation open in the editor again, and the list, as when one fired.
    pub fn refresh(&self, settings: &Settings) -> Task<AppMessage> {
        match self.editor.as_ref().and_then(|editor| editor.id) {
            Some(id) => Task::batch([self.reload(settings), read_firings(settings, id)]),
            None => self.reload(settings),
        }
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Listed(Err(ClientError::Unauthorized))
                | Message::Agents(Err(ClientError::Unauthorized))
        )
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        match message {
            Message::Listed(Ok(list)) => self.list = list,
            Message::Agents(Ok(agents)) => {
                self.agents = agents.into_iter().map(|agent| agent.name).collect();
            }
            Message::Listed(Err(_)) | Message::Agents(Err(_)) => {}
            Message::Tick(now) => self.now = now,
            Message::Select(id) => {
                let Some(automation) = self.list.iter().find(|a| a.id == id) else {
                    return Task::none();
                };
                self.editor = Some(Editor::of(automation));
                return read_firings(settings, id);
            }
            Message::Firings(id, result) => {
                if let Some(editor) = self.editor.as_mut().filter(|e| e.id == Some(id)) {
                    match result {
                        Ok(firings) => editor.firings = Some(firings),
                        Err(e) => editor.notice = Some(Notice::Failed(say(&e))),
                    }
                }
            }
            Message::NewAutomation => self.editor = Some(Editor::new()),
            Message::Save => return self.save(settings),
            Message::Saved(Ok(automation)) => {
                if let Some(editor) = &mut self.editor {
                    if editor.id.is_none() {
                        editor.firings = None;
                    }
                    editor.id = Some(automation.id);
                    editor.notice = Some(Notice::Saved);
                }
                return self.refresh(settings);
            }
            Message::RunNow => {
                if let Some(id) = self.editor.as_ref().and_then(|editor| editor.id) {
                    let client = Client::new(settings.clone());
                    return Task::perform(
                        async move { client.run_automation(id).await },
                        |result| AppMessage::Automations(Message::Ran(result)),
                    );
                }
            }
            Message::Ran(Ok(firing)) => {
                if let Some(editor) = &mut self.editor {
                    editor.notice = Some(Notice::Ran(firing));
                }
                return self.refresh(settings);
            }
            Message::Delete => {
                if let Some(id) = self.editor.as_ref().and_then(|editor| editor.id) {
                    let client = Client::new(settings.clone());
                    return Task::perform(
                        async move { client.delete_automation(id).await },
                        |result| AppMessage::Automations(Message::Deleted(result)),
                    );
                }
            }
            Message::Deleted(Ok(())) => {
                self.editor = None;
                return self.reload(settings);
            }
            Message::Saved(Err(e)) | Message::Ran(Err(e)) | Message::Deleted(Err(e)) => {
                if let Some(editor) = &mut self.editor {
                    editor.confirming = false;
                    editor.notice = Some(Notice::Failed(say(&e)));
                }
            }
            edit => self.edit(edit),
        }
        Task::none()
    }

    /// The changes of the editor's fields.
    fn edit(&mut self, message: Message) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        editor.notice = None;
        match message {
            Message::NameEdited(name) => editor.name = name,
            Message::ScheduleEdited(schedule) | Message::PresetChosen(schedule) => {
                editor.schedule = schedule;
            }
            Message::AgentChosen(agent) => editor.agent = agent,
            Message::PlaceChosen(place) => editor.place = place,
            Message::Prompt(action) => editor.prompt.perform(action),
            Message::Switched(on) => editor.enabled = on,
            Message::AskDelete => editor.confirming = true,
            Message::Keep => editor.confirming = false,
            _ => {}
        }
    }

    fn save(&mut self, settings: &Settings) -> Task<AppMessage> {
        let Some(editor) = &mut self.editor else {
            return Task::none();
        };
        match editor.request() {
            Ok(request) => {
                editor.notice = None;
                let (client, id) = (Client::new(settings.clone()), editor.id);
                Task::perform(
                    async move { client.save_automation(id, &request).await },
                    |result| AppMessage::Automations(Message::Saved(result)),
                )
            }
            Err(problem) => {
                editor.notice = Some(Notice::Failed(problem));
                Task::none()
            }
        }
    }

    /// What is open on the page, in a line for the Moebius agent.
    pub fn context(&self) -> String {
        match &self.editor {
            Some(editor) if editor.id.is_some() => format!(
                "Editing the automation {} (id {}): schedule {}, agent {}",
                editor.name,
                editor.id.unwrap_or_default(),
                editor.schedule,
                editor.agent
            ),
            Some(_) => "Writing a new automation.".to_string(),
            None => format!("{} automations; none is open.", self.list.len()),
        }
    }

    /// The automation open in the editor, as the server last listed it.
    fn saved(&self) -> Option<&Automation> {
        let id = self.editor.as_ref()?.id?;
        self.list.iter().find(|automation| automation.id == id)
    }
}

fn read_firings(settings: &Settings, id: i64) -> Task<AppMessage> {
    let client = Client::new(settings.clone());
    Task::perform(async move { client.firings(id).await }, move |result| {
        AppMessage::Automations(Message::Firings(id, result))
    })
}
