//! The inbox (`docs/design/inbox.png`): everything that needs the person in one column, newest first, filtered by kind
//! and repository, each item with the buttons that act on it. An action is a request to the server; the item leaves
//! when the snapshot is read again, which the server's event (or the answer) causes. The issues a grilling proposes
//! are drawn as a list the person edits (a title field and a body editor for each, and a button to remove it) before
//! confirming it; what the person edited is kept here, by item, and is what Confirm sends.

use std::collections::HashMap;

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, space, text, text_editor, text_input};
use iced::{Alignment, Border, Color, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Board, Card, Change, Client, ClientError, GrillingIssue, InboxItem, Snapshot};
use crate::live::say;
use crate::page::{self, Tone, tag};
use crate::settings::Settings;
use crate::{board, connect, theme};

const READY_TO_MERGE: &str = "ready_to_merge";
const APPROVE_ISSUE: &str = "approve_issue";
const STUCK: &str = "stuck";
const CHANGE_CLOSED: &str = "change_closed";
const PROPOSAL: &str = "proposal";
const GRILLING_ISSUES: &str = "grilling_issues";
const GRILLING_REPORT: &str = "grilling_report";

/// The kinds the core adds, in the order the filter lists them; kinds of column scripts follow.
const KINDS: [&str; 7] = [
    READY_TO_MERGE,
    APPROVE_ISSUE,
    STUCK,
    CHANGE_CLOSED,
    PROPOSAL,
    GRILLING_ISSUES,
    GRILLING_REPORT,
];

/// What a person can do with an item, with the id of what it is done to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Merge {
        change: i64,
    },
    Reject {
        change: i64,
    },
    Approve {
        card: i64,
    },
    Retry {
        card: i64,
    },
    MergeProposal {
        proposal: i64,
    },
    DismissProposal {
        proposal: i64,
    },
    /// Confirm the grilling with the issues as they are edited.
    ConfirmGrilling {
        grilling: i64,
    },
    DismissGrilling {
        grilling: i64,
    },
}

impl Action {
    /// What the item says while the request is on its way.
    fn working(&self) -> &'static str {
        match self {
            Action::Merge { .. } => "Merging…",
            Action::Reject { .. } => "Sending back…",
            Action::Approve { .. } => "Approving…",
            Action::Retry { .. } => "Retrying…",
            Action::MergeProposal { .. } => "Merging…",
            Action::DismissProposal { .. } => "Dismissing…",
            Action::ConfirmGrilling { .. } => "Making the cards…",
            Action::DismissGrilling { .. } => "Dismissing…",
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
    /// The title typed for an issue (by its index) of a grilling's item.
    Title(i64, usize, String),
    /// An edit of the body of an issue of a grilling's item.
    Body(i64, usize, text_editor::Action),
    /// Drop an issue of a grilling's item from the list.
    Remove(i64, usize),
    /// The clock, for how long ago an item was added.
    Tick(i64),
}

/// One issue of a grilling, as the person is editing it.
struct Draft {
    title: String,
    body: text_editor::Content,
    blocked_by: Vec<usize>,
}

impl Draft {
    fn issue(&self) -> GrillingIssue {
        GrillingIssue {
            title: self.title.trim().to_string(),
            body: self.body.text().trim().to_string(),
            blocked_by: self.blocked_by.clone(),
        }
    }
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
    /// The issues of the items that propose some, as the person has edited them.
    drafts: HashMap<i64, Vec<Draft>>,
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
            drafts: HashMap::new(),
            now,
        }
    }

    /// Follows the snapshot: an item that proposes issues gets its list to edit, and a list whose item is gone is
    /// dropped. A list already there keeps the person's edits.
    pub fn follow(&mut self, snapshot: &Snapshot) {
        let proposing = |item: &&InboxItem| item.kind == GRILLING_ISSUES;
        self.drafts.retain(|id, _| {
            snapshot
                .inbox
                .iter()
                .filter(proposing)
                .any(|item| item.id == *id)
        });
        for item in snapshot.inbox.iter().filter(proposing) {
            self.drafts.entry(item.id).or_insert_with(|| {
                item.issues()
                    .into_iter()
                    .map(|issue| Draft {
                        title: issue.title,
                        body: text_editor::Content::with_text(&issue.body),
                        blocked_by: issue.blocked_by,
                    })
                    .collect()
            });
        }
    }

    /// The issues of an item as they are now, none when the item proposes none.
    fn edited(&self, item: i64) -> Vec<GrillingIssue> {
        self.drafts
            .get(&item)
            .map(|drafts| drafts.iter().map(Draft::issue).collect())
            .unwrap_or_default()
    }

    /// Whether the list of the item can be confirmed: it has an issue, and each has a title.
    fn confirmable(&self, item: i64) -> bool {
        let issues = self.edited(item);
        !issues.is_empty() && issues.iter().all(|issue| !issue.title.is_empty())
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
            Message::Title(item, index, title) => {
                if let Some(draft) = self.draft(item, index) {
                    draft.title = title;
                }
            }
            Message::Body(item, index, action) => {
                if let Some(draft) = self.draft(item, index) {
                    draft.body.perform(action);
                }
            }
            Message::Remove(item, index) => {
                if let Some(drafts) = self.drafts.get_mut(&item)
                    && index < drafts.len()
                {
                    drafts.remove(index);
                    // The issues after it move up, and nothing waits for it any more.
                    for draft in drafts {
                        draft.blocked_by.retain(|blocker| *blocker != index);
                        for blocker in &mut draft.blocked_by {
                            if *blocker > index {
                                *blocker -= 1;
                            }
                        }
                    }
                }
            }
            Message::Tick(now) => self.now = now,
        }
        Task::none()
    }

    fn draft(&mut self, item: i64, index: usize) -> Option<&mut Draft> {
        self.drafts.get_mut(&item)?.get_mut(index)
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
        let issues = self.edited(item);
        if self.working.contains_key(&item)
            || (matches!(action, Action::Reject { .. })
                && reason.as_deref().is_none_or(str::is_empty))
            || (matches!(action, Action::ConfirmGrilling { .. }) && !self.confirmable(item))
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
                    Action::MergeProposal { proposal } => client.merge_proposal(proposal).await,
                    Action::DismissProposal { proposal } => client.dismiss_proposal(proposal).await,
                    Action::ConfirmGrilling { grilling } => {
                        client.confirm_grilling(grilling, &issues).await
                    }
                    Action::DismissGrilling { grilling } => client.dismiss_grilling(grilling).await,
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
        } else if item.kind == PROPOSAL {
            meta.push("Retro".to_string());
        } else if item.grilling().is_some() {
            meta.push("Grilling".to_string());
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
        if item.kind == PROPOSAL {
            content = content.push(trials(item));
        }
        if item.kind == GRILLING_ISSUES {
            content = content.push(self.issues_view(item.id));
        }
        if item.kind == GRILLING_REPORT {
            content = content.push(report(item.report()));
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
            (PROPOSAL, _, _) => {
                if let Some(proposal) = item.proposal() {
                    buttons.push(page::primary_button(
                        "Merge",
                        act(Action::MergeProposal { proposal }),
                    ));
                    buttons.push(page::white_button(
                        "Dismiss",
                        act(Action::DismissProposal { proposal }),
                    ));
                }
            }
            (GRILLING_ISSUES, _, _) => {
                if let Some(grilling) = item.grilling() {
                    buttons.push(page::primary_button(
                        "Confirm",
                        self.confirmable(id)
                            .then(|| act(Action::ConfirmGrilling { grilling }))
                            .flatten(),
                    ));
                    buttons.push(page::white_button(
                        "Dismiss",
                        act(Action::DismissGrilling { grilling }),
                    ));
                }
            }
            (GRILLING_REPORT, _, _) => {
                if let Some(grilling) = item.grilling() {
                    buttons.push(page::primary_button(
                        "Dismiss",
                        act(Action::DismissGrilling { grilling }),
                    ));
                }
            }
            _ => {}
        }
        row(buttons).spacing(8).wrap().vertical_spacing(8).into()
    }

    /// The issues a grilling proposes as a list to edit: a title field and a body editor for each, with what blocks
    /// it, and a button to remove it.
    fn issues_view(&self, item: i64) -> Element<'_, AppMessage> {
        let Some(drafts) = self.drafts.get(&item).filter(|drafts| !drafts.is_empty()) else {
            return page::body("No issue is left. Dismiss the item, or ask the agent again.");
        };
        column(drafts.iter().enumerate().map(|(index, draft)| {
            let mut head = row![tag(format!("Issue {}", index + 1), Tone::Plain)]
                .spacing(8)
                .align_y(Alignment::Center);
            if !draft.blocked_by.is_empty() {
                let blockers: Vec<String> = draft
                    .blocked_by
                    .iter()
                    .map(|blocker| (blocker + 1).to_string())
                    .collect();
                head = head.push(
                    text(format!("Blocked by issue {}", blockers.join(", ")))
                        .size(theme::LABEL)
                        .color(theme::TEXT_MUTED),
                );
            }
            head = head.push(space::horizontal()).push(page::white_button(
                "Remove",
                Some(AppMessage::Inbox(Message::Remove(item, index))),
            ));
            let title = text_input("Title", &draft.title)
                .on_input(move |typed| AppMessage::Inbox(Message::Title(item, index, typed)))
                .size(theme::BODY)
                .padding([9, 12])
                .style(connect::input_style);
            let body = text_editor(&draft.body)
                .placeholder("What the issue asks for.")
                .on_action(move |action| AppMessage::Inbox(Message::Body(item, index, action)))
                .size(theme::BODY)
                .padding(12)
                .height(96)
                .style(|_, status| text_editor::Style {
                    background: theme::LANE.into(),
                    border: Border {
                        color: match status {
                            text_editor::Status::Focused { .. } => theme::TEAL,
                            _ => theme::LINE,
                        },
                        width: 1.0,
                        radius: theme::RADIUS_CONTROL.into(),
                    },
                    placeholder: theme::TEXT_MUTED,
                    value: theme::TEXT,
                    selection: theme::TEAL_FILL,
                });
            container(column![head, title, body].spacing(8))
                .padding(12)
                .width(Length::Fill)
                .style(|_| page::box_style())
                .into()
        }))
        .spacing(10)
        .into()
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
            || (item.grilling().is_some() && item.repository() == Some(board.repository.id))
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
        PROPOSAL => "Proposals".to_string(),
        GRILLING_ISSUES => "Grilled issues".to_string(),
        GRILLING_REPORT => "Reports".to_string(),
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
        PROPOSAL => ("Retro proposal".into(), Tone::Amber),
        GRILLING_ISSUES => ("Issues from a grilling".into(), Tone::Amber),
        GRILLING_REPORT => ("Report from a grilling".into(), Tone::Amber),
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
        PROPOSAL => Some("The Retro proposes this change to the harness configuration.".into()),
        GRILLING_ISSUES => Some(
            "The Grilling partner ended in these issues. Edit them, remove what you do not want, and confirm to make a card of each."
                .into(),
        ),
        GRILLING_REPORT => {
            Some("The Grilling partner ended in a research report. Read it, then dismiss it.".into())
        }
        _ => reason.map(str::to_string),
    }
}

/// The report of a grilling, as text on a field of its own.
fn report(report: &str) -> Element<'_, AppMessage> {
    container(
        text(report)
            .size(theme::BODY)
            .color(theme::TEXT)
            .width(Length::Fill)
            .wrapping(Wrapping::WordOrGlyph),
    )
    .padding(14)
    .width(Length::Fill)
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

/// What the trials of a proposal found, one line for each: the card, the outcome and why.
fn trials(item: &InboxItem) -> Element<'_, AppMessage> {
    let lines = item.data["trials"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|trial| {
            let outcome = trial["outcome"].as_str().unwrap_or_default();
            let card = match trial["card"].as_i64() {
                Some(card) => format!("Card {card}"),
                None => "No card".to_string(),
            };
            row![
                tag(card, Tone::Plain),
                tag(
                    sentence(outcome),
                    if outcome == "positive" {
                        Tone::Teal
                    } else {
                        Tone::Plain
                    }
                ),
                text(trial["reason"].as_str().unwrap_or_default())
                    .size(theme::BODY)
                    .color(theme::TEXT_SECONDARY)
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        });
    column(lines).spacing(6).into()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn grilling_snapshot() -> Snapshot {
        serde_json::from_value(serde_json::json!({"inbox": [{
            "id": 3, "kind": "grilling_issues", "title": "3 issues from a grilling", "at": 1,
            "data": {"grilling": 7, "repository": 1, "issues": [
                {"title": "A", "body": "First.", "blocked_by": []},
                {"title": "B", "body": "Second.", "blocked_by": [0]},
                {"title": "C", "body": "Third.", "blocked_by": [0, 1]}]}
        }]}))
        .unwrap()
    }

    fn titles_and_blockers(inbox: &Inbox) -> Vec<(String, Vec<usize>)> {
        inbox
            .edited(3)
            .into_iter()
            .map(|issue| (issue.title, issue.blocked_by))
            .collect()
    }

    #[test]
    fn the_issues_of_a_grilling_are_edited_and_removed_with_their_blockers_following() {
        let mut inbox = Inbox::new(0);
        inbox.follow(&grilling_snapshot());
        assert_eq!(inbox.edited(3).len(), 3);
        assert_eq!(inbox.edited(3)[1].body, "Second.");

        let _ = inbox.update(Message::Title(3, 2, "C, renamed".into()), None);
        // Removing the first issue drops it from the blockers, and the indexes after it move up.
        let _ = inbox.update(Message::Remove(3, 0), None);
        assert_eq!(
            titles_and_blockers(&inbox),
            [("B".into(), vec![]), ("C, renamed".into(), vec![0])]
        );

        // The edits stay when the snapshot is read again, and go with the item.
        inbox.follow(&grilling_snapshot());
        assert_eq!(inbox.edited(3).len(), 2);
        inbox.follow(&Snapshot::default());
        assert!(inbox.edited(3).is_empty());
    }

    #[test]
    fn a_list_without_an_issue_or_with_a_blank_title_cannot_be_confirmed() {
        let mut inbox = Inbox::new(0);
        inbox.follow(&grilling_snapshot());
        assert!(inbox.confirmable(3));
        let _ = inbox.update(Message::Title(3, 1, "  ".into()), None);
        assert!(!inbox.confirmable(3));
        for _ in 0..3 {
            let _ = inbox.update(Message::Remove(3, 0), None);
        }
        assert!(!inbox.confirmable(3));
    }
}
