//! A chat in Pi-Bolt's own terminal interface: the pane of the live view's shell, made as big as the chat's detail,
//! connected to `GET /v1/chats/{id}/tui`. The pane is opened when the chat's switch is set to "Terminal" and closed when
//! it is set back, another chat is chosen or the page is left, which closes the socket and ends Pi-Bolt there.

use iced::{Element, Subscription, keyboard};

use crate::app::Message as AppMessage;
use crate::chat;
use crate::client::{self, TerminalEvent};
use crate::settings::Settings;
use crate::terminal::{PaneEvent, Shell};

#[derive(Debug, Clone)]
pub enum Message {
    /// Something the connection did.
    Shell(TerminalEvent),
    /// The pane was laid out at a size, or a click gave it focus or took it away.
    Pane(PaneEvent),
    /// A key nothing else took, for the terminal if it has focus.
    Key(keyboard::Event),
}

/// The terminal of the open chat, if its switch is set to "Terminal".
#[derive(Default)]
pub struct ChatTerminal {
    shell: Option<Shell>,
    /// How many terminals were opened, which tells one's connection from the next one's.
    opened: u32,
}

impl ChatTerminal {
    /// Whether the terminal is shown instead of the conversation.
    pub fn is_shown(&self) -> bool {
        self.shell.is_some()
    }

    /// A new terminal starts, on a new connection.
    pub fn open(&mut self) {
        self.opened += 1;
        self.shell = Some(Shell::in_chat(self.opened));
    }

    /// The terminal is gone, and its connection with it.
    pub fn close(&mut self) {
        self.shell = None;
    }

    pub fn update(&mut self, message: Message) {
        let Some(shell) = &mut self.shell else {
            return;
        };
        match message {
            Message::Shell(event) => shell.on_event(event),
            Message::Pane(PaneEvent::Resized(size)) => shell.resized(size),
            Message::Pane(PaneEvent::Focus(focused)) => shell.set_focus(focused),
            Message::Key(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) => shell.key(&key, modifiers, text.as_deref()),
            Message::Key(_) => {}
        }
    }

    /// The connection of chat `chat` and the keys that go to it, for as long as the terminal is shown.
    pub fn subscription(&self, chat: i64, settings: &Settings) -> Subscription<AppMessage> {
        let Some(shell) = &self.shell else {
            return Subscription::none();
        };
        let connection = Subscription::run_with(
            (settings.clone(), chat, shell.generation),
            |(settings, chat, _)| {
                client::terminal(settings.clone(), format!("/v1/chats/{chat}/tui"))
            },
        )
        .map(|event| AppMessage::Chat(chat::Message::Terminal(Message::Shell(event))));
        let keys = keyboard::listen()
            .map(|event| AppMessage::Chat(chat::Message::Terminal(Message::Key(event))));
        Subscription::batch([connection, keys])
    }

    /// The pane, if the terminal is shown.
    pub fn view(&self) -> Option<Element<'_, AppMessage>> {
        self.shell.as_ref().map(|shell| {
            shell.view(|event| AppMessage::Chat(chat::Message::Terminal(Message::Pane(event))))
        })
    }
}
