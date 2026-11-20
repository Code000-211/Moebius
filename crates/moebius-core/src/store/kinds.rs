//! Kinds of issues (ADR 0019 and the glossary): what an issue is, such as a bug or a feature, with the fields of its own
//! an owner defined for the organisation. A field has a name and one type: `text`, `number`, `choice` (one of its
//! options), `date` (`YYYY-MM-DD`) or `person` (a handle of `people`). A project picks the kinds it offers
//! (`project_kinds`); a card has at most one kind (`cards.kind_id`) and a value for some of that kind's fields
//! (`card_fields`). A field belongs to its kind, so the same field is set on the cards of every project that picked the
//! kind. Values are kept as text, in the form their type has (a number as its decimal text, a date as `YYYY-MM-DD`),
//! and read as JSON values by type.
//!
//! This module only keeps and reads the records; what a value may be, and who may change what, is checked by `app::kinds`.

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Result, Store};

/// The tables of this part, and the column that gives a card its kind.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE kinds (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    position INTEGER NOT NULL
);
CREATE TABLE kind_fields (
    id INTEGER PRIMARY KEY,
    kind_id INTEGER NOT NULL REFERENCES kinds(id),
    name TEXT NOT NULL COLLATE NOCASE,
    type TEXT NOT NULL,
    options TEXT NOT NULL DEFAULT '[]',
    position INTEGER NOT NULL,
    UNIQUE (kind_id, name)
);
CREATE TABLE project_kinds (
    project_id INTEGER NOT NULL REFERENCES projects(id),
    kind_id INTEGER NOT NULL REFERENCES kinds(id),
    PRIMARY KEY (project_id, kind_id)
);
ALTER TABLE cards ADD COLUMN kind_id INTEGER REFERENCES kinds(id);
CREATE TABLE card_fields (
    card_id INTEGER NOT NULL REFERENCES cards(id),
    field_id INTEGER NOT NULL REFERENCES kind_fields(id),
    value TEXT NOT NULL,
    PRIMARY KEY (card_id, field_id)
);
"#;

/// The name of a card's kind, for a `SELECT` of `cards`; null while it has none.
pub(super) const CARD_KIND: &str = "(SELECT name FROM kinds WHERE id = cards.kind_id)";

/// A card's field values as a JSON array of `[name, type, value]`, for a `SELECT` of `cards`.
pub(super) const CARD_FIELDS: &str = "(SELECT json_group_array(json_array(f.name, f.type, v.value)) FROM card_fields v JOIN kind_fields f ON f.id = v.field_id WHERE v.card_id = cards.id)";

/// The type of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Text,
    Number,
    Choice,
    Date,
    Person,
}

impl FieldType {
    /// The word the store and the API use for the type.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Choice => "choice",
            Self::Date => "date",
            Self::Person => "person",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "text" => Self::Text,
            "number" => Self::Number,
            "choice" => Self::Choice,
            "date" => Self::Date,
            "person" => Self::Person,
            _ => return None,
        })
    }

    /// The JSON value a stored `text` of this type stands for: a number is a JSON number, anything else a string.
    pub fn value(self, text: &str) -> Value {
        if self == Self::Number
            && let Ok(n) = text.parse::<i64>()
        {
            return Value::from(n);
        }
        if self == Self::Number
            && let Ok(n) = text.parse::<f64>()
            && let Some(n) = serde_json::Number::from_f64(n)
        {
            return Value::Number(n);
        }
        Value::String(text.to_string())
    }
}

/// A field of a kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindField {
    pub id: i64,
    /// Unique within its kind, regardless of case.
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    /// The values of a `choice`, in order; empty for any other type.
    pub options: Vec<String>,
}

/// A kind of issue with its fields, in the order they are shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kind {
    pub id: i64,
    /// Unique regardless of case.
    pub name: String,
    pub fields: Vec<KindField>,
}

/// A field as it is written when a kind is made or replaced: its name, type and options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub options: Vec<String>,
}

/// Reads the kind name and field values a [`CARD_KIND`] and [`CARD_FIELDS`] pair gave, as columns `kind` and `fields`.
pub(super) fn card_kind(
    r: &rusqlite::Row<'_>,
    kind: usize,
    fields: usize,
) -> rusqlite::Result<(Option<String>, BTreeMap<String, Value>)> {
    let bad = |index, e: Box<dyn std::error::Error + Send + Sync>| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, e)
    };
    let rows: Vec<(String, String, String)> = match r.get::<_, Option<String>>(fields)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| bad(fields, e.into()))?,
        None => Vec::new(),
    };
    let values = rows
        .into_iter()
        .map(|(name, ty, text)| {
            let ty = FieldType::parse(&ty).unwrap_or(FieldType::Text);
            (name, ty.value(&text))
        })
        .collect();
    Ok((r.get(kind)?, values))
}

fn field_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<KindField> {
    let ty: String = r.get(2)?;
    let options: String = r.get(3)?;
    Ok(KindField {
        id: r.get(0)?,
        name: r.get(1)?,
        field_type: FieldType::parse(&ty).unwrap_or(FieldType::Text),
        options: serde_json::from_str(&options).unwrap_or_default(),
    })
}

impl Store {
    /// Every kind with its fields, in the order they were made.
    pub fn kinds(&self) -> Result<Vec<Kind>> {
        let conn = self.read();
        let mut kinds: Vec<Kind> = conn
            .prepare("SELECT id, name FROM kinds ORDER BY position, id")?
            .query_map([], |r| {
                Ok(Kind {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    fields: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, type, options, kind_id FROM kind_fields ORDER BY position, id",
        )?;
        let fields = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(4)?, field_of(r)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (kind_id, field) in fields {
            if let Some(kind) = kinds.iter_mut().find(|k| k.id == kind_id) {
                kind.fields.push(field);
            }
        }
        Ok(kinds)
    }

    /// The kind with `id`.
    pub fn kind(&self, id: i64) -> Result<Option<Kind>> {
        Ok(self.kinds()?.into_iter().find(|k| k.id == id))
    }

    /// The kind named `name`, in any case.
    pub fn kind_by_name(&self, name: &str) -> Result<Option<Kind>> {
        Ok(self
            .kinds()?
            .into_iter()
            .find(|k| k.name.eq_ignore_ascii_case(name)))
    }

    /// Makes a kind with `fields`, after the kinds there are.
    pub fn add_kind(&self, name: &str, fields: &[FieldDef]) -> Result<Kind> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO kinds (name, position) VALUES (?1, (SELECT COALESCE(MAX(position), 0) + 1 FROM kinds))",
            [name],
        )?;
        let id = tx.last_insert_rowid();
        for (position, field) in fields.iter().enumerate() {
            tx.execute(
                "INSERT INTO kind_fields (kind_id, name, type, options, position) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    field.name,
                    field.field_type.as_str(),
                    serde_json::to_string(&field.options).unwrap_or_else(|_| "[]".into()),
                    position as i64
                ],
            )?;
        }
        tx.commit()?;
        drop(conn);
        Ok(self.kind(id)?.expect("the kind was just made"))
    }

    /// Replaces a kind's name and fields. A field of `fields` with the name of one the kind has keeps its id and the
    /// values cards hold for it, a field the kind has and `fields` does not name is removed with its values, and a
    /// choice that lost an option loses the values that were that option.
    pub fn replace_kind(&self, id: i64, name: &str, fields: &[FieldDef]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE kinds SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        let existing: Vec<(i64, String)> = tx
            .prepare("SELECT id, name FROM kind_fields WHERE kind_id = ?1")?
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (field_id, existing_name) in &existing {
            if !fields
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(existing_name))
            {
                tx.execute("DELETE FROM card_fields WHERE field_id = ?1", [field_id])?;
                tx.execute("DELETE FROM kind_fields WHERE id = ?1", [field_id])?;
            }
        }
        for (position, field) in fields.iter().enumerate() {
            let options = serde_json::to_string(&field.options).unwrap_or_else(|_| "[]".into());
            let field_id: Option<i64> = tx
                .query_row(
                    "SELECT id FROM kind_fields WHERE kind_id = ?1 AND name = ?2",
                    params![id, field.name],
                    |r| r.get(0),
                )
                .optional()?;
            match field_id {
                Some(field_id) => {
                    tx.execute(
                        "UPDATE kind_fields SET name = ?2, options = ?3, position = ?4 WHERE id = ?1",
                        params![field_id, field.name, options, position as i64],
                    )?;
                    if field.field_type == FieldType::Choice {
                        let stale: Vec<String> = tx
                            .prepare("SELECT DISTINCT value FROM card_fields WHERE field_id = ?1")?
                            .query_map([field_id], |r| r.get(0))?
                            .collect::<rusqlite::Result<_>>()?;
                        for value in stale {
                            // An option is the same option in any case; its value takes the spelling it has now.
                            match field
                                .options
                                .iter()
                                .find(|o| o.eq_ignore_ascii_case(&value))
                            {
                                Some(option) if *option != value => {
                                    tx.execute(
                                        "UPDATE card_fields SET value = ?3 WHERE field_id = ?1 AND value = ?2",
                                        params![field_id, value, option],
                                    )?;
                                }
                                Some(_) => {}
                                None => {
                                    tx.execute(
                                        "DELETE FROM card_fields WHERE field_id = ?1 AND value = ?2",
                                        params![field_id, value],
                                    )?;
                                }
                            }
                        }
                    }
                }
                None => {
                    tx.execute(
                        "INSERT INTO kind_fields (kind_id, name, type, options, position) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![id, field.name, field.field_type.as_str(), options, position as i64],
                    )?;
                }
            }
        }
        Ok(tx.commit()?)
    }

    /// Removes a kind with its fields and the projects' picks of it. The caller checked that no card has it.
    pub fn delete_kind(&self, id: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM card_fields WHERE field_id IN (SELECT id FROM kind_fields WHERE kind_id = ?1)",
            [id],
        )?;
        tx.execute("DELETE FROM kind_fields WHERE kind_id = ?1", [id])?;
        tx.execute("DELETE FROM project_kinds WHERE kind_id = ?1", [id])?;
        tx.execute("DELETE FROM kinds WHERE id = ?1", [id])?;
        Ok(tx.commit()?)
    }

    /// The ids of the kinds a project picked, in the order the kinds were made.
    pub fn project_kinds(&self, project_id: i64) -> Result<Vec<i64>> {
        self.rows(
            "SELECT k.id FROM project_kinds p JOIN kinds k ON k.id = p.kind_id WHERE p.project_id = ?1 ORDER BY k.position, k.id",
            [project_id],
            |r| r.get(0),
        )
    }

    /// Replaces the kinds a project picked with `kinds` (ids).
    pub fn set_project_kinds(&self, project_id: i64, kinds: &[i64]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM project_kinds WHERE project_id = ?1",
            [project_id],
        )?;
        for kind in kinds {
            tx.execute(
                "INSERT OR IGNORE INTO project_kinds (project_id, kind_id) VALUES (?1, ?2)",
                params![project_id, kind],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// How many cards of a project have the kind.
    pub fn cards_of_kind(&self, kind_id: i64, project_id: Option<i64>) -> Result<i64> {
        Ok(self.read().query_row(
            "SELECT COUNT(*) FROM cards WHERE kind_id = ?1 AND (?2 IS NULL OR project_id = ?2)",
            params![kind_id, project_id],
            |r| r.get(0),
        )?)
    }

    /// Gives a card a kind, or none, and drops the values of the fields of its earlier kind.
    pub fn set_card_kind(&self, card_id: i64, kind_id: Option<i64>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM card_fields WHERE card_id = ?1", [card_id])?;
        tx.execute(
            "UPDATE cards SET kind_id = ?2 WHERE id = ?1",
            params![card_id, kind_id],
        )?;
        Ok(tx.commit()?)
    }

    /// Sets the value (kept as text) of a field of the card's kind, or clears it with `None`.
    pub fn set_card_field(&self, card_id: i64, field_id: i64, value: Option<&str>) -> Result<()> {
        let conn = self.conn();
        match value {
            Some(value) => conn.execute(
                "INSERT INTO card_fields (card_id, field_id, value) VALUES (?1, ?2, ?3) ON CONFLICT (card_id, field_id) DO UPDATE SET value = excluded.value",
                params![card_id, field_id, value],
            )?,
            None => conn.execute(
                "DELETE FROM card_fields WHERE card_id = ?1 AND field_id = ?2",
                params![card_id, field_id],
            )?,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store;

    fn severity() -> FieldDef {
        FieldDef {
            name: "Severity".into(),
            field_type: FieldType::Choice,
            options: vec!["low".into(), "high".into()],
        }
    }

    #[test]
    fn a_kind_keeps_its_fields_in_order_and_a_card_its_values_by_type() {
        let (_dir, s) = test_store();
        let estimate = FieldDef {
            name: "Estimate".into(),
            field_type: FieldType::Number,
            options: Vec::new(),
        };
        let bug = s.add_kind("Bug", &[severity(), estimate.clone()]).unwrap();
        assert_eq!(
            bug.fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["Severity", "Estimate"]
        );
        assert_eq!(s.kind_by_name("bug").unwrap().unwrap().id, bug.id);
        assert!(
            s.add_kind("BUG", &[]).is_err(),
            "a name is unique in any case"
        );

        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        let card = s.add_card(repo.id, "t", "b", "person", "Backlog").unwrap();
        assert_eq!(card.kind, None);
        s.set_card_kind(card.id, Some(bug.id)).unwrap();
        s.set_card_field(card.id, bug.fields[0].id, Some("high"))
            .unwrap();
        s.set_card_field(card.id, bug.fields[1].id, Some("3.5"))
            .unwrap();
        let card = s.card(card.id).unwrap().unwrap();
        assert_eq!(card.kind.as_deref(), Some("Bug"));
        assert_eq!(card.fields["Severity"], serde_json::json!("high"));
        assert_eq!(card.fields["Estimate"], serde_json::json!(3.5));

        s.set_card_field(card.id, bug.fields[0].id, None).unwrap();
        assert!(
            !s.card(card.id)
                .unwrap()
                .unwrap()
                .fields
                .contains_key("Severity")
        );
        s.set_card_kind(card.id, None).unwrap();
        let card = s.card(card.id).unwrap().unwrap();
        assert_eq!((card.kind, card.fields.len()), (None, 0));
    }

    #[test]
    fn replacing_a_kind_keeps_the_values_of_a_field_that_stays_and_drops_the_rest() {
        let (_dir, s) = test_store();
        let bug = s.add_kind("Bug", &[severity()]).unwrap();
        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        let a = s.add_card(repo.id, "a", "", "person", "Backlog").unwrap();
        let b = s.add_card(repo.id, "b", "", "person", "Backlog").unwrap();
        for (card, value) in [(&a, "low"), (&b, "high")] {
            s.set_card_kind(card.id, Some(bug.id)).unwrap();
            s.set_card_field(card.id, bug.fields[0].id, Some(value))
                .unwrap();
        }
        let narrowed = FieldDef {
            options: vec!["high".into(), "urgent".into()],
            ..severity()
        };
        s.replace_kind(bug.id, "Defect", &[narrowed]).unwrap();
        assert!(
            s.card(a.id).unwrap().unwrap().fields.is_empty(),
            "low is no option now"
        );
        assert_eq!(s.card(b.id).unwrap().unwrap().fields["Severity"], "high");
        assert_eq!(
            s.card(b.id).unwrap().unwrap().kind.as_deref(),
            Some("Defect")
        );
        s.replace_kind(bug.id, "Defect", &[]).unwrap();
        assert!(s.card(b.id).unwrap().unwrap().fields.is_empty());
        assert!(s.kind(bug.id).unwrap().unwrap().fields.is_empty());
    }

    #[test]
    fn a_project_picks_kinds_and_deleting_a_kind_takes_its_picks() {
        let (_dir, s) = test_store();
        let bug = s.add_kind("Bug", &[]).unwrap();
        let task = s.add_kind("Task", &[]).unwrap();
        let project = s.add_project("Web", "WEB").unwrap();
        s.set_project_kinds(project.id, &[task.id, bug.id, bug.id])
            .unwrap();
        assert_eq!(s.project_kinds(project.id).unwrap(), [bug.id, task.id]);
        s.delete_kind(bug.id).unwrap();
        assert_eq!(s.project_kinds(project.id).unwrap(), [task.id]);
        assert_eq!(s.kinds().unwrap().len(), 1);
    }
}
