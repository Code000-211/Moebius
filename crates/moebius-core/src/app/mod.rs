//! The core put together: the store, the event bus, the repositories, the forges and the decision points, and the
//! operations the desktop API and the board engine perform on them. Every change to a card is published on the bus.
//!
//! This module holds `App` itself, its errors and the lookups every operation shares; the operations are grouped by
//! what they act on: `repositories` adds and edits repositories, `cards` keeps cards in line with their issues and
//! moves them, `issues` is the issue tracker (parents, blocking, labels, claims, comments, closing), `issues` is the issue tracker (parents, blocking, labels, claims, comments, closing), `stopping` holds, releases and deletes a card, `runs` is a person's hand in a running run (steering,
//! stopping, shells), `chats` starts and carries on chats, `changes` publishes, offers and merges changes, `checks`
//! runs a repository's own checks on them, `packages` proposes installing, updating and removing Pi packages as changes
//! of the harness configuration repository, `localize` gives a code run the files its issue is likely to touch and
//! records what came of it, `reproduce` runs a change's reproduction test for a repository that works test first,
//! `projects` reads a project's settings from the harness configuration repository,
//! `whereabouts` knows where the server and the app run, `automations` keeps the automations and starts their chats
//! when their schedules come due, and `plugin_settings` keeps what plugins ask of each repository and of the
//! server.

mod attempts;
mod automations;
mod board;
mod cards;
mod changes;
mod chats;
mod checks;
mod deletions;
mod diffs;
mod issues;
mod localize;
mod packages;
pub mod plugin_settings;
mod projects;
mod repositories;
mod reproduce;
mod runs;
mod stopping;
mod whereabouts;
pub mod workflows;

pub use packages::{
    Available, Offered, PackageInfo, PackageView, UPDATE_ITEM, UpdateFailure, Updates,
};

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use moebius_plugin_api::Host;

use crate::board::Runs;
use crate::bus::{Bus, Event};
use crate::config::{LocalizeConfig, PackagesConfig};
use crate::decide::{Actor, Decisions, Question};
use crate::harness::repo::HarnessRepo;
use crate::harness::terminal::Slots;
use crate::repos::forge::{Forge, ForgeError, NoForge};
use crate::repos::{GitError, Repos};
use crate::store::{Card, Change, Repository, Run, Store, StoreError};

pub use attempts::{AttemptCheck, read_pick};
pub use automations::{
    AUTOMATION_ORIGIN, AutomationView, cron, is_automation, on_schedule as on_automation_schedule,
};
pub use board::{BoardFacts, CardFacts};
pub use cards::HistoryEntry;
pub use chats::{ASSISTANT_ORIGIN, CHAT_ORIGIN, Chat, ChatSettings};
pub(crate) use checks::tail;
pub(crate) use deletions::list;
pub use diffs::Diff;
pub(crate) use issues::now_millis;
pub use issues::{Child, IssueQuery, IssueView, state_of};
pub use reproduce::{Reproduction, TestRun, parse as parse_reproduction};
pub use whereabouts::{ClientInfo, OFFICIAL_REPOSITORY, ServerInfo};

/// The columns of the default workflow (spec 6).
pub mod columns {
    pub const BACKLOG: &str = "Backlog";
    pub const NOT_STARTED: &str = "Not started";
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

/// The author of issues a person created in the web app; their cards are trusted.
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
    #[error("{0}")]
    Forbidden(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

pub use stopping::CardStopWaits;

/// Held by the thread of a research while it goes (see [`App::researching`]).
pub(crate) struct Researching {
    app: Arc<App>,
    id: i64,
}

impl Drop for Researching {
    fn drop(&mut self) {
        self.app
            .researching
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

pub struct App {
    pub store: Arc<Store>,
    pub bus: Bus,
    pub repos: Repos,
    pub decisions: Decisions,
    pub data_dir: PathBuf,
    /// How long a repository's own checks may run.
    pub checks_timeout: Duration,
    /// How long blocking a card waits for a run that is still starting to be stoppable, and how long deleting one waits
    /// for its runs to end (see `stopping`).
    pub card_stop_waits: CardStopWaits,
    /// Whether code runs are given a hint of the files that are likely relevant, and how many (see `localize`).
    pub localize: LocalizeConfig,
    /// Where Pi packages are found and fetched from (see `packages`).
    pub packages: PackagesConfig,
    /// The checks going now, as change and commit.
    checking: Mutex<BTreeSet<(i64, String)>>,
    /// The attempts being judged now (see `attempts`).
    judging: Mutex<BTreeSet<i64>>,
    /// The repositories already told to move their `.moebius/` folder (see `projects`).
    moved_told: Mutex<BTreeSet<i64>>,
    /// The runs of reproduction tests, going or ended (see `reproduce`).
    tests: Mutex<HashMap<reproduce::TestKey, TestRun>>,
    /// Whether the schedule's replays are going now (see `replay::start_due`).
    pub replaying: AtomicBool,
    /// Held while a run event is stored and published, so that events are published in the order of their `seq`.
    run_events: Mutex<()>,
    /// Held while a message is given to a chat's agent or a chat's work is committed to be offered, so that the agent
    /// does not start on a message while its worktree is committed.
    chat_turns: Mutex<()>,
    /// The chats being woken: reopened, their agent not yet registered with the harness (see `wake_chat`). Entered
    /// under `chat_turns`, so that a message, a stop or an end arriving meanwhile can tell.
    waking: Mutex<BTreeSet<i64>>,
    /// The research whose thread is going (see `research::start`): a research is deleted only once its thread is done.
    researching: Mutex<BTreeSet<i64>>,
    /// The terminals open on each run.
    pub terminals: Arc<Slots>,
    /// Why each waiting card waits, as its column script last said (`moebius.wait`, `ask`, `stuck`). Not stored: a
    /// reason is rebuilt by the next tick after a restart.
    waiting: Mutex<HashMap<i64, String>>,
    /// The workflow runs waiting for a person's decision: the decision the resume route sends to the thread that waits
    /// for it (see `workflows::approval`).
    workflow_decisions:
        Mutex<HashMap<i64, std::sync::mpsc::Sender<crate::workflows::nodes::approval::Decision>>>,
    /// The workflow runs a thread drives now, so that a decision at the edge of a wait cannot start a second drive
    /// of the same run (see `workflows::run`).
    workflow_drives: Mutex<BTreeSet<i64>>,
    /// The cards whose forge refused to close their issue for a script, with when and what it said: not asked again for
    /// a while (see `issues`). Not stored.
    close_refused: Mutex<HashMap<i64, (std::time::Instant, String)>>,
    /// The cards whose forge refused a label for a script, with when: not asked again for `close_retry`. Not stored.
    label_refused: Mutex<HashMap<i64, std::time::Instant>>,
    /// The cancellation flag of each workflow run: set when a person cancels the run and looked at by the thread of
    /// the run between nodes and while its node goes (see `workflows::stopping`).
    workflow_cancels: Mutex<HashMap<i64, Arc<AtomicBool>>>,
    /// How long a forge that refused to close an issue, or a label, is left alone.
    close_retry: Duration,
    forges: BTreeMap<&'static str, Box<dyn Forge>>,
    /// The harness configuration repository's clone, once the server has opened it.
    harness_repo: OnceLock<Arc<HarnessRepo>>,
    /// What starts the chats of the server, once it has a harness.
    runs: OnceLock<Arc<dyn Runs>>,
    /// Where the server and the web app run (see `whereabouts`).
    whereabouts: whereabouts::Whereabouts,
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
            card_stop_waits: CardStopWaits::default(),
            localize: LocalizeConfig::default(),
            packages: PackagesConfig::default(),
            checking: Mutex::default(),
            judging: Mutex::default(),
            moved_told: Mutex::default(),
            tests: Mutex::default(),
            replaying: AtomicBool::new(false),
            run_events: Mutex::default(),
            chat_turns: Mutex::default(),
            waking: Mutex::default(),
            researching: Mutex::default(),
            terminals: Arc::default(),
            waiting: Mutex::default(),
            workflow_decisions: Mutex::default(),
            workflow_drives: Mutex::default(),
            close_refused: Mutex::default(),
            label_refused: Mutex::default(),
            workflow_cancels: Mutex::default(),
            close_retry: Duration::from_secs(600),
            forges,
            harness_repo: OnceLock::new(),
            runs: OnceLock::new(),
            whereabouts: Default::default(),
        }
    }

    /// Notes that the thread of the research `id` is going, or that it is being deleted; the returned guard says it is
    /// done when dropped. None when something already holds the research, which is how a start and a delete exclude
    /// each other.
    pub(crate) fn researching(self: &Arc<Self>, id: i64) -> Option<Researching> {
        self.researching
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id)
            .then(|| Researching {
                app: self.clone(),
                id,
            })
    }

    /// Tells the core which clone is the harness configuration repository, where chats can work, and gives the
    /// automations saved before it existed their workflow files.
    pub fn set_harness_repo(&self, repo: Arc<HarnessRepo>) {
        let _ = self.harness_repo.set(repo);
        if let Err(error) = self.migrate_automations() {
            eprintln!("Moebius cannot migrate the automations to workflows: {error}");
        }
    }

    /// Tells the core what starts chats, so that a workflow's `chat` node can start one.
    pub fn set_runs(&self, runs: Arc<dyn Runs>) {
        let _ = self.runs.set(runs);
    }

    /// What starts chats; refused when the server has not set it.
    pub(crate) fn runs(&self) -> Result<&Arc<dyn Runs>> {
        self.runs
            .get()
            .ok_or_else(|| AppError::Invalid("the server has no harness to start a chat".into()))
    }

    /// The harness configuration repository; refused when the server has none.
    pub(crate) fn harness_repo(&self) -> Result<&Arc<HarnessRepo>> {
        self.harness_repo.get().ok_or_else(|| {
            AppError::Invalid(
                "the server has no harness configuration repository ([harness])".into(),
            )
        })
    }

    /// Makes `forge` available to the repositories that name it.
    pub fn add_forge(&mut self, forge: Box<dyn Forge>) {
        self.forges.insert(forge.name(), forge);
    }

    /// The HTTP header git needs to reach `url` when it is a repository on the GitHub web host (github.com or the
    /// Enterprise host) and the GitHub App or an access token is set up, or one on the GitLab or the Forgejo that is set up; none otherwise, so a token never goes to another host.
    pub fn git_auth(&self, url: &str) -> Result<Option<String>> {
        for name in ["gitlab", "forgejo"] {
            if let Some(forge) = self.forges.get(name)
                && let Some(header) = forge.auth(url, "")?
            {
                return Ok(Some(header));
            }
        }
        let (Some(forge), Some((_, path))) = (
            self.forges.get("github"),
            url.strip_prefix("https://")
                .and_then(|rest| rest.split_once('/')),
        ) else {
            return Ok(None);
        };
        // The forge sends the token only when the address is on its own host.
        let name = path.trim_end_matches('/').trim_end_matches(".git");
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

    /// Records what a person did: `kind` names both the decision and the option they took; the other was to leave
    /// things as they were.
    pub(crate) fn record_action(
        &self,
        by: &Actor,
        kind: &str,
        card_id: Option<i64>,
        run_id: Option<i64>,
        context: BTreeMap<String, String>,
        reason: &str,
    ) -> Result<()> {
        self.decisions.record_by(
            by,
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

    /// Stores an event of a run and publishes it as `run_event` with the `seq` it was stored under.
    pub fn record_run_event(
        &self,
        run_id: i64,
        kind: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        let _ordered = self.run_events.lock().unwrap_or_else(|e| e.into_inner());
        // Pi's records can carry what a tool printed; nothing unredacted is stored or streamed. The store redacts
        // once and gives back what it stored.
        let (seq, data) = self.store.append_redacted_event(run_id, kind, data)?;
        self.bus.publish(Event::RunEvent {
            run: run_id,
            seq,
            kind: kind.to_string(),
            data,
        });
        Ok(())
    }
}
