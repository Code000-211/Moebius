//! How a message changes the live view: the run loaded, a steer sent, the run stopped, the clock, and the shell in
//! its worktree opened, closed and given its keys.

use iced::{Task, keyboard};

use crate::app::Message as AppMessage;
use crate::client::Client;
use crate::settings::Settings;
use crate::widgets::terminal::Shell;

use super::{Live, Message, say};

impl Live {
    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Loaded(Ok(loaded)) => {
                self.run = loaded.run;
                self.now = loaded.now;
                self.notice = None;
                for event in loaded.events {
                    self.conversation.push(event);
                }
                self.set_judgements(loaded.judgements);
            }
            Message::Loaded(Err(e)) => self.notice = Some(say(&e)),
            Message::Judgements(Ok(judgements)) => self.set_judgements(judgements),
            Message::Judgements(Err(_)) => {}
            Message::SteerEdited(text) => self.steer = text,
            Message::Steer => {
                if let Some(settings) = settings.cloned()
                    && self.running()
                    && !self.steer.trim().is_empty()
                {
                    let (run, message) = (self.run.id, self.steer.clone());
                    return Task::perform(
                        async move { Client::new(settings).steer(run, &message).await },
                        |run| AppMessage::Live(Message::Steered(run)),
                    );
                }
            }
            Message::Steered(Ok(_)) => {
                self.steer.clear();
                self.notice = None;
            }
            Message::Steered(Err(e)) | Message::Stopped(Err(e)) => self.notice = Some(say(&e)),
            Message::AskStop => self.confirming = true,
            Message::KeepGoing => self.confirming = false,
            Message::Stop => {
                self.confirming = false;
                if let Some(settings) = settings.cloned() {
                    let run = self.run.id;
                    return Task::perform(
                        async move { Client::new(settings).stop(run).await },
                        |run| AppMessage::Live(Message::Stopped(run)),
                    );
                }
            }
            Message::Stopped(Ok(_)) => self.notice = None,
            Message::Tick(now) => self.now = now,
            Message::ToggleShell => {
                self.shell = match self.shell {
                    Some(_) => None,
                    None if self.has_worktree() => {
                        self.shells += 1;
                        Some(Shell::new(self.shells))
                    }
                    None => None,
                };
            }
            Message::Shell(event) => {
                if let Some(shell) = &mut self.shell {
                    shell.on_event(event);
                }
            }
            Message::ShellPane(event) => {
                if let Some(shell) = &mut self.shell {
                    shell.on_pane(event);
                }
            }
            Message::Key(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) => {
                if let Some(shell) = &mut self.shell {
                    shell.key(&key, modifiers, text.as_deref());
                }
            }
            Message::Key(_) => {}
        }
        Task::none()
    }
}
