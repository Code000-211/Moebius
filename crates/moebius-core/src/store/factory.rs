//! The software factory's state (ADR 0031): what the store keeps so that every merge the server makes can be held to a
//! repository's rules. A repository keeps its factory settings in columns of `repositories` (whether a measurement is
//! required, the owner's locked paths, the review brief, the ready-for-testing label and the
//! measurers) and, while it is frozen, why. A merged change keeps the commit its merge made. A
//! measurement is accepted for one head of one change. A revert of a merged change is kept once, with the time of the
//! revert commit, which the automatic freeze counts.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// The tables and columns of this part.
pub(super) const SCHEMA: &str = r#"
ALTER TABLE repositories ADD COLUMN require_measurement INTEGER NOT NULL DEFAULT 0;
ALTER TABLE repositories ADD COLUMN locked_paths TEXT NOT NULL DEFAULT '[]';
ALTER TABLE repositories ADD COLUMN review_brief TEXT NOT NULL DEFAULT '["REVIEW.md"]';
ALTER TABLE repositories ADD COLUMN testing_label TEXT NOT NULL DEFAULT 'ready-for-testing';
ALTER TABLE repositories ADD COLUMN measurers TEXT NOT NULL DEFAULT '[]';
CREATE TABLE freezes (
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    cause TEXT NOT NULL,
    source TEXT NOT NULL,
    reason TEXT NOT NULL,
    at INTEGER NOT NULL,
    PRIMARY KEY (repository_id, cause)
);
ALTER TABLE changes ADD COLUMN merge_commit TEXT;
CREATE TABLE measurements (
    id INTEGER PRIMARY KEY,
    change_id INTEGER NOT NULL REFERENCES changes(id),
    head TEXT NOT NULL,
    accepted_by TEXT NOT NULL,
    source TEXT NOT NULL,
    at INTEGER NOT NULL
);
CREATE INDEX measurements_of_a_change ON measurements (change_id, head);
CREATE TABLE breaker_trips (
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    breaker TEXT NOT NULL,
    cause TEXT NOT NULL,
    at INTEGER NOT NULL,
    PRIMARY KEY (repository_id, breaker, cause)
);
CREATE TABLE reverts (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    change_id INTEGER NOT NULL REFERENCES changes(id),
    revert_commit TEXT NOT NULL,
    at INTEGER NOT NULL,
    UNIQUE (repository_id, revert_commit)
);
"#;

/// Why a repository is frozen, and since when. A frozen repository starts no review round, merges nothing and starts no
/// agent run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frozen {
    /// What the owner reads: "Frozen: <reason>".
    pub reason: String,
    /// `manual` (a person froze it), `label` (the label `factory:freeze` is on an issue: it lifts when the label does)
    /// or `automatic` (a breaker tripped).
    pub source: String,
    /// When it was frozen, in seconds since the epoch.
    pub since: i64,
}

/// A repository's factory settings, as one edit writes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactorySettings {
    pub require_measurement: bool,
    pub locked_paths: Vec<String>,
    pub review_brief: Vec<String>,
    pub testing_label: String,
    pub measurers: Vec<String>,
}

/// The three columns of a repository select that say why it is frozen: all the causes' reasons together, the first
/// cause's source and when the first began; null while no cause holds.
pub(super) const FROZEN_COLUMNS: &str = "(SELECT group_concat(reason, '; ') FROM (SELECT reason FROM freezes WHERE repository_id = repositories.id ORDER BY at, cause)), \
     (SELECT source FROM freezes WHERE repository_id = repositories.id ORDER BY at, cause LIMIT 1), \
     (SELECT MIN(at) FROM freezes WHERE repository_id = repositories.id)";

/// The default of `review_brief`.
pub const DEFAULT_REVIEW_BRIEF: &str = "REVIEW.md";
/// The default of `testing_label`.
pub const DEFAULT_TESTING_LABEL: &str = "ready-for-testing";

fn list(text: String) -> Vec<String> {
    serde_json::from_str(&text).unwrap_or_default()
}

/// The factory part of a repository row, read from the columns [`REPOSITORY_FACTORY_COLUMNS`] starting at `at`.
pub(super) fn read_factory(
    r: &rusqlite::Row,
    at: usize,
) -> rusqlite::Result<(FactorySettings, Option<Frozen>)> {
    let settings = FactorySettings {
        require_measurement: r.get(at)?,
        locked_paths: list(r.get(at + 1)?),
        review_brief: list(r.get(at + 2)?),
        testing_label: r.get(at + 3)?,
        measurers: list(r.get(at + 4)?),
    };
    let reason: Option<String> = r.get(at + 5)?;
    let source: Option<String> = r.get(at + 6)?;
    let since: Option<i64> = r.get(at + 7)?;
    let frozen = reason.map(|reason| Frozen {
        reason,
        source: source.unwrap_or_else(|| "manual".into()),
        since: since.unwrap_or_default(),
    });
    Ok((settings, frozen))
}

impl Store {
    /// Replaces the repository's factory settings.
    pub fn set_repository_factory(&self, id: i64, settings: &FactorySettings) -> Result<()> {
        self.conn().execute(
            "UPDATE repositories SET require_measurement = ?1, locked_paths = ?2, \
             review_brief = ?3, testing_label = ?4, measurers = ?5 WHERE id = ?6",
            params![
                settings.require_measurement,
                serde_json::to_string(&settings.locked_paths)?,
                serde_json::to_string(&settings.review_brief)?,
                settings.testing_label,
                serde_json::to_string(&settings.measurers)?,
                id
            ],
        )?;
        Ok(())
    }

    /// Adds a cause to the repository's freeze: `cause` is `manual`, `label` or the breaker's name (`tokens`, `reverts`,
    /// `nightly`), `source` what the owner reads it as (`manual`, `label` or `automatic`). False when that cause holds
    /// already, which keeps its first reason. Causes are kept apart, so that lifting one never lifts another.
    pub fn freeze_repository(
        &self,
        id: i64,
        cause: &str,
        source: &str,
        reason: &str,
    ) -> Result<bool> {
        Ok(self.conn().execute(
            "INSERT OR IGNORE INTO freezes (repository_id, cause, source, reason, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, cause, source, reason, now()],
        )? > 0)
    }

    /// Lifts one cause of the freeze. False when it did not hold.
    pub fn lift_freeze_cause(&self, id: i64, cause: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "DELETE FROM freezes WHERE repository_id = ?1 AND cause = ?2",
            params![id, cause],
        )? > 0)
    }

    /// Whether the cause holds.
    pub fn has_freeze_cause(&self, id: i64, cause: &str) -> Result<bool> {
        Ok(self.read().query_row(
            "SELECT EXISTS(SELECT 1 FROM freezes WHERE repository_id = ?1 AND cause = ?2)",
            params![id, cause],
            |r| r.get(0),
        )?)
    }

    /// Lifts every cause of the freeze. False when the repository was not frozen.
    pub fn unfreeze_repository(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM freezes WHERE repository_id = ?1", [id])?
            > 0)
    }

    /// Records when a change merged and, when it is known, the commit the merge made.
    pub fn set_change_merged(&self, id: i64, commit: Option<&str>) -> Result<()> {
        self.conn().execute(
            "UPDATE changes SET state = 'merged', merge_commit = COALESCE(?1, merge_commit) WHERE id = ?2",
            params![commit, id],
        )?;
        Ok(())
    }

    /// The merged change of the repository that was merged as, or whose head is, the commit `commit`.
    pub fn merged_change_of_commit(&self, repository_id: i64, commit: &str) -> Result<Option<i64>> {
        Ok(self
            .read()
            .query_row(
                "SELECT id FROM changes WHERE repository_id = ?1 AND state = 'merged' \
                 AND (merge_commit = ?2 OR head = ?2) ORDER BY id DESC LIMIT 1",
                params![repository_id, commit],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The merged change of the repository whose pull request is number `number`.
    pub fn merged_change_of_request(&self, repository_id: i64, number: i64) -> Result<Option<i64>> {
        Ok(self
            .read()
            .query_row(
                "SELECT id FROM changes WHERE repository_id = ?1 AND state = 'merged' AND pull_request = ?2 \
                 ORDER BY id DESC LIMIT 1",
                params![repository_id, number],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Notes that the breaker `breaker` tripped for `cause` in the repository: true the first time, so that one cause
    /// freezes a repository once, and a person who unfreezes it is not frozen again by what already tripped it.
    pub fn trip_breaker(&self, repository_id: i64, breaker: &str, cause: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "INSERT OR IGNORE INTO breaker_trips (repository_id, breaker, cause, at) VALUES (?1, ?2, ?3, ?4)",
            params![repository_id, breaker, cause, now()],
        )? > 0)
    }

    /// Accepts a measurement for the head of the change: who accepted it and where it came from (`person`, or the
    /// forge comment of a measurer).
    pub fn add_measurement(
        &self,
        change_id: i64,
        head: &str,
        accepted_by: &str,
        source: &str,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO measurements (change_id, head, accepted_by, source, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![change_id, head, accepted_by, source, now()],
        )?;
        Ok(())
    }

    /// Whether a measurement is accepted for exactly this head of the change.
    pub fn has_measurement(&self, change_id: i64, head: &str) -> Result<bool> {
        Ok(self.read().query_row(
            "SELECT EXISTS(SELECT 1 FROM measurements WHERE change_id = ?1 AND head = ?2)",
            params![change_id, head],
            |r| r.get(0),
        )?)
    }

    /// Keeps a revert of a merged change: true when `revert_commit` was not kept before. `at` is the time of the revert.
    pub fn add_revert(
        &self,
        repository_id: i64,
        change_id: i64,
        revert_commit: &str,
        at: i64,
    ) -> Result<bool> {
        Ok(self.conn().execute(
            "INSERT OR IGNORE INTO reverts (repository_id, change_id, revert_commit, at) VALUES (?1, ?2, ?3, ?4)",
            params![repository_id, change_id, revert_commit, at],
        )? > 0)
    }

    /// How many different merged changes of the repository were reverted since `since`.
    pub fn reverted_since(&self, repository_id: i64, since: i64) -> Result<i64> {
        Ok(self.read().query_row(
            "SELECT COUNT(DISTINCT change_id) FROM reverts WHERE repository_id = ?1 AND at >= ?2",
            params![repository_id, since],
            |r| r.get(0),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn a_new_repository_has_the_defaults_and_is_not_frozen() {
        let (_dir, store) = store();
        let repo = store.add_repository("r", "/r", "none", "main").unwrap();
        assert!(!repo.require_measurement);
        assert!(repo.locked_paths.is_empty());
        assert_eq!(repo.review_brief, vec!["REVIEW.md".to_string()]);
        assert_eq!(repo.testing_label, "ready-for-testing");
        assert!(repo.measurers.is_empty());
        assert!(repo.frozen.is_none());
        // What the row holds is what the insert returned.
        assert_eq!(store.repository(repo.id).unwrap().unwrap(), repo);
    }

    #[test]
    fn causes_of_a_freeze_are_kept_apart_and_only_a_person_lifts_them_all() {
        let (_dir, store) = store();
        let repo = store.add_repository("r", "/r", "none", "main").unwrap();
        assert!(
            store
                .freeze_repository(repo.id, "label", "label", "the label is on #1")
                .unwrap()
        );
        assert!(
            store
                .freeze_repository(repo.id, "nightly", "automatic", "the nightly check failed")
                .unwrap()
        );
        // The same cause twice keeps the first reason.
        assert!(
            !store
                .freeze_repository(repo.id, "label", "label", "again")
                .unwrap()
        );
        let frozen = store.repository(repo.id).unwrap().unwrap().frozen.unwrap();
        assert_eq!(frozen.source, "label");
        assert!(
            frozen.reason.contains("the label is on #1"),
            "{}",
            frozen.reason
        );
        assert!(
            frozen.reason.contains("nightly check failed"),
            "{}",
            frozen.reason
        );
        // Lifting the label's cause leaves the other.
        assert!(store.lift_freeze_cause(repo.id, "label").unwrap());
        assert!(!store.lift_freeze_cause(repo.id, "label").unwrap());
        let frozen = store.repository(repo.id).unwrap().unwrap().frozen.unwrap();
        assert_eq!(
            (frozen.reason.as_str(), frozen.source.as_str()),
            ("the nightly check failed", "automatic")
        );
        assert!(store.unfreeze_repository(repo.id).unwrap());
        assert!(!store.unfreeze_repository(repo.id).unwrap());
        assert!(store.repository(repo.id).unwrap().unwrap().frozen.is_none());
    }

    #[test]
    fn a_merged_change_is_found_by_the_commit_of_its_merge() {
        let (_dir, store) = store();
        let repo = store.add_repository("r", "/r", "none", "main").unwrap();
        let a = store.add_change(repo.id, None, "b1", None, "h1").unwrap();
        let b = store.add_change(repo.id, None, "b2", None, "h2").unwrap();
        store.set_change_merged(a.id, Some("c1")).unwrap();
        assert_eq!(
            store.merged_change_of_commit(repo.id, "c1").unwrap(),
            Some(a.id)
        );
        assert_eq!(
            store.merged_change_of_commit(repo.id, "nope").unwrap(),
            None
        );
        // A change that is still open is no merge.
        assert_eq!(store.change(b.id).unwrap().unwrap().state, "open");
    }

    #[test]
    fn a_measurement_counts_for_its_head_only_and_a_revert_is_kept_once() {
        let (_dir, store) = store();
        let repo = store.add_repository("r", "/r", "none", "main").unwrap();
        let change = store.add_change(repo.id, None, "b", None, "h1").unwrap();
        store
            .add_measurement(change.id, "h1", "owner", "person")
            .unwrap();
        assert!(store.has_measurement(change.id, "h1").unwrap());
        assert!(!store.has_measurement(change.id, "h2").unwrap());
        assert!(store.add_revert(repo.id, change.id, "r1", now()).unwrap());
        assert!(!store.add_revert(repo.id, change.id, "r1", now()).unwrap());
        assert_eq!(store.reverted_since(repo.id, now() - 60).unwrap(), 1);
        assert_eq!(store.reverted_since(repo.id, now() + 60).unwrap(), 0);
    }
}
