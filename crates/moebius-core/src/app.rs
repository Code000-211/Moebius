//! The core put together: the store, the event bus, the repositories, the forges and the decision points, and the
//! operations the desktop API and the board engine perform on them. Every change to a card is published on the bus.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use moebius_plugin_api::Host;
use serde_json::json;

use crate::bus::{AppEvent, Bus};
use crate::decide::{Decisions, Question};
use crate::forge::{Forge, NoForge};
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

/// Why an operation failed.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("{0} not found")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

pub struct App {
    pub store: Arc<Store>,
    pub bus: Bus,
    pub repos: Repos,
    pub decisions: Decisions,
    pub data_dir: PathBuf,
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
            forges,
        }
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
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
            || name.starts_with('.')
        {
            return Err(AppError::Invalid(format!(
                "{name:?} is not a repository name"
            )));
        }
        let clone = self.repos.clone_or_fetch(name, url)?;
        let branch = self.repos.default_branch(&clone)?;
        let repository =
            self.store
                .add_repository(name, &clone.to_string_lossy(), "none", &branch)?;
        self.bus
            .publish(AppEvent::new("repository_added", json!(repository)));
        Ok(repository)
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

    pub fn move_card(&self, card_id: i64, column: &str) -> Result<Card> {
        let from = self.card(card_id)?.column;
        self.store.move_card(card_id, column)?;
        self.bus.publish(AppEvent::new(
            "card_moved",
            json!({ "card": card_id, "from": from, "to": column }),
        ));
        self.card(card_id)
    }

    /// Offers the work on `branch` as a change for the card and puts it in the inbox, ready to merge.
    pub fn offer_change(&self, card_id: i64, branch: &str) -> Result<Change> {
        let card = self.card(card_id)?;
        let change = self
            .store
            .add_change(card.repository_id, Some(card_id), branch)?;
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
        let forge = self
            .forges
            .get(repository.forge.as_str())
            .ok_or_else(|| AppError::NotFound(format!("forge {}", repository.forge)))?;
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
        self.store.set_change_state(change_id, "merged")?;
        self.bus.publish(AppEvent::new(
            "change_merged",
            json!({ "change": change_id, "commit": commit }),
        ));
        if let Some(card_id) = change.card_id {
            self.store.resolve_inbox_for_card(card_id)?;
            self.move_card(card_id, columns::DONE)?;
        }
        Ok(commit)
    }
}
