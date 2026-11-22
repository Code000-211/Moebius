//! What a person has chosen for themselves in the web app and wants in every browser: whether the thinking of
//! an agent's turn is shown open and whether long lines wrap. One row per person, made the first time they choose; a person with no row has the
//! defaults.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Result, Store};

/// The table of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE person_preferences (
    person_id INTEGER PRIMARY KEY REFERENCES people(id),
    show_thinking_expanded INTEGER NOT NULL DEFAULT 0
);
"#;

/// A person's preferences. `Default` is what a person who chose nothing has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Whether the thinking of a finished turn is shown open; a thought that is still arriving is always open.
    pub show_thinking_expanded: bool,
    /// Whether code blocks and long lines of chat answers, card threads and run pages wrap instead of scrolling sideways.
    pub wrap_long_lines: bool,
}

impl Store {
    /// The preferences of `person`; the defaults when they chose none.
    pub fn preferences(&self, person: i64) -> Result<Preferences> {
        let found = self.rows(
            "SELECT show_thinking_expanded, wrap_long_lines FROM person_preferences WHERE person_id = ?1",
            [person],
            |row| {
                Ok(Preferences {
                    show_thinking_expanded: row.get::<_, i64>(0)? != 0,
                    wrap_long_lines: row.get::<_, i64>(1)? != 0,
                })
            },
        )?;
        Ok(found.into_iter().next().unwrap_or_default())
    }

    /// Replaces the preferences of `person`.
    pub fn set_preferences(&self, person: i64, preferences: Preferences) -> Result<()> {
        self.conn().execute(
            "INSERT INTO person_preferences (person_id, show_thinking_expanded, wrap_long_lines) VALUES (?1, ?2, ?3)
             ON CONFLICT (person_id) DO UPDATE SET show_thinking_expanded = excluded.show_thinking_expanded,
                                                   wrap_long_lines = excluded.wrap_long_lines",
            params![
                person,
                preferences.show_thinking_expanded as i64,
                preferences.wrap_long_lines as i64
            ],
        )?;
        Ok(())
    }
}
