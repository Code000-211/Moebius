//! A chat not yet started: who it is with, where, on which model and reasoning level, and the small pickers under
//! the field to write in that choose them, for a draft and for a waiting chat.

use std::fmt;

use iced::font::Weight;
use iced::widget::{column, container, pick_list, row, text};
use iced::{Alignment, Border, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Agent, Chat, Client, Snapshot};
use crate::settings::Settings;
use crate::theme;
use crate::widgets::form::{LEVELS, menu_style, pick_style};
use crate::widgets::page;

use super::view::{HARNESS_PLACE, message_field};
use super::{Chats, Draft, GRILLING_AGENT, Message, Open};

/// Reads the agents and the models a chat's pickers offer.
pub(super) fn load_choices(settings: &Settings) -> Task<AppMessage> {
    let (agents, models) = (Client::new(settings.clone()), Client::new(settings.clone()));
    Task::batch([
        Task::perform(async move { agents.agents().await }, |agents| {
            AppMessage::Chat(Message::Agents(agents))
        }),
        Task::perform(
            async move { models.available_models(false, false).await },
            |models| AppMessage::Chat(Message::Models(models)),
        ),
    ])
}

/// Where a new chat works, as its picker offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PlaceChoice {
    Nowhere,
    Repository(i64, String),
    Harness,
}

impl fmt::Display for PlaceChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaceChoice::Nowhere => f.write_str("No repository"),
            PlaceChoice::Repository(_, name) => f.write_str(name),
            PlaceChoice::Harness => f.write_str("Harness configuration"),
        }
    }
}

/// A small picker of the row under the field to write in, headed by `label`; without `on_pick` it only shows what is
/// chosen, muted.
fn picker<'a, T>(
    label: &'static str,
    choices: Vec<T>,
    chosen: Option<T>,
    placeholder: &'static str,
    on_pick: Option<fn(T) -> Message>,
) -> Element<'a, AppMessage>
where
    T: fmt::Display + Clone + PartialEq + 'a,
{
    let label = text(label)
        .size(theme::LABEL)
        .font(theme::MONO)
        .color(theme::TEXT_MUTED);
    let field: Element<'a, AppMessage> = match on_pick {
        Some(on_pick) => pick_list(choices, chosen, move |choice| {
            AppMessage::Chat(on_pick(choice))
        })
        .placeholder(placeholder)
        .text_size(theme::LABEL)
        .padding([theme::SPACE_4, theme::SPACE_8])
        .style(pick_style)
        .menu_style(menu_style)
        .into(),
        None => container(
            text(chosen.map_or(placeholder.to_string(), |chosen| chosen.to_string()))
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
        )
        .padding([theme::SPACE_4, theme::SPACE_8])
        .style(|_| container::Style {
            border: Border {
                color: theme::LINE,
                width: 1.0,
                radius: theme::RADIUS_CONTROL.into(),
            },
            ..container::Style::default()
        })
        .into(),
    };
    row![label, field]
        .spacing(theme::SPACE_6)
        .align_y(Alignment::Center)
        .into()
}

impl Chats {
    /// The model picker: the models the upstreams serve, and `current` as it is when they do not list it.
    pub(super) fn model_picker<'a>(
        &self,
        current: Option<&str>,
        enabled: bool,
    ) -> Element<'a, AppMessage> {
        let mut models = self.models.clone();
        if let Some(current) = current
            && !models.iter().any(|model| model == current)
        {
            models.push(current.to_string());
        }
        let on_pick = enabled.then_some(Message::ModelChosen as fn(String) -> Message);
        picker("Model", models, current.map(String::from), "Model", on_pick)
    }

    /// The reasoning picker: the levels Pi-Bolt takes, `current` chosen.
    fn reasoning_picker<'a>(
        &self,
        current: Option<&str>,
        enabled: bool,
    ) -> Element<'a, AppMessage> {
        let levels: Vec<String> = LEVELS.iter().map(|level| level.to_string()).collect();
        let on_pick = enabled.then_some(Message::ThinkingChosen as fn(String) -> Message);
        picker(
            "Reasoning",
            levels,
            current.map(String::from),
            "Not set",
            on_pick,
        )
    }

    /// The agent `name` as the server lists it.
    pub(super) fn agent(&self, name: &str) -> Option<&Agent> {
        self.agents.iter().find(|agent| agent.name == name)
    }

    /// The empty chat of a draft: a hint where the conversation will be, the field for the first message with Send
    /// and Terminal, and under them the pickers of the chat's agent, place, model and reasoning.
    pub(super) fn draft_view<'a>(
        &'a self,
        draft: &'a Draft,
        snapshot: &'a Snapshot,
    ) -> Element<'a, AppMessage> {
        let place = if draft.harness {
            Some(HARNESS_PLACE)
        } else {
            snapshot
                .boards
                .iter()
                .find(|board| Some(board.repository.id) == draft.repository)
                .map(|board| board.repository.name.as_str())
        };
        let who = match place {
            Some(name) if draft.harness => format!("{} in the {name}", draft.agent),
            Some(name) => format!("{} in {name}", draft.agent),
            None => format!("{}, no repository", draft.agent),
        };
        let head = column![
            text("New chat")
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold)),
            page::context(who),
        ]
        .spacing(theme::SPACE_2);
        let ready = draft.ready() && !draft.sending;
        let can_send = ready && !draft.message.trim().is_empty();
        let send = can_send.then_some(AppMessage::Chat(Message::Send));
        let terminal = ready.then_some(AppMessage::Chat(Message::StartInTerminal));
        let hint = if draft.harness {
            "The agent works on a branch of the harness configuration repository: agents, skills, extensions, MCP servers and rules. \"Offer changes\" offers what it built as a change, if every agent still works with it."
        } else if draft.agent == GRILLING_AGENT {
            "The Grilling partner questions your idea in a repository. Ending the chat puts the issues it proposes in the Inbox, to confirm."
        } else {
            "With a repository the agent works on its own branch. \"Offer changes\" offers what it built as a change."
        };
        let note = match &draft.notice {
            Some(notice) => notice.as_str(),
            None if draft.needs_model && draft.model.is_none() => {
                "This agent has no model yet: choose one below, or give the Coder one on the Agents page."
            }
            None if !draft.ready() => "A grilling needs a repository: choose one below.",
            None => "Write the first message to start the chat, or open it in the terminal.",
        };
        let agent = self.agent(&draft.agent);
        let names: Vec<String> = self.agents.iter().map(|agent| agent.name.clone()).collect();
        let mut places = vec![PlaceChoice::Nowhere];
        places.extend(snapshot.boards.iter().map(|board| {
            PlaceChoice::Repository(board.repository.id, board.repository.name.clone())
        }));
        if self.has_harness {
            places.push(PlaceChoice::Harness);
        }
        let chosen_place = if draft.harness {
            PlaceChoice::Harness
        } else {
            places
                .iter()
                .find(|place| matches!(place, PlaceChoice::Repository(id, _) if Some(*id) == draft.repository))
                .cloned()
                .unwrap_or(PlaceChoice::Nowhere)
        };
        let to_place: fn(PlaceChoice) -> Message = |place| match place {
            PlaceChoice::Nowhere => Message::RepositoryChosen(None),
            PlaceChoice::Repository(id, _) => Message::RepositoryChosen(Some(id)),
            PlaceChoice::Harness => Message::HarnessChosen,
        };
        let model = draft.model.as_deref().or(agent
            .map(|agent| agent.model.as_str())
            .filter(|model| !model.is_empty()));
        let thinking = draft
            .thinking
            .as_deref()
            .or(agent.and_then(|agent| agent.thinking.as_deref()));
        let pickers = row![
            picker(
                "Agent",
                names,
                Some(draft.agent.clone()),
                "Agent",
                Some(Message::AgentChosen as fn(String) -> Message),
            ),
            picker("Place", places, Some(chosen_place), "Place", Some(to_place)),
            self.model_picker(model, true),
            self.reasoning_picker(thinking, true),
        ]
        .spacing(theme::SPACE_12)
        .align_y(Alignment::Center);
        let compose = column![
            row![
                message_field(&draft.message, send.clone()),
                page::primary_button("Send", send),
                page::white_button("Terminal", terminal),
            ]
            .spacing(theme::SPACE_10)
            .align_y(Alignment::Center),
            pickers.wrap().vertical_spacing(theme::SPACE_8),
            text(note)
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
        ]
        .spacing(theme::SPACE_8)
        .padding([theme::SPACE_12, theme::BOX_PADDING]);
        container(column![
            container(head).padding(theme::BOX_PADDING),
            page::rule(),
            container(page::body(hint))
                .padding(theme::BOX_PADDING)
                .width(Length::Fill)
                .height(Length::Fill),
            page::rule(),
            compose,
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| page::box_style())
        .into()
    }

    /// The field to write in and the Send button, the primary one unless `send_is_primary` is false; under them the
    /// chat's model and reasoning, which change while it waits, and what the chat does with the message.
    pub(super) fn compose_view<'a>(
        &'a self,
        chat: &'a Chat,
        open: &'a Open,
        send_is_primary: bool,
    ) -> Element<'a, AppMessage> {
        let can_send = !open.message.trim().is_empty();
        let send = can_send.then_some(AppMessage::Chat(Message::Send));
        let field = message_field(&open.message, send.clone());
        let hint = match (&open.notice, chat.status.as_str()) {
            (Some(notice), _) => notice.as_str(),
            (None, "starting" | "running") => {
                "The agent is working. Your message is queued and sent when it is done with this turn."
            }
            (None, "waiting") => "The agent answers at once.",
            (None, _) => {
                "This chat has ended. What you send starts a new chat with the same agent, which is told this one."
            }
        };
        let send_button = if send_is_primary {
            page::primary_button("Send", send)
        } else {
            page::white_button("Send", send)
        };
        let mut fields = row![field, send_button]
            .spacing(theme::SPACE_10)
            .align_y(Alignment::Center);
        // As in a new chat, Terminal sits beside Send; it opens once the agent has answered, because while it works its
        // process has the worktree.
        if chat.is_open() {
            let terminal =
                (chat.status == "waiting").then_some(AppMessage::Chat(Message::ShowTerminal(true)));
            fields = fields.push(page::white_button("Terminal", terminal));
        }
        let mut compose = column![fields]
            .spacing(theme::SPACE_8)
            .padding([theme::SPACE_12, theme::BOX_PADDING]);
        if chat.is_open() {
            let agent = self.agent(&chat.agent);
            let model = chat
                .model
                .as_deref()
                .or(agent.map(|agent| agent.model.as_str()));
            let thinking = chat
                .thinking
                .as_deref()
                .or(agent.and_then(|agent| agent.thinking.as_deref()));
            let waiting = chat.status == "waiting";
            compose = compose.push(
                row![
                    self.model_picker(model, waiting),
                    self.reasoning_picker(thinking, waiting),
                ]
                .spacing(theme::SPACE_12)
                .align_y(Alignment::Center),
            );
        }
        compose
            .push(
                text(hint)
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED),
            )
            .into()
    }
}
