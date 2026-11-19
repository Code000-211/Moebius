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

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, StoreError, now};

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
}

/// Whether `key` is a project's key: an uppercase letter, then 1 to 9 uppercase letters or digits.
pub fn is_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && (2..=KEY_LIMIT).contains(&key.len())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
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
        (SELECT repository_id FROM project_repositories WHERE project_id = projects.id ORDER BY position, repository_id))";

fn project(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        key: row.get(2)?,
        created_at: row.get(3)?,
        repositories: serde_json::from_str(&row.get::<_, String>(4)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, e.into())
        })?,
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

    /// Puts the card on the board of the project `project_id`; its column stays.
    pub fn set_card_project(&self, card_id: i64, project_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE cards SET project_id = ?1 WHERE id = ?2",
            [project_id, card_id],
        )?;
        Ok(())
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
    use crate::store::{MIGRATIONS, test_store};

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
    fn removing_a_repository_takes_its_home_project_when_nothing_is_left_on_it() {
        let (_dir, s) = test_store();
        let web = s.add_repository("web", "/srv/web", "none", "main").unwrap();
        s.add_card(web.id, "First", "", "lea", "Backlog").unwrap();
        let kept = s.add_project("Storefront", "SHOP").unwrap();
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
}
