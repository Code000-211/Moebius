//! Repositories and their cards: a repository Moebius works in, the cards of its issues, the issues that block a card,
//! the moves of a card from column to column, whether a person or the Moebius agent holds a card, and the numbers of
//! the issues whose cards were deleted. A new repository gets its home project here, and a new card lands on it
//! (see `projects`).

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
    /// The project the repository's new issues land in (see `projects`).
    #[serde(default)]
    pub home_project_id: i64,
    /// The project whose own files this repository is (ADR 0019): a git repository Moebius keeps for a project without
    /// a forge, which is no repository of the project's but holds the work of its cards that touch no other. None for
    /// every repository a person added.
    #[serde(default)]
    pub files_of: Option<i64>,
    /// Whether the server keeps the label `ready_label` on the repository's open issues that are approved and have no
    /// open blocker (see `app::ready`).
    #[serde(default)]
    pub mark_ready: bool,
    /// The label the server marks ready issues with.
    #[serde(default = "default_ready_label")]
    pub ready_label: String,
    /// Which issues the repository's agents take: `all` approved ones, or `labelled` only those carrying `ready_label`
    /// that nobody else took.
    #[serde(default = "default_agents_take")]
    pub agents_take: String,
}

fn default_ready_label() -> String {
    super::taking::DEFAULT_READY_LABEL.into()
}

fn default_agents_take() -> String {
    "all".into()
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
    /// The project whose board the card is on.
    #[serde(default)]
    pub project_id: i64,
    /// The number of the card's issue in its repository: the forge's number for an issue of a forge.
    pub number: i64,
    /// The number the card has in its project, which names it with the project's key (`WEB-24`); unique in the project.
    /// A new card takes its issue's number when the project has no card with that number, else the next free one.
    #[serde(default)]
    pub project_number: i64,
    /// The key of the card's project, such as `WEB`; kept when the project is renamed.
    #[serde(default)]
    pub project_key: String,
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
    /// Set while a person or the Moebius agent holds the card: the board runs no script for it and starts no run.
    #[serde(default)]
    pub held: Option<Held>,
    /// The number of the issue this one is a sub-issue of, in the same repository.
    #[serde(default)]
    pub parent: Option<i64>,
    /// Whether the issue is closed: it was closed, or its card is in Done.
    #[serde(default)]
    pub closed: bool,
    /// The labels of the issue, in order.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Who claimed the issue: `person`, `agent:<name>` or a name a session gave; none while it is free.
    #[serde(default)]
    pub claimed_by: Option<String>,
    /// The handles of the people the card is given to, in the order the people were added; empty while it is no one's.
    #[serde(default)]
    pub assignees: Vec<String>,
    /// How soon the card should be done, from its `priority:` labels; `none` while it has none.
    #[serde(default)]
    pub priority: super::planning::Priority,
    /// The day the card is due, as `YYYY-MM-DD`, from its `due:` label; none while it has none.
    #[serde(default)]
    pub due: Option<String>,
    /// The card's estimate in points, from its `estimate:` label; none while it has none.
    #[serde(default)]
    pub estimate: Option<u32>,
    /// When the card last moved, was edited or was released from a hold (Unix seconds); scripts see it as
    /// `card.moved_at`.
    #[serde(default)]
    pub moved_at: i64,
    /// When a script archived the card (Unix seconds); none while it is on the board. An archived card stays in the
    /// store, in the issue list and in the analytics, and leaves the board; a move brings it back.
    #[serde(default)]
    pub archived_at: Option<i64>,
    /// The name of the card's kind of issue (see `kinds`); none while it has none.
    #[serde(default)]
    pub kind: Option<String>,
    /// The values of the fields of the card's kind that are set, by field name: a number for a `number` field, text for
    /// the others (a `person` field holds a handle, a `date` field `YYYY-MM-DD`).
    #[serde(default)]
    pub fields: std::collections::BTreeMap<String, serde_json::Value>,
    /// The names of the repositories the card works in, in the order it names them: its own issue's repository first,
    /// then those a person, the Moebius agent or a run asked for (see `card_repositories`).
    #[serde(default)]
    pub repositories: Vec<String>,
    /// The forge login the issue is assigned to when that is none of this server's people: someone on another server
    /// took it, and this server leaves it alone (ADR 0027). None while the issue is nobody's or a person's of this server.
    #[serde(default)]
    pub taken_by: Option<String>,
    /// Why the server could not assign its person on the forge when they took the issue; none when it could, or
    /// nobody took it here.
    #[serde(default)]
    pub assign_problem: Option<String>,
}

/// Why and by whom a card is held (blocked), and since when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Held {
    pub reason: Option<String>,
    /// `person`, or `agent:<name>` for the Moebius agent.
    pub by: String,
    pub at: i64,
}

/// A move of a card from one column to another, with the card's note at the time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardMove {
    pub from: String,
    pub to: String,
    pub note: Option<String>,
    pub at: i64,
}

/// The statements that delete what is stored for some cards and their runs, before the runs, the changes and the cards
/// themselves (see [`card_records_last`]): each argument is a query for ids that uses the statement's one parameter.
/// `judgements` are those of the cards and their runs. Whatever points at a run, a change or a judgement goes first.
fn card_statements(cards: &str, runs: &str, judgements: &str) -> Vec<String> {
    vec![
        format!("DELETE FROM lesson_uses WHERE card_id IN ({cards})"),
        format!("DELETE FROM events WHERE run_id IN ({runs})"),
        format!("DELETE FROM usage WHERE run_id IN ({runs})"),
        format!("DELETE FROM replays WHERE judgement_id IN ({judgements}) OR run_id IN ({runs})"),
        format!("DELETE FROM shadows WHERE judgement_id IN ({judgements})"),
        format!("DELETE FROM judgements WHERE id IN ({judgements})"),
        format!(
            "DELETE FROM inbox_assignees WHERE item_id IN (SELECT id FROM inbox_items WHERE card_id IN ({cards}))"
        ),
        format!("DELETE FROM card_assignees WHERE card_id IN ({cards})"),
        format!("DELETE FROM milestone_cards WHERE card_id IN ({cards})"),
        format!("DELETE FROM cycle_cards WHERE card_id IN ({cards})"),
        format!("DELETE FROM card_follows WHERE card_id IN ({cards})"),
        format!("DELETE FROM notices WHERE card_id IN ({cards})"),
        format!("DELETE FROM card_fields WHERE card_id IN ({cards})"),
        format!("DELETE FROM inbox_items WHERE card_id IN ({cards})"),
        format!("DELETE FROM dependencies WHERE card_id IN ({cards})"),
        format!("DELETE FROM card_links WHERE card_id IN ({cards}) OR other_id IN ({cards})"),
        format!("DELETE FROM card_labels WHERE card_id IN ({cards})"),
        format!("DELETE FROM card_comments WHERE card_id IN ({cards})"),
        format!("DELETE FROM card_moves WHERE card_id IN ({cards})"),
        format!("DELETE FROM attempts WHERE run_id IN ({runs}) OR card_id IN ({cards})"),
        format!("DELETE FROM trials WHERE run_id IN ({runs}) OR card_id IN ({cards})"),
        format!("DELETE FROM research_runs WHERE run_id IN ({runs})"),
        format!("DELETE FROM firings WHERE run_id IN ({runs})"),
        format!("UPDATE retros SET run_id = NULL WHERE run_id IN ({runs})"),
    ]
}

/// The last statements of removing cards: their runs, their changes and the cards.
fn card_records_last(runs: &str, changes: &str, cards: &str) -> Vec<String> {
    vec![
        format!("DELETE FROM runs WHERE id IN ({runs})"),
        format!("DELETE FROM changes WHERE id IN ({changes})"),
        format!("DELETE FROM cards WHERE id IN ({cards})"),
    ]
}

const REPOSITORY_COLUMNS: &str = "id, name, path, forge, default_branch, github, trusted, project, home_project_id, files_of, mark_ready, ready_label, agents_take";

fn repository_from_row(r: &rusqlite::Row) -> rusqlite::Result<Repository> {
    let trusted: String = r.get(6)?;
    Ok(Repository {
        id: r.get(0)?,
        name: r.get(1)?,
        path: r.get(2)?,
        forge: r.get(3)?,
        default_branch: r.get(4)?,
        github: r.get(5)?,
        trusted: serde_json::from_str(&trusted).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, e.into())
        })?,
        project: r.get(7)?,
        home_project_id: r.get(8)?,
        files_of: r.get(9)?,
        mark_ready: r.get(10)?,
        ready_label: r.get(11)?,
        agents_take: r.get(12)?,
    })
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
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let home = super::projects::insert_home_project(&tx, name)?;
        tx.execute(
            "INSERT INTO repositories (name, path, forge, default_branch, github, trusted, project, home_project_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                name,
                path,
                forge,
                default_branch,
                github,
                serde_json::to_string(trusted)?,
                project,
                home
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO project_repositories (project_id, repository_id, position) VALUES (?1, ?2, 0)",
            [home, id],
        )?;
        tx.commit()?;
        Ok(Repository {
            id,
            name: name.into(),
            path: path.into(),
            forge: forge.into(),
            default_branch: default_branch.into(),
            github: github.map(Into::into),
            trusted: trusted.to_vec(),
            project: project.map(Into::into),
            home_project_id: home,
            files_of: None,
            mark_ready: false,
            ready_label: default_ready_label(),
            agents_take: default_agents_take(),
        })
    }

    pub fn repositories(&self) -> Result<Vec<Repository>> {
        self.rows(
            &format!("SELECT {REPOSITORY_COLUMNS} FROM repositories ORDER BY id"),
            [],
            repository_from_row,
        )
    }

    /// Replaces the logins whose issues on the repository's forge need no approval.
    pub fn set_repository_trusted(&self, id: i64, trusted: &[String]) -> Result<()> {
        self.conn().execute(
            "UPDATE repositories SET trusted = ?1 WHERE id = ?2",
            params![serde_json::to_string(trusted)?, id],
        )?;
        Ok(())
    }

    /// The repository with `id`, read as one row.
    pub fn repository(&self, id: i64) -> Result<Option<Repository>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {REPOSITORY_COLUMNS} FROM repositories WHERE id = ?1"),
                [id],
                repository_from_row,
            )
            .optional()?)
    }

    /// Removes a repository and everything stored for it: its cards with their changes, runs, decisions, inbox,
    /// moves and dependencies; the open inbox items of its card-less changes, grillings and proposals; its chats and grillings; its lessons, research, automations and localize index. The
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
        let mut statements = card_statements(cards, &runs, &judgements);
        statements.extend([
            format!(
                "DELETE FROM inbox_items WHERE card_id IS NULL AND (\
                 (kind = 'ready_to_merge' AND json_extract(data, '$.change') IN ({changes})) \
                 OR (kind IN ('grilling_issues', 'grilling_report') AND json_extract(data, '$.grilling') IN (SELECT id FROM grillings WHERE repository_id = ?1 OR run_id IN ({runs}))) \
                 OR (kind = 'proposal' AND json_extract(data, '$.proposal') IN ({proposals})))"
            ),
            format!("DELETE FROM trials WHERE proposal_id IN ({proposals})"),
            format!("DELETE FROM research_runs WHERE research_id IN ({research})"),
            "DELETE FROM firings WHERE automation_id IN (SELECT id FROM automations WHERE repository_id = ?1)"
                .to_string(),
            format!("UPDATE proposals SET change_id = NULL WHERE change_id IN ({changes})"),
            format!("DELETE FROM proposals WHERE id IN ({proposals})"),
            format!(
                "DELETE FROM experiments WHERE research_id IN ({research}) OR change_id IN ({changes})"
            ),
            format!("DELETE FROM research WHERE id IN ({research})"),
            format!("DELETE FROM grillings WHERE repository_id = ?1 OR run_id IN ({runs})"),
            "DELETE FROM automations WHERE repository_id = ?1".to_string(),
            "DELETE FROM localize_indexed WHERE repository_id = ?1".to_string(),
            "DELETE FROM lessons WHERE repository_id = ?1".to_string(),
            "DELETE FROM deleted_cards WHERE repository_id = ?1".to_string(),
            "DELETE FROM forge_seen WHERE repository_id = ?1".to_string(),
        ]);
        statements.extend(card_records_last(&runs, &changes, cards));
        // The workflow runs of the repository go with it, with their node rows (which cascade) and the approvals they
        // asked for: `workflow_runs.repository_id` is a reference, so a run cannot outlive its repository. What else
        // pointed at them (a firing, a change of another repository) keeps its row and loses the link. A run that has
        // not ended is for the caller to refuse first (see `active_workflow_runs_of_repository`).
        let workflow_runs = "SELECT id FROM workflow_runs WHERE repository_id = ?1";
        statements.extend([
            format!(
                "DELETE FROM inbox_items WHERE kind = '{}' AND json_extract(data, '$.run') IN ({workflow_runs})",
                super::inbox::WORKFLOW_APPROVAL
            ),
            format!(
                "UPDATE firings SET workflow_run_id = NULL WHERE workflow_run_id IN ({workflow_runs})"
            ),
            format!("UPDATE changes SET run_id = NULL WHERE run_id IN ({workflow_runs})"),
            "DELETE FROM workflow_runs WHERE repository_id = ?1".to_string(),
        ]);
        statements.push("DELETE FROM project_repositories WHERE repository_id = ?1".to_string());
        let home: i64 = tx.query_row(
            "SELECT home_project_id FROM repositories WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?;
        statements.push("DELETE FROM repositories WHERE id = ?1".to_string());
        for statement in &statements {
            tx.execute(statement, [id])?;
        }
        // The home project goes with its last repository, unless cards or another repository are still on it; what
        // the project picked and its members go with it.
        let unused = "NOT EXISTS (SELECT 1 FROM project_repositories WHERE project_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM cards WHERE project_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM repositories WHERE home_project_id = ?1)";
        for table in ["project_kinds", "project_members"] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE project_id = ?1 AND {unused}"),
                [home],
            )?;
        }
        // So do its cycles, which hold no cards any more: those were deleted above.
        tx.execute(
            &format!("DELETE FROM cycle_cards WHERE cycle_id IN (SELECT id FROM cycles WHERE project_id = ?1) AND {unused}"),
            [home],
        )?;
        tx.execute(
            &format!("DELETE FROM cycles WHERE project_id = ?1 AND {unused}"),
            [home],
        )?;
        tx.execute(
            &format!("DELETE FROM imports WHERE project_id = ?1 AND {unused}"),
            [home],
        )?;
        // What a project owns under its id, apart from the finished runs of its workflows, which keep it as a plain
        // value, goes with it: the minutes its schedules fired at and its automations with their firings. The id is
        // kept in `deleted_projects`, so no later project takes it over.
        for statement in [
            "DELETE FROM workflow_schedules WHERE project_id = ?1",
            "DELETE FROM firings WHERE automation_id IN (SELECT id FROM automations WHERE project_id = ?1)",
            "DELETE FROM automations WHERE project_id = ?1",
            "INSERT OR IGNORE INTO deleted_projects (id) SELECT id FROM projects WHERE id = ?1",
        ] {
            tx.execute(&format!("{statement} AND {unused}"), [home])?;
        }
        tx.execute(
            &format!("DELETE FROM projects WHERE id = ?1 AND {unused}"),
            [home],
        )?;
        tx.execute(
            &format!("DROP TABLE IF EXISTS {}", super::localize::table(id)),
            [],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Removes one card and everything stored for it, as `delete_repository` does for each of a repository's cards:
    /// its runs with their events, usage and replays, its changes, inbox items, dependencies, moves, attempts, trials
    /// and judgements, and the uses of lessons for it. The worktrees and branches on disk belong to the caller. With
    /// `remember` the card's number is kept as deleted, so that the poll does not make a card for its issue again.
    /// Says whether such a card was stored.
    pub fn delete_card(&self, id: i64, remember: bool) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let Some((repository_id, number)) = tx
            .query_row(
                "SELECT repository_id, number FROM cards WHERE id = ?1",
                [id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?
        else {
            return Ok(false);
        };
        let cards = "SELECT id FROM cards WHERE id = ?1";
        let runs = "SELECT id FROM runs WHERE card_id = ?1";
        let changes = "SELECT id FROM changes WHERE card_id = ?1";
        let judgements =
            format!("SELECT id FROM judgements WHERE run_id IN ({runs}) OR card_id = ?1");
        let mut statements = card_statements(cards, runs, &judgements);
        // A proposal of the Retro or an experiment may name the card's change; the change goes, they stay.
        statements.extend([
            format!("UPDATE proposals SET change_id = NULL WHERE change_id IN ({changes})"),
            format!("UPDATE experiments SET change_id = NULL WHERE change_id IN ({changes})"),
        ]);
        statements.extend(card_records_last(runs, changes, cards));
        for statement in &statements {
            tx.execute(statement, [id])?;
        }
        // What the forge was seen to say of the issue goes with the card, except whether the issue is closed: an issue that
        // blocks another is done once its forge says so, card or not.
        tx.execute(
            "DELETE FROM forge_seen WHERE repository_id = ?1 AND number = ?2 AND field <> 'state'",
            params![repository_id, number],
        )?;
        if remember {
            tx.execute(
                "INSERT OR REPLACE INTO deleted_cards (repository_id, number, at) VALUES (?1, ?2, ?3)",
                params![repository_id, number, now()],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    /// Whether the card of issue `number` was deleted, so that the poll must not make it again.
    pub fn card_was_deleted(&self, repository_id: i64, number: i64) -> Result<bool> {
        Ok(self
            .conn()
            .query_row(
                "SELECT 1 FROM deleted_cards WHERE repository_id = ?1 AND number = ?2",
                [repository_id, number],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
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
        // A new card lands on its repository's home project.
        let project_id: i64 = conn.query_row(
            "SELECT home_project_id FROM repositories WHERE id = ?1",
            [repository_id],
            |r| r.get(0),
        )?;
        // The number in the project is the issue's number when the project has no card with it, else the next free one.
        let project_number: i64 = conn.query_row(
            "SELECT CASE WHEN EXISTS (SELECT 1 FROM cards WHERE project_id = ?1 AND project_number = ?2)
                         THEN (SELECT MAX(project_number) + 1 FROM cards WHERE project_id = ?1) ELSE ?2 END",
            params![project_id, number],
            |r| r.get(0),
        )?;
        let project_key: String = conn.query_row(
            "SELECT key FROM projects WHERE id = ?1",
            [project_id],
            |r| r.get(0),
        )?;
        let repository_name: String = conn.query_row(
            "SELECT name FROM repositories WHERE id = ?1",
            [repository_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO cards (repository_id, number, title, body, author, column_name, created_at, project_id, project_number, moved_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?7)",
            params![repository_id, number, title, body, author, column, at, project_id, project_number],
        )?;
        Ok(Card {
            id: conn.last_insert_rowid(),
            repository_id,
            project_id,
            number,
            project_number,
            project_key,
            title: title.into(),
            body: body.into(),
            author: author.into(),
            column: column.into(),
            created_at: at,
            note: None,
            retried_after: None,
            confirmed: false,
            held: None,
            parent: None,
            closed: false,
            labels: Vec::new(),
            claimed_by: None,
            assignees: Vec::new(),
            priority: Default::default(),
            due: None,
            estimate: None,
            moved_at: at,
            archived_at: None,
            kind: None,
            fields: Default::default(),
            repositories: vec![repository_name],
            taken_by: None,
            assign_problem: None,
        })
    }

    pub(super) fn cards_where(
        &self,
        clause: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<Card>> {
        self.rows(&format!(
            "SELECT id, repository_id, number, title, body, author, column_name, created_at, note, retried_after, confirmed, held_by, held_reason, held_at, parent, (closed OR column_name = 'Done'), claimed_by, (SELECT json_group_array(label) FROM (SELECT label FROM card_labels WHERE card_id = cards.id ORDER BY label)), {}, project_id, project_number, COALESCE((SELECT key FROM projects WHERE id = cards.project_id), ''), moved_at, archived_at, {}, {}, {}, {}, assign_problem FROM cards WHERE {clause} ORDER BY id", super::assignees::CARD_ASSIGNEES, super::kinds::CARD_KIND, super::kinds::CARD_FIELDS, super::card_repositories::CARD_REPOSITORIES, super::taking::CARD_TAKEN_BY
        ), params, |r| {
            let labels: Vec<String> = serde_json::from_str(&r.get::<_, String>(17)?).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    17,
                    rusqlite::types::Type::Text,
                    e.into(),
                )
            })?;
            let kind_and_fields = super::kinds::card_kind(r, 24, 25)?;
            Ok(Card {
                priority: super::planning::priority_of(&labels),
                due: super::planning::due_of(&labels),
                estimate: super::planning::estimate_of(&labels),
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
                held: r.get::<_, Option<String>>(11)?.map(|by| {
                    Ok::<_, rusqlite::Error>(Held {
                        by,
                        reason: r.get(12)?,
                        at: r.get::<_, Option<i64>>(13)?.unwrap_or(0),
                    })
                }).transpose()?,
                parent: r.get(14)?,
                closed: r.get(15)?,
                claimed_by: r.get(16)?,
                labels,
                assignees: super::assignees::handles(r, 18)?,
                kind: kind_and_fields.0,
                fields: kind_and_fields.1,
                project_id: r.get(19)?,
                project_number: r.get(20)?,
                project_key: r.get(21)?,
                moved_at: r.get(22)?,
                archived_at: r.get(23)?,
                repositories: super::card_repositories::names(r, 26)?,
                taken_by: r.get(28)?,
                assign_problem: r.get(29)?,
            })
        })
    }

    pub fn card(&self, id: i64) -> Result<Option<Card>> {
        Ok(self.cards_where("id = ?1", [id])?.pop())
    }

    /// Every card of every project, oldest first.
    pub fn all_cards(&self) -> Result<Vec<Card>> {
        self.cards_where("1 = 1", [])
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

    /// The card named `key` and `project_number` (`WEB-24` is key `WEB`, number 24), whatever its repository.
    pub fn card_by_key(&self, key: &str, project_number: i64) -> Result<Option<Card>> {
        Ok(self
            .cards_where(
                "project_id = (SELECT id FROM projects WHERE key = ?1) AND project_number = ?2",
                params![key, project_number],
            )?
            .pop())
    }

    pub fn set_card_text(&self, id: i64, title: &str, body: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET title = ?1, body = ?2, moved_at = ?3 WHERE id = ?4",
            params![title, body, now(), id],
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
        self.add_card_awaiting(
            repository_id,
            number,
            title,
            body,
            author,
            column,
            true,
            blocked_by,
            None,
        )
    }

    /// Adds a card that the board leaves alone until [`Store::release_card`]: one whose parent, labels and blockers
    /// are still being set, so that no script starts it without them. `confirmed` says whether it needs no approval;
    /// `by` is who holds it.
    #[allow(clippy::too_many_arguments)]
    pub fn add_held_card(
        &self,
        repository_id: i64,
        number: Option<i64>,
        title: &str,
        body: &str,
        author: &str,
        column: &str,
        confirmed: bool,
        by: &str,
    ) -> Result<Card> {
        self.add_card_awaiting(
            repository_id,
            number,
            title,
            body,
            author,
            column,
            confirmed,
            &BTreeSet::new(),
            Some(by),
        )
    }

    /// Stores a card, its blockers, whether it needs approval, and its hold, in one step, so that the board never sees
    /// it without any of them.
    #[allow(clippy::too_many_arguments)]
    fn add_card_awaiting(
        &self,
        repository_id: i64,
        number: Option<i64>,
        title: &str,
        body: &str,
        author: &str,
        column: &str,
        confirmed: bool,
        blocked_by: &BTreeSet<i64>,
        held_by: Option<&str>,
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
        if confirmed {
            tx.execute("UPDATE cards SET confirmed = 1 WHERE id = ?1", [id])?;
        }
        if let Some(by) = held_by {
            tx.execute(
                "UPDATE cards SET held_by = ?1, held_reason = ?2, held_at = ?3 WHERE id = ?4",
                params![
                    by,
                    "its parent, labels and blockers are still being set",
                    now(),
                    id
                ],
            )?;
        }
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

    /// The numbers each card is blocked by, by card id, in ascending order; a card with none is missing.
    pub fn all_dependencies(&self) -> Result<std::collections::HashMap<i64, Vec<i64>>> {
        let mut by_card: std::collections::HashMap<i64, Vec<i64>> = Default::default();
        for (card, blocker) in self.rows(
            "SELECT card_id, blocked_by FROM dependencies ORDER BY card_id, blocked_by",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )? {
            by_card.entry(card).or_default().push(blocker);
        }
        Ok(by_card)
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
            params![crate::blobs::redact(note), id],
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

    /// Holds a card for `by` (`person`, or `agent:<name>`), with a `reason` if there is one; the board then leaves it
    /// alone. Says whether it did: a card that is held already is left as it is.
    pub fn hold_card(&self, id: i64, by: &str, reason: Option<&str>) -> Result<bool> {
        let reason = reason.map(crate::blobs::redact);
        let held = self.conn().execute(
            "UPDATE cards SET held_by = ?1, held_reason = ?2, held_at = ?3 WHERE id = ?4 AND held_by IS NULL",
            params![by, reason, now(), id],
        )?;
        Ok(held > 0)
    }

    /// Releases a held card. Says whether it was held.
    pub fn release_card(&self, id: i64) -> Result<bool> {
        let released = self.conn().execute(
            "UPDATE cards SET held_by = NULL, held_reason = NULL, held_at = NULL, moved_at = ?2 WHERE id = ?1 AND held_by IS NOT NULL",
            params![id, now()],
        )?;
        Ok(released > 0)
    }

    /// Archives a card: it leaves the board but stays in the store. Says whether it did: an archived card is left as
    /// it is.
    pub fn archive_card(&self, id: i64) -> Result<bool> {
        let archived = self.conn().execute(
            "UPDATE cards SET archived_at = ?1 WHERE id = ?2 AND archived_at IS NULL",
            params![now(), id],
        )?;
        Ok(archived > 0)
    }

    /// Sets when the card last moved, for tests that make up a history.
    pub fn set_card_moved_at(&self, id: i64, at: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET moved_at = ?1 WHERE id = ?2",
            params![at, id],
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
            "UPDATE cards SET column_name = ?1, moved_at = ?3, archived_at = NULL WHERE id = ?2",
            params![column, id, now()],
        )?;
        Ok(())
    }

    /// Moves a card from `from` to `to` only if it is still in `from`, setting its note to `note` when one is given, and
    /// records the move. Returns whether the card moved: it did not when something else moved it first.
    pub fn move_card_from(
        &self,
        id: i64,
        from: &str,
        to: &str,
        note: Option<&str>,
    ) -> Result<bool> {
        let note = note.map(crate::blobs::redact);
        let conn = self.conn();
        let recorded = conn.execute(
            "INSERT INTO card_moves (card_id, from_column, to_column, note, at)
             SELECT id, column_name, ?1, COALESCE(?2, note), ?3 FROM cards WHERE id = ?4 AND column_name = ?5",
            params![to, note, now(), id, from],
        )?;
        if recorded == 0 {
            return Ok(false);
        }
        conn.execute(
            "UPDATE cards SET column_name = ?1, note = COALESCE(?2, note), moved_at = ?4, archived_at = NULL WHERE id = ?3",
            params![to, note, id, now()],
        )?;
        Ok(true)
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
    fn deleting_a_repository_takes_its_finished_workflow_runs_with_their_nodes_and_approvals() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let other = s
            .add_repository("other", "/srv/other", "none", "main")
            .unwrap();
        let value = serde_json::json!({});
        let run = s
            .add_workflow_run("release", repo.id, "manual", &value)
            .unwrap();
        s.add_workflow_node_run(run.id, "ask", 1, &value).unwrap();
        s.add_inbox_item(
            crate::store::WORKFLOW_APPROVAL,
            None,
            "Ship?",
            &serde_json::json!({"run": run.id}),
        )
        .unwrap();
        s.finish_workflow_run(run.id, "succeeded", None).unwrap();
        let kept = s
            .add_workflow_run("release", other.id, "manual", &value)
            .unwrap();
        assert!(
            s.active_workflow_runs_of_repository(repo.id)
                .unwrap()
                .is_empty()
        );

        assert!(s.delete_repository(repo.id).unwrap());

        assert!(s.workflow_run(run.id).unwrap().is_none());
        assert!(s.workflow_node_runs(run.id).unwrap().is_empty());
        assert_eq!(s.inbox_for_workflow_run(run.id).unwrap(), None);
        // Another repository's runs are left alone, and its active ones are listed.
        assert!(s.workflow_run(kept.id).unwrap().is_some());
        assert_eq!(
            s.active_workflow_runs_of_repository(other.id).unwrap(),
            vec![kept.id]
        );
    }

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

    /// The number of rows in `table`.
    fn rows_in(s: &Store, table: &str) -> i64 {
        s.conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    /// Everything a card can have stored for it, and for a second card whose records must stay.
    fn card_with_records(s: &Store, repo: i64, number: i64) -> (Card, i64) {
        use crate::store::{NewJudgement, NewLesson, Usage};
        let card = s
            .add_numbered_card(repo, number, "Work", "body", "leon", "In progress")
            .unwrap();
        s.set_dependencies(card.id, &BTreeSet::from([9])).unwrap();
        // Any link will do for the table; a card linked to itself keeps the two cards' records apart.
        s.add_link(card.id, card.id, crate::store::RELATED).unwrap();
        s.move_card(card.id, "In review").unwrap();
        let run = s
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        s.append_event(run.id, "agent_start", &serde_json::json!({}))
            .unwrap();
        s.add_usage(&Usage {
            run_id: Some(run.id),
            model: "m".into(),
            input_tokens: 1,
            output_tokens: 1,
            cached_tokens: 0,
            duration_ms: 1,
            tools: vec![],
        })
        .unwrap();
        s.add_attempt(run.id, card.id, 1, None).unwrap();
        s.add_change(
            repo,
            Some(card.id),
            &format!("moebius/card-{number}"),
            None,
            "abc",
        )
        .unwrap();
        s.add_inbox_item("stuck", Some(card.id), "Work", &serde_json::json!({}))
            .unwrap();
        for (run_id, card_id) in [(Some(run.id), Some(card.id)), (None, Some(card.id))] {
            s.add_judgement(&NewJudgement {
                kind: "tool_call".into(),
                choice: "allow".into(),
                decider: "person".into(),
                run_id,
                card_id,
                ..NewJudgement::default()
            })
            .unwrap();
        }
        let lesson = s
            .add_lesson(&NewLesson {
                repository_id: repo,
                kind: "pitfall".into(),
                situation: format!("situation {number}"),
                lesson: "lesson".into(),
                advice: "advice".into(),
                cards: vec![],
                retro_date: None,
            })
            .unwrap()
            .unwrap();
        s.note_lessons_used(&[lesson.id], Some(card.id)).unwrap();
        s.add_retro("2026-01-01", &serde_json::json!({}), "reported")
            .ok();
        s.add_proposal("2026-01-01", "moebius/proposal", "Try", "", &[], &[card.id])
            .unwrap();
        let proposal = s.proposals(None).unwrap().pop().unwrap().id;
        s.add_trial(
            proposal,
            Some(card.id),
            Some(run.id),
            "positive",
            "",
            &serde_json::json!({}),
        )
        .unwrap();
        (card, run.id)
    }

    const CARD_TABLES: [&str; 13] = [
        "cards",
        "runs",
        "events",
        "usage",
        "attempts",
        "changes",
        "inbox_items",
        "judgements",
        "lesson_uses",
        "dependencies",
        "card_links",
        "card_moves",
        "trials",
    ];

    #[test]
    fn deleting_a_card_takes_all_it_has_and_nothing_of_another_card() {
        let (_dir, s) = store();
        let repo = s
            .add_github_repository("widgets", "/srv/widgets", "main", "acme/widgets", &[])
            .unwrap();
        let (doomed, doomed_run) = card_with_records(&s, repo.id, 1);
        let alone: Vec<i64> = CARD_TABLES.iter().map(|t| rows_in(&s, t)).collect();
        assert!(alone.iter().all(|n| *n > 0), "{alone:?}");
        let (kept, _) = card_with_records(&s, repo.id, 2);

        assert!(s.delete_card(doomed.id, true).unwrap());
        assert!(!s.delete_card(doomed.id, true).unwrap());
        assert_eq!(s.card(doomed.id).unwrap(), None);
        assert_eq!(s.run(doomed_run).unwrap(), None);
        // Only the second card's records are left: as many rows as the first card alone had.
        let left: Vec<i64> = CARD_TABLES.iter().map(|t| rows_in(&s, t)).collect();
        assert_eq!(left, alone);
        assert_eq!(
            s.cards(repo.id)
                .unwrap()
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            [kept.id]
        );
        assert_eq!(s.runs_for_card(kept.id).unwrap().len(), 1);
        assert_eq!(s.dependencies(kept.id).unwrap(), [9]);
        // Its number is remembered; the other card's is not.
        assert!(s.card_was_deleted(repo.id, 1).unwrap());
        assert!(!s.card_was_deleted(repo.id, 2).unwrap());
    }

    #[test]
    fn a_card_deleted_without_remembering_its_number_may_come_again() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/tmp/demo", "none", "main")
            .unwrap();
        let card = s.add_card(repo.id, "Work", "", "leon", "Backlog").unwrap();
        assert!(s.delete_card(card.id, false).unwrap());
        assert!(!s.card_was_deleted(repo.id, card.number).unwrap());
        // Removing the repository takes the remembered numbers with it.
        let forge = s
            .add_github_repository("widgets", "/srv/widgets", "main", "acme/widgets", &[])
            .unwrap();
        let other = s
            .add_numbered_card(forge.id, 5, "Work", "", "leon", "Backlog")
            .unwrap();
        assert!(s.delete_card(other.id, true).unwrap());
        assert!(s.card_was_deleted(forge.id, 5).unwrap());
        assert!(s.delete_repository(forge.id).unwrap());
        assert_eq!(rows_in(&s, "deleted_cards"), 0);
    }

    #[test]
    fn a_hold_is_set_once_released_once_and_survives_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, card) = {
            let s = Store::open(dir.path()).unwrap();
            let repo = s
                .add_repository("demo", "/tmp/demo", "none", "main")
                .unwrap();
            let card = s.add_card(repo.id, "Work", "", "leon", "Backlog").unwrap();
            assert_eq!(card.held, None);
            assert!(
                s.hold_card(card.id, "person", Some("wait for the design"))
                    .unwrap()
            );
            // Held already: the first hold stays.
            assert!(!s.hold_card(card.id, "agent:Moebius", None).unwrap());
            (repo, card)
        };
        let s = Store::open(dir.path()).unwrap();
        let held = s.card(card.id).unwrap().unwrap().held.unwrap();
        assert_eq!(
            (held.by.as_str(), held.reason.as_deref()),
            ("person", Some("wait for the design"))
        );
        assert!(held.at > 0);
        assert_eq!(s.cards(repo.id).unwrap()[0].held, Some(held));
        assert!(s.release_card(card.id).unwrap());
        assert!(!s.release_card(card.id).unwrap());
        assert_eq!(s.card(card.id).unwrap().unwrap().held, None);
    }
}
