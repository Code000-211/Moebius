//! The boards and what is on them, as the desktop API has them: repositories, cards, changes, the inbox and the
//! snapshot that holds them all, a card's history, and a repository's files and column scripts.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::runs::{CHAT_BRANCH, Run};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Repository {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub path: String,
    /// `none`, `github` or `gitlab`.
    #[serde(default)]
    pub forge: String,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub github: Option<String>,
    #[serde(default)]
    pub trusted: Vec<String>,
    /// The project's path on GitLab.
    #[serde(default)]
    pub project: Option<String>,
}

impl Repository {
    /// The forge and the repository's name there, as a page's header shows it: `GitHub · acme/radiance`.
    pub fn forge_tag(&self) -> String {
        match (self.forge.as_str(), &self.github, &self.project) {
            ("gitlab", _, Some(project)) => format!("GitLab · {project}"),
            (_, Some(github), _) => format!("GitHub · {github}"),
            _ => "No forge".to_string(),
        }
    }

    /// Whether the repository is on a forge, whose logins it may trust.
    pub fn on_forge(&self) -> bool {
        self.github.is_some() || self.project.is_some()
    }
}

/// What `POST /v1/repositories` takes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NewRepository {
    pub name: String,
    pub url: String,
    /// `none`, `github` or `gitlab`.
    pub forge: &'static str,
    /// `owner/name`, with the forge `github`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
    /// `group/project`, with the forge `gitlab`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trusted: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Card {
    pub id: i64,
    pub repository_id: i64,
    pub number: i64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub author: String,
    pub column: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub retried_after: Option<i64>,
    /// The `code` rounds and the tokens since the card was last retried.
    #[serde(default)]
    pub rounds: i64,
    #[serde(default)]
    pub tokens: i64,
    /// The card's latest run.
    #[serde(default)]
    pub run: Option<Run>,
    /// The numbers of the issues that block the card.
    #[serde(default)]
    pub blocked_by: Vec<i64>,
    /// How many tool calls of the latest run were refused.
    #[serde(default)]
    pub refusals: i64,
    /// The card's open change.
    #[serde(default)]
    pub change: Option<CardChange>,
}

impl Card {
    /// Whether the card's latest run is working.
    pub fn is_live(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|run| matches!(run.status.as_str(), "starting" | "running"))
    }
}

/// The open change of a card, as far as the board shows it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CardChange {
    pub id: i64,
    /// `pending`, `success`, `failure` or `none`.
    pub checks: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Change {
    pub id: i64,
    /// None for a change of the harness configuration repository.
    #[serde(default)]
    pub repository_id: Option<i64>,
    /// Whether the change belongs to the harness configuration repository.
    #[serde(default)]
    pub harness_repository: bool,
    #[serde(default)]
    pub card_id: Option<i64>,
    pub branch: String,
    /// `open`, `merged` or `closed`.
    pub state: String,
    #[serde(default)]
    pub pull_request: Option<i64>,
    #[serde(default)]
    pub checks: Option<String>,
    #[serde(default)]
    pub review: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Board {
    pub repository: Repository,
    #[serde(default)]
    pub cards: Vec<Card>,
    /// The most `code` rounds a card gets; 0 when the server does not say.
    #[serde(default)]
    pub round_limit: i64,
    /// The reason each waiting card waits, by card id.
    #[serde(default)]
    pub waiting: HashMap<i64, String>,
    #[serde(default)]
    pub changes: Vec<Change>,
}

impl Board {
    /// How many runs are working on the board's cards.
    pub fn live_runs(&self) -> usize {
        self.cards.iter().filter(|card| card.is_live()).count()
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InboxItem {
    pub id: i64,
    pub kind: String,
    #[serde(default)]
    pub card_id: Option<i64>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub at: i64,
}

/// One issue a grilling proposes, or the person made of it: `blocked_by` are indexes into the list it is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrillingIssue {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub blocked_by: Vec<usize>,
}

impl InboxItem {
    /// The id of the grilling a `grilling_issues` or `grilling_report` item is about.
    pub fn grilling(&self) -> Option<i64> {
        self.data["grilling"].as_i64()
    }

    /// The id of the repository a grilling's item is about.
    pub fn repository(&self) -> Option<i64> {
        self.data["repository"].as_i64()
    }

    /// The issues a `grilling_issues` item proposes.
    pub fn issues(&self) -> Vec<GrillingIssue> {
        serde_json::from_value(self.data["issues"].clone()).unwrap_or_default()
    }

    /// The report of a `grilling_report` item.
    pub fn report(&self) -> &str {
        self.data["report"].as_str().unwrap_or_default()
    }

    /// The id of the change a `ready_to_merge` or `change_closed` item is about.
    pub fn change(&self) -> Option<i64> {
        self.data["change"].as_i64()
    }

    /// The id of the proposal a `proposal` item is about.
    pub fn proposal(&self) -> Option<i64> {
        self.data["proposal"].as_i64()
    }

    /// The id of the takeover a `takeover` item is about.
    pub fn takeover(&self) -> Option<i64> {
        self.data["takeover"].as_i64()
    }

    /// The id of the chat whose work a `ready_to_merge` item offers: the number in its branch `moebius/chat-<id>`.
    pub fn chat(&self) -> Option<i64> {
        self.data["branch"]
            .as_str()?
            .strip_prefix(CHAT_BRANCH)?
            .parse()
            .ok()
    }
}

/// Everything the app shows, as `GET /v1/snapshot` answers.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub boards: Vec<Board>,
    /// The changes of the harness configuration repository, which belongs to no board.
    #[serde(default)]
    pub harness_changes: Vec<Change>,
    #[serde(default)]
    pub inbox: Vec<InboxItem>,
}

/// One entry of a card's timeline.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HistoryEntry {
    pub at: i64,
    pub kind: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

/// A column's script as `GET /v1/repositories/{id}/scripts/{column}` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScriptDetail {
    pub column: String,
    /// Where the repository keeps its own script for the column.
    pub path: String,
    /// `repository` or `built-in`.
    pub source: String,
    pub text: String,
}

/// A directory of a repository's default branch as `GET /v1/repositories/{id}/tree` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tree {
    /// The directory, `""` for the root.
    pub path: String,
    /// Directories first, each group by name.
    pub entries: Vec<TreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    /// From the repository's root.
    pub path: String,
    /// `dir` or `file`.
    pub kind: String,
    #[serde(default)]
    pub size: u64,
}

/// A file of a repository's default branch as `GET /v1/repositories/{id}/files/{path}` answers it: its text, or for a
/// binary file or one too large to edit only its size.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileDetail {
    pub path: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub binary: bool,
    #[serde(default)]
    pub too_large: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The snapshot of the reference, with a field the app does not know and a card without the optional fields.
    const SNAPSHOT: &str = r#"{
      "boards": [
        {"repository": {"id": 1, "name": "demo", "path": "/srv/.moebius/repos/demo", "forge": "none",
                        "default_branch": "main", "github": null, "trusted": []},
         "cards": [{"id": 1, "repository_id": 1, "number": 1, "title": "Add a feature", "body": "", "author": "person",
                    "column": "Backlog", "created_at": 1790000000, "added_later": true},
                   {"id": 2, "repository_id": 1, "number": 2, "title": "Work", "column": "In progress",
                    "rounds": 2, "tokens": 900, "blocked_by": [1], "refusals": 1,
                    "run": {"id": 4, "agent": "Coder", "purpose": "code", "status": "running"},
                    "change": {"id": 3, "state": "open", "checks": "pending", "review_verdict": null}}],
         "round_limit": 5,
         "waiting": {"1": "needs a person"},
         "changes": []}
      ],
      "inbox": [{"id": 7, "kind": "approve_issue", "card_id": 1, "title": "Add a feature",
                 "data": {"reason": "new author"}, "resolved_at": null, "at": 1790000001}]
    }"#;

    #[test]
    fn the_snapshot_of_the_reference_is_read() {
        let snapshot: Snapshot = serde_json::from_str(SNAPSHOT).unwrap();
        let board = &snapshot.boards[0];
        assert_eq!(board.repository.name, "demo");
        assert_eq!(board.repository.github, None);
        assert_eq!(board.cards[0].column, "Backlog");
        assert_eq!(board.cards[0].note, None);
        assert_eq!(board.waiting[&1], "needs a person");
        assert_eq!(
            (board.cards[0].rounds, board.cards[0].run.as_ref()),
            (0, None)
        );
        let work = &board.cards[1];
        assert_eq!((work.rounds, work.tokens, work.refusals), (2, 900, 1));
        assert_eq!(work.blocked_by, [1]);
        assert_eq!(work.change.as_ref().unwrap().checks, "pending");
        assert_eq!((board.round_limit, board.live_runs()), (5, 1));
        assert_eq!(snapshot.inbox[0].kind, "approve_issue");
    }

    #[test]
    fn missing_parts_of_the_snapshot_are_empty() {
        let snapshot: Snapshot = serde_json::from_str("{}").unwrap();
        assert_eq!(snapshot, Snapshot::default());
    }
}
