//! Automations: chats a schedule starts (see `app::automations`), each firing of one, and the server's local clock
//! that the schedules are read in.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// An automation as stored: when it fires, with which agent and prompt, where its chat works and whether it is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Automation {
    pub id: i64,
    pub name: String,
    /// A cron expression of five fields in the server's local time.
    pub schedule: String,
    pub agent: String,
    /// The first message of each chat it starts.
    pub prompt: String,
    /// The repository its chats work in; none for the harness configuration repository or an empty directory.
    pub repository_id: Option<i64>,
    /// Whether its chats work in the harness configuration repository.
    pub harness_repository: bool,
    pub enabled: bool,
    pub created_at: i64,
}

/// What an automation is made of, as a person saves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewAutomation {
    pub name: String,
    pub schedule: String,
    pub agent: String,
    pub prompt: String,
    #[serde(default)]
    pub repository: Option<i64>,
    /// Whether its chats work in the harness configuration repository instead of a repository.
    #[serde(default)]
    pub harness: bool,
    #[serde(default = "on")]
    pub enabled: bool,
}

fn on() -> bool {
    true
}

/// One firing of an automation: the chat it started, or why it started none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Firing {
    pub id: i64,
    pub automation_id: i64,
    /// `started` (with its chat), `skipped` (the chat of the firing before still ran) or `failed`.
    pub status: String,
    /// The chat it started.
    pub run_id: Option<i64>,
    /// The status of that chat now.
    pub chat_status: Option<String>,
    /// Whether a person fired it ("Run now") rather than the schedule.
    pub manual: bool,
    /// Why it was skipped or failed.
    pub reason: String,
    pub at: i64,
}

const AUTOMATION_COLUMNS: &str =
    "id, name, schedule, agent, prompt, repository_id, harness_repository, enabled, created_at";

fn automation(r: &Row) -> rusqlite::Result<Automation> {
    Ok(Automation {
        id: r.get(0)?,
        name: r.get(1)?,
        schedule: r.get(2)?,
        agent: r.get(3)?,
        prompt: r.get(4)?,
        repository_id: r.get(5)?,
        harness_repository: r.get(6)?,
        enabled: r.get(7)?,
        created_at: r.get(8)?,
    })
}

const FIRING_QUERY: &str =
    "SELECT f.id, f.automation_id, f.status, f.run_id, r.status, f.manual, f.reason, f.at
     FROM firings f LEFT JOIN runs r ON r.id = f.run_id";

fn firing(r: &Row) -> rusqlite::Result<Firing> {
    Ok(Firing {
        id: r.get(0)?,
        automation_id: r.get(1)?,
        status: r.get(2)?,
        run_id: r.get(3)?,
        chat_status: r.get(4)?,
        manual: r.get(5)?,
        reason: r.get(6)?,
        at: r.get(7)?,
    })
}

impl Store {
    /// Stores a new automation, checked up to the local minute `checked`, so that it first fires after it.
    pub fn add_automation(&self, new: &NewAutomation, checked: &str) -> Result<Automation> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO automations
                   (name, schedule, agent, prompt, repository_id, harness_repository, enabled, checked_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    new.name,
                    new.schedule,
                    new.agent,
                    new.prompt,
                    new.repository,
                    new.harness,
                    new.enabled,
                    checked,
                    now()
                ],
            )?;
            conn.last_insert_rowid()
        };
        Ok(self
            .automation(id)?
            .expect("the automation was just stored"))
    }

    /// Replaces what the automation `id` is made of; false when there is none.
    pub fn update_automation(&self, id: i64, new: &NewAutomation) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE automations SET name = ?2, schedule = ?3, agent = ?4, prompt = ?5, repository_id = ?6,
               harness_repository = ?7, enabled = ?8 WHERE id = ?1",
            params![
                id,
                new.name,
                new.schedule,
                new.agent,
                new.prompt,
                new.repository,
                new.harness,
                new.enabled
            ],
        )? == 1)
    }

    /// Deletes the automation `id` and its firings; false when there is none. Its chats stay.
    pub fn delete_automation(&self, id: i64) -> Result<bool> {
        let conn = self.conn();
        conn.execute("DELETE FROM firings WHERE automation_id = ?1", [id])?;
        Ok(conn.execute("DELETE FROM automations WHERE id = ?1", [id])? == 1)
    }

    pub fn automation(&self, id: i64) -> Result<Option<Automation>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {AUTOMATION_COLUMNS} FROM automations WHERE id = ?1"),
                [id],
                automation,
            )
            .optional()?)
    }

    /// The automations, by name.
    pub fn automations(&self) -> Result<Vec<Automation>> {
        self.rows(
            &format!("SELECT {AUTOMATION_COLUMNS} FROM automations ORDER BY name, id"),
            [],
            automation,
        )
    }

    /// The local minute up to which the schedule of the automation `id` was looked at.
    pub fn automation_checked(&self, id: i64) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT checked_at FROM automations WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    /// The schedules of every automation were looked at up to the local minute `checked`.
    pub fn set_automations_checked(&self, checked: &str) -> Result<()> {
        self.conn()
            .execute("UPDATE automations SET checked_at = ?1", [checked])?;
        Ok(())
    }

    /// Records a firing of the automation and returns it.
    pub fn add_firing(
        &self,
        automation_id: i64,
        status: &str,
        run_id: Option<i64>,
        manual: bool,
        reason: &str,
    ) -> Result<Firing> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO firings (automation_id, status, run_id, manual, reason, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![automation_id, status, run_id, manual, reason, now()],
            )?;
            conn.last_insert_rowid()
        };
        Ok(self
            .rows(&format!("{FIRING_QUERY} WHERE f.id = ?1"), [id], firing)?
            .remove(0))
    }

    /// The firings of the automation, newest first; at most `limit`.
    pub fn firings(&self, automation_id: i64, limit: i64) -> Result<Vec<Firing>> {
        self.rows(
            &format!("{FIRING_QUERY} WHERE f.automation_id = ?1 ORDER BY f.id DESC LIMIT ?2"),
            params![automation_id, limit],
            firing,
        )
    }

    /// The latest firing of the automation that started a chat.
    pub fn last_started_firing(&self, automation_id: i64) -> Result<Option<Firing>> {
        Ok(self
            .rows(
                &format!(
                    "{FIRING_QUERY} WHERE f.automation_id = ?1 AND f.status = 'started' ORDER BY f.id DESC LIMIT 1"
                ),
                [automation_id],
                firing,
            )?
            .pop())
    }

    /// The chats that automations started and that wait for a message, oldest first.
    pub fn waiting_automation_chats(&self) -> Result<Vec<i64>> {
        self.rows(
            "SELECT f.run_id FROM firings f JOIN runs r ON r.id = f.run_id
             WHERE f.status = 'started' AND r.ended_at IS NULL AND r.status = 'waiting' ORDER BY f.id",
            [],
            |r| r.get(0),
        )
    }

    /// The local minute it is on the server, as `YYYY-MM-DD HH:MM`.
    pub fn local_minute(&self) -> Result<String> {
        Ok(self.conn().query_row(
            "SELECT strftime('%Y-%m-%d %H:%M', 'now', 'localtime')",
            [],
            |r| r.get(0),
        )?)
    }

    /// The local minute `minute` (`YYYY-MM-DD HH:MM`) as Unix seconds; none when it is no time.
    pub fn local_minute_to_unix(&self, minute: &str) -> Result<Option<i64>> {
        Ok(self.conn().query_row(
            "SELECT CAST(strftime('%s', ?1, 'utc') AS INTEGER)",
            [minute],
            |r| r.get(0),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store;

    fn nightly() -> NewAutomation {
        NewAutomation {
            name: "Nightly summary".into(),
            schedule: "0 2 * * *".into(),
            agent: "Moebius".into(),
            prompt: "Sum up what the agents did today.".into(),
            repository: None,
            harness: false,
            enabled: true,
        }
    }

    #[test]
    fn an_automation_is_stored_changed_and_deleted_with_its_firings() {
        let (_dir, s) = test_store();
        let a = s.add_automation(&nightly(), "2026-10-29 08:00").unwrap();
        assert_eq!((a.name.as_str(), a.enabled), ("Nightly summary", true));
        assert_eq!(
            s.automation_checked(a.id).unwrap().as_deref(),
            Some("2026-10-29 08:00")
        );
        let off = NewAutomation {
            enabled: false,
            ..nightly()
        };
        assert!(s.update_automation(a.id, &off).unwrap());
        assert!(!s.automation(a.id).unwrap().unwrap().enabled);
        assert!(!s.update_automation(a.id + 1, &off).unwrap());

        let run = s.add_run(None, "Moebius", "pi-bolt", "chat").unwrap();
        s.set_run_status(run.id, "waiting").unwrap();
        let started = s
            .add_firing(a.id, "started", Some(run.id), false, "")
            .unwrap();
        assert_eq!(started.chat_status.as_deref(), Some("waiting"));
        s.add_firing(a.id, "skipped", None, true, "still runs")
            .unwrap();
        let firings = s.firings(a.id, 10).unwrap();
        assert_eq!(
            firings
                .iter()
                .map(|f| f.status.as_str())
                .collect::<Vec<_>>(),
            ["skipped", "started"]
        );
        assert_eq!(s.last_started_firing(a.id).unwrap().unwrap().id, started.id);
        assert_eq!(s.waiting_automation_chats().unwrap(), [run.id]);

        s.set_automations_checked("2026-10-29 08:05").unwrap();
        assert_eq!(
            s.automation_checked(a.id).unwrap().as_deref(),
            Some("2026-10-29 08:05")
        );
        assert!(s.delete_automation(a.id).unwrap());
        assert!(s.automations().unwrap().is_empty());
        assert!(s.firings(a.id, 10).unwrap().is_empty());
        // The chat it started stays.
        assert!(s.run(run.id).unwrap().is_some());
    }

    #[test]
    fn the_local_clock_reads_as_a_minute_and_back() {
        let (_dir, s) = test_store();
        let minute = s.local_minute().unwrap();
        let unix = s.local_minute_to_unix(&minute).unwrap().unwrap();
        assert!((now() - unix).abs() < 120, "{minute} is {unix}");
    }
}
