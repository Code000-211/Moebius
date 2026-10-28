//! The inbox (`docs/design/inbox.png`): everything that needs the person in one column, newest first, filtered by kind
//! and repository, each item with the buttons that act on it. An action is a request to the server; the item leaves
//! when the snapshot is read again, which the server's event (or the answer) causes.

use std::collections::HashMap;

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Border, Color, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Board, Card, Change, Client, ClientError, InboxItem, Snapshot};
use crate::live::say;
use crate::page::{self, Tone, tag};
use crate::settings::Settings;
use crate::{board, connect, theme};

const READY_TO_MERGE: &str = "ready_to_merge";
const APPROVE_ISSUE: &str = "approve_issue";
const STUCK: &str = "stuck";
const CHANGE_CLOSED: &str = "change_closed";

/// The kinds the core adds, in the order the filter lists them; kinds of column scripts follow.
const KINDS: [&str; 4] = [READY_TO_MERGE, APPROVE_ISSUE, STUCK, CHANGE_CLOSED];

/// What a person can do with an item, with the id of what it is done to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Merge { change: i64 },
    Reject { change: i64 },
    Approve { card: i64 },
    Retry { card: i64 },
}

impl Action {
    /// What the item says while the request is on its way.
    fn working(&self) -> &'static str {
        match self {
            Action::Merge { .. } => "Merging…",
            Action::Reject { .. } => "Sending back…",
            Action::Approve { .. } => "Approving…",
            Action::Retry { .. } => "Retrying…",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Show one kind of item, or all.
    Kind(Option<String>),
    /// Show the items of one repository, or all.
    Repository(Option<i64>),
    /// The first step of rejecting an item's change: ask for the reason.
    AskReject(i64),
    CancelReject(i64),
    /// The reason typed for rejecting the item's change.
    Reason(i64, String),
    /// Do something with the item.
    Act(i64, Action),
    /// The server answered the request made for the item.
    Done(i64, Result<(), ClientError>),
    /// The clock, for how long ago an item was added.
    Tick(i64),
}

/// The inbox page's state: the filters, and what the items are going through.
pub struct Inbox {
    kind: Option<String>,
    repository: Option<i64>,
    /// The requests on their way, by item.
    working: HashMap<i64, Action>,
    /// What went wrong with the last request, by item, as one short line.
    notices: HashMap<i64, String>,
    /// The items whose change is being rejected, with the reason typed so far.
    rejecting: HashMap<i64, String>,
    now: i64,
}

impl Inbox {
    pub fn new(now: i64) -> Self {
        Inbox {
            kind: None,
            repository: None,
            working: HashMap::new(),
            notices: HashMap::new(),
            rejecting: HashMap::new(),
            now,
        }
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Kind(kind) => self.kind = kind,
            Message::Repository(repository) => self.repository = repository,
            Message::AskReject(item) => {
                self.rejecting.entry(item).or_default();
            }
            Message::CancelReject(item) => {
                self.rejecting.remove(&item);
            }
            Message::Reason(item, reason) => {
                self.rejecting.insert(item, reason);
            }
            Message::Act(item, action) => return self.act(item, action, settings),
            Message::Done(item, result) => {
                self.working.remove(&item);
                match result {
                    Ok(()) => {
                        self.rejecting.remove(&item);
                    }
                    Err(e) => {
                        self.notices.insert(item, say(&e));
                    }
                }
            }
            Message::Tick(now) => self.now = now,
        }
        Task::none()
    }

    /// What the request made for the item says: while it is on its way, or why it failed.
    pub fn status(&self, item: i64) -> Option<&str> {
        self.working
            .get(&item)
            .map(Action::working)
            .or_else(|| self.notices.get(&item).map(String::as_str))
    }

    fn act(&mut self, item: i64, action: Action, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        let reason = self.rejecting.get(&item).map(|r| r.trim().to_string());
        if self.working.contains_key(&item)
            || (matches!(action, Action::Reject { .. })
                && reason.as_deref().is_none_or(str::is_empty))
        {
            return Task::none();
        }
        self.notices.remove(&item);
        self.working.insert(item, action.clone());
        let client = Client::new(settings.clone());
        Task::perform(
            async move {
                match action {
                    Action::Merge { change } => client.merge(change).await,
                    Action::Reject { change } => {
                        client.reject(change, &reason.unwrap_or_default()).await
                    }
                    Action::Approve { card } => client.approve(card).await,
                    Action::Retry { card } => client.retry(card).await,
                }
            },
            move |result| AppMessage::Inbox(Message::Done(item, result)),
        )
    }

    /// The items the filters let through, newest first.
    fn shown<'a>(&self, snapshot: &'a Snapshot) -> Vec<&'a InboxItem> {
        let mut items: Vec<_> = snapshot
            .inbox
            .iter()
            .filter(|item| self.in_kind(item) && self.in_repository(snapshot, item))
            .collect();
        items.sort_by_key(|item| std::cmp::Reverse((item.at, item.id)));
        items
    }

    fn in_kind(&self, item: &InboxItem) -> bool {
        self.kind.as_ref().is_none_or(|kind| *kind == item.kind)
    }

    fn in_repository(&self, snapshot: &Snapshot, item: &InboxItem) -> bool {
        self.repository.is_none_or(|id| {
            found(snapshot, item)
                .board
                .is_some_and(|b| b.repository.id == id)
        })
    }

    pub fn view<'a>(&'a self, snapshot: &'a Snapshot) -> Element<'a, AppMessage> {
        let shown = self.shown(snapshot);
        let total = snapshot.inbox.len();
        let line = match (total, shown.len()) {
            (0, _) => "Nothing needs you.".to_string(),
            (1, _) => "1 thing needs you.".to_string(),
            (total, shown) if shown == total => format!("{total} things need you."),
            (total, shown) => format!("{total} things need you; {shown} shown."),
        };
        let list: Element<_> = if shown.is_empty() {
            let empty = if total == 0 {
                "Everything runs on its own."
            } else {
                "No item matches the filter."
            };
            page::boxed("Inbox", page::body(empty))
        } else {
            column(shown.into_iter().map(|item| self.item_view(snapshot, item)))
                .spacing(12)
                .width(Length::Fill)
                .into()
        };
        column![
            page::header("", "Inbox", None, page::context(line), space().into()),
            self.filters(snapshot),
            scrollable(list).height(Length::Fill),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }

    /// The chips: one for all and one for each kind, then one for each repository when there are several. A kind's
    /// count is of the repository chosen, a repository's of the kind chosen.
    fn filters<'a>(&'a self, snapshot: &'a Snapshot) -> Element<'a, AppMessage> {
        let count = |kind: Option<&str>, repository: Option<i64>| {
            snapshot
                .inbox
                .iter()
                .filter(|item| kind.is_none_or(|kind| item.kind == kind))
                .filter(|item| {
                    repository.is_none_or(|id| {
                        found(snapshot, item)
                            .board
                            .is_some_and(|b| b.repository.id == id)
                    })
                })
                .count()
        };
        let mut kinds: Vec<&str> = KINDS
            .into_iter()
            .filter(|kind| snapshot.inbox.iter().any(|item| item.kind == *kind))
            .collect();
        let mut others: Vec<&str> = snapshot
            .inbox
            .iter()
            .map(|item| item.kind.as_str())
            .filter(|kind| !KINDS.contains(kind))
            .collect();
        others.sort();
        others.dedup();
        kinds.extend(others);

        let mut by_kind = row![chip(
            format!("All {}", count(None, self.repository)),
            self.kind.is_none(),
            false,
            Message::Kind(None),
        )]
        .spacing(8);
        for kind in kinds {
            let label = format!("{} {}", chip_name(kind), count(Some(kind), self.repository));
            let selected = self.kind.as_deref() == Some(kind);
            let red = kind == STUCK;
            by_kind = by_kind.push(chip(
                label,
                selected,
                red,
                Message::Kind(Some(kind.to_string())),
            ));
        }
        let mut filters = column![by_kind.wrap().vertical_spacing(8)].spacing(8);
        if snapshot.boards.len() > 1 {
            let mut by_repository = row![chip(
                "Every repository".to_string(),
                self.repository.is_none(),
                false,
                Message::Repository(None),
            )]
            .spacing(8);
            for board in &snapshot.boards {
                let id = board.repository.id;
                let label = format!(
                    "{} {}",
                    board.repository.name,
                    count(self.kind.as_deref(), Some(id))
                );
                by_repository = by_repository.push(chip(
                    label,
                    self.repository == Some(id),
                    false,
                    Message::Repository(Some(id)),
                ));
            }
            filters = filters.push(by_repository.wrap().vertical_spacing(8));
        }
        filters.into()
    }

    fn item_view<'a>(
        &'a self,
        snapshot: &'a Snapshot,
        item: &'a InboxItem,
    ) -> Element<'a, AppMessage> {
        let found = found(snapshot, item);
        let (label, tone) = kind_tag(item);
        let mut meta = Vec::new();
        if let Some(board) = found.board {
            meta.push(board.repository.name.clone());
        }
        if let Some(card) = found.card {
            meta.push(format!("#{}", card.number));
            if item.kind == APPROVE_ISSUE && !card.author.is_empty() {
                meta.push(format!("by {}", card.author));
            }
        } else if item.card_id.is_none() {
            meta.push("chat".to_string());
        }
        let meta_color = if item.kind == STUCK {
            theme::RED_TEXT
        } else {
            theme::TEXT_MUTED
        };
        let mut content = column![
            row![
                tag(label, tone),
                text(meta.join(" · ")).size(theme::LABEL).color(meta_color),
                space::horizontal(),
                text(page::ago(self.now, item.at))
                    .size(theme::LABEL)
                    .color(theme::TEXT_MUTED),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            text(item.title.as_str())
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold))
                .wrapping(Wrapping::WordOrGlyph),
        ]
        .spacing(10);
        if let Some(why) = why(item, &found) {
            content = content.push(page::body(why));
        }
        if item.kind == READY_TO_MERGE {
            content = content.push(facts(item, &found));
        }
        content = content.push(self.footer(item));
        if let Some(notice) = self.notices.get(&item.id) {
            content = content.push(
                text(notice.as_str())
                    .size(theme::LABEL)
                    .font(theme::manrope(Weight::Semibold))
                    .color(theme::AMBER_TEXT),
            );
        }
        let (fill, border, width) = match tone {
            Tone::Red => (theme::RED_FILL, theme::RED_BORDER, 1.5),
            Tone::Amber => (theme::SURFACE, theme::AMBER_BORDER, 1.0),
            _ => (theme::SURFACE, theme::LINE, 1.0),
        };
        container(content)
            .padding(18)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(fill.into()),
                border: Border {
                    color: border,
                    width,
                    radius: theme::RADIUS_BOX.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    /// The buttons of an item: the primary one first. While a request is on its way they give way to what it does;
    /// while a change is being rejected, to the reason's field.
    fn footer<'a>(&'a self, item: &InboxItem) -> Element<'a, AppMessage> {
        let id = item.id;
        let act = |action| Some(AppMessage::Inbox(Message::Act(id, action)));
        if let Some(action) = self.working.get(&id) {
            return text(action.working())
                .size(theme::CONTEXT)
                .color(theme::TEXT_SECONDARY)
                .into();
        }
        if let (Some(reason), Some(change)) = (self.rejecting.get(&id), item.change()) {
            let ready = !reason.trim().is_empty();
            let send = Action::Reject { change };
            let field = text_input("Why? The next round is told.", reason)
                .on_input(move |typed| AppMessage::Inbox(Message::Reason(id, typed)))
                .on_submit_maybe(ready.then(|| AppMessage::Inbox(Message::Act(id, send.clone()))))
                .size(theme::BODY)
                .padding([9, 12])
                .style(connect::input_style);
            return row![
                field,
                page::primary_button(
                    "Send back",
                    ready.then(|| AppMessage::Inbox(Message::Act(id, Action::Reject { change })))
                ),
                page::white_button("Cancel", Some(AppMessage::Inbox(Message::CancelReject(id)))),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into();
        }
        let mut buttons: Vec<Element<AppMessage>> = Vec::new();
        match (item.kind.as_str(), item.change(), item.card_id) {
            (READY_TO_MERGE, Some(change), card) => {
                buttons.push(page::primary_button("Merge", act(Action::Merge { change })));
                match card {
                    Some(card) => {
                        buttons.push(page::white_button(
                            "Open live view",
                            Some(AppMessage::OpenCard(card)),
                        ));
                        buttons.push(page::white_button(
                            "Reject with a reason",
                            Some(AppMessage::Inbox(Message::AskReject(id))),
                        ));
                    }
                    None => buttons.push(page::white_button(
                        "Open chat",
                        item.chat().map(AppMessage::OpenChat),
                    )),
                }
            }
            (APPROVE_ISSUE, _, Some(card)) => {
                buttons.push(page::primary_button(
                    "Approve",
                    act(Action::Approve { card }),
                ));
            }
            (STUCK | CHANGE_CLOSED, _, Some(card)) => {
                buttons.push(page::primary_button("Retry", act(Action::Retry { card })));
            }
            _ => {}
        }
        row(buttons).spacing(8).wrap().vertical_spacing(8).into()
    }
}

/// What an item is about, looked up in the snapshot: the board of its repository, its card and its change.
struct Found<'a> {
    board: Option<&'a Board>,
    card: Option<&'a Card>,
    change: Option<&'a Change>,
}

fn found<'a>(snapshot: &'a Snapshot, item: &InboxItem) -> Found<'a> {
    let change = item.change();
    let board = snapshot.boards.iter().find(|board| {
        board.cards.iter().any(|card| Some(card.id) == item.card_id)
            || board.changes.iter().any(|c| Some(c.id) == change)
    });
    Found {
        board,
        card: board.and_then(|b| b.cards.iter().find(|c| Some(c.id) == item.card_id)),
        change: board.and_then(|b| b.changes.iter().find(|c| Some(c.id) == change)),
    }
}

/// The name of a kind on its chip.
fn chip_name(kind: &str) -> String {
    match kind {
        READY_TO_MERGE => "To merge".to_string(),
        APPROVE_ISSUE => "Approvals".to_string(),
        STUCK => "Stuck".to_string(),
        CHANGE_CLOSED => "Closed".to_string(),
        other => sentence(other),
    }
}

/// A kind of a column script, such as `needs_design`, as a label: `Needs design`.
fn sentence(kind: &str) -> String {
    let mut words = kind.replace('_', " ");
    if let Some(first) = words.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    words
}

/// What the item is, and how loud: red for what is blocked, amber for what waits for the person's move.
fn kind_tag(item: &InboxItem) -> (String, Tone) {
    match item.kind.as_str() {
        STUCK => ("Stuck".into(), Tone::Red),
        READY_TO_MERGE if item.card_id.is_none() => ("Change from a chat".into(), Tone::Amber),
        READY_TO_MERGE => ("Ready to merge".into(), Tone::Amber),
        APPROVE_ISSUE => ("Outside issue".into(), Tone::Amber),
        CHANGE_CLOSED => ("Closed without a merge".into(), Tone::Amber),
        other => (sentence(other), Tone::Plain),
    }
}

/// The sentence under the title: why the item is here.
fn why(item: &InboxItem, found: &Found) -> Option<String> {
    let reason = item.data["reason"].as_str().filter(|r| !r.is_empty());
    match item.kind.as_str() {
        STUCK => Some(reason.map_or("The card is stuck.".into(), |r| format!("Stopped: {r}."))),
        APPROVE_ISSUE => Some(match reason {
            Some(reason) => format!("A person has to approve this issue: {reason}."),
            None => "A person has to approve this issue.".into(),
        }),
        CHANGE_CLOSED => {
            let number = item.data["pull_request"]
                .as_i64()
                .or(found.change.and_then(|c| c.pull_request));
            Some(match number {
                Some(number) => format!("Pull request #{number} was closed without a merge."),
                None => "The change's pull request was closed without a merge.".into(),
            })
        }
        READY_TO_MERGE => None,
        _ => reason.map(str::to_string),
    }
}

/// The facts of a change that waits to be merged: what it took, how its checks stand and where it is.
fn facts<'a>(item: &'a InboxItem, found: &Found<'a>) -> Element<'a, AppMessage> {
    let mut facts = Vec::new();
    if let Some(card) = found.card {
        facts.push(fact("Rounds", card.rounds.to_string(), theme::TEXT, None));
        facts.push(fact(
            "Tokens",
            board::tokens(card.tokens),
            theme::TEXT,
            None,
        ));
    }
    let checks = match found.change.and_then(|c| c.checks.as_deref()) {
        Some("success") => ("CI green", theme::TEAL),
        Some("pending") => ("CI running", theme::TEXT),
        Some("failure") => ("CI failed", theme::TEXT),
        _ => ("No checks", theme::TEXT),
    };
    let mut verdict = checks.0.to_string();
    if let Some(review) = found.change.and_then(|c| c.review.as_deref()) {
        verdict = format!("{verdict} · review {review}");
    }
    facts.push(fact("Checks", verdict, checks.1, None));
    let branch = item.data["branch"]
        .as_str()
        .or(found.change.map(|c| c.branch.as_str()));
    if let Some(branch) = branch {
        facts.push(fact(
            "Branch",
            branch.to_string(),
            theme::TEXT,
            Some(theme::MONO),
        ));
    }
    row(facts).spacing(8).width(Length::Fill).into()
}

/// One box of facts: a small label over a value.
fn fact<'a>(
    label: &'static str,
    value: String,
    color: Color,
    font: Option<iced::Font>,
) -> Element<'a, AppMessage> {
    let mut value = text(value).size(theme::BODY).color(color);
    if let Some(font) = font {
        value = value.font(font).size(theme::CODE);
    }
    container(
        column![
            text(label).size(theme::LABEL).color(theme::TEXT_MUTED),
            value
        ]
        .spacing(2),
    )
    .padding([8, 11])
    .width(Length::FillPortion(1))
    .style(|_| container::Style {
        background: Some(theme::LANE.into()),
        border: Border {
            color: theme::TRACK,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// A filter chip of the inbox.
fn chip<'a>(label: String, selected: bool, red: bool, message: Message) -> Element<'a, AppMessage> {
    page::chip(label, selected, red, AppMessage::Inbox(message))
}
