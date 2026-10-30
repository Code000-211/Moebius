//! Repositories and their cards: a repository Moebius works in, the cards of its issues, the issues that block a card,
//! and the moves of a card from column to column.

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub forge: String,
    pub default_branch: String,
    /// The repository's name on GitHub, as `owner/repo`; none for other forges.
    pub github: Option<String>,
    /// The logins on its forge whose issues move on without a person's approval.
    pub trusted: Vec<String>,
    /// The repository's path on a forge other than GitHub, such as `group/subgroup/project` on GitLab or `owner/repo`
    /// on Forgejo; none for GitHub and without a forge.
    #[serde(default)]
    pub project: Option<String>,
}

impl Repository {
    /// Where new work starts and scripts are read from: the default branch as the forge has it. For GitHub that is
    /// `origin`'s, which the poll keeps up to date; without a forge it is the clone's own.
    pub fn base_ref(&self) -> String {
        match self.forge.as_str() {
            "none" => self.default_branch.clone(),
            _ => format!("origin/{}", self.default_branch),
        }
    }

    /// The repository's name on its forge: `github` on GitHub, `project` on any other forge; none without one.
    pub fn forge_name(&self) -> Option<&str> {
        match self.forge.as_str() {
            "github" => self.github.as_deref(),
            _ => self.project.as_deref(),
        }
    }
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
    /// Why the card went back to coding, for the next `code` run; cleared when a `code` run succeeds.
    pub note: Option<String>,
    /// The id of the card's latest run when a person last retried it; its rounds, reviews and tokens count only the
    /// runs after it.
    pub retried_after: Option<i64>,
    /// Whether the card was made from a grilling a person confirmed, which needs no approval.
    pub confirmed: bool,
}

/// A move of a card from one column to another, with the card's note at the time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardMove {
    pub from: String,
    pub to: String,
    pub note: Option<String>,
    pub at: i64,
}

impl Store {
    pub fn add_repository(
        &self,
        name: &str,
        path: &str,
        forge: &str,
        default_branch: &str,
    ) -> Result<Repository> {
        self.insert_repository(name, path, forge, default_branch, None, &[])
    }

    /// Adds a repository whose issues live on GitHub as `github` (`owner/repo`); `trusted` are the logins whose
    /// issues need no approval.
    pub fn add_github_repository(
        &self,
        name: &str,
        path: &str,
        default_branch: &str,
        github: &str,
        trusted: &[String],
    ) -> Result<Repository> {
        self.insert_repository(name, path, "github", default_branch, Some(github), trusted)
    }

    /// Adds a repository whose issues live on `forge` (such as `gitlab`) as `project`, its path there; `trusted` are
    /// the logins whose issues need no approval.
    pub fn add_project_repository(
        &self,
        name: &str,
        path: &str,
        default_branch: &str,
        forge: &str,
        project: &str,
        trusted: &[String],
    ) -> Result<Repository> {
        self.insert_repository(name, path, forge, default_branch, Some(project), trusted)
    }

    fn insert_repository(
        &self,
        name: &str,
        path: &str,
        forge: &str,
        default_branch: &str,
        forge_name: Option<&str>,
        trusted: &[String],
    ) -> Result<Repository> {
        // The name on GitHub has a column of its own; any other forge's is the project.
        let (github, project) = match forge {
            "github" => (forge_name, None),
            _ => (None, forge_name),
        };
        let conn = self.conn();
        conn.execute(
            "INSERT INTO repositories (name, path, forge, default_branch, github, trusted, project)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                name,
                path,
                forge,
                default_branch,
                github,
                serde_json::to_string(trusted)?,
                project
            ],
        )?;
        Ok(Repository {
            id: conn.last_insert_rowid(),
            name: name.into(),
            path: path.into(),
            forge: forge.into(),
            default_branch: default_branch.into(),
            github: github.map(Into::into),
            trusted: trusted.to_vec(),
            project: project.map(Into::into),
        })
    }

    pub fn repositories(&self) -> Result<Vec<Repository>> {
        self.rows("SELECT id, name, path, forge, default_branch, github, trusted, project FROM repositories ORDER BY id", [], |r| {
            let trusted: String = r.get(6)?;
            Ok(Repository {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
                forge: r.get(3)?,
                default_branch: r.get(4)?,
                github: r.get(5)?,
                trusted: serde_json::from_str(&trusted).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        e.into(),
                    )
                })?,
                project: r.get(7)?,
            })
        })
    }

    /// Replaces the logins whose issues on the repository's forge need no approval.
    pub fn set_repository_trusted(&self, id: i64, trusted: &[String]) -> Result<()> {
        self.conn().execute(
            "UPDATE repositories SET trusted = ?1 WHERE id = ?2",
            params![serde_json::to_string(trusted)?, id],
        )?;
        Ok(())
    }

    pub fn repository(&self, id: i64) -> Result<Option<Repository>> {
        Ok(self.repositories()?.into_iter().find(|r| r.id == id))
    }

    /// Removes a repository and everything stored for it: its cards with their changes, runs, decisions, inbox,
    /// moves and dependencies; its chats and grillings; its lessons, research, automations and localize index. The
    /// clone and worktrees on disk belong to the caller. Says whether such a repository was stored.
    ///
    /// The foreign keys decide the order: whatever points at a card, a run, a change or a judgement goes first.
    pub fn delete_repository(&self, id: i64) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let stored = tx
            .query_row("SELECT 1 FROM repositories WHERE id = ?1", [id], |_| Ok(()))
            .optional()?
            .is_some();
        if !stored {
            return Ok(false);
        }
        let cards = "SELECT id FROM cards WHERE repository_id = ?1";
        let runs = format!("SELECT id FROM runs WHERE repository_id = ?1 OR card_id IN ({cards})");
        let changes =
            format!("SELECT id FROM changes WHERE repository_id = ?1 OR card_id IN ({cards})");
        let judgements =
            format!("SELECT id FROM judgements WHERE run_id IN ({runs}) OR card_id IN ({cards})");
        let research = "SELECT id FROM research WHERE repository_id = ?1";
        let proposals = "SELECT id FROM proposals WHERE repository_id = ?1";
        let statements = [
            format!("DELETE FROM lesson_uses WHERE card_id IN ({cards})"),
            format!("DELETE FROM events WHERE run_id IN ({runs})"),
            format!("DELETE FROM usage WHERE run_id IN ({runs})"),
            format!(
                "DELETE FROM replays WHERE judgement_id IN ({judgements}) OR run_id IN ({runs})"
            ),
            format!("DELETE FROM shadows WHERE judgement_id IN ({judgements})"),
            format!("DELETE FROM judgements WHERE id IN ({judgements})"),
            format!("DELETE FROM inbox_items WHERE card_id IN ({cards})"),
            format!("DELETE FROM dependencies WHERE card_id IN ({cards})"),
            format!("DELETE FROM card_moves WHERE card_id IN ({cards})"),
            format!("DELETE FROM attempts WHERE run_id IN ({runs}) OR card_id IN ({cards})"),
            format!(
                "DELETE FROM trials WHERE proposal_id IN ({proposals}) OR run_id IN ({runs}) OR card_id IN ({cards})"
            ),
            format!(
                "DELETE FROM research_runs WHERE research_id IN ({research}) OR run_id IN ({runs})"
            ),
            format!(
                "DELETE FROM firings WHERE run_id IN ({runs}) OR automation_id IN (SELECT id FROM automations WHERE repository_id = ?1)"
            ),
            format!("UPDATE proposals SET change_id = NULL WHERE change_id IN ({changes})"),
            format!("DELETE FROM proposals WHERE id IN ({proposals})"),
            format!("UPDATE retros SET run_id = NULL WHERE run_id IN ({runs})"),
            format!(
                "DELETE FROM experiments WHERE research_id IN ({research}) OR change_id IN ({changes})"
            ),
            format!("DELETE FROM research WHERE id IN ({research})"),
            format!("DELETE FROM grillings WHERE repository_id = ?1 OR run_id IN ({runs})"),
            "DELETE FROM automations WHERE repository_id = ?1".to_string(),
            "DELETE FROM localize_indexed WHERE repository_id = ?1".to_string(),
            "DELETE FROM lessons WHERE repository_id = ?1".to_string(),
            format!("DELETE FROM runs WHERE id IN ({runs})"),
            format!("DELETE FROM changes WHERE id IN ({changes})"),
            format!("DELETE FROM cards WHERE id IN ({cards})"),
            "DELETE FROM repositories WHERE id = ?1".to_string(),
        ];
        for statement in &statements {
            tx.execute(statement, [id])?;
        }
        tx.execute(
            &format!("DROP TABLE IF EXISTS {}", super::localize::table(id)),
            [],
        )?;
        tx.commit()?;
        Ok(true)
    }

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
        Self::insert_card(&conn, repository_id, number, title, body, author, column)
    }

    /// Adds a card with the number of its issue on the forge.
    pub fn add_numbered_card(
        &self,
        repository_id: i64,
        number: i64,
        title: &str,
        body: &str,
        author: &str,
        column: &str,
    ) -> Result<Card> {
        let conn = self.conn();
        Self::insert_card(&conn, repository_id, number, title, body, author, column)
    }

    fn insert_card(
        conn: &Connection,
        repository_id: i64,
        number: i64,
        title: &str,
        body: &str,
        author: &str,
        column: &str,
    ) -> Result<Card> {
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
            note: None,
            retried_after: None,
            confirmed: false,
        })
    }

    fn cards_where(&self, clause: &str, params: impl rusqlite::Params) -> Result<Vec<Card>> {
        self.rows(&format!(
            "SELECT id, repository_id, number, title, body, author, column_name, created_at, note, retried_after, confirmed FROM cards WHERE {clause} ORDER BY id"
        ), params, |r| {
            Ok(Card {
                id: r.get(0)?,
                repository_id: r.get(1)?,
                number: r.get(2)?,
                title: r.get(3)?,
                body: r.get(4)?,
                author: r.get(5)?,
                column: r.get(6)?,
                created_at: r.get(7)?,
                note: r.get(8)?,
                retried_after: r.get(9)?,
                confirmed: r.get(10)?,
            })
        })
    }

    pub fn card(&self, id: i64) -> Result<Option<Card>> {
        Ok(self.cards_where("id = ?1", [id])?.pop())
    }

    pub fn cards(&self, repository_id: i64) -> Result<Vec<Card>> {
        self.cards_where("repository_id = ?1", [repository_id])
    }

    /// The card of issue `number` in a repository.
    pub fn card_by_number(&self, repository_id: i64, number: i64) -> Result<Option<Card>> {
        Ok(self
            .cards_where(
                "repository_id = ?1 AND number = ?2",
                [repository_id, number],
            )?
            .pop())
    }

    pub fn set_card_text(&self, id: i64, title: &str, body: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET title = ?1, body = ?2 WHERE id = ?3",
            params![title, body, id],
        )?;
        Ok(())
    }

    /// Adds a card made from a confirmed grilling, which needs no approval, with the numbers of the issues that block
    /// it, in one step so that no script sees it without them. `number` is the number of its issue on the forge; none
    /// takes the next number. A card the poll made for that issue already is confirmed instead.
    #[allow(clippy::too_many_arguments)]
    pub fn add_confirmed_card(
        &self,
        repository_id: i64,
        number: Option<i64>,
        title: &str,
        body: &str,
        author: &str,
        column: &str,
        blocked_by: &BTreeSet<i64>,
    ) -> Result<Card> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let number = match number {
            Some(number) => number,
            None => tx.query_row(
                "SELECT COALESCE(MAX(number), 0) + 1 FROM cards WHERE repository_id = ?1",
                [repository_id],
                |r| r.get(0),
            )?,
        };
        let id = match tx
            .query_row(
                "SELECT id FROM cards WHERE repository_id = ?1 AND number = ?2",
                [repository_id, number],
                |r| r.get(0),
            )
            .optional()?
        {
            Some(id) => id,
            None => Self::insert_card(&tx, repository_id, number, title, body, author, column)?.id,
        };
        tx.execute("UPDATE cards SET confirmed = 1 WHERE id = ?1", [id])?;
        tx.execute("DELETE FROM dependencies WHERE card_id = ?1", [id])?;
        for blocker in blocked_by {
            tx.execute(
                "INSERT INTO dependencies (card_id, blocked_by) VALUES (?1, ?2)",
                [id, *blocker],
            )?;
        }
        tx.commit()?;
        drop(conn);
        Ok(self.card(id)?.expect("the card was just stored"))
    }

    /// Replaces the numbers of the issues that block a card.
    pub fn set_dependencies(&self, card_id: i64, blocked_by: &BTreeSet<i64>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM dependencies WHERE card_id = ?1", [card_id])?;
        for number in blocked_by {
            tx.execute(
                "INSERT INTO dependencies (card_id, blocked_by) VALUES (?1, ?2)",
                [card_id, *number],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// The numbers of the issues that block a card, in order.
    pub fn dependencies(&self, card_id: i64) -> Result<Vec<i64>> {
        self.rows(
            "SELECT blocked_by FROM dependencies WHERE card_id = ?1 ORDER BY blocked_by",
            [card_id],
            |r| r.get(0),
        )
    }

    pub fn set_card_note(&self, id: i64, note: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET note = ?1 WHERE id = ?2",
            params![note, id],
        )?;
        Ok(())
    }

    /// Gives the card a fresh start: the runs it has now no longer count toward its limits.
    pub fn set_card_retried(&self, id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET retried_after = (SELECT MAX(id) FROM runs WHERE card_id = ?1) WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    /// Moves a card and records the move, with the card's note as it is now.
    pub fn move_card(&self, id: i64, column: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO card_moves (card_id, from_column, to_column, note, at)
             SELECT id, column_name, ?1, note, ?2 FROM cards WHERE id = ?3",
            params![column, now(), id],
        )?;
        conn.execute(
            "UPDATE cards SET column_name = ?1 WHERE id = ?2",
            params![column, id],
        )?;
        Ok(())
    }

    /// The moves of a card since moves were recorded, oldest first.
    pub fn moves_for_card(&self, card_id: i64) -> Result<Vec<CardMove>> {
        self.rows("SELECT from_column, to_column, note, at FROM card_moves WHERE card_id = ?1 ORDER BY id", [card_id], |r| {
            Ok(CardMove {
                from: r.get(0)?,
                to: r.get(1)?,
                note: r.get(2)?,
                at: r.get(3)?,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store as store;

    #[test]
    fn a_repository_on_another_forge_keeps_its_project_path() {
        let (_dir, s) = store();
        let trusted = vec!["leon".to_string()];
        let repo = s
            .add_project_repository(
                "widgets",
                "/srv/widgets",
                "main",
                "gitlab",
                "acme/tools/widgets",
                &trusted,
            )
            .unwrap();
        assert_eq!(s.repository(repo.id).unwrap(), Some(repo.clone()));
        assert_eq!(
            (
                repo.forge.as_str(),
                repo.github.as_deref(),
                repo.forge_name()
            ),
            ("gitlab", None, Some("acme/tools/widgets"))
        );
        assert_eq!(repo.base_ref(), "origin/main");
        let github = s
            .add_github_repository("gadgets", "/srv/gadgets", "main", "acme/gadgets", &[])
            .unwrap();
        assert_eq!(
            (github.forge_name(), github.project.as_deref()),
            (Some("acme/gadgets"), None)
        );
    }

    #[test]
    fn github_repositories_numbered_cards_and_dependencies_round_trip() {
        let (_dir, s) = store();
        let trusted = vec!["leon".to_string()];
        let repo = s
            .add_github_repository("widgets", "/srv/widgets", "main", "acme/widgets", &trusted)
            .unwrap();
        assert_eq!(s.repository(repo.id).unwrap(), Some(repo.clone()));
        assert_eq!(
            (repo.forge.as_str(), repo.github.as_deref(), &repo.trusted),
            ("github", Some("acme/widgets"), &trusted)
        );

        let card = s
            .add_numbered_card(repo.id, 41, "Issue", "body", "leon", "Backlog")
            .unwrap();
        assert_eq!(s.card_by_number(repo.id, 41).unwrap(), Some(card.clone()));
        assert_eq!(s.card_by_number(repo.id, 42).unwrap(), None);
        s.set_card_text(card.id, "Renamed", "new body").unwrap();
        let stored = s.card(card.id).unwrap().unwrap();
        assert_eq!(
            (stored.title.as_str(), stored.body.as_str()),
            ("Renamed", "new body")
        );

        s.set_dependencies(card.id, &BTreeSet::from([9, 3]))
            .unwrap();
        assert_eq!(s.dependencies(card.id).unwrap(), [3, 9]);
        s.set_dependencies(card.id, &BTreeSet::from([9, 12]))
            .unwrap();
        assert_eq!(s.dependencies(card.id).unwrap(), [9, 12]);
    }

    #[test]
    fn a_confirmed_card_has_its_dependencies_from_the_start_and_adopts_a_card_of_the_poll() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/tmp/demo", "none", "main")
            .unwrap();
        let blockers = BTreeSet::from([4]);
        let first = s
            .add_confirmed_card(
                repo.id,
                None,
                "First",
                "",
                "person",
                "Backlog",
                &BTreeSet::new(),
            )
            .unwrap();
        let second = s
            .add_confirmed_card(repo.id, None, "Second", "", "person", "Backlog", &blockers)
            .unwrap();
        assert_eq!((first.number, second.number), (1, 2));
        assert!(first.confirmed && second.confirmed);
        assert_eq!(s.dependencies(second.id).unwrap(), [4]);
        assert!(
            !s.add_card(repo.id, "Other", "", "leon", "Backlog")
                .unwrap()
                .confirmed
        );

        // The poll made a card for the issue just opened: it is confirmed instead of added again.
        let polled = s
            .add_numbered_card(repo.id, 40, "Polled", "", "bot", "Backlog")
            .unwrap();
        assert!(!polled.confirmed);
        let adopted = s
            .add_confirmed_card(
                repo.id,
                Some(40),
                "Polled",
                "",
                "person",
                "Backlog",
                &blockers,
            )
            .unwrap();
        assert_eq!(adopted.id, polled.id);
        assert!(adopted.confirmed);
        assert_eq!(s.dependencies(polled.id).unwrap(), [4]);
    }

    #[test]
    fn deleting_a_repository_takes_its_cards_changes_and_index_with_it() {
        let (_dir, s) = store();
        let doomed = s
            .add_github_repository("widgets", "/srv/widgets", "main", "acme/widgets", &[])
            .unwrap();
        let kept = s
            .add_github_repository("gadgets", "/srv/gadgets", "main", "acme/gadgets", &[])
            .unwrap();
        let card = s
            .add_numbered_card(doomed.id, 1, "Issue", "body", "leon", "Backlog")
            .unwrap();
        let change = s
            .add_change(doomed.id, Some(card.id), "moebius/issue-1", None, "abc")
            .unwrap();
        let other = s
            .add_numbered_card(kept.id, 2, "Other", "body", "leon", "Backlog")
            .unwrap();
        s.set_dependencies(other.id, &BTreeSet::from([1])).unwrap();
        s.replace_localize_index(
            doomed.id,
            "abc",
            &[crate::store::localize::IndexedFile {
                path: "src/lib.rs".into(),
                path_words: "src lib.rs".into(),
                words: "fn main".into(),
            }],
        )
        .unwrap();

        assert!(s.delete_repository(doomed.id).unwrap());
        assert!(!s.delete_repository(doomed.id).unwrap());
        assert_eq!(s.repository(doomed.id).unwrap(), None);
        assert_eq!(s.card(card.id).unwrap(), None);
        assert_eq!(s.change(change.id).unwrap(), None);
        assert_eq!(s.localize_index_commit(doomed.id).unwrap(), None);
        assert_eq!(s.repositories().unwrap(), vec![kept]);
        assert_eq!(
            s.card(other.id).unwrap().map(|c| c.title),
            Some("Other".into())
        );
    }
}
