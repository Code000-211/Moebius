//! The terminal tabs of a project: a shell on the server in the repository's clone (`GET
//! /v1/repositories/{id}/terminal`), in the pane of the Terminal page. Each tab has its own connection, kept while the
//! project's page is shown; leaving the page drops them, which ends the shells, and coming back starts new ones under
//! the screen that was left.

use iced::widget::{column, container};
use iced::{Element, Length, Subscription, keyboard};

use crate::app::Message as AppMessage;
use crate::client::{self, TerminalEvent};
use crate::settings::Settings;
use crate::widgets::page;
use crate::widgets::terminal::Shell;

use super::{Message, Project, Tab};

/// A terminal tab: its number and its shell.
pub(super) struct TerminalTab {
    pub n: u32,
    shell: Shell,
}

impl TerminalTab {
    /// The terminal with number `n`, whose connection is told from others by `generation`.
    pub fn new(n: u32, generation: u32) -> Self {
        TerminalTab {
            n,
            shell: Shell::in_clone(generation),
        }
    }
}

/// The keys of a terminal tab: any key nothing else took. The app ignores them where no terminal is shown.
pub fn terminal_key(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<AppMessage> {
    match event {
        iced::Event::Keyboard(event @ keyboard::Event::KeyPressed { .. })
            if status == iced::event::Status::Ignored =>
        {
            Some(AppMessage::ProjectKey(event))
        }
        _ => None,
    }
}

impl Project {
    /// Whether the tab shown is a terminal, which takes the keys.
    pub fn shows_terminal(&self) -> bool {
        matches!(self.tab, Tab::Terminal(_))
    }

    fn terminal_mut(&mut self, n: u32) -> Option<&mut Shell> {
        self.terminals
            .iter_mut()
            .find(|terminal| terminal.n == n)
            .map(|terminal| &mut terminal.shell)
    }

    /// Something a terminal's connection or pane did, or a key for the terminal shown.
    pub(super) fn shell_update(&mut self, message: Message) {
        match message {
            Message::Shell(n, event) => {
                if let Some(shell) = self.terminal_mut(n) {
                    shell.on_event(event);
                }
            }
            Message::Pane(n, event) => {
                if let Some(shell) = self.terminal_mut(n) {
                    shell.on_pane(event);
                }
            }
            Message::Key(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) => {
                // A panel with a field over the tabs takes the typing.
                if self.find.is_some() {
                    return;
                }
                if let Tab::Terminal(n) = self.tab
                    && let Some(shell) = self.terminal_mut(n)
                {
                    shell.key(&key, modifiers, text.as_deref());
                }
            }
            _ => {}
        }
    }

    /// The connections of the terminals, for as long as the page is shown: dropping one closes its socket.
    pub fn subscription(&self, repository: i64, settings: &Settings) -> Subscription<AppMessage> {
        Subscription::batch(self.terminals.iter().map(|terminal| {
            Subscription::run_with(
                (
                    settings.clone(),
                    repository,
                    terminal.n,
                    terminal.shell.generation,
                ),
                |(settings, repository, n, _)| {
                    use iced::futures::StreamExt;
                    let (repository, n) = (*repository, *n);
                    client::terminal(
                        settings.clone(),
                        format!("/v1/repositories/{repository}/terminal"),
                    )
                    .map(move |event: TerminalEvent| {
                        AppMessage::Project(repository, Message::Shell(n, event))
                    })
                },
            )
        }))
    }

    /// The terminal with number `n`, as big as the tab.
    pub(super) fn terminal_view(&self, repository: i64, n: u32) -> Element<'_, AppMessage> {
        let Some(terminal) = self.terminals.iter().find(|terminal| terminal.n == n) else {
            return container(page::body("This terminal is gone."))
                .padding(crate::theme::BOX_PADDING)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| page::box_style())
                .into();
        };
        column![
            terminal
                .shell
                .view(move |event| AppMessage::Project(repository, Message::Pane(n, event)))
        ]
        .height(Length::Fill)
        .into()
    }
}
