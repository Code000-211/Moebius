//! The store: records in SQLite and large content in the blob store (see `docs/spec.md`, section 3.1).
//!
//! This module opens the database, migrates its schema and holds what every kind of record shares: the connection,
//! the clock and the helpers that read rows. Each kind of record has a module of its own, with its types and the
//! methods of `Store` that read and write it: `cards` (repositories, cards, dependencies and moves), `changes`, `runs`,
//! `events` (a run's events), `chats`, `judgements`, `replays`, `inbox`, `grillings`, `shadows` (the decision model's
//! shadows and takeovers), `usage`, `found` (what the Retro collects), `retros` (Retros, proposals and trials) and
//! `research` (research, its runs and experiments). Every public type is re-exported here, as `store::Card` and the
//! like.

mod cards;
mod changes;
mod chats;
mod events;
mod found;
mod grillings;
mod inbox;
mod judgements;
mod replays;
mod research;
mod retros;
mod runs;
mod shadows;
mod usage;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Params, Row};

use crate::blobs::BlobStore;

pub use cards::{Card, CardMove, Repository};
pub use changes::Change;
pub use events::Event;
pub use found::Found;
pub use grillings::{Grilling, GrillingIssue};
pub use inbox::InboxItem;
pub use judgements::{Judgement, NewJudgement};
pub use replays::Replay;
pub use research::{Experiment, Research};
pub use retros::{Proposal, Retro, Trial};
pub use runs::Run;
pub use shadows::{NewShadow, ShadowStats, Takeover};
pub use usage::{
    ANSWERING, ActivityUsage, AgentUsage, ModelUsage, NO_RUN, ToolWait, UNKNOWN, Usage, UsageTotals,
};

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
    "ALTER TABLE runs ADD COLUMN origin TEXT NOT NULL DEFAULT 'chat';",
];

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
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

    /// The rows of `sql` with `params`, each read by `row`.
    fn rows<T>(
        &self,
        sql: &str,
        params: impl Params,
        row: impl FnMut(&Row) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Sets `set` (with `values`) on the row `id` of `table` while its status is one of `from`; false, and nothing
    /// changed, when it has another.
    fn move_status(
        &self,
        table: &str,
        set: &str,
        mut values: Vec<rusqlite::types::Value>,
        id: i64,
        from: &[&str],
    ) -> Result<bool> {
        let marks = vec!["?"; from.len()].join(", ");
        let sql = format!("UPDATE {table} SET {set} WHERE id = ? AND status IN ({marks})");
        values.push(id.into());
        values.extend(
            from.iter()
                .map(|s| rusqlite::types::Value::from(s.to_string())),
        );
        Ok(self
            .conn()
            .execute(&sql, rusqlite::params_from_iter(values))?
            == 1)
    }

    /// The date of today on the server, as `YYYY-MM-DD`.
    pub fn today(&self) -> Result<String> {
        Ok(self
            .conn()
            .query_row("SELECT strftime('%Y-%m-%d', 'now', 'localtime')", [], |r| {
                r.get(0)
            })?)
    }

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
}

/// A store in a temporary directory, for the tests of every kind of record.
#[cfg(test)]
fn test_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::test_store as store;
    use super::*;

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
}
