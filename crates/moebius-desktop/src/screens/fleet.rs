//! The Fleet page: every run that is working, at a glance. One tile for each card whose latest run is starting,
//! running or waiting and for each chat that is, with who runs it, what for, how long, and the last steps of the run
//! in the compact box the chat draws in full. The tiles read their events when the page opens and follow the stream's
//! `run_event`s while it is open; leaving the page drops them. A click on a tile opens the run's live view or the chat.

use std::collections::{HashMap, HashSet};

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, responsive, row, scrollable, space, text};
use iced::{Alignment, Border, Element, Length, Padding, Task};

use crate::app::{Message as AppMessage, Page};
use crate::client::{Chat, Client, ClientError, RunEvent, Snapshot};
use crate::settings::Settings;
use crate::widgets::conversation::{Conversation, Entry};
use crate::widgets::page::{self, Tone, tag};
use crate::{screens::live, theme, widgets::steps};

/// The narrowest a tile is drawn, which sets how many fit in a row.
const MAX_COLUMNS: usize = 3;

/// What reading a run brings: when it started and its events after the ones already held.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub started_at: i64,
    pub events: Vec<RunEvent>,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The run with this id was read.
    Loaded(i64, Result<Loaded, ClientError>),
    /// The clock, for how long the runs have run.
    Tick(i64),
}

/// A run that is working, as the snapshot and the chats say.
#[derive(Debug, Clone, PartialEq)]
pub struct Work {
    pub run: i64,
    pub agent: String,
    /// `code`, `review`, ... or `chat`.
    pub kind: String,
    pub status: String,
    /// `#number title` of a card, the title of a chat.
    pub title: String,
    pub repository: String,
    pub started_at: i64,
    /// The tokens of the card since it was last retried; a chat's are not known.
    pub tokens: Option<i64>,
    /// The message that opens the card's live view or the chat.
    pub open: OpenTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenTarget {
    Card(i64),
    Chat(i64),
}

fn is_working(status: &str) -> bool {
    matches!(status, "starting" | "running" | "waiting")
}

/// Every working run, the oldest first.
pub fn working(snapshot: &Snapshot, chats: &[Chat]) -> Vec<Work> {
    let mut work = Vec::new();
    for board in &snapshot.boards {
        for card in &board.cards {
            let Some(run) = card.run.as_ref().filter(|run| is_working(&run.status)) else {
                continue;
            };
            work.push(Work {
                run: run.id,
                agent: run.agent.clone(),
                kind: run.purpose.clone(),
                status: run.status.clone(),
                title: format!("#{} {}", card.number, card.title),
                repository: board.repository.name.clone(),
                started_at: 0,
                tokens: Some(card.tokens).filter(|tokens| *tokens > 0),
                open: OpenTarget::Card(card.id),
            });
        }
    }
    for chat in chats.iter().filter(|chat| is_working(&chat.status)) {
        let repository = match (chat.harness_repository, chat.repository_id) {
            (true, _) => "harness configuration".to_string(),
            (false, Some(id)) => snapshot
                .boards
                .iter()
                .find(|board| board.repository.id == id)
                .map(|board| board.repository.name.clone())
                .unwrap_or_default(),
            (false, None) => String::new(),
        };
        work.push(Work {
            run: chat.id,
            agent: chat.agent.clone(),
            kind: "chat".to_string(),
            status: chat.status.clone(),
            title: if chat.title.is_empty() {
                "Chat".to_string()
            } else {
                chat.title.clone()
            },
            repository,
            started_at: chat.started_at,
            tokens: None,
            open: OpenTarget::Chat(chat.id),
        });
    }
    work.sort_by_key(|work| work.run);
    work
}

/// What the page says under its title.
pub fn context(count: usize) -> String {
    match count {
        0 => "Nothing is working.".to_string(),
        1 => "1 agent working".to_string(),
        n => format!("{n} agents working"),
    }
}

/// What is known of a run on show.
#[derive(Default)]
struct Tile {
    started_at: i64,
    conversation: Conversation,
}

#[derive(Default)]
pub struct Fleet {
    /// The tiles by run id; empty while another page is shown.
    tiles: HashMap<i64, Tile>,
    now: i64,
}

fn load(settings: Settings, run: i64, after: i64) -> Task<AppMessage> {
    Task::perform(
        async move {
            let client = Client::new(settings);
            Ok(Loaded {
                started_at: client.run(run).await?.started_at,
                events: client.run_events(run, after).await?,
            })
        },
        move |loaded| AppMessage::Fleet(Message::Loaded(run, loaded)),
    )
}

impl Fleet {
    pub fn new(now: i64) -> Self {
        Fleet {
            tiles: HashMap::new(),
            now,
        }
    }

    /// Drops the tiles, as the page is left.
    pub fn clear(&mut self) {
        self.tiles.clear();
    }

    /// Makes the tiles those of the working runs: a new one is read, one whose run is no longer working goes.
    pub fn follow(
        &mut self,
        snapshot: &Snapshot,
        chats: &[Chat],
        settings: &Settings,
    ) -> Task<AppMessage> {
        let work = working(snapshot, chats);
        let live: HashSet<i64> = work.iter().map(|work| work.run).collect();
        self.tiles.retain(|run, _| live.contains(run));
        let mut tasks = Vec::new();
        for work in work {
            if let std::collections::hash_map::Entry::Vacant(slot) = self.tiles.entry(work.run) {
                slot.insert(Tile {
                    started_at: work.started_at,
                    conversation: Conversation::default(),
                });
                tasks.push(load(settings.clone(), work.run, 0));
            }
        }
        Task::batch(tasks)
    }

    /// Reads what the tiles missed, as the stream was away.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        Task::batch(
            self.tiles
                .iter()
                .map(|(run, tile)| load(settings.clone(), *run, tile.conversation.last_seq())),
        )
    }

    /// A `run_event` of the stream, for the tile of its run if there is one.
    pub fn on_event(&mut self, run: i64, event: RunEvent) {
        if let Some(tile) = self.tiles.get_mut(&run) {
            tile.conversation.push(event);
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Loaded(run, Ok(loaded)) => {
                if let Some(tile) = self.tiles.get_mut(&run) {
                    if loaded.started_at > 0 {
                        tile.started_at = loaded.started_at;
                    }
                    for event in loaded.events {
                        tile.conversation.push(event);
                    }
                }
            }
            Message::Loaded(_, Err(_)) => {}
            Message::Tick(now) => self.now = now,
        }
    }

    pub fn view<'a>(&'a self, snapshot: &Snapshot, chats: &[Chat]) -> Element<'a, AppMessage> {
        let work = working(snapshot, chats);
        let header = page::header(
            "",
            Page::Fleet.title(),
            None,
            page::context(context(work.len())),
            text("").into(),
        );
        let body: Element<'a, AppMessage> = if work.is_empty() {
            container(page::body("Nothing is working."))
                .padding(theme::BOX_PADDING)
                .width(Length::Fill)
                .style(|_| page::box_style())
                .into()
        } else {
            responsive(move |size| {
                let columns = (((size.width + theme::PAGE_GAP)
                    / (theme::TILE_WIDTH + theme::PAGE_GAP))
                    as usize)
                    .clamp(1, MAX_COLUMNS);
                let mut grid = column![].spacing(theme::PAGE_GAP).width(Length::Fill);
                for line in work.chunks(columns) {
                    let mut cells = row![].spacing(theme::PAGE_GAP).width(Length::Fill);
                    for work in line {
                        cells = cells.push(self.tile(work));
                    }
                    for _ in line.len()..columns {
                        cells = cells.push(space::horizontal().width(Length::FillPortion(1)));
                    }
                    grid = grid.push(cells);
                }
                scrollable(container(grid).padding(Padding::default().right(theme::SPACE_12)))
                    .height(Length::Fill)
                    .into()
            })
            .into()
        };
        column![header, body]
            .spacing(theme::PAGE_GAP)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// The tile of a run: who and what for, which card or chat in which repository, and its last steps.
    fn tile<'a>(&'a self, work: &Work) -> Element<'a, AppMessage> {
        let tile = self.tiles.get(&work.run);
        let started_at = tile.map_or(work.started_at, |tile| tile.started_at);
        let mut title = row![
            text(work.agent.clone())
                .size(theme::CONTEXT)
                .font(theme::manrope(Weight::Bold))
                .color(theme::TEXT),
            tag(work.kind.clone(), Tone::Plain),
        ]
        .spacing(theme::SPACE_8)
        .align_y(Alignment::Center);
        if work.status == "waiting" {
            title = title.push(tag("waiting".to_string(), Tone::Amber));
        }
        let mut head = row![title, space::horizontal()].align_y(Alignment::Center);
        if started_at > 0 {
            head = head.push(
                text(steps::elapsed(self.now - started_at))
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TEXT_SECONDARY),
            );
        }
        let mut about = vec![];
        if !work.repository.is_empty() {
            about.push(work.repository.clone());
        }
        if let Some(tokens) = work.tokens {
            about.push(live::tokens(tokens));
        }
        let mut content = column![
            head,
            text(work.title.clone())
                .size(theme::CARD_TITLE)
                .font(theme::manrope(Weight::Semibold))
                .color(theme::TEXT),
        ]
        .spacing(theme::SPACE_6);
        if !about.is_empty() {
            content = content.push(
                text(about.join(" · "))
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .wrapping(Wrapping::None)
                    .color(theme::TEXT_MUTED),
            );
        }
        content = content.push(self.steps(work, tile));
        let target = match work.open {
            OpenTarget::Card(id) => AppMessage::OpenCard(id),
            OpenTarget::Chat(id) => AppMessage::OpenChat(id),
        };
        button(content)
            .padding(theme::BOX_PADDING)
            .width(Length::FillPortion(1))
            .on_press(target)
            .style(|_, status| button::Style {
                background: Some(theme::SURFACE.into()),
                text_color: theme::TEXT,
                border: Border {
                    color: if matches!(status, button::Status::Hovered) {
                        theme::TEXT_MUTED
                    } else {
                        theme::LINE
                    },
                    width: 1.0,
                    radius: theme::RADIUS_BOX.into(),
                },
                ..button::Style::default()
            })
            .into()
    }

    /// The last steps of the run, or a line saying there are none yet.
    fn steps<'a>(&'a self, work: &Work, tile: Option<&'a Tile>) -> Element<'a, AppMessage> {
        let Some((conversation, (first, group))) = tile.and_then(|tile| {
            latest_steps(tile.conversation.entries()).map(|steps| (&tile.conversation, steps))
        }) else {
            return text("No steps yet.")
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED)
                .into();
        };
        let working = work.status != "waiting";
        let stops = if working {
            self.now
        } else {
            conversation.last_at()
        };
        steps::steps_box(
            group,
            first,
            working,
            stops - conversation.started(first),
            &HashSet::new(),
            // A compact box has no clicks; the tile around it takes them.
            |_| AppMessage::Select(Page::Fleet),
            true,
        )
    }
}

/// The last run of steps in the entries: where it starts and the steps.
fn latest_steps(entries: &[Entry]) -> Option<(usize, &[Entry])> {
    let is_step = |entry: &Entry| matches!(entry, Entry::Thinking(_) | Entry::Tool(_));
    let end = entries.iter().rposition(is_step)? + 1;
    let first = entries[..end]
        .iter()
        .rposition(|entry| !is_step(entry))
        .map_or(0, |at| at + 1);
    Some((first, &entries[first..end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_context_counts_the_agents() {
        assert_eq!(context(0), "Nothing is working.");
        assert_eq!(context(1), "1 agent working");
        assert_eq!(context(3), "3 agents working");
    }

    #[test]
    fn the_latest_steps_are_the_last_run_of_steps() {
        let text = || Entry::Text("a".into());
        let thought = || Entry::Thinking("t".into());
        assert!(latest_steps(&[text()]).is_none());
        let entries = [thought(), text(), thought(), thought(), text()];
        let (first, group) = latest_steps(&entries).unwrap();
        assert_eq!((first, group.len()), (2, 2));
    }
}
