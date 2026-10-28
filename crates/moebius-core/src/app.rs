//! The core put together: the store, the event bus, the repositories, the forges and the decision points, and the
//! operations the desktop API and the board engine perform on them. Every change to a card is published on the bus.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use moebius_plugin_api::Host;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::blobs::redact;
use crate::board::Runs;
use crate::bus::{AppEvent, Bus};
use crate::decide::{Decisions, Question};
use crate::forge::{Forge, ForgeError, NoForge};
use crate::github::Issue;
use crate::repos::{GitError, Repos, git};
use crate::store::{Card, Change, Repository, Run, Store, StoreError, now};
use crate::terminal::{MAX_PER_RUN, Slot, Slots};

/// The columns of the default workflow (spec 6).
pub mod columns {
    pub const BACKLOG: &str = "Backlog";
    pub const READY: &str = "Ready";
    pub const IN_PROGRESS: &str = "In progress";
    pub const IN_REVIEW: &str = "In review";
    pub const READY_TO_MERGE: &str = "Ready to merge";
    pub const DONE: &str = "Done";
}

/// The runs of a card that count: a retry gives the card a fresh start, so what ran before it no longer does.
pub fn runs_since_retry<'a>(runs: &'a [Run], card: &Card) -> impl Iterator<Item = &'a Run> {
    let retried_after = card.retried_after;
    runs.iter().filter(move |r| Some(r.id) > retried_after)
}

/// How long after a publish a change without checks for its commit is taken to be waiting for them; after that, the
/// repository has no checks (in seconds).
const CHECKS_GRACE: i64 = 120;

/// How much of the end of the output of a repository's checks is kept.
const CHECK_OUTPUT: usize = 4096;

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
    #[error("cannot read or write files: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0} not found")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0} exists already")]
    Exists(String),
    #[error("{0}")]
    Conflict(String),
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

/// How many characters of a chat's first message its title keeps.
const TITLE_LENGTH: usize = 80;

/// A chat as the list of chats shows it. Its title is the first line of its first message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chat {
    pub id: i64,
    pub agent: String,
    pub status: String,
    pub repository_id: Option<i64>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub title: String,
    /// The `seq` of the chat's last answer or error, which the person has to read.
    pub answered: i64,
    /// How far the person has read the chat (`POST /v1/chats/{id}/seen`); unread while below `answered`.
    pub seen: i64,
}

/// One thing in a card's history: `moved`, `run_started`, `run_ended`, `decided`, `inbox_added` or `inbox_resolved`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    pub at: i64,
    pub kind: String,
    pub data: serde_json::Value,
}

impl HistoryEntry {
    fn new(at: i64, kind: &str, data: serde_json::Value) -> Self {
        Self {
            at,
            kind: kind.into(),
            data,
        }
    }
}

/// The most characters of an ended chat's conversation that the chat continuing it is given.
const CONVERSATION_LIMIT: usize = 20_000;

pub struct App {
    pub store: Arc<Store>,
    pub bus: Bus,
    pub repos: Repos,
    pub decisions: Decisions,
    pub data_dir: PathBuf,
    /// How long a repository's own checks may run.
    pub checks_timeout: Duration,
    /// The checks going now, as change and commit.
    checking: Mutex<BTreeSet<(i64, String)>>,
    /// Held while a run event is stored and published, so that events are published in the order of their `seq`.
    run_events: Mutex<()>,
    /// The terminals open on each run.
    pub terminals: Arc<Slots>,
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
            checks_timeout: Duration::from_secs(1800),
            checking: Mutex::default(),
            run_events: Mutex::default(),
            terminals: Arc::default(),
            waiting: Mutex::default(),
            forges,
        }
    }

    /// Makes `forge` available to the repositories that name it.
    pub fn add_forge(&mut self, forge: Box<dyn Forge>) {
        self.forges.insert(forge.name(), forge);
    }

    /// The HTTP header git needs to reach `url` when it is a repository on github.com and the GitHub App is set up;
    /// none otherwise, so a token never goes to another host.
    pub fn git_auth(&self, url: &str) -> Result<Option<String>> {
        let (Some(forge), Some(name)) = (
            self.forges.get("github"),
            url.strip_prefix("https://github.com/"),
        ) else {
            return Ok(None);
        };
        let name = name.trim_end_matches('/').trim_end_matches(".git");
        Ok(forge.auth(url, name)?)
    }

    /// Brings the repository's clone up to date with its forge.
    pub fn refresh(&self, repository: &Repository) -> Result<()> {
        Ok(self.forge(repository)?.refresh(repository)?)
    }

    pub(crate) fn forge(&self, repository: &Repository) -> Result<&dyn Forge> {
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

    pub(crate) fn repository(&self, id: i64) -> Result<Repository> {
        self.store
            .repository(id)?
            .ok_or_else(|| AppError::NotFound(format!("repository {id}")))
    }

    pub fn card(&self, id: i64) -> Result<Card> {
        self.store
            .card(id)?
            .ok_or_else(|| AppError::NotFound(format!("card {id}")))
    }

    pub fn run(&self, id: i64) -> Result<Run> {
        self.store
            .run(id)?
            .ok_or_else(|| AppError::NotFound(format!("run {id}")))
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

    /// Creates an issue a person wrote in the app: a new card in the backlog. On a forge the issue is opened there
    /// first, the same way a confirmed grilling opens its issues, and the card is confirmed, since the person
    /// wrote it, so that it moves on without approval.
    pub fn create_issue(
        &self,
        repository_id: i64,
        title: &str,
        body: &str,
        author: &str,
    ) -> Result<Card> {
        let repository = self.repository(repository_id)?;
        if title.trim().is_empty() {
            return Err(AppError::Invalid("an issue needs a title".into()));
        }
        if repository.forge != "none" {
            return self.create_confirmed_issue(&repository, title, body, &BTreeSet::new());
        }
        let card = self
            .store
            .add_card(repository_id, title, body, author, columns::BACKLOG)?;
        self.bus.publish(AppEvent::new("card_added", json!(card)));
        Ok(card)
    }

    /// Creates an issue from a confirmed grilling: a card in the backlog that needs no approval, blocked by the issues
    /// with the numbers `blocked_by`. On a forge the issue is opened there first, with a "Blocked by" line for the
    /// poll to read, and the card gets its number, so that the poll finds it existing.
    pub(crate) fn create_confirmed_issue(
        &self,
        repository: &Repository,
        title: &str,
        body: &str,
        blocked_by: &BTreeSet<i64>,
    ) -> Result<Card> {
        let card = if repository.forge == "none" {
            self.store.add_confirmed_card(
                repository.id,
                None,
                title,
                body,
                PERSON,
                columns::BACKLOG,
                blocked_by,
            )?
        } else {
            let numbers: Vec<String> = blocked_by.iter().map(|n| format!("#{n}")).collect();
            let body = if numbers.is_empty() {
                body.to_string()
            } else {
                format!("{}\n\nBlocked by {}", body.trim(), numbers.join(", "))
                    .trim()
                    .to_string()
            };
            let number = self
                .forge(repository)?
                .create_issue(repository, title, &body)?;
            self.store.add_confirmed_card(
                repository.id,
                Some(number),
                title,
                &body,
                PERSON,
                columns::BACKLOG,
                blocked_by,
            )?
        };
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

    /// Records what a person did: `kind` names both the decision and the option they took; the other was to leave
    /// things as they were.
    fn record_action(
        &self,
        kind: &str,
        card_id: Option<i64>,
        run_id: Option<i64>,
        context: BTreeMap<String, String>,
        reason: &str,
    ) -> Result<()> {
        self.decisions.record_person(
            &Question {
                kind: kind.into(),
                context,
                options: vec![kind.into(), "leave".into()],
                fail_closed: "leave".into(),
                run_id,
                card_id,
            },
            kind,
            reason,
        )?;
        Ok(())
    }

    /// A person retries a card that is stuck or whose pull request was closed without a merge: the decision is
    /// recorded, the card's item is resolved and its limits start afresh, so the board starts work on it again. A card
    /// with a closed change goes back to In progress with a note saying so; its next publish opens a new change.
    pub fn retry_card(&self, card_id: i64, reason: &str) -> Result<Card> {
        let card = self.card(card_id)?;
        let stuck = self.store.has_open_inbox_item(card_id, "stuck")?;
        let closed = self.store.has_open_inbox_item(card_id, "change_closed")?;
        if !stuck && !closed {
            return Err(AppError::Invalid(format!(
                "card {card_id} is neither stuck nor has a closed pull request"
            )));
        }
        let context = BTreeMap::from([("title".into(), card.title.clone())]);
        self.record_action("retry", Some(card_id), None, context, reason)?;
        self.store.set_card_retried(card_id)?;
        self.store.resolve_inbox_for_card(card_id)?;
        if closed {
            self.store
                .set_card_note(card_id, "The pull request was closed without a merge.")?;
            return self.move_card(card_id, columns::IN_PROGRESS);
        }
        self.set_waiting(card_id, None);
        self.card(card_id)
    }

    /// A person rejects the change waiting to be merged: the decision is recorded and the card goes back to coding,
    /// where the next round is told the reason. The change stays open and is updated by that round.
    pub fn reject_change(&self, change_id: i64, reason: &str) -> Result<Card> {
        let change = self.change(change_id)?;
        let card = match change.card_id.map(|id| self.card(id)).transpose()? {
            Some(card) if change.state == "open" && card.column == columns::READY_TO_MERGE => card,
            _ => {
                return Err(AppError::Invalid(format!(
                    "change {change_id} is not waiting to be merged"
                )));
            }
        };
        let context = BTreeMap::from([
            ("title".into(), card.title.clone()),
            ("branch".into(), change.branch.clone()),
        ]);
        self.record_action("reject", Some(card.id), None, context, reason)?;
        let note = match reason.trim() {
            "" => "A person rejected the change.",
            reason => reason,
        };
        self.store.set_card_note(card.id, note)?;
        self.store.resolve_inbox_for_card(card.id)?;
        self.move_card(card.id, columns::IN_PROGRESS)
    }

    /// A person sends a message to a running run: the agent gets it and the run keeps it as a `steered` event.
    pub fn steer_run(&self, runs: &dyn Runs, run_id: i64, message: &str) -> Result<Run> {
        let run = self.run(run_id)?;
        if message.trim().is_empty() {
            return Err(AppError::Invalid("a steer needs a message".into()));
        }
        if !runs.steer(run_id, message) {
            return Err(AppError::Invalid(format!("run {run_id} is not running")));
        }
        let message = redact(message);
        self.record_run_event(run_id, "steered", &json!({ "message": message }))?;
        let context = BTreeMap::from([("message".into(), message)]);
        self.record_action("steer", run.card_id, Some(run_id), context, "")?;
        Ok(run)
    }

    /// A person stops a running run. It ends as `stopped` once its agent has settled, and its card then waits in the
    /// inbox as stuck.
    pub fn stop_run(&self, runs: &dyn Runs, run_id: i64, reason: &str) -> Result<Run> {
        let run = self.run(run_id)?;
        if !runs.stop(run_id) {
            return Err(AppError::Invalid(format!("run {run_id} is not running")));
        }
        self.record_action("stop", run.card_id, Some(run_id), BTreeMap::new(), reason)?;
        Ok(run)
    }

    /// A person opens a shell in the worktree of `run_id`, which has one while the run is starting, running or
    /// waiting, and at most [`MAX_PER_RUN`] shells at a time. Returns the worktree and the shell's place among the
    /// run's. Only the opening is recorded, as a `shell_opened` event: never what is typed or printed.
    pub fn open_shell(&self, run_id: i64) -> Result<(PathBuf, Slot)> {
        let run = self.run(run_id)?;
        let worktree = match (run.status.as_str(), run.worktree) {
            ("starting" | "running" | "waiting", Some(worktree))
                if Path::new(&worktree).is_dir() =>
            {
                PathBuf::from(worktree)
            }
            _ => {
                return Err(AppError::Invalid(format!(
                    "run {run_id} has no worktree to open a shell in"
                )));
            }
        };
        let slot = self.terminals.take(run_id).ok_or_else(|| {
            AppError::Invalid(format!(
                "run {run_id} has {MAX_PER_RUN} terminals open already"
            ))
        })?;
        self.record_run_event(run_id, "shell_opened", &json!({ "who": PERSON }))?;
        Ok((worktree, slot))
    }

    /// A person starts a chat with `agent`, in a branch of the repository `repository_id` or, without one, in an empty
    /// directory. The agent answers `message`; the answer arrives as the run's events.
    pub fn start_chat(
        self: &Arc<Self>,
        runs: &dyn Runs,
        agent: Option<&str>,
        repository_id: Option<i64>,
        message: &str,
        continues: Option<i64>,
    ) -> Result<Run> {
        // A chat that continues an ended one keeps its agent and repository unless others are named.
        let earlier = continues.map(|id| self.ended_chat(id)).transpose()?;
        let agent = agent
            .or(earlier.as_ref().map(|run| run.agent.as_str()))
            .unwrap_or(crate::pibolt::MOEBIUS_AGENT);
        let repository_id = repository_id.or(earlier.as_ref().and_then(|run| run.repository_id));
        if message.trim().is_empty() {
            return Err(AppError::Invalid("a chat needs a message".into()));
        }
        if !runs.has_agent(agent) {
            return Err(AppError::Invalid(format!("no agent named {agent}")));
        }
        if agent == crate::pibolt::GRILLING_AGENT && repository_id.is_none() {
            return Err(AppError::Invalid(
                "a grilling needs a repository, whose issues it ends in".into(),
            ));
        }
        let repository = repository_id.map(|id| self.repository(id)).transpose()?;
        let conversation = earlier
            .as_ref()
            .map(|run| self.conversation_of(run.id))
            .transpose()?;
        runs.start_chat(
            self,
            agent,
            repository.as_ref(),
            message,
            conversation.as_deref(),
        )
        .map_err(AppError::Invalid)
    }

    /// The chat `run_id`, which must have ended to be continued.
    fn ended_chat(&self, run_id: i64) -> Result<Run> {
        let run = self.run(run_id)?;
        if run.purpose != "chat" {
            return Err(AppError::Invalid(format!("run {run_id} is not a chat")));
        }
        if run.ended_at.is_none() {
            return Err(AppError::Invalid(format!(
                "chat {run_id} has not ended; write to it instead"
            )));
        }
        Ok(run)
    }

    /// What was said in a chat, oldest first: the person's messages and the agent's answers, at most the last
    /// [`CONVERSATION_LIMIT`] characters of it.
    fn conversation_of(&self, run_id: i64) -> Result<String> {
        let lines: Vec<String> = self
            .store
            .events(run_id)?
            .iter()
            .filter_map(|event| match event.kind.as_str() {
                "said" => Some(format!("Person: {}", event.data["message"].as_str()?)),
                "agent_end" => Some(format!(
                    "Agent: {}",
                    crate::pibolt::final_answer(&event.data)?
                )),
                _ => None,
            })
            .collect();
        let text = lines.join("\n\n");
        let start = text
            .char_indices()
            .rev()
            .nth(CONVERSATION_LIMIT.saturating_sub(1))
            .map_or(0, |(i, _)| i);
        Ok(text[start..].to_string())
    }

    /// A person writes to a chat that has not ended: the chat keeps the message as a `said` event, and the agent
    /// answers it when it is done with what it is doing.
    pub fn say_in_chat(&self, runs: &dyn Runs, run_id: i64, message: &str) -> Result<Run> {
        let run = self.running_chat(run_id)?;
        if message.trim().is_empty() {
            return Err(AppError::Invalid("a message cannot be blank".into()));
        }
        // Stored first, so that it comes before the agent's records of its answer.
        self.record_run_event(run_id, "said", &json!({ "message": redact(message) }))?;
        if !runs.say(run_id, message) {
            return Err(AppError::Invalid(format!("chat {run_id} has ended")));
        }
        Ok(run)
    }

    /// A person ends a chat that has not ended. It ends as `succeeded` once its agent has shut down and its work is
    /// committed and, if there is any, offered as a change (see `offer_chat_change`).
    pub fn end_chat(&self, runs: &dyn Runs, run_id: i64) -> Result<Run> {
        let run = self.running_chat(run_id)?;
        if !runs.end(run_id) {
            return Err(AppError::Invalid(format!("chat {run_id} has ended")));
        }
        Ok(run)
    }

    /// The chat `run_id`, if it exists and has not ended.
    fn running_chat(&self, run_id: i64) -> Result<Run> {
        let run = self.run(run_id)?;
        if run.purpose != "chat" {
            return Err(AppError::Invalid(format!("run {run_id} is not a chat")));
        }
        if run.ended_at.is_some() {
            return Err(AppError::Invalid(format!("chat {run_id} has ended")));
        }
        Ok(run)
    }

    /// The chats, newest first.
    pub fn chats(&self) -> Result<Vec<Chat>> {
        self.store
            .chats()?
            .into_iter()
            .map(|run| {
                let title = self.chat_title(run.id)?;
                Ok(Chat {
                    id: run.id,
                    agent: run.agent,
                    status: run.status,
                    repository_id: run.repository_id,
                    started_at: run.started_at,
                    ended_at: run.ended_at,
                    title,
                    answered: self.store.answered_seq(run.id)?,
                    seen: self.store.seen_seq(run.id)?,
                })
            })
            .collect()
    }

    /// A person has read the chat `run_id` up to the event `seq`.
    pub fn mark_chat_seen(&self, run_id: i64, seq: i64) -> Result<()> {
        let run = self.run(run_id)?;
        if run.purpose != "chat" {
            return Err(AppError::Invalid(format!("run {run_id} is not a chat")));
        }
        Ok(self.store.set_seen_seq(run_id, seq)?)
    }

    /// The title of a chat: the first line of its first message.
    fn chat_title(&self, run_id: i64) -> Result<String> {
        let first = self.store.events_after(run_id, 0, 1)?;
        Ok(first
            .first()
            .and_then(|event| event.data["message"].as_str())
            .unwrap_or_default()
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(TITLE_LENGTH)
            .collect())
    }

    /// Stores an event of a run and publishes it as `run_event` with the `seq` it was stored under.
    pub fn record_run_event(
        &self,
        run_id: i64,
        kind: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        let _ordered = self.run_events.lock().unwrap_or_else(|e| e.into_inner());
        // Pi's records can carry what a tool printed; nothing unredacted is stored or streamed.
        let data = &crate::blobs::redact_json(data);
        let seq = self.store.append_event(run_id, kind, data)?;
        self.bus.publish(AppEvent::new(
            "run_event",
            json!({ "run": run_id, "seq": seq, "kind": kind, "data": data }),
        ));
        Ok(())
    }

    /// What happened to a card, oldest first: its moves (since they were recorded), its runs starting and ending,
    /// the decisions made for it and its inbox items.
    pub fn card_history(&self, card_id: i64) -> Result<Vec<HistoryEntry>> {
        self.card(card_id)?;
        let mut history = Vec::new();
        for m in self.store.moves_for_card(card_id)? {
            let data = json!({ "from": m.from, "to": m.to, "note": m.note });
            history.push(HistoryEntry::new(m.at, "moved", data));
        }
        for run in self.store.runs_for_card(card_id)? {
            let data = json!({ "run": run.id, "agent": run.agent, "purpose": run.purpose });
            history.push(HistoryEntry::new(run.started_at, "run_started", data));
            if let Some(ended_at) = run.ended_at {
                let data = json!({ "run": run.id, "status": run.status });
                history.push(HistoryEntry::new(ended_at, "run_ended", data));
            }
        }
        for j in self.store.judgements_for_card(card_id)? {
            let data = json!({
                "judgement": j.id, "kind": j.kind, "choice": j.choice, "decider": j.decider, "reason": j.reason,
            });
            history.push(HistoryEntry::new(j.at, "decided", data));
        }
        for item in self.store.inbox_for_card(card_id)? {
            let data = json!({ "item": item.id, "kind": item.kind, "title": item.title });
            history.push(HistoryEntry::new(item.at, "inbox_added", data.clone()));
            if let Some(resolved_at) = item.resolved_at {
                history.push(HistoryEntry::new(resolved_at, "inbox_resolved", data));
            }
        }
        // Stable, so that what happened in the same second stays in the order of its kind above.
        history.sort_by_key(|entry| entry.at);
        Ok(history)
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
        let mut waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
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

    /// The numbers of the issues that block the card and are not done yet; an issue with no card counts as not done.
    pub fn blocked_by(&self, card: &Card) -> Result<Vec<i64>> {
        let mut blocked_by = Vec::new();
        for number in self.store.dependencies(card.id)? {
            let blocker = self.store.card_by_number(card.repository_id, number)?;
            if blocker.is_none_or(|b| b.column != columns::DONE) {
                blocked_by.push(number);
            }
        }
        Ok(blocked_by)
    }

    /// Why each waiting card among `cards` waits.
    pub fn waiting(&self, cards: &[Card]) -> BTreeMap<i64, String> {
        let waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
        cards
            .iter()
            .filter_map(|card| Some((card.id, waiting.get(&card.id)?.clone())))
            .collect()
    }

    /// Publishes the work of the card's latest successful `code` run on the branch of its change, `moebius/card-<number>`,
    /// through the repository's forge: the first time that creates the change, later it updates it. Each round's branch
    /// descends from the last, so the branch only ever moves forward. Does nothing when this work is published.
    pub fn publish_change(&self, card_id: i64) -> Result<Change> {
        let card = self.card(card_id)?;
        let repository = self.repository(card.repository_id)?;
        let forge = self.forge(&repository)?;
        let code_branch = self
            .store
            .code_branch(card_id)?
            .ok_or_else(|| AppError::Invalid("the card has no code run to publish".into()))?;
        let clone = Path::new(&repository.path);
        let head = git(clone, &["rev-parse", &code_branch])?;
        let existing = self.store.open_change(card_id)?;
        if let Some(change) = existing.as_ref().filter(|c| c.head.as_ref() == Some(&head)) {
            return Ok(change.clone());
        }
        let branch = format!("moebius/card-{}", card.number);
        self.repos.point_branch(clone, &branch, &head)?;
        let pull_request = existing.as_ref().and_then(|c| c.pull_request);
        let body = format!("Closes #{}\n\nMade by Moebius.", card.number);
        let pull_request = forge.publish(&repository, &card.title, &body, &branch, pull_request)?;
        let change = match existing {
            Some(change) => {
                self.store.set_change_head(change.id, &head, now())?;
                self.change(change.id)?
            }
            None => self.store.add_change(
                card.repository_id,
                Some(card_id),
                &branch,
                pull_request,
                &head,
            )?,
        };
        self.bus
            .publish(AppEvent::new("change_updated", json!(change)));
        Ok(change)
    }

    /// Publishes the card's change and puts it in the inbox, ready to merge.
    pub fn offer_change(&self, card_id: i64) -> Result<Change> {
        let change = self.publish_change(card_id)?;
        let card = self.card(card_id)?;
        self.store.add_inbox_item(
            "ready_to_merge",
            Some(card_id),
            &card.title,
            &json!({ "change": change.id, "branch": change.branch }),
        )?;
        self.bus
            .publish(AppEvent::new("change_offered", json!(change)));
        self.move_card(card_id, columns::READY_TO_MERGE)?;
        Ok(change)
    }

    /// Offers what a chat in a repository left on its branch as a change without a card: published through the
    /// repository's forge and put in the inbox, ready to merge. A chat that left nothing beyond the repository's
    /// default branch, or has no repository, offers nothing.
    pub fn offer_chat_change(&self, run_id: i64) -> Result<Option<Change>> {
        let run = self.run(run_id)?;
        let Some(repository_id) = run.repository_id else {
            return Ok(None);
        };
        let repository = self.repository(repository_id)?;
        let clone = Path::new(&repository.path);
        let branch = run.branch();
        let beyond_base = format!("{}..{branch}", repository.base_ref());
        if git(clone, &["rev-list", "--count", &beyond_base])? == "0" {
            return Ok(None);
        }
        let title = self.chat_title(run_id)?;
        let body = format!(
            "This change came from chat {run_id} with the agent {}.\n\nMade by Moebius.",
            run.agent
        );
        self.offer_branch_change(&repository, &branch, &title, &body)
            .map(Some)
    }

    /// Offers `branch` of `repository`, which has commits beyond the default branch, as a change without a card:
    /// published through the repository's forge and put in the inbox, ready to merge.
    pub fn offer_branch_change(
        &self,
        repository: &Repository,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<Change> {
        let head = git(Path::new(&repository.path), &["rev-parse", branch])?;
        let forge = self.forge(repository)?;
        let pull_request = forge.publish(repository, title, body, branch, None)?;
        let change = self
            .store
            .add_change(repository.id, None, branch, pull_request, &head)?;
        self.store.add_inbox_item(
            "ready_to_merge",
            None,
            title,
            &json!({ "change": change.id, "branch": change.branch }),
        )?;
        self.bus
            .publish(AppEvent::new("change_offered", json!(change)));
        Ok(change)
    }

    /// How the checks of the change stand for its current commit: `pending`, `success`, `failure` or `none`. Checks
    /// recorded for an older commit do not count; on GitHub the change then waits for its checks for a while after
    /// it was published, so that a pushed commit is not taken for one without CI.
    pub fn checks_of(&self, change: &Change) -> Result<&'static str> {
        let recorded = change
            .checks
            .as_deref()
            .filter(|_| change.checks_head == change.head);
        Ok(match recorded {
            Some("success") => "success",
            Some("failure") => "failure",
            Some(_) => "pending",
            None if self.repository(change.repository_id)?.forge == "github"
                && now() - change.published_at.unwrap_or(0) < CHECKS_GRACE =>
            {
                "pending"
            }
            None => "none",
        })
    }

    /// The files that keep the change from merging cleanly into the repository's default branch, one per line; none
    /// when it merges cleanly.
    pub fn conflict_of(&self, change: &Change) -> Result<Option<String>> {
        let repository = self.repository(change.repository_id)?;
        Ok(self.repos.conflicts(
            Path::new(&repository.path),
            &repository.base_ref(),
            &change.branch,
        )?)
    }

    /// Runs the repository's own checks, the command in `.moebius/checks` on its default branch (never on the change's,
    /// so that its author cannot change them), on the card's change. They run on a thread of their own, in a throwaway
    /// worktree; meanwhile the change's checks are `pending`. Does nothing for a repository on a forge, which has its
    /// own checks, or without that file, or when the checks already ran or are running for the change's commit.
    pub fn run_checks(self: &Arc<Self>, card_id: i64) -> Result<Change> {
        // Held while the change is read: checks that end record their result before they are removed from the set,
        // so a change read under the lock is final when its checks are not in the set.
        let mut checking = self.checking.lock().expect("checks are not poisoned");
        let change = self
            .store
            .open_change(card_id)?
            .ok_or_else(|| AppError::Invalid("the card has no change to check".into()))?;
        let repository = self.repository(change.repository_id)?;
        let (Some(head), Some(script)) = (change.head.clone(), self.checks_script(&repository))
        else {
            return Ok(change);
        };
        let key = (change.id, head.clone());
        let finished = change.checks_head.as_ref() == Some(&head)
            && change.checks.as_deref().is_some_and(|c| c != "pending");
        if finished || checking.contains(&key) {
            return Ok(change);
        }
        self.store
            .set_local_checks(change.id, "pending", &head, None)?;
        checking.insert(key.clone());
        drop(checking);
        let app = self.clone();
        let (checked, running) = (change.clone(), key.clone());
        let started = std::thread::Builder::new()
            .name(format!("checks of change {}", change.id))
            .spawn(move || {
                app.check_change(&repository, &checked, &head, &script);
                app.checking
                    .lock()
                    .expect("checks are not poisoned")
                    .remove(&running);
            });
        if let Err(e) = started {
            // Nothing runs, so the next call starts the checks again.
            self.checking
                .lock()
                .expect("checks are not poisoned")
                .remove(&key);
            return Err(AppError::Invalid(format!("cannot start the checks: {e}")));
        }
        let change = self.change(change.id)?;
        self.bus
            .publish(AppEvent::new("change_updated", json!(change)));
        Ok(change)
    }

    /// The repository's own checks: the command in `.moebius/checks` on its default branch. None for a repository on a
    /// forge, which has its own checks, and for one without that file.
    fn checks_script(&self, repository: &Repository) -> Option<String> {
        if repository.forge != "none" {
            return None;
        }
        let spec = format!("{}:.moebius/checks", repository.base_ref());
        git(Path::new(&repository.path), &["show", &spec]).ok()
    }

    /// Runs `script` on the commit `rev` in a throwaway worktree named `name` and returns whether it succeeded and the
    /// end of its output.
    fn run_script_on(
        &self,
        repository: &Repository,
        name: &str,
        rev: &str,
        script: &str,
    ) -> (bool, String) {
        let clone = Path::new(&repository.path);
        match self
            .repos
            .add_detached_worktree(&repository.name, clone, name, rev)
        {
            Ok(worktree) => {
                let log = self.data_dir.join(format!("{name}.log"));
                let result = execute(script, &worktree, &log, self.checks_timeout);
                let _ = std::fs::remove_file(&log);
                let _ = self.repos.remove_worktree(clone, &worktree);
                result
            }
            Err(e) => (false, format!("cannot make a worktree for the checks: {e}")),
        }
    }

    /// Runs the repository's own checks on the commit `rev` and waits for them: whether they succeeded, or none when the
    /// repository has none (see `run_checks`). For a trial (see `trial`), which has no change to record them on.
    pub fn local_checks(&self, repository: &Repository, name: &str, rev: &str) -> Option<bool> {
        let script = self.checks_script(repository)?;
        Some(self.run_script_on(repository, name, rev, &script).0)
    }

    /// Runs `script` on `head` in a worktree of its own and records how it ended, unless the change has moved on.
    fn check_change(&self, repository: &Repository, change: &Change, head: &str, script: &str) {
        // Named by commit too, as checks of an older commit may still be going when a newer one is published.
        let name = format!("checks-{}-{}", change.id, &head[..12.min(head.len())]);
        let (succeeded, output) = self.run_script_on(repository, &name, head, script);
        let current = self.change(change.id);
        if current.is_ok_and(|c| c.head.as_deref() == Some(head)) {
            let checks = if succeeded { "success" } else { "failure" };
            let _ = self
                .store
                .set_local_checks(change.id, checks, head, Some(&output));
            if let Ok(change) = self.change(change.id) {
                self.bus
                    .publish(AppEvent::new("change_updated", json!(change)));
            }
        }
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

    /// A change is merged, here or on the forge: its inbox items are resolved and its card, if it has one, is done.
    pub fn change_merged(&self, change: &Change, commit: Option<&str>) -> Result<()> {
        self.store.set_change_state(change.id, "merged")?;
        self.bus.publish(AppEvent::new(
            "change_merged",
            json!({ "change": change.id, "commit": commit }),
        ));
        match change.card_id {
            Some(card_id) => {
                self.store.resolve_inbox_for_card(card_id)?;
                self.move_card(card_id, columns::DONE)?;
            }
            None => self.store.resolve_inbox_for_change(change.id)?,
        }
        Ok(())
    }

    /// A change's pull request was closed on the forge without being merged: the change is closed and, when it has a
    /// card, the person is told.
    pub fn change_closed(&self, change: &Change) -> Result<()> {
        self.store.set_change_state(change.id, "closed")?;
        self.bus.publish(AppEvent::new(
            "change_closed",
            json!({ "change": change.id }),
        ));
        match change.card_id {
            Some(card_id) => {
                let card = self.card(card_id)?;
                self.store.resolve_inbox_for_card(card_id)?;
                self.store.add_inbox_item(
                    "change_closed",
                    Some(card_id),
                    &card.title,
                    &json!({ "change": change.id, "pull_request": change.pull_request }),
                )?;
            }
            None => self.store.resolve_inbox_for_change(change.id)?,
        }
        Ok(())
    }

    /// Records the checks of a change's pull request for its head commit `checks_head`, the failing ones, and the
    /// latest review, and publishes the change if they differ from what was known.
    pub fn record_change_status(
        &self,
        change: &Change,
        checks: Option<&str>,
        checks_head: &str,
        failed_checks: Option<&str>,
        review: Option<&str>,
    ) -> Result<()> {
        if change.checks.as_deref() == checks
            && change.checks_head.as_deref() == Some(checks_head)
            && change.failed_checks.as_deref() == failed_checks
            && change.review.as_deref() == review
        {
            return Ok(());
        }
        self.store
            .set_change_status(change.id, checks, checks_head, failed_checks, review)?;
        self.bus.publish(AppEvent::new(
            "change_updated",
            json!(self.change(change.id)?),
        ));
        Ok(())
    }
}

/// Runs `script` with `sh -c` in `dir`, writing its output to `log`, and stops it (and what it started) after
/// `timeout`. Returns whether it succeeded and the end of its output with secrets removed.
fn execute(script: &str, dir: &Path, log: &Path, timeout: Duration) -> (bool, String) {
    let run = || -> std::io::Result<(bool, bool)> {
        let file = std::fs::File::create(log)?;
        let mut child = Command::new("sh")
            .args(["-c", script])
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(file)
            .process_group(0)
            .spawn()?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok((status.success(), false));
            }
            if Instant::now() >= deadline {
                let group = format!("-{}", child.id());
                let _ = Command::new("kill").args(["-KILL", "--", &group]).status();
                child.wait()?;
                return Ok((false, true));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    match run() {
        Ok((succeeded, timed_out)) => {
            let mut output = tail(log);
            if timed_out {
                output.push_str(&format!(
                    "\nThe checks were stopped after {} seconds.",
                    timeout.as_secs()
                ));
            }
            (succeeded, output)
        }
        Err(e) => (false, format!("cannot run the checks: {e}")),
    }
}

/// The end of the file `log`, without secrets, as much as is kept of the output of checks.
pub(crate) fn tail(log: &Path) -> String {
    let Ok(mut file) = std::fs::File::open(log) else {
        return String::new();
    };
    // Read more than is kept, so that a secret cut in two by the start is still whole when it is redacted.
    let len = file.metadata().map_or(0, |m| m.len());
    let _ = file.seek(SeekFrom::Start(len.saturating_sub(4 * CHECK_OUTPUT as u64)));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    let text = redact(&String::from_utf8_lossy(&bytes));
    let start = (text.len().saturating_sub(CHECK_OUTPUT)..)
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(text.len());
    text[start..].trim().to_string()
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
    fn checks_are_stopped_after_their_time_and_their_output_is_cut_and_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let run =
            |script: &str, seconds| execute(script, dir.path(), &log, Duration::from_secs(seconds));

        let (succeeded, output) = run("echo started; sleep 30", 1);
        assert!(!succeeded);
        assert_eq!(output, "started\nThe checks were stopped after 1 seconds.");

        let token = format!("ghp_{}", "a".repeat(36));
        let (succeeded, output) = run(
            &format!("yes line | head -c 20000; echo {token}; echo END"),
            30,
        );
        assert!(succeeded);
        assert!(output.len() <= CHECK_OUTPUT, "{}", output.len());
        assert!(
            output.ends_with("[redacted:github-token]\nEND"),
            "{output:?}"
        );

        let (succeeded, output) = run("echo nope >&2; exit 3", 30);
        assert_eq!((succeeded, output.as_str()), (false, "nope"));
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
