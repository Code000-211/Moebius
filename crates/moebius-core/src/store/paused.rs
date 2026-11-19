//! Whether the server's runs are paused: one row that exists while a person has paused all agents, with who did it and
//! when. It is kept in the store, so a restart leaves the server paused until a person resumes it.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// The table of this part: at most one row, with the id 1.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE runs_paused (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    by TEXT NOT NULL,
    at INTEGER NOT NULL
);
"#;

/// That all agents are paused: who paused them and when (Unix seconds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paused {
    pub by: String,
    pub at: i64,
}

impl Store {
    /// The pause that is in force, if any.
    pub fn runs_paused(&self) -> Result<Option<Paused>> {
        Ok(self
            .conn()
            .query_row("SELECT by, at FROM runs_paused WHERE id = 1", [], |r| {
                Ok(Paused {
                    by: r.get(0)?,
                    at: r.get(1)?,
                })
            })
            .optional()?)
    }

    /// Pauses all agents as `by`. False when they were paused already, which keeps who paused them and when.
    pub fn pause_runs(&self, by: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "INSERT OR IGNORE INTO runs_paused (id, by, at) VALUES (1, ?1, ?2)",
            params![by, now()],
        )? > 0)
    }

    /// Resumes all agents. False when they were not paused.
    pub fn resume_runs(&self) -> Result<bool> {
        Ok(self.conn().execute("DELETE FROM runs_paused", [])? > 0)
    }
}

#[cfg(test)]
mod tests {
    use crate::store::test_store as store;

    #[test]
    fn the_first_pause_is_kept_until_the_agents_are_resumed() {
        let (_dir, s) = store();
        assert_eq!(s.runs_paused().unwrap(), None);
        assert!(!s.resume_runs().unwrap());
        assert!(s.pause_runs("person:ada").unwrap());
        assert!(!s.pause_runs("person:bob").unwrap());
        assert_eq!(s.runs_paused().unwrap().unwrap().by, "person:ada");
        assert!(s.resume_runs().unwrap());
        assert_eq!(s.runs_paused().unwrap(), None);
    }
}
