//! The search field at the top of the sidebar: what is typed, the entries it matches and the one chosen. Matching is a
//! plain function over what the app already holds (the snapshot, the chats and the agents); the app opens the entry
//! chosen. The list shows over the page while the field has text.

use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text};
use iced::{Border, Element, Length, Padding, Task, keyboard};

use crate::app::{Message as AppMessage, Page};
use crate::client::{Agent, Chat, Client, ClientError, NewChat, Run, Snapshot};
use crate::settings::Settings;
use crate::theme;

/// The most entries the list shows, the last of them the ask entry.
const LIMIT: usize = 8;

/// The id of the field, which Ctrl+K focuses.
pub const FIELD: &str = "search";

/// The pages the search finds by title.
const PAGES: [Page; 14] = [
    Page::Inbox,
    Page::Chat,
    Page::Fleet,
    Page::Agents,
    Page::Skills,
    Page::McpServers,
    Page::Rules,
    Page::Retro,
    Page::Experiments,
    Page::DecisionModel,
    Page::Usage,
    Page::Models,
    Page::Plugins,
    Page::Settings,
];

#[derive(Debug, Clone)]
pub enum Message {
    Edited(String),
    /// Up or Down moved the selection.
    Move(Direction),
    /// Enter: the selected entry opens.
    Submit,
    /// An entry was clicked.
    Pick(usize),
    /// Escape closed the list.
    Close,
    /// Ctrl+K, or the icon of the collapsed sidebar: the sidebar opens and the field has the focus.
    Focus,
    /// The server answered the chat that the ask entry started, which was asked with this text.
    Started(String, Result<Run, ClientError>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

/// What an entry opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Card(i64),
    Chat(i64),
    Agent(String),
    Page(Page),
    /// A chat with the Moebius agent, with this as its first message.
    Ask(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub group: &'static str,
    pub label: String,
    pub detail: String,
    pub target: Target,
}

/// What is typed, the entry chosen, and what went wrong when the ask entry could not start its chat.
#[derive(Debug, Default)]
pub struct Search {
    pub text: String,
    pub selected: usize,
    pub notice: Option<String>,
}

impl Search {
    pub fn is_open(&self) -> bool {
        !self.text.trim().is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.selected = 0;
        self.notice = None;
    }

    /// Moves the selection, staying within the `count` entries.
    pub fn step(&mut self, direction: Direction, count: usize) {
        self.selected = match direction {
            Direction::Up => self.selected.saturating_sub(1),
            Direction::Down => (self.selected + 1).min(count.saturating_sub(1)),
        };
    }

    /// The list under the field, over the page.
    pub fn view<'a>(&'a self, entries: &[Entry]) -> Element<'a, AppMessage> {
        let mut list = column![].spacing(1);
        let mut group = "";
        for (index, entry) in entries.iter().enumerate() {
            if entry.group != group {
                group = entry.group;
                if !group.is_empty() {
                    list = list.push(
                        container(
                            text(group)
                                .size(theme::GROUP_LABEL)
                                .color(theme::TEXT_MUTED),
                        )
                        .padding(Padding {
                            top: 6.0,
                            right: 10.0,
                            bottom: 2.0,
                            left: 10.0,
                        }),
                    );
                }
            }
            let selected = index == self.selected;
            let line = row![
                text(entry.label.clone())
                    .size(theme::BODY)
                    .width(Length::Fill),
                text(entry.detail.clone())
                    .size(theme::LABEL)
                    .wrapping(Wrapping::None)
                    .color(theme::TEXT_MUTED),
            ]
            .spacing(10);
            list = list.push(
                button(line)
                    .width(Length::Fill)
                    .padding([6, 10])
                    .on_press(AppMessage::Search(Message::Pick(index)))
                    .style(move |_, status| button::Style {
                        background: (selected || status == button::Status::Hovered)
                            .then(|| theme::SIDEBAR_CURRENT.into()),
                        text_color: theme::TEXT,
                        border: Border {
                            radius: theme::RADIUS_ITEM.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }),
            );
        }
        if let Some(notice) = &self.notice {
            list = list.push(
                container(text(notice.as_str()).size(theme::LABEL).color(theme::AMBER))
                    .padding([6, 10]),
            );
        }
        container(list)
            .width(Length::Fixed(380.0))
            .padding(6)
            .style(|_| container::Style {
                background: Some(theme::SURFACE.into()),
                border: Border {
                    color: theme::LINE,
                    width: 1.0,
                    radius: theme::RADIUS_CONTROL.into(),
                },
                ..container::Style::default()
            })
            .into()
    }
}

/// Starts the chat of the ask entry, with `text` as its first message.
pub fn ask(settings: &Settings, text: String) -> Task<AppMessage> {
    let client = Client::new(settings.clone());
    let message = text.clone();
    Task::perform(
        async move {
            let chat = NewChat {
                agent: Some(crate::chat::DEFAULT_AGENT.into()),
                message: Some(message),
                ..NewChat::default()
            };
            client.start_chat(&chat).await
        },
        move |run| AppMessage::Search(Message::Started(text.clone(), run)),
    )
}

/// The keys the search listens to wherever the focus is: Ctrl+K (Cmd+K on macOS), and the arrows and Escape of the
/// list, which the app ignores while the list is closed.
pub fn key(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<AppMessage> {
    let iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };
    let message = match key.as_ref() {
        keyboard::Key::Character("k") if modifiers.command() => Message::Focus,
        keyboard::Key::Named(keyboard::key::Named::ArrowUp) => Message::Move(Direction::Up),
        keyboard::Key::Named(keyboard::key::Named::ArrowDown) => Message::Move(Direction::Down),
        keyboard::Key::Named(keyboard::key::Named::Escape) => Message::Close,
        _ => return None,
    };
    Some(AppMessage::Search(message))
}

/// Whether every word occurs in `haystack`, ignoring case.
fn has_words(haystack: &str, words: &[String]) -> bool {
    let haystack = haystack.to_lowercase();
    words.iter().all(|word| haystack.contains(word))
}

/// The entries for `query`: cards of every board (by title words and `#number`), projects by name, chats, agents and
/// pages, in that order, then the ask entry last. At most eight in all; the ask entry is never cut. Nothing for a blank query.
pub fn matches(query: &str, snapshot: &Snapshot, chats: &[Chat], agents: &[Agent]) -> Vec<Entry> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let cards = snapshot.boards.iter().flat_map(|board| {
        board
            .cards
            .iter()
            .filter(|card| has_words(&format!("{} #{}", card.title, card.number), &words))
            .map(|card| Entry {
                group: "Cards",
                label: format!("#{} {}", card.number, card.title),
                detail: format!("{} · {}", board.repository.name, card.column),
                target: Target::Card(card.id),
            })
            .collect::<Vec<_>>()
    });
    let projects = snapshot
        .boards
        .iter()
        .filter(|board| has_words(&board.repository.name, &words))
        .map(|board| Entry {
            group: "Projects",
            label: board.repository.name.clone(),
            detail: "Board, files and editor".to_string(),
            target: Target::Page(Page::Board(board.repository.id)),
        });
    let chats = chats
        .iter()
        .filter(|chat| has_words(&chat.title, &words))
        .map(|chat| Entry {
            group: "Chats",
            label: chat.title.clone(),
            detail: chat.agent.clone(),
            target: Target::Chat(chat.id),
        });
    let agents = agents
        .iter()
        .filter(|agent| has_words(&agent.name, &words))
        .map(|agent| Entry {
            group: "Agents",
            label: agent.name.clone(),
            detail: agent.model.clone(),
            target: Target::Agent(agent.name.clone()),
        });
    let pages = PAGES
        .iter()
        .filter(|page| has_words(page.title(), &words))
        .map(|page| Entry {
            group: "Pages",
            label: page.title().to_string(),
            detail: page.group().to_string(),
            target: Target::Page(*page),
        });
    let text = query.trim().to_string();
    let mut entries: Vec<Entry> = cards
        .chain(projects)
        .chain(chats)
        .chain(agents)
        .chain(pages)
        .take(LIMIT - 1)
        .collect();
    entries.push(Entry {
        group: "",
        label: format!("Ask the Moebius agent: {text}"),
        detail: String::new(),
        target: Target::Ask(text),
    });
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot() -> Snapshot {
        let card = |id: i64, repository: i64, title: &str| {
            json!({"id": id, "repository_id": repository, "number": id, "title": title,
                   "column": "Ready", "created_at": 1})
        };
        let board = |id: i64, name: &str, cards: Vec<serde_json::Value>| {
            json!({"repository": {"id": id, "name": name}, "cards": cards, "round_limit": 5,
                   "waiting": {}, "changes": []})
        };
        serde_json::from_value(json!({"boards": [
            board(1, "radiance", vec![card(54, 1, "Retry model downloads"), card(48, 1, "Cap the tree depth")]),
            board(2, "moebius", vec![card(7, 2, "Retry the Usage report")]),
        ], "inbox": []}))
        .unwrap()
    }

    fn chats() -> Vec<Chat> {
        serde_json::from_value(json!([
            {"id": 3, "agent": "Moebius", "status": "waiting", "title": "Why does the usage page retry?"},
            {"id": 2, "agent": "Coder", "status": "succeeded", "title": "Add a rule"},
        ]))
        .unwrap()
    }

    fn agents() -> Vec<Agent> {
        serde_json::from_value(json!([
            {"name": "Coder", "model": "m"}, {"name": "Reviewer", "model": "m"}
        ]))
        .unwrap()
    }

    fn targets(query: &str) -> Vec<Target> {
        matches(query, &snapshot(), &chats(), &agents())
            .into_iter()
            .map(|entry| entry.target)
            .collect()
    }

    #[test]
    fn a_blank_query_matches_nothing() {
        assert!(targets("").is_empty());
        assert!(targets("   ").is_empty());
    }

    #[test]
    fn every_word_must_occur_whatever_its_case() {
        assert_eq!(
            targets("RETRY usage"),
            [
                Target::Card(7),
                Target::Chat(3),
                Target::Ask("RETRY usage".into())
            ]
        );
        assert_eq!(targets("zzz"), [Target::Ask("zzz".into())]);
    }

    #[test]
    fn a_number_with_a_hash_finds_the_card_and_the_title_words_too() {
        assert_eq!(
            targets("#48"),
            [Target::Card(48), Target::Ask("#48".into())]
        );
        assert_eq!(
            targets("depth #48"),
            [Target::Card(48), Target::Ask("depth #48".into())]
        );
        assert_eq!(targets("tree #54").len(), 1);
    }

    #[test]
    fn groups_come_in_order_cards_chats_agents_pages_and_the_ask_entry_is_last() {
        let all = targets("re");
        let at = |target: Target| all.iter().position(|t| *t == target).unwrap();
        assert!(at(Target::Card(54)) < at(Target::Chat(3)));
        assert!(at(Target::Chat(3)) < at(Target::Agent("Reviewer".into())));
        assert!(at(Target::Agent("Reviewer".into())) < at(Target::Page(Page::Retro)));
        assert_eq!(all.last(), Some(&Target::Ask("re".into())));
    }

    #[test]
    fn agents_and_pages_are_found_by_name_and_title() {
        assert_eq!(
            targets("review"),
            [
                Target::Agent("Reviewer".into()),
                Target::Ask("review".into())
            ]
        );
        assert_eq!(
            targets("usage"),
            [
                Target::Card(7),
                Target::Chat(3),
                Target::Page(Page::Usage),
                Target::Ask("usage".into())
            ]
        );
    }

    #[test]
    fn a_project_is_found_by_its_repositorys_name() {
        assert_eq!(
            targets("radiance"),
            [Target::Page(Page::Board(1)), Target::Ask("radiance".into())]
        );
    }

    #[test]
    fn at_most_eight_entries_and_the_ask_entry_is_never_cut() {
        let cards: Vec<_> = (1..=20)
            .map(|id| json!({"id": id, "repository_id": 1, "number": id, "title": "same", "column": "Ready"}))
            .collect();
        let snapshot: Snapshot = serde_json::from_value(json!({"boards": [
            {"repository": {"id": 1, "name": "r"}, "cards": cards, "round_limit": 5,
             "waiting": {}, "changes": []}], "inbox": []}))
        .unwrap();
        let entries = matches("same", &snapshot, &[], &[]);
        assert_eq!(entries.len(), 8);
        assert!(matches!(entries[7].target, Target::Ask(_)));
        assert_eq!(entries[0].detail, "r · Ready");
    }

    #[test]
    fn the_selection_stays_within_the_list() {
        let mut search = Search::default();
        search.step(Direction::Up, 3);
        assert_eq!(search.selected, 0);
        for _ in 0..5 {
            search.step(Direction::Down, 3);
        }
        assert_eq!(search.selected, 2);
    }
}
