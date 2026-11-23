//! Commands in issue comments (ADR 0028): what the store keeps so that `/take`, `/release` and `/done` in a comment on
//! an issue are read once and limited. A repository says whether commands are on, how many open issues a login may hold
//! that it took by command, and whether the server answers on the forge (columns of `repositories`); `commands_since`
//! is when they were turned on, so that comments from before are never read as commands. Each issue remembers the
//! newest comment whose commands were read (`command_seen`), and a card remembers the login it took by command
//! (`cards.command_taker`).

use rusqlite::{OptionalExtension, params};

use super::{Result, Store, now};

/// The tables and columns of this part.
pub(super) const SCHEMA: &str = r#"
ALTER TABLE repositories ADD COLUMN commands INTEGER NOT NULL DEFAULT 0;
ALTER TABLE repositories ADD COLUMN command_limit INTEGER NOT NULL DEFAULT 3;
ALTER TABLE repositories ADD COLUMN answer_on_forge INTEGER NOT NULL DEFAULT 1;
ALTER TABLE repositories ADD COLUMN commands_since INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cards ADD COLUMN command_taker TEXT;
CREATE TABLE command_seen (
    repository_id INTEGER NOT NULL,
    number INTEGER NOT NULL,
    comment_id INTEGER NOT NULL,
    PRIMARY KEY (repository_id, number)
);
"#;

/// How many open issues a login may hold that it took by command until the repository says otherwise.
pub const DEFAULT_COMMAND_LIMIT: i64 = 3;

impl Store {
    /// Sets whether commands are read, how many issues a login may hold, and whether the server answers on the forge.
    /// Turning commands on records the time, in seconds, from which comments count.
    pub fn set_repository_commands(
        &self,
        id: i64,
        commands: bool,
        limit: i64,
        answer_on_forge: bool,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE repositories SET commands_since = CASE WHEN ?1 AND NOT commands THEN ?5 ELSE commands_since END, \
             commands = ?1, command_limit = ?2, answer_on_forge = ?3 WHERE id = ?4",
            params![commands, limit, answer_on_forge, id, now()],
        )?;
        Ok(())
    }

    /// The time, in seconds since the epoch, from which comments of the repository count as commands.
    pub fn commands_since(&self, repository_id: i64) -> Result<i64> {
        Ok(self.read().query_row(
            "SELECT commands_since FROM repositories WHERE id = ?1",
            [repository_id],
            |r| r.get(0),
        )?)
    }

    /// The id of the newest comment of the issue whose commands were read; 0 when none was.
    pub fn command_seen(&self, repository_id: i64, number: i64) -> Result<i64> {
        Ok(self
            .read()
            .query_row(
                "SELECT comment_id FROM command_seen WHERE repository_id = ?1 AND number = ?2",
                params![repository_id, number],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// Records that the commands of the issue's comments up to `comment_id` were read.
    pub fn set_command_seen(&self, repository_id: i64, number: i64, comment_id: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO command_seen (repository_id, number, comment_id) VALUES (?1, ?2, ?3) \
             ON CONFLICT (repository_id, number) DO UPDATE SET comment_id = MAX(comment_id, ?3)",
            params![repository_id, number, comment_id],
        )?;
        Ok(())
    }

    /// Records the login the card was taken by through `/take`, or that none was.
    pub fn set_command_taker(&self, card_id: i64, login: Option<&str>) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET command_taker = ?1 WHERE id = ?2",
            params![login, card_id],
        )?;
        Ok(())
    }

    /// The login the card was taken by through `/take`, while it is one of its assignees on the forge.
    pub fn command_taker(&self, card_id: i64) -> Result<Option<String>> {
        Ok(self
            .read()
            .query_row(
                "SELECT command_taker FROM cards WHERE id = ?1 AND EXISTS (SELECT 1 FROM json_each(forge_assignees) j \
                 WHERE j.value = cards.command_taker COLLATE NOCASE)",
                [card_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    /// How many open issues of the repository `login` holds that it took with `/take` and is still assigned to.
    pub fn command_takes(&self, repository_id: i64, login: &str) -> Result<i64> {
        Ok(self.read().query_row(
            "SELECT COUNT(*) FROM cards WHERE repository_id = ?1 AND command_taker = ?2 COLLATE NOCASE \
             AND NOT (closed OR column_name = 'Done') \
             AND EXISTS (SELECT 1 FROM json_each(forge_assignees) j WHERE j.value = ?2 COLLATE NOCASE)",
            params![repository_id, login],
            |r| r.get(0),
        )?)
    }

    /// Whether the comment of the forge `forge_id` on the card was written through Moebius: the server's own.
    pub fn comment_is_ours(&self, card_id: i64, forge_id: i64) -> Result<bool> {
        Ok(self
            .read()
            .query_row(
                "SELECT local FROM card_comments WHERE card_id = ?1 AND forge_id = ?2",
                params![card_id, forge_id],
                |r| r.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(false))
    }
}
