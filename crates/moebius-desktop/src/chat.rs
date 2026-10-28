//! The chat screen: the chats with agents, newest first, and the chat chosen on the right. Its conversation is the
//! live view's (`Conversation`, drawn by `transcript`) with the agent's text as Markdown; under it stand the change the
//! chat offered, to merge from there, and the field to write in. The state follows one chat: a load of its events, then
//! the stream's `run_event`s. The list of chats is read from the server and kept current by the same stream.

use std::collections::HashMap;

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, markdown, row, scrollable, space, text, text_input};
use iced::{Alignment, Border, Element, Length, Padding, Task, border};

use crate::app::Message as AppMessage;
use crate::client::{
    Agent, Change, Chat, Client, ClientError, InboxItem, Judgement, Run, RunEvent, Snapshot,
};
use crate::conversation::{Conversation, Entry, Said};
use crate::inbox::{self, Action, Inbox};
use crate::live::say;
use crate::page::{self, Tone, tag};
use crate::settings::Settings;
use crate::{connect, theme, transcript};

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
    Edited(String),
    Send,
    /// The server took the message, or said why not.
    Sent(Result<Run, ClientError>),
    AskStop,
    KeepGoing,
    Stop,
    End,
    /// The server answered stopping or ending the chat.
    Ended(Result<Run, ClientError>),
    /// Open the form of a new chat, or close it.
    NewChat(bool),
    /// The agents the form can offer were read.
    Agents(Result<Vec<Agent>, ClientError>),
    AgentChosen(String),
    RepositoryChosen(Option<i64>),
    FirstEdited(String),
    Start,
    Started(Result<Run, ClientError>),
    /// The clock, for how long ago a chat started.
    Tick(i64),
    /// A link of the agent's text was clicked.
    LinkClicked(String),
}

/// The chat being read.
struct Open {
    id: i64,
    conversation: Conversation,
    /// The judgements of the chat's tool calls, oldest first, which say who decided each check.
    judgements: Vec<Judgement>,
    /// The agent's texts parsed, by the index of the entry, with the text they were parsed from.
    markdown: HashMap<usize, (String, Vec<markdown::Item>)>,
    message: String,
    /// Whether "Stop" asks to be confirmed.
    confirming: bool,
    /// The last thing that went wrong, as a short line.
    notice: Option<String>,
}

impl Open {
    fn new(id: i64) -> Self {
        Open {
            id,
            conversation: Conversation::default(),
            judgements: Vec::new(),
            markdown: HashMap::new(),
            message: String::new(),
            confirming: false,
            notice: None,
        }
    }

    fn push(&mut self, event: RunEvent) {
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
}

/// The form of a new chat.
struct Form {
    agent: String,
    repository: Option<i64>,
    message: String,
    notice: Option<String>,
}

impl Form {
    /// Whether the chat can start: it has a first message, and a grilling a repository.
    fn ready(&self) -> bool {
        !self.message.trim().is_empty()
            && (self.agent != GRILLING_AGENT || self.repository.is_some())
    }
}

pub struct Chats {
    /// The chats, newest first.
    list: Vec<Chat>,
    /// The agents a new chat can be with, as the server last listed them.
    agents: Vec<Agent>,
    open: Option<Open>,
    form: Option<Form>,
    /// The time of the last tick, Unix seconds.
    now: i64,
}

/// Reads the events of the chat after `after`, and who decided its tool calls.
fn load(settings: Settings, chat: i64, after: i64) -> Task<AppMessage> {
    let events = Task::perform(
        {
            let settings = settings.clone();
            async move { Client::new(settings).run_events(chat, after).await }
        },
        move |events| AppMessage::Chat(Message::Loaded(chat, events)),
    );
    Task::batch([events, load_judgements(settings, chat)])
}

fn load_judgements(settings: Settings, chat: i64) -> Task<AppMessage> {
    Task::perform(
        async move { Client::new(settings).tool_judgements(chat).await },
        move |judgements| AppMessage::Chat(Message::Judged(chat, judgements)),
    )
}

/// Sends a request about a chat and tells `then` how it went.
fn request<F, Fut>(
    settings: &Settings,
    then: fn(Result<Run, ClientError>) -> Message,
    ask: F,
) -> Task<AppMessage>
where
    F: FnOnce(Client) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Run, ClientError>> + Send + 'static,
{
    let client = Client::new(settings.clone());
    Task::perform(ask(client), move |run| AppMessage::Chat(then(run)))
}

impl Chats {
    pub fn new(now: i64) -> Self {
        Chats {
            list: Vec::new(),
            agents: Vec::new(),
            open: None,
            form: None,
            now,
        }
    }

    /// The chats, newest first.
    pub fn list(&self) -> &[Chat] {
        &self.list
    }

    /// How many chats wait for the person's next message.
    pub fn waiting(&self) -> usize {
        self.list
            .iter()
            .filter(|chat| chat.status == "waiting")
            .count()
    }

    /// Reads the chats again, and what the open chat missed.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        let list = Task::perform(async move { client.chats().await }, |chats| {
            AppMessage::Chat(Message::Listed(chats))
        });
        match &self.open {
            Some(open) => Task::batch([
                list,
                load(settings.clone(), open.id, open.conversation.last_seq()),
            ]),
            None => list,
        }
    }

    /// Shows the chat with this id.
    pub fn open(&mut self, id: i64, settings: &Settings) -> Task<AppMessage> {
        self.open = Some(Open::new(id));
        self.form = None;
        load(settings.clone(), id, 0)
    }

    /// A `run_event` of the stream: the chat's status follows the agent, and the open chat takes the event. Whether
    /// the open chat's judgements are to be read again, as a check of it arrived.
    pub fn on_event(&mut self, run: i64, event: &RunEvent) -> bool {
        if let Some(chat) = self.list.iter_mut().find(|c| c.id == run && c.is_open()) {
            match event.kind.as_str() {
                "agent_start" => chat.status = "running".to_string(),
                "agent_settled" if event.data["aborted"] != true => {
                    chat.status = "waiting".to_string();
                }
                _ => {}
            }
        }
        let Some(open) = self.open.as_mut().filter(|open| open.id == run) else {
            return false;
        };
        open.push(event.clone());
        event.kind == "tool_check"
    }

    /// Reads the open chat's judgements again.
    pub fn reload_judgements(&self, settings: &Settings) -> Task<AppMessage> {
        match &self.open {
            Some(open) => load_judgements(settings.clone(), open.id),
            None => Task::none(),
        }
    }

    /// A run ended, as the stream says.
    pub fn on_ended(&mut self, run: i64, status: &str) {
        if let Some(chat) = self.list.iter_mut().find(|chat| chat.id == run) {
            chat.status = status.to_string();
        }
        if let Some(open) = self.open.as_mut().filter(|open| open.id == run) {
            open.confirming = false;
        }
    }

    fn chat(&self, id: i64) -> Option<&Chat> {
        self.list.iter().find(|chat| chat.id == id)
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        match message {
            Message::Listed(Ok(chats)) => {
                self.list = chats;
            }
            Message::Listed(Err(_)) => {}
            Message::Loaded(id, Ok(events)) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.id == id) {
                    open.notice = None;
                    for event in events {
                        open.push(event);
                    }
                }
            }
            Message::Loaded(id, Err(e)) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.id == id) {
                    open.notice = Some(say(&e));
                }
            }
            Message::Judged(id, Ok(judgements)) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.id == id) {
                    open.judgements = judgements;
                    open.conversation.name_deciders(&open.judgements);
                }
            }
            Message::Judged(_, Err(_)) => {}
            Message::Select(id) => return self.open(id, settings),
            Message::Edited(text) => {
                if let Some(open) = &mut self.open {
                    open.message = text;
                }
            }
            Message::Send => {
                if let Some(open) = &self.open
                    && !open.message.trim().is_empty()
                    && self.chat(open.id).is_some_and(Chat::is_open)
                {
                    let (id, message) = (open.id, open.message.clone());
                    return request(settings, Message::Sent, move |client| async move {
                        client.say(id, &message).await
                    });
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
                    let id = open.id;
                    return request(settings, Message::Ended, move |client| async move {
                        client.stop(id).await
                    });
                }
            }
            Message::End => {
                if let Some(open) = &self.open {
                    let id = open.id;
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
            Message::NewChat(shown) => {
                self.form = shown.then(|| Form {
                    agent: DEFAULT_AGENT.to_string(),
                    repository: None,
                    message: String::new(),
                    notice: None,
                });
                if shown {
                    let client = Client::new(settings.clone());
                    return Task::perform(async move { client.agents().await }, |agents| {
                        AppMessage::Chat(Message::Agents(agents))
                    });
                }
            }
            Message::Agents(Ok(agents)) => {
                // An agent whose files are broken is listed without a model and cannot be chosen.
                self.agents = agents
                    .into_iter()
                    .filter(|agent| !agent.model.is_empty())
                    .collect();
                // The Moebius agent is the first, if the server can run it.
                if let (Some(form), Some(first)) = (&mut self.form, self.agents.first())
                    && !self.agents.iter().any(|agent| agent.name == form.agent)
                {
                    form.agent = first.name.clone();
                }
            }
            Message::Agents(Err(e)) => {
                if let Some(form) = &mut self.form {
                    form.notice = Some(say(&e));
                }
            }
            Message::AgentChosen(agent) => {
                if let Some(form) = &mut self.form {
                    form.agent = agent;
                }
            }
            Message::RepositoryChosen(repository) => {
                if let Some(form) = &mut self.form {
                    form.repository = repository;
                }
            }
            Message::FirstEdited(message) => {
                if let Some(form) = &mut self.form {
                    form.message = message;
                }
            }
            Message::Start => {
                if let Some(form) = &self.form
                    && form.ready()
                {
                    let (agent, message, repository) =
                        (form.agent.clone(), form.message.clone(), form.repository);
                    return request(settings, Message::Started, move |client| async move {
                        client.start_chat(&agent, &message, repository).await
                    });
                }
            }
            Message::Started(Ok(run)) => {
                return Task::batch([self.open(run.id, settings), self.reload(settings)]);
            }
            Message::Started(Err(e)) => {
                if let Some(form) = &mut self.form {
                    form.notice = Some(say(&e));
                }
            }
            Message::Tick(now) => self.now = now,
            Message::LinkClicked(uri) => {
                if let Some(open) = &mut self.open {
                    open.notice = Some(format!("Links are not opened from here: {uri}"));
                }
            }
        }
        Task::none()
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
            page::white_button("New chat", Some(AppMessage::Chat(Message::NewChat(true)))),
        );
        let detail: Element<_> = match (&self.form, &self.open) {
            (Some(form), _) => new_chat_view(form, &self.agents, snapshot),
            (None, Some(open)) => match self.chat(open.id) {
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
        let chosen = self.open.as_ref().map(|open| open.id);
        let list: Element<_> = if self.list.is_empty() {
            page::body("No chat yet.")
        } else {
            column(self.list.iter().map(|chat| {
                let repository = repository_name(snapshot, chat.repository_id);
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
        let repository = repository_name(snapshot, chat.repository_id);
        let title = row![
            text(chat.title.as_str())
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold))
                .wrapping(Wrapping::WordOrGlyph),
            status_tag(&chat.status),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let who = match repository {
            Some(name) => format!("{} in {name}", chat.agent),
            None => format!("{}, no repository", chat.agent),
        };
        let head = row![
            column![title, page::context(who)]
                .spacing(2)
                .width(Length::Fill),
            actions(chat, open),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let conversation = scrollable(
            container(self.conversation_view(open))
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
                .push(compose_view(chat, open, !merge_is_primary)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| page::box_style())
        .into()
    }

    fn conversation_view<'a>(&'a self, open: &'a Open) -> Element<'a, AppMessage> {
        if open.conversation.entries().is_empty() {
            return page::body("Nothing has been said in this chat yet.");
        }
        column(
            open.conversation
                .entries()
                .iter()
                .enumerate()
                .map(|(index, entry)| match entry {
                    Entry::Text(_) => agent_text(open.markdown.get(&index)),
                    Entry::Tool(call) => transcript::tool_call(call),
                    Entry::Person(said, message) => person_view(*said, message),
                    Entry::Error(message) => transcript::error(message),
                }),
        )
        .spacing(12)
        .width(Length::Fill)
        .into()
    }
}

/// The name of the repository with this id, when the snapshot has it.
fn repository_name(snapshot: &Snapshot, id: Option<i64>) -> Option<&str> {
    snapshot
        .boards
        .iter()
        .find(|board| Some(board.repository.id) == id)
        .map(|board| board.repository.name.as_str())
}

/// What the chat's status says on its tag: the colour tells whether the agent works or waits for the person.
fn status_tag<'a>(status: &str) -> Element<'a, AppMessage> {
    let (label, tone) = match status {
        "starting" => ("Starting", Tone::Teal),
        "running" => ("Running", Tone::Teal),
        "waiting" => ("Waiting", Tone::Amber),
        "succeeded" => ("Ended", Tone::Plain),
        "failed" => ("Failed", Tone::Plain),
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

/// The right of the chat's head: ending it and stopping it (or the question that confirms stopping), while it lasts.
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
        actions = actions
            .push(page::white_button(
                "End chat",
                Some(AppMessage::Chat(Message::End)),
            ))
            .push(page::white_button(
                "Stop",
                Some(AppMessage::Chat(Message::AskStop)),
            ));
    }
    actions.into()
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

/// The form of a new chat: the agent, the repository, and the first message.
fn new_chat_view<'a>(
    form: &'a Form,
    agents: &'a [Agent],
    snapshot: &'a Snapshot,
) -> Element<'a, AppMessage> {
    let label = |label: &'static str| {
        text(label)
            .size(theme::LABEL)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::SIDEBAR_GROUP)
    };
    let agents = agents.iter().fold(row![].spacing(8), |chips, agent| {
        chips.push(page::chip(
            format!("{} · {}", agent.name, agent.model),
            form.agent == agent.name,
            false,
            AppMessage::Chat(Message::AgentChosen(agent.name.clone())),
        ))
    });
    let mut repositories = row![page::chip(
        "No repository".to_string(),
        form.repository.is_none(),
        false,
        AppMessage::Chat(Message::RepositoryChosen(None)),
    )]
    .spacing(8);
    for board in &snapshot.boards {
        let id = board.repository.id;
        repositories = repositories.push(page::chip(
            board.repository.name.clone(),
            form.repository == Some(id),
            false,
            AppMessage::Chat(Message::RepositoryChosen(Some(id))),
        ));
    }
    let start = form.ready().then_some(AppMessage::Chat(Message::Start));
    let hint = if form.agent == GRILLING_AGENT {
        "The Grilling partner questions your idea in a repository. Ending the chat puts the issues it proposes in the Inbox, to confirm."
    } else {
        "With a repository the agent works on its own branch. Ending the chat offers what it built as a change."
    };
    let mut content = column![
        text("New chat")
            .size(theme::BOX_HEADING)
            .font(theme::manrope(Weight::Bold)),
        label("Agent"),
        agents.wrap().vertical_spacing(8),
        label("Repository"),
        repositories.wrap().vertical_spacing(8),
        page::body(hint),
        label("First message"),
        text_input("What should the agent do?", &form.message)
            .on_input(|message| AppMessage::Chat(Message::FirstEdited(message)))
            .on_submit_maybe(start.clone())
            .size(theme::BODY)
            .padding([9, 12])
            .style(connect::input_style),
        row![
            page::primary_button("Start chat", start),
            page::white_button("Cancel", Some(AppMessage::Chat(Message::NewChat(false)))),
        ]
        .spacing(8),
    ]
    .spacing(10);
    if let Some(notice) = &form.notice {
        content = content.push(
            text(notice.as_str())
                .size(theme::LABEL)
                .font(theme::manrope(Weight::Semibold))
                .color(theme::AMBER_TEXT),
        );
    }
    container(content)
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| page::box_style())
        .into()
}

/// The viewer of the agent's Markdown: a link click is a message, and a code block is drawn as a file is.
struct Markdown;

impl<'a> markdown::Viewer<'a, AppMessage> for Markdown {
    fn on_link_click(uri: markdown::Uri) -> AppMessage {
        AppMessage::Chat(Message::LinkClicked(uri))
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
fn agent_text(parsed: Option<&(String, Vec<markdown::Item>)>) -> Element<'_, AppMessage> {
    let items = parsed.map_or(&[][..], |(_, items)| items.as_slice());
    container(markdown::view_with(items, markdown_settings(), &Markdown))
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
        .find(|change| change.branch == branch)?;
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

/// The field to write in and the Send button, the primary one unless `send_is_primary` is false; under them what the
/// chat does with the message.
fn compose_view<'a>(chat: &Chat, open: &'a Open, send_is_primary: bool) -> Element<'a, AppMessage> {
    let writable = chat.is_open();
    let can_send = writable && !open.message.trim().is_empty();
    let send = can_send.then_some(AppMessage::Chat(Message::Send));
    let field = text_input("Write to the agent", &open.message)
        .on_input_maybe(writable.then_some(|text| AppMessage::Chat(Message::Edited(text))))
        .on_submit_maybe(send.clone())
        .size(theme::BODY)
        .padding([10, 12])
        .style(connect::input_style);
    let hint = match (&open.notice, chat.status.as_str()) {
        (Some(notice), _) => notice.as_str(),
        (None, "starting" | "running") => {
            "The agent is working. Your message is queued and sent when it is done with this turn."
        }
        (None, "waiting") => "The agent answers at once.",
        (None, _) => "This chat has ended.",
    };
    column![
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
        text(hint).size(theme::LABEL).color(theme::TEXT_MUTED),
    ]
    .spacing(8)
    .padding([12, 16])
    .into()
}
