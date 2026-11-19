//! The repositories a card works in (ADR 0019) and the worktrees a run of it has in them. A card always works in its
//! own issue's repository (for a card of a project without repositories, the project's files) and in the ones named
//! after it in `card_repositories`, in the order they are named; `Card::repositories` holds the names, its own first.
//! A run of the card has a worktree in each, recorded in `run_worktrees` with the commit it started at: the first is
//! the run's own worktree (`runs.worktree`), where its harness starts, and the others are beside it. A run from before
//! has no rows; it worked in the card's own repository only.

use rusqlite::{OptionalExtension, params};

use super::{Result, Store};

/// The tables of this part. Their rows go with their card, run or repository.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE card_repositories (
    card_id INTEGER NOT NULL,
    repository_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (card_id, repository_id)
);
CREATE TABLE run_worktrees (
    run_id INTEGER NOT NULL,
    repository_id INTEGER NOT NULL,
    path TEXT NOT NULL,
    start TEXT NOT NULL,
    PRIMARY KEY (run_id, repository_id)
);
CREATE TRIGGER card_repositories_follow_their_card AFTER DELETE ON cards
BEGIN
    DELETE FROM card_repositories WHERE card_id = old.id;
END;
CREATE TRIGGER run_worktrees_follow_their_run AFTER DELETE ON runs
BEGIN
    DELETE FROM run_worktrees WHERE run_id = old.id;
END;
CREATE TRIGGER card_repositories_follow_their_repository AFTER DELETE ON repositories
BEGIN
    DELETE FROM card_repositories WHERE repository_id = old.id;
    DELETE FROM run_worktrees WHERE repository_id = old.id;
END;
"#;

/// The names of the repositories a card names besides its own, as a JSON array in their order, for a `SELECT` of
/// `cards`.
pub(super) const CARD_REPOSITORIES: &str = "(SELECT name FROM repositories WHERE id = cards.repository_id), (SELECT json_group_array(name) FROM (SELECT r.name FROM card_repositories c JOIN repositories r ON r.id = c.repository_id WHERE c.card_id = cards.id ORDER BY c.position))";

/// The names a card works in, read from the two columns [`CARD_REPOSITORIES`] gives at `index`: its own first.
pub(super) fn names(r: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Vec<String>> {
    let own: Option<String> = r.get(index)?;
    let others: Vec<String> =
        serde_json::from_str(&r.get::<_, String>(index + 1)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                index + 1,
                rusqlite::types::Type::Text,
                e.into(),
            )
        })?;
    Ok(own.into_iter().chain(others).collect())
}

/// One worktree of a run in one of its card's repositories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunWorktree {
    pub repository_id: i64,
    pub path: String,
    /// The commit the worktree was made at.
    pub start: String,
}

impl Store {
    /// The ids of the repositories the card names besides its own issue's, in their order.
    pub fn card_repositories(&self, card_id: i64) -> Result<Vec<i64>> {
        self.rows(
            "SELECT repository_id FROM card_repositories WHERE card_id = ?1 ORDER BY position",
            [card_id],
            |r| r.get(0),
        )
    }

    /// Replaces the repositories the card names besides its own with `repositories`, in that order.
    pub fn set_card_repositories(&self, card_id: i64, repositories: &[i64]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM card_repositories WHERE card_id = ?1",
            [card_id],
        )?;
        for (position, repository) in repositories.iter().enumerate() {
            tx.execute(
                "INSERT INTO card_repositories (card_id, repository_id, position) VALUES (?1, ?2, ?3)",
                params![card_id, repository, position as i64],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Ends the `code` run `run_id` of `card_id` as succeeded, as `set_run_status` does, and at once adds the
    /// repositories `added` to the card after the ones it names and sends it back to coding with `note`: in one
    /// transaction, so the board never sees the run succeeded without the note.
    pub fn succeed_asking_for_repositories(
        &self,
        run_id: i64,
        card_id: i64,
        added: &[i64],
        note: &str,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE runs SET status = 'succeeded', ended_at = COALESCE(?1, ended_at) WHERE id = ?2",
            params![super::now(), run_id],
        )?;
        for repository in added {
            tx.execute(
                "INSERT OR IGNORE INTO card_repositories (card_id, repository_id, position)
                 VALUES (?1, ?2, (SELECT COALESCE(MAX(position) + 1, 0) FROM card_repositories WHERE card_id = ?1))",
                params![card_id, repository],
            )?;
        }
        tx.execute(
            "UPDATE cards SET note = ?1 WHERE id = ?2",
            params![note, card_id],
        )?;
        Ok(tx.commit()?)
    }

    /// Records the worktree `path` of the run `run_id` in `repository_id`, made at the commit `start`.
    pub fn add_run_worktree(
        &self,
        run_id: i64,
        repository_id: i64,
        path: &str,
        start: &str,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO run_worktrees (run_id, repository_id, path, start) VALUES (?1, ?2, ?3, ?4)",
            params![run_id, repository_id, path, start],
        )?;
        Ok(())
    }

    /// The worktrees of the run, in the order its card names their repositories; empty for a run from before they
    /// were recorded and for a run without a card.
    pub fn run_worktrees(&self, run_id: i64) -> Result<Vec<RunWorktree>> {
        self.rows(
            "SELECT repository_id, path, start FROM run_worktrees WHERE run_id = ?1 ORDER BY rowid",
            [run_id],
            |r| {
                Ok(RunWorktree {
                    repository_id: r.get(0)?,
                    path: r.get(1)?,
                    start: r.get(2)?,
                })
            },
        )
    }

    /// The branch of the card's latest successful `code` run that had a worktree in `repository_id`, where the card's
    /// work in that repository stands. A run from before the worktrees were recorded counts for the card's own
    /// repository.
    pub fn code_branch_in(&self, card_id: i64, repository_id: i64) -> Result<Option<String>> {
        let id: Option<i64> = self
            .conn()
            .query_row(
                "SELECT r.id FROM runs r JOIN cards c ON c.id = r.card_id
                 WHERE r.card_id = ?1 AND r.purpose = 'code' AND r.status = 'succeeded'
                   AND (EXISTS (SELECT 1 FROM run_worktrees w WHERE w.run_id = r.id AND w.repository_id = ?2)
                        OR (c.repository_id = ?2 AND NOT EXISTS (SELECT 1 FROM run_worktrees w WHERE w.run_id = r.id)))
                 ORDER BY r.id DESC LIMIT 1",
                params![card_id, repository_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(id.map(|id| format!("moebius/run-{id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (Store::in_memory(dir.path()).unwrap(), dir)
    }

    #[test]
    fn a_card_works_in_its_own_repository_and_the_ones_it_names_in_their_order() {
        let (s, _dir) = store();
        let api = s.add_repository("api", "/r/api", "none", "main").unwrap();
        let web = s.add_repository("web", "/r/web", "none", "main").unwrap();
        let docs = s.add_repository("docs", "/r/docs", "none", "main").unwrap();
        let card = s.add_card(api.id, "Both", "", "leon", "Backlog").unwrap();
        assert_eq!(s.card(card.id).unwrap().unwrap().repositories, ["api"]);
        s.set_card_repositories(card.id, &[docs.id, web.id])
            .unwrap();
        assert_eq!(s.card_repositories(card.id).unwrap(), [docs.id, web.id]);
        assert_eq!(
            s.card(card.id).unwrap().unwrap().repositories,
            ["api", "docs", "web"]
        );
        s.set_card_repositories(card.id, &[]).unwrap();
        assert_eq!(s.card(card.id).unwrap().unwrap().repositories, ["api"]);
    }

    #[test]
    fn a_code_branch_in_a_repository_is_that_of_the_latest_code_run_with_a_worktree_there() {
        let (s, _dir) = store();
        let api = s.add_repository("api", "/r/api", "none", "main").unwrap();
        let web = s.add_repository("web", "/r/web", "none", "main").unwrap();
        let card = s.add_card(api.id, "Both", "", "leon", "Backlog").unwrap();
        // A run from before worktrees were recorded worked in the card's own repository.
        let old = s
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        s.set_run_status(old.id, "succeeded").unwrap();
        let old_branch = format!("moebius/run-{}", old.id);
        assert_eq!(s.code_branch_in(card.id, api.id).unwrap(), Some(old_branch));
        assert_eq!(s.code_branch_in(card.id, web.id).unwrap(), None);
        let both = s
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        s.add_run_worktree(both.id, api.id, "/w/api", "a").unwrap();
        s.add_run_worktree(both.id, web.id, "/w/web", "b").unwrap();
        s.set_run_status(both.id, "succeeded").unwrap();
        let branch = format!("moebius/run-{}", both.id);
        assert_eq!(
            s.code_branch_in(card.id, api.id).unwrap(),
            Some(branch.clone())
        );
        assert_eq!(s.code_branch_in(card.id, web.id).unwrap(), Some(branch));
        assert_eq!(
            s.run_worktrees(both.id)
                .unwrap()
                .iter()
                .map(|w| w.repository_id)
                .collect::<Vec<_>>(),
            [api.id, web.id]
        );
    }

    #[test]
    fn a_run_asking_for_a_repository_succeeds_with_it_added_and_the_card_noted() {
        let (s, _dir) = store();
        let api = s.add_repository("api", "/r/api", "none", "main").unwrap();
        let web = s.add_repository("web", "/r/web", "none", "main").unwrap();
        let docs = s.add_repository("docs", "/r/docs", "none", "main").unwrap();
        let card = s.add_card(api.id, "Both", "", "leon", "Backlog").unwrap();
        s.set_card_repositories(card.id, &[web.id]).unwrap();
        let run = s
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        s.succeed_asking_for_repositories(run.id, card.id, &[docs.id, web.id], "more")
            .unwrap();
        let card = s.card(card.id).unwrap().unwrap();
        assert_eq!(card.repositories, ["api", "web", "docs"]);
        assert_eq!(card.note.as_deref(), Some("more"));
        assert_eq!(s.run(run.id).unwrap().unwrap().status, "succeeded");
    }

    #[test]
    fn the_rows_go_with_their_card_and_run() {
        let (s, _dir) = store();
        let api = s.add_repository("api", "/r/api", "none", "main").unwrap();
        let web = s.add_repository("web", "/r/web", "none", "main").unwrap();
        let card = s.add_card(api.id, "Both", "", "leon", "Backlog").unwrap();
        s.set_card_repositories(card.id, &[web.id]).unwrap();
        let run = s
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        s.add_run_worktree(run.id, web.id, "/w/web", "b").unwrap();
        assert!(s.delete_card(card.id, false).unwrap());
        assert!(s.card_repositories(card.id).unwrap().is_empty());
        assert!(s.run_worktrees(run.id).unwrap().is_empty());
    }
}
