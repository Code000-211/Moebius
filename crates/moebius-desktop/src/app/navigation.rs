//! Where the person is and the way back: the place left by each click, "Back", Alt+Left and the mouse's back button,
//! and Escape in an editor.

use iced::Task;

use crate::screens::agents;
use crate::screens::project::{self, Project, Tab};
use crate::search;
use crate::widgets::float_chat;

use super::message::{Message, Page};
use super::{App, Opened};

/// Where the person is, enough to show it again as it was: the page and what is open on it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Location {
    page: Page,
    place: Place,
}

/// What is open on a page.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Place {
    /// The page as it opens.
    Whole,
    /// A tab of a project, with no card open.
    Tab(Tab),
    /// A card of a project's board, its run or its history.
    Card(i64),
    /// The editor of the script of the column with this name.
    Script(String),
    /// The chat with this id on the Chat page.
    Chat(i64),
    /// The editor of the agent with this name.
    Agent(String),
}

/// The other ways back: Alt+Left where nothing else takes it (a text field moves by words with it), and the mouse's back
/// button.
pub fn back_key(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. })
            if modifiers.alt()
                && key == iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft)
                && status == iced::event::Status::Ignored =>
        {
            Some(Message::Back)
        }
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Back)) => {
            Some(Message::Back)
        }
        _ => None,
    }
}

impl App {
    /// Whether `message` is a click on a page that may take the person to another place, which "Back" returns from.
    pub(super) fn navigates(message: &Message) -> bool {
        matches!(
            message,
            Message::Visit(_)
                | Message::OpenPorts(_)
                | Message::OpenCard(_)
                | Message::CloseCard
                | Message::EditScript(..)
                | Message::OpenChat(_)
                | Message::Float(float_chat::Message::OpenInChat)
                | Message::Search(
                    search::Message::Submit
                        | search::Message::Pick(_)
                        | search::Message::Started(_, Ok(_))
                )
        )
    }

    /// Where the person is now.
    pub(super) fn location(&self) -> Location {
        let page = self.page();
        let place = match page {
            Page::Board(id) => match &self.opened {
                Some(Opened::Run(live)) => Place::Card(live.card),
                Some(Opened::History { card, .. }) => Place::Card(*card),
                Some(Opened::Script(editor)) => Place::Script(editor.column().to_string()),
                None => Place::Tab(
                    self.projects
                        .get(&id)
                        .map_or(Tab::Board, |project| project.tab().clone()),
                ),
            },
            Page::Chat => self.chats.opened().map_or(Place::Whole, Place::Chat),
            Page::Agents => self
                .agents
                .editing()
                .map_or(Place::Whole, |name| Place::Agent(name.to_string())),
            _ => Place::Whole,
        };
        Location { page, place }
    }

    /// A click took the person from `left` to where they are now: `left` is kept to go back to, unless the click went
    /// back to the place kept last, which is then dropped, as "Back" would have done.
    pub(super) fn arrive(&mut self, left: Location) {
        let now = self.location();
        if now == left {
            return;
        }
        if self.history.last() == Some(&now) {
            self.history.pop();
        } else {
            self.history.push(left);
        }
    }

    /// Shows `location` again: its page, the tab of its project, and the card, script, chat or agent that was open.
    pub(super) fn go_to(&mut self, location: Location) -> Task<Message> {
        let page = self.show_page(location.page);
        let place = match (location.page, location.place) {
            (Page::Board(id), Place::Tab(tab)) => self.projects.entry(id).or_default().update(
                id,
                project::Message::Tab(tab),
                self.settings.as_ref(),
            ),
            (Page::Board(_), Place::Card(card)) => self.open(card),
            (Page::Board(id), Place::Script(column)) => {
                self.handle(Message::EditScript(id, column))
            }
            (Page::Chat, Place::Chat(chat)) => match &self.settings {
                Some(settings) => self.chats.open(chat, settings),
                None => Task::none(),
            },
            (Page::Agents, Place::Agent(name)) => self
                .agents
                .update(agents::Message::Select(name), self.settings.as_ref()),
            _ => Task::none(),
        };
        Task::batch([page, place])
    }

    /// What the link back to `location` calls it.
    pub(super) fn location_title(&self, location: &Location) -> String {
        let repository = |id| {
            self.snapshot
                .boards
                .iter()
                .find(|board| board.repository.id == id)
                .map_or("the board", |board| board.repository.name.as_str())
        };
        match (&location.page, &location.place) {
            (Page::Board(id), Place::Tab(Tab::Board)) => repository(*id).to_string(),
            (Page::Board(id), Place::Tab(tab)) => format!("{} of {}", tab.title(), repository(*id)),
            (Page::Board(_), Place::Card(card)) => self
                .snapshot
                .boards
                .iter()
                .flat_map(|board| &board.cards)
                .find(|c| c.id == *card)
                .map_or_else(|| "the card".to_string(), |c| format!("#{}", c.number)),
            (Page::Board(_), Place::Script(column)) => format!("the {column} script"),
            (Page::Agents, Place::Agent(name)) => name.clone(),
            (page, _) => page.title().to_string(),
        }
    }

    /// Escape in an editor goes back, when the editor came from another place and holds nothing unsaved.
    pub(super) fn escape(&mut self) -> Task<Message> {
        let unsaved = match (self.page(), &self.opened) {
            (Page::Board(_), Some(Opened::Script(editor))) => Some(editor.unsaved()),
            (Page::Board(id), None) => self
                .projects
                .get(&id)
                .filter(|project| matches!(project.tab(), Tab::Editor(_)))
                .map(Project::unsaved),
            (Page::Rules, _) => Some(self.rules.unsaved()),
            (Page::Agents, _) => self.agents.unsaved(),
            _ => None,
        };
        match unsaved {
            Some(false) => self.handle(Message::Back),
            _ => Task::none(),
        }
    }
}
