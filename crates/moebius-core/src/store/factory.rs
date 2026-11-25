//! The software factory (ADR 0031): what the store keeps so that the owner of a busy repository has little to do. A
//! log of what happened that the daily digest reports (`factory_log`), the days a digest was made (`digests`, so that
//! one day has one), when each card's issue was taken (`assigned_since`, which the stale handling reads), the stale
//! settings of each repository (`upkeep_settings`) and the backlog file of each project (`backlogs`).

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Result, Store, now};
use crate::blobs::{redact, redact_json};

/// The tables of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE factory_log (
    id INTEGER PRIMARY KEY,
    at INTEGER NOT NULL,
    kind TEXT NOT NULL,
    repository_id INTEGER,
    number INTEGER,
    who TEXT,
    detail TEXT NOT NULL DEFAULT '',
    data TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX factory_log_by_time ON factory_log (at);
CREATE TABLE digests (
    day TEXT PRIMARY KEY,
    at INTEGER NOT NULL,
    since INTEGER NOT NULL,
    item_id INTEGER,
    log_id INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE assigned_since (
    card_id INTEGER PRIMARY KEY,
    since INTEGER NOT NULL
);
CREATE TABLE upkeep_settings (
    repository_id INTEGER PRIMARY KEY,
    release_after_days INTEGER NOT NULL DEFAULT 14,
    stale_after_days INTEGER NOT NULL DEFAULT 14,
    close_after_days INTEGER NOT NULL DEFAULT 30
);
CREATE TABLE backlogs (
    project_id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL,
    path TEXT NOT NULL,
    pattern TEXT NOT NULL,
    closed_marker TEXT NOT NULL,
    cap INTEGER NOT NULL,
    difficulty_label TEXT NOT NULL,
    lane_label TEXT NOT NULL
);
"#;

/// How many days an issue may be taken without a followed pull request before it is released, until a repository says
/// otherwise. 0 turns it off.
pub const DEFAULT_RELEASE_AFTER_DAYS: i64 = 14;
/// How many days a followed pull request whose last review asked for changes may see no push before it gets a
/// reminder. 0 turns the reminder and the closing off.
pub const DEFAULT_STALE_AFTER_DAYS: i64 = 14;
/// How many days such a pull request may see no push before it is closed. 0 turns the closing off.
pub const DEFAULT_CLOSE_AFTER_DAYS: i64 = 30;

/// The default item pattern of a backlog: a heading whose text starts with an ID such as `K-47`, then the title.
pub const DEFAULT_BACKLOG_PATTERN: &str =
    r"^#{1,6}\s+(?P<id>[A-Z][A-Z0-9]*-\d+)\s*[:.\u{2013}\u{2014}-]?\s*(?P<title>.*?)\s*$";
/// The default text that marks a backlog item as closed.
pub const DEFAULT_CLOSED_MARKER: &str = "[closed]";
/// The default most open issues one sync creates.
pub const DEFAULT_BACKLOG_CAP: i64 = 15;
/// The default label of the form `difficulty:<easy|medium|hard>`: the text before the level.
pub const DEFAULT_DIFFICULTY_LABEL: &str = "difficulty:";
/// The default label of an item that requires a lane.
pub const DEFAULT_LANE_LABEL: &str = "lane-required";

/// A round of the Reviewer on a followed pull request: the repository, the pull request, the round and the verdict
/// (`approve` or `changes`, none while there is none).
pub type ReviewRound = (i64, i64, i64, Option<String>);

/// What a change row says of its merge: the repository, the card, the pull request and who opened it, if someone else.
type MergedChange = (Option<i64>, Option<i64>, Option<i64>, Option<String>);

/// One line of the log of what the factory did or saw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub id: i64,
    pub at: i64,
    /// `merged`, `merge_failed`, `revert`, `frozen`, `unfrozen`, `measurement`, `take`, `release`, `stale_reminder`,
    /// `stale_closed`, `auto_release` or `backlog_sync`; anything else a part of Moebius notes is kept and shown.
    pub kind: String,
    pub repository_id: Option<i64>,
    pub number: Option<i64>,
    pub who: Option<String>,
    pub detail: String,
    pub data: Value,
}

/// The stale settings of a repository, in days; 0 turns the matching handling off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upkeep {
    pub release_after_days: i64,
    pub stale_after_days: i64,
    pub close_after_days: i64,
}

impl Default for Upkeep {
    fn default() -> Self {
        Self {
            release_after_days: DEFAULT_RELEASE_AFTER_DAYS,
            stale_after_days: DEFAULT_STALE_AFTER_DAYS,
            close_after_days: DEFAULT_CLOSE_AFTER_DAYS,
        }
    }
}

/// Where a project keeps its backlog and how the sync reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backlog {
    pub project_id: i64,
    /// The repository that holds the file and receives the issues.
    pub repository_id: i64,
    /// The file's path in the repository's default branch.
    pub path: String,
    /// A regular expression with the groups `id` and `title`, matched against each line; a line that matches starts an
    /// item, which lasts until the next heading of the same level or a higher one.
    pub pattern: String,
    /// Text that marks an item as closed wherever it stands in the item, in any case; empty has no marker.
    pub closed_marker: String,
    /// The most open issues one sync creates.
    pub cap: i64,
    /// The start of the label that states an item's difficulty, as `difficulty:` for `difficulty:easy`.
    pub difficulty_label: String,
    /// The label for an item that says a lane is required.
    pub lane_label: String,
}

/// A digest that was made: the day, when, from when it reports, and its inbox item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestDay {
    pub day: String,
    pub at: i64,
    pub since: i64,
    pub item_id: Option<i64>,
}

fn activity_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Activity> {
    let data: String = r.get(7)?;
    Ok(Activity {
        id: r.get(0)?,
        at: r.get(1)?,
        kind: r.get(2)?,
        repository_id: r.get(3)?,
        number: r.get(4)?,
        who: r.get(5)?,
        detail: r.get(6)?,
        data: serde_json::from_str(&data).unwrap_or(Value::Null),
    })
}

const ACTIVITY_COLUMNS: &str = "id, at, kind, repository_id, number, who, detail, data";

impl Store {
    /// Writes a line to the log the digest reads. What it says is redacted. `merged`, `merge_failed`, `take` and
    /// `release` are written by Moebius itself; a part that builds on the factory (a freeze, a revert, a measurement)
    /// writes its own with this.
    pub fn note_activity(
        &self,
        kind: &str,
        repository_id: Option<i64>,
        number: Option<i64>,
        who: Option<&str>,
        detail: &str,
        data: &Value,
    ) -> Result<()> {
        self.note_activity_at(now(), kind, repository_id, number, who, detail, data)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn note_activity_at(
        &self,
        at: i64,
        kind: &str,
        repository_id: Option<i64>,
        number: Option<i64>,
        who: Option<&str>,
        detail: &str,
        data: &Value,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO factory_log (at, kind, repository_id, number, who, detail, data) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                at,
                kind,
                repository_id,
                number,
                who,
                redact(detail),
                serde_json::to_string(&redact_json(data))?
            ],
        )?;
        Ok(())
    }

    /// The log lines after the line `after` up to the line `upto`, oldest first. Digests partition the log by these
    /// numbers, which only grow, so that a line written in the second a digest was made is told once.
    pub fn activity_after(&self, after: i64, upto: i64) -> Result<Vec<Activity>> {
        self.rows(
            &format!(
                "SELECT {ACTIVITY_COLUMNS} FROM factory_log WHERE id > ?1 AND id <= ?2 ORDER BY id"
            ),
            params![after, upto],
            activity_from_row,
        )
    }

    /// The number of the newest log line, 0 for an empty log.
    pub fn last_activity(&self) -> Result<i64> {
        Ok(self
            .read()
            .query_row("SELECT COALESCE(MAX(id), 0) FROM factory_log", [], |r| {
                r.get(0)
            })?)
    }

    /// The log lines from `since` (included) to `until` (excluded) by their time, oldest first.
    pub fn activity_between(&self, since: i64, until: i64) -> Result<Vec<Activity>> {
        self.rows(
            &format!(
                "SELECT {ACTIVITY_COLUMNS} FROM factory_log WHERE at >= ?1 AND at < ?2 ORDER BY id"
            ),
            params![since, until],
            activity_from_row,
        )
    }

    /// The log lines of `kind` about the issue or pull request `number` of the repository, oldest first.
    pub fn activity_about(
        &self,
        kind: &str,
        repository_id: i64,
        number: i64,
    ) -> Result<Vec<Activity>> {
        self.rows(
            &format!(
                "SELECT {ACTIVITY_COLUMNS} FROM factory_log WHERE kind = ?1 AND repository_id = ?2 AND number = ?3 \
                 ORDER BY id"
            ),
            params![kind, repository_id, number],
            activity_from_row,
        )
    }

    /// Takes the digest of `day` for the caller: true for the first to ask, false when the day has one already, so a
    /// day has one digest however often the clock looks and whether or not the server restarted in between. `since` is
    /// where the digest starts reporting.
    pub fn claim_digest(&self, day: &str, since: i64) -> Result<bool> {
        Ok(self.conn().execute(
            "INSERT OR IGNORE INTO digests (day, at, since) VALUES (?1, ?2, ?3)",
            params![day, now(), since],
        )? > 0)
    }

    /// Records the inbox item of the digest of `day`, and that it reported everything up to the time `until` and the log
    /// line `log_id`: the next digest starts there, so that nothing is told twice or not at all.
    pub fn set_digest_item(&self, day: &str, item_id: i64, until: i64, log_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE digests SET item_id = ?1, at = ?3, log_id = ?4 WHERE day = ?2",
            params![item_id, day, until, log_id],
        )?;
        Ok(())
    }

    /// The newest log line the digests have reported, 0 when none was made.
    pub fn digested_activity(&self) -> Result<i64> {
        Ok(self
            .read()
            .query_row("SELECT COALESCE(MAX(log_id), 0) FROM digests", [], |r| {
                r.get(0)
            })?)
    }

    /// The digest of `day`, if one was made.
    pub fn digest_of(&self, day: &str) -> Result<Option<DigestDay>> {
        Ok(self
            .read()
            .query_row(
                "SELECT day, at, since, item_id FROM digests WHERE day = ?1",
                [day],
                |r| {
                    Ok(DigestDay {
                        day: r.get(0)?,
                        at: r.get(1)?,
                        since: r.get(2)?,
                        item_id: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    /// When the latest digest was made, if there is one.
    pub fn last_digest_at(&self) -> Result<Option<i64>> {
        Ok(self
            .read()
            .query_row("SELECT MAX(at) FROM digests", [], |r| r.get(0))?)
    }

    /// Forgets the digest of `day`, so that the next look at the clock makes it again; for a person who asks for the
    /// day's digest anew.
    pub fn forget_digest(&self, day: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM digests WHERE day = ?1", [day])?;
        Ok(())
    }

    /// The stale settings of the repository: what was set, else the defaults.
    pub fn upkeep(&self, repository_id: i64) -> Result<Upkeep> {
        Ok(self
            .read()
            .query_row(
                "SELECT release_after_days, stale_after_days, close_after_days FROM upkeep_settings \
                 WHERE repository_id = ?1",
                [repository_id],
                |r| {
                    Ok(Upkeep {
                        release_after_days: r.get(0)?,
                        stale_after_days: r.get(1)?,
                        close_after_days: r.get(2)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }

    /// Sets the stale settings of the repository.
    pub fn set_upkeep(&self, repository_id: i64, upkeep: &Upkeep) -> Result<()> {
        self.conn().execute(
            "INSERT INTO upkeep_settings (repository_id, release_after_days, stale_after_days, close_after_days) \
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT (repository_id) DO UPDATE SET \
             release_after_days = ?2, stale_after_days = ?3, close_after_days = ?4",
            params![
                repository_id,
                upkeep.release_after_days,
                upkeep.stale_after_days,
                upkeep.close_after_days
            ],
        )?;
        Ok(())
    }

    /// Since when the card's issue has had an assignee on the forge, in seconds since the epoch, as the poll saw it;
    /// none while it has none.
    pub fn assigned_since(&self, card_id: i64) -> Result<Option<i64>> {
        Ok(self
            .read()
            .query_row(
                "SELECT since FROM assigned_since WHERE card_id = ?1",
                [card_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Writes down that the card's issue was taken at `at`, replacing what was there.
    pub fn set_assigned_since(&self, card_id: i64, at: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO assigned_since (card_id, since) VALUES (?1, ?2) \
             ON CONFLICT (card_id) DO UPDATE SET since = ?2",
            params![card_id, at],
        )?;
        Ok(())
    }

    /// The backlog of the project, if it has one.
    pub fn backlog(&self, project_id: i64) -> Result<Option<Backlog>> {
        Ok(self
            .read()
            .query_row(
                "SELECT project_id, repository_id, path, pattern, closed_marker, cap, difficulty_label, lane_label \
                 FROM backlogs WHERE project_id = ?1",
                [project_id],
                |r| {
                    Ok(Backlog {
                        project_id: r.get(0)?,
                        repository_id: r.get(1)?,
                        path: r.get(2)?,
                        pattern: r.get(3)?,
                        closed_marker: r.get(4)?,
                        cap: r.get(5)?,
                        difficulty_label: r.get(6)?,
                        lane_label: r.get(7)?,
                    })
                },
            )
            .optional()?)
    }

    /// Sets the backlog of the project.
    pub fn set_backlog(&self, backlog: &Backlog) -> Result<()> {
        self.conn().execute(
            "INSERT INTO backlogs (project_id, repository_id, path, pattern, closed_marker, cap, difficulty_label, \
             lane_label) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT (project_id) DO UPDATE SET \
             repository_id = ?2, path = ?3, pattern = ?4, closed_marker = ?5, cap = ?6, difficulty_label = ?7, \
             lane_label = ?8",
            params![
                backlog.project_id,
                backlog.repository_id,
                backlog.path,
                backlog.pattern,
                backlog.closed_marker,
                backlog.cap,
                backlog.difficulty_label,
                backlog.lane_label
            ],
        )?;
        Ok(())
    }

    /// Takes the backlog of the project away.
    pub fn remove_backlog(&self, project_id: i64) -> Result<()> {
        self.conn()
            .execute("DELETE FROM backlogs WHERE project_id = ?1", [project_id])?;
        Ok(())
    }

    /// The ids of the cards of the repository whose title starts with `<item>:`, oldest first, closed ones included.
    pub fn cards_of_backlog_item(&self, repository_id: i64, item: &str) -> Result<Vec<i64>> {
        self.rows(
            "SELECT id FROM cards WHERE repository_id = ?1 AND substr(title, 1, length(?2) + 1) = ?2 || ':' \
             ORDER BY id",
            params![repository_id, item],
            |r| r.get(0),
        )
    }

    /// Whether a log line of `kind` names the change `change_id` in its data (a measurement that came with it, say).
    pub fn has_activity_for_change(&self, kind: &str, change_id: i64) -> Result<bool> {
        Ok(self.read().query_row(
            "SELECT EXISTS (SELECT 1 FROM factory_log WHERE kind = ?1 AND json_extract(data, '$.change') = ?2)",
            params![kind, change_id],
            |r| r.get(0),
        )?)
    }

    /// The rounds of the Reviewer on followed pull requests that began from `since` to `until`: the repository, the pull
    /// request, the round and the verdict (`approve` or `changes`, none while there is none).
    pub fn review_rounds_between(&self, since: i64, until: i64) -> Result<Vec<ReviewRound>> {
        self.rows(
            "SELECT c.repository_id, COALESCE(c.pull_request, 0), e.round, e.verdict FROM external_reviews e \
             JOIN changes c ON c.id = e.change_id WHERE e.at >= ?1 AND e.at < ?2 AND c.repository_id IS NOT NULL \
             ORDER BY e.id",
            params![since, until],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
    }

    /// How many runs of the Reviewer of a card's own change started from `since` to `until`, by repository.
    pub fn review_runs_between(&self, since: i64, until: i64) -> Result<Vec<(i64, i64)>> {
        self.rows(
            "SELECT repository_id, COUNT(*) FROM runs WHERE purpose IN ('review', 'review-spec', 'review-standards') \
             AND started_at >= ?1 AND started_at < ?2 AND repository_id IS NOT NULL GROUP BY repository_id",
            params![since, until],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    /// The tokens all runs used from `since` to `until`.
    pub fn tokens_between(&self, since: i64, until: i64) -> Result<i64> {
        Ok(self.read().query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0) FROM usage WHERE at >= ?1 AND at < ?2",
            params![since, until],
            |r| r.get(0),
        )?)
    }

    /// The people-readable record of a card's merge: written when its change is set to `merged`. Kept apart from
    /// [`Store::note_activity`] because it reads the change.
    pub(super) fn note_merge(&self, change_id: i64) -> Result<()> {
        let row: Option<MergedChange> = self
            .conn()
            .query_row(
                "SELECT repository_id, card_id, pull_request, outside_author FROM changes WHERE id = ?1",
                [change_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((repository_id, card_id, pull_request, author)) = row else {
            return Ok(());
        };
        let (number, title): (Option<i64>, String) = match card_id {
            Some(card) => self
                .conn()
                .query_row(
                    "SELECT number, title FROM cards WHERE id = ?1",
                    [card],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .unwrap_or_default(),
            None => (None, String::new()),
        };
        self.note_activity(
            "merged",
            repository_id,
            pull_request.or(number),
            Some(author.as_deref().unwrap_or("moebius")),
            "",
            &serde_json::json!({ "change": change_id, "card": card_id, "issue": number, "title": title }),
        )
    }

    /// Logs what changed between two lists of assignees (`take` for a login that came, `release` for one that went) and
    /// keeps when the card was taken.
    pub(super) fn note_assignees(&self, card_id: i64, before: &[String], after: &[String]) {
        let Ok((repository_id, number)) = self.conn().query_row(
            "SELECT repository_id, number FROM cards WHERE id = ?1",
            [card_id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)),
        ) else {
            return;
        };
        let has = |list: &[String], login: &str| list.iter().any(|l| l.eq_ignore_ascii_case(login));
        for login in after.iter().filter(|l| !has(before, l)) {
            let _ = self.note_activity(
                "take",
                Some(repository_id),
                number,
                Some(login),
                "",
                &serde_json::json!({ "card": card_id }),
            );
        }
        for login in before.iter().filter(|l| !has(after, l)) {
            let _ = self.note_activity(
                "release",
                Some(repository_id),
                number,
                Some(login),
                "",
                &serde_json::json!({ "card": card_id }),
            );
        }
        if after.is_empty() {
            let _ = self
                .conn()
                .execute("DELETE FROM assigned_since WHERE card_id = ?1", [card_id]);
        } else if before.is_empty() || self.assigned_since(card_id).ok().flatten().is_none() {
            let _ = self.set_assigned_since(card_id, now());
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_day_has_one_digest_however_often_it_is_claimed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        assert!(store.claim_digest("2026-11-25", 0).unwrap());
        assert!(!store.claim_digest("2026-11-25", 0).unwrap());
        assert!(store.claim_digest("2026-11-26", 5).unwrap());
        store.set_digest_item("2026-11-25", 7, 100, 3).unwrap();
        assert_eq!(store.digested_activity().unwrap(), 3);
        assert_eq!(
            store.digest_of("2026-11-25").unwrap().unwrap().item_id,
            Some(7)
        );
        store.forget_digest("2026-11-25").unwrap();
        assert!(store.claim_digest("2026-11-25", 0).unwrap());
    }

    #[test]
    fn the_log_is_read_between_two_moments_and_redacted() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        store
            .note_activity_at(10, "take", Some(1), Some(3), Some("alice"), "", &json!({}))
            .unwrap();
        store
            .note_activity_at(
                20,
                "merge_failed",
                Some(1),
                Some(4),
                None,
                "token ghp_0123456789abcdefghijklmnopqrstuvwxyz was refused",
                &json!({}),
            )
            .unwrap();
        let lines = store.activity_between(10, 20).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].who.as_deref(), Some("alice"));
        let later = store.activity_between(20, 21).unwrap();
        assert!(!later[0].detail.contains("ghp_"));
    }

    #[test]
    fn a_repository_has_the_default_settings_until_it_is_given_others() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        assert_eq!(store.upkeep(1).unwrap(), Upkeep::default());
        let own = Upkeep {
            release_after_days: 3,
            stale_after_days: 0,
            close_after_days: 9,
        };
        store.set_upkeep(1, &own).unwrap();
        assert_eq!(store.upkeep(1).unwrap(), own);
        assert_eq!(store.upkeep(2).unwrap(), Upkeep::default());
    }
}
