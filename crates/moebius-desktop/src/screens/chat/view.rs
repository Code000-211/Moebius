//! How the chat screen is drawn: the list of chats, and the chat chosen with its actions, the change it offered and
//! the field to write in.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::client::{Change, Chat, InboxItem, Snapshot};
use crate::screens::inbox::{self, Action, Inbox};
use crate::widgets::page::{self, Tone, tag};
use crate::{screens::connect, theme};

use super::thread::OnPage;
use super::{Chats, GRILLING_AGENT, MESSAGE_FIELD, Message, Open};

/// Where the chat works: the name of its repository when the snapshot has it, or the harness configuration.
fn place_name<'a>(snapshot: &'a Snapshot, chat: &Chat) -> Option<&'a str> {
    if chat.harness_repository {
        return Some(HARNESS_PLACE);
    }
    snapshot
        .boards
        .iter()
        .find(|board| Some(board.repository.id) == chat.repository_id)
        .map(|board| board.repository.name.as_str())
}

/// How the harness configuration repository is called where a chat's place is named.
pub(super) const HARNESS_PLACE: &str = "harness configuration";

/// What the chat's status says on its tag: the colour tells whether the agent works or waits for the person.
fn status_tag<'a>(status: &str) -> Element<'a, AppMessage> {
    let (label, tone) = match status {
        "starting" => ("Starting", Tone::Teal),
        "running" => ("Running", Tone::Teal),
        "waiting" => ("Waiting", Tone::Amber),
        "succeeded" => ("Ended", Tone::Plain),
        "failed" => ("Failed", Tone::Red),
        "stopped" => ("Stopped", Tone::Plain),
        other => (other, Tone::Plain),
    };
    tag(label.to_string(), tone)
}

/// A chat of the list: its title, status, agent and repository, and when it started.
fn chat_row<'a>(
    chat: &'a Chat,
    repository: Option<&'a str>,
    now: i64,
    chosen: bool,
) -> Element<'a, AppMessage> {
    let mut meta = vec![chat.agent.as_str()];
    meta.extend(repository);
    let content = column![
        row![
            status_tag(&chat.status),
            space::horizontal(),
            text(page::ago(now, chat.started_at))
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
        ]
        .align_y(Alignment::Center),
        text(chat.title.as_str())
            .size(theme::BODY)
            .font(theme::manrope(Weight::Semibold))
            .wrapping(Wrapping::WordOrGlyph),
        text(meta.join(" · "))
            .size(theme::LABEL)
            .font(theme::MONO)
            .color(theme::TEXT_SECONDARY),
    ]
    .spacing(theme::SPACE_6);
    button(content)
        .width(Length::Fill)
        .padding([theme::SPACE_8, theme::SPACE_10])
        .on_press(AppMessage::Chat(Message::Select(chat.id)))
        .style(move |_, status| {
            let (fill, line) = match (chosen, status) {
                (true, _) => (theme::LANE, theme::TEXT),
                (false, button::Status::Hovered) => (theme::LANE, theme::LINE),
                (false, _) => (theme::SURFACE, theme::TRACK),
            };
            button::Style {
                background: Some(fill.into()),
                text_color: theme::TEXT,
                border: Border {
                    color: line,
                    width: 1.0,
                    radius: theme::RADIUS_CARD.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

/// The switch between a chat's conversation and its terminal, for a chat that is not over (the caller leaves it out
/// for one that is). The terminal opens once the agent has answered: while it works its process has the worktree.
fn switch<'a>(chat: &Chat, terminal: bool) -> Element<'a, AppMessage> {
    let chip = |label: &str, selected: bool, shown: bool| {
        page::chip(
            label.to_string(),
            selected,
            false,
            AppMessage::Chat(Message::ShowTerminal(shown)),
        )
    };
    let mut switch = row![chip("Chat", !terminal, false)]
        .spacing(theme::SPACE_6)
        .align_y(Alignment::Center);
    if terminal || chat.status == "waiting" {
        switch = switch.push(chip("Terminal", terminal, true));
    } else {
        switch = switch.push(iced::widget::tooltip(
            container(
                text("Terminal")
                    .size(theme::BREADCRUMB)
                    .font(theme::manrope(Weight::Semibold))
                    .color(theme::TEXT_MUTED),
            )
            .padding([theme::SPACE_4, theme::SPACE_10]),
            container(
                text("Opens when the agent has answered")
                    .size(theme::LABEL)
                    .font(theme::MONO),
            )
            .padding([theme::SPACE_4, theme::SPACE_8])
            .style(|_| page::box_style()),
            iced::widget::tooltip::Position::Bottom,
        ));
    }
    switch.into()
}

/// The right of the chat's head, while it lasts: offering what its worktree changes, stopping its agent while it works
/// (or the question that confirms stopping), and ending a grilling, which ends in the issues it proposes. Other chats
/// are not ended here: they wait for the next message as long as the person likes.
fn actions<'a>(chat: &Chat, open: &Open) -> Element<'a, AppMessage> {
    let mut actions = row![].spacing(theme::SPACE_10).align_y(Alignment::Center);
    if !chat.is_open() {
        return actions.into();
    }
    if open.confirming {
        actions = actions
            .push(
                text("Stop this chat?")
                    .size(theme::CONTEXT)
                    .font(theme::MONO)
                    .color(theme::TEXT_SECONDARY),
            )
            .push(page::white_button(
                "Keep going",
                Some(AppMessage::Chat(Message::KeepGoing)),
            ))
            .push(page::white_button(
                "Stop chat",
                Some(AppMessage::Chat(Message::Stop)),
            ));
    } else {
        if chat.changed_files > 0 {
            actions = actions.push(offer_button(chat));
        }
        if chat.agent == GRILLING_AGENT {
            actions = actions.push(page::white_button(
                "End chat",
                Some(AppMessage::Chat(Message::End)),
            ));
        }
        if chat.is_working() {
            actions = actions.push(page::white_button(
                "Stop",
                Some(AppMessage::Chat(Message::AskStop)),
            ));
        }
    }
    actions.into()
}

/// "Offer changes (n files)": pressed, the chat's work is offered as a change. While the agent works its worktree may be
/// half edited, so the button is muted and says on hover when it can be pressed.
fn offer_button<'a>(chat: &Chat) -> Element<'a, AppMessage> {
    let label = match chat.changed_files {
        1 => "Offer changes (1 file)".to_string(),
        n => format!("Offer changes ({n} files)"),
    };
    if !chat.is_working() {
        return page::white_button(label, Some(AppMessage::Chat(Message::Offer)));
    }
    iced::widget::tooltip(
        page::white_button(label, None),
        container(
            text("Offer once the agent has answered")
                .size(theme::LABEL)
                .font(theme::MONO),
        )
        .padding([theme::SPACE_4, theme::SPACE_8])
        .style(|_| page::box_style()),
        iced::widget::tooltip::Position::Bottom,
    )
    .into()
}

/// What stands in the right pane when no chat is chosen.
fn empty_view(line: &str) -> Element<'_, AppMessage> {
    container(page::body(line))
        .padding(theme::BOX_PADDING)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| page::box_style())
        .into()
}

/// The change the chat offered, found by its branch among the board's changes, and the inbox item that waits for it
/// to be merged; none while the chat has offered nothing.
fn offered_change<'a>(
    chat: &Chat,
    snapshot: &'a Snapshot,
) -> Option<(&'a Change, Option<&'a InboxItem>)> {
    let branch = chat.branch();
    let change = snapshot
        .boards
        .iter()
        .flat_map(|board| &board.changes)
        .chain(&snapshot.harness_changes)
        .rfind(|change| change.branch == branch)?;
    let item = snapshot
        .inbox
        .iter()
        .find(|item| item.chat() == Some(chat.id));
    Some((change, item))
}

/// The change a chat offered: its state, its branch and, while a person can merge it, the Merge button.
fn change_view<'a>(
    change: &'a Change,
    item: Option<&InboxItem>,
    inbox: &'a Inbox,
    merge_is_primary: bool,
) -> Element<'a, AppMessage> {
    let (label, tone) = match change.state.as_str() {
        "merged" => ("Merged", Tone::Teal),
        "closed" => ("Closed", Tone::Plain),
        _ => ("Change offered", Tone::Amber),
    };
    let mut head = row![
        tag(label.to_string(), tone),
        text(change.branch.as_str())
            .font(theme::MONO)
            .size(theme::CODE)
            .wrapping(Wrapping::WordOrGlyph),
        space::horizontal(),
    ]
    .spacing(theme::SPACE_10)
    .align_y(Alignment::Center);
    if let Some(item) = item {
        if let Some(progress) = inbox.status(item.id) {
            head = head.push(
                text(progress)
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TEXT_SECONDARY),
            );
        }
        let merge = Action::Merge { change: change.id };
        let message = Some(AppMessage::Inbox(inbox::Message::Act(item.id, merge)));
        head = head.push(if merge_is_primary {
            page::primary_button("Merge", message)
        } else {
            page::white_button("Merge", message)
        });
    }
    container(head)
        .padding([theme::SPACE_8, theme::SPACE_10])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::LANE.into()),
            border: Border {
                color: theme::TRACK,
                width: 1.0,
                radius: theme::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// The field to write a message in; Enter sends when `send` is a message.
pub(super) fn message_field(value: &str, send: Option<AppMessage>) -> Element<'_, AppMessage> {
    text_input("Write to the agent", value)
        .id(MESSAGE_FIELD)
        .on_input(|text| AppMessage::Chat(Message::Edited(text)))
        .on_submit_maybe(send)
        .size(theme::BODY)
        .padding(theme::FIELD_PADDING)
        .style(connect::input_style)
        .into()
}

impl Chats {
    /// The page: the header, the list of chats, and the form of a new chat or the chat chosen.
    pub fn view<'a>(&'a self, snapshot: &'a Snapshot, inbox: &'a Inbox) -> Element<'a, AppMessage> {
        let line = match self.waiting() {
            0 => "Talk to an agent. What it builds in a repository is offered as a change.".into(),
            1 => "1 chat waits for your next message.".to_string(),
            n => format!("{n} chats wait for your next message."),
        };
        let header = page::header(
            "",
            "Chat",
            None,
            page::context(line),
            page::white_button("New chat", Some(AppMessage::Chat(Message::NewChat))),
        );
        let detail: Element<_> = match (&self.draft, &self.open) {
            (Some(draft), _) => self.draft_view(draft, snapshot),
            (None, Some(open)) => match self.chat(open.thread.id) {
                Some(chat) => self.chat_view(chat, open, snapshot, inbox),
                None => empty_view("Reading the chat…"),
            },
            (None, None) => empty_view("Choose a chat, or start a new one."),
        };
        column![
            header,
            row![self.list_view(snapshot), detail]
                .spacing(theme::PAGE_GAP)
                .height(Length::Fill),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }

    /// The chats, newest first, the chosen one marked.
    pub(super) fn list_view<'a>(&'a self, snapshot: &'a Snapshot) -> Element<'a, AppMessage> {
        let chosen = self
            .open
            .as_ref()
            .filter(|_| self.draft.is_none())
            .map(|open| open.thread.id);
        let list: Element<_> = if self.listed().next().is_none() {
            page::body("No chat yet.")
        } else {
            column(self.listed().map(|chat| {
                let repository = place_name(snapshot, chat);
                chat_row(chat, repository, self.now, chosen == Some(chat.id))
            }))
            .spacing(theme::SPACE_8)
            .into()
        };
        container(scrollable(list).height(Length::Fill))
            .padding(theme::SPACE_12)
            .width(theme::LIST_WIDTH)
            .height(Length::Fill)
            .style(|_| page::box_style())
            .into()
    }

    fn chat_view<'a>(
        &'a self,
        chat: &'a Chat,
        open: &'a Open,
        snapshot: &'a Snapshot,
        inbox: &'a Inbox,
    ) -> Element<'a, AppMessage> {
        let repository = place_name(snapshot, chat);
        // The title takes what the tag leaves, so a long one wraps instead of squeezing the tag.
        let title = row![
            text(chat.title.as_str())
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold))
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
            status_tag(&chat.status),
        ]
        .spacing(theme::SPACE_10)
        .align_y(Alignment::Center);
        let who = match repository {
            Some(_) if chat.harness_repository => {
                format!("{} in the harness configuration", chat.agent)
            }
            Some(name) => format!("{} in {name}", chat.agent),
            None => format!("{}, no repository", chat.agent),
        };
        let mut head = row![
            column![title, page::context(who)]
                .spacing(theme::SPACE_2)
                .width(Length::Fill),
        ]
        .spacing(theme::SPACE_10)
        .align_y(Alignment::Center);
        if chat.is_open() {
            head = head.push(switch(chat, self.terminal.is_shown()));
        }
        head = head.push(actions(chat, open));
        if let Some(terminal) = self.terminal.view() {
            let content = column![
                container(head).padding(theme::BOX_PADDING),
                page::rule(),
                terminal
            ];
            return container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| page::box_style())
                .into();
        }
        let conversation = scrollable(
            container(open.thread.view::<OnPage>(&chat.status, self.now))
                .padding(theme::SPACE_16)
                .width(Length::Fill),
        )
        .anchor_bottom()
        .height(Length::Fill);
        let mut content = column![
            container(head).padding(theme::BOX_PADDING),
            page::rule(),
            conversation
        ];
        let offered = offered_change(chat, snapshot);
        // Merge is what an ended chat with a change to merge is for, so it takes the primary place from Send.
        let merge_is_primary = !chat.is_open() && offered.is_some_and(|(_, item)| item.is_some());
        if let Some((change, item)) = offered {
            let offer = change_view(change, item, inbox, merge_is_primary);
            content = content.push(container(offer).padding([theme::SPACE_8, theme::BOX_PADDING]));
        }
        container(
            content
                .push(page::rule())
                .push(self.compose_view(chat, open, !merge_is_primary)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| page::box_style())
        .into()
    }
}
