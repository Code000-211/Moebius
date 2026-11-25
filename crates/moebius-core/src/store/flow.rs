//! What keeps measurements and reviews flowing without the owner's clicks (ADR 0033): the settings of each repository
//! (`flow_settings`), the measurements a registered measurer posted as a comment (`measurement_reports`), when the checks
//! of a head turned green (`green_heads`) and the pull requests that name no issue (`unbound_requests`).

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// The tables and columns of this part.
pub(super) const SCHEMA: &str = r#"
ALTER TABLE external_reviews ADD COLUMN measurement_id INTEGER;
CREATE TABLE flow_settings (
    repository_id INTEGER PRIMARY KEY,
    review_on_green INTEGER NOT NULL DEFAULT 0,
    green_wait_minutes INTEGER NOT NULL DEFAULT 5,
    unbound_close_days INTEGER NOT NULL DEFAULT 14,
    status_labels INTEGER NOT NULL DEFAULT 0,
    status_prefix TEXT NOT NULL DEFAULT 'status:'
);
CREATE TABLE measurement_reports (
    id INTEGER PRIMARY KEY,
    change_id INTEGER NOT NULL REFERENCES changes(id),
    head TEXT NOT NULL,
    before_commit TEXT NOT NULL,
    measurer TEXT NOT NULL,
    comment_id INTEGER NOT NULL,
    body TEXT NOT NULL,
    state TEXT NOT NULL,
    at INTEGER NOT NULL,
    UNIQUE (change_id, comment_id)
);
CREATE TABLE green_heads (
    change_id INTEGER NOT NULL,
    head TEXT NOT NULL,
    since INTEGER NOT NULL,
    asked INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (change_id, head)
);
CREATE TABLE unbound_requests (
    repository_id INTEGER NOT NULL,
    number INTEGER NOT NULL,
    first_seen INTEGER NOT NULL,
    commented INTEGER NOT NULL DEFAULT 0,
    labelled INTEGER NOT NULL DEFAULT 0,
    closed_at INTEGER,
    explained INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (repository_id, number)
);
"#;

/// The default of `green_wait_minutes`.
pub const DEFAULT_GREEN_WAIT_MINUTES: i64 = 5;
/// The default of `unbound_close_days`.
pub const DEFAULT_UNBOUND_CLOSE_DAYS: i64 = 14;
/// The default of `status_prefix`.
pub const DEFAULT_STATUS_PREFIX: &str = "status:";

/// How a repository lets reviews and pull requests flow by themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flow {
    /// Whether a followed pull request is reviewed once the checks of its head have been green for a while, without
    /// its author's `/done`.
    pub review_on_green: bool,
    /// How many minutes the checks must have been green, with no push, before that review starts.
    pub green_wait_minutes: i64,
    /// After how many days a pull request that names no issue is closed; 0 never closes one.
    pub unbound_close_days: i64,
    /// Whether the server keeps exactly one status label on every open issue that has a card (see `app::flow::status`).
    pub status_labels: bool,
    /// What a status label starts with; the label is the prefix and the state's name.
    pub status_prefix: String,
}

impl Default for Flow {
    fn default() -> Self {
        Self {
            review_on_green: false,
            green_wait_minutes: DEFAULT_GREEN_WAIT_MINUTES,
            unbound_close_days: DEFAULT_UNBOUND_CLOSE_DAYS,
            status_labels: false,
            status_prefix: DEFAULT_STATUS_PREFIX.into(),
        }
    }
}

/// A measurement a registered measurer wrote as a comment, for one head of one change. Its `state` is `submitted` (the
/// round that reads it is not over), `accepted` (the Reviewer approved the change with it), `insufficient` (the Reviewer
/// said it is not enough) or `invalid` (the head moved).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MeasurementReport {
    pub id: i64,
    pub change_id: i64,
    /// The commit measured (`after`), which is the change's head when the report was read.
    pub head: String,
    /// The merge base the numbers were taken against (`before`).
    pub before: String,
    pub measurer: String,
    #[serde(skip)]
    pub comment_id: i64,
    /// The comment, redacted.
    #[serde(skip)]
    pub body: String,
    pub state: String,
    pub at: i64,
}

fn report(r: &rusqlite::Row) -> rusqlite::Result<MeasurementReport> {
    Ok(MeasurementReport {
        id: r.get(0)?,
        change_id: r.get(1)?,
        head: r.get(2)?,
        before: r.get(3)?,
        measurer: r.get(4)?,
        comment_id: r.get(5)?,
        body: r.get(6)?,
        state: r.get(7)?,
        at: r.get(8)?,
    })
}

const REPORT_COLUMNS: &str =
    "id, change_id, head, before_commit, measurer, comment_id, body, state, at";

/// What is known of a pull request that names no issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unbound {
    pub first_seen: i64,
    pub commented: bool,
    pub labelled: bool,
    pub closed_at: Option<i64>,
    /// Whether the pull request was told once why an issue it names cannot take it up.
    pub explained: bool,
}

impl Store {
    /// The flow settings of the repository.
    pub fn flow(&self, repository_id: i64) -> Result<Flow> {
        Ok(self
            .read()
            .query_row(
                "SELECT review_on_green, green_wait_minutes, unbound_close_days, status_labels, status_prefix \
                 FROM flow_settings WHERE repository_id = ?1",
                [repository_id],
                |r| {
                    Ok(Flow {
                        review_on_green: r.get(0)?,
                        green_wait_minutes: r.get(1)?,
                        unbound_close_days: r.get(2)?,
                        status_labels: r.get(3)?,
                        status_prefix: r.get(4)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }

    /// Replaces the repository's flow settings.
    pub fn set_flow(&self, repository_id: i64, flow: &Flow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO flow_settings (repository_id, review_on_green, green_wait_minutes, unbound_close_days, \
             status_labels, status_prefix) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (repository_id) DO UPDATE SET \
             review_on_green = ?2, green_wait_minutes = ?3, unbound_close_days = ?4, status_labels = ?5, status_prefix = ?6",
            params![
                repository_id,
                flow.review_on_green,
                flow.green_wait_minutes,
                flow.unbound_close_days,
                flow.status_labels,
                flow.status_prefix
            ],
        )?;
        Ok(())
    }

    /// Keeps a measurement read from the comment `comment_id`; none when that comment was kept already.
    pub fn add_measurement_report(
        &self,
        change_id: i64,
        (head, before): (&str, &str),
        measurer: &str,
        comment_id: i64,
        body: &str,
    ) -> Result<Option<i64>> {
        let conn = self.conn();
        let added = conn.execute(
            "INSERT OR IGNORE INTO measurement_reports \
             (change_id, head, before_commit, measurer, comment_id, body, state, at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'submitted', ?7)",
            params![change_id, head, before, measurer, comment_id, body, now()],
        )?;
        Ok((added > 0).then(|| conn.last_insert_rowid()))
    }

    /// The measurement `id`.
    pub fn measurement_report(&self, id: i64) -> Result<Option<MeasurementReport>> {
        Ok(self
            .read()
            .query_row(
                &format!("SELECT {REPORT_COLUMNS} FROM measurement_reports WHERE id = ?1"),
                [id],
                report,
            )
            .optional()?)
    }

    /// The measurements of the change, oldest first.
    pub fn measurement_reports(&self, change_id: i64) -> Result<Vec<MeasurementReport>> {
        self.rows(
            &format!(
                "SELECT {REPORT_COLUMNS} FROM measurement_reports WHERE change_id = ?1 ORDER BY id"
            ),
            [change_id],
            report,
        )
    }

    /// Moves the measurement `id` to `state`.
    pub fn set_measurement_report_state(&self, id: i64, state: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE measurement_reports SET state = ?1 WHERE id = ?2",
            params![state, id],
        )?;
        Ok(())
    }

    /// Makes the measurements of the change that were not judged yet, and are of another head than `head`, invalid:
    /// how many.
    pub fn invalidate_measurement_reports(&self, change_id: i64, head: &str) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE measurement_reports SET state = 'invalid' \
             WHERE change_id = ?1 AND head <> ?2 AND state = 'submitted'",
            params![change_id, head],
        )?)
    }

    /// Records that the checks of `head` were seen green now, unless they were before.
    pub fn note_green(&self, change_id: i64, head: &str) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO green_heads (change_id, head, since) VALUES (?1, ?2, ?3)",
            params![change_id, head, now()],
        )?;
        Ok(())
    }

    /// When the checks of `head` were first seen green, if they were.
    pub fn green_since(&self, change_id: i64, head: &str) -> Result<Option<i64>> {
        Ok(self
            .read()
            .query_row(
                "SELECT since FROM green_heads WHERE change_id = ?1 AND head = ?2",
                params![change_id, head],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Says a review of `head` was asked for because its checks were green: true the first time, so that one head is
    /// asked for once whatever became of the round.
    pub fn ask_green_review(&self, change_id: i64, head: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE green_heads SET asked = 1 WHERE change_id = ?1 AND head = ?2 AND asked = 0",
            params![change_id, head],
        )? > 0)
    }

    /// Makes the checks of `head` count as green since `since`; for tests, which cannot wait minutes.
    #[cfg(test)]
    pub(crate) fn set_green_since(&self, change_id: i64, head: &str, since: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE green_heads SET since = ?3 WHERE change_id = ?1 AND head = ?2",
            params![change_id, head, since],
        )?;
        Ok(())
    }

    /// Makes the first sight of the pull request `number` count as `first_seen`; for tests.
    #[cfg(test)]
    pub(crate) fn set_unbound_first_seen(
        &self,
        repository_id: i64,
        number: i64,
        first_seen: i64,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE unbound_requests SET first_seen = ?3 WHERE repository_id = ?1 AND number = ?2",
            params![repository_id, number, first_seen],
        )?;
        Ok(())
    }

    /// What is known of the pull request `number`, which is none until it was seen without an issue.
    pub fn unbound(&self, repository_id: i64, number: i64) -> Result<Option<Unbound>> {
        Ok(self
            .read()
            .query_row(
                "SELECT first_seen, commented, labelled, closed_at, explained FROM unbound_requests \
                 WHERE repository_id = ?1 AND number = ?2",
                params![repository_id, number],
                |r| {
                    Ok(Unbound {
                        first_seen: r.get(0)?,
                        commented: r.get(1)?,
                        labelled: r.get(2)?,
                        closed_at: r.get(3)?,
                        explained: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// Records that the pull request `number` was seen without an issue, at the first time only.
    pub fn note_unbound(&self, repository_id: i64, number: i64) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO unbound_requests (repository_id, number, first_seen) VALUES (?1, ?2, ?3)",
            params![repository_id, number, now()],
        )?;
        Ok(())
    }

    /// Records what was done about the pull request: `commented`, `labelled`, `unlabelled`, `explained` or `closed`.
    pub fn set_unbound_done(&self, repository_id: i64, number: i64, what: &str) -> Result<()> {
        let sql = match what {
            "commented" => {
                "UPDATE unbound_requests SET commented = 1 WHERE repository_id = ?1 AND number = ?2"
            }
            "labelled" => {
                "UPDATE unbound_requests SET labelled = 1 WHERE repository_id = ?1 AND number = ?2"
            }
            "unlabelled" => {
                "UPDATE unbound_requests SET labelled = 0 WHERE repository_id = ?1 AND number = ?2"
            }
            "explained" => {
                "UPDATE unbound_requests SET explained = 1 WHERE repository_id = ?1 AND number = ?2"
            }
            _ => {
                "UPDATE unbound_requests SET closed_at = CAST(strftime('%s', 'now') AS INTEGER) WHERE repository_id = ?1 AND number = ?2"
            }
        };
        self.conn().execute(sql, params![repository_id, number])?;
        Ok(())
    }

    /// The issues of the repository a backlog sync made, with the item each is for.
    pub fn backlog_cards_of(&self, repository_id: i64) -> Result<Vec<(i64, String)>> {
        self.rows(
            "SELECT b.card_id, b.item_id FROM backlog_cards b JOIN cards c ON c.id = b.card_id \
             WHERE c.repository_id = ?1 ORDER BY b.card_id",
            [repository_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_and_are_kept_per_repository() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        assert_eq!(store.flow(1).unwrap(), Flow::default());
        let own = Flow {
            review_on_green: true,
            green_wait_minutes: 9,
            unbound_close_days: 0,
            status_labels: true,
            status_prefix: "s/".into(),
        };
        store.set_flow(1, &own).unwrap();
        assert_eq!(store.flow(1).unwrap(), own);
        assert_eq!(store.flow(2).unwrap(), Flow::default());
    }

    #[test]
    fn a_comment_is_kept_once_and_a_push_invalidates_what_was_not_judged() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        let repository = store.add_repository("r", "/p", "none", "main").unwrap();
        let change = store
            .add_change(repository.id, None, "b", Some(7), "h1")
            .unwrap();
        let first = store
            .add_measurement_report(change.id, ("h1", "b0"), "ada", 5, "body")
            .unwrap()
            .unwrap();
        assert!(
            store
                .add_measurement_report(change.id, ("h1", "b0"), "ada", 5, "again")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .invalidate_measurement_reports(change.id, "h1")
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .invalidate_measurement_reports(change.id, "h2")
                .unwrap(),
            1
        );
        assert_eq!(
            store.measurement_report(first).unwrap().unwrap().state,
            "invalid"
        );
    }

    #[test]
    fn a_head_is_asked_for_once_and_green_starts_at_the_first_sight() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        store.note_green(1, "h1").unwrap();
        let since = store.green_since(1, "h1").unwrap().unwrap();
        store.note_green(1, "h1").unwrap();
        assert_eq!(store.green_since(1, "h1").unwrap(), Some(since));
        assert_eq!(store.green_since(1, "h2").unwrap(), None);
        assert!(store.ask_green_review(1, "h1").unwrap());
        assert!(!store.ask_green_review(1, "h1").unwrap());
    }
}
