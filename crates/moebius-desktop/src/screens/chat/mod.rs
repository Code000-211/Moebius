//! The chat screen: the chats with agents, newest first, and the chat chosen on the right. Its conversation is the
//! live view's (`Conversation`, drawn by `transcript`) with the agent's text as Markdown; under it stand the change the
//! chat offered, to merge from there, and the field to write in. The state follows one chat: a load of its events, then
//! the stream's `run_event`s. The list of chats is read from the server and kept current by the same stream. Under the
//! field to write in stand small pickers: for a new chat its agent, place, model and reasoning level, for a started one
//! its model and reasoning level, which change it while it waits.
//!
//! `thread` has a chat's conversation wherever it is shown (here and in the floating chat), `update` how a message
//! changes the screen, `view` how it is drawn, and `draft` a chat not yet started with the pickers under the field.

mod draft;
mod thread;
mod update;
mod view;

use iced::{Subscription, Task};

use crate::app::Message as AppMessage;
use crate::client::{
    Agent, Available, Change, Chat, Client, ClientError, Judgement, NewChat, Run, RunEvent, Skill,
};
use crate::settings::Settings;
use crate::theme;
use crate::widgets::chat_terminal::{self, ChatTerminal};

use draft::load_choices;

pub use thread::{OnPage, Place, Thread, load, load_judgements};

/// The agent a chat is with unless another is named.
pub const DEFAULT_AGENT: &str = "Moebius";

/// The agent of a grilling, which needs a repository to end in issues of.
const GRILLING_AGENT: &str = "Grilling-partner";

#[derive(Debug, Clone)]
pub enum Message {
    /// The chats were read.
    Listed(Result<Vec<Chat>, ClientError>),
    /// The events of the chat with this id were read, those after the ones already held.
    Loaded(i64, Result<Vec<RunEvent>, ClientError>),
    /// The judgements of the tool calls of the chat with this id were read.
    Judged(i64, Result<Vec<Judgement>, ClientError>),
    Select(i64),
    /// The list of chats that drops down from the top bar opens or closes.
    ShowList(bool),
    Edited(String),
    Send,
    /// The server took the message, or said why not.
    Sent(Result<Run, ClientError>),
    /// The server started the chat that continues the open, ended one, or said why not.
    Continued(Result<Run, ClientError>),
    AskStop,
    KeepGoing,
    Stop,
    /// End a grilling, which ends in the issues it proposes.
    End,
    /// The server answered stopping or ending the chat.
    Ended(Result<Run, ClientError>),
    /// Offer what the open chat's worktree changes as a change; the chat goes on.
    Offer,
    /// The server offered the change, or said why not.
    Offered(Result<Change, ClientError>),
    /// Open an empty chat, a draft that is nothing on the server until its first message or its terminal starts it.
    NewChat,
    /// Open an empty chat in the repository with this id, as a project's chat tab does.
    NewChatIn(i64),
    /// The agents a chat can be with were read.
    Agents(Result<Vec<Agent>, ClientError>),
    /// The models the upstreams serve were read, for the model picker.
    Models(Result<Available, ClientError>),
    AgentChosen(String),
    RepositoryChosen(Option<i64>),
    /// The harness configuration repository was chosen as the place to work in.
    HarnessChosen,
    /// A model was chosen for the draft, or for the open chat while it waits.
    ModelChosen(String),
    /// A reasoning level was chosen for the draft, or for the open chat while it waits.
    ThinkingChosen(String),
    /// The server changed the open chat's model or reasoning level, or said why not.
    SettingsChanged(Result<Run, ClientError>),
    /// The skills were read: a server with a harness configuration repository always lists the built-in one.
    Skills(Result<Vec<Skill>, ClientError>),
    /// Start the draft without a message and show its terminal once it waits.
    StartInTerminal,
    /// The server started the chat from the draft, or said why not.
    Started(Result<Run, ClientError>),
    /// The clock, for how long ago a chat started.
    Tick(i64),
    /// A link of the agent's text was clicked.
    LinkClicked(String),
    /// A step of the agent was clicked: it unfolds when folded and folds when unfolded. The number is its entry's.
    Fold(usize),
    /// The server was told how far the open chat is read; nothing waits on the answer.
    MarkedRead,
    /// The conversation was scrolled: whether it is at its end now, so that it follows what comes.
    Scrolled(bool),
    /// Show the open chat's terminal (Pi-Bolt's own interface) instead of its conversation, or the conversation again.
    ShowTerminal(bool),
    /// Something the terminal did.
    Terminal(chat_terminal::Message),
}

/// The chat being read.
struct Open {
    thread: Thread,
    message: String,
    /// Whether "Stop" asks to be confirmed.
    confirming: bool,
    /// The last thing that went wrong, as a short line.
    notice: Option<String>,
    /// Whether the conversation is at its end, so that what the agent writes scrolls it on; not after the person
    /// scrolled up.
    follow: bool,
}

impl Open {
    fn new(id: i64) -> Self {
        Open {
            thread: Thread::new(id),
            message: String::new(),
            confirming: false,
            notice: None,
            follow: true,
        }
    }
}

/// A chat that is not started: who it is with, where, on which model and reasoning level, and the first message being
/// written. It is nothing on the server and not in the list; the first Send, or Terminal, starts the chat, and leaving
/// drops the draft.
pub(super) struct Draft {
    agent: String,
    repository: Option<i64>,
    harness: bool,
    /// The model chosen in place of the agent's; none keeps the agent's.
    model: Option<String>,
    /// The reasoning level chosen in place of the agent's; none keeps the agent's.
    thinking: Option<String>,
    message: String,
    /// Whether the server is starting the chat, so a second Send waits.
    sending: bool,
    /// The last thing that went wrong, as a short line.
    notice: Option<String>,
    /// Whether the agent has no model of its own, so that the chat runs on the one chosen for it.
    needs_model: bool,
}

impl Draft {
    pub(super) fn new() -> Self {
        Draft {
            agent: DEFAULT_AGENT.to_string(),
            repository: None,
            harness: false,
            model: None,
            thinking: None,
            message: String::new(),
            sending: false,
            notice: None,
            needs_model: false,
        }
    }

    /// Whether the chat can start: a grilling needs a repository, and an agent without a model needs one chosen.
    pub(super) fn ready(&self) -> bool {
        (self.agent != GRILLING_AGENT || self.repository.is_some())
            && (!self.needs_model || self.model.is_some())
    }

    /// The chat to start, with `message` or without one.
    pub(super) fn new_chat(&self, message: Option<String>) -> NewChat {
        NewChat {
            agent: Some(self.agent.clone()),
            message,
            repository: self.repository,
            harness: self.harness,
            model: self.model.clone(),
            thinking: self.thinking.clone(),
        }
    }
}

/// The id of the field to write in, which the chat's draft focuses.
const MESSAGE_FIELD: &str = "chat-message";
/// The id of the open chat's conversation, which is scrolled to its end as it grows.
const CONVERSATION: &str = "chat-conversation";

/// Whether a scrolled conversation shows its end, give or take a line.
pub fn at_end(viewport: iced::widget::scrollable::Viewport) -> bool {
    viewport.absolute_offset_reversed().y <= theme::SPACE_24
}

pub struct Chats {
    /// The chats, newest first.
    list: Vec<Chat>,
    /// The agents a new chat can be with, as the server last listed them.
    agents: Vec<Agent>,
    /// The ids of the models the upstreams serve, as the server last listed them.
    models: Vec<String>,
    /// Whether the server has a harness configuration repository, where a chat can work.
    has_harness: bool,
    open: Option<Open>,
    draft: Option<Draft>,
    /// The terminal of the open chat, while it is shown.
    terminal: ChatTerminal,
    /// The chat started from a draft's Terminal button, whose terminal is shown once it waits.
    terminal_next: Option<i64>,
    /// Whether the draft being started is to show its terminal once it waits.
    starting_terminal: bool,
    /// The time of the last tick, Unix seconds.
    now: i64,
    /// Whether the list of chats is dropped down from the top bar.
    list_open: bool,
}

/// Sends a request about a chat and tells `then` how it went.
fn request<T: Send + 'static, F, Fut>(
    settings: &Settings,
    then: fn(Result<T, ClientError>) -> Message,
    ask: F,
) -> Task<AppMessage>
where
    F: FnOnce(Client) -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, ClientError>> + Send + 'static,
{
    let client = Client::new(settings.clone());
    Task::perform(ask(client), move |run| AppMessage::Chat(then(run)))
}

impl Chats {
    pub fn new(now: i64) -> Self {
        Chats {
            list: Vec::new(),
            agents: Vec::new(),
            models: Vec::new(),
            has_harness: false,
            open: None,
            draft: None,
            terminal: ChatTerminal::default(),
            terminal_next: None,
            starting_terminal: false,
            now,
            list_open: false,
        }
    }

    /// The id of the chat open on the page.
    pub fn opened(&self) -> Option<i64> {
        self.open.as_ref().map(|open| open.thread.id)
    }

    /// The chat open on the page, as the list has it.
    pub fn open_chat(&self) -> Option<&Chat> {
        self.open
            .as_ref()
            .and_then(|open| self.chat(open.thread.id))
    }

    /// The chats, newest first, those of the floating window included.
    pub fn list(&self) -> &[Chat] {
        &self.list
    }

    /// The chats the page lists, newest first: all but those of the floating window, which the person reads there,
    /// unless one is open here.
    pub fn listed(&self) -> impl Iterator<Item = &Chat> {
        let open = self.open.as_ref().map(|open| open.thread.id);
        self.list
            .iter()
            .filter(move |chat| !chat.is_assistant() || Some(chat.id) == open)
    }

    /// How many chats of the list wait for an answer to a question their agent asked.
    pub fn asking(&self) -> usize {
        self.listed()
            .filter(|chat| chat.status == "waiting" && chat.asks)
            .count()
    }

    /// How many chats have an answer the person has not read; those of the floating window never count.
    pub fn unread(&self) -> usize {
        self.list
            .iter()
            .filter(|chat| !chat.is_assistant() && chat.answered > chat.seen)
            .count()
    }

    /// The open chat is on the screen: what it holds is read. Tells the server when that moves on.
    pub fn mark_read(&mut self, settings: &Settings) -> Task<AppMessage> {
        let Some(open) = &self.open else {
            return Task::none();
        };
        let id = open.thread.id;
        let Some(chat) = self.list.iter_mut().find(|chat| chat.id == id) else {
            return Task::none();
        };
        // Told once an answer is there, not at every streamed piece of it.
        let seq = open.thread.last_seq();
        if chat.answered <= chat.seen || seq <= chat.seen {
            return Task::none();
        }
        chat.seen = seq;
        let client = Client::new(settings.clone());
        Task::perform(async move { client.mark_seen(id, seq).await }, |_| {
            AppMessage::Chat(Message::MarkedRead)
        })
    }

    /// Reads the chats again, and what the open chat missed.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let list = self.reload_list(settings);
        match &self.open {
            Some(open) => Task::batch([
                list,
                load::<OnPage>(settings.clone(), open.thread.id, open.thread.last_seq()),
            ]),
            None => list,
        }
    }

    /// Opens an empty chat and reads what its pickers offer.
    fn open_draft(&mut self, settings: &Settings) -> Task<AppMessage> {
        self.terminal.close();
        self.open = None;
        self.draft = Some(Draft::new());
        self.starting_terminal = false;
        let skills = Client::new(settings.clone());
        Task::batch([
            iced::widget::operation::focus(MESSAGE_FIELD),
            load_choices(settings),
            Task::perform(async move { skills.skills().await }, |skills| {
                AppMessage::Chat(Message::Skills(skills))
            }),
        ])
    }

    /// The person leaves the chat page: the terminal closes its connection (the conversation is read again by
    /// `reload`) and a chat that is not started is dropped.
    pub fn leave(&mut self) {
        self.terminal.close();
        self.draft = None;
    }

    /// Shows the chat with this id.
    pub fn open(&mut self, id: i64, settings: &Settings) -> Task<AppMessage> {
        self.terminal.close();
        self.open = Some(Open::new(id));
        self.draft = None;
        let events = load::<OnPage>(settings.clone(), id, 0);
        // The pickers under the field show the chat's model and reasoning, or its agent's.
        if self.agents.is_empty() {
            return Task::batch([events, load_choices(settings)]);
        }
        events
    }

    /// A `run_event` of the stream: the chat's status follows the agent, and the open chat takes the event. Whether
    /// the open chat's judgements are to be read again, as a check of it arrived.
    pub fn on_event(&mut self, run: i64, event: &RunEvent) -> bool {
        if matches!(event.kind.as_str(), "agent_settled" | "moebius_error")
            && let Some(chat) = self.list.iter_mut().find(|c| c.id == run)
        {
            chat.answered = chat.answered.max(event.seq);
        }
        if let Some(chat) = self.list.iter_mut().find(|c| c.id == run && c.is_open()) {
            match event.kind.as_str() {
                "agent_start" => chat.status = "running".to_string(),
                "agent_settled" if event.data["aborted"] != true => {
                    chat.status = "waiting".to_string();
                }
                _ => {}
            }
        }
        let Some(open) = self.open.as_mut().filter(|open| open.thread.id == run) else {
            return false;
        };
        open.thread.push(event.clone());
        event.kind == "tool_check"
    }

    /// Scrolls the open chat `run` to its end, if it is open and followed.
    pub fn follow(&self, run: i64) -> Task<AppMessage> {
        match &self.open {
            Some(open) if open.thread.id == run && open.follow => {
                iced::widget::operation::snap_to_end(CONVERSATION)
            }
            _ => Task::none(),
        }
    }

    /// Reads the chats again.
    pub fn reload_list(&self, settings: &Settings) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        Task::perform(async move { client.chats().await }, |chats| {
            AppMessage::Chat(Message::Listed(chats))
        })
    }

    /// Whether the run `run` is a chat of the list.
    pub fn lists(&self, run: i64) -> bool {
        self.chat(run).is_some()
    }

    /// Reads the open chat's judgements again.
    pub fn reload_judgements(&self, settings: &Settings) -> Task<AppMessage> {
        match &self.open {
            Some(open) => load_judgements::<OnPage>(settings.clone(), open.thread.id),
            None => Task::none(),
        }
    }

    /// A run ended, as the stream says.
    pub fn on_ended(&mut self, run: i64, status: &str) {
        if let Some(chat) = self.list.iter_mut().find(|chat| chat.id == run) {
            chat.status = status.to_string();
        }
        if let Some(open) = self.open.as_mut().filter(|open| open.thread.id == run) {
            open.confirming = false;
            self.terminal.close();
        }
    }

    /// The terminal's connection and keys, while the open chat shows it.
    pub fn terminal_subscription(&self, settings: &Settings) -> Subscription<AppMessage> {
        match &self.open {
            Some(open) => self.terminal.subscription(open.thread.id, settings),
            None => Subscription::none(),
        }
    }

    fn chat(&self, id: i64) -> Option<&Chat> {
        self.list.iter().find(|chat| chat.id == id)
    }

    /// Shows the terminal of the chat started from a draft's Terminal button, once the chat waits and is open.
    fn show_started_terminal(&mut self) {
        let Some(id) = self.terminal_next else {
            return;
        };
        if self.opened() != Some(id) {
            self.terminal_next = None;
        } else if self.chat(id).is_some_and(|chat| chat.status == "waiting") {
            self.terminal_next = None;
            self.terminal.open();
        }
    }
}
