//! How the tabs of a project are opened, shown and closed: the editor of a file, a terminal in the repository's clone
//! and a chat in the project, with Board always first.

use iced::Task;

use crate::app::Message as AppMessage;
use crate::screens::chat;
use crate::settings::Settings;

use super::shell::TerminalTab;
use super::{Message, Project, Tab};

impl Tab {
    /// What a link back to the tab calls it: Board, a file's name, Terminal or Chat.
    pub fn title(&self) -> String {
        match self {
            Tab::Board => "Board".to_string(),
            Tab::Editor(path) => path.rsplit('/').next().unwrap_or(path).to_string(),
            Tab::Terminal(_) => "Terminal".to_string(),
            Tab::Chat(_) => "Chat".to_string(),
        }
    }
}

impl Project {
    /// The tab shown.
    pub fn tab(&self) -> &Tab {
        &self.tab
    }

    /// The tabs after Board, in the order they were opened.
    pub fn opened(&self) -> &[Tab] {
        &self.opened
    }

    /// Shows Board or a tab that is open, without reading anything, as when a card of the board opens.
    pub fn show_tab(&mut self, tab: Tab) {
        if tab == Tab::Board || self.opened.contains(&tab) {
            self.tab = tab;
        } else {
            self.tab = Tab::Board;
        }
        self.menu = false;
        self.closing = None;
    }

    /// Shows `tab` (Board when it is not open any more): a chat tab shows its chat on the Chat page's state.
    pub(super) fn show(&mut self, repository: i64, tab: Tab) -> Task<AppMessage> {
        let changed = self.tab != tab;
        self.show_tab(tab);
        if !changed {
            return Task::none();
        }
        let Tab::Chat(n) = self.tab else {
            return Task::none();
        };
        match self.chats.iter().find(|(number, _)| *number == n) {
            Some((_, Some(chat))) => Task::done(AppMessage::Chat(chat::Message::Select(*chat))),
            _ => Task::done(AppMessage::Chat(chat::Message::NewChatIn(repository))),
        }
    }

    /// The person came to the page: the Files sidebar reads the default branch, and the chat of the tab shown is read
    /// again, as the Chat page may have shown another.
    pub fn enter(&mut self, repository: i64, settings: &Settings) -> Task<AppMessage> {
        let files = self.list(repository, "", settings);
        let chat = match self.chat_of(&self.tab) {
            Some(chat) => Task::done(AppMessage::Chat(chat::Message::Select(chat))),
            None => Task::none(),
        };
        Task::batch([files, chat])
    }

    /// Opens a tab at the end and shows it.
    pub(super) fn push_tab(&mut self, tab: Tab) {
        if !self.opened.contains(&tab) {
            self.opened.push(tab.clone());
        }
        self.show_tab(tab);
    }

    /// Opens a terminal in the repository's clone as a tab.
    pub(super) fn new_terminal(&mut self) {
        self.counter += 1;
        let n = self.counter;
        self.counter += 1;
        self.terminals.push(TerminalTab::new(n, self.counter));
        self.push_tab(Tab::Terminal(n));
    }

    /// Opens a chat in the project as a tab; the chat starts with its first message.
    pub(super) fn new_chat(&mut self, repository: i64) -> Task<AppMessage> {
        self.counter += 1;
        let n = self.counter;
        self.chats.push((n, None));
        self.push_tab(Tab::Chat(n));
        Task::done(AppMessage::Chat(chat::Message::NewChatIn(repository)))
    }

    /// The chat of the chat tab shown started or went on in the chat with this id.
    pub fn follow_chat(&mut self, chat: i64) {
        if let Tab::Chat(n) = self.tab
            && let Some(entry) = self.chats.iter_mut().find(|(number, _)| *number == n)
        {
            entry.1 = Some(chat);
        }
    }

    /// Whether the chat tab shown has no chat yet, so that a chat the Chat state starts belongs to it.
    pub fn awaits_chat(&self) -> bool {
        matches!(self.tab, Tab::Chat(n) if self.chats.iter().any(|(number, chat)| *number == n && chat.is_none()))
    }

    /// The chat of the chat tab with this number, once it is started.
    pub fn chat_of(&self, tab: &Tab) -> Option<i64> {
        let Tab::Chat(n) = tab else {
            return None;
        };
        self.chats
            .iter()
            .find(|(number, _)| number == n)
            .and_then(|(_, chat)| *chat)
    }

    /// Closes `tab`, and shows the one before it, or Board.
    pub(super) fn close_tab(&mut self, tab: &Tab) {
        let Some(index) = self.opened.iter().position(|other| other == tab) else {
            return;
        };
        self.opened.remove(index);
        match tab {
            Tab::Editor(path) => {
                self.files.retain(|file| file.path != *path);
                if self.closing.as_deref() == Some(path.as_str()) {
                    self.closing = None;
                }
            }
            Tab::Terminal(n) => self.terminals.retain(|terminal| terminal.n != *n),
            Tab::Chat(n) => self.chats.retain(|(number, _)| number != n),
            Tab::Board => {}
        }
        if self.tab == *tab {
            self.tab = match index {
                0 => Tab::Board,
                index => self.opened[index - 1].clone(),
            };
        }
    }

    /// A click on a tab or its cross, or a menu item.
    pub(super) fn tabs_update(
        &mut self,
        repository: i64,
        message: Message,
        settings: &Settings,
    ) -> Task<AppMessage> {
        match message {
            Message::Tab(tab) => {
                return Task::batch([
                    self.show(repository, tab),
                    self.list(repository, "", settings),
                ]);
            }
            Message::Close(Tab::Editor(path)) => {
                if self.file(&path).is_some_and(|file| file.unsaved()) {
                    self.tab = Tab::Editor(path.clone());
                    self.closing = Some(path);
                } else {
                    self.close_tab(&Tab::Editor(path));
                }
            }
            Message::Close(tab) => self.close_tab(&tab),
            Message::Discard(path) => self.close_tab(&Tab::Editor(path)),
            Message::Keep => self.closing = None,
            Message::ToggleFiles => {
                self.files_folded = !self.files_folded;
                if !self.files_folded {
                    return self.list(repository, "", settings);
                }
            }
            Message::Menu => self.menu = !self.menu,
            Message::NewEditor => {
                self.menu = false;
                return self.find_update(repository, super::find::Message::Go, settings);
            }
            Message::NewTerminal => self.new_terminal(),
            Message::NewChat => return self.new_chat(repository),
            _ => {}
        }
        Task::none()
    }
}
