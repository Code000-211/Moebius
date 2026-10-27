//! The store: records in SQLite and large content in the blob store (see `docs/spec.md`, section 3.1).

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
const MIGRATIONS: &[&str] = &[r#"
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
"#];

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub id: i64,
    pub repository_id: i64,
    pub card_id: Option<i64>,
    pub branch: String,
    pub state: String,
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
}

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
        let conn = self.conn();
        conn.execute(
            "INSERT INTO repositories (name, path, forge, default_branch) VALUES (?1, ?2, ?3, ?4)",
            params![name, path, forge, default_branch],
        )?;
        Ok(Repository {
            id: conn.last_insert_rowid(),
            name: name.into(),
            path: path.into(),
            forge: forge.into(),
            default_branch: default_branch.into(),
        })
    }

    pub fn repositories(&self) -> Result<Vec<Repository>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, path, forge, default_branch FROM repositories ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Repository {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
                forge: r.get(3)?,
                default_branch: r.get(4)?,
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
        })
    }

    fn cards_where(&self, clause: &str, param: i64) -> Result<Vec<Card>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, repository_id, number, title, body, author, column_name, created_at FROM cards WHERE {clause} ORDER BY id"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok(Card {
                id: r.get(0)?,
                repository_id: r.get(1)?,
                number: r.get(2)?,
                title: r.get(3)?,
                body: r.get(4)?,
                author: r.get(5)?,
                column: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn card(&self, id: i64) -> Result<Option<Card>> {
        Ok(self.cards_where("id = ?1", id)?.pop())
    }

    pub fn cards(&self, repository_id: i64) -> Result<Vec<Card>> {
        self.cards_where("repository_id = ?1", repository_id)
    }

    pub fn move_card(&self, id: i64, column: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET column_name = ?1 WHERE id = ?2",
            params![column, id],
        )?;
        Ok(())
    }

    // Changes

    pub fn add_change(
        &self,
        repository_id: i64,
        card_id: Option<i64>,
        branch: &str,
    ) -> Result<Change> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO changes (repository_id, card_id, branch, state, created_at) VALUES (?1, ?2, ?3, 'open', ?4)",
            params![repository_id, card_id, branch, now()],
        )?;
        Ok(Change {
            id: conn.last_insert_rowid(),
            repository_id,
            card_id,
            branch: branch.into(),
            state: "open".into(),
        })
    }

    pub fn change(&self, id: i64) -> Result<Option<Change>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT id, repository_id, card_id, branch, state FROM changes WHERE id = ?1",
                [id],
                |r| {
                    Ok(Change {
                        id: r.get(0)?,
                        repository_id: r.get(1)?,
                        card_id: r.get(2)?,
                        branch: r.get(3)?,
                        state: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// The changes of a repository, oldest first.
    pub fn changes(&self, repository_id: i64) -> Result<Vec<Change>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, repository_id, card_id, branch, state FROM changes WHERE repository_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([repository_id], |r| {
            Ok(Change {
                id: r.get(0)?,
                repository_id: r.get(1)?,
                card_id: r.get(2)?,
                branch: r.get(3)?,
                state: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
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
        })
    }

    fn runs_where(&self, clause: &str, param: rusqlite::types::Value) -> Result<Vec<Run>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, card_id, agent, harness, purpose, status, worktree, started_at, ended_at
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
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn run(&self, id: i64) -> Result<Option<Run>> {
        Ok(self.runs_where("id = ?1", id.into())?.pop())
    }

    /// The runs of a card, oldest first.
    pub fn runs_for_card(&self, card_id: i64) -> Result<Vec<Run>> {
        self.runs_where("card_id = ?1", card_id.into())
    }

    /// The runs that have not ended.
    pub fn active_runs(&self) -> Result<Vec<Run>> {
        self.runs_where("ended_at IS NULL AND ?1", 1.into())
    }

    pub fn set_run_status(&self, id: i64, status: &str) -> Result<()> {
        let ended = matches!(status, "succeeded" | "failed" | "stopped").then(now);
        self.conn().execute(
            "UPDATE runs SET status = ?1, ended_at = COALESCE(?2, ended_at) WHERE id = ?3",
            params![status, ended, id],
        )?;
        Ok(())
    }

    pub fn set_run_worktree(&self, id: i64, worktree: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE runs SET worktree = ?1 WHERE id = ?2",
            params![worktree, id],
        )?;
        Ok(())
    }

    /// Appends an event to a run and returns its sequence number.
    pub fn append_event(&self, run_id: i64, kind: &str, data: &serde_json::Value) -> Result<i64> {
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
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT run_id, seq, kind, data, at FROM events WHERE run_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map([run_id], |r| {
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
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, kind, context, options, choice, reason, decider, run_id, card_id, outcome, at
             FROM judgements WHERE {clause} ORDER BY id"
        ))?;
        let rows = stmt.query_map([param], |r| {
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

    /// The items not resolved yet, oldest first.
    pub fn inbox(&self) -> Result<Vec<InboxItem>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, kind, card_id, title, data, resolved_at, at FROM inbox_items WHERE resolved_at IS NULL ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
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

    /// Resolves every open item about a card, such as its "ready to merge" once it is merged.
    pub fn resolve_inbox_for_card(&self, card_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 WHERE card_id = ?2 AND resolved_at IS NULL",
            params![now(), card_id],
        )?;
        Ok(())
    }

    // Usage

    pub fn add_usage(&self, u: &Usage) -> Result<()> {
        self.conn().execute(
            "INSERT INTO usage (run_id, model, input_tokens, output_tokens, cached_tokens, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![u.run_id, u.model, u.input_tokens, u.output_tokens, u.cached_tokens, now()],
        )?;
        Ok(())
    }

    pub fn usage_for_run(&self, run_id: i64) -> Result<Vec<Usage>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT run_id, model, input_tokens, output_tokens, cached_tokens FROM usage WHERE run_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([run_id], |r| {
            Ok(Usage {
                run_id: r.get(0)?,
                model: r.get(1)?,
                input_tokens: r.get(2)?,
                output_tokens: r.get(3)?,
                cached_tokens: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Tokens used by all runs of a card.
    pub fn tokens_for_card(&self, card_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(SUM(u.input_tokens + u.output_tokens), 0) FROM usage u JOIN runs r ON r.id = u.run_id
             WHERE r.card_id = ?1",
            [card_id],
            |r| r.get(0),
        )?)
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
        assert_eq!(s.cards(repo.id).unwrap().len(), 2);

        let change = s.add_change(repo.id, Some(a.id), "moebius/1").unwrap();
        s.set_change_state(change.id, "merged").unwrap();
        assert_eq!(s.change(change.id).unwrap().unwrap().state, "merged");

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
        };
        s.add_usage(&u).unwrap();
        s.add_usage(&u).unwrap();
        assert_eq!(s.usage_for_run(run.id).unwrap(), vec![u.clone(), u]);
        assert_eq!(s.tokens_for_card(a.id).unwrap(), 240);
    }
}
