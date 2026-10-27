//! The core put together: the store, the event bus, the repositories, the forges and the decision points, and the
//! operations the desktop API and the board engine perform on them. Every change to a card is published on the bus.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use moebius_plugin_api::Host;
use regex::Regex;
use serde_json::json;

use crate::bus::{AppEvent, Bus};
use crate::decide::{Decisions, Question};
use crate::forge::{Forge, ForgeError, NoForge};
use crate::github::Issue;
use crate::repos::{GitError, Repos};
use crate::store::{Card, Change, Repository, Store, StoreError};

/// The columns of the default workflow (spec 6).
pub mod columns {
    pub const BACKLOG: &str = "Backlog";
    pub const READY: &str = "Ready";
    pub const IN_PROGRESS: &str = "In progress";
    pub const IN_REVIEW: &str = "In review";
    pub const READY_TO_MERGE: &str = "Ready to merge";
    pub const DONE: &str = "Done";
}

/// The author of issues a person created in the desktop app; their cards are trusted.
pub const PERSON: &str = "person";

/// Why an operation failed.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Forge(#[from] ForgeError),
    #[error("{0} not found")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
}

/// The issue numbers an issue's body says it is blocked by: "blocked by #12", "Blocked by: #3, #4".
fn blocked_by(body: &str) -> BTreeSet<i64> {
    static BLOCKED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)blocked by:?[ \t]*((?:#\d+[ \t,]*)+)").unwrap());
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#(\d+)").unwrap());
    BLOCKED
        .captures_iter(body)
        .flat_map(|blocked| {
            NUMBER
                .captures_iter(&blocked[1])
                .filter_map(|n| n[1].parse().ok())
                .collect::<Vec<i64>>()
        })
        .collect()
}

/// Whether `name` is made of letters, digits and `-_.` only.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

pub type Result<T> = std::result::Result<T, AppError>;

pub struct App {
    pub store: Arc<Store>,
    pub bus: Bus,
    pub repos: Repos,
    pub decisions: Decisions,
    pub data_dir: PathBuf,
    /// Why each waiting card waits, as its column script last said (`moebius.wait`, `ask`, `stuck`). Not stored: a
    /// reason is rebuilt by the next tick after a restart.
    waiting: Mutex<HashMap<i64, String>>,
    forges: BTreeMap<&'static str, Box<dyn Forge>>,
}

impl App {
    /// The core over `store`, with its repositories under `data_dir` and the bundled forges.
    pub fn new(store: Arc<Store>, host: Arc<Host>, data_dir: &Path) -> Self {
        let repos = Repos::new(data_dir);
        let mut forges: BTreeMap<&'static str, Box<dyn Forge>> = BTreeMap::new();
        let none = NoForge::new(repos.clone());
        forges.insert(none.name(), Box::new(none));
        Self {
            decisions: Decisions::new(store.clone(), host),
            store,
            bus: Bus::new(),
            repos,
            data_dir: data_dir.to_path_buf(),
            waiting: Mutex::default(),
            forges,
        }
    }

    /// Makes `forge` available to the repositories that name it.
    pub fn add_forge(&mut self, forge: Box<dyn Forge>) {
        self.forges.insert(forge.name(), forge);
    }

    /// Brings the repository's clone up to date with its forge.
    pub fn refresh(&self, repository: &Repository) -> Result<()> {
        Ok(self.forge(repository)?.refresh(repository)?)
    }

    fn forge(&self, repository: &Repository) -> Result<&dyn Forge> {
        self.forges
            .get(repository.forge.as_str())
            .map(|forge| &**forge)
            .ok_or_else(|| {
                AppError::Invalid(format!(
                    "{} needs the forge {}, which is not set up",
                    repository.name, repository.forge
                ))
            })
    }

    fn repository(&self, id: i64) -> Result<Repository> {
        self.store
            .repository(id)?
            .ok_or_else(|| AppError::NotFound(format!("repository {id}")))
    }

    pub fn card(&self, id: i64) -> Result<Card> {
        self.store
            .card(id)?
            .ok_or_else(|| AppError::NotFound(format!("card {id}")))
    }

    fn change(&self, id: i64) -> Result<Change> {
        self.store
            .change(id)?
            .ok_or_else(|| AppError::NotFound(format!("change {id}")))
    }

    /// Clones `url` and adds it as a repository without a forge.
    pub fn add_repository(&self, name: &str, url: &str) -> Result<Repository> {
        let (path, branch) = self.clone_repository(name, url, None)?;
        let repository = self.store.add_repository(name, &path, "none", &branch)?;
        self.bus
            .publish(AppEvent::new("repository_added", json!(repository)));
        Ok(repository)
    }

    /// Clones `url` and adds it as a repository whose issues are polled from GitHub as `github` (`owner/repo`).
    /// Issues by the logins in `trusted` need no approval. With the GitHub App set up, the clone authenticates as
    /// its installation, so private repositories work too.
    pub fn add_github_repository(
        &self,
        name: &str,
        url: &str,
        github: &str,
        trusted: &[String],
    ) -> Result<Repository> {
        if !github
            .split_once('/')
            .is_some_and(|(owner, repo)| is_plain_name(owner) && is_plain_name(repo))
        {
            return Err(AppError::Invalid(format!(
                "{github:?} is not a GitHub repository as owner/repo"
            )));
        }
        let auth = match self.forges.get("github") {
            Some(forge) => forge.auth(url, github)?,
            None => None,
        };
        let (path, branch) = self.clone_repository(name, url, auth.as_deref())?;
        let repository = self
            .store
            .add_github_repository(name, &path, &branch, github, trusted)?;
        self.bus
            .publish(AppEvent::new("repository_added", json!(repository)));
        Ok(repository)
    }

    /// Clones `url` as the repository `name` and returns the clone's path and default branch.
    fn clone_repository(
        &self,
        name: &str,
        url: &str,
        auth: Option<&str>,
    ) -> Result<(String, String)> {
        if !is_plain_name(name) || name.starts_with('.') {
            return Err(AppError::Invalid(format!(
                "{name:?} is not a repository name"
            )));
        }
        if self.store.repositories()?.iter().any(|r| r.name == name) {
            return Err(AppError::Invalid(format!(
                "the repository name {name:?} is taken"
            )));
        }
        let clone = self.repos.clone_or_fetch(name, url, auth)?;
        let branch = self.repos.default_branch(&clone)?;
        Ok((clone.to_string_lossy().into_owned(), branch))
    }

    /// Whether the issue behind `card` may move on without a person's approval: a person created it in the desktop
    /// app, or its author is one of the repository's trusted GitHub logins. Anything unknown is not trusted.
    pub fn is_trusted(&self, card: &Card) -> bool {
        let Ok(Some(repository)) = self.store.repository(card.repository_id) else {
            return false;
        };
        if repository.forge == "none" {
            return card.author == PERSON;
        }
        repository
            .trusted
            .iter()
            .any(|login| login.eq_ignore_ascii_case(&card.author))
    }

    /// Brings the cards of `repository` in line with its GitHub issues: open issues get a card in the backlog,
    /// changed titles and bodies are copied, closed issues move their card to done, and "blocked by #n" in a body
    /// becomes the card's dependencies. A closed issue without a card is not worth one.
    pub fn sync_issues(&self, repository: &Repository, issues: &[Issue]) -> Result<()> {
        for issue in issues {
            let body = issue.body.as_deref().unwrap_or_default();
            let card = match self.store.card_by_number(repository.id, issue.number)? {
                Some(card) => {
                    if card.title != issue.title || card.body != body {
                        self.store.set_card_text(card.id, &issue.title, body)?;
                        self.bus
                            .publish(AppEvent::new("card_updated", json!(self.card(card.id)?)));
                    }
                    card
                }
                None if issue.state == "open" => {
                    let card = self.store.add_numbered_card(
                        repository.id,
                        issue.number,
                        &issue.title,
                        body,
                        &issue.user.login,
                        columns::BACKLOG,
                    )?;
                    self.bus.publish(AppEvent::new("card_added", json!(card)));
                    card
                }
                None => continue,
            };
            let mut blockers = blocked_by(body);
            blockers.remove(&issue.number);
            self.store.set_dependencies(card.id, &blockers)?;
            if issue.state == "closed" && card.column != columns::DONE {
                self.move_card(card.id, columns::DONE)?;
            }
        }
        Ok(())
    }

    /// Creates an issue in a repository without a forge: a new card in the backlog.
    pub fn create_issue(
        &self,
        repository_id: i64,
        title: &str,
        body: &str,
        author: &str,
    ) -> Result<Card> {
        let repository = self.repository(repository_id)?;
        if repository.forge != "none" {
            return Err(AppError::Invalid(format!(
                "issues of {} are created on its forge, {}",
                repository.name, repository.forge
            )));
        }
        if title.trim().is_empty() {
            return Err(AppError::Invalid("an issue needs a title".into()));
        }
        let card = self
            .store
            .add_card(repository_id, title, body, author, columns::BACKLOG)?;
        self.bus.publish(AppEvent::new("card_added", json!(card)));
        Ok(card)
    }

    /// A person approves an outside issue: the decision is recorded and the card moves to ready.
    pub fn approve_issue(&self, card_id: i64, reason: &str) -> Result<Card> {
        let card = self.card(card_id)?;
        if card.column != columns::BACKLOG {
            return Err(AppError::Invalid(format!(
                "card {card_id} is not in the backlog"
            )));
        }
        self.decisions.record_person(
            &Question {
                kind: "approve_issue".into(),
                context: BTreeMap::from([
                    ("title".into(), card.title.clone()),
                    ("body".into(), card.body.clone()),
                    ("author".into(), card.author.clone()),
                ]),
                options: vec!["approve".into(), "wait".into()],
                fail_closed: "wait".into(),
                run_id: None,
                card_id: Some(card_id),
            },
            "approve",
            reason,
        )?;
        self.store.resolve_inbox_for_card(card_id)?;
        self.move_card(card_id, columns::READY)
    }

    pub fn move_card(&self, card_id: i64, column: &str) -> Result<Card> {
        let from = self.card(card_id)?.column;
        self.store.move_card(card_id, column)?;
        self.bus.publish(AppEvent::new(
            "card_moved",
            json!({ "card": card_id, "from": from, "to": column }),
        ));
        self.set_waiting(card_id, None);
        self.card(card_id)
    }

    /// Why the card waits now, or none when it does not. Publishes `card_waiting` when that is not what it was.
    pub fn set_waiting(&self, card_id: i64, reason: Option<String>) {
        let mut waiting = self
            .waiting
            .lock()
            .expect("waiting reasons are not poisoned");
        let before = match &reason {
            Some(reason) => waiting.insert(card_id, reason.clone()),
            None => waiting.remove(&card_id),
        };
        if before != reason {
            self.bus.publish(AppEvent::new(
                "card_waiting",
                json!({ "card": card_id, "reason": reason }),
            ));
        }
    }

    /// Why each waiting card among `cards` waits.
    pub fn waiting(&self, cards: &[Card]) -> BTreeMap<i64, String> {
        let waiting = self
            .waiting
            .lock()
            .expect("waiting reasons are not poisoned");
        cards
            .iter()
            .filter_map(|card| Some((card.id, waiting.get(&card.id)?.clone())))
            .collect()
    }

    /// Offers the work on `branch` as a change for the card, through the repository's forge, and puts it in the
    /// inbox, ready to merge.
    pub fn offer_change(&self, card_id: i64, branch: &str) -> Result<Change> {
        let card = self.card(card_id)?;
        let repository = self.repository(card.repository_id)?;
        let pull_request = self.forge(&repository)?.offer(&repository, &card, branch)?;
        let change =
            self.store
                .add_change(card.repository_id, Some(card_id), branch, pull_request)?;
        self.store.add_inbox_item(
            "ready_to_merge",
            Some(card_id),
            &card.title,
            &json!({ "change": change.id, "branch": branch }),
        )?;
        self.bus
            .publish(AppEvent::new("change_offered", json!(change)));
        self.move_card(card_id, columns::READY_TO_MERGE)?;
        Ok(change)
    }

    /// A person merges a change: the decision is recorded, the forge merges, and the card moves to done.
    pub fn merge_change(&self, change_id: i64, reason: &str) -> Result<String> {
        let change = self.change(change_id)?;
        if change.state != "open" {
            return Err(AppError::Invalid(format!(
                "change {change_id} is {}",
                change.state
            )));
        }
        let repository = self.repository(change.repository_id)?;
        let forge = self.forge(&repository)?;
        self.decisions.record_person(
            &Question {
                kind: "merge".into(),
                context: BTreeMap::from([
                    ("repository".into(), repository.name.clone()),
                    ("branch".into(), change.branch.clone()),
                ]),
                options: vec!["merge".into(), "reject".into()],
                fail_closed: "reject".into(),
                run_id: None,
                card_id: change.card_id,
            },
            "merge",
            reason,
        )?;
        let message = match change.card_id.map(|id| self.card(id)).transpose()? {
            Some(card) => format!("Merge #{}: {}", card.number, card.title),
            None => format!("Merge {}", change.branch),
        };
        let commit = forge.merge(&repository, &change, &message)?;
        self.change_merged(&change, Some(&commit))?;
        Ok(commit)
    }

    /// A change is merged, here or on the forge: its inbox items are resolved and its card is done.
    pub fn change_merged(&self, change: &Change, commit: Option<&str>) -> Result<()> {
        self.store.set_change_state(change.id, "merged")?;
        self.bus.publish(AppEvent::new(
            "change_merged",
            json!({ "change": change.id, "commit": commit }),
        ));
        if let Some(card_id) = change.card_id {
            self.store.resolve_inbox_for_card(card_id)?;
            self.move_card(card_id, columns::DONE)?;
        }
        Ok(())
    }

    /// A change's pull request was closed on the forge without being merged: the change is closed and the person is
    /// told.
    pub fn change_closed(&self, change: &Change) -> Result<()> {
        self.store.set_change_state(change.id, "closed")?;
        self.bus.publish(AppEvent::new(
            "change_closed",
            json!({ "change": change.id }),
        ));
        if let Some(card_id) = change.card_id {
            let card = self.card(card_id)?;
            self.store.resolve_inbox_for_card(card_id)?;
            self.store.add_inbox_item(
                "change_closed",
                Some(card_id),
                &card.title,
                &json!({ "change": change.id, "pull_request": change.pull_request }),
            )?;
        }
        Ok(())
    }

    /// Records the checks and review of a change's pull request, and publishes the change if they differ from what
    /// was known.
    pub fn record_change_status(
        &self,
        change: &Change,
        checks: Option<&str>,
        review: Option<&str>,
    ) -> Result<()> {
        if change.checks.as_deref() == checks && change.review.as_deref() == review {
            return Ok(());
        }
        self.store.set_change_status(change.id, checks, review)?;
        self.bus.publish(AppEvent::new(
            "change_updated",
            json!(self.change(change.id)?),
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use moebius_plugin_api::Host;

    #[test]
    fn blockers_are_read_from_a_body() {
        let numbers = |body| blocked_by(body).into_iter().collect::<Vec<_>>();
        assert_eq!(numbers("Needs the parser.\n\nblocked by #12"), [12]);
        assert_eq!(numbers("Blocked by: #3, #4\nBLOCKED BY #9 #3"), [3, 4, 9]);
        assert_eq!(numbers("Fixes #5, not blocked by anything"), [] as [i64; 0]);
    }

    #[test]
    fn who_is_trusted_depends_on_the_repository() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = App::new(
            store.clone(),
            Arc::new(Host::new(std::time::Duration::from_secs(1))),
            dir.path(),
        );
        let local = store
            .add_repository("local", "/srv/local", "none", "main")
            .unwrap();
        let remote = store
            .add_github_repository(
                "remote",
                "/srv/remote",
                "main",
                "acme/remote",
                &["Leon".into()],
            )
            .unwrap();
        let card = |repository: &Repository, author| {
            store
                .add_card(repository.id, "Card", "", author, columns::BACKLOG)
                .unwrap()
        };
        assert!(app.is_trusted(&card(&local, PERSON)));
        assert!(!app.is_trusted(&card(&local, "leon")));
        assert!(app.is_trusted(&card(&remote, "leon")), "logins ignore case");
        assert!(!app.is_trusted(&card(&remote, "mallory")));
        assert!(
            !app.is_trusted(&card(&remote, PERSON)),
            "a GitHub user named like the desktop's person is an outsider"
        );
    }
}
