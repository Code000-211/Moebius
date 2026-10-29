//! The inbox (`docs/design/inbox.png`): everything that needs the person in one column, newest first, filtered by kind
//! and repository, each item with the buttons that act on it. An action is a request to the server; the item leaves
//! when the snapshot is read again, which the server's event (or the answer) causes. The issues a grilling proposes
//! are drawn as a list the person edits (a title field and a body editor for each, and a button to remove it) before
//! confirming it; what the person edited is kept here, by item, and is what Confirm sends.
//!
//! `view` has how the inbox is drawn, and `item` what an item says of itself.

mod item;
mod view;

use std::collections::HashMap;

use iced::Task;
use iced::widget::text_editor;

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, GrillingIssue, InboxItem, Snapshot};
use crate::screens::live::say;
use crate::settings::Settings;

use item::found;

const READY_TO_MERGE: &str = "ready_to_merge";

const APPROVE_ISSUE: &str = "approve_issue";

const STUCK: &str = "stuck";

const CHANGE_CLOSED: &str = "change_closed";

const PROPOSAL: &str = "proposal";

const TAKEOVER: &str = "takeover";

const GRILLING_ISSUES: &str = "grilling_issues";

const GRILLING_REPORT: &str = "grilling_report";

/// The kinds the core adds, in the order the filter lists them; kinds of column scripts follow.
const KINDS: [&str; 8] = [
    READY_TO_MERGE,
    APPROVE_ISSUE,
    STUCK,
    CHANGE_CLOSED,
    PROPOSAL,
    TAKEOVER,
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
    MergeTakeover {
        takeover: i64,
    },
    DismissTakeover {
        takeover: i64,
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
            Action::MergeTakeover { .. } => "Merging…",
            Action::DismissTakeover { .. } => "Dismissing…",
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
                    Action::MergeTakeover { takeover } => client.merge_takeover(takeover).await,
                    Action::DismissTakeover { takeover } => client.dismiss_takeover(takeover).await,
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
