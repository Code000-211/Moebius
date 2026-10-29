//! What the floating chat tells the Moebius agent of the page: where the person is, what is open there and the text
//! being edited, and the repository a chat started there works in.

use crate::client;
use crate::screens::project::Tab;

use super::message::Page;
use super::{App, Opened};

/// How many characters of a text being edited the floating chat tells the agent, at most.
const EXCERPT: usize = 2_500;

/// `text` under `label`, cut to [`EXCERPT`] characters, saying so when it was cut.
fn excerpt(label: &str, text: &str) -> String {
    let count = text.chars().count();
    if count <= EXCERPT {
        return format!("{label}:\n{text}");
    }
    let start: String = text.chars().take(EXCERPT).collect();
    format!("{label} (the first {EXCERPT} of {count} characters):\n{start}")
}

impl App {
    /// The repository of the page shown, if it is a board's: a chat the floating chat starts there works in it.
    pub(super) fn page_repository(&self) -> Option<i64> {
        match self.page() {
            Page::Board(id) => Some(id),
            _ => None,
        }
    }

    /// Where the person is in the app, in a few lines for the Moebius agent: the page, the repository, what is open on
    /// it, and the text being edited, cut to a sensible size. The floating chat sends it with every message.
    pub fn page_context(&self) -> String {
        let page = self.page();
        // A project's page is named by its tab; a card's page belongs to the board.
        let project = match page {
            Page::Board(id) if self.opened.is_none() => self
                .projects
                .get(&id)
                .filter(|project| project.tab() != Tab::Board),
            _ => None,
        };
        let title = match (page, project) {
            (Page::Board(_), Some(project)) => project.tab().title(),
            _ => page.title(),
        };
        let mut lines = vec![match page.group() {
            "" => format!("Page: {title}"),
            group => format!("Page: {group} > {title}"),
        }];
        match page {
            Page::Board(id) => {
                let Some(board) = self.snapshot.boards.iter().find(|b| b.repository.id == id)
                else {
                    return lines.join("\n");
                };
                let repository = &board.repository;
                lines.push(format!(
                    "Repository: {} (id {})",
                    repository.name, repository.id
                ));
                if let Some(project) = project {
                    lines.extend(project.context());
                    return lines.join("\n");
                }
                let card_of = |id| board.cards.iter().find(|card| card.id == id);
                let describe = |card: &client::Card| {
                    format!(
                        "Open card: #{} {} (column {}, card id {})",
                        card.number, card.title, card.column, card.id
                    )
                };
                match &self.opened {
                    Some(Opened::Run(live)) => {
                        if let Some(card) = card_of(live.card) {
                            lines.push(describe(card));
                        }
                        let run = live.run();
                        lines.push(format!(
                            "Watching run {} of {}: {}",
                            run.id, run.agent, run.status
                        ));
                    }
                    Some(Opened::History { card, .. }) => {
                        if let Some(card) = card_of(*card) {
                            lines.push(describe(card));
                            lines
                                .push("The card has no run yet; its history is shown.".to_string());
                        }
                    }
                    Some(Opened::Script(editor)) => {
                        let path = match editor.path() {
                            "" => "not read yet",
                            path => path,
                        };
                        lines.push(format!(
                            "Editing the script of the {} column ({path})",
                            editor.column()
                        ));
                        lines.push(excerpt(
                            "The script as written in the editor",
                            &editor.request(),
                        ));
                    }
                    None => lines.push(format!(
                        "The board: {} cards, {} changes",
                        board.cards.len(),
                        board.changes.len()
                    )),
                }
            }
            Page::Chat => match self.chats.open_chat() {
                Some(chat) => lines.push(format!(
                    "Open chat {}: \"{}\" with {}, {}",
                    chat.id, chat.title, chat.agent, chat.status
                )),
                None => lines.push("No chat is open.".to_string()),
            },
            Page::Rules => lines.push(excerpt("The rules", &self.rules.context_text())),
            Page::Agents => match self.agents.editing() {
                Some(name) => lines.push(format!("Editing the agent {name}")),
                None => lines.push("No agent is open.".to_string()),
            },
            Page::Automations => lines.push(self.automations.context()),
            Page::Models => {
                lines.push("The upstreams, models and API keys the server uses.".to_string())
            }
            Page::Settings => lines
                .push("The settings of moebius.toml, edited while the server runs.".to_string()),
            Page::Inbox
            | Page::Fleet
            | Page::AddRepository
            | Page::Skills
            | Page::McpServers
            | Page::Retro
            | Page::Experiments
            | Page::DecisionModel
            | Page::Usage
            | Page::Start
            | Page::Plugins
            | Page::Terminal => {}
        }
        lines.join("\n")
    }

    /// Where the floating chat's chat works, or where its next message would start one: the page's repository.
    pub(super) fn float_place(&self) -> String {
        let repository = match self.float.chat() {
            Some(_) => self.float.repository(),
            None => self.page_repository(),
        };
        let name = self
            .snapshot
            .boards
            .iter()
            .find(|board| Some(board.repository.id) == repository)
            .map(|board| board.repository.name.as_str());
        match name {
            Some(name) => format!("Works in {name}"),
            None => "No repository".to_string(),
        }
    }
}
