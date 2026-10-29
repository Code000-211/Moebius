//! The Terminal page: a shell on the server, in the directory of `moebius.toml` (`GET /v1/terminal`), in the pane of the
//! live view's shell made as big as the page. The shell is opened when the page is opened and closed when it is left.

use iced::widget::column;
use iced::{Element, Length, Subscription, keyboard};

use crate::app::Message as AppMessage;
use crate::client::{self, TerminalEvent};
use crate::screens::ports::Scope;
use crate::settings::Settings;
use crate::theme;
use crate::widgets::page;
use crate::widgets::terminal::{PaneEvent, Shell};

#[derive(Debug, Clone)]
pub enum Message {
    /// Something the shell's connection did.
    Shell(TerminalEvent),
    /// The pane was laid out at a size, or a click gave it focus or took it away.
    Pane(PaneEvent),
    /// A key nothing else took, for the shell if it has focus.
    Key(keyboard::Event),
}

/// The shell of the page, if the page has been opened.
#[derive(Default)]
pub struct ServerTerminal {
    shell: Option<Shell>,
    /// How many shells were opened, which tells one's connection from the next one's.
    opened: u32,
}

impl ServerTerminal {
    /// The page was opened: a new shell starts, on a new connection.
    pub fn open(&mut self) {
        self.opened += 1;
        self.shell = Some(Shell::on_server(self.opened));
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

    /// The shell's connection and the keys that go to it, for as long as the page is shown: leaving it drops both,
    /// which closes the socket.
    pub fn subscription(&self, settings: &Settings) -> Subscription<AppMessage> {
        let Some(shell) = &self.shell else {
            return Subscription::none();
        };
        let connection =
            Subscription::run_with((settings.clone(), shell.generation), |(settings, _)| {
                client::terminal(settings.clone(), "/v1/terminal".to_string())
            })
            .map(|event| AppMessage::Terminal(Message::Shell(event)));
        let keys = keyboard::listen().map(|event| AppMessage::Terminal(Message::Key(event)));
        Subscription::batch([connection, keys])
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        let header = page::header(
            "Setup",
            "Terminal",
            None,
            page::context("Shell on the server, in the directory of moebius.toml."),
            crate::screens::ports::button(Scope::Server),
        );
        let mut content = column![header]
            .spacing(theme::PAGE_GAP)
            .width(Length::Fill)
            .height(Length::Fill);
        if let Some(shell) = &self.shell {
            content = content.push(shell.view(|event| AppMessage::Terminal(Message::Pane(event))));
        }
        content.into()
    }
}
