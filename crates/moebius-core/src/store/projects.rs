//! The projects (ADR 0019): each with a name, a key such as `WEB` that names its cards (`WEB-24`), one board and the
//! repositories it works in, in the order it names them. A repository belongs to one or more projects and has one home
//! project, where its new issues land; every card is on the board of exactly one project (`cards.project_id`).
//!
//! The migration that adds the tables makes one project per repository, with the repository's name and id, as its
//! home project, and puts each card on it. Repository names were unique only in their case and project names are
//! unique in any case, so a repository whose name an earlier one has in another case gets `<name> (<id>)`, which no
//! repository name can be, since those have no spaces. SQL cannot make a key from a name, so the migration leaves a provisional key,
//! `#<id>`, which no project can choose; [`give_keys`] replaces every such key with one made from the name as soon as
//! the store is open. A repository added later makes its home project the same way (see `cards`).
//!
//! A project also has a store of files of its own: a repository row whose `files_of` names the project. It is not one
//! of the project's repositories (`project_repositories` does not list it), but the project is its home project, so the
//! cards of a project without a repository are on its board and run, are reviewed and are merged in it like any
//! repository's without a forge. `app::projects::files` makes it.
//!
//! A project can be private (ADR 0018, [`PRIVATE`]): it names its members in `project_members`, and nobody else, owners
//! included, sees its board, cards, runs or files. Who sees what is decided in `app::projects::sight`.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Repository, Result, Store, StoreError, now};

/// The tables of this part, one project for each repository, and every card on its repository's project.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    key TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL
);
CREATE TABLE project_repositories (
    project_id INTEGER NOT NULL REFERENCES projects(id),
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    position INTEGER NOT NULL,
    PRIMARY KEY (project_id, repository_id)
);
CREATE INDEX project_repositories_by_repository ON project_repositories (repository_id);
ALTER TABLE repositories ADD COLUMN home_project_id INTEGER REFERENCES projects(id);
ALTER TABLE cards ADD COLUMN project_id INTEGER REFERENCES projects(id);
CREATE INDEX cards_by_project ON cards (project_id);
INSERT INTO projects (id, name, key, created_at)
    SELECT id,
           CASE WHEN EXISTS (SELECT 1 FROM repositories AS earlier
                             WHERE earlier.name = repositories.name COLLATE NOCASE AND earlier.id < repositories.id)
                THEN name || ' (' || id || ')' ELSE name END,
           '#' || id, CAST(strftime('%s', 'now') AS INTEGER)
    FROM repositories;
INSERT INTO project_repositories (project_id, repository_id, position) SELECT id, id, 0 FROM repositories;
UPDATE repositories SET home_project_id = id;
UPDATE cards SET project_id = repository_id;
"#;

/// The repository that holds a project's own files names the project; a project has at most one.
pub(super) const FILES: &str = r#"
ALTER TABLE repositories ADD COLUMN files_of INTEGER REFERENCES projects(id);
CREATE UNIQUE INDEX repositories_by_files_of ON repositories (files_of) WHERE files_of IS NOT NULL;
"#;

/// A card's number in its project, which names it with the project's key (`WEB-24`). A card keeps its number; of the
/// cards of a project that share one (a project with several repositories), the earliest keeps it and each later one
/// takes the next numbers after the project's highest, in the order of their ids.
pub(super) const NUMBERS_SCHEMA: &str = r#"
ALTER TABLE cards ADD COLUMN project_number INTEGER NOT NULL DEFAULT 0;
UPDATE cards SET project_number = number;
UPDATE cards SET project_number =
    (SELECT MAX(o.number) FROM cards o WHERE o.project_id = cards.project_id)
    + (SELECT COUNT(*) FROM cards d
       WHERE d.project_id = cards.project_id AND d.id <= cards.id
         AND EXISTS (SELECT 1 FROM cards e WHERE e.project_id = d.project_id AND e.number = d.number AND e.id < d.id))
WHERE EXISTS (SELECT 1 FROM cards e WHERE e.project_id = cards.project_id AND e.number = cards.number AND e.id < cards.id);
CREATE UNIQUE INDEX cards_by_key ON cards (project_id, project_number);
"#;

/// A project is private or not, and a private one names its members. Every project is public at first.
pub(super) const PRIVATE: &str = r#"
ALTER TABLE projects ADD COLUMN private INTEGER NOT NULL DEFAULT 0;
CREATE TABLE project_members (
    project_id INTEGER NOT NULL REFERENCES projects(id),
    person_id INTEGER NOT NULL REFERENCES people(id),
    PRIMARY KEY (project_id, person_id)
);
CREATE INDEX project_members_by_person ON project_members (person_id);
"#;

/// The most characters a project's key has.
pub const KEY_LIMIT: usize = 10;

/// A project: a board over any number of repositories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    /// Unique regardless of case; a project can be renamed.
    pub name: String,
    /// An uppercase letter and then 1 to 9 uppercase letters or digits, such as `WEB`; unique, and kept when the project is renamed.
    pub key: String,
    pub created_at: i64,
    /// The ids of the project's repositories, in the order the project names them.
    pub repositories: Vec<i64>,
    /// The id of the repository that holds the project's own files, once it is made; it is not one of `repositories`.
    #[serde(default)]
    pub files: Option<i64>,
    /// Whether the project takes requests (see `intake`): a form anyone who may see it fills in.
    #[serde(default)]
    pub intake: bool,
    /// Whether only its members see it.
    pub private: bool,
    /// The ids of the people a private project names as its members, in the order of their ids; a public project
    /// keeps the list it had, which counts for nothing while it is public.
    pub members: Vec<i64>,
}

/// Whether `key` is a project's key: an uppercase letter, then 1 to 9 uppercase letters or digits.
pub fn is_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && (2..=KEY_LIMIT).contains(&key.len())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// The project key and number a card's name holds: `WEB-24` is `("WEB", 24)`; the key may be in any case. None for
/// anything else, such as `#24`, `web-0x` or a key that is not one.
pub fn parse_card_key(text: &str) -> Option<(String, i64)> {
    let (key, number) = text.split_once('-')?;
    let key = key.to_ascii_uppercase();
    if !is_key(&key) || number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number.parse().ok().map(|n| (key, n))
}

/// A key made from `name` that `taken` does not refuse: the name's letters and digits in capitals, after a `P` when it
/// does not start with a letter, cut to [`KEY_LIMIT`]; a number is put at its end while that is taken or shorter than
/// two characters, so `web` becomes `WEB`, a second `web-` `WEB2`, and `2fa` `P2FA`.
fn key_for(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let letters: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let base = match letters.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => letters,
        _ => format!("P{letters}"),
    };
    let fits = |n: &str| {
        let room = KEY_LIMIT - n.len();
        format!("{}{n}", &base[..base.len().min(room)])
    };
    let whole = fits("");
    if is_key(&whole) && !taken(&whole) {
        return whole;
    }
    (2..)
        .map(|n| fits(&n.to_string()))
        .find(|key| !taken(key))
        .expect("some number makes a key that is free")
}

/// The keys the projects on `conn` have.
fn taken_keys(conn: &Connection) -> rusqlite::Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT key FROM projects")?;
    stmt.query_map([], |r| r.get(0))?.collect()
}

/// Gives every project with a provisional key (`#<id>`, which the migration leaves) a key made from its name, in the
/// order the projects were made.
pub(super) fn give_keys(conn: &Connection) -> rusqlite::Result<()> {
    let unkeyed: Vec<(i64, String)> = {
        let mut stmt =
            conn.prepare("SELECT id, name FROM projects WHERE key LIKE '#%' ORDER BY id")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    if unkeyed.is_empty() {
        return Ok(());
    }
    let mut taken = taken_keys(conn)?;
    for (id, name) in unkeyed {
        let key = key_for(&name, |key| taken.contains(key));
        conn.execute(
            "UPDATE projects SET key = ?1 WHERE id = ?2",
            params![key, id],
        )?;
        taken.insert(key);
    }
    Ok(())
}

/// Makes the home project of a new repository on `conn`: the repository's name and a key made from it. Returns its id.
pub(super) fn insert_home_project(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
    let taken = taken_keys(conn)?;
    let key = key_for(name, |key| taken.contains(key));
    conn.execute(
        "INSERT INTO projects (name, key, created_at) VALUES (?1, ?2, ?3)",
        params![name, key, now()],
    )?;
    Ok(conn.last_insert_rowid())
}

const COLUMNS: &str = "id, name, key, created_at, \
    (SELECT json_group_array(repository_id) FROM \
        (SELECT repository_id FROM project_repositories WHERE project_id = projects.id ORDER BY position, repository_id)), \
    (SELECT id FROM repositories WHERE files_of = projects.id), intake, \
    private, \
    (SELECT json_group_array(person_id) FROM \
        (SELECT person_id FROM project_members WHERE project_id = projects.id ORDER BY person_id))";

/// The JSON list of ids in column `i` of `row`.
fn ids(row: &Row, i: usize) -> rusqlite::Result<Vec<i64>> {
    serde_json::from_str(&row.get::<_, String>(i)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, e.into())
    })
}

fn project(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        key: row.get(2)?,
        created_at: row.get(3)?,
        repositories: ids(row, 4)?,
        files: row.get(5)?,
        intake: row.get(6)?,
        private: row.get(7)?,
        members: ids(row, 8)?,
    })
}

impl Store {
    /// Adds a project with `name` and `key` and no repository. A name taken in any case, or a key taken, is an error;
    /// the caller checks both first and that the key is one (see [`is_key`]).
    pub fn add_project(&self, name: &str, key: &str) -> Result<Project> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO projects (name, key, created_at) VALUES (?1, ?2, ?3)",
                params![name, key, now()],
            )?;
            conn.last_insert_rowid()
        };
        self.project(id)?
            .ok_or(StoreError::Sql(rusqlite::Error::QueryReturnedNoRows))
    }

    /// Adds the repository `name` at `path`, on `default_branch`, that holds the own files of the project
    /// `project_id`: without a forge, with the project as its home project and not one of its repositories. A project
    /// that has its files already, or a name a repository has, is an error.
    pub fn add_project_files(
        &self,
        project_id: i64,
        name: &str,
        path: &str,
        default_branch: &str,
    ) -> Result<Repository> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO repositories (name, path, forge, default_branch, trusted, home_project_id, files_of)
                 VALUES (?1, ?2, 'none', ?3, '[]', ?4, ?4)",
                params![name, path, default_branch, project_id],
            )?;
            conn.last_insert_rowid()
        };
        self.repository(id)?
            .ok_or(StoreError::Sql(rusqlite::Error::QueryReturnedNoRows))
    }

    /// The project with `id`.
    pub fn project(&self, id: i64) -> Result<Option<Project>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM projects WHERE id = ?1"),
                [id],
                project,
            )
            .optional()?)
    }

    /// The project whose name is `name`, in any case.
    pub fn project_by_name(&self, name: &str) -> Result<Option<Project>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM projects WHERE name = ?1"),
                [name],
                project,
            )
            .optional()?)
    }

    /// The project whose key is `key`.
    pub fn project_by_key(&self, key: &str) -> Result<Option<Project>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM projects WHERE key = ?1"),
                [key],
                project,
            )
            .optional()?)
    }

    /// Every project, in the order they were made.
    pub fn projects(&self) -> Result<Vec<Project>> {
        self.rows(
            &format!("SELECT {COLUMNS} FROM projects ORDER BY id"),
            [],
            project,
        )
    }

    /// Renames the project with `id`; its key stays. False when there is none.
    pub fn rename_project(&self, id: i64, name: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE projects SET name = ?1 WHERE id = ?2",
            params![name, id],
        )? == 1)
    }

    /// Makes `repository_id` one of the repositories of the project `project_id`, after the ones it has; nothing
    /// happens when it is one already. The repository's home project does not change.
    pub fn link_repository(&self, project_id: i64, repository_id: i64) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO project_repositories (project_id, repository_id, position)
             VALUES (?1, ?2, COALESCE((SELECT MAX(position) + 1 FROM project_repositories WHERE project_id = ?1), 0))",
            [project_id, repository_id],
        )?;
        Ok(())
    }

    /// Puts the card on the board of the project `project_id`; its column stays. It is named by the new project's key
    /// from then on, with its issue's number when the project has no card with that number, else the next free one.
    pub fn set_card_project(&self, card_id: i64, project_id: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE cards SET project_id = ?1,
                 project_number = (SELECT CASE WHEN EXISTS (SELECT 1 FROM cards o WHERE o.project_id = ?1 AND o.project_number = cards.number)
                                               THEN (SELECT MAX(o.project_number) + 1 FROM cards o WHERE o.project_id = ?1)
                                               ELSE cards.number END)
             WHERE id = ?2 AND project_id IS NOT ?1",
            [project_id, card_id],
        )?;
        // A project offers only the kinds it picked, so a card that moves to one that did not pick its kind loses it,
        // with the values of its fields.
        let unoffered = "kind_id IS NOT NULL AND kind_id NOT IN (SELECT kind_id FROM project_kinds WHERE project_id = ?1)";
        tx.execute(
            &format!("DELETE FROM card_fields WHERE card_id = ?2 AND EXISTS (SELECT 1 FROM cards WHERE id = ?2 AND {unoffered})"),
            [project_id, card_id],
        )?;
        tx.execute(
            &format!("UPDATE cards SET kind_id = NULL WHERE id = ?2 AND {unoffered}"),
            [project_id, card_id],
        )?;
        Ok(tx.commit()?)
    }

    /// Makes the project `id` private or public and names its members: the people with the ids `members`, which the
    /// caller checks are people. False when there is no such project.
    pub fn set_project_access(&self, id: i64, private: bool, members: &[i64]) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if tx.execute(
            "UPDATE projects SET private = ?1 WHERE id = ?2",
            params![private, id],
        )? != 1
        {
            return Ok(false);
        }
        tx.execute("DELETE FROM project_members WHERE project_id = ?1", [id])?;
        for person in members {
            tx.execute(
                "INSERT OR IGNORE INTO project_members (project_id, person_id) VALUES (?1, ?2)",
                params![id, person],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    /// The ids of the private projects that the person `person` is no member of: every private project when there is
    /// no person.
    pub fn hidden_projects(&self, person: Option<i64>) -> Result<HashSet<i64>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id FROM projects WHERE private AND NOT EXISTS
                 (SELECT 1 FROM project_members WHERE project_id = projects.id AND person_id = ?1)",
        )?;
        Ok(stmt
            .query_map([person], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// The project a judgement on the card `card`, made in the run `run`, belongs to: the card's, else that of the
    /// run's card, else the home project of the run's repository; none for a judgement about no project's work.
    pub fn project_of(&self, card: Option<i64>, run: Option<i64>) -> Result<Option<i64>> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(
                 (SELECT project_id FROM cards WHERE id = ?1),
                 (SELECT cards.project_id FROM runs JOIN cards ON cards.id = runs.card_id WHERE runs.id = ?2),
                 (SELECT repositories.home_project_id FROM runs
                      JOIN repositories ON repositories.id = runs.repository_id WHERE runs.id = ?2))",
            params![card, run],
            |r| r.get(0),
        )?)
    }

    /// How many cards on the project's board are open: not closed and not in `Done`.
    pub fn open_cards(&self, project_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM cards WHERE project_id = ?1 AND NOT closed AND column_name <> 'Done'",
            [project_id],
            |r| r.get(0),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MIGRATIONS, Role, test_store};

    #[test]
    fn a_key_is_made_from_the_name_and_never_taken_twice() {
        let none = |_: &str| false;
        assert_eq!(key_for("web", none), "WEB");
        assert_eq!(key_for("web-app.v2", none), "WEBAPPV2");
        assert_eq!(key_for("2fa", none), "P2FA");
        assert_eq!(key_for("a", none), "A2");
        assert_eq!(key_for("", none), "P2");
        assert_eq!(key_for("über", none), "BER");
        assert_eq!(key_for("averyveryverylongname", none), "AVERYVERYV");
        assert_eq!(key_for("web", |k| k == "WEB"), "WEB2");
        assert_eq!(key_for("web", |k| ["WEB", "WEB2"].contains(&k)), "WEB3");
        assert_eq!(
            key_for("averyveryverylongname", |k| k == "AVERYVERYV"),
            "AVERYVERY2"
        );
        for name in ["web", "a", "", "2fa", "averyveryverylongname", "--"] {
            assert!(is_key(&key_for(name, none)), "{name}");
        }
        assert!(!is_key("web") && !is_key("W") && !is_key("2FA") && !is_key("WEB-1"));
        assert!(!is_key("ABCDEFGHIJK") && !is_key("#1"));
    }

    /// A store from before projects: every repository becomes a project with its name, as its home project, every
    /// card is on its repository's project, and each project has a key made from its name that no other has.
    #[test]
    fn a_store_from_before_puts_every_card_on_its_repository_s_project() {
        let dir = tempfile::tempdir().unwrap();
        {
            let conn = rusqlite::Connection::open(dir.path().join("moebius.sqlite")).unwrap();
            let projects = MIGRATIONS
                .iter()
                .position(|sql| *sql == SCHEMA)
                .expect("a migration adds projects");
            for sql in &MIGRATIONS[..projects] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", projects as i64)
                .unwrap();
            conn.execute_batch(
                "INSERT INTO repositories (id, name, path, forge, default_branch) VALUES (1, 'web', '/srv/web', 'none', 'main');
                 INSERT INTO repositories (id, name, path, forge, default_branch) VALUES (2, 'api', '/srv/api', 'github', 'main');
                 INSERT INTO repositories (id, name, path, forge, default_branch) VALUES (3, 'web-', '/srv/web-', 'none', 'main');
                 INSERT INTO repositories (id, name, path, forge, default_branch) VALUES (4, 'Web', '/srv/Web', 'none', 'main');
                 INSERT INTO cards (id, repository_id, number, title, body, author, column_name, created_at)
                     VALUES (1, 1, 1, 'Front page', '', 'person:owner', 'Backlog', 1);
                 INSERT INTO cards (id, repository_id, number, title, body, author, column_name, created_at)
                     VALUES (2, 2, 7, 'An endpoint', '', 'lea', 'Done', 1);
                 INSERT INTO cards (id, repository_id, number, title, body, author, column_name, created_at)
                     VALUES (3, 2, 8, 'Another', '', 'lea', 'In progress', 1);
                 INSERT INTO cards (id, repository_id, number, title, body, author, column_name, created_at)
                     VALUES (4, 3, 1, 'Copy', '', 'lea', 'Backlog', 1);",
            )
            .unwrap();
        }
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len());
        let projects: Vec<(i64, String, String, Vec<i64>)> = store
            .projects()
            .unwrap()
            .into_iter()
            .map(|p| (p.id, p.name, p.key, p.repositories))
            .collect();
        assert_eq!(
            projects,
            [
                (1, "web".into(), "WEB".into(), vec![1]),
                (2, "api".into(), "API".into(), vec![2]),
                (3, "web-".into(), "WEB2".into(), vec![3]),
                (4, "Web (4)".into(), "WEB4".into(), vec![4]),
            ]
        );
        for repository in store.repositories().unwrap() {
            assert_eq!(repository.home_project_id, repository.id);
            for card in store.cards(repository.id).unwrap() {
                assert_eq!(
                    card.project_id, repository.home_project_id,
                    "{}",
                    card.title
                );
                // Each card keeps its number, which now names it with its project's key.
                assert_eq!(card.project_number, card.number, "{}", card.title);
                assert_eq!(
                    card.project_key,
                    store.project(repository.id).unwrap().unwrap().key
                );
            }
        }
        assert_eq!(
            store.open_cards(2).unwrap(),
            1,
            "the card in Done is not open"
        );

        // Opening it again changes no key.
        drop(store);
        let again = Store::open(dir.path()).unwrap();
        assert_eq!(again.project(3).unwrap().unwrap().key, "WEB2");
    }

    #[test]
    fn a_new_repository_makes_its_home_project_and_its_cards_land_there() {
        let (_dir, s) = test_store();
        let web = s.add_repository("web", "/srv/web", "none", "main").unwrap();
        let home = s.project(web.home_project_id).unwrap().unwrap();
        assert_eq!(
            (home.name.as_str(), home.key.as_str(), home.repositories),
            ("web", "WEB", vec![web.id])
        );
        let card = s.add_card(web.id, "First", "", "lea", "Backlog").unwrap();
        assert_eq!(card.project_id, home.id);
        assert_eq!(s.card(card.id).unwrap().unwrap().project_id, home.id);
        assert_eq!(s.open_cards(home.id).unwrap(), 1);

        // A project made by hand takes a key; the next repository's home project gets one that is free.
        let other = s.add_project("Storefront", "WEBB").unwrap();
        assert!(other.repositories.is_empty());
        assert!(s.add_project("storefront", "SHOP").is_err(), "a taken name");
        assert!(s.add_project("Shop", "WEB").is_err(), "a taken key");
        let webb = s
            .add_repository("webb", "/srv/webb", "none", "main")
            .unwrap();
        assert_eq!(
            s.project(webb.home_project_id).unwrap().unwrap().key,
            "WEBB2"
        );

        // Renaming keeps the key.
        assert!(s.rename_project(other.id, "Shop").unwrap());
        let renamed = s.project(other.id).unwrap().unwrap();
        assert_eq!(
            (renamed.name.as_str(), renamed.key.as_str()),
            ("Shop", "WEBB")
        );
        assert_eq!(s.project_by_name("SHOP").unwrap().unwrap().id, other.id);
        assert_eq!(s.project_by_key("WEBB").unwrap().unwrap().id, other.id);
        assert!(!s.rename_project(999, "Gone").unwrap());
    }

    #[test]
    fn a_private_project_hides_from_everyone_but_its_members() {
        let (_dir, s) = test_store();
        let lea = s.add_person("lea", "Lea", Role::Member).unwrap();
        let ben = s.add_person("ben", "Ben", Role::Owner).unwrap();
        let shop = s.add_project("Storefront", "SHOP").unwrap();
        assert!((!shop.private && shop.members.is_empty()));
        assert!(s.hidden_projects(Some(lea.id)).unwrap().is_empty());

        assert!(
            s.set_project_access(shop.id, true, &[lea.id, lea.id])
                .unwrap()
        );
        let private = s.project(shop.id).unwrap().unwrap();
        assert!(private.private);
        assert_eq!(private.members, [lea.id]);
        assert!(s.hidden_projects(Some(lea.id)).unwrap().is_empty());
        assert_eq!(
            s.hidden_projects(Some(ben.id)).unwrap(),
            HashSet::from([shop.id])
        );
        assert_eq!(s.hidden_projects(None).unwrap(), HashSet::from([shop.id]));

        assert!(s.set_project_access(shop.id, false, &[lea.id]).unwrap());
        assert!(s.hidden_projects(Some(ben.id)).unwrap().is_empty());
        assert!(!s.set_project_access(999, true, &[]).unwrap());
    }

    #[test]
    fn removing_a_repository_takes_its_home_project_when_nothing_is_left_on_it() {
        let (_dir, s) = test_store();
        let web = s.add_repository("web", "/srv/web", "none", "main").unwrap();
        s.add_card(web.id, "First", "", "lea", "Backlog").unwrap();
        let kept = s.add_project("Storefront", "SHOP").unwrap();
        let lea = s.add_person("lea", "Lea", Role::Member).unwrap();
        assert!(
            s.set_project_access(web.home_project_id, true, &[lea.id])
                .unwrap()
        );
        assert!(s.delete_repository(web.id).unwrap());
        assert!(s.project(web.home_project_id).unwrap().is_none());
        assert!(s.project(kept.id).unwrap().is_some());
        // The key is free again.
        let again = s.add_repository("web", "/srv/web", "none", "main").unwrap();
        assert_eq!(
            s.project(again.home_project_id).unwrap().unwrap().key,
            "WEB"
        );
    }

    #[test]
    fn parsing_a_card_key_takes_a_key_and_digits_in_any_case() {
        assert_eq!(parse_card_key("WEB-24"), Some(("WEB".into(), 24)));
        assert_eq!(parse_card_key("web-1"), Some(("WEB".into(), 1)));
        for not in [
            "#24",
            "24",
            "WEB",
            "WEB-",
            "WEB-x",
            "W-1",
            "WEB--1",
            "WEB-1-2",
            "1WEB-1",
            "WEB-+1",
            "TOOLONGKEY1-1",
        ] {
            assert_eq!(parse_card_key(not), None, "{not}");
        }
    }

    #[test]
    fn a_card_is_named_by_its_project_key_and_a_number_unique_in_the_project() {
        let (_dir, s) = test_store();
        let web = s.add_repository("web", "/srv/web", "none", "main").unwrap();
        let api = s.add_repository("api", "/srv/api", "none", "main").unwrap();
        // The number of a card is the number of its issue while that is free in its project.
        let seven = s
            .add_numbered_card(web.id, 7, "Seven", "", "lea", "Backlog")
            .unwrap();
        assert_eq!(
            (seven.project_key.as_str(), seven.project_number),
            ("WEB", 7)
        );
        // Another repository of the same project may have an issue with the same number: the card takes the next free.
        s.conn()
            .execute(
                "UPDATE repositories SET home_project_id = ?1 WHERE id = ?2",
                [web.home_project_id, api.id],
            )
            .unwrap();
        let clash = s
            .add_numbered_card(api.id, 7, "Clash", "", "lea", "Backlog")
            .unwrap();
        assert_eq!((clash.number, clash.project_number), (7, 8));
        let local = s.add_card(api.id, "Local", "", "lea", "Backlog").unwrap();
        assert_eq!(local.project_number, 9);
        // Keys find them whatever the repository, and numbers of other projects do not count.
        assert_eq!(s.card_by_key("WEB", 8).unwrap().unwrap().id, clash.id);
        assert_eq!(s.card_by_key("WEB", 7).unwrap().unwrap().id, seven.id);
        assert_eq!(s.card_by_key("WEB", 99).unwrap(), None);
        assert_eq!(s.card_by_key("NOPE", 7).unwrap(), None);
        assert_eq!(s.card(clash.id).unwrap().unwrap().project_key, "WEB");
        // A project's rename keeps the key, so the name of its cards.
        assert!(s.rename_project(web.home_project_id, "Storefront").unwrap());
        assert_eq!(s.card_by_key("WEB", 7).unwrap().unwrap().id, seven.id);
    }

    #[test]
    fn a_card_moved_to_a_project_is_renumbered_when_the_number_is_taken_there() {
        let (_dir, s) = test_store();
        let web = s.add_repository("web", "/srv/web", "none", "main").unwrap();
        let api = s.add_repository("api", "/srv/api", "none", "main").unwrap();
        let taken = s
            .add_numbered_card(web.id, 7, "Web", "", "lea", "Backlog")
            .unwrap();
        let moved = s
            .add_numbered_card(api.id, 7, "Api", "", "lea", "Backlog")
            .unwrap();
        assert_eq!(
            (moved.project_key.as_str(), moved.project_number),
            ("API", 7)
        );
        s.set_card_project(moved.id, web.home_project_id).unwrap();
        let moved = s.card(moved.id).unwrap().unwrap();
        assert_eq!(
            (moved.project_key.as_str(), moved.project_number),
            ("WEB", 8)
        );
        assert_eq!(s.card_by_key("WEB", 7).unwrap().unwrap().id, taken.id);
        // A move that changes nothing changes nothing; a free number is kept.
        s.set_card_project(moved.id, web.home_project_id).unwrap();
        assert_eq!(s.card(moved.id).unwrap().unwrap().project_number, 8);
        let free = s
            .add_numbered_card(api.id, 30, "Free", "", "lea", "Backlog")
            .unwrap();
        s.set_card_project(free.id, web.home_project_id).unwrap();
        assert_eq!(s.card(free.id).unwrap().unwrap().project_number, 30);
    }

    #[test]
    fn a_store_whose_project_has_cards_with_the_same_number_numbers_them_apart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let conn = rusqlite::Connection::open(dir.path().join("moebius.sqlite")).unwrap();
            let numbers = MIGRATIONS
                .iter()
                .position(|sql| *sql == NUMBERS_SCHEMA)
                .expect("a migration numbers the cards");
            for sql in &MIGRATIONS[..numbers] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", numbers as i64)
                .unwrap();
            // Two repositories in one project, each with an issue 1 and 2, and a third repository's card 1.
            conn.execute_batch(
                "INSERT INTO projects (id, name, key, created_at) VALUES (1, 'web', 'WEB', 0), (3, 'ops', 'OPS', 0);
                 INSERT INTO repositories (id, name, path, forge, default_branch, home_project_id) VALUES (1, 'web', '/w', 'none', 'main', 1), (2, 'api', '/a', 'none', 'main', 1), (3, 'ops', '/o', 'none', 'main', 3);
                 INSERT INTO cards (id, repository_id, number, title, body, author, column_name, created_at, project_id) VALUES
                    (1, 1, 1, 'a', '', 'x', 'Backlog', 0, 1), (2, 1, 2, 'b', '', 'x', 'Backlog', 0, 1),
                    (3, 2, 1, 'c', '', 'x', 'Backlog', 0, 1), (4, 2, 2, 'd', '', 'x', 'Backlog', 0, 1),
                    (5, 3, 1, 'e', '', 'x', 'Backlog', 0, 3);",
            )
            .unwrap();
        }
        let store = Store::open(dir.path()).unwrap();
        let numbers: Vec<(i64, i64)> = (1..=5)
            .map(|id| (id, store.card(id).unwrap().unwrap().project_number))
            .collect();
        assert_eq!(numbers, [(1, 1), (2, 2), (3, 3), (4, 4), (5, 1)]);
    }

    #[test]
    fn a_home_project_that_picked_a_kind_goes_with_its_repository_and_a_moved_card_loses_a_kind_the_target_did_not_pick()
     {
        let (_dir, s) = test_store();
        let bug = s.add_kind("Bug", &[]).unwrap();
        let web = s.add_repository("web", "/srv/web", "none", "main").unwrap();
        s.set_project_kinds(web.home_project_id, &[bug.id]).unwrap();
        let card = s.add_card(web.id, "a", "", "lea", "Backlog").unwrap();
        s.set_card_kind(card.id, Some(bug.id)).unwrap();
        let other = s.add_project("Storefront", "SHOP").unwrap();
        s.set_card_project(card.id, other.id).unwrap();
        assert_eq!(s.card(card.id).unwrap().unwrap().kind, None);
        s.set_project_kinds(other.id, &[bug.id]).unwrap();
        s.set_card_kind(card.id, Some(bug.id)).unwrap();
        s.set_card_project(card.id, other.id).unwrap();
        assert_eq!(
            s.card(card.id).unwrap().unwrap().kind.as_deref(),
            Some("Bug"),
            "a target that picked the kind keeps it"
        );
        s.delete_card(card.id, false).unwrap();
        assert!(s.delete_repository(web.id).unwrap());
        assert!(s.project(web.home_project_id).unwrap().is_none());
    }
}
