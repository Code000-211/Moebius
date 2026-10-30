//! A chat's conversation, wherever it is shown: the `Place` that says what its clicks and reads become, the
//! `Thread` of events folded into a conversation, and how the agent's Markdown and a person's messages are drawn.

use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use iced::widget::text::Wrapping;
use iced::widget::{column, container, markdown, text};
use iced::{Border, Element, Length, Padding, Task, border};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Judgement, RunEvent};
use crate::settings::Settings;
use crate::widgets::conversation::{Conversation, Entry, Said};
use crate::widgets::page;
use crate::{theme, widgets::steps, widgets::transcript};

use super::Message;

/// Where a chat's conversation is shown: the chat page or the floating chat. It says what the conversation's clicks
/// and reads become.
pub trait Place: 'static {
    /// A step of the agent was clicked; the number is its entry's.
    fn fold(step: usize) -> AppMessage;
    /// A link of the agent's text was clicked.
    fn link(uri: String) -> AppMessage;
    /// The events of the chat were read, those after the ones already held.
    fn loaded(chat: i64, events: Result<Vec<RunEvent>, ClientError>) -> AppMessage;
    /// The judgements of the chat's tool calls were read.
    fn judged(chat: i64, judgements: Result<Vec<Judgement>, ClientError>) -> AppMessage;
}

/// The chat page.
pub struct OnPage;

impl Place for OnPage {
    fn fold(step: usize) -> AppMessage {
        AppMessage::Chat(Message::Fold(step))
    }

    fn link(uri: String) -> AppMessage {
        AppMessage::Chat(Message::LinkClicked(uri))
    }

    fn loaded(chat: i64, events: Result<Vec<RunEvent>, ClientError>) -> AppMessage {
        AppMessage::Chat(Message::Loaded(chat, events))
    }

    fn judged(chat: i64, judgements: Result<Vec<Judgement>, ClientError>) -> AppMessage {
        AppMessage::Chat(Message::Judged(chat, judgements))
    }
}

/// The conversation of one chat as it is followed: what was said and done, who decided its tool calls, the agent's
/// texts parsed as Markdown and the steps unfolded. The chat page and the floating chat each hold one.
pub struct Thread {
    pub id: i64,
    conversation: Conversation,
    /// The judgements of the chat's tool calls, oldest first, which say who decided each check.
    judgements: Vec<Judgement>,
    /// The agent's texts parsed, by the index of the entry, with the text they were parsed from.
    markdown: HashMap<usize, (String, Vec<markdown::Item>)>,
    /// The steps that are unfolded, by the index of their entry.
    unfolded: HashSet<usize>,
}

impl Thread {
    pub fn new(id: i64) -> Self {
        Thread {
            id,
            conversation: Conversation::default(),
            judgements: Vec::new(),
            markdown: HashMap::new(),
            unfolded: HashSet::new(),
        }
    }

    /// The `seq` of the last event held.
    pub fn last_seq(&self) -> i64 {
        self.conversation.last_seq()
    }

    pub fn push(&mut self, event: RunEvent) {
        // An event in order changes at most the last entry and adds new ones; one out of order rebuilds them all.
        let from = if event.seq > self.conversation.last_seq() {
            self.conversation.entries().len().saturating_sub(1)
        } else {
            0
        };
        // A streamed piece of text changes no tool call, so the deciders stay as they are named.
        let names = event.kind != "message_update";
        self.conversation.push(event);
        if names {
            self.conversation.name_deciders(&self.judgements);
        }
        for (index, entry) in self.conversation.entries().iter().enumerate().skip(from) {
            if let Entry::Text(text) = entry
                && self
                    .markdown
                    .get(&index)
                    .is_none_or(|(held, _)| held != text)
            {
                let items = markdown::parse(text).collect();
                self.markdown.insert(index, (text.clone(), items));
            }
        }
    }

    /// Who decided the chat's tool calls, as read again.
    pub fn judge(&mut self, judgements: Vec<Judgement>) {
        self.judgements = judgements;
        self.conversation.name_deciders(&self.judgements);
    }

    /// Unfolds the step when folded and folds it when unfolded.
    pub fn fold(&mut self, step: usize) {
        if !self.unfolded.remove(&step) {
            self.unfolded.insert(step);
        }
    }

    /// The conversation: the agent's texts as Markdown, the person's messages on the right, and the steps that follow
    /// one another as one box, the last one working while the chat's `status` says the agent works.
    pub fn view<P: Place>(&self, status: &str, now: i64) -> Element<'_, AppMessage> {
        let conversation = &self.conversation;
        let entries = conversation.entries();
        if entries.is_empty() {
            return page::body("Nothing has been said in this chat yet.");
        }
        let working = matches!(status, "starting" | "running");
        let mut views: Vec<Element<'_, AppMessage>> = Vec::new();
        let mut index = 0;
        while index < entries.len() {
            match &entries[index] {
                Entry::Text(_) => views.push(agent_text::<P>(self.markdown.get(&index))),
                Entry::Person(said, message) => views.push(person_view(*said, message)),
                Entry::Error(message) => views.push(transcript::error(message)),
                Entry::McpError { server, message } => {
                    views.push(transcript::mcp_error(server, message))
                }
                Entry::Thinking(_) | Entry::Tool(_) => {
                    // The steps that follow one another are one box.
                    let end = entries[index..]
                        .iter()
                        .position(|entry| !matches!(entry, Entry::Thinking(_) | Entry::Tool(_)))
                        .map_or(entries.len(), |len| index + len);
                    let last = end == entries.len();
                    let stops = if last {
                        if working { now } else { conversation.last_at() }
                    } else {
                        conversation.started(end)
                    };
                    views.push(steps::steps_box(
                        &entries[index..end],
                        index,
                        last && working,
                        stops - conversation.started(index),
                        &self.unfolded,
                        P::fold,
                        false,
                    ));
                    index = end;
                    continue;
                }
            }
            index += 1;
        }
        column(views)
            .spacing(theme::SPACE_12)
            .width(Length::Fill)
            .into()
    }
}

/// Reads the events of the chat after `after`, and who decided its tool calls, for the conversation shown at `P`.
pub fn load<P: Place>(settings: Settings, chat: i64, after: i64) -> Task<AppMessage> {
    let events = Task::perform(
        {
            let settings = settings.clone();
            async move { Client::new(settings).run_events(chat, after).await }
        },
        move |events| P::loaded(chat, events),
    );
    Task::batch([events, load_judgements::<P>(settings, chat)])
}

/// Reads who decided the tool calls of the chat, for the conversation shown at `P`.
pub fn load_judgements<P: Place>(settings: Settings, chat: i64) -> Task<AppMessage> {
    Task::perform(
        async move { Client::new(settings).tool_judgements(chat).await },
        move |judgements| P::judged(chat, judgements),
    )
}

/// The viewer of the agent's Markdown: a link click is a message of the place it is shown at, and a code block is
/// drawn as a file is.
struct Markdown<P>(PhantomData<P>);

impl<'a, P: Place> markdown::Viewer<'a, AppMessage> for Markdown<P> {
    fn on_link_click(uri: markdown::Uri) -> AppMessage {
        P::link(uri)
    }

    fn code_block(
        &self,
        _settings: markdown::Settings,
        language: Option<&'a str>,
        code: &'a str,
        _lines: &'a [markdown::Text],
    ) -> Element<'a, AppMessage> {
        transcript::code_block(language.unwrap_or("txt"), code)
    }
}

/// The sizes and fonts of the agent's Markdown.
fn markdown_settings() -> markdown::Settings {
    let style = markdown::Style {
        font: theme::MANROPE,
        inline_code_highlight: markdown::Highlight {
            background: theme::HEAD_PILL.into(),
            border: border::rounded(theme::RADIUS_SMALL),
        },
        inline_code_padding: Padding::from([0.0, theme::SPACE_4]),
        inline_code_color: theme::TEXT,
        inline_code_font: theme::MONO,
        code_block_font: theme::MONO,
        link_color: theme::TEXT,
    };
    let mut settings = markdown::Settings::with_text_size(theme::BODY, style);
    settings.h1_size = theme::PAGE_TITLE.into();
    settings.h2_size = theme::BOX_HEADING.into();
    settings.h3_size = theme::BODY.into();
    settings.h4_size = theme::BODY.into();
    settings.code_size = theme::CODE.into();
    settings.spacing = theme::SPACE_10.into();
    settings
}

/// A text of the agent, as Markdown once it is parsed.
fn agent_text<P: Place>(parsed: Option<&(String, Vec<markdown::Item>)>) -> Element<'_, AppMessage> {
    let items = parsed.map_or(&[][..], |(_, items)| items.as_slice());
    let source = parsed.map_or("", |(source, _)| source.as_str());
    container(
        column![
            markdown::view_with(items, markdown_settings(), &Markdown::<P>(PhantomData)),
            transcript::copy_button(source, theme::TEXT_MUTED),
        ]
        .spacing(theme::SPACE_4),
    )
    .max_width(theme::PROSE_WIDTH)
    .into()
}

/// A message of the person: a box on the right.
fn person_view(said: Said, message: &str) -> Element<'_, AppMessage> {
    let mut content = column![].spacing(theme::SPACE_4);
    if said == Said::Steered {
        content = content.push(
            text("You steered the chat")
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_SECONDARY),
        );
    }
    let bubble = container(
        content.push(
            text(message)
                .size(theme::BODY)
                .color(theme::TEXT)
                .wrapping(Wrapping::WordOrGlyph),
        ),
    )
    .padding([theme::SPACE_8, theme::SPACE_12])
    .max_width(theme::MESSAGE_WIDTH)
    .style(|_| container::Style {
        background: Some(theme::LANE.into()),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_CARD.into(),
        },
        ..container::Style::default()
    });
    container(bubble).align_right(Length::Fill).into()
}
