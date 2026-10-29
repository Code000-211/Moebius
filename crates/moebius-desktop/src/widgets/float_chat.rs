//! The floating chat: a round button at the bottom right of every screen, like a website's support button, that grows
//! into a window with the Moebius agent over the page. Minimising turns it back into the button, and the chat stays
//! across pages. Every message carries where the person is (the app's `page_context`), which only the agent is told:
//! the first one starts a chat with the Moebius agent, in the repository of the page when it has one, and the later
//! ones go to the same chat. The conversation is drawn and followed as on the chat page (`chat::Thread`).

use iced::font::Weight;
use iced::widget::{button, column, container, opaque, row, scrollable, text, text_input};
use iced::{Alignment, Border, Element, Length, Padding, Shadow, Task, Vector};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Judgement, Run, RunEvent};
use crate::screens::chat::{self, Place, Thread};
use crate::screens::live::say;
use crate::settings::Settings;
use crate::widgets::icons::Icon;
use crate::widgets::page;
use crate::{screens::connect, theme};

#[derive(Debug, Clone)]
pub enum Message {
    /// Open the window, or minimise it to the button.
    Show(bool),
    Edited(String),
    /// Send what is written; the app adds where the person is.
    Send,
    /// The server started the chat from the first message, or said why not.
    Started(Result<Run, ClientError>),
    /// The server took a later message, or said why not.
    Sent(Result<Run, ClientError>),
    /// The events of the chat with this id were read, those after the ones already held.
    Loaded(i64, Result<Vec<RunEvent>, ClientError>),
    /// The judgements of the tool calls of the chat with this id were read.
    Judged(i64, Result<Vec<Judgement>, ClientError>),
    /// A step of the agent was clicked: it unfolds when folded and folds when unfolded.
    Fold(usize),
    /// A link of the agent's text was clicked.
    LinkClicked(String),
    /// Start over: the next message starts a new chat.
    NewConversation,
    /// Open the chat on the Chat page, where its terminal and the change it offers are.
    OpenInChat,
    /// The clock, for how long the agent's steps take.
    Tick(i64),
}

/// The floating window.
pub struct Floating;

impl Place for Floating {
    fn fold(step: usize) -> AppMessage {
        AppMessage::Float(Message::Fold(step))
    }

    fn link(uri: String) -> AppMessage {
        AppMessage::Float(Message::LinkClicked(uri))
    }

    fn loaded(chat: i64, events: Result<Vec<RunEvent>, ClientError>) -> AppMessage {
        AppMessage::Float(Message::Loaded(chat, events))
    }

    fn judged(chat: i64, judgements: Result<Vec<Judgement>, ClientError>) -> AppMessage {
        AppMessage::Float(Message::Judged(chat, judgements))
    }
}

/// The id of the window's field, which opening the window focuses.
const FIELD: &str = "float-chat-message";

/// The names of the round button and of the window's minimise button, for a test to find them.
pub const BUTTON: &str = "floating-chat";
pub const MINIMISE: &str = "floating-chat-minimise";

/// The floating chat: whether the window is open and the chat it holds.
pub struct FloatChat {
    open: bool,
    /// The chat, once the first message started it.
    thread: Option<Thread>,
    /// The chat's status: `starting`, `running`, `waiting`, or how it ended.
    status: String,
    /// The repository the chat works in, if any.
    repository: Option<i64>,
    message: String,
    /// Whether the server is taking the message, so that a second Send waits.
    sending: bool,
    /// The last thing that went wrong, as a short line.
    notice: Option<String>,
    /// The time of the last tick, Unix seconds.
    now: i64,
}

impl FloatChat {
    pub fn new(now: i64) -> Self {
        FloatChat {
            open: false,
            thread: None,
            status: String::new(),
            repository: None,
            message: String::new(),
            sending: false,
            notice: None,
            now,
        }
    }

    /// Whether the window is open rather than the button.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The id of the chat the window holds, once started.
    pub fn chat(&self) -> Option<i64> {
        self.thread.as_ref().map(|thread| thread.id)
    }

    /// The repository the chat works in, once started.
    pub fn repository(&self) -> Option<i64> {
        self.repository
    }

    /// Whether the agent is working, so that the window needs the clock.
    pub fn working(&self) -> bool {
        self.open && matches!(self.status.as_str(), "starting" | "running")
    }

    /// Opens the window with `text` written in the field, for the person to finish and send.
    pub fn prefill(&mut self, text: String) -> Task<AppMessage> {
        self.open = true;
        self.message = text;
        iced::widget::operation::focus(FIELD)
    }

    /// Opens the window on `run`, a chat with the Moebius agent started elsewhere in the app, in `repository`.
    pub fn adopt(
        &mut self,
        run: &Run,
        repository: Option<i64>,
        settings: &Settings,
    ) -> Task<AppMessage> {
        self.open = true;
        self.thread = Some(Thread::new(run.id));
        self.status.clone_from(&run.status);
        self.repository = repository;
        self.sending = false;
        self.notice = None;
        chat::load::<Floating>(settings.clone(), run.id, 0)
    }

    /// Sends what is written: the first message, or one after the chat ended, starts a chat with the Moebius agent in
    /// `repository`; a later one goes to the chat. The agent is told `context` with it.
    pub fn send(
        &mut self,
        settings: &Settings,
        context: String,
        repository: Option<i64>,
    ) -> Task<AppMessage> {
        let message = self.message.trim().to_string();
        if message.is_empty() || self.sending {
            return Task::none();
        }
        self.sending = true;
        self.notice = None;
        let client = Client::new(settings.clone());
        match self.thread.as_ref().filter(|_| self.is_going_on()) {
            Some(thread) => {
                let id = thread.id;
                Task::perform(
                    async move { client.say(id, &message, Some(&context)).await },
                    |result| AppMessage::Float(Message::Sent(result)),
                )
            }
            None => {
                self.repository = repository;
                Task::perform(
                    async move { client.ask(&message, repository, &context).await },
                    |result| AppMessage::Float(Message::Started(result)),
                )
            }
        }
    }

    /// Whether the chat has not ended, so a message goes to it.
    fn is_going_on(&self) -> bool {
        matches!(self.status.as_str(), "starting" | "running" | "waiting")
    }

    /// A `run_event` of the stream: the chat's status follows the agent, and the conversation takes the event. Whether
    /// the judgements are to be read again, as a check arrived.
    pub fn on_event(&mut self, run: i64, event: &RunEvent) -> bool {
        let Some(thread) = self.thread.as_mut().filter(|thread| thread.id == run) else {
            return false;
        };
        match event.kind.as_str() {
            "agent_start" => self.status = "running".to_string(),
            "agent_settled" if event.data["aborted"] != true => {
                self.status = "waiting".to_string();
            }
            _ => {}
        }
        thread.push(event.clone());
        event.kind == "tool_check"
    }

    /// A run ended, as the stream says.
    pub fn on_ended(&mut self, run: i64, status: &str) {
        if self.chat() == Some(run) {
            self.status = status.to_string();
        }
    }

    /// Reads what the chat missed, as after the stream was reconnected.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        match &self.thread {
            Some(thread) => chat::load::<Floating>(settings.clone(), thread.id, thread.last_seq()),
            None => Task::none(),
        }
    }

    /// Reads who decided the chat's tool calls again.
    pub fn reload_judgements(&self, settings: &Settings) -> Task<AppMessage> {
        match &self.thread {
            Some(thread) => chat::load_judgements::<Floating>(settings.clone(), thread.id),
            None => Task::none(),
        }
    }

    /// What the window does with a message; `Send` and `OpenInChat` need the app and are answered there.
    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Show(open) => {
                self.open = open;
                if open {
                    return iced::widget::operation::focus(FIELD);
                }
            }
            Message::Edited(text) => self.message = text,
            Message::Send | Message::OpenInChat => {}
            Message::Started(Ok(run)) => {
                self.message.clear();
                return match settings {
                    Some(settings) => self.adopt(&run, self.repository, settings),
                    None => Task::none(),
                };
            }
            Message::Sent(Ok(_)) => {
                self.sending = false;
                self.message.clear();
            }
            Message::Started(Err(e)) | Message::Sent(Err(e)) => {
                self.sending = false;
                self.notice = Some(say(&e));
            }
            Message::Loaded(id, Ok(events)) => {
                if let Some(thread) = self.thread.as_mut().filter(|thread| thread.id == id) {
                    for event in events {
                        thread.push(event);
                    }
                }
            }
            Message::Loaded(_, Err(e)) => self.notice = Some(say(&e)),
            Message::Judged(id, Ok(judgements)) => {
                if let Some(thread) = self.thread.as_mut().filter(|thread| thread.id == id) {
                    thread.judge(judgements);
                }
            }
            Message::Judged(_, Err(_)) => {}
            Message::Fold(step) => {
                if let Some(thread) = &mut self.thread {
                    thread.fold(step);
                }
            }
            Message::LinkClicked(uri) => {
                self.notice = Some(format!("Links are not opened from here: {uri}"));
            }
            Message::NewConversation => {
                self.thread = None;
                self.status.clear();
                self.repository = None;
                self.sending = false;
                self.notice = None;
                return iced::widget::operation::focus(FIELD);
            }
            Message::Tick(now) => self.now = now,
        }
        Task::none()
    }

    /// The layer over the page: the button at the bottom right, or the window there. `place` says where the chat works,
    /// or where the next message would start it.
    pub fn view<'a>(&'a self, place: String) -> Element<'a, AppMessage> {
        let content = if self.open {
            self.window(place)
        } else {
            round_button()
        };
        container(content)
            .padding(theme::FLOAT_MARGIN)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_right(Length::Fill)
            .align_bottom(Length::Fill)
            .into()
    }

    fn window<'a>(&'a self, place: String) -> Element<'a, AppMessage> {
        let title = column![
            text("Moebius agent")
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold)),
            text(place).size(theme::LABEL).color(theme::TEXT_SECONDARY),
        ]
        .spacing(2)
        .width(Length::Fill);
        let minimise = container(
            button(Icon::Minimise.view(theme::TEXT_SECONDARY, 18.0))
                .padding(6)
                .on_press(AppMessage::Float(Message::Show(false)))
                .style(|_, status| button::Style {
                    background: (status == button::Status::Hovered).then(|| theme::LANE.into()),
                    border: Border {
                        radius: theme::RADIUS_SMALL.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }),
        )
        .id(MINIMISE);
        let mut head =
            column![row![title, minimise].spacing(8).align_y(Alignment::Center)].spacing(8);
        if self.thread.is_some() {
            head = head.push(
                row![
                    link("New conversation", Message::NewConversation),
                    link("Open in Chat", Message::OpenInChat),
                ]
                .spacing(16),
            );
        }
        let conversation: Element<'a, AppMessage> = match &self.thread {
            Some(thread) => thread.view::<Floating>(&self.status, self.now),
            None => page::body(
                "Ask the Moebius agent anything about this page. It is told where you are: the page, the \
                 repository, and what you are editing.",
            ),
        };
        let send = (!self.message.trim().is_empty() && !self.sending)
            .then_some(AppMessage::Float(Message::Send));
        let hint = match (&self.notice, self.status.as_str()) {
            (Some(notice), _) => notice.as_str(),
            (None, _) if self.thread.is_none() => "Your first message starts the chat.",
            (None, "starting" | "running") => {
                "The agent is working. Your message is sent when it is done with this turn."
            }
            (None, "waiting") => "The agent answers at once.",
            (None, _) => "This chat has ended. What you send starts a new one.",
        };
        let compose = column![
            row![
                text_input("Ask the Moebius agent", &self.message)
                    .id(FIELD)
                    .on_input(|text| AppMessage::Float(Message::Edited(text)))
                    .on_submit_maybe(send.clone())
                    .size(theme::BODY)
                    .padding([10, 12])
                    .style(connect::input_style),
                page::primary_button("Send", send),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            text(hint).size(theme::LABEL).color(theme::TEXT_MUTED),
        ]
        .spacing(6)
        .padding([12, 14]);
        let window = container(column![
            container(head).padding([12, 14]),
            page::rule(),
            scrollable(
                container(conversation)
                    .padding(Padding::from([14, 14]))
                    .width(Length::Fill)
            )
            .anchor_bottom()
            .height(Length::Fill),
            page::rule(),
            compose,
        ])
        .width(theme::FLOAT_WIDTH)
        .height(Length::Fill)
        .max_height(theme::FLOAT_HEIGHT)
        .style(|_| container::Style {
            shadow: shadow(),
            ..page::box_style()
        });
        opaque(window)
    }
}

/// The shadow under the floating button and window: tight and faint.
fn shadow() -> Shadow {
    Shadow {
        color: theme::FLOAT_SHADOW,
        offset: Vector::new(0.0, 2.0),
        blur_radius: 8.0,
    }
}

/// The button at the bottom right that opens the floating chat: a small dark "Agent" button.
fn round_button<'a>() -> Element<'a, AppMessage> {
    let button = button(
        row![
            Icon::Chat.view(theme::ON_FILL, 14.0),
            text("Agent")
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::ON_FILL),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([7, 12])
    .on_press(AppMessage::Float(Message::Show(true)))
    .style(|_, status| button::Style {
        background: Some(
            if status == button::Status::Hovered {
                theme::TEXT_SECONDARY
            } else {
                theme::TEXT
            }
            .into(),
        ),
        text_color: theme::ON_FILL,
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        shadow: shadow(),
        ..button::Style::default()
    });
    container(button).id(BUTTON).into()
}

/// A small button that reads as a link, in the head of the window.
fn link<'a>(label: &'a str, message: Message) -> Element<'a, AppMessage> {
    button(
        text(label)
            .size(theme::LABEL)
            .font(theme::manrope(Weight::Semibold)),
    )
    .padding(0)
    .on_press(AppMessage::Float(message))
    .style(|_, status| button::Style {
        text_color: if status == button::Status::Hovered {
            theme::ONLINE_TEXT
        } else {
            theme::TEAL
        },
        ..button::Style::default()
    })
    .into()
}
