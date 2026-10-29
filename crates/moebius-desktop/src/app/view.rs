//! How the app is drawn: the sidebar, the page with its link back, the floating chat and the search list over it,
//! and the subscriptions the page shown needs.

use std::time::Duration;

use iced::widget::{column, container, opaque, row, stack};
use iced::{Element, Length, Padding, Subscription};

use crate::client;
use crate::screens::fleet;
use crate::screens::inbox;
use crate::screens::live;
use crate::screens::project::{self, Tab};
use crate::search;
use crate::widgets::float_chat;
use crate::{
    screens::board, screens::chat, screens::connect, screens::history, sidebar, theme,
    widgets::page,
};

use super::message::{Message, Page};
use super::navigation::back_key;
use super::{App, Opened, Screen};

impl App {
    pub fn subscription(&self) -> Subscription<Message> {
        match (&self.screen, &self.settings) {
            (Screen::Main, Some(settings)) => {
                let stream = Subscription::run_with(settings.clone(), |settings| {
                    client::events(settings.clone())
                })
                .map(Message::Stream);
                // The time a run started is told as how long ago, so the page needs the clock while it is open.
                let clock = match self.opened {
                    Some(Opened::Run(_)) => iced::time::every(Duration::from_secs(30))
                        .map(|_| Message::Live(live::Message::Tick(live::now()))),
                    _ => Subscription::none(),
                };
                let inbox = match self.page() {
                    Page::Inbox => iced::time::every(Duration::from_secs(30))
                        .map(|_| Message::Inbox(inbox::Message::Tick(live::now()))),
                    _ => Subscription::none(),
                };
                let fleet = match self.page() {
                    Page::Fleet => iced::time::every(Duration::from_secs(5))
                        .map(|_| Message::Fleet(fleet::Message::Tick(live::now()))),
                    _ => Subscription::none(),
                };
                let chats = match self.page() {
                    Page::Chat => iced::time::every(Duration::from_secs(30))
                        .map(|_| Message::Chat(chat::Message::Tick(live::now()))),
                    _ => Subscription::none(),
                };
                let shell = match &self.opened {
                    Some(Opened::Run(live)) => live.subscription(settings),
                    _ => Subscription::none(),
                };
                let terminal = match self.page() {
                    Page::Terminal => self.terminal.subscription(settings),
                    _ => Subscription::none(),
                };
                let chat_terminal = match self.page() {
                    Page::Chat => self.chats.terminal_subscription(settings),
                    _ => Subscription::none(),
                };
                let floating = if self.float.working() {
                    iced::time::every(Duration::from_secs(30))
                        .map(|_| Message::Float(float_chat::Message::Tick(live::now())))
                } else {
                    Subscription::none()
                };
                let keys = iced::event::listen_with(search::key);
                let save = iced::event::listen_with(project::key);
                let back = iced::event::listen_with(back_key);
                Subscription::batch([
                    stream,
                    clock,
                    inbox,
                    fleet,
                    chats,
                    shell,
                    terminal,
                    chat_terminal,
                    floating,
                    keys,
                    save,
                    back,
                ])
            }
            _ => Subscription::none(),
        }
    }

    pub fn theme(&self) -> iced::Theme {
        theme::theme()
    }

    pub fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::Connect => connect::view(&self.form, self.settings.as_ref()),
            Screen::Main => {
                let page = self.page();
                let shell = row![
                    sidebar::view(
                        &self.snapshot,
                        self.connection,
                        &page,
                        sidebar::Counts {
                            chats_unread: self.chats.unread(),
                            working: fleet::working(&self.snapshot, self.chats.list()).len(),
                            tokens_today: self.usage.today(),
                            setup_incomplete: self.start.incomplete(),
                        },
                        self.collapsed,
                        &self.search.text
                    ),
                    container(self.page_with_back(page))
                        .padding(theme::PAGE_PADDING)
                        .width(Length::Fill)
                        .height(Length::Fill),
                ];
                // The floating chat stands over the page, except on the Chat page, which holds every chat and whose
                // own Send is where the button would stand.
                let floating: Element<'_, Message> = if page == Page::Chat {
                    iced::widget::space().into()
                } else {
                    self.float.view(self.float_place())
                };
                if !self.search.is_open() {
                    return stack![shell, floating].into();
                }
                // The list stands under the field, over the page.
                let entries = self.search_entries();
                let list = container(opaque(self.search.view(&entries)))
                    .padding(Padding {
                        top: 100.0,
                        left: 12.0,
                        ..Padding::ZERO
                    })
                    .width(Length::Fill)
                    .height(Length::Fill);
                stack![shell, floating, list].into()
            }
        }
    }

    /// The page, under the link back to the place left last when it was reached from there.
    pub(super) fn page_with_back(&self, page: Page) -> Element<'_, Message> {
        match self.history.last() {
            Some(left) => column![
                page::back(self.location_title(left), Message::Back),
                self.page_view(page)
            ]
            .spacing(10)
            .into(),
            None => self.page_view(page),
        }
    }

    pub(super) fn page_view(&self, page: Page) -> Element<'_, Message> {
        match page {
            Page::Board(id) => {
                let Some(board) = self.snapshot.boards.iter().find(|b| b.repository.id == id)
                else {
                    // A board that is gone (its repository was removed) leaves the inbox.
                    return self.inbox.view(&self.snapshot);
                };
                let card_of = |id| board.cards.iter().find(|card| card.id == id);
                match &self.opened {
                    Some(Opened::Run(live)) => card_of(live.card)
                        .map(|card| live.view(board, card, self.edits.card_view(card.id))),
                    Some(Opened::History { card, entries, now }) => card_of(*card).map(|card| {
                        history::view(
                            board,
                            card,
                            entries.as_deref(),
                            *now,
                            self.edits.card_view(card.id),
                        )
                    }),
                    Some(Opened::Script(editor)) => Some(editor.view()),
                    None => None,
                }
                .unwrap_or_else(|| {
                    let project = self.projects.get(&id);
                    let tabs = project::tabs(id, project);
                    match project {
                        Some(project) if project.tab() != Tab::Board => {
                            project.view(&board.repository, tabs)
                        }
                        _ => board::view(
                            board,
                            &self.snapshot.inbox,
                            &self.new_issue,
                            &self.edits,
                            tabs,
                        ),
                    }
                })
            }
            Page::AddRepository => self.add_repository.view(),
            Page::Inbox => self.inbox.view(&self.snapshot),
            Page::Chat => self.chats.view(&self.snapshot, &self.inbox),
            Page::Fleet => self.fleet.view(&self.snapshot, self.chats.list()),
            Page::Agents => self.agents.view(&self.harness),
            Page::Skills => self.skills.view(&self.harness),
            Page::McpServers => self.mcp_servers.view(&self.harness),
            Page::Usage => self.usage.view(),
            Page::Retro => self.retros.view(),
            Page::Rules => self.rules.view(),
            Page::Experiments => self.experiments.view(&self.snapshot),
            Page::DecisionModel => self.decision_model.view(),
            Page::Start => self.start.view(&self.harness),
            Page::Models => self.setup.models_page(),
            Page::Plugins => self.setup.plugins_page(),
            Page::Terminal => self.terminal.view(),
            Page::Settings => self.setup.settings_page(self.settings.as_ref()),
        }
    }
}
