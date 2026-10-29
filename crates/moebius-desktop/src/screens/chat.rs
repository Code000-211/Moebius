//! The chat screen: the chats with agents, newest first, and the chat chosen on the right. Its conversation is the
//! live view's (`Conversation`, drawn by `transcript`) with the agent's text as Markdown; under it stand the change the
//! chat offered, to merge from there, and the field to write in. The state follows one chat: a load of its events, then
//! the stream's `run_event`s. The list of chats is read from the server and kept current by the same stream. Under the
//! field to write in stand small pickers: for a new chat its agent, place, model and reasoning level, for a started one
//! its model and reasoning level, which change it while it waits.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::marker::PhantomData;

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, markdown, pick_list, row, scrollable, space, text, text_input,
};
use iced::{Alignment, Border, Element, Length, Padding, Subscription, Task, border};

use crate::app::Message as AppMessage;
use crate::client::{
    Agent, Available, Change, Chat, Client, ClientError, InboxItem, Judgement, NewChat, Run,
    RunEvent, Skill, Snapshot,
};
use crate::screens::agents::{menu_style, pick_style};
use crate::screens::inbox::{self, Action, Inbox};
use crate::screens::live::say;
use crate::settings::Settings;
use crate::widgets::chat_terminal::{self, ChatTerminal};
use crate::widgets::conversation::{Conversation, Entry, Said};
use crate::widgets::page::{self, Tone, tag};
use crate::{screens::connect, theme, widgets::steps, widgets::transcript};

/// The agent a chat is with unless another is named.
pub const DEFAULT_AGENT: &str = "Moebius";

/// The agent of a grilling, which needs a repository to end in issues of.
const GRILLING_AGENT: &str = "Grilling-partner";

/// The reasoning levels a chat can be given, as Pi-Bolt takes them.
const LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

#[derive(Debug, Clone)]
pub enum Message {
    /// The chats were read.
    Listed(Result<Vec<Chat>, ClientError>),
    /// The events of the chat with this id were read, those after the ones already held.
    Loaded(i64, Result<Vec<RunEvent>, ClientError>),
    /// The judgements of the tool calls of the chat with this id were read.
    Judged(i64, Result<Vec<Judgement>, ClientError>),
    Select(i64),
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
    /// Show the open chat's terminal (Pi-Bolt's own interface) instead of its conversation, or the conversation again.
    ShowTerminal(bool),
    /// Something the terminal did.
    Terminal(chat_terminal::Message),
}

/// Where a chat's conversation is shown: the chat page or the floating chat. It says what the conversation's clicks
/// and reads become.
pub trait Place: 'static {
    /// A step of the agent was clicked; the number is its entry's.
    fn fold(step: usize) -> AppMessage;
    /// A link of the agent's text was clicked.
    fn link(uri: String) -> AppMessage;
    /// The events of the chat were read, those after the ones already held.
    fn loaded(chat: i64, events: Result<Vec<RunEvent>, ClientError>) -> AppMessage;
    /// The judgements of the chat's tool calls were read.
    fn judged(chat: i64, judgements: Result<Vec<Judgement>, ClientError>) -> AppMessage;
}

/// The chat page.
pub struct OnPage;

impl Place for OnPage {
    fn fold(step: usize) -> AppMessage {
        AppMessage::Chat(Message::Fold(step))
    }

    fn link(uri: String) -> AppMessage {
        AppMessage::Chat(Message::LinkClicked(uri))
    }

    fn loaded(chat: i64, events: Result<Vec<RunEvent>, ClientError>) -> AppMessage {
        AppMessage::Chat(Message::Loaded(chat, events))
    }

    fn judged(chat: i64, judgements: Result<Vec<Judgement>, ClientError>) -> AppMessage {
        AppMessage::Chat(Message::Judged(chat, judgements))
    }
}

/// The conversation of one chat as it is followed: what was said and done, who decided its tool calls, the agent's
/// texts parsed as Markdown and the steps unfolded. The chat page and the floating chat each hold one.
pub struct Thread {
    pub id: i64,
    conversation: Conversation,
    /// The judgements of the chat's tool calls, oldest first, which say who decided each check.
    judgements: Vec<Judgement>,
    /// The agent's texts parsed, by the index of the entry, with the text they were parsed from.
    markdown: HashMap<usize, (String, Vec<markdown::Item>)>,
    /// The steps that are unfolded, by the index of their entry.
    unfolded: HashSet<usize>,
}

impl Thread {
    pub fn new(id: i64) -> Self {
        Thread {
            id,
            conversation: Conversation::default(),
            judgements: Vec::new(),
            markdown: HashMap::new(),
            unfolded: HashSet::new(),
        }
    }

    /// The `seq` of the last event held.
    pub fn last_seq(&self) -> i64 {
        self.conversation.last_seq()
    }

    pub fn push(&mut self, event: RunEvent) {
        self.conversation.push(event);
        self.conversation.name_deciders(&self.judgements);
        for (index, entry) in self.conversation.entries().iter().enumerate() {
            if let Entry::Text(text) = entry
                && self
                    .markdown
                    .get(&index)
                    .is_none_or(|(held, _)| held != text)
            {
                let items = markdown::parse(text).collect();
                self.markdown.insert(index, (text.clone(), items));
            }
        }
    }

    /// Who decided the chat's tool calls, as read again.
    pub fn judge(&mut self, judgements: Vec<Judgement>) {
        self.judgements = judgements;
        self.conversation.name_deciders(&self.judgements);
    }

    /// Unfolds the step when folded and folds it when unfolded.
    pub fn fold(&mut self, step: usize) {
        if !self.unfolded.remove(&step) {
            self.unfolded.insert(step);
        }
    }

    /// The conversation: the agent's texts as Markdown, the person's messages on the right, and the steps that follow
    /// one another as one box, the last one working while the chat's `status` says the agent works.
    pub fn view<P: Place>(&self, status: &str, now: i64) -> Element<'_, AppMessage> {
        let conversation = &self.conversation;
        let entries = conversation.entries();
        if entries.is_empty() {
            return page::body("Nothing has been said in this chat yet.");
        }
        let working = matches!(status, "starting" | "running");
        let mut views: Vec<Element<'_, AppMessage>> = Vec::new();
        let mut index = 0;
        while index < entries.len() {
            match &entries[index] {
                Entry::Text(_) => views.push(agent_text::<P>(self.markdown.get(&index))),
                Entry::Person(said, message) => views.push(person_view(*said, message)),
                Entry::Error(message) => views.push(transcript::error(message)),
                Entry::Thinking(_) | Entry::Tool(_) => {
                    // The steps that follow one another are one box.
                    let end = entries[index..]
                        .iter()
                        .position(|entry| !matches!(entry, Entry::Thinking(_) | Entry::Tool(_)))
                        .map_or(entries.len(), |len| index + len);
                    let last = end == entries.len();
                    let stops = if last {
                        if working { now } else { conversation.last_at() }
                    } else {
                        conversation.started(end)
                    };
                    views.push(steps::steps_box(
                        &entries[index..end],
                        index,
                        last && working,
                        stops - conversation.started(index),
                        &self.unfolded,
                        P::fold,
                        false,
                    ));
                    index = end;
                    continue;
                }
            }
            index += 1;
        }
        column(views).spacing(12).width(Length::Fill).into()
    }
}

/// The chat being read.
struct Open {
    thread: Thread,
    message: String,
    /// Whether "Stop" asks to be confirmed.
    confirming: bool,
    /// The last thing that went wrong, as a short line.
    notice: Option<String>,
}

impl Open {
    fn new(id: i64) -> Self {
        Open {
            thread: Thread::new(id),
            message: String::new(),
            confirming: false,
            notice: None,
        }
    }
}

/// A chat that is not started: who it is with, where, on which model and reasoning level, and the first message being
/// written. It is nothing on the server and not in the list; the first Send, or Terminal, starts the chat, and leaving
/// drops the draft.
struct Draft {
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
}

impl Draft {
    fn new() -> Self {
        Draft {
            agent: DEFAULT_AGENT.to_string(),
            repository: None,
            harness: false,
            model: None,
            thinking: None,
            message: String::new(),
            sending: false,
            notice: None,
        }
    }

    /// Whether the chat can start: a grilling needs a repository.
    fn ready(&self) -> bool {
        self.agent != GRILLING_AGENT || self.repository.is_some()
    }

    /// The chat to start, with `message` or without one.
    fn new_chat(&self, message: Option<String>) -> NewChat {
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
}

/// Reads the events of the chat after `after`, and who decided its tool calls, for the conversation shown at `P`.
pub fn load<P: Place>(settings: Settings, chat: i64, after: i64) -> Task<AppMessage> {
    let events = Task::perform(
        {
            let settings = settings.clone();
            async move { Client::new(settings).run_events(chat, after).await }
        },
        move |events| P::loaded(chat, events),
    );
    Task::batch([events, load_judgements::<P>(settings, chat)])
}

/// Reads who decided the tool calls of the chat, for the conversation shown at `P`.
pub fn load_judgements<P: Place>(settings: Settings, chat: i64) -> Task<AppMessage> {
    Task::perform(
        async move { Client::new(settings).tool_judgements(chat).await },
        move |judgements| P::judged(chat, judgements),
    )
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

    /// How many chats of the list wait for the person's next message.
    pub fn waiting(&self) -> usize {
        self.listed()
            .filter(|chat| chat.status == "waiting")
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
        let (id, seq) = (open.thread.id, open.thread.last_seq());
        let Some(chat) = self.list.iter_mut().find(|chat| chat.id == id) else {
            return Task::none();
        };
        if seq <= chat.seen {
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

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        match message {
            Message::Listed(Ok(chats)) => {
                self.list = chats;
                self.show_started_terminal();
            }
            Message::Listed(Err(_)) => {}
            Message::Loaded(id, Ok(events)) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.thread.id == id) {
                    open.notice = None;
                    for event in events {
                        open.thread.push(event);
                    }
                }
            }
            Message::Loaded(id, Err(e)) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.thread.id == id) {
                    open.notice = Some(say(&e));
                }
            }
            Message::Judged(id, Ok(judgements)) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.thread.id == id) {
                    open.thread.judge(judgements);
                }
            }
            Message::Judged(_, Err(_)) => {}
            Message::Select(id) => return self.open(id, settings),
            Message::ShowTerminal(true) => self.terminal.open(),
            Message::ShowTerminal(false) => {
                self.terminal.close();
                if let Some(open) = &self.open {
                    return load::<OnPage>(
                        settings.clone(),
                        open.thread.id,
                        open.thread.last_seq(),
                    );
                }
            }
            Message::Terminal(message) => self.terminal.update(message),
            Message::Edited(text) => {
                if let Some(draft) = &mut self.draft {
                    draft.message = text;
                } else if let Some(open) = &mut self.open {
                    open.message = text;
                }
            }
            Message::Send => {
                if let Some(draft) = &mut self.draft {
                    if draft.message.trim().is_empty() || draft.sending || !draft.ready() {
                        return Task::none();
                    }
                    draft.sending = true;
                    draft.notice = None;
                    self.starting_terminal = false;
                    let chat = draft.new_chat(Some(draft.message.clone()));
                    return request(settings, Message::Started, move |client| async move {
                        client.start_chat(&chat).await
                    });
                }
                if let Some(open) = &self.open
                    && !open.message.trim().is_empty()
                    && let Some(chat) = self.chat(open.thread.id)
                {
                    let (id, message) = (open.thread.id, open.message.clone());
                    // Writing to an ended chat goes on in a new chat that is told this one.
                    if !chat.is_open() {
                        return request(settings, Message::Continued, move |client| async move {
                            client.continue_chat(id, &message).await
                        });
                    }
                    return request(settings, Message::Sent, move |client| async move {
                        client.say(id, &message, None).await
                    });
                }
            }
            Message::Continued(Ok(run)) => {
                return Task::batch([self.open(run.id, settings), self.reload(settings)]);
            }
            Message::Continued(Err(e)) => {
                if let Some(open) = &mut self.open {
                    open.notice = Some(say(&e));
                }
            }
            Message::Sent(result) => {
                if let Some(open) = &mut self.open {
                    match result {
                        Ok(_) => {
                            open.message.clear();
                            open.notice = None;
                        }
                        Err(e) => open.notice = Some(say(&e)),
                    }
                }
            }
            Message::AskStop => self.set_confirming(true),
            Message::KeepGoing => self.set_confirming(false),
            Message::Stop => {
                self.set_confirming(false);
                if let Some(open) = &self.open {
                    let id = open.thread.id;
                    return request(settings, Message::Ended, move |client| async move {
                        client.stop(id).await
                    });
                }
            }
            Message::End => {
                if let Some(open) = &self.open {
                    let id = open.thread.id;
                    return request(settings, Message::Ended, move |client| async move {
                        client.end_chat(id).await
                    });
                }
            }
            Message::Ended(result) => {
                if let Some(open) = &mut self.open {
                    open.notice = result.err().map(|e| say(&e));
                }
            }
            Message::Offer => {
                if let Some(open) = &self.open {
                    let id = open.thread.id;
                    return request(settings, Message::Offered, move |client| async move {
                        client.offer_chat(id).await
                    });
                }
            }
            Message::Offered(result) => {
                if let Some(open) = &mut self.open {
                    open.notice = Some(match result {
                        Ok(change) => format!(
                            "Offered as a change on {}: it waits in the Inbox, ready to merge.",
                            change.branch
                        ),
                        Err(e) => say(&e),
                    });
                }
                return self.reload(settings);
            }
            Message::NewChat => return self.open_draft(settings),
            Message::Agents(Ok(agents)) => {
                // An agent whose files are broken is listed without a model and cannot be chosen.
                self.agents = agents
                    .into_iter()
                    .filter(|agent| !agent.model.is_empty())
                    .collect();
                // The Moebius agent is the first, if the server can run it.
                if let (Some(draft), Some(first)) = (&mut self.draft, self.agents.first())
                    && !self.agents.iter().any(|agent| agent.name == draft.agent)
                {
                    draft.agent = first.name.clone();
                }
            }
            Message::Agents(Err(e)) => {
                if let Some(draft) = &mut self.draft {
                    draft.notice = Some(say(&e));
                }
            }
            Message::Models(Ok(available)) => {
                let mut models: Vec<String> = available.models.into_iter().map(|m| m.id).collect();
                models.sort();
                models.dedup();
                self.models = models;
            }
            Message::Models(Err(_)) => {}
            Message::AgentChosen(agent) => {
                if let Some(draft) = &mut self.draft {
                    // The model and reasoning follow the agent unless the person chose others.
                    draft.agent = agent;
                }
            }
            Message::Skills(skills) => {
                self.has_harness = skills.is_ok_and(|skills| !skills.is_empty());
            }
            Message::RepositoryChosen(repository) => {
                if let Some(draft) = &mut self.draft {
                    draft.repository = repository;
                    draft.harness = false;
                }
            }
            Message::HarnessChosen => {
                if let Some(draft) = self.draft.as_mut().filter(|_| self.has_harness) {
                    draft.repository = None;
                    draft.harness = true;
                }
            }
            Message::ModelChosen(model) => return self.choose(settings, Some(model), None),
            Message::ThinkingChosen(level) => return self.choose(settings, None, Some(level)),
            Message::SettingsChanged(result) => {
                if let Some(open) = &mut self.open {
                    open.notice = result.as_ref().err().map(say);
                }
                return self.reload_list(settings);
            }
            Message::StartInTerminal => {
                if let Some(draft) = &mut self.draft
                    && draft.ready()
                    && !draft.sending
                {
                    draft.sending = true;
                    draft.notice = None;
                    self.starting_terminal = true;
                    let chat = draft.new_chat(None);
                    return request(settings, Message::Started, move |client| async move {
                        client.start_chat(&chat).await
                    });
                }
            }
            Message::Started(Ok(run)) => {
                let task = Task::batch([self.open(run.id, settings), self.reload(settings)]);
                if std::mem::take(&mut self.starting_terminal) {
                    self.terminal_next = Some(run.id);
                    if run.status == "waiting" {
                        self.terminal_next = None;
                        self.terminal.open();
                    }
                }
                return task;
            }
            Message::Started(Err(e)) => {
                self.starting_terminal = false;
                if let Some(draft) = &mut self.draft {
                    draft.sending = false;
                    draft.notice = Some(say(&e));
                }
            }
            Message::Tick(now) => self.now = now,
            Message::MarkedRead => {}
            Message::Fold(step) => {
                if let Some(open) = &mut self.open {
                    open.thread.fold(step);
                }
            }
            Message::LinkClicked(uri) => {
                if let Some(open) = &mut self.open {
                    open.notice = Some(format!("Links are not opened from here: {uri}"));
                }
            }
        }
        Task::none()
    }

    /// A model or a reasoning level was chosen: the draft's, or, while the open chat waits, the chat's, which the
    /// server is asked to change. Choosing what the agent has keeps the agent's.
    fn choose(
        &mut self,
        settings: &Settings,
        model: Option<String>,
        thinking: Option<String>,
    ) -> Task<AppMessage> {
        if let Some(draft) = &mut self.draft {
            let agent = self.agents.iter().find(|agent| agent.name == draft.agent);
            if let Some(model) = model {
                let agents = agent.is_some_and(|agent| agent.model == model);
                draft.model = (!agents).then_some(model);
            }
            if let Some(level) = thinking {
                let agents = agent.is_some_and(|agent| agent.thinking.as_ref() == Some(&level));
                draft.thinking = (!agents).then_some(level);
            }
            return Task::none();
        }
        let Some(open) = &self.open else {
            return Task::none();
        };
        if self
            .chat(open.thread.id)
            .is_none_or(|chat| chat.status != "waiting")
        {
            return Task::none();
        }
        let id = open.thread.id;
        request(
            settings,
            Message::SettingsChanged,
            move |client| async move {
                client
                    .change_chat_settings(id, model.as_deref(), thinking.as_deref())
                    .await
            },
        )
    }

    fn set_confirming(&mut self, confirming: bool) {
        if let Some(open) = &mut self.open {
            open.confirming = confirming;
        }
    }

    /// The page: the header, the list of chats, and the form of a new chat or the chat chosen.
    pub fn view<'a>(&'a self, snapshot: &'a Snapshot, inbox: &'a Inbox) -> Element<'a, AppMessage> {
        let line = match self.waiting() {
            0 => "Talk to an agent. What it builds in a repository is offered as a change.".into(),
            1 => "1 chat waits for your next message.".to_string(),
            n => format!("{n} chats wait for your next message."),
        };
        let header = page::header(
            "",
            "Chat",
            None,
            page::context(line),
            page::white_button("New chat", Some(AppMessage::Chat(Message::NewChat))),
        );
        let detail: Element<_> = match (&self.draft, &self.open) {
            (Some(draft), _) => self.draft_view(draft, snapshot),
            (None, Some(open)) => match self.chat(open.thread.id) {
                Some(chat) => self.chat_view(chat, open, snapshot, inbox),
                None => empty_view("Reading the chat…"),
            },
            (None, None) => empty_view("Choose a chat, or start a new one."),
        };
        column![
            header,
            row![self.list_view(snapshot), detail]
                .spacing(14)
                .height(Length::Fill),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }

    /// The chats, newest first, the chosen one marked.
    fn list_view<'a>(&'a self, snapshot: &'a Snapshot) -> Element<'a, AppMessage> {
        let chosen = self
            .open
            .as_ref()
            .filter(|_| self.draft.is_none())
            .map(|open| open.thread.id);
        let list: Element<_> = if self.listed().next().is_none() {
            page::body("No chat yet.")
        } else {
            column(self.listed().map(|chat| {
                let repository = place_name(snapshot, chat);
                chat_row(chat, repository, self.now, chosen == Some(chat.id))
            }))
            .spacing(8)
            .into()
        };
        container(scrollable(list).height(Length::Fill))
            .padding(12)
            .width(320)
            .height(Length::Fill)
            .style(|_| page::box_style())
            .into()
    }

    fn chat_view<'a>(
        &'a self,
        chat: &'a Chat,
        open: &'a Open,
        snapshot: &'a Snapshot,
        inbox: &'a Inbox,
    ) -> Element<'a, AppMessage> {
        let repository = place_name(snapshot, chat);
        // The title takes what the tag leaves, so a long one wraps instead of squeezing the tag.
        let title = row![
            text(chat.title.as_str())
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold))
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
            status_tag(&chat.status),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let who = match repository {
            Some(_) if chat.harness_repository => {
                format!("{} in the harness configuration", chat.agent)
            }
            Some(name) => format!("{} in {name}", chat.agent),
            None => format!("{}, no repository", chat.agent),
        };
        let mut head = row![
            column![title, page::context(who)]
                .spacing(2)
                .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if chat.is_open() {
            head = head.push(switch(chat, self.terminal.is_shown()));
        }
        head = head.push(actions(chat, open));
        if let Some(terminal) = self.terminal.view() {
            let content = column![container(head).padding([14, 18]), page::rule(), terminal];
            return container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| page::box_style())
                .into();
        }
        let conversation = scrollable(
            container(open.thread.view::<OnPage>(&chat.status, self.now))
                .padding(Padding::from([16, 16]))
                .width(Length::Fill),
        )
        .anchor_bottom()
        .height(Length::Fill);
        let mut content = column![
            container(head).padding([14, 18]),
            page::rule(),
            conversation
        ];
        let offered = offered_change(chat, snapshot);
        // Merge is what an ended chat with a change to merge is for, so it takes the primary place from Send.
        let merge_is_primary = !chat.is_open() && offered.is_some_and(|(_, item)| item.is_some());
        if let Some((change, item)) = offered {
            let offer = change_view(change, item, inbox, merge_is_primary);
            content = content.push(container(offer).padding([8, 16]));
        }
        container(
            content
                .push(page::rule())
                .push(self.compose_view(chat, open, !merge_is_primary)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| page::box_style())
        .into()
    }
}

/// Where the chat works: the name of its repository when the snapshot has it, or the harness configuration.
fn place_name<'a>(snapshot: &'a Snapshot, chat: &Chat) -> Option<&'a str> {
    if chat.harness_repository {
        return Some(HARNESS_PLACE);
    }
    snapshot
        .boards
        .iter()
        .find(|board| Some(board.repository.id) == chat.repository_id)
        .map(|board| board.repository.name.as_str())
}

/// How the harness configuration repository is called where a chat's place is named.
const HARNESS_PLACE: &str = "harness configuration";

/// What the chat's status says on its tag: the colour tells whether the agent works or waits for the person.
fn status_tag<'a>(status: &str) -> Element<'a, AppMessage> {
    let (label, tone) = match status {
        "starting" => ("Starting", Tone::Teal),
        "running" => ("Running", Tone::Teal),
        "waiting" => ("Waiting", Tone::Amber),
        "succeeded" => ("Ended", Tone::Plain),
        "failed" => ("Failed", Tone::Red),
        "stopped" => ("Stopped", Tone::Plain),
        other => (other, Tone::Plain),
    };
    tag(label.to_string(), tone)
}

/// A chat of the list: its title, status, agent and repository, and when it started.
fn chat_row<'a>(
    chat: &'a Chat,
    repository: Option<&'a str>,
    now: i64,
    chosen: bool,
) -> Element<'a, AppMessage> {
    let mut meta = vec![chat.agent.as_str()];
    meta.extend(repository);
    let content = column![
        row![
            status_tag(&chat.status),
            space::horizontal(),
            text(page::ago(now, chat.started_at))
                .size(theme::LABEL)
                .color(theme::TEXT_MUTED),
        ]
        .align_y(Alignment::Center),
        text(chat.title.as_str())
            .size(theme::BODY)
            .font(theme::manrope(Weight::Semibold))
            .wrapping(Wrapping::WordOrGlyph),
        text(meta.join(" · "))
            .size(theme::LABEL)
            .color(theme::TEXT_SECONDARY),
    ]
    .spacing(6);
    button(content)
        .width(Length::Fill)
        .padding([10, 12])
        .on_press(AppMessage::Chat(Message::Select(chat.id)))
        .style(move |_, status| {
            let (fill, line) = match (chosen, status) {
                (true, _) => (theme::LANE, theme::TEXT),
                (false, button::Status::Hovered) => (theme::LANE, theme::LINE),
                (false, _) => (theme::SURFACE, theme::TRACK),
            };
            button::Style {
                background: Some(fill.into()),
                text_color: theme::TEXT,
                border: Border {
                    color: line,
                    width: 1.0,
                    radius: theme::RADIUS_CARD.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

/// The switch between a chat's conversation and its terminal, for a chat that is not over (the caller leaves it out
/// for one that is). The terminal opens once the agent has answered: while it works its process has the worktree.
fn switch<'a>(chat: &Chat, terminal: bool) -> Element<'a, AppMessage> {
    let chip = |label: &str, selected: bool, shown: bool| {
        page::chip(
            label.to_string(),
            selected,
            false,
            AppMessage::Chat(Message::ShowTerminal(shown)),
        )
    };
    let mut switch = row![chip("Chat", !terminal, false)]
        .spacing(6)
        .align_y(Alignment::Center);
    if terminal || chat.status == "waiting" {
        switch = switch.push(chip("Terminal", terminal, true));
    } else {
        switch = switch.push(iced::widget::tooltip(
            container(text("Terminal").size(theme::BODY).color(theme::TEXT_MUTED)).padding([4, 12]),
            container(text("Opens when the agent has answered").size(theme::LABEL))
                .padding([4, 8])
                .style(|_| page::box_style()),
            iced::widget::tooltip::Position::Bottom,
        ));
    }
    switch.into()
}

/// The right of the chat's head, while it lasts: offering what its worktree changes, stopping its agent while it works
/// (or the question that confirms stopping), and ending a grilling, which ends in the issues it proposes. Other chats
/// are not ended here: they wait for the next message as long as the person likes.
fn actions<'a>(chat: &Chat, open: &Open) -> Element<'a, AppMessage> {
    let mut actions = row![].spacing(10).align_y(Alignment::Center);
    if !chat.is_open() {
        return actions.into();
    }
    if open.confirming {
        actions = actions
            .push(
                text("Stop this chat?")
                    .size(theme::CONTEXT)
                    .color(theme::TEXT_SECONDARY),
            )
            .push(page::white_button(
                "Keep going",
                Some(AppMessage::Chat(Message::KeepGoing)),
            ))
            .push(page::white_button(
                "Stop chat",
                Some(AppMessage::Chat(Message::Stop)),
            ));
    } else {
        if chat.changed_files > 0 {
            actions = actions.push(offer_button(chat));
        }
        if chat.agent == GRILLING_AGENT {
            actions = actions.push(page::white_button(
                "End chat",
                Some(AppMessage::Chat(Message::End)),
            ));
        }
        if chat.is_working() {
            actions = actions.push(page::white_button(
                "Stop",
                Some(AppMessage::Chat(Message::AskStop)),
            ));
        }
    }
    actions.into()
}

/// "Offer changes (n files)": pressed, the chat's work is offered as a change. While the agent works its worktree may be
/// half edited, so the button is muted and says on hover when it can be pressed.
fn offer_button<'a>(chat: &Chat) -> Element<'a, AppMessage> {
    let label = match chat.changed_files {
        1 => "Offer changes (1 file)".to_string(),
        n => format!("Offer changes ({n} files)"),
    };
    if !chat.is_working() {
        return page::white_button(label, Some(AppMessage::Chat(Message::Offer)));
    }
    iced::widget::tooltip(
        page::white_button(label, None),
        container(text("Offer once the agent has answered").size(theme::LABEL))
            .padding([4, 8])
            .style(|_| page::box_style()),
        iced::widget::tooltip::Position::Bottom,
    )
    .into()
}

/// What stands in the right pane when no chat is chosen.
fn empty_view(line: &str) -> Element<'_, AppMessage> {
    container(page::body(line))
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| page::box_style())
        .into()
}

/// The viewer of the agent's Markdown: a link click is a message of the place it is shown at, and a code block is
/// drawn as a file is.
struct Markdown<P>(PhantomData<P>);

impl<'a, P: Place> markdown::Viewer<'a, AppMessage> for Markdown<P> {
    fn on_link_click(uri: markdown::Uri) -> AppMessage {
        P::link(uri)
    }

    fn code_block(
        &self,
        _settings: markdown::Settings,
        language: Option<&'a str>,
        code: &'a str,
        _lines: &'a [markdown::Text],
    ) -> Element<'a, AppMessage> {
        transcript::code_block(language.unwrap_or("txt"), code)
    }
}

/// The sizes and fonts of the agent's Markdown.
fn markdown_settings() -> markdown::Settings {
    let style = markdown::Style {
        font: theme::MANROPE,
        inline_code_highlight: markdown::Highlight {
            background: theme::HEAD_PILL.into(),
            border: border::rounded(4),
        },
        inline_code_padding: Padding::from([0.0, 3.0]),
        inline_code_color: theme::TEXT,
        inline_code_font: theme::MONO,
        code_block_font: theme::MONO,
        link_color: theme::TEAL,
    };
    let mut settings = markdown::Settings::with_text_size(15, style);
    settings.h1_size = 20.0.into();
    settings.h2_size = 18.0.into();
    settings.h3_size = 16.5.into();
    settings.h4_size = 15.0.into();
    settings.code_size = theme::CODE.into();
    settings.spacing = 10.0.into();
    settings
}

/// A text of the agent, as Markdown once it is parsed.
fn agent_text<P: Place>(parsed: Option<&(String, Vec<markdown::Item>)>) -> Element<'_, AppMessage> {
    let items = parsed.map_or(&[][..], |(_, items)| items.as_slice());
    container(markdown::view_with(
        items,
        markdown_settings(),
        &Markdown::<P>(PhantomData),
    ))
    .max_width(720)
    .into()
}

/// A message of the person: a box on the right.
fn person_view(said: Said, message: &str) -> Element<'_, AppMessage> {
    let mut content = column![].spacing(4);
    if said == Said::Steered {
        content = content.push(
            text("You steered the chat")
                .size(theme::LABEL)
                .font(theme::manrope(Weight::Bold))
                .color(theme::TEXT_SECONDARY),
        );
    }
    let bubble = container(
        content.push(
            text(message)
                .size(theme::BODY)
                .color(theme::TEXT)
                .wrapping(Wrapping::WordOrGlyph),
        ),
    )
    .padding([8, 12])
    .max_width(560)
    .style(|_| container::Style {
        background: Some(theme::LANE.into()),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_CARD.into(),
        },
        ..container::Style::default()
    });
    container(bubble).align_right(Length::Fill).into()
}

/// The change the chat offered, found by its branch among the board's changes, and the inbox item that waits for it
/// to be merged; none while the chat has offered nothing.
fn offered_change<'a>(
    chat: &Chat,
    snapshot: &'a Snapshot,
) -> Option<(&'a Change, Option<&'a InboxItem>)> {
    let branch = chat.branch();
    let change = snapshot
        .boards
        .iter()
        .flat_map(|board| &board.changes)
        .chain(&snapshot.harness_changes)
        .rfind(|change| change.branch == branch)?;
    let item = snapshot
        .inbox
        .iter()
        .find(|item| item.chat() == Some(chat.id));
    Some((change, item))
}

/// The change a chat offered: its state, its branch and, while a person can merge it, the Merge button.
fn change_view<'a>(
    change: &'a Change,
    item: Option<&InboxItem>,
    inbox: &'a Inbox,
    merge_is_primary: bool,
) -> Element<'a, AppMessage> {
    let (label, tone) = match change.state.as_str() {
        "merged" => ("Merged", Tone::Teal),
        "closed" => ("Closed", Tone::Plain),
        _ => ("Change offered", Tone::Amber),
    };
    let mut head = row![
        tag(label.to_string(), tone),
        text(change.branch.as_str())
            .font(theme::MONO)
            .size(theme::CODE)
            .wrapping(Wrapping::WordOrGlyph),
        space::horizontal(),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if let Some(item) = item {
        if let Some(progress) = inbox.status(item.id) {
            head = head.push(
                text(progress)
                    .size(theme::LABEL)
                    .color(theme::TEXT_SECONDARY),
            );
        }
        let merge = Action::Merge { change: change.id };
        let message = Some(AppMessage::Inbox(inbox::Message::Act(item.id, merge)));
        head = head.push(if merge_is_primary {
            page::primary_button("Merge", message)
        } else {
            page::white_button("Merge", message)
        });
    }
    container(head)
        .padding([10, 12])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::LANE.into()),
            border: Border {
                color: theme::TRACK,
                width: 1.0,
                radius: theme::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// The field to write a message in; Enter sends when `send` is a message.
fn message_field(value: &str, send: Option<AppMessage>) -> Element<'_, AppMessage> {
    text_input("Write to the agent", value)
        .id(MESSAGE_FIELD)
        .on_input(|text| AppMessage::Chat(Message::Edited(text)))
        .on_submit_maybe(send)
        .size(theme::BODY)
        .padding([10, 12])
        .style(connect::input_style)
        .into()
}

/// Reads the agents and the models a chat's pickers offer.
fn load_choices(settings: &Settings) -> Task<AppMessage> {
    let (agents, models) = (Client::new(settings.clone()), Client::new(settings.clone()));
    Task::batch([
        Task::perform(async move { agents.agents().await }, |agents| {
            AppMessage::Chat(Message::Agents(agents))
        }),
        Task::perform(
            async move { models.available_models(false).await },
            |models| AppMessage::Chat(Message::Models(models)),
        ),
    ])
}

/// Where a new chat works, as its picker offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PlaceChoice {
    Nowhere,
    Repository(i64, String),
    Harness,
}

impl fmt::Display for PlaceChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaceChoice::Nowhere => f.write_str("No repository"),
            PlaceChoice::Repository(_, name) => f.write_str(name),
            PlaceChoice::Harness => f.write_str("Harness configuration"),
        }
    }
}

/// A small picker of the row under the field to write in, headed by `label`; without `on_pick` it only shows what is
/// chosen, muted.
fn picker<'a, T>(
    label: &'static str,
    choices: Vec<T>,
    chosen: Option<T>,
    placeholder: &'static str,
    on_pick: Option<fn(T) -> Message>,
) -> Element<'a, AppMessage>
where
    T: fmt::Display + Clone + PartialEq + 'a,
{
    let label = text(label)
        .size(theme::LABEL)
        .font(theme::manrope(Weight::Semibold))
        .color(theme::TEXT_MUTED);
    let field: Element<'a, AppMessage> = match on_pick {
        Some(on_pick) => pick_list(choices, chosen, move |choice| {
            AppMessage::Chat(on_pick(choice))
        })
        .placeholder(placeholder)
        .text_size(theme::LABEL)
        .padding([3, 8])
        .style(pick_style)
        .menu_style(menu_style)
        .into(),
        None => container(
            text(chosen.map_or(placeholder.to_string(), |chosen| chosen.to_string()))
                .size(theme::LABEL)
                .color(theme::TEXT_MUTED),
        )
        .padding([3, 8])
        .style(|_| container::Style {
            border: Border {
                color: theme::TRACK,
                width: 1.0,
                radius: theme::RADIUS_CONTROL.into(),
            },
            ..container::Style::default()
        })
        .into(),
    };
    row![label, field]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}

impl Chats {
    /// The model picker: the models the upstreams serve, and `current` as it is when they do not list it.
    fn model_picker<'a>(&self, current: Option<&str>, enabled: bool) -> Element<'a, AppMessage> {
        let mut models = self.models.clone();
        if let Some(current) = current
            && !models.iter().any(|model| model == current)
        {
            models.push(current.to_string());
        }
        let on_pick = enabled.then_some(Message::ModelChosen as fn(String) -> Message);
        picker("Model", models, current.map(String::from), "Model", on_pick)
    }

    /// The reasoning picker: the levels Pi-Bolt takes, `current` chosen.
    fn reasoning_picker<'a>(
        &self,
        current: Option<&str>,
        enabled: bool,
    ) -> Element<'a, AppMessage> {
        let levels: Vec<String> = LEVELS.iter().map(|level| level.to_string()).collect();
        let on_pick = enabled.then_some(Message::ThinkingChosen as fn(String) -> Message);
        picker(
            "Reasoning",
            levels,
            current.map(String::from),
            "Not set",
            on_pick,
        )
    }

    /// The agent `name` as the server lists it.
    fn agent(&self, name: &str) -> Option<&Agent> {
        self.agents.iter().find(|agent| agent.name == name)
    }

    /// The empty chat of a draft: a hint where the conversation will be, the field for the first message with Send
    /// and Terminal, and under them the pickers of the chat's agent, place, model and reasoning.
    fn draft_view<'a>(
        &'a self,
        draft: &'a Draft,
        snapshot: &'a Snapshot,
    ) -> Element<'a, AppMessage> {
        let place = if draft.harness {
            Some(HARNESS_PLACE)
        } else {
            snapshot
                .boards
                .iter()
                .find(|board| Some(board.repository.id) == draft.repository)
                .map(|board| board.repository.name.as_str())
        };
        let who = match place {
            Some(name) if draft.harness => format!("{} in the {name}", draft.agent),
            Some(name) => format!("{} in {name}", draft.agent),
            None => format!("{}, no repository", draft.agent),
        };
        let head = column![
            text("New chat")
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold)),
            page::context(who),
        ]
        .spacing(2);
        let ready = draft.ready() && !draft.sending;
        let can_send = ready && !draft.message.trim().is_empty();
        let send = can_send.then_some(AppMessage::Chat(Message::Send));
        let terminal = ready.then_some(AppMessage::Chat(Message::StartInTerminal));
        let hint = if draft.harness {
            "The agent works on a branch of the harness configuration repository: agents, skills, extensions, MCP servers and rules. \"Offer changes\" offers what it built as a change, if every agent still works with it."
        } else if draft.agent == GRILLING_AGENT {
            "The Grilling partner questions your idea in a repository. Ending the chat puts the issues it proposes in the Inbox, to confirm."
        } else {
            "With a repository the agent works on its own branch. \"Offer changes\" offers what it built as a change."
        };
        let note = match &draft.notice {
            Some(notice) => notice.as_str(),
            None if !draft.ready() => "A grilling needs a repository: choose one below.",
            None => "Write the first message to start the chat, or open it in the terminal.",
        };
        let agent = self.agent(&draft.agent);
        let names: Vec<String> = self.agents.iter().map(|agent| agent.name.clone()).collect();
        let mut places = vec![PlaceChoice::Nowhere];
        places.extend(snapshot.boards.iter().map(|board| {
            PlaceChoice::Repository(board.repository.id, board.repository.name.clone())
        }));
        if self.has_harness {
            places.push(PlaceChoice::Harness);
        }
        let chosen_place = if draft.harness {
            PlaceChoice::Harness
        } else {
            places
                .iter()
                .find(|place| matches!(place, PlaceChoice::Repository(id, _) if Some(*id) == draft.repository))
                .cloned()
                .unwrap_or(PlaceChoice::Nowhere)
        };
        let to_place: fn(PlaceChoice) -> Message = |place| match place {
            PlaceChoice::Nowhere => Message::RepositoryChosen(None),
            PlaceChoice::Repository(id, _) => Message::RepositoryChosen(Some(id)),
            PlaceChoice::Harness => Message::HarnessChosen,
        };
        let model = draft
            .model
            .as_deref()
            .or(agent.map(|agent| agent.model.as_str()));
        let thinking = draft
            .thinking
            .as_deref()
            .or(agent.and_then(|agent| agent.thinking.as_deref()));
        let pickers = row![
            picker(
                "Agent",
                names,
                Some(draft.agent.clone()),
                "Agent",
                Some(Message::AgentChosen as fn(String) -> Message),
            ),
            picker("Place", places, Some(chosen_place), "Place", Some(to_place)),
            self.model_picker(model, true),
            self.reasoning_picker(thinking, true),
        ]
        .spacing(14)
        .align_y(Alignment::Center);
        let compose = column![
            row![
                message_field(&draft.message, send.clone()),
                page::primary_button("Send", send),
                page::white_button("Terminal", terminal),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            pickers.wrap().vertical_spacing(8),
            text(note).size(theme::LABEL).color(theme::TEXT_MUTED),
        ]
        .spacing(8)
        .padding([12, 16]);
        container(column![
            container(head).padding([14, 18]),
            page::rule(),
            container(page::body(hint))
                .padding(16)
                .width(Length::Fill)
                .height(Length::Fill),
            page::rule(),
            compose,
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| page::box_style())
        .into()
    }

    /// The field to write in and the Send button, the primary one unless `send_is_primary` is false; under them the
    /// chat's model and reasoning, which change while it waits, and what the chat does with the message.
    fn compose_view<'a>(
        &'a self,
        chat: &'a Chat,
        open: &'a Open,
        send_is_primary: bool,
    ) -> Element<'a, AppMessage> {
        let can_send = !open.message.trim().is_empty();
        let send = can_send.then_some(AppMessage::Chat(Message::Send));
        let field = message_field(&open.message, send.clone());
        let hint = match (&open.notice, chat.status.as_str()) {
            (Some(notice), _) => notice.as_str(),
            (None, "starting" | "running") => {
                "The agent is working. Your message is queued and sent when it is done with this turn."
            }
            (None, "waiting") => "The agent answers at once.",
            (None, _) => {
                "This chat has ended. What you send starts a new chat with the same agent, which is told this one."
            }
        };
        let mut compose = column![
            row![
                field,
                if send_is_primary {
                    page::primary_button("Send", send)
                } else {
                    page::white_button("Send", send)
                }
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .padding([12, 16]);
        if chat.is_open() {
            let agent = self.agent(&chat.agent);
            let model = chat
                .model
                .as_deref()
                .or(agent.map(|agent| agent.model.as_str()));
            let thinking = chat
                .thinking
                .as_deref()
                .or(agent.and_then(|agent| agent.thinking.as_deref()));
            let waiting = chat.status == "waiting";
            compose = compose.push(
                row![
                    self.model_picker(model, waiting),
                    self.reasoning_picker(thinking, waiting),
                ]
                .spacing(14)
                .align_y(Alignment::Center),
            );
        }
        compose
            .push(text(hint).size(theme::LABEL).color(theme::TEXT_MUTED))
            .into()
    }
}
