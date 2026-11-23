//! The core put together: the store, the event bus, the repositories, the forges and the decision points, and the
//! operations the desktop API and the board engine perform on them. Every change to a card is published on the bus.
//!
//! This module holds `App` itself, its errors and the lookups every operation shares; the operations are grouped by
//! what they act on: `repositories` adds and edits repositories, `cards` keeps cards in line with their issues and
//! moves them, `issues` is the issue tracker (parents, blocking, labels, claims, comments, closing), `issues` is the issue tracker (parents, blocking, labels, claims, comments, closing), `snapshot` reads for every card at once what the snapshot shows of each, `stopping` holds, releases and deletes a card, `runs` is a person's hand in a running run (steering,
//! stopping, shells), `chats` starts and carries on chats, `changes` publishes, offers and merges changes, `checks`
//! runs a repository's own checks on them, `packages` proposes installing, updating and removing Pi packages as changes
//! of the harness configuration repository, `localize` gives a code run the files its issue is likely to touch and
//! records what came of it, `reproduce` runs a change's reproduction test for a repository that works test first,
//! `projects` reads a project's settings from the harness configuration repository,
//! `analytics` reads throughput, times, burn-ups and tokens per point from the moves and runs within what a person may see,
//! `cycles` keeps a project's cycles and carries unfinished cards over when one closes,
//! `views` keeps saved views and works out the cards each shows a person,
//! `milestones` makes milestones and counts their cards within what a person may see,
//! `whereabouts` knows where the server and the app run, `automations` keeps the automations and starts their chats
//! when their schedules come due, and `plugin_settings` keeps what plugins ask of each repository and of the
//! server.

mod analytics;
mod assignees;
mod attempts;
mod automations;
mod board;
mod card_repositories;
mod cards;
mod changes;
mod chat_access;
mod chats;
mod checks;
mod cycles;
mod deletions;
mod diffs;
mod intake;
mod issues;
mod jira;
mod kinds;
mod localize;
mod milestones;
mod notices;
mod packages;
mod pausing;
mod planning;
pub mod plugin_settings;
mod projects;
mod repositories;
mod reproduce;
mod runs;
mod snapshot;
mod stopping;
mod templates;
mod view_filters;
mod views;
mod whereabouts;
pub mod workflows;

pub use notices::handles_in;
pub use packages::{
    Available, Offered, PackageInfo, PackageView, UPDATE_ITEM, UpdateFailure, Updates,
};

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
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
pub use changes::CardMerge;
pub use chat_access::{ChatScope, may_use};
pub use chats::{ASSISTANT_ORIGIN, CHAT_ORIGIN, Chat, ChatSettings};
pub(crate) use checks::tail;
pub(crate) use deletions::list;
pub use diffs::Diff;
pub(crate) use issues::now_millis;
pub use issues::{Child, IssueQuery, IssueView, Links, Similar, state_of};
pub use planning::{Compared, EstimateProposal};
pub use reproduce::{Reproduction, TestRun, parse as parse_reproduction};
pub use runs::{FinishedCard, FinishedRun};
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

/// The bare author of records from before people had identities. The migration gave them to the first owner, and no new
/// record is made with it; it is only recognised ([`is_person`]).
pub const PERSON: &str = "person";

/// The author of what the first owner makes: `person:owner`. Tests and made-up cards use it.
pub const OWNER_AUTHOR: &str = "person:owner";

/// The author or decider of a person's work: `person:<handle>`.
pub fn person_author(handle: &str) -> String {
    format!("{PERSON}:{handle}")
}

/// Whether `author` (or a decider) is a person of the organisation: `person`, or `person:<handle>` for a known one. A
/// forge login cannot hold a `:`, so no forge user is taken for one.
pub fn is_person(author: &str) -> bool {
    author == PERSON
        || author
            .strip_prefix(PERSON)
            .and_then(|rest| rest.strip_prefix(':'))
            .is_some_and(|handle| !handle.is_empty())
}

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
    /// A wiki page changed after the version a person based their save on: what is there now, `None` when the page is
    /// gone. Nothing was written.
    #[error("someone saved a newer version of {path} since you opened it")]
    Stale {
        path: String,
        current: Option<Box<crate::wiki::PageView>>,
    },
}

pub type Result<T> = std::result::Result<T, AppError>;

pub use analytics::{
    ANALYTICS_DEFAULT_DAYS, Analytics, AnalyticsQuery, AnalyticsTotals, BurnUp, ColumnTime,
    ProjectFigures, Spread, ThroughputDay,
};
pub use cycles::{BurnUpDay, Carry, Closed, CycleDetail, CycleFields, CycleFigures, CycleView};
pub use jira::{
    Counts, LinkCounts, Options as JiraOptions, Report as JiraReport, Users as JiraUsers,
};
pub use kinds::{CardQuery, KindView};
pub use milestones::{MilestoneDetail, MilestoneFields, MilestoneProject, MilestoneView};
pub use projects::{Member, ProjectRepository, ProjectView, Sight, UNQUOTED};
pub use snapshot::CardLookups;
pub use stopping::CardStopWaits;
pub use templates::{IssueTemplateView, PageTemplateView, Templates};
pub use views::{ViewCard, ViewCards, ViewEdit, ViewFields};

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
    /// The wiki's repository, once the server has opened it; it is replaced when a person connects one that the
    /// server could not open at start.
    wiki: RwLock<Option<Arc<crate::wiki::Wiki>>>,
    /// Why the server has no wiki, when it could not open it at start.
    wiki_problem: Mutex<Option<String>>,
    /// What starts the chats of the server, once it has a harness.
    runs: OnceLock<Arc<dyn Runs>>,
    /// Where the server and the web app run (see `whereabouts`).
    whereabouts: whereabouts::Whereabouts,
}

impl App {
    /// The author of what `by` makes on a person's behalf: the person's own `person:<handle>`, and for an agent or a
    /// script the first owner's, since they act for the organisation's person.
    pub fn author_of(&self, by: &crate::decide::Actor) -> String {
        if by.is_person() {
            return by.decider();
        }
        self.owner_author()
    }

    /// The author of what the first owner makes: `person:owner`, the migration's first owner being the person the
    /// records from before belong to.
    pub fn owner_author(&self) -> String {
        match self.store.person_by_handle(crate::store::FIRST_OWNER) {
            Ok(Some(owner)) => owner.author(),
            _ => OWNER_AUTHOR.to_string(),
        }
    }

    /// The core over `store`, with its repositories under `data_dir` and the bundled forges.
    pub fn new(store: Arc<Store>, host: Arc<Host>, data_dir: &Path) -> Self {
        let repos = Repos::new(data_dir);
        let mut forges: BTreeMap<&'static str, Box<dyn Forge>> = BTreeMap::new();
        let none = NoForge::new(repos.clone());
        forges.insert(none.name(), Box::new(none));
        let bus = Bus::new();
        // A run's events are published once they are committed, in the order of their `seq`, whoever stored them.
        let events_bus = bus.clone();
        store.observe_events(move |events| {
            for e in events {
                events_bus.publish(Event::RunEvent {
                    run: e.run_id,
                    seq: e.seq,
                    kind: e.kind.clone(),
                    data: e.data.clone(),
                });
            }
        });
        Self {
            decisions: Decisions::new(store.clone(), host),
            store,
            bus,
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
            wiki: RwLock::default(),
            wiki_problem: Mutex::default(),
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

    /// Tells the core which clone is the harness configuration repository, where chats can work, gives the
    /// automations saved before it existed their workflow files, moves the server-wide workflow files and the rows
    /// that belong to them to their projects (see `migrate_workflows_to_projects`), and moves the column scripts of
    /// the repositories' settings to their projects.
    pub fn set_harness_repo(&self, repo: Arc<HarnessRepo>) {
        let _ = self.harness_repo.set(repo);
        // A start after a crash between the move of the workflow files and the store step finishes the step first, so
        // that an automation whose file is in its project is not given a second one.
        if let Err(error) = self.settle_workflow_rows() {
            eprintln!("Moebius cannot finish moving the workflows to their projects: {error}");
        }
        if let Err(error) = self.migrate_automations() {
            eprintln!("Moebius cannot migrate the automations to workflows: {error}");
        }
        self.migrate_workflows_to_projects();
        self.move_scripts_to_projects();
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

    /// Tells the core which repository is the wiki, which clears what it said about having none.
    pub fn set_wiki(&self, wiki: Arc<crate::wiki::Wiki>) {
        *self.wiki.write().unwrap_or_else(|e| e.into_inner()) = Some(wiki);
        *self.wiki_problem.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Takes the wiki away again, for `problem`: what undoes opening it when connecting a repository failed.
    pub fn clear_wiki(&self, problem: Option<String>) {
        *self.wiki.write().unwrap_or_else(|e| e.into_inner()) = None;
        *self.wiki_problem.lock().unwrap_or_else(|e| e.into_inner()) = problem;
    }

    /// Tells the core why the server has no wiki.
    pub fn set_wiki_problem(&self, problem: String) {
        *self.wiki_problem.lock().unwrap_or_else(|e| e.into_inner()) = Some(problem);
    }

    /// The wiki; a conflict, with the reason, when the server could not open it.
    pub fn wiki(&self) -> Result<Arc<crate::wiki::Wiki>> {
        let wiki = self.wiki.read().unwrap_or_else(|e| e.into_inner()).clone();
        wiki.ok_or_else(|| {
            let problem = self.wiki_problem.lock().unwrap_or_else(|e| e.into_inner());
            AppError::Conflict(match &*problem {
                Some(problem) => format!("the server has no wiki: {problem}"),
                None => "the server has no wiki".into(),
            })
        })
    }

    /// How the wiki is kept: on this server only, in a git repository too, or not at all.
    pub fn wiki_status(&self) -> crate::wiki::Status {
        let wiki = self.wiki.read().unwrap_or_else(|e| e.into_inner()).clone();
        let problem = self
            .wiki_problem
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        crate::wiki::Status::of(wiki.as_deref(), problem)
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

    /// The card a name such as `WEB-24` (a project's key and the card's number in it) names; `404` when there is none and
    /// `422` when the text is not such a name.
    pub fn card_named(&self, name: &str) -> Result<Card> {
        let (key, number) = crate::store::parse_card_key(name).ok_or_else(|| {
            AppError::Invalid(format!("{name:?} is not a card key such as WEB-24"))
        })?;
        self.store
            .card_by_key(&key, number)?
            .ok_or_else(|| AppError::NotFound(format!("card {key}-{number}")))
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

    /// Stores an event of a run and publishes it as `run_event` with the `seq` it was stored under, once it is
    /// committed. A streamed delta (`message_update`) waits a moment to be stored with the others (see
    /// `Store::queue_delta`); any other event is stored, with the deltas before it, before this returns.
    pub fn record_run_event(
        &self,
        run_id: i64,
        kind: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        // Pi's records can carry what a tool printed; nothing unredacted is stored or streamed: the store redacts
        // once, and what it publishes is what it stored.
        if kind == "message_update" {
            self.store.queue_delta(run_id, data)?;
        } else {
            self.store.append_redacted_event(run_id, kind, data)?;
        }
        Ok(())
    }
}
