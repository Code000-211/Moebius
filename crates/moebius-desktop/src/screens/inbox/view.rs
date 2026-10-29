//! How the inbox is drawn: the filters by kind and repository, each item with its facts, and the footer with the
//! buttons that act on it, or the list of a grilling's issues to edit before it is confirmed.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, space, text, text_editor, text_input};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::client::{InboxItem, Snapshot};
use crate::widgets::page::{self, Tone, tag};
use crate::{screens::connect, theme};

use super::item::{chip, chip_name, facts, found, kind_tag, own_change, report, trials, why};
use super::{
    APPROVE_ISSUE, Action, CHANGE_CLOSED, GRILLING_ISSUES, GRILLING_REPORT, Inbox, KINDS, Message,
    PROPOSAL, READY_TO_MERGE, STUCK, TAKEOVER,
};

impl Inbox {
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
        if found.change.is_some_and(|c| c.harness_repository) {
            meta.insert(0, "Harness configuration".to_string());
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
        if item.kind == PROPOSAL && item.data["diff"].is_string() {
            content = content.push(own_change(item));
        } else if item.kind == PROPOSAL {
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
    pub(super) fn footer<'a>(&'a self, item: &InboxItem) -> Element<'a, AppMessage> {
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
            (TAKEOVER, _, _) => {
                if let Some(takeover) = item.takeover() {
                    buttons.push(page::primary_button(
                        "Merge",
                        act(Action::MergeTakeover { takeover }),
                    ));
                    buttons.push(page::white_button(
                        "Dismiss",
                        act(Action::DismissTakeover { takeover }),
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
