//! The store: records in SQLite and large content in the blob store (see `docs/spec.md`, section 3.1).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::blobs::{BlobId, BlobStore};

/// Why the store failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("blob store: {0}")]
    Io(#[from] std::io::Error),
    #[error("stored JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Each entry migrates the schema from the version before it; the schema's version is `PRAGMA user_version`.
const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE repositories (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    path TEXT NOT NULL,
    forge TEXT NOT NULL,
    default_branch TEXT NOT NULL
);
CREATE TABLE cards (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    number INTEGER NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    author TEXT NOT NULL,
    column_name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (repository_id, number)
);
CREATE TABLE changes (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    card_id INTEGER REFERENCES cards(id),
    branch TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE runs (
    id INTEGER PRIMARY KEY,
    card_id INTEGER REFERENCES cards(id),
    agent TEXT NOT NULL,
    harness TEXT NOT NULL,
    purpose TEXT NOT NULL,
    status TEXT NOT NULL,
    worktree TEXT,
    started_at INTEGER NOT NULL,
    ended_at INTEGER
);
CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES runs(id),
    seq INTEGER NOT NULL,
    kind TEXT NOT NULL,
    data TEXT NOT NULL,
    at INTEGER NOT NULL,
    UNIQUE (run_id, seq)
);
CREATE TABLE judgements (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    context TEXT NOT NULL,
    options TEXT NOT NULL,
    choice TEXT NOT NULL,
    reason TEXT NOT NULL,
    decider TEXT NOT NULL,
    run_id INTEGER REFERENCES runs(id),
    card_id INTEGER REFERENCES cards(id),
    outcome TEXT,
    at INTEGER NOT NULL
);
CREATE TABLE inbox_items (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    card_id INTEGER REFERENCES cards(id),
    title TEXT NOT NULL,
    data TEXT NOT NULL,
    resolved_at INTEGER,
    at INTEGER NOT NULL
);
CREATE TABLE usage (
    id INTEGER PRIMARY KEY,
    run_id INTEGER REFERENCES runs(id),
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cached_tokens INTEGER NOT NULL,
    at INTEGER NOT NULL
);
CREATE INDEX events_by_run ON events (run_id, seq);
CREATE INDEX judgements_by_kind ON judgements (kind, at);
CREATE INDEX usage_by_run ON usage (run_id);
"#,
    r#"
ALTER TABLE repositories ADD COLUMN github TEXT;
ALTER TABLE repositories ADD COLUMN trusted TEXT NOT NULL DEFAULT '[]';
CREATE TABLE dependencies (
    card_id INTEGER NOT NULL REFERENCES cards(id),
    blocked_by INTEGER NOT NULL,
    PRIMARY KEY (card_id, blocked_by)
);
"#,
    r#"
ALTER TABLE changes ADD COLUMN pull_request INTEGER;
ALTER TABLE changes ADD COLUMN checks TEXT;
ALTER TABLE changes ADD COLUMN review TEXT;
"#,
    "ALTER TABLE runs ADD COLUMN summary TEXT;",
    r#"
ALTER TABLE cards ADD COLUMN note TEXT;
ALTER TABLE changes ADD COLUMN head TEXT;
ALTER TABLE changes ADD COLUMN published_at INTEGER;
ALTER TABLE changes ADD COLUMN checks_head TEXT;
ALTER TABLE changes ADD COLUMN failed_checks TEXT;
ALTER TABLE changes ADD COLUMN check_output TEXT;
"#,
    "ALTER TABLE cards ADD COLUMN retried_after INTEGER;",
    r#"
CREATE TABLE card_moves (
    id INTEGER PRIMARY KEY,
    card_id INTEGER NOT NULL REFERENCES cards(id),
    from_column TEXT NOT NULL,
    to_column TEXT NOT NULL,
    note TEXT,
    at INTEGER NOT NULL
);
CREATE INDEX card_moves_by_card ON card_moves (card_id, id);
"#,
    "ALTER TABLE runs ADD COLUMN repository_id INTEGER REFERENCES repositories(id);",
    "ALTER TABLE usage ADD COLUMN duration_ms INTEGER NOT NULL DEFAULT 0;",
    r#"
CREATE TABLE retros (
    id INTEGER PRIMARY KEY,
    date TEXT NOT NULL UNIQUE,
    report TEXT NOT NULL,
    status TEXT NOT NULL,
    run_id INTEGER REFERENCES runs(id),
    warnings TEXT NOT NULL DEFAULT '[]',
    at INTEGER NOT NULL
);
CREATE TABLE proposals (
    id INTEGER PRIMARY KEY,
    retro_date TEXT NOT NULL REFERENCES retros(date),
    branch TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    agents TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'open',
    at INTEGER NOT NULL
);
CREATE INDEX proposals_by_retro ON proposals (retro_date);
"#,
    r#"
ALTER TABLE proposals ADD COLUMN cards TEXT NOT NULL DEFAULT '[]';
CREATE TABLE trials (
    id INTEGER PRIMARY KEY,
    proposal_id INTEGER NOT NULL REFERENCES proposals(id),
    card_id INTEGER,
    run_id INTEGER REFERENCES runs(id),
    outcome TEXT NOT NULL,
    reason TEXT NOT NULL,
    measurements TEXT NOT NULL DEFAULT '{}',
    at INTEGER NOT NULL
);
CREATE INDEX trials_by_proposal ON trials (proposal_id);
"#,
    r#"
CREATE TABLE replays (
    id INTEGER PRIMARY KEY,
    judgement_id INTEGER NOT NULL UNIQUE REFERENCES judgements(id),
    run_id INTEGER NOT NULL REFERENCES runs(id),
    exit INTEGER,
    timed_out INTEGER NOT NULL,
    output TEXT NOT NULL,
    created TEXT NOT NULL,
    changed TEXT NOT NULL,
    deleted TEXT NOT NULL,
    at INTEGER NOT NULL
);
CREATE INDEX replays_by_run ON replays (run_id);
"#,
    r#"
CREATE TABLE research (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    goal TEXT NOT NULL,
    measure TEXT NOT NULL,
    better TEXT NOT NULL,
    token_budget INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    baseline REAL,
    reason TEXT NOT NULL DEFAULT '',
    at INTEGER NOT NULL
);
CREATE TABLE research_runs (
    research_id INTEGER NOT NULL REFERENCES research(id),
    run_id INTEGER NOT NULL REFERENCES runs(id),
    PRIMARY KEY (research_id, run_id)
);
CREATE TABLE experiments (
    id INTEGER PRIMARY KEY,
    research_id INTEGER NOT NULL REFERENCES research(id),
    branch TEXT NOT NULL,
    title TEXT NOT NULL,
    value REAL,
    checks INTEGER,
    kept INTEGER NOT NULL,
    reason TEXT NOT NULL,
    change_id INTEGER REFERENCES changes(id),
    at INTEGER NOT NULL
);
CREATE INDEX experiments_by_research ON experiments (research_id, id);
"#,
    r#"
ALTER TABLE cards ADD COLUMN confirmed INTEGER NOT NULL DEFAULT 0;
CREATE TABLE grillings (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES runs(id),
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    issues TEXT NOT NULL,
    report TEXT,
    status TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    at INTEGER NOT NULL
);
"#,
    r#"
CREATE TABLE shadows (
    id INTEGER PRIMARY KEY,
    judgement_id INTEGER NOT NULL UNIQUE REFERENCES judgements(id),
    kind TEXT NOT NULL,
    choice TEXT,
    reason TEXT NOT NULL,
    failed INTEGER NOT NULL,
    at INTEGER NOT NULL
);
CREATE INDEX shadows_by_kind ON shadows (kind, id);
CREATE TABLE takeovers (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    agreement REAL NOT NULL,
    count INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    at INTEGER NOT NULL
);
"#,
    r#"
ALTER TABLE runs ADD COLUMN seen_seq INTEGER NOT NULL DEFAULT 0;
"#,
    "ALTER TABLE usage ADD COLUMN tools TEXT;",
    r#"
PRAGMA defer_foreign_keys = ON;
CREATE TEMP TABLE changes_old AS SELECT * FROM changes;
DROP TABLE changes;
CREATE TABLE changes (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER REFERENCES repositories(id),
    card_id INTEGER REFERENCES cards(id),
    branch TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    pull_request INTEGER,
    checks TEXT,
    review TEXT,
    head TEXT,
    published_at INTEGER,
    checks_head TEXT,
    failed_checks TEXT,
    check_output TEXT,
    harness_repository INTEGER NOT NULL DEFAULT 0
);
INSERT INTO changes (id, repository_id, card_id, branch, state, created_at, pull_request, checks, review, head,
    published_at, checks_head, failed_checks, check_output)
SELECT id, repository_id, card_id, branch, state, created_at, pull_request, checks, review, head,
    published_at, checks_head, failed_checks, check_output FROM changes_old;
DROP TABLE changes_old;
ALTER TABLE runs ADD COLUMN harness_repository INTEGER NOT NULL DEFAULT 0;
"#,
];

const CHANGE_COLUMNS: &str = "id, repository_id, card_id, branch, state, pull_request, checks, review, \
     head, published_at, checks_head, failed_checks, check_output, harness_repository";

fn change_from_row(r: &rusqlite::Row) -> rusqlite::Result<Change> {
    Ok(Change {
        id: r.get(0)?,
        repository_id: r.get(1)?,
        card_id: r.get(2)?,
        branch: r.get(3)?,
        state: r.get(4)?,
        pull_request: r.get(5)?,
        checks: r.get(6)?,
        review: r.get(7)?,
        head: r.get(8)?,
        published_at: r.get(9)?,
        checks_head: r.get(10)?,
        failed_checks: r.get(11)?,
        check_output: r.get(12)?,
        harness_repository: r.get(13)?,
    })
}

/// The tool names of a `usage.tools` column; none for a row recorded before the column or one that does not read.
fn tools_from_column(column: Option<String>) -> Vec<String> {
    column
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub forge: String,
    pub default_branch: String,
    /// The repository's name on GitHub, as `owner/repo`; none for other forges.
    pub github: Option<String>,
    /// The GitHub logins whose issues move on without a person's approval.
    pub trusted: Vec<String>,
}

impl Repository {
    /// Where new work starts and scripts are read from: the default branch as the forge has it. For GitHub that is
    /// `origin`'s, which the poll keeps up to date; without a forge it is the clone's own.
    pub fn base_ref(&self) -> String {
        match self.forge.as_str() {
            "none" => self.default_branch.clone(),
            _ => format!("origin/{}", self.default_branch),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub id: i64,
    pub repository_id: i64,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub author: String,
    pub column: String,
    pub created_at: i64,
    /// Why the card went back to coding, for the next `code` run; cleared when a `code` run succeeds.
    pub note: Option<String>,
    /// The id of the card's latest run when a person last retried it; its rounds, reviews and tokens count only the
    /// runs after it.
    pub retried_after: Option<i64>,
    /// Whether the card was made from a grilling a person confirmed, which needs no approval.
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub id: i64,
    /// The repository the change belongs to; none for a change of the harness configuration repository.
    pub repository_id: Option<i64>,
    pub card_id: Option<i64>,
    pub branch: String,
    pub state: String,
    /// The number of the change's pull request on GitHub.
    pub pull_request: Option<i64>,
    /// `pending`, `success` or `failure` once the checks are known, for the commit `checks_head`.
    pub checks: Option<String>,
    /// The state of the pull request's latest review, such as `approved`, once there is one.
    pub review: Option<String>,
    /// The commit of `branch` that was last published.
    pub head: Option<String>,
    /// When `head` was published, in seconds since the epoch.
    pub published_at: Option<i64>,
    /// The commit `checks` were read or run for.
    pub checks_head: Option<String>,
    /// The failing checks of the pull request, one per line.
    pub failed_checks: Option<String>,
    /// The end of the output of the repository's own checks, secrets removed.
    pub check_output: Option<String>,
    /// Whether the change belongs to the harness configuration repository, which is no repository of Moebius.
    pub harness_repository: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: i64,
    pub card_id: Option<i64>,
    pub agent: String,
    pub harness: String,
    pub purpose: String,
    pub status: String,
    pub worktree: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    /// What the agent answered last, once the harness has reported it.
    pub summary: Option<String>,
    /// The repository a chat works in; none for a run of a card, and for a chat in an empty directory.
    pub repository_id: Option<i64>,
    /// Whether a chat works in the harness configuration repository.
    pub harness_repository: bool,
}

impl Run {
    /// The branch the run's work is committed on.
    pub fn branch(&self) -> String {
        let kind = if self.purpose == "chat" {
            "chat"
        } else {
            "run"
        };
        format!("moebius/{kind}-{}", self.id)
    }
}

/// A move of a card from one column to another, with the card's note at the time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardMove {
    pub from: String,
    pub to: String,
    pub note: Option<String>,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub run_id: i64,
    pub seq: i64,
    pub kind: String,
    pub data: serde_json::Value,
    pub at: i64,
}

/// One recorded choice at a decision point (ADR 0010).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Judgement {
    pub id: i64,
    pub kind: String,
    pub context: Vec<BlobId>,
    pub options: Vec<String>,
    pub choice: String,
    pub reason: String,
    pub decider: String,
    pub run_id: Option<i64>,
    pub card_id: Option<i64>,
    pub outcome: Option<String>,
    pub at: i64,
}

/// What a judgement is about, before it is recorded.
#[derive(Debug, Clone, Default)]
pub struct NewJudgement {
    pub kind: String,
    pub context: Vec<BlobId>,
    pub options: Vec<String>,
    pub choice: String,
    pub reason: String,
    pub decider: String,
    pub run_id: Option<i64>,
    pub card_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboxItem {
    pub id: i64,
    pub kind: String,
    pub card_id: Option<i64>,
    pub title: String,
    pub data: serde_json::Value,
    pub resolved_at: Option<i64>,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub run_id: Option<i64>,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    /// How long the model call took, from the request to the end of the response or stream.
    pub duration_ms: i64,
    /// The names of the tools the answer asked for, in order; empty for an answer that asks for none.
    pub tools: Vec<String>,
}

/// One thing that went wrong in a window of time, as the Retro collects it. `text` is whatever the source has to say, in
/// full; the report shortens it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub kind: &'static str,
    pub run_id: Option<i64>,
    pub card_id: Option<i64>,
    pub agent: Option<String>,
    pub text: String,
    pub at: i64,
}

/// A day's Retro: its report (see `retro`), how far it got and what the analyst left out.
#[derive(Debug, Clone, PartialEq)]
pub struct Retro {
    /// The local day it is about, as `YYYY-MM-DD`.
    pub date: String,
    pub report: serde_json::Value,
    /// `reported`, `analysing`, `proposed` or `failed`.
    pub status: String,
    /// The analyst's run, if there was one.
    pub run_id: Option<i64>,
    pub warnings: Vec<String>,
    pub at: i64,
}

/// A change to the harness configuration that a Retro proposes, on a branch of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: i64,
    pub retro_date: String,
    pub branch: String,
    /// The subject of the proposal's commit.
    pub title: String,
    /// The rest of the commit's message: what changes and which errors it addresses.
    pub body: String,
    /// The agents whose files the commit changes.
    pub agents: Vec<String>,
    /// The cards whose errors the proposal addresses, as the `Cards:` line ending its commit message names them.
    pub cards: Vec<i64>,
    /// `open`, `trying`, `positive`, `negative`, `untried`, `merged` or `dismissed`.
    pub status: String,
    /// The trials made of it, oldest first.
    pub trials: Vec<Trial>,
    pub at: i64,
}

/// A proposal tried on one card (see `trial`): how it went and what was measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trial {
    pub id: i64,
    pub proposal_id: i64,
    /// The card it was tried on; none for the one trial of a proposal that names no cards.
    pub card_id: Option<i64>,
    /// The trial's run, if one was started.
    pub run_id: Option<i64>,
    /// `positive`, `negative`, `skipped` or `failed`.
    pub outcome: String,
    pub reason: String,
    pub measurements: serde_json::Value,
    pub at: i64,
}

/// A research (see `research`): a brief a person confirms, and what became of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Research {
    pub id: i64,
    pub repository_id: i64,
    pub goal: String,
    /// The shell command that measures; its result is the last number it prints.
    pub measure: String,
    /// `lower` or `higher`: which way the measurement should move.
    pub better: String,
    /// The tokens its research runs may use, checked before each round.
    pub token_budget: i64,
    /// `draft`, `confirmed`, `running`, `done`, `spent`, `stopped` or `failed`.
    pub status: String,
    /// The measurement of the repository as it is; none until it was measured.
    pub baseline: Option<f64>,
    /// Why it ended, once it did.
    pub reason: String,
    pub at: i64,
}

/// One issue a grilling proposes: `blocked_by` are indexes into the list the issue is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrillingIssue {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub blocked_by: Vec<usize>,
}

/// A decision model's answer to a held-out judgement's question (see `decision_model`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewShadow {
    pub judgement_id: i64,
    pub kind: String,
    /// The model's choice; none when it failed.
    pub choice: Option<String>,
    /// The model's reason, or why it failed.
    pub reason: String,
    pub failed: bool,
}

/// How well the decision model agrees with the recorded choices of one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowStats {
    pub kind: String,
    pub shadows: i64,
    /// The shadows whose choice is the recorded one.
    pub agreed: i64,
    /// The shadows where the model failed; they count as disagreement.
    pub failed: i64,
}

impl ShadowStats {
    /// The share of the shadows that agreed; none before there is one.
    pub fn agreement(&self) -> Option<f64> {
        (self.shadows > 0).then(|| self.agreed as f64 / self.shadows as f64)
    }
}

/// A proposal that the decision model take over a kind of decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Takeover {
    pub id: i64,
    pub kind: String,
    /// The agreement measured when it was proposed, between 0 and 1.
    pub agreement: f64,
    /// How many shadows it was measured on.
    pub count: i64,
    /// `open`, `merged`, `dismissed` or `released`.
    pub status: String,
    pub at: i64,
}

/// What a grilling chat ended in (see `grilling`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grilling {
    pub id: i64,
    pub run_id: i64,
    pub repository_id: i64,
    /// The issues proposed; none for a report and for an empty grilling.
    pub issues: Vec<GrillingIssue>,
    /// The research report, for a grilling that ended in one.
    pub report: Option<String>,
    /// `proposed`, `reported`, `empty`, `confirmed` or `dismissed`.
    pub status: String,
    /// Why a grilling is `empty`.
    pub reason: String,
    pub at: i64,
}

/// One lever tried (see `research`): the measurement of its branch and whether it was kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Experiment {
    pub id: i64,
    pub research_id: i64,
    /// The lever's branch, `research/<research id>-<short name>`.
    pub branch: String,
    /// The subject of the lever's commit.
    pub title: String,
    /// What the measure printed on the branch; none when the measurement failed.
    pub value: Option<f64>,
    /// Whether the repository's own checks passed on the branch; none when it has none or the measurement failed.
    pub checks: Option<bool>,
    pub kept: bool,
    pub reason: String,
    /// The change offered for a kept lever.
    pub change_id: Option<i64>,
    pub at: i64,
}

/// A refused tool call run again in a throwaway copy of the worktree (see `replay`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replay {
    pub id: i64,
    /// The judgement that refused the call.
    pub judgement: i64,
    pub run: i64,
    /// The exit code; none when the call did not run, or was ended by a signal.
    pub exit: Option<i32>,
    pub timed_out: bool,
    /// What the call printed, or why it did not run; secrets removed and cut to a few KB.
    pub output: String,
    /// The files, relative to the worktree, that the call created, changed and deleted in the copy.
    pub created: Vec<String>,
    pub changed: Vec<String>,
    pub deleted: Vec<String>,
    pub at: i64,
}

/// The name under which usage that belongs to no run is counted by agent.
pub const NO_RUN: &str = "No run";

/// What one model used in a window of days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub tokens: i64,
    pub calls: i64,
    /// The input tokens among `tokens`, which `cached_tokens` is a share of.
    pub input_tokens: i64,
    pub cached_tokens: i64,
}

/// What the runs of one agent used in a window of days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentUsage {
    pub agent: String,
    pub calls: i64,
    pub tokens: i64,
    pub input_tokens: i64,
    pub cached_tokens: i64,
    pub model_ms: i64,
}

/// What the model did with its time in a window of days: one tool, answering, or something unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityUsage {
    /// A tool's name, [`ANSWERING`] or [`UNKNOWN`].
    pub activity: String,
    pub model_ms: i64,
    pub calls: i64,
    pub output_tokens: i64,
}

/// The activity of a model call that asked for no tool.
pub const ANSWERING: &str = "answering";
/// The activity of a model call recorded before the tools of calls were.
pub const UNKNOWN: &str = "unknown";

/// What a tool made runs wait for in a window of days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolWait {
    pub tool: String,
    pub calls: i64,
    pub seconds: i64,
}

/// All usage in a window of days: tokens, calls, input tokens, cached tokens and model time.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsageTotals {
    pub tokens: i64,
    pub calls: i64,
    pub input_tokens: i64,
    pub cached_tokens: i64,
    pub model_ms: i64,
}

/// The start of the local day that is `?1 - 1` days ago, as Unix seconds: the first moment of a window of `?1` days.
const WINDOW_START: &str = "CAST(strftime('%s', 'now', 'localtime', 'start of day', '-' || (?1 - 1) || ' days', 'utc') AS INTEGER)";

impl Usage {
    pub fn total(&self) -> i64 {
        self.input_tokens + self.output_tokens
    }
}

/// The store. Safe to share between threads; each call takes the connection for as long as it needs it.
pub struct Store {
    conn: Mutex<Connection>,
    blobs: BlobStore,
}

impl Store {
    /// Opens (or creates) the store in `data_dir` and brings its schema up to date.
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let conn = Connection::open(data_dir.join("moebius.sqlite"))?;
        Self::with(conn, BlobStore::new(data_dir.join("blobs")))
    }

    /// A store in memory, with blobs under `blob_dir`; for tests.
    pub fn in_memory(blob_dir: &Path) -> Result<Self> {
        Self::with(Connection::open_in_memory()?, BlobStore::new(blob_dir))
    }

    fn with(mut conn: Connection, blobs: BlobStore) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
            blobs,
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    pub fn schema_version(&self) -> Result<usize> {
        let version: i64 = self
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        Ok(version as usize)
    }

    // Repositories

    pub fn add_repository(
        &self,
        name: &str,
        path: &str,
        forge: &str,
        default_branch: &str,
    ) -> Result<Repository> {
        self.insert_repository(name, path, forge, default_branch, None, &[])
    }

    /// Adds a repository whose issues live on GitHub as `github` (`owner/repo`); `trusted` are the logins whose
    /// issues need no approval.
    pub fn add_github_repository(
        &self,
        name: &str,
        path: &str,
        default_branch: &str,
        github: &str,
        trusted: &[String],
    ) -> Result<Repository> {
        self.insert_repository(name, path, "github", default_branch, Some(github), trusted)
    }

    fn insert_repository(
        &self,
        name: &str,
        path: &str,
        forge: &str,
        default_branch: &str,
        github: Option<&str>,
        trusted: &[String],
    ) -> Result<Repository> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO repositories (name, path, forge, default_branch, github, trusted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                name,
                path,
                forge,
                default_branch,
                github,
                serde_json::to_string(trusted)?
            ],
        )?;
        Ok(Repository {
            id: conn.last_insert_rowid(),
            name: name.into(),
            path: path.into(),
            forge: forge.into(),
            default_branch: default_branch.into(),
            github: github.map(Into::into),
            trusted: trusted.to_vec(),
        })
    }

    pub fn repositories(&self) -> Result<Vec<Repository>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, path, forge, default_branch, github, trusted FROM repositories ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            let trusted: String = r.get(6)?;
            Ok(Repository {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
                forge: r.get(3)?,
                default_branch: r.get(4)?,
                github: r.get(5)?,
                trusted: serde_json::from_str(&trusted).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        e.into(),
                    )
                })?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn repository(&self, id: i64) -> Result<Option<Repository>> {
        Ok(self.repositories()?.into_iter().find(|r| r.id == id))
    }

    // Cards

    /// Adds a card with the next number in its repository.
    pub fn add_card(
        &self,
        repository_id: i64,
        title: &str,
        body: &str,
        author: &str,
        column: &str,
    ) -> Result<Card> {
        let conn = self.conn();
        let number: i64 = conn.query_row(
            "SELECT COALESCE(MAX(number), 0) + 1 FROM cards WHERE repository_id = ?1",
            [repository_id],
            |r| r.get(0),
        )?;
        Self::insert_card(&conn, repository_id, number, title, body, author, column)
    }

    /// Adds a card with the number of its issue on the forge.
    pub fn add_numbered_card(
        &self,
        repository_id: i64,
        number: i64,
        title: &str,
        body: &str,
        author: &str,
        column: &str,
    ) -> Result<Card> {
        let conn = self.conn();
        Self::insert_card(&conn, repository_id, number, title, body, author, column)
    }

    fn insert_card(
        conn: &Connection,
        repository_id: i64,
        number: i64,
        title: &str,
        body: &str,
        author: &str,
        column: &str,
    ) -> Result<Card> {
        let at = now();
        conn.execute(
            "INSERT INTO cards (repository_id, number, title, body, author, column_name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![repository_id, number, title, body, author, column, at],
        )?;
        Ok(Card {
            id: conn.last_insert_rowid(),
            repository_id,
            number,
            title: title.into(),
            body: body.into(),
            author: author.into(),
            column: column.into(),
            created_at: at,
            note: None,
            retried_after: None,
            confirmed: false,
        })
    }

    fn cards_where(&self, clause: &str, params: impl rusqlite::Params) -> Result<Vec<Card>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, repository_id, number, title, body, author, column_name, created_at, note, retried_after, confirmed FROM cards WHERE {clause} ORDER BY id"
        ))?;
        let rows = stmt.query_map(params, |r| {
            Ok(Card {
                id: r.get(0)?,
                repository_id: r.get(1)?,
                number: r.get(2)?,
                title: r.get(3)?,
                body: r.get(4)?,
                author: r.get(5)?,
                column: r.get(6)?,
                created_at: r.get(7)?,
                note: r.get(8)?,
                retried_after: r.get(9)?,
                confirmed: r.get(10)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn card(&self, id: i64) -> Result<Option<Card>> {
        Ok(self.cards_where("id = ?1", [id])?.pop())
    }

    pub fn cards(&self, repository_id: i64) -> Result<Vec<Card>> {
        self.cards_where("repository_id = ?1", [repository_id])
    }

    /// The card of issue `number` in a repository.
    pub fn card_by_number(&self, repository_id: i64, number: i64) -> Result<Option<Card>> {
        Ok(self
            .cards_where(
                "repository_id = ?1 AND number = ?2",
                [repository_id, number],
            )?
            .pop())
    }

    pub fn set_card_text(&self, id: i64, title: &str, body: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET title = ?1, body = ?2 WHERE id = ?3",
            params![title, body, id],
        )?;
        Ok(())
    }

    /// Adds a card made from a confirmed grilling, which needs no approval, with the numbers of the issues that block
    /// it, in one step so that no script sees it without them. `number` is the number of its issue on the forge; none
    /// takes the next number. A card the poll made for that issue already is confirmed instead.
    #[allow(clippy::too_many_arguments)]
    pub fn add_confirmed_card(
        &self,
        repository_id: i64,
        number: Option<i64>,
        title: &str,
        body: &str,
        author: &str,
        column: &str,
        blocked_by: &BTreeSet<i64>,
    ) -> Result<Card> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let number = match number {
            Some(number) => number,
            None => tx.query_row(
                "SELECT COALESCE(MAX(number), 0) + 1 FROM cards WHERE repository_id = ?1",
                [repository_id],
                |r| r.get(0),
            )?,
        };
        let id = match tx
            .query_row(
                "SELECT id FROM cards WHERE repository_id = ?1 AND number = ?2",
                [repository_id, number],
                |r| r.get(0),
            )
            .optional()?
        {
            Some(id) => id,
            None => Self::insert_card(&tx, repository_id, number, title, body, author, column)?.id,
        };
        tx.execute("UPDATE cards SET confirmed = 1 WHERE id = ?1", [id])?;
        tx.execute("DELETE FROM dependencies WHERE card_id = ?1", [id])?;
        for blocker in blocked_by {
            tx.execute(
                "INSERT INTO dependencies (card_id, blocked_by) VALUES (?1, ?2)",
                [id, *blocker],
            )?;
        }
        tx.commit()?;
        drop(conn);
        Ok(self.card(id)?.expect("the card was just stored"))
    }

    /// Replaces the numbers of the issues that block a card.
    pub fn set_dependencies(&self, card_id: i64, blocked_by: &BTreeSet<i64>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM dependencies WHERE card_id = ?1", [card_id])?;
        for number in blocked_by {
            tx.execute(
                "INSERT INTO dependencies (card_id, blocked_by) VALUES (?1, ?2)",
                [card_id, *number],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// The numbers of the issues that block a card, in order.
    pub fn dependencies(&self, card_id: i64) -> Result<Vec<i64>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT blocked_by FROM dependencies WHERE card_id = ?1 ORDER BY blocked_by",
        )?;
        let rows = stmt.query_map([card_id], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_card_note(&self, id: i64, note: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET note = ?1 WHERE id = ?2",
            params![note, id],
        )?;
        Ok(())
    }

    /// Gives the card a fresh start: the runs it has now no longer count toward its limits.
    pub fn set_card_retried(&self, id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET retried_after = (SELECT MAX(id) FROM runs WHERE card_id = ?1) WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    /// Moves a card and records the move, with the card's note as it is now.
    pub fn move_card(&self, id: i64, column: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO card_moves (card_id, from_column, to_column, note, at)
             SELECT id, column_name, ?1, note, ?2 FROM cards WHERE id = ?3",
            params![column, now(), id],
        )?;
        conn.execute(
            "UPDATE cards SET column_name = ?1 WHERE id = ?2",
            params![column, id],
        )?;
        Ok(())
    }

    /// The moves of a card since moves were recorded, oldest first.
    pub fn moves_for_card(&self, card_id: i64) -> Result<Vec<CardMove>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT from_column, to_column, note, at FROM card_moves WHERE card_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([card_id], |r| {
            Ok(CardMove {
                from: r.get(0)?,
                to: r.get(1)?,
                note: r.get(2)?,
                at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // Changes

    pub fn add_change(
        &self,
        repository_id: i64,
        card_id: Option<i64>,
        branch: &str,
        pull_request: Option<i64>,
        head: &str,
    ) -> Result<Change> {
        self.insert_change(Some(repository_id), card_id, branch, pull_request, head)
    }

    /// Adds a change of the harness configuration repository, which has no card and no pull request.
    pub fn add_harness_change(&self, branch: &str, head: &str) -> Result<Change> {
        self.insert_change(None, None, branch, None, head)
    }

    fn insert_change(
        &self,
        repository_id: Option<i64>,
        card_id: Option<i64>,
        branch: &str,
        pull_request: Option<i64>,
        head: &str,
    ) -> Result<Change> {
        let conn = self.conn();
        let at = now();
        let harness_repository = repository_id.is_none();
        conn.execute(
            "INSERT INTO changes (repository_id, card_id, branch, state, pull_request, head, published_at, created_at, \
             harness_repository) VALUES (?1, ?2, ?3, 'open', ?4, ?5, ?6, ?6, ?7)",
            params![repository_id, card_id, branch, pull_request, head, at, harness_repository],
        )?;
        Ok(Change {
            id: conn.last_insert_rowid(),
            repository_id,
            card_id,
            branch: branch.into(),
            state: "open".into(),
            pull_request,
            checks: None,
            review: None,
            head: Some(head.into()),
            published_at: Some(at),
            checks_head: None,
            failed_checks: None,
            check_output: None,
            harness_repository,
        })
    }

    /// The card's open change, if it has one.
    pub fn open_change(&self, card_id: i64) -> Result<Option<Change>> {
        Ok(self
            .conn()
            .query_row(
                &format!(
                    "SELECT {CHANGE_COLUMNS} FROM changes WHERE card_id = ?1 AND state = 'open' ORDER BY id DESC"
                ),
                [card_id],
                change_from_row,
            )
            .optional()?)
    }

    /// The latest change made of `branch`, open or not, if there is one.
    pub fn latest_change_on_branch(&self, branch: &str) -> Result<Option<Change>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {CHANGE_COLUMNS} FROM changes WHERE branch = ?1 ORDER BY id DESC"),
                [branch],
                change_from_row,
            )
            .optional()?)
    }

    /// Records that `head` of the change's branch was published just now.
    pub fn set_change_head(&self, id: i64, head: &str, published_at: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET head = ?1, published_at = ?2 WHERE id = ?3",
            params![head, published_at, id],
        )?;
        Ok(())
    }

    pub fn change(&self, id: i64) -> Result<Option<Change>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {CHANGE_COLUMNS} FROM changes WHERE id = ?1"),
                [id],
                change_from_row,
            )
            .optional()?)
    }

    /// The changes of a repository, oldest first.
    pub fn changes(&self, repository_id: i64) -> Result<Vec<Change>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {CHANGE_COLUMNS} FROM changes WHERE repository_id = ?1 ORDER BY id"
        ))?;
        let rows = stmt.query_map([repository_id], change_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The changes of the harness configuration repository, oldest first.
    pub fn harness_changes(&self) -> Result<Vec<Change>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {CHANGE_COLUMNS} FROM changes WHERE harness_repository = 1 ORDER BY id"
        ))?;
        let rows = stmt.query_map([], change_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The open changes of a repository that have a pull request, oldest first.
    pub fn open_pull_requests(&self, repository_id: i64) -> Result<Vec<Change>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {CHANGE_COLUMNS} FROM changes \
             WHERE repository_id = ?1 AND state = 'open' AND pull_request IS NOT NULL ORDER BY id"
        ))?;
        let rows = stmt.query_map([repository_id], change_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Records what the forge says about the change's pull request: checks of the commit `checks_head`, the failing
    /// ones, and the latest review.
    pub fn set_change_status(
        &self,
        id: i64,
        checks: Option<&str>,
        checks_head: &str,
        failed_checks: Option<&str>,
        review: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET checks = ?1, checks_head = ?2, failed_checks = ?3, review = ?4 WHERE id = ?5",
            params![checks, checks_head, failed_checks, review, id],
        )?;
        Ok(())
    }

    /// Records the repository's own checks of the commit `head`: how they stand and the end of their output.
    pub fn set_local_checks(
        &self,
        id: i64,
        checks: &str,
        head: &str,
        output: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET checks = ?1, checks_head = ?2, check_output = ?3 WHERE id = ?4",
            params![checks, head, output, id],
        )?;
        Ok(())
    }

    pub fn set_change_state(&self, id: i64, state: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET state = ?1 WHERE id = ?2",
            params![state, id],
        )?;
        Ok(())
    }

    // Runs and events

    pub fn add_run(
        &self,
        card_id: Option<i64>,
        agent: &str,
        harness: &str,
        purpose: &str,
    ) -> Result<Run> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO runs (card_id, agent, harness, purpose, status, started_at) VALUES (?1, ?2, ?3, ?4, 'starting', ?5)",
            params![card_id, agent, harness, purpose, at],
        )?;
        Ok(Run {
            id: conn.last_insert_rowid(),
            card_id,
            agent: agent.into(),
            harness: harness.into(),
            purpose: purpose.into(),
            status: "starting".into(),
            worktree: None,
            started_at: at,
            ended_at: None,
            summary: None,
            repository_id: None,
            harness_repository: false,
        })
    }

    fn runs_where(&self, clause: &str, param: rusqlite::types::Value) -> Result<Vec<Run>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, card_id, agent, harness, purpose, status, worktree, started_at, ended_at, summary, repository_id,
                harness_repository
             FROM runs WHERE {clause} ORDER BY id"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok(Run {
                id: r.get(0)?,
                card_id: r.get(1)?,
                agent: r.get(2)?,
                harness: r.get(3)?,
                purpose: r.get(4)?,
                status: r.get(5)?,
                worktree: r.get(6)?,
                started_at: r.get(7)?,
                ended_at: r.get(8)?,
                summary: r.get(9)?,
                repository_id: r.get(10)?,
                harness_repository: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn run(&self, id: i64) -> Result<Option<Run>> {
        Ok(self.runs_where("id = ?1", id.into())?.pop())
    }

    /// The runs of a card, oldest first. Its trials are not among them: they are no part of the card's work.
    pub fn runs_for_card(&self, card_id: i64) -> Result<Vec<Run>> {
        self.runs_where("card_id = ?1 AND purpose <> 'trial'", card_id.into())
    }

    /// The chats, newest first.
    pub fn chats(&self) -> Result<Vec<Run>> {
        let mut chats = self.runs_where("purpose = ?1", "chat".to_string().into())?;
        chats.reverse();
        Ok(chats)
    }

    /// How far a person has read the run: the `seq` of the last of its events they have seen.
    pub fn seen_seq(&self, run_id: i64) -> Result<i64> {
        Ok(self
            .conn()
            .query_row("SELECT seen_seq FROM runs WHERE id = ?1", [run_id], |r| {
                r.get(0)
            })?)
    }

    /// A person has read the run up to the event `seq`; reading never goes back.
    pub fn set_seen_seq(&self, run_id: i64, seq: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE runs SET seen_seq = MAX(seen_seq, ?2) WHERE id = ?1",
            params![run_id, seq],
        )?;
        Ok(())
    }

    /// The `seq` of the last event of the run that a person has to read: an answer that settled, or an error. Zero
    /// without one.
    pub fn answered_seq(&self, run_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM events
             WHERE run_id = ?1 AND kind IN ('agent_settled', 'moebius_error')",
            [run_id],
            |r| r.get(0),
        )?)
    }

    /// The runs that have not ended.
    pub fn active_runs(&self) -> Result<Vec<Run>> {
        self.runs_where("ended_at IS NULL AND ?1", 1.into())
    }

    /// Ends every run that has not ended as failed, with an event saying why; returns them. For a server that
    /// starts again, since no run outlives the server that drove it.
    pub fn fail_unended_runs(&self, why: &str) -> Result<Vec<Run>> {
        let runs = self.active_runs()?;
        for run in &runs {
            self.append_event(
                run.id,
                "moebius_error",
                &serde_json::json!({ "message": why }),
            )?;
            self.set_run_status(run.id, "failed")?;
        }
        Ok(runs)
    }

    /// Marks a waiting chat as running; a chat in any other state stays as it is.
    pub fn set_chat_working(&self, id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE runs SET status = 'running' WHERE id = ?1 AND status = 'waiting'",
            [id],
        )?;
        Ok(())
    }

    pub fn set_run_status(&self, id: i64, status: &str) -> Result<()> {
        let ended = matches!(status, "succeeded" | "failed" | "stopped").then(now);
        let conn = self.conn();
        conn.execute(
            "UPDATE runs SET status = ?1, ended_at = COALESCE(?2, ended_at) WHERE id = ?3",
            params![status, ended, id],
        )?;
        if status == "succeeded" {
            // The note the run was started with has been dealt with.
            conn.execute(
                "UPDATE cards SET note = NULL WHERE id = (SELECT card_id FROM runs WHERE id = ?1 AND purpose = 'code')",
                [id],
            )?;
        }
        Ok(())
    }

    pub fn set_run_repository(&self, id: i64, repository_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE runs SET repository_id = ?1 WHERE id = ?2",
            params![repository_id, id],
        )?;
        Ok(())
    }

    /// Records that the chat works in the harness configuration repository.
    pub fn set_run_harness_repository(&self, id: i64) -> Result<()> {
        self.conn()
            .execute("UPDATE runs SET harness_repository = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn set_run_summary(&self, id: i64, summary: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE runs SET summary = ?1 WHERE id = ?2",
            params![summary, id],
        )?;
        Ok(())
    }

    /// The branch of the card's latest successful `code` run, where its work stands.
    pub fn code_branch(&self, card_id: i64) -> Result<Option<String>> {
        Ok(self
            .runs_for_card(card_id)?
            .iter()
            .rev()
            .find(|r| r.purpose == "code" && r.status == "succeeded")
            .map(Run::branch))
    }

    pub fn set_run_worktree(&self, id: i64, worktree: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE runs SET worktree = ?1 WHERE id = ?2",
            params![worktree, id],
        )?;
        Ok(())
    }

    /// Appends an event to a run and returns its sequence number.
    /// Stores an event of a run, with every secret in it redacted, and returns its `seq`.
    pub fn append_event(&self, run_id: i64, kind: &str, data: &serde_json::Value) -> Result<i64> {
        let data = &crate::blobs::redact_json(data);
        let conn = self.conn();
        let seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO events (run_id, seq, kind, data, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![run_id, seq, kind, serde_json::to_string(data)?, now()],
        )?;
        Ok(seq)
    }

    pub fn events(&self, run_id: i64) -> Result<Vec<Event>> {
        self.events_after(run_id, 0, i64::MAX)
    }

    /// At most `limit` events of a run with a sequence number above `after`, in order.
    pub fn events_after(&self, run_id: i64, after: i64, limit: i64) -> Result<Vec<Event>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT run_id, seq, kind, data, at FROM events WHERE run_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![run_id, after, limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get(4)?,
            ))
        })?;
        rows.map(|row| {
            let (run_id, seq, kind, data, at) = row?;
            Ok(Event {
                run_id,
                seq,
                kind,
                data: serde_json::from_str(&data)?,
                at,
            })
        })
        .collect()
    }

    // Judgements

    pub fn add_judgement(&self, j: &NewJudgement) -> Result<Judgement> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO judgements (kind, context, options, choice, reason, decider, run_id, card_id, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                j.kind,
                serde_json::to_string(&j.context)?,
                serde_json::to_string(&j.options)?,
                j.choice,
                j.reason,
                j.decider,
                j.run_id,
                j.card_id,
                at
            ],
        )?;
        Ok(Judgement {
            id: conn.last_insert_rowid(),
            kind: j.kind.clone(),
            context: j.context.clone(),
            options: j.options.clone(),
            choice: j.choice.clone(),
            reason: j.reason.clone(),
            decider: j.decider.clone(),
            run_id: j.run_id,
            card_id: j.card_id,
            outcome: None,
            at,
        })
    }

    fn judgements_where(
        &self,
        clause: &str,
        param: rusqlite::types::Value,
    ) -> Result<Vec<Judgement>> {
        self.judgements_query(&format!("WHERE {clause} ORDER BY id"), [param])
    }

    /// The judgements, newest first, of the card, of the run and of the kind if given; at most `limit`.
    pub fn judgements_page(
        &self,
        card_id: Option<i64>,
        run_id: Option<i64>,
        kind: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Judgement>> {
        self.judgements_query(
            "WHERE (?1 IS NULL OR card_id = ?1) AND (?2 IS NULL OR run_id = ?2) AND (?3 IS NULL OR kind = ?3)
             ORDER BY id DESC LIMIT ?4",
            params![card_id, run_id, kind, limit],
        )
    }

    fn judgements_query(
        &self,
        tail: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<Judgement>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, kind, context, options, choice, reason, decider, run_id, card_id, outcome, at
             FROM judgements {tail}"
        ))?;
        let rows = stmt.query_map(params, |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, i64>(10)?,
            ))
        })?;
        rows.map(|row| {
            let (id, kind, context, options, choice, reason, decider, run_id, card_id, outcome, at) = row?;
            Ok(Judgement {
                id,
                kind,
                context: serde_json::from_str(&context)?,
                options: serde_json::from_str(&options)?,
                choice,
                reason,
                decider,
                run_id,
                card_id,
                outcome,
                at,
            })
        })
        .collect()
    }

    /// The judgements of `kind` with an id above `after`, oldest first; at most `limit`.
    pub fn judgements_of_kind_after(
        &self,
        kind: &str,
        after: i64,
        limit: i64,
    ) -> Result<Vec<Judgement>> {
        self.judgements_query(
            "WHERE kind = ?1 AND id > ?2 ORDER BY id LIMIT ?3",
            params![kind, after, limit],
        )
    }

    /// Calls `each` with the kind, the id, whether there is an outcome and the decider of every judgement, oldest
    /// first, without reading them all into memory.
    pub fn each_judgement(&self, mut each: impl FnMut(&str, i64, bool, &str)) -> Result<()> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT kind, id, outcome IS NOT NULL, decider FROM judgements ORDER BY id")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            each(
                &row.get::<_, String>(0)?,
                row.get(1)?,
                row.get(2)?,
                &row.get::<_, String>(3)?,
            );
        }
        Ok(())
    }

    pub fn judgement(&self, id: i64) -> Result<Option<Judgement>> {
        Ok(self.judgements_where("id = ?1", id.into())?.pop())
    }

    pub fn judgements(&self, kind: &str) -> Result<Vec<Judgement>> {
        self.judgements_where("kind = ?1", kind.to_string().into())
    }

    pub fn judgements_for_card(&self, card_id: i64) -> Result<Vec<Judgement>> {
        self.judgements_where("card_id = ?1", card_id.into())
    }

    pub fn set_outcome(&self, judgement_id: i64, outcome: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE judgements SET outcome = ?1 WHERE id = ?2",
            params![outcome, judgement_id],
        )?;
        Ok(())
    }

    /// Sets `outcome` on every judgement made for the card that has none yet; returns how many were set.
    pub fn set_missing_outcomes(&self, card_id: i64, outcome: &str) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE judgements SET outcome = ?1 WHERE card_id = ?2 AND outcome IS NULL",
            params![outcome, card_id],
        )?)
    }

    /// The refused tool calls no outcome and no replay is known for, made since `since`, oldest first.
    pub fn replay_candidates(&self, since: i64) -> Result<Vec<Judgement>> {
        self.judgements_query(
            "WHERE kind = 'tool_call' AND choice = 'refuse' AND run_id IS NOT NULL
               AND at >= ?1 AND NOT EXISTS (SELECT 1 FROM replays WHERE judgement_id = judgements.id)
             ORDER BY id",
            [since],
        )
    }

    /// How many tool calls the guard refused since `since` because of each rule, by the rule's name. A pattern and a
    /// rule in prose that the teacher names both write their reason as `<rule>: <why>`; a refusal without a rule
    /// (the teacher's own failures, a rules file that cannot be read, a missing teacher) is not counted.
    pub fn refusals_by_rule(&self, since: i64) -> Result<BTreeMap<String, i64>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT substr(reason, 1, instr(reason, ': ') - 1) AS rule, COUNT(*) FROM judgements
             WHERE kind = 'tool_call' AND choice = 'refuse' AND decider IN ('guard/patterns', 'guard/teacher')
               AND at >= ?1 AND instr(reason, ': ') > 1 AND rule <> 'teacher'
               AND reason NOT LIKE 'the rules cannot be read%'
             GROUP BY rule",
        )?;
        let rows = stmt.query_map([since], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // Replays

    /// Stores `replay` (its `id` and `at` are set here) and returns it as stored.
    pub fn add_replay(&self, replay: Replay) -> Result<Replay> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO replays (judgement_id, run_id, exit, timed_out, output, created, changed, deleted, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                replay.judgement,
                replay.run,
                replay.exit,
                replay.timed_out,
                replay.output,
                serde_json::to_string(&replay.created)?,
                serde_json::to_string(&replay.changed)?,
                serde_json::to_string(&replay.deleted)?,
                at
            ],
        )?;
        Ok(Replay {
            id: conn.last_insert_rowid(),
            at,
            ..replay
        })
    }

    /// The replays of the judgement and of the run, if given, newest first; at most `limit`.
    pub fn replays(
        &self,
        judgement: Option<i64>,
        run: Option<i64>,
        limit: i64,
    ) -> Result<Vec<Replay>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, judgement_id, run_id, exit, timed_out, output, created, changed, deleted, at
             FROM replays WHERE (?1 IS NULL OR judgement_id = ?1) AND (?2 IS NULL OR run_id = ?2)
             ORDER BY id DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![judgement, run, limit], |r| {
            Ok((
                Replay {
                    id: r.get(0)?,
                    judgement: r.get(1)?,
                    run: r.get(2)?,
                    exit: r.get(3)?,
                    timed_out: r.get(4)?,
                    output: r.get(5)?,
                    created: Vec::new(),
                    changed: Vec::new(),
                    deleted: Vec::new(),
                    at: r.get(9)?,
                },
                [r.get::<_, String>(6)?, r.get(7)?, r.get(8)?],
            ))
        })?;
        rows.map(|row| {
            let (replay, [created, changed, deleted]) = row?;
            Ok(Replay {
                created: serde_json::from_str(&created)?,
                changed: serde_json::from_str(&changed)?,
                deleted: serde_json::from_str(&deleted)?,
                ..replay
            })
        })
        .collect()
    }

    /// How many replays were made since `since`.
    pub fn replays_since(&self, since: i64) -> Result<usize> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM replays WHERE at >= ?1",
            [since],
            |r| r.get::<_, i64>(0),
        )? as usize)
    }

    // Inbox

    pub fn add_inbox_item(
        &self,
        kind: &str,
        card_id: Option<i64>,
        title: &str,
        data: &serde_json::Value,
    ) -> Result<InboxItem> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO inbox_items (kind, card_id, title, data, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![kind, card_id, title, serde_json::to_string(data)?, at],
        )?;
        Ok(InboxItem {
            id: conn.last_insert_rowid(),
            kind: kind.into(),
            card_id,
            title: title.into(),
            data: data.clone(),
            resolved_at: None,
            at,
        })
    }

    /// How many tool calls of a run were refused.
    pub fn refused_tool_calls(&self, run_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM events WHERE run_id = ?1 AND kind = 'tool_check'
               AND json_extract(data, '$.decision') = 'refuse'",
            [run_id],
            |r| r.get(0),
        )?)
    }

    /// The items not resolved yet, oldest first.
    pub fn inbox(&self) -> Result<Vec<InboxItem>> {
        self.inbox_where("resolved_at IS NULL AND ?1", 1.into())
    }

    /// All the items about a card, resolved or not, oldest first.
    pub fn inbox_for_card(&self, card_id: i64) -> Result<Vec<InboxItem>> {
        self.inbox_where("card_id = ?1", card_id.into())
    }

    fn inbox_where(&self, clause: &str, param: rusqlite::types::Value) -> Result<Vec<InboxItem>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, kind, card_id, title, data, resolved_at, at FROM inbox_items WHERE {clause} ORDER BY id"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })?;
        rows.map(|row| {
            let (id, kind, card_id, title, data, resolved_at, at) = row?;
            Ok(InboxItem {
                id,
                kind,
                card_id,
                title,
                data: serde_json::from_str(&data)?,
                resolved_at,
                at,
            })
        })
        .collect()
    }

    pub fn resolve_inbox_item(&self, id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 WHERE id = ?2",
            params![now(), id],
        )?;
        Ok(())
    }

    /// Whether the card has an unresolved item of `kind`.
    pub fn has_open_inbox_item(&self, card_id: i64, kind: &str) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM inbox_items WHERE card_id = ?1 AND kind = ?2 AND resolved_at IS NULL)",
            params![card_id, kind],
            |r| r.get(0),
        )?)
    }

    /// Resolves every open item about a card, such as its "ready to merge" once it is merged.
    pub fn resolve_inbox_for_card(&self, card_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 WHERE card_id = ?2 AND resolved_at IS NULL",
            params![now(), card_id],
        )?;
        Ok(())
    }

    /// Resolves the open "ready to merge" item of a change, which has no card to resolve it through.
    pub fn resolve_inbox_for_change(&self, change_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 \
             WHERE kind = 'ready_to_merge' AND json_extract(data, '$.change') = ?2 AND resolved_at IS NULL",
            params![now(), change_id],
        )?;
        Ok(())
    }

    /// Resolves the open inbox item of a grilling, which has no card to resolve it through.
    pub fn resolve_inbox_for_grilling(&self, grilling_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 \
             WHERE kind IN ('grilling_issues', 'grilling_report') \
             AND json_extract(data, '$.grilling') = ?2 AND resolved_at IS NULL",
            params![now(), grilling_id],
        )?;
        Ok(())
    }

    /// Resolves the open inbox item of a proposal, which has no card to resolve it through.
    pub fn resolve_inbox_for_proposal(&self, proposal_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 \
             WHERE kind = 'proposal' AND json_extract(data, '$.proposal') = ?2 AND resolved_at IS NULL",
            params![now(), proposal_id],
        )?;
        Ok(())
    }

    // Grillings

    pub fn add_grilling(
        &self,
        run_id: i64,
        repository_id: i64,
        issues: &[GrillingIssue],
        report: Option<&str>,
        status: &str,
        reason: &str,
    ) -> Result<Grilling> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO grillings (run_id, repository_id, issues, report, status, reason, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run_id,
                repository_id,
                serde_json::to_string(issues)?,
                report,
                status,
                reason,
                now()
            ],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        Ok(self.grilling(id)?.expect("the grilling was just stored"))
    }

    fn grillings_where(
        &self,
        clause: &str,
        param: rusqlite::types::Value,
    ) -> Result<Vec<Grilling>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, run_id, repository_id, issues, report, status, reason, at
             FROM grillings WHERE {clause} ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok((
                Grilling {
                    id: r.get(0)?,
                    run_id: r.get(1)?,
                    repository_id: r.get(2)?,
                    issues: Vec::new(),
                    report: r.get(4)?,
                    status: r.get(5)?,
                    reason: r.get(6)?,
                    at: r.get(7)?,
                },
                r.get::<_, String>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (mut grilling, issues) = row?;
            grilling.issues = serde_json::from_str(&issues)?;
            Ok(grilling)
        })
        .collect()
    }

    pub fn grilling(&self, id: i64) -> Result<Option<Grilling>> {
        Ok(self.grillings_where("id = ?1", id.into())?.pop())
    }

    /// All grillings, the newest first.
    pub fn grillings(&self) -> Result<Vec<Grilling>> {
        self.grillings_where("?1", 1.into())
    }

    /// Gives a grilling that has the status `from` the status `to`; false, and nothing changed, when it has another.
    pub fn move_grilling(&self, id: i64, from: &[&str], to: &str) -> Result<bool> {
        let marks = vec!["?"; from.len()].join(", ");
        let sql = format!("UPDATE grillings SET status = ? WHERE id = ? AND status IN ({marks})");
        let mut values: Vec<rusqlite::types::Value> = vec![to.to_string().into(), id.into()];
        values.extend(
            from.iter()
                .map(|s| rusqlite::types::Value::from(s.to_string())),
        );
        Ok(self
            .conn()
            .execute(&sql, rusqlite::params_from_iter(values))?
            == 1)
    }

    // Shadows and takeovers

    pub fn add_shadow(&self, shadow: &NewShadow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO shadows (judgement_id, kind, choice, reason, failed, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                shadow.judgement_id,
                shadow.kind,
                shadow.choice,
                shadow.reason,
                shadow.failed,
                now()
            ],
        )?;
        Ok(())
    }

    /// What the shadows of each kind, or only of `kind`, say about the model, kinds in alphabetical order.
    pub fn shadow_stats(&self, kind: Option<&str>) -> Result<Vec<ShadowStats>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT s.kind, COUNT(*), COALESCE(SUM(s.failed = 0 AND s.choice = j.choice), 0), COALESCE(SUM(s.failed), 0)
             FROM shadows s JOIN judgements j ON j.id = s.judgement_id
             WHERE ?1 IS NULL OR s.kind = ?1 GROUP BY s.kind ORDER BY s.kind",
        )?;
        let rows = stmt.query_map([kind], |r| {
            Ok(ShadowStats {
                kind: r.get(0)?,
                shadows: r.get(1)?,
                agreed: r.get(2)?,
                failed: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_takeover(&self, kind: &str, agreement: f64, count: i64) -> Result<Takeover> {
        let conn = self.conn();
        let at = now();
        conn.execute(
            "INSERT INTO takeovers (kind, agreement, count, at) VALUES (?1, ?2, ?3, ?4)",
            params![kind, agreement, count, at],
        )?;
        Ok(Takeover {
            id: conn.last_insert_rowid(),
            kind: kind.into(),
            agreement,
            count,
            status: "open".into(),
            at,
        })
    }

    /// The takeovers, oldest first.
    pub fn takeovers(&self) -> Result<Vec<Takeover>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT id, kind, agreement, count, status, at FROM takeovers ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Takeover {
                id: r.get(0)?,
                kind: r.get(1)?,
                agreement: r.get(2)?,
                count: r.get(3)?,
                status: r.get(4)?,
                at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn takeover(&self, id: i64) -> Result<Option<Takeover>> {
        Ok(self.takeovers()?.into_iter().find(|t| t.id == id))
    }

    /// Gives a takeover that has the status `from` the status `to`; false, and nothing changed, when it has another.
    pub fn move_takeover(&self, id: i64, from: &str, to: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE takeovers SET status = ?1 WHERE id = ?2 AND status = ?3",
            params![to, id, from],
        )? == 1)
    }

    /// Resolves the open inbox item of a takeover, which has no card to resolve it through.
    pub fn resolve_inbox_for_takeover(&self, takeover_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 \
             WHERE kind = 'takeover' AND json_extract(data, '$.takeover') = ?2 AND resolved_at IS NULL",
            params![now(), takeover_id],
        )?;
        Ok(())
    }

    // Usage

    pub fn add_usage(&self, u: &Usage) -> Result<()> {
        self.conn().execute(
            "INSERT INTO usage (run_id, model, input_tokens, output_tokens, cached_tokens, duration_ms, tools, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![u.run_id, u.model, u.input_tokens, u.output_tokens, u.cached_tokens, u.duration_ms, serde_json::to_string(&u.tools)?, now()],
        )?;
        Ok(())
    }

    pub fn usage_for_run(&self, run_id: i64) -> Result<Vec<Usage>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT run_id, model, input_tokens, output_tokens, cached_tokens, duration_ms, tools FROM usage WHERE run_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([run_id], |r| {
            Ok(Usage {
                run_id: r.get(0)?,
                model: r.get(1)?,
                input_tokens: r.get(2)?,
                output_tokens: r.get(3)?,
                cached_tokens: r.get(4)?,
                duration_ms: r.get(5)?,
                tools: tools_from_column(r.get(6)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Tokens used by the runs of a card since it was last retried.
    pub fn tokens_for_card(&self, card_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(SUM(u.input_tokens + u.output_tokens), 0) FROM usage u JOIN runs r ON r.id = u.run_id
             WHERE r.card_id = ?1 AND r.purpose <> 'trial'
               AND r.id > COALESCE((SELECT retried_after FROM cards WHERE id = ?1), 0)",
            [card_id],
            |r| r.get(0),
        )?)
    }

    /// Tokens used by all runs since the start of the server's local day.
    pub fn tokens_today(&self) -> Result<i64> {
        Ok(self.usage_totals(1)?.tokens)
    }

    /// The date of today on the server, as `YYYY-MM-DD`.
    pub fn today(&self) -> Result<String> {
        Ok(self
            .conn()
            .query_row("SELECT strftime('%Y-%m-%d', 'now', 'localtime')", [], |r| {
                r.get(0)
            })?)
    }

    /// Everything used in the last `days` local days, today included.
    pub fn usage_totals(&self, days: i64) -> Result<UsageTotals> {
        Ok(self.conn().query_row(
            &format!(
                "SELECT COALESCE(SUM(input_tokens + output_tokens), 0), COUNT(*), COALESCE(SUM(input_tokens), 0),
                        COALESCE(SUM(cached_tokens), 0), COALESCE(SUM(duration_ms), 0)
                 FROM usage WHERE at >= {WINDOW_START}"
            ),
            [days],
            |r| {
                Ok(UsageTotals {
                    tokens: r.get(0)?,
                    calls: r.get(1)?,
                    input_tokens: r.get(2)?,
                    cached_tokens: r.get(3)?,
                    model_ms: r.get(4)?,
                })
            },
        )?)
    }

    /// The tokens of each model on each local day of the last `days` days, as `(YYYY-MM-DD, model, tokens)`,
    /// oldest day first.
    pub fn usage_by_day(&self, days: i64) -> Result<Vec<(String, String, i64)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT strftime('%Y-%m-%d', at, 'unixepoch', 'localtime') AS day, model, SUM(input_tokens + output_tokens)
             FROM usage WHERE at >= {WINDOW_START} GROUP BY day, model ORDER BY day, model"
        ))?;
        let rows = stmt.query_map([days], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What each model used in the last `days` days, the one with the most tokens first.
    pub fn usage_by_model(&self, days: i64) -> Result<Vec<ModelUsage>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT model, SUM(input_tokens + output_tokens) AS tokens, COUNT(*), SUM(input_tokens), SUM(cached_tokens)
             FROM usage WHERE at >= {WINDOW_START} GROUP BY model ORDER BY tokens DESC, model"
        ))?;
        let rows = stmt.query_map([days], |r| {
            Ok(ModelUsage {
                model: r.get(0)?,
                tokens: r.get(1)?,
                calls: r.get(2)?,
                input_tokens: r.get(3)?,
                cached_tokens: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What the runs of each agent used in the last `days` days, the agent with the most tokens first. Usage without
    /// a run is counted under [`NO_RUN`].
    pub fn usage_by_agent(&self, days: i64) -> Result<Vec<AgentUsage>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT COALESCE(r.agent, ?2) AS agent, COUNT(*), SUM(u.input_tokens + u.output_tokens) AS tokens,
                    SUM(u.input_tokens), SUM(u.cached_tokens), SUM(u.duration_ms)
             FROM usage u LEFT JOIN runs r ON r.id = u.run_id
             WHERE u.at >= {WINDOW_START} GROUP BY agent ORDER BY tokens DESC, agent"
        ))?;
        let rows = stmt.query_map(params![days, NO_RUN], |r| {
            Ok(AgentUsage {
                agent: r.get(0)?,
                calls: r.get(1)?,
                tokens: r.get(2)?,
                input_tokens: r.get(3)?,
                cached_tokens: r.get(4)?,
                model_ms: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What the model did in the last `days` days, the activity with the most model time first. A call that asked for
    /// `n` tools gives each tool `1/n` of its duration and output tokens (what does not divide goes to the first
    /// tools, so nothing is lost); a call that asked for none is `answering`. A call counts once for each distinct
    /// tool it named. Calls recorded before their tools were are `unknown`, and are listed only when there are no other
    /// calls in the window.
    pub fn usage_by_activity(&self, days: i64) -> Result<Vec<ActivityUsage>> {
        let calls: Vec<(Option<String>, i64, i64)> = {
            let conn = self.conn();
            let mut stmt = conn.prepare(&format!(
                "SELECT tools, duration_ms, output_tokens FROM usage WHERE at >= {WINDOW_START}"
            ))?;
            let rows = stmt.query_map([days], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut sums: BTreeMap<String, ActivityUsage> = BTreeMap::new();
        let mut unknown = ActivityUsage {
            activity: UNKNOWN.into(),
            model_ms: 0,
            calls: 0,
            output_tokens: 0,
        };
        for (tools, model_ms, output_tokens) in calls {
            let Some(tools) = tools else {
                unknown.model_ms += model_ms;
                unknown.calls += 1;
                unknown.output_tokens += output_tokens;
                continue;
            };
            let tools = tools_from_column(Some(tools));
            let names: Vec<&str> = if tools.is_empty() {
                vec![ANSWERING]
            } else {
                tools.iter().map(String::as_str).collect()
            };
            let n = names.len() as i64;
            let mut counted = BTreeSet::new();
            for (i, name) in names.into_iter().enumerate() {
                let i = i as i64;
                let sum = sums
                    .entry(name.to_string())
                    .or_insert_with(|| ActivityUsage {
                        activity: name.to_string(),
                        model_ms: 0,
                        calls: 0,
                        output_tokens: 0,
                    });
                sum.model_ms += model_ms / n + i64::from(i < model_ms % n);
                sum.output_tokens += output_tokens / n + i64::from(i < output_tokens % n);
                if counted.insert(name) {
                    sum.calls += 1;
                }
            }
        }
        let mut activities: Vec<ActivityUsage> = sums.into_values().collect();
        if activities.is_empty() && unknown.calls > 0 {
            activities.push(unknown);
        }
        activities.sort_by(|a, b| {
            b.model_ms
                .cmp(&a.model_ms)
                .then_with(|| a.activity.cmp(&b.activity))
        });
        Ok(activities)
    }

    /// The tools whose calls took longest in the last `days` days, by the time between a call's
    /// `tool_execution_start` and `tool_execution_end` events, at most `limit` of them.
    pub fn tool_waits(&self, days: i64, limit: i64) -> Result<Vec<ToolWait>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT json_extract(s.data, '$.toolName') AS tool, COUNT(*), SUM(e.at - s.at) AS seconds
             FROM events s JOIN events e ON e.run_id = s.run_id AND e.kind = 'tool_execution_end'
              AND json_extract(e.data, '$.toolCallId') = json_extract(s.data, '$.toolCallId')
             WHERE s.kind = 'tool_execution_start' AND s.at >= {WINDOW_START} AND tool IS NOT NULL
             GROUP BY tool ORDER BY seconds DESC, tool LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![days, limit], |r| {
            Ok(ToolWait {
                tool: r.get(0)?,
                calls: r.get(1)?,
                seconds: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // Retros and proposals

    /// The first moment of the local day `date` (`YYYY-MM-DD`) and of the day after it, as Unix seconds; none when
    /// `date` is no date.
    pub fn day_window(&self, date: &str) -> Result<Option<(i64, i64)>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT CAST(strftime('%s', date(?1) || ' 00:00:00', 'utc') AS INTEGER),
                    CAST(strftime('%s', date(?1, '+1 day') || ' 00:00:00', 'utc') AS INTEGER)
             WHERE date(?1) = ?1",
                [date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// The local time of day on the server, as minutes after midnight.
    pub fn minutes_into_the_day(&self) -> Result<u16> {
        Ok(self.conn().query_row(
            "SELECT CAST(strftime('%H', 'now', 'localtime') AS INTEGER) * 60
                  + CAST(strftime('%M', 'now', 'localtime') AS INTEGER)",
            [],
            |r| r.get(0),
        )?)
    }

    /// The `Found` rows of `sql`, which takes the window as `?1` and `?2` and the run as `?3`.
    fn found(
        &self,
        kind: &'static str,
        sql: &str,
        from: i64,
        to: i64,
        run: Option<i64>,
    ) -> Result<Vec<Found>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![from, to, run], |r| {
            Ok(Found {
                kind: "",
                run_id: r.get(0)?,
                card_id: r.get(1)?,
                agent: r.get(2)?,
                text: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                at: r.get(4)?,
            })
        })?;
        rows.map(|row| Ok(Found { kind, ..row? })).collect()
    }

    /// The tool calls that failed in `[from, to)`: the tool and the start of what it said. A call Moebius refused is
    /// not among them; it is a refusal. With `run`, those of that run only, whatever its purpose; without, those of
    /// every run but the Retro's and the trials'. The same goes for the other sources below.
    pub fn failed_tool_calls(&self, from: i64, to: i64, run: Option<i64>) -> Result<Vec<Found>> {
        self.found(
            "tool_failed",
            "SELECT e.run_id, r.card_id, r.agent,
                    COALESCE(json_extract(e.data, '$.toolName'), 'tool') || ': ' || COALESCE(
                        json_extract(e.data, '$.result.content[0].text'), json_extract(e.data, '$.result'), ''),
                    e.at
             FROM events e JOIN runs r ON r.id = e.run_id
             WHERE e.kind = 'tool_execution_end' AND json_extract(e.data, '$.isError') = 1
               AND COALESCE(json_extract(e.data, '$.result.content[0].text'), '') NOT LIKE 'Moebius refused this call:%'
               AND (r.purpose NOT IN ('retro', 'trial', 'research') AND ?3 IS NULL OR r.id = ?3) AND e.at >= ?1 AND e.at < ?2
             ORDER BY e.at, e.id",
            from,
            to,
            run,
        )
    }

    /// The tool calls the rules refused in `[from, to)`: the tool and the reason.
    pub fn refused_tool_calls_between(
        &self,
        from: i64,
        to: i64,
        run: Option<i64>,
    ) -> Result<Vec<Found>> {
        self.found(
            "tool_refused",
            "SELECT e.run_id, r.card_id, r.agent,
                    COALESCE(json_extract(e.data, '$.tool'), 'tool') || ': ' || COALESCE(json_extract(e.data, '$.reason'), ''),
                    e.at
             FROM events e JOIN runs r ON r.id = e.run_id
             WHERE e.kind = 'tool_check' AND json_extract(e.data, '$.decision') = 'refuse'
               AND (r.purpose NOT IN ('retro', 'trial', 'research') AND ?3 IS NULL OR r.id = ?3) AND e.at >= ?1 AND e.at < ?2
             ORDER BY e.at, e.id",
            from,
            to,
            run,
        )
    }

    /// The runs that failed in `[from, to)`, with the latest `moebius_error` of each.
    pub fn failed_runs(&self, from: i64, to: i64, run: Option<i64>) -> Result<Vec<Found>> {
        self.ended_runs("run_failed", "failed", from, to, run)
    }

    /// The runs that were stopped in `[from, to)`.
    pub fn stopped_runs(&self, from: i64, to: i64, run: Option<i64>) -> Result<Vec<Found>> {
        self.ended_runs("run_stopped", "stopped", from, to, run)
    }

    fn ended_runs(
        &self,
        kind: &'static str,
        status: &str,
        from: i64,
        to: i64,
        run: Option<i64>,
    ) -> Result<Vec<Found>> {
        self.found(
            kind,
            &format!(
                "SELECT r.id, r.card_id, r.agent,
                        (SELECT json_extract(e.data, '$.message') FROM events e
                         WHERE e.run_id = r.id AND e.kind = 'moebius_error' ORDER BY e.seq DESC LIMIT 1),
                        r.ended_at
                 FROM runs r
                 WHERE r.status = '{status}' AND (r.purpose NOT IN ('retro', 'trial', 'research') AND ?3 IS NULL OR r.id = ?3)
                   AND r.ended_at >= ?1 AND r.ended_at < ?2
                 ORDER BY r.ended_at, r.id"
            ),
            from,
            to,
            run,
        )
    }

    /// The messages people sent to running runs in `[from, to)`.
    pub fn steering(&self, from: i64, to: i64, run: Option<i64>) -> Result<Vec<Found>> {
        self.found(
            "steered",
            "SELECT e.run_id, r.card_id, r.agent, json_extract(e.data, '$.message'), e.at
             FROM events e JOIN runs r ON r.id = e.run_id
             WHERE e.kind = 'steered' AND (r.purpose NOT IN ('trial', 'research') AND ?3 IS NULL OR r.id = ?3)
               AND e.at >= ?1 AND e.at < ?2
             ORDER BY e.at, e.id",
            from,
            to,
            run,
        )
    }

    /// The cards sent back from review to coding in `[from, to)`, with the note that said why. The note decides the
    /// kind: failed checks, a merge conflict, or else the Reviewer's findings.
    pub fn sent_back_cards(&self, from: i64, to: i64) -> Result<Vec<Found>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT m.card_id, COALESCE(m.note, ''), m.at FROM card_moves m
             WHERE m.from_column = 'In review' AND m.to_column = 'In progress' AND m.at >= ?1 AND m.at < ?2
             ORDER BY m.at, m.id",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            let note: String = r.get(1)?;
            let kind = if note.starts_with("The checks failed") {
                "checks_failed"
            } else if note.starts_with("The change no longer merges") {
                "conflict"
            } else {
                "review_findings"
            };
            Ok(Found {
                kind,
                run_id: None,
                card_id: Some(r.get(0)?),
                agent: None,
                text: note,
                at: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Stores the Retro of a day with `status`; false, and nothing stored, when the day has one already.
    pub fn add_retro(&self, date: &str, report: &serde_json::Value, status: &str) -> Result<bool> {
        let added = self.conn().execute(
            "INSERT INTO retros (date, report, status, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (date) DO NOTHING",
            params![date, serde_json::to_string(report)?, status, now()],
        )?;
        Ok(added == 1)
    }

    pub fn set_retro_run(&self, date: &str, run_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE retros SET run_id = ?1 WHERE date = ?2",
            params![run_id, date],
        )?;
        Ok(())
    }

    /// Ends the analysis of a day's Retro: its status, and what was left out or went wrong.
    pub fn finish_retro(&self, date: &str, status: &str, warnings: &[String]) -> Result<()> {
        self.conn().execute(
            "UPDATE retros SET status = ?1, warnings = ?2 WHERE date = ?3",
            params![status, serde_json::to_string(warnings)?, date],
        )?;
        Ok(())
    }

    /// Fails the Retros whose analysis the last server did not see through.
    pub fn fail_analysing_retros(&self, why: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE retros SET status = 'failed', warnings = json_array(?1) WHERE status = 'analysing'",
            [why],
        )?;
        Ok(())
    }

    fn retros_where(&self, clause: &str, param: &str) -> Result<Vec<Retro>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT date, report, status, run_id, warnings, at FROM retros WHERE {clause} ORDER BY date DESC"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get::<_, String>(4)?,
                r.get(5)?,
            ))
        })?;
        rows.map(|row| {
            let (date, report, status, run_id, warnings, at) = row?;
            Ok(Retro {
                date,
                report: serde_json::from_str(&report)?,
                status,
                run_id,
                warnings: serde_json::from_str(&warnings)?,
                at,
            })
        })
        .collect()
    }

    /// The Retro of a day, if it has one.
    pub fn retro(&self, date: &str) -> Result<Option<Retro>> {
        Ok(self.retros_where("date = ?1", date)?.pop())
    }

    /// All Retros, the latest day first.
    pub fn retros(&self) -> Result<Vec<Retro>> {
        self.retros_where("?1 = ?1", "")
    }

    pub fn add_proposal(
        &self,
        retro_date: &str,
        branch: &str,
        title: &str,
        body: &str,
        agents: &[String],
        cards: &[i64],
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO proposals (retro_date, branch, title, body, agents, cards, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                retro_date,
                branch,
                title,
                body,
                serde_json::to_string(agents)?,
                serde_json::to_string(cards)?,
                now()
            ],
        )?;
        Ok(())
    }

    fn proposals_where(
        &self,
        clause: &str,
        param: rusqlite::types::Value,
    ) -> Result<Vec<Proposal>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, retro_date, branch, title, body, agents, cards, status, at FROM proposals
             WHERE {clause} ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok((
                Proposal {
                    id: r.get(0)?,
                    retro_date: r.get(1)?,
                    branch: r.get(2)?,
                    title: r.get(3)?,
                    body: r.get(4)?,
                    agents: vec![],
                    cards: vec![],
                    status: r.get(7)?,
                    trials: vec![],
                    at: r.get(8)?,
                },
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        let mut stmt = conn.prepare(
            "SELECT id, proposal_id, card_id, run_id, outcome, reason, measurements, at FROM trials
             WHERE proposal_id = ?1 ORDER BY id",
        )?;
        rows.map(|row| {
            let (proposal, agents, cards) = row?;
            let trials = stmt.query_map([proposal.id], |r| {
                Ok((
                    Trial {
                        id: r.get(0)?,
                        proposal_id: r.get(1)?,
                        card_id: r.get(2)?,
                        run_id: r.get(3)?,
                        outcome: r.get(4)?,
                        reason: r.get(5)?,
                        measurements: serde_json::Value::Null,
                        at: r.get(7)?,
                    },
                    r.get::<_, String>(6)?,
                ))
            })?;
            let trials = trials
                .map(|row| {
                    let (trial, measurements) = row?;
                    Ok(Trial {
                        measurements: serde_json::from_str(&measurements)?,
                        ..trial
                    })
                })
                .collect::<Result<_>>()?;
            Ok(Proposal {
                agents: serde_json::from_str(&agents)?,
                cards: serde_json::from_str(&cards)?,
                trials,
                ..proposal
            })
        })
        .collect()
    }

    /// The proposals of one Retro (`Some(date)`) or of all, the newest first, with their trials.
    pub fn proposals(&self, retro_date: Option<&str>) -> Result<Vec<Proposal>> {
        match retro_date {
            Some(date) => self.proposals_where("retro_date = ?1", date.to_string().into()),
            None => self.proposals_where("?1", 1.into()),
        }
    }

    pub fn proposal(&self, id: i64) -> Result<Option<Proposal>> {
        Ok(self.proposals_where("id = ?1", id.into())?.pop())
    }

    /// Sets the status of a proposal.
    pub fn set_proposal_status(&self, id: i64, status: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE proposals SET status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    /// Starts the trying of an open proposal; false, and nothing changed, when it is not open or another proposal is
    /// being tried, since only one is at a time.
    pub fn start_trying(&self, id: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE proposals SET status = 'trying'
             WHERE id = ?1 AND status = 'open' AND NOT EXISTS (SELECT 1 FROM proposals WHERE status = 'trying')",
            [id],
        )? == 1)
    }

    /// Opens the proposals the last server was trying, without the trials it had made of them.
    pub fn reopen_trying_proposals(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM trials WHERE proposal_id IN (SELECT id FROM proposals WHERE status = 'trying')",
            [],
        )?;
        conn.execute(
            "UPDATE proposals SET status = 'open' WHERE status = 'trying'",
            [],
        )?;
        Ok(())
    }

    pub fn add_trial(
        &self,
        proposal_id: i64,
        card_id: Option<i64>,
        run_id: Option<i64>,
        outcome: &str,
        reason: &str,
        measurements: &serde_json::Value,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO trials (proposal_id, card_id, run_id, outcome, reason, measurements, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                proposal_id,
                card_id,
                run_id,
                outcome,
                reason,
                serde_json::to_string(measurements)?,
                now()
            ],
        )?;
        Ok(())
    }

    // Research

    pub fn add_research(
        &self,
        repository_id: i64,
        goal: &str,
        measure: &str,
        better: &str,
        token_budget: i64,
    ) -> Result<Research> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO research (repository_id, goal, measure, better, token_budget, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![repository_id, goal, measure, better, token_budget, now()],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        Ok(self.research(id)?.expect("the research was just stored"))
    }

    fn research_where(&self, clause: &str, param: rusqlite::types::Value) -> Result<Vec<Research>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, repository_id, goal, measure, better, token_budget, status, baseline, reason, at
             FROM research WHERE {clause} ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok(Research {
                id: r.get(0)?,
                repository_id: r.get(1)?,
                goal: r.get(2)?,
                measure: r.get(3)?,
                better: r.get(4)?,
                token_budget: r.get(5)?,
                status: r.get(6)?,
                baseline: r.get(7)?,
                reason: r.get(8)?,
                at: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn research(&self, id: i64) -> Result<Option<Research>> {
        Ok(self.research_where("id = ?1", id.into())?.pop())
    }

    /// All research, the newest first.
    pub fn researches(&self) -> Result<Vec<Research>> {
        self.research_where("?1", 1.into())
    }

    /// Gives a research that has one of the statuses `from` the status `to` and the reason it got it; false, and
    /// nothing changed, when it has another status.
    pub fn move_research(&self, id: i64, from: &[&str], to: &str, reason: &str) -> Result<bool> {
        let marks = vec!["?"; from.len()].join(", ");
        let sql = format!(
            "UPDATE research SET status = ?, reason = ? WHERE id = ? AND status IN ({marks})"
        );
        let mut values: Vec<rusqlite::types::Value> =
            vec![to.to_string().into(), reason.to_string().into(), id.into()];
        values.extend(
            from.iter()
                .map(|s| rusqlite::types::Value::from(s.to_string())),
        );
        Ok(self
            .conn()
            .execute(&sql, rusqlite::params_from_iter(values))?
            == 1)
    }

    /// Starts a confirmed research; false, and nothing changed, when it is not confirmed or another one is running,
    /// since only one runs at a time.
    pub fn start_research(&self, id: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE research SET status = 'running'
             WHERE id = ?1 AND status = 'confirmed' AND NOT EXISTS (SELECT 1 FROM research WHERE status = 'running')",
            [id],
        )? == 1)
    }

    /// Sends the research the last server was running back to confirmed.
    pub fn reconfirm_running_research(&self) -> Result<()> {
        self.conn().execute(
            "UPDATE research SET status = 'confirmed' WHERE status = 'running'",
            [],
        )?;
        Ok(())
    }

    pub fn set_baseline(&self, id: i64, baseline: f64) -> Result<()> {
        self.conn().execute(
            "UPDATE research SET baseline = ?1 WHERE id = ?2",
            params![baseline, id],
        )?;
        Ok(())
    }

    /// Records that a run belongs to a research, so that its tokens count against the budget.
    pub fn add_research_run(&self, research_id: i64, run_id: i64) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO research_runs (research_id, run_id) VALUES (?1, ?2)",
            params![research_id, run_id],
        )?;
        Ok(())
    }

    /// The runs of a research, oldest first.
    pub fn runs_of_research(&self, research_id: i64) -> Result<Vec<Run>> {
        self.runs_where(
            "id IN (SELECT run_id FROM research_runs WHERE research_id = ?1)",
            research_id.into(),
        )
    }

    /// The tokens the runs of a research used.
    pub fn research_tokens(&self, research_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(SUM(u.input_tokens + u.output_tokens), 0) FROM usage u
             JOIN research_runs x ON x.run_id = u.run_id WHERE x.research_id = ?1",
            [research_id],
            |r| r.get(0),
        )?)
    }

    pub fn add_experiment(&self, e: &Experiment) -> Result<Experiment> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO experiments (research_id, branch, title, value, checks, kept, reason, change_id, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                e.research_id,
                e.branch,
                e.title,
                e.value,
                e.checks,
                e.kept,
                e.reason,
                e.change_id,
                now()
            ],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        Ok(self
            .experiments(e.research_id)?
            .into_iter()
            .find(|x| x.id == id)
            .expect("the experiment was just stored"))
    }

    /// The experiments of a research, the first first.
    pub fn experiments(&self, research_id: i64) -> Result<Vec<Experiment>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, research_id, branch, title, value, checks, kept, reason, change_id, at
             FROM experiments WHERE research_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([research_id], |r| {
            Ok(Experiment {
                id: r.get(0)?,
                research_id: r.get(1)?,
                branch: r.get(2)?,
                title: r.get(3)?,
                value: r.get(4)?,
                checks: r.get(5)?,
                kept: r.get(6)?,
                reason: r.get(7)?,
                change_id: r.get(8)?,
                at: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn secrets_in_run_events_are_redacted_before_they_are_stored() {
        let (_dir, s) = store();
        let run = s.add_run(None, "Coder", "pi-bolt", "code").unwrap();
        let key = format!("sk-ant-api03-{}", "x".repeat(40));
        let event =
            json!({ "result": { "content": [{ "type": "text", "text": format!("KEY={key}") }] } });
        s.append_event(run.id, "tool_execution_end", &event)
            .unwrap();
        let stored = s.events(run.id).unwrap().pop().unwrap().data;
        assert_eq!(
            stored["result"]["content"][0]["text"],
            "KEY=[redacted:anthropic-key]"
        );
    }

    #[test]
    fn the_schema_is_migrated_once_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            Store::open(dir.path()).unwrap().schema_version().unwrap(),
            MIGRATIONS.len()
        );
        assert_eq!(
            Store::open(dir.path()).unwrap().schema_version().unwrap(),
            MIGRATIONS.len()
        );
    }

    #[test]
    fn a_database_at_schema_one_is_migrated_and_keeps_its_repositories() {
        let dir = tempfile::tempdir().unwrap();
        {
            let conn = Connection::open(dir.path().join("moebius.sqlite")).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute(
                "INSERT INTO repositories (name, path, forge, default_branch) VALUES ('old', '/srv/old', 'none', 'main')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO runs (agent, harness, purpose, status, started_at) VALUES ('Coder', 'pi-bolt', 'code', 'succeeded', 1)",
                [],
            )
            .unwrap();
        }
        let s = Store::open(dir.path()).unwrap();
        assert_eq!(s.schema_version().unwrap(), MIGRATIONS.len());
        assert_eq!(s.run(1).unwrap().unwrap().summary, None);
        let old = &s.repositories().unwrap()[0];
        assert_eq!((old.name.as_str(), old.github.clone()), ("old", None));
        assert!(old.trusted.is_empty());
    }

    #[test]
    fn a_change_may_belong_to_no_repository_and_the_migration_keeps_the_changes_it_had() {
        let dir = tempfile::tempdir().unwrap();
        let before = MIGRATIONS.len() - 1;
        {
            let conn = Connection::open(dir.path().join("moebius.sqlite")).unwrap();
            conn.pragma_update(None, "foreign_keys", true).unwrap();
            for sql in &MIGRATIONS[..before] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", before as i64)
                .unwrap();
            conn.execute_batch(
                "INSERT INTO repositories (name, path, forge, default_branch) VALUES ('old', '/srv/old', 'none', 'main');
                 INSERT INTO changes (repository_id, branch, state, created_at, head) VALUES (1, 'moebius/chat-1', 'open', 1, 'abc');
                 INSERT INTO research (repository_id, goal, measure, better, token_budget, at) VALUES (1, 'g', 'm', 'lower', 1, 1);
                 INSERT INTO experiments (research_id, branch, title, kept, reason, change_id, at) VALUES (1, 'b', 't', 1, 'r', 1, 1);",
            )
            .unwrap();
        }
        let s = Store::open(dir.path()).unwrap();
        let old = s.change(1).unwrap().unwrap();
        assert_eq!(
            (
                old.repository_id,
                old.harness_repository,
                old.head.as_deref()
            ),
            (Some(1), false, Some("abc"))
        );
        let harness = s.add_harness_change("moebius/chat-2", "def").unwrap();
        assert_eq!(
            (harness.repository_id, harness.harness_repository),
            (None, true)
        );
        assert_eq!(s.harness_changes().unwrap(), vec![harness]);
        assert_eq!(s.changes(1).unwrap().len(), 1);
    }

    #[test]
    fn github_repositories_numbered_cards_and_dependencies_round_trip() {
        let (_dir, s) = store();
        let trusted = vec!["leon".to_string()];
        let repo = s
            .add_github_repository("widgets", "/srv/widgets", "main", "acme/widgets", &trusted)
            .unwrap();
        assert_eq!(s.repository(repo.id).unwrap(), Some(repo.clone()));
        assert_eq!(
            (repo.forge.as_str(), repo.github.as_deref(), &repo.trusted),
            ("github", Some("acme/widgets"), &trusted)
        );

        let card = s
            .add_numbered_card(repo.id, 41, "Issue", "body", "leon", "Backlog")
            .unwrap();
        assert_eq!(s.card_by_number(repo.id, 41).unwrap(), Some(card.clone()));
        assert_eq!(s.card_by_number(repo.id, 42).unwrap(), None);
        s.set_card_text(card.id, "Renamed", "new body").unwrap();
        let stored = s.card(card.id).unwrap().unwrap();
        assert_eq!(
            (stored.title.as_str(), stored.body.as_str()),
            ("Renamed", "new body")
        );

        s.set_dependencies(card.id, &BTreeSet::from([9, 3]))
            .unwrap();
        assert_eq!(s.dependencies(card.id).unwrap(), [3, 9]);
        s.set_dependencies(card.id, &BTreeSet::from([9, 12]))
            .unwrap();
        assert_eq!(s.dependencies(card.id).unwrap(), [9, 12]);
    }

    #[test]
    fn a_note_stays_on_the_card_until_a_code_run_succeeds() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("radiance", "/srv/radiance", "none", "main")
            .unwrap();
        let card = s.add_card(repo.id, "First", "", "leon", "Backlog").unwrap();
        assert_eq!(card.note, None);
        s.set_card_note(card.id, "Fix the lint.").unwrap();
        let note = || s.card(card.id).unwrap().unwrap().note;
        assert_eq!(note().as_deref(), Some("Fix the lint."));

        let review = s.add_run(Some(card.id), "Reviewer", "x", "review").unwrap();
        s.set_run_status(review.id, "succeeded").unwrap();
        let failed = s.add_run(Some(card.id), "Coder", "x", "code").unwrap();
        s.set_run_status(failed.id, "failed").unwrap();
        assert_eq!(note().as_deref(), Some("Fix the lint."));

        let coded = s.add_run(Some(card.id), "Coder", "x", "code").unwrap();
        s.set_run_status(coded.id, "succeeded").unwrap();
        assert_eq!(note(), None);
    }

    #[test]
    fn records_round_trip() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("radiance", "/srv/radiance", "none", "main")
            .unwrap();
        assert_eq!(s.repository(repo.id).unwrap(), Some(repo.clone()));

        let a = s
            .add_card(repo.id, "First", "body", "leon", "Backlog")
            .unwrap();
        let b = s
            .add_card(repo.id, "Second", "", "leon", "Backlog")
            .unwrap();
        assert_eq!((a.number, b.number), (1, 2));
        s.move_card(a.id, "Ready").unwrap();
        assert_eq!(s.card(a.id).unwrap().unwrap().column, "Ready");
        let moves = s.moves_for_card(a.id).unwrap();
        assert_eq!(
            moves
                .iter()
                .map(|m| (m.from.as_str(), m.to.as_str()))
                .collect::<Vec<_>>(),
            [("Backlog", "Ready")]
        );
        assert!(s.moves_for_card(b.id).unwrap().is_empty());
        assert_eq!(s.cards(repo.id).unwrap().len(), 2);

        let change = s
            .add_change(repo.id, Some(a.id), "moebius/1", None, "abc")
            .unwrap();
        s.set_change_state(change.id, "merged").unwrap();
        assert_eq!(s.change(change.id).unwrap().unwrap().state, "merged");

        let pull = s
            .add_change(repo.id, Some(b.id), "moebius/2", Some(9), "def")
            .unwrap();
        assert_eq!(
            s.open_pull_requests(repo.id).unwrap(),
            std::slice::from_ref(&pull)
        );
        s.set_change_status(
            pull.id,
            Some("failure"),
            "def",
            Some("lint: failure"),
            Some("approved"),
        )
        .unwrap();
        let stored = s.change(pull.id).unwrap().unwrap();
        assert_eq!(
            (
                stored.pull_request,
                stored.checks.as_deref(),
                stored.review.as_deref()
            ),
            (Some(9), Some("failure"), Some("approved"))
        );
        assert_eq!(
            (
                stored.checks_head.as_deref(),
                stored.failed_checks.as_deref()
            ),
            (Some("def"), Some("lint: failure"))
        );
        s.set_change_state(pull.id, "closed").unwrap();
        assert!(s.open_pull_requests(repo.id).unwrap().is_empty());

        let run = s.add_run(Some(a.id), "Coder", "pi-bolt", "code").unwrap();
        s.set_run_worktree(run.id, "/srv/wt/1").unwrap();
        s.set_run_status(run.id, "succeeded").unwrap();
        let stored = s.run(run.id).unwrap().unwrap();
        assert_eq!(
            (stored.status.as_str(), stored.worktree.as_deref()),
            ("succeeded", Some("/srv/wt/1"))
        );
        assert!(stored.ended_at.is_some());

        assert_eq!(
            s.append_event(run.id, "text", &json!({"delta": "hi"}))
                .unwrap(),
            1
        );
        assert_eq!(
            s.append_event(run.id, "tool_call", &json!({"tool": "read"}))
                .unwrap(),
            2
        );
        let events = s.events(run.id).unwrap();
        assert_eq!(
            events.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
            ["text", "tool_call"]
        );
        assert_eq!(events[0].data, json!({"delta": "hi"}));
        assert_eq!(s.events_after(run.id, 1, 5).unwrap(), events[1..]);
        assert_eq!(s.events_after(run.id, 0, 1).unwrap(), events[..1]);

        let blob = s.blobs().put("the context").unwrap();
        let j = s
            .add_judgement(&NewJudgement {
                kind: "tool_call".into(),
                context: vec![blob.clone()],
                options: vec!["allow".into(), "refuse".into()],
                choice: "allow".into(),
                reason: "no rule applies".into(),
                decider: "pattern".into(),
                run_id: Some(run.id),
                card_id: Some(a.id),
            })
            .unwrap();
        s.set_outcome(j.id, "right").unwrap();
        let stored = s.judgement(j.id).unwrap().unwrap();
        assert_eq!(stored.context, vec![blob]);
        assert_eq!(stored.outcome.as_deref(), Some("right"));
        assert_eq!(s.judgements("tool_call").unwrap().len(), 1);
        assert_eq!(s.judgements_for_card(a.id).unwrap().len(), 1);

        let item = s
            .add_inbox_item(
                "ready_to_merge",
                Some(a.id),
                "First",
                &json!({"change": change.id}),
            )
            .unwrap();
        assert_eq!(s.inbox().unwrap(), vec![item.clone()]);
        s.resolve_inbox_item(item.id).unwrap();
        assert!(s.inbox().unwrap().is_empty());

        let u = Usage {
            run_id: Some(run.id),
            model: "local".into(),
            input_tokens: 100,
            output_tokens: 20,
            cached_tokens: 50,
            duration_ms: 0,
            tools: Vec::new(),
        };
        s.add_usage(&u).unwrap();
        s.add_usage(&u).unwrap();
        assert_eq!(s.usage_for_run(run.id).unwrap(), vec![u.clone(), u]);
        assert_eq!(s.tokens_for_card(a.id).unwrap(), 240);
    }

    #[test]
    fn a_retry_forgets_the_tokens_used_before_it_and_today_counts_only_today() {
        let (_dir, s) = store();
        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        let card = s.add_card(repo.id, "A", "", "leon", "Backlog").unwrap();
        let used = |run: &Run, tokens| {
            s.add_usage(&Usage {
                run_id: Some(run.id),
                model: "local".into(),
                input_tokens: tokens,
                output_tokens: 1,
                cached_tokens: 0,
                duration_ms: 0,
                tools: Vec::new(),
            })
            .unwrap();
        };
        let first = s.add_run(Some(card.id), "Coder", "pi", "code").unwrap();
        used(&first, 99);
        assert_eq!(s.tokens_for_card(card.id).unwrap(), 100);
        assert_eq!(s.card(card.id).unwrap().unwrap().retried_after, None);

        s.set_card_retried(card.id).unwrap();
        assert_eq!(
            s.card(card.id).unwrap().unwrap().retried_after,
            Some(first.id)
        );
        assert_eq!(s.tokens_for_card(card.id).unwrap(), 0);
        let second = s.add_run(Some(card.id), "Coder", "pi", "code").unwrap();
        used(&second, 9);
        assert_eq!(s.tokens_for_card(card.id).unwrap(), 10);

        // Usage from before the start of today is not today's.
        assert_eq!(s.tokens_today().unwrap(), 110);
        s.conn()
            .execute(
                "UPDATE usage SET at = at - 2 * 86400 WHERE run_id = ?1",
                [first.id],
            )
            .unwrap();
        assert_eq!(s.tokens_today().unwrap(), 10);
    }

    #[test]
    fn model_time_is_split_over_what_each_call_did() {
        let (_dir, s) = store();
        let add = |tools: &[&str], output_tokens, duration_ms| {
            s.add_usage(&Usage {
                run_id: None,
                model: "m".into(),
                input_tokens: 1,
                output_tokens,
                cached_tokens: 0,
                duration_ms,
                tools: tools.iter().map(|t| t.to_string()).collect(),
            })
            .unwrap();
        };
        add(&["bash"], 10, 1000);
        add(&["bash", "read"], 21, 3001);
        add(&["read", "read"], 4, 400);
        add(&[], 7, 500);
        // Two days ago, and before the tools of calls were recorded.
        add(&["edit"], 5, 9000);
        s.conn()
            .execute("UPDATE usage SET at = at - 2 * 86400 WHERE id = 5", [])
            .unwrap();
        let activity = |activity: &str, model_ms, calls, output_tokens| ActivityUsage {
            activity: activity.into(),
            model_ms,
            calls,
            output_tokens,
        };

        // The odd millisecond and token of the second call go to its first tool; a call that names a tool twice
        // counts once for it but gives it both shares.
        assert_eq!(
            s.usage_by_activity(1).unwrap(),
            [
                activity("bash", 2501, 2, 21),
                activity("read", 1900, 2, 14),
                activity(ANSWERING, 500, 1, 7),
            ]
        );
        assert_eq!(
            s.usage_by_activity(3).unwrap()[0],
            activity("edit", 9000, 1, 5)
        );

        // Calls from before the tools were recorded are left out while there are others, and listed alone otherwise.
        s.conn()
            .execute("UPDATE usage SET tools = NULL WHERE id = 1", [])
            .unwrap();
        assert_eq!(s.usage_by_activity(1).unwrap().len(), 3);
        assert!(
            !s.usage_by_activity(1)
                .unwrap()
                .iter()
                .any(|a| a.activity == UNKNOWN)
        );
        s.conn()
            .execute("UPDATE usage SET tools = NULL", [])
            .unwrap();
        assert_eq!(
            s.usage_by_activity(1).unwrap(),
            [activity(UNKNOWN, 4901, 4, 42)]
        );
    }

    #[test]
    fn usage_is_counted_by_window_day_model_agent_and_tool() {
        let (_dir, s) = store();
        let coder = s.add_run(None, "Coder", "pi", "code").unwrap().id;
        let reviewer = s.add_run(None, "Reviewer", "pi", "review").unwrap().id;
        let add = |run: Option<i64>, model: &str, input, output, cached, duration_ms| {
            s.add_usage(&Usage {
                run_id: run,
                model: model.into(),
                input_tokens: input,
                output_tokens: output,
                cached_tokens: cached,
                duration_ms,
                tools: Vec::new(),
            })
            .unwrap();
        };
        add(Some(coder), "big", 1000, 100, 600, 4000);
        add(Some(coder), "big", 500, 50, 0, 2000);
        add(Some(reviewer), "small", 200, 20, 100, 1000);
        add(None, "big", 10, 0, 0, 500);
        // Two days ago a smaller model led.
        add(Some(reviewer), "small", 900, 0, 0, 3000);
        add(Some(coder), "big", 100, 0, 0, 100);
        s.conn()
            .execute("UPDATE usage SET at = at - 2 * 86400 WHERE id > 4", [])
            .unwrap();
        let day = |ago: i64| -> String {
            s.conn()
                .query_row(
                    "SELECT strftime('%Y-%m-%d', 'now', 'localtime', ?1)",
                    [format!("-{ago} days")],
                    |r| r.get(0),
                )
                .unwrap()
        };

        assert_eq!(s.today().unwrap(), day(0));
        assert_eq!(
            s.usage_totals(1).unwrap(),
            UsageTotals {
                tokens: 1880,
                calls: 4,
                input_tokens: 1710,
                cached_tokens: 700,
                model_ms: 7500
            }
        );
        assert_eq!(s.tokens_today().unwrap(), 1880);
        let three = s.usage_totals(3).unwrap();
        assert_eq!(
            (three.tokens, three.calls, three.model_ms),
            (2880, 6, 10600)
        );

        let model = |model: &str, tokens, calls, input_tokens, cached_tokens| ModelUsage {
            model: model.into(),
            tokens,
            calls,
            input_tokens,
            cached_tokens,
        };
        assert_eq!(
            s.usage_by_model(1).unwrap(),
            [
                model("big", 1660, 3, 1510, 600),
                model("small", 220, 1, 200, 100)
            ]
        );
        assert_eq!(
            s.usage_by_model(3).unwrap(),
            [
                model("big", 1760, 4, 1610, 600),
                model("small", 1120, 2, 1100, 100)
            ]
        );

        let agent =
            |agent: &str, calls, tokens, input_tokens, cached_tokens, model_ms| AgentUsage {
                agent: agent.into(),
                calls,
                tokens,
                input_tokens,
                cached_tokens,
                model_ms,
            };
        assert_eq!(
            s.usage_by_agent(1).unwrap(),
            [
                agent("Coder", 2, 1650, 1500, 600, 6000),
                agent("Reviewer", 1, 220, 200, 100, 1000),
                agent(NO_RUN, 1, 10, 10, 0, 500),
            ]
        );

        let (old, today) = (day(2), day(0));
        let by_day =
            |date: &str, model: &str, tokens| (date.to_string(), model.to_string(), tokens);
        assert_eq!(
            s.usage_by_day(365).unwrap(),
            [
                by_day(&old, "big", 100),
                by_day(&old, "small", 900),
                by_day(&today, "big", 1660),
                by_day(&today, "small", 220),
            ]
        );
        assert_eq!(s.usage_by_day(1).unwrap().len(), 2);

        // A call waits from its start event to its end event; a call that has not ended waits for nothing yet.
        let call = |id: &str, tool: &str, seconds: i64| {
            let start = json!({"toolCallId": id, "toolName": tool});
            let begun = s
                .append_event(coder, "tool_execution_start", &start)
                .unwrap();
            let end = s.append_event(coder, "tool_execution_end", &start).unwrap();
            // The end is set from the start, so that a second passing between the two appends changes nothing.
            s.conn()
                .execute(
                    "UPDATE events SET at = (SELECT at FROM events WHERE run_id = ?2 AND seq = ?4) + ?1 \
                     WHERE run_id = ?2 AND seq = ?3",
                    params![seconds, coder, end, begun],
                )
                .unwrap();
        };
        call("c1", "bash", 60);
        call("c2", "bash", 30);
        call("c3", "edit", 5);
        call("c4", "read", 0);
        s.append_event(
            coder,
            "tool_execution_start",
            &json!({"toolCallId": "c5", "toolName": "grep"}),
        )
        .unwrap();
        let wait = |tool: &str, calls, seconds| ToolWait {
            tool: tool.into(),
            calls,
            seconds,
        };
        assert_eq!(
            s.tool_waits(1, 10).unwrap(),
            [wait("bash", 2, 90), wait("edit", 1, 5), wait("read", 1, 0)]
        );
        assert_eq!(s.tool_waits(1, 2).unwrap().len(), 2);
    }

    #[test]
    fn a_retro_collects_the_errors_of_one_local_day_and_nothing_at_or_after_its_end() {
        use crate::retro::report;
        let (_dir, s) = store();
        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        let card = s.add_card(repo.id, "Card", "", "leon", "Backlog").unwrap();
        let (from, to) = s.day_window("2026-03-10").unwrap().unwrap();
        // A local day is 23 to 25 hours long where the clocks change.
        assert!((23 * 3600..=25 * 3600).contains(&(to - from)));
        assert_eq!(s.day_window("2026-02-30").unwrap(), None);
        assert_eq!(s.day_window("today").unwrap(), None);

        let run = s
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        let result = |text: &str| json!({ "toolName": "bash", "isError": true, "result": { "content": [{ "type": "text", "text": text }] } });
        let at = |s: &Store, seq: i64, at: i64| {
            s.conn()
                .execute(
                    "UPDATE events SET at = ?1 WHERE run_id = ?2 AND seq = ?3",
                    params![at, run.id, seq],
                )
                .unwrap();
        };
        // seq 1..4: failed tool calls at the first moment, one second before, at the end, one second before it.
        for (seq, when) in [(1, from), (2, from - 1), (3, to), (4, to - 1)] {
            s.append_event(
                run.id,
                "tool_execution_end",
                &result(&format!("failed {seq}")),
            )
            .unwrap();
            at(&s, seq, when);
        }
        // A call that went well, and one that Moebius refused (which is a refusal, not a failure).
        s.append_event(
            run.id,
            "tool_execution_end",
            &json!({ "toolName": "bash", "isError": false }),
        )
        .unwrap();
        s.append_event(
            run.id,
            "tool_execution_end",
            &result("Moebius refused this call: no-rm"),
        )
        .unwrap();
        let refusal = |decision: &str| json!({ "tool": "bash", "decision": decision, "reason": "no-rm: rm -rf" });
        s.append_event(run.id, "tool_check", &refusal("refuse"))
            .unwrap();
        s.append_event(run.id, "tool_check", &refusal("allow"))
            .unwrap();
        s.append_event(
            run.id,
            "steered",
            &json!({ "message": "use the other file" }),
        )
        .unwrap();
        for seq in 5..=10 {
            at(&s, seq, from + 100 + seq);
        }

        // A run that failed with a long message, a stopped one at the end of the day (the next day's), and a retro run.
        let failed = s.add_run(None, "Reviewer", "pi-bolt", "review").unwrap();
        s.append_event(
            failed.id,
            "moebius_error",
            &json!({ "message": "x".repeat(600) }),
        )
        .unwrap();
        s.set_run_status(failed.id, "failed").unwrap();
        let stopped = s
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        s.set_run_status(stopped.id, "stopped").unwrap();
        let analyst = s
            .add_run(None, "Retro-analyst", "pi-bolt", "retro")
            .unwrap();
        s.set_run_status(analyst.id, "failed").unwrap();
        for (id, ended) in [
            (failed.id, from + 300),
            (stopped.id, to),
            (analyst.id, from + 301),
        ] {
            s.conn()
                .execute(
                    "UPDATE runs SET ended_at = ?1 WHERE id = ?2",
                    params![ended, id],
                )
                .unwrap();
        }

        // Cards sent back from review, one for each note, and moves that are not.
        for (note, when) in [
            ("The checks failed:\ncargo test", from + 400),
            ("The change no longer merges cleanly into main.", from + 401),
            ("The test is missing.", from + 402),
            ("Too late.", to),
        ] {
            s.set_card_note(card.id, note).unwrap();
            s.move_card(card.id, "In review").unwrap();
            s.move_card(card.id, "In progress").unwrap();
            s.conn()
                .execute(
                    "UPDATE card_moves SET at = ?1 WHERE note = ?2 AND to_column = 'In progress'",
                    params![when, note],
                )
                .unwrap();
            s.conn()
                .execute(
                    "UPDATE card_moves SET at = ?1 WHERE to_column = 'In review'",
                    params![from - 5],
                )
                .unwrap();
        }

        let first = report(&s, "2026-03-10").unwrap();
        let seen: Vec<_> = first
            .errors
            .iter()
            .map(|e| {
                (
                    e.kind.as_str(),
                    e.run,
                    e.card,
                    e.agent.as_deref(),
                    e.text.chars().take(20).collect(),
                )
            })
            .collect();
        let (r, c) = (Some(run.id), Some(card.id));
        assert_eq!(
            seen,
            [
                ("tool_failed", r, c, Some("Coder"), "bash: failed 1".into()),
                (
                    "tool_refused",
                    r,
                    c,
                    Some("Coder"),
                    "bash: no-rm: rm -rf".into()
                ),
                ("steered", r, c, Some("Coder"), "use the other file".into()),
                (
                    "run_failed",
                    Some(failed.id),
                    None,
                    Some("Reviewer"),
                    "x".repeat(20)
                ),
                (
                    "checks_failed",
                    None,
                    c,
                    None,
                    "The checks failed:\nc".into()
                ),
                ("conflict", None, c, None, "The change no longer".into()),
                (
                    "review_findings",
                    None,
                    c,
                    None,
                    "The test is missing.".into()
                ),
                ("tool_failed", r, c, Some("Coder"), "bash: failed 4".into()),
            ]
        );
        // The failed call at the first moment of the day is the first; the long text is shortened.
        assert_eq!(first.errors[3].text.chars().count(), 501);
        assert!(first.errors[3].text.ends_with('…'));
        assert!(first.errors.iter().all(|e| e.run != Some(analyst.id)));
        // The next day has a call that failed, the run stopped and the card sent back at its first moment.
        let next = report(&s, "2026-03-11").unwrap();
        let kinds: Vec<&str> = next.errors.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["tool_failed", "run_stopped", "review_findings"]);
        assert!(report(&s, "soon").is_err());
    }

    #[test]
    fn retros_and_their_proposals_round_trip_and_a_day_has_one_retro() {
        let (_dir, s) = store();
        let report = json!({ "date": "2026-03-10", "errors": [] });
        assert!(s.add_retro("2026-03-10", &report, "analysing").unwrap());
        assert!(!s.add_retro("2026-03-10", &json!({}), "reported").unwrap());
        assert!(s.add_retro("2026-03-11", &report, "reported").unwrap());
        s.finish_retro("2026-03-10", "proposed", &["a warning".to_string()])
            .unwrap();
        s.add_proposal(
            "2026-03-10",
            "proposal/2026-03-10-x",
            "Title",
            "Body",
            &["Coder".to_string()],
            &[48, 50],
        )
        .unwrap();
        let retro = s.retro("2026-03-10").unwrap().unwrap();
        assert_eq!(
            (retro.status.as_str(), retro.warnings),
            ("proposed", vec!["a warning".to_string()])
        );
        assert_eq!(retro.report, report);
        let dates: Vec<String> = s.retros().unwrap().into_iter().map(|r| r.date).collect();
        assert_eq!(dates, ["2026-03-11", "2026-03-10"]);
        let proposals = s.proposals(None).unwrap();
        assert_eq!(proposals, s.proposals(Some("2026-03-10")).unwrap());
        assert_eq!(
            (proposals[0].status.as_str(), &proposals[0].agents[..]),
            ("open", &["Coder".to_string()][..])
        );
        assert_eq!(proposals[0].cards, [48, 50]);
        assert!(proposals[0].trials.is_empty());
        assert!(s.proposals(Some("2026-03-11")).unwrap().is_empty());
        s.add_retro("2026-03-12", &report, "analysing").unwrap();
        s.fail_analysing_retros("stopped").unwrap();
        assert_eq!(s.retro("2026-03-12").unwrap().unwrap().status, "failed");
        assert_eq!(s.retro("2026-03-11").unwrap().unwrap().status, "reported");
    }

    #[test]
    fn one_proposal_is_tried_at_a_time_and_a_trial_that_was_not_seen_through_is_forgotten() {
        let (_dir, s) = store();
        s.add_retro("2026-03-10", &json!({}), "proposed").unwrap();
        for branch in ["a", "b"] {
            s.add_proposal("2026-03-10", branch, branch, "", &[], &[])
                .unwrap();
        }
        assert!(s.start_trying(1).unwrap());
        // Another one is being tried, and this one is not open any more.
        assert!(!s.start_trying(2).unwrap());
        assert!(!s.start_trying(1).unwrap());
        s.add_trial(1, None, None, "skipped", "no cards", &json!({ "a": 1 }))
            .unwrap();
        let trials = s.proposal(1).unwrap().unwrap().trials;
        assert_eq!(
            (trials[0].outcome.as_str(), &trials[0].measurements),
            ("skipped", &json!({ "a": 1 }))
        );
        s.reopen_trying_proposals().unwrap();
        let proposal = s.proposal(1).unwrap().unwrap();
        assert_eq!(proposal.status, "open");
        assert!(proposal.trials.is_empty());
        assert!(s.start_trying(2).unwrap());
        assert!(s.proposal(3).unwrap().is_none());
    }

    #[test]
    fn a_trial_run_is_no_part_of_its_cards_work_nor_of_the_days_report() {
        let (_dir, s) = store();
        let repository = s.add_repository("demo", "/demo", "none", "main").unwrap();
        let card = s
            .add_card(repository.id, "Task", "", "person", "Ready")
            .unwrap();
        let code = s.add_run(Some(card.id), "Coder", "pi", "code").unwrap();
        let trial = s.add_run(Some(card.id), "Coder", "pi", "trial").unwrap();
        for run in [code.id, trial.id] {
            s.set_run_status(run, "failed").unwrap();
            s.append_event(run, "moebius_error", &json!({ "message": "boom" }))
                .unwrap();
            s.add_usage(&Usage {
                run_id: Some(run),
                model: "m".into(),
                input_tokens: 10,
                output_tokens: 5,
                cached_tokens: 0,
                duration_ms: 1,
                tools: Vec::new(),
            })
            .unwrap();
        }
        let runs: Vec<i64> = s
            .runs_for_card(card.id)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(runs, [code.id]);
        assert_eq!(s.tokens_for_card(card.id).unwrap(), 15);
        // The day's report leaves the trial out; asked for the run, the same sources find it.
        let day = s.today().unwrap();
        let (from, to) = s.day_window(&day).unwrap().unwrap();
        let day_runs: Vec<_> = s.failed_runs(from, to + 86400, None).unwrap();
        assert_eq!(
            day_runs.iter().map(|f| f.run_id).collect::<Vec<_>>(),
            [Some(code.id)]
        );
        let of_trial = s.failed_runs(0, i64::MAX, Some(trial.id)).unwrap();
        assert_eq!(of_trial.len(), 1);
        assert_eq!(of_trial[0].run_id, Some(trial.id));
        assert_eq!(of_trial[0].text, "boom");
    }

    #[test]
    fn one_research_runs_at_a_time_and_only_a_confirmed_one_starts() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/tmp/demo", "main", "none")
            .unwrap();
        let [a, b] = [1, 2].map(|_| {
            s.add_research(repo.id, "goal", "echo 1", "lower", 10)
                .unwrap()
                .id
        });
        assert!(!s.start_research(a).unwrap());
        for id in [a, b] {
            assert!(s.move_research(id, &["draft"], "confirmed", "").unwrap());
        }
        assert!(!s.move_research(a, &["draft"], "confirmed", "").unwrap());
        assert!(s.start_research(a).unwrap());
        assert!(!s.start_research(b).unwrap());
        assert!(
            s.move_research(a, &["running"], "spent", "used up")
                .unwrap()
        );
        assert!(s.start_research(b).unwrap());
        let spent = s.research(a).unwrap().unwrap();
        assert_eq!(
            (spent.status.as_str(), spent.reason.as_str()),
            ("spent", "used up")
        );
        // The server that was running b died.
        s.reconfirm_running_research().unwrap();
        assert_eq!(s.research(b).unwrap().unwrap().status, "confirmed");
        assert_eq!(s.research(a).unwrap().unwrap().status, "spent");
    }

    #[test]
    fn a_confirmed_card_has_its_dependencies_from_the_start_and_adopts_a_card_of_the_poll() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/tmp/demo", "none", "main")
            .unwrap();
        let blockers = BTreeSet::from([4]);
        let first = s
            .add_confirmed_card(
                repo.id,
                None,
                "First",
                "",
                "person",
                "Backlog",
                &BTreeSet::new(),
            )
            .unwrap();
        let second = s
            .add_confirmed_card(repo.id, None, "Second", "", "person", "Backlog", &blockers)
            .unwrap();
        assert_eq!((first.number, second.number), (1, 2));
        assert!(first.confirmed && second.confirmed);
        assert_eq!(s.dependencies(second.id).unwrap(), [4]);
        assert!(
            !s.add_card(repo.id, "Other", "", "leon", "Backlog")
                .unwrap()
                .confirmed
        );

        // The poll made a card for the issue just opened: it is confirmed instead of added again.
        let polled = s
            .add_numbered_card(repo.id, 40, "Polled", "", "bot", "Backlog")
            .unwrap();
        assert!(!polled.confirmed);
        let adopted = s
            .add_confirmed_card(
                repo.id,
                Some(40),
                "Polled",
                "",
                "person",
                "Backlog",
                &blockers,
            )
            .unwrap();
        assert_eq!(adopted.id, polled.id);
        assert!(adopted.confirmed);
        assert_eq!(s.dependencies(polled.id).unwrap(), [4]);
    }

    #[test]
    fn a_grilling_is_stored_with_its_issues_and_changes_status_once() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/tmp/demo", "none", "main")
            .unwrap();
        let run = s
            .add_run(None, "Grilling-partner", "pi-bolt", "chat")
            .unwrap();
        let issues = [GrillingIssue {
            title: "One".into(),
            body: "Body".into(),
            blocked_by: vec![],
        }];
        let grilling = s
            .add_grilling(run.id, repo.id, &issues, None, "proposed", "")
            .unwrap();
        assert_eq!(s.grilling(grilling.id).unwrap().unwrap().issues, issues);
        assert!(
            s.move_grilling(grilling.id, &["proposed"], "confirmed")
                .unwrap()
        );
        assert!(
            !s.move_grilling(grilling.id, &["proposed"], "dismissed")
                .unwrap()
        );
        assert_eq!(s.grillings().unwrap()[0].status, "confirmed");
    }

    #[test]
    fn shadows_are_counted_per_kind_and_a_failure_disagrees() {
        let (_dir, s) = store();
        let judge = |kind: &str, choice: &str| {
            s.add_judgement(&NewJudgement {
                kind: kind.into(),
                choice: choice.into(),
                ..Default::default()
            })
            .unwrap()
            .id
        };
        let shadow = |id: i64, kind: &str, choice: Option<&str>| {
            s.add_shadow(&NewShadow {
                judgement_id: id,
                kind: kind.into(),
                choice: choice.map(String::from),
                reason: String::new(),
                failed: choice.is_none(),
            })
            .unwrap();
        };
        let (a, b, c, d) = (
            judge("a", "yes"),
            judge("a", "no"),
            judge("a", "yes"),
            judge("b", "yes"),
        );
        shadow(a, "a", Some("yes"));
        shadow(b, "a", Some("yes"));
        shadow(c, "a", None);
        shadow(d, "b", Some("yes"));
        let stats = s.shadow_stats(None).unwrap();
        assert_eq!(
            stats
                .iter()
                .map(|k| (k.kind.as_str(), k.shadows, k.agreed, k.failed))
                .collect::<Vec<_>>(),
            [("a", 3, 1, 1), ("b", 1, 1, 0)]
        );
        assert_eq!(s.shadow_stats(Some("b")).unwrap().len(), 1);
        assert!(s.shadow_stats(Some("c")).unwrap().is_empty());
    }

    #[test]
    fn a_takeover_changes_status_from_the_one_it_has() {
        let (_dir, s) = store();
        let t = s.add_takeover("triage", 0.97, 120).unwrap();
        assert_eq!(
            (t.status.as_str(), s.takeovers().unwrap().len()),
            ("open", 1)
        );
        assert!(!s.move_takeover(t.id, "merged", "released").unwrap());
        assert!(s.move_takeover(t.id, "open", "merged").unwrap());
        assert!(!s.move_takeover(t.id, "open", "dismissed").unwrap());
        assert_eq!(s.takeover(t.id).unwrap().unwrap().status, "merged");
        assert_eq!(s.takeover(t.id + 1).unwrap(), None);
    }

    #[test]
    fn refusals_are_counted_by_the_rule_named_in_the_reason() {
        let (_dir, s) = store();
        let judge = |kind: &str, choice: &str, reason: &str, decider: &str| {
            s.add_judgement(&NewJudgement {
                kind: kind.into(),
                choice: choice.into(),
                reason: reason.into(),
                decider: decider.into(),
                ..Default::default()
            })
            .unwrap();
        };
        judge(
            "tool_call",
            "refuse",
            "no-force-push: it rewrites",
            "guard/patterns",
        );
        judge(
            "tool_call",
            "refuse",
            "no-force-push: again",
            "guard/patterns",
        );
        judge(
            "tool_call",
            "refuse",
            "leave-ci-alone: not .github",
            "guard/teacher",
        );
        judge(
            "tool_call",
            "refuse",
            "teacher: the answer is not JSON",
            "guard/teacher",
        );
        judge(
            "tool_call",
            "refuse",
            "the rules cannot be read, so no call is allowed: x: y",
            "guard/patterns",
        );
        judge(
            "tool_call",
            "refuse",
            "no teacher is configured to check the rules in prose",
            "guard/teacher",
        );
        judge(
            "tool_call",
            "refuse",
            "no-force-push: by a model",
            "decision-model",
        );
        judge(
            "tool_call",
            "allow",
            "no-force-push: allowed",
            "guard/patterns",
        );
        judge(
            "other",
            "refuse",
            "no-force-push: other kind",
            "guard/patterns",
        );
        let counted = s.refusals_by_rule(0).unwrap();
        assert_eq!(
            counted,
            BTreeMap::from([
                ("leave-ci-alone".to_string(), 1),
                ("no-force-push".to_string(), 2)
            ])
        );
        assert!(s.refusals_by_rule(now() + 10).unwrap().is_empty());
    }
}
