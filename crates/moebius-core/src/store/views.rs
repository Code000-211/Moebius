//! The saved views: a filter of the filter bar with a layout and a sort, kept for the person who made it or shared with
//! everyone. The store keeps the records; which cards a view shows, and which views a person may see, is decided in
//! `app::views`.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, StoreError, now};

/// The tables of this part.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE views (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    owner_id INTEGER NOT NULL REFERENCES people(id),
    shared INTEGER NOT NULL DEFAULT 0,
    filter TEXT NOT NULL,
    layout TEXT NOT NULL,
    sort TEXT NOT NULL,
    descending INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE INDEX views_by_owner ON views (owner_id);
"#;

/// The layouts a view can have.
pub const LAYOUTS: [&str; 4] = ["list", "board", "calendar", "timeline"];

/// The orders a view's cards can be in.
pub const SORTS: [&str; 6] = ["created", "updated", "priority", "due", "title", "estimate"];

/// What a view asks of a card. Every part that is set must match; within a list, any value matches. An empty list or
/// text asks nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewFilter {
    /// The ids of the projects whose cards are shown.
    #[serde(default)]
    pub projects: Vec<i64>,
    /// The ids of the repositories whose cards are shown.
    #[serde(default)]
    pub repositories: Vec<i64>,
    /// Cards that have any of these labels.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Cards given to any of these people (handles, in any case).
    #[serde(default)]
    pub assignees: Vec<String>,
    /// Cards of any of these priorities (`urgent`, `high`, `medium`, `low`, `none`).
    #[serde(default)]
    pub priorities: Vec<String>,
    /// Cards in any of these cycles (ids).
    #[serde(default)]
    pub cycles: Vec<i64>,
    /// Cards in any of these milestones (ids).
    #[serde(default)]
    pub milestones: Vec<i64>,
    /// Cards in any of these columns.
    #[serde(default)]
    pub columns: Vec<String>,
    /// Cards made by any of these authors (in any case).
    #[serde(default)]
    pub authors: Vec<String>,
    /// Cards whose title or body has this text, in any case.
    #[serde(default)]
    pub text: String,
}

/// A saved view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub id: i64,
    pub name: String,
    /// The person who made it; the only one who sees it while it is not shared.
    pub owner_id: i64,
    /// Whether every member sees it, not only its owner.
    pub shared: bool,
    pub filter: ViewFilter,
    /// One of [`LAYOUTS`].
    pub layout: String,
    /// One of [`SORTS`].
    pub sort: String,
    pub descending: bool,
    pub created_at: i64,
}

const COLUMNS: &str = "id, name, owner_id, shared, filter, layout, sort, descending, created_at";

fn view(row: &Row) -> rusqlite::Result<View> {
    let filter: String = row.get(4)?;
    Ok(View {
        id: row.get(0)?,
        name: row.get(1)?,
        owner_id: row.get(2)?,
        shared: row.get(3)?,
        filter: serde_json::from_str(&filter).unwrap_or_default(),
        layout: row.get(5)?,
        sort: row.get(6)?,
        descending: row.get(7)?,
        created_at: row.get(8)?,
    })
}

/// What a person gives a view; the owner is not among them, it is the person who makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewView<'a> {
    pub name: &'a str,
    pub shared: bool,
    pub filter: &'a ViewFilter,
    pub layout: &'a str,
    pub sort: &'a str,
    pub descending: bool,
}

impl Store {
    /// Adds a view owned by `owner`. The caller checks the fields.
    pub fn add_view(&self, owner: i64, new: &NewView) -> Result<View> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO views (name, owner_id, shared, filter, layout, sort, descending, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    new.name,
                    owner,
                    new.shared,
                    serde_json::to_string(new.filter).unwrap_or_default(),
                    new.layout,
                    new.sort,
                    new.descending,
                    now()
                ],
            )?;
            conn.last_insert_rowid()
        };
        self.view(id)?
            .ok_or(StoreError::Sql(rusqlite::Error::QueryReturnedNoRows))
    }

    /// Replaces a view's name, sharing, filter, layout and sort; its owner stays.
    pub fn edit_view(&self, id: i64, new: &NewView) -> Result<()> {
        self.conn().execute(
            "UPDATE views SET name = ?2, shared = ?3, filter = ?4, layout = ?5, sort = ?6, descending = ?7 \
             WHERE id = ?1",
            params![
                id,
                new.name,
                new.shared,
                serde_json::to_string(new.filter).unwrap_or_default(),
                new.layout,
                new.sort,
                new.descending
            ],
        )?;
        Ok(())
    }

    /// The view with `id`, whoever it is for.
    pub fn view(&self, id: i64) -> Result<Option<View>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM views WHERE id = ?1"),
                [id],
                view,
            )
            .optional()?)
    }

    /// The views `person` may open: the shared ones and their own, in the order they were made.
    pub fn views_of(&self, person: i64) -> Result<Vec<View>> {
        self.rows(
            &format!("SELECT {COLUMNS} FROM views WHERE shared = 1 OR owner_id = ?1 ORDER BY id"),
            [person],
            view,
        )
    }

    /// Deletes a view. Whether there was one.
    pub fn delete_view(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM views WHERE id = ?1", [id])?
            > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store;

    const OWNER_ID: i64 = 1;

    fn new<'a>(name: &'a str, shared: bool, filter: &'a ViewFilter) -> NewView<'a> {
        NewView {
            name,
            shared,
            filter,
            layout: "list",
            sort: "created",
            descending: false,
        }
    }

    #[test]
    fn a_view_keeps_its_filter_and_a_person_lists_theirs_and_the_shared() {
        let (_dir, s) = test_store();
        let other = s
            .add_person("ben", "Ben", super::super::Role::Member)
            .unwrap();
        let filter = ViewFilter {
            labels: vec!["bug".into()],
            cycles: vec![3],
            text: "login".into(),
            ..ViewFilter::default()
        };
        let mine = s.add_view(OWNER_ID, &new("Bugs", false, &filter)).unwrap();
        assert_eq!(mine.filter, filter);
        let shared = s
            .add_view(OWNER_ID, &new("Everyone", true, &ViewFilter::default()))
            .unwrap();
        let bens = s.add_view(other.id, &new("Ben's", false, &filter)).unwrap();
        let names = |person| {
            s.views_of(person)
                .unwrap()
                .into_iter()
                .map(|v| v.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(OWNER_ID), ["Bugs", "Everyone"]);
        assert_eq!(names(other.id), ["Everyone", "Ben's"]);

        s.edit_view(shared.id, &new("Team", false, &filter))
            .unwrap();
        assert_eq!(names(other.id), ["Ben's"]);
        assert!(!s.view(shared.id).unwrap().unwrap().shared);
        assert!(s.delete_view(bens.id).unwrap());
        assert!(!s.delete_view(bens.id).unwrap());
    }
}
