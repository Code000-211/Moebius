//! Kinds of issues and the fields of their own: owners define them for the organisation, a project picks the kinds it
//! offers, and a card of the project has one of those kinds with values for its fields. This module checks what is
//! written (names, types, a value against its field's type and options, a person against the people of the
//! organisation, a kind against the kinds its card's project picked), keeps the records of `store::kinds`, announces a
//! change on the bus, and answers the question across projects: the cards of every project that have a kind and a
//! value, ordered by a field.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{App, AppError, Result};
use crate::bus::Event;
use crate::store::{Card, FieldDef, FieldType, Kind, KindField};

/// The most characters a name of a kind, a field or an option has.
pub const NAME_LIMIT: usize = 60;
/// The most fields a kind has.
pub const FIELD_LIMIT: usize = 30;
/// The most characters a `text` value has.
pub const TEXT_LIMIT: usize = 2000;

/// A kind as the desktop API answers it: the kind, its fields and the projects that picked it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindView {
    #[serde(flatten)]
    pub kind: Kind,
    /// The ids of the projects that offer the kind.
    pub projects: Vec<i64>,
}

impl KindView {
    /// The view as `sight` sees it: without the private projects whose records it does not see, so that the list
    /// does not tell that they exist (see `app::projects::sight`).
    pub fn seen_by(mut self, sight: &super::Sight) -> Self {
        self.projects.retain(|id| sight.project_record(*id));
        self
    }
}

/// A question across projects: every part that is set must match. See [`App::issues_across`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardQuery {
    /// Only the cards of this project.
    pub project: Option<i64>,
    /// Only the cards of this kind (its name, in any case).
    pub kind: Option<String>,
    /// `open` or `closed`.
    pub state: Option<String>,
    /// Only the cards whose field of this name has this value, all of them.
    pub fields: Vec<(String, String)>,
    /// The name of a field the cards are ordered by; without it they are in the order they were made.
    pub sort: Option<String>,
    /// Orders by the field the other way round.
    pub descending: bool,
}

/// Refuses a name that is empty, too long, padded or has a control character; `what` says what it names.
fn check_name(what: &str, name: &str) -> Result<()> {
    if name.is_empty()
        || name.chars().count() > NAME_LIMIT
        || name.trim() != name
        || name.chars().any(char::is_control)
    {
        return Err(AppError::Invalid(format!(
            "{name:?} is not a {what} name: 1 to {NAME_LIMIT} characters, no space around it, no control character"
        )));
    }
    Ok(())
}

/// Refuses the fields of a kind that cannot be kept: too many, a name twice (in any case), a `choice` without options
/// or with an option twice, and options on any other type.
fn check_fields(fields: &[FieldDef]) -> Result<()> {
    if fields.len() > FIELD_LIMIT {
        return Err(AppError::Invalid(format!(
            "a kind has at most {FIELD_LIMIT} fields"
        )));
    }
    let mut names = BTreeSet::new();
    for field in fields {
        check_name("field", &field.name)?;
        if !names.insert(field.name.to_lowercase()) {
            return Err(AppError::Invalid(format!(
                "the field {} is there twice",
                field.name
            )));
        }
        if field.field_type == FieldType::Choice {
            if field.options.is_empty() {
                return Err(AppError::Invalid(format!(
                    "the choice {} needs at least one option",
                    field.name
                )));
            }
            let mut options = BTreeSet::new();
            for option in &field.options {
                check_name("option", option)?;
                if !options.insert(option.to_lowercase()) {
                    return Err(AppError::Invalid(format!(
                        "the option {option} of {} is there twice",
                        field.name
                    )));
                }
            }
        } else if !field.options.is_empty() {
            return Err(AppError::Invalid(format!(
                "only a choice has options, not the {} {}",
                field.field_type.as_str(),
                field.name
            )));
        }
    }
    Ok(())
}

/// Whether `text` is a date as `YYYY-MM-DD` that exists.
fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let [y, m, d] = parts[..] else { return false };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i32>(), m.parse::<u8>(), d.parse::<u8>()) else {
        return false;
    };
    time::Month::try_from(m)
        .ok()
        .is_some_and(|m| time::Date::from_calendar_date(y, m, d).is_ok())
}

/// How a stored value compares: numbers by their value, anything else as lowercase text.
fn compare(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x
            .as_f64()
            .partial_cmp(&y.as_f64())
            .unwrap_or(std::cmp::Ordering::Equal),
        _ => a
            .as_str()
            .unwrap_or_default()
            .to_lowercase()
            .cmp(&b.as_str().unwrap_or_default().to_lowercase()),
    }
}

impl App {
    /// Every kind with the projects that picked it, in the order the kinds were made.
    pub fn kind_views(&self) -> Result<Vec<KindView>> {
        let projects = self.store.projects()?;
        let mut picked = Vec::new();
        for project in &projects {
            picked.push((project.id, self.store.project_kinds(project.id)?));
        }
        Ok(self
            .store
            .kinds()?
            .into_iter()
            .map(|kind| KindView {
                projects: picked
                    .iter()
                    .filter(|(_, kinds)| kinds.contains(&kind.id))
                    .map(|(id, _)| *id)
                    .collect(),
                kind,
            })
            .collect())
    }

    fn kind_view(&self, id: i64) -> Result<KindView> {
        self.kind_views()?
            .into_iter()
            .find(|k| k.kind.id == id)
            .ok_or_else(|| AppError::NotFound(format!("kind {id}")))
    }

    /// The kind named `name` in any case, or `NotFound`.
    pub fn kind_named(&self, name: &str) -> Result<Kind> {
        self.store
            .kind_by_name(name)?
            .ok_or_else(|| AppError::NotFound(format!("kind {name}")))
    }

    /// Makes a kind. Its name is unique in any case; its fields have names that are unique in the kind.
    pub fn add_kind(&self, name: &str, fields: &[FieldDef]) -> Result<KindView> {
        check_name("kind", name)?;
        check_fields(fields)?;
        if self.store.kind_by_name(name)?.is_some() {
            return Err(AppError::Exists(format!("a kind named {name}")));
        }
        let kind = self.store.add_kind(name, fields)?;
        self.bus.publish(Event::KindChanged(kind.clone()));
        self.kind_view(kind.id)
    }

    /// Replaces the name and the fields of the kind `id`. A field is the same field while its name is (in any case):
    /// it keeps its values, but its type cannot change; take the field off and add it again for that. A field left out
    /// is removed with the values cards hold for it, and a choice that lost an option loses the values that were it.
    pub fn update_kind(&self, id: i64, name: &str, fields: &[FieldDef]) -> Result<KindView> {
        let kind = self
            .store
            .kind(id)?
            .ok_or_else(|| AppError::NotFound(format!("kind {id}")))?;
        check_name("kind", name)?;
        check_fields(fields)?;
        if self
            .store
            .kind_by_name(name)?
            .is_some_and(|other| other.id != id)
        {
            return Err(AppError::Exists(format!("a kind named {name}")));
        }
        for field in fields {
            if let Some(had) = kind
                .fields
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(&field.name))
                && had.field_type != field.field_type
            {
                return Err(AppError::Invalid(format!(
                    "the field {} is a {} and stays one: remove it and add it again as a {}",
                    had.name,
                    had.field_type.as_str(),
                    field.field_type.as_str()
                )));
            }
        }
        // A field keeps the spelling it was made with unless the new definition spells it otherwise.
        self.store.replace_kind(id, name, fields)?;
        let kind = self.kind_view(id)?;
        self.bus.publish(Event::KindChanged(kind.kind.clone()));
        for card in self
            .store
            .all_cards()?
            .iter()
            .filter(|c| c.kind.as_deref() == Some(&kind.kind.name))
        {
            self.announce_card_kind(card);
        }
        Ok(kind)
    }

    /// Deletes the kind `id`, with its fields. A kind that a card has is not deleted: the cards would lose what they say.
    /// The refusal counts only the cards `sight` sees; when every card is in a private project it does not see, it says
    /// so without a number (that the kind is in use is all it can tell, since the kind is not deleted).
    pub fn delete_kind(&self, id: i64, sight: &super::Sight) -> Result<()> {
        let kind = self
            .store
            .kind(id)?
            .ok_or_else(|| AppError::NotFound(format!("kind {id}")))?;
        let all = self.store.cards_of_kind(id, None)?;
        if all > 0 {
            let mut cards = all;
            for project in sight.hidden() {
                cards -= self.store.cards_of_kind(id, Some(*project))?;
            }
            return Err(AppError::Conflict(if cards > 0 {
                format!(
                    "{cards} cards are a {}: give them another kind first",
                    kind.name
                )
            } else {
                format!(
                    "cards of projects you do not see are a {}: they must have another kind first",
                    kind.name
                )
            }));
        }
        let projects = self.kind_view(id)?.projects;
        self.store.delete_kind(id)?;
        self.bus.publish(Event::KindRemoved { kind: id });
        for project in projects {
            self.bus.publish(Event::ProjectKindsChanged {
                project,
                kinds: self.store.project_kinds(project)?,
            });
        }
        Ok(())
    }

    /// Replaces the kinds the project `id` offers with those `names` name (in any case). A kind the project drops while
    /// a card of it still has that kind is refused.
    pub fn set_project_kinds(&self, id: i64, names: &[String]) -> Result<Vec<i64>> {
        self.project(id)?;
        let mut kinds = Vec::new();
        for name in names {
            let kind = self
                .store
                .kind_by_name(name)?
                .ok_or_else(|| AppError::Invalid(format!("there is no kind {name}")))?;
            if !kinds.contains(&kind.id) {
                kinds.push(kind.id);
            }
        }
        for dropped in self
            .store
            .project_kinds(id)?
            .into_iter()
            .filter(|k| !kinds.contains(k))
        {
            let cards = self.store.cards_of_kind(dropped, Some(id))?;
            if cards > 0 {
                let name = self
                    .store
                    .kind(dropped)?
                    .map(|k| k.name)
                    .unwrap_or_default();
                return Err(AppError::Conflict(format!(
                    "{cards} cards of the project are a {name}: give them another kind before the project drops it"
                )));
            }
        }
        self.store.set_project_kinds(id, &kinds)?;
        let kinds = self.store.project_kinds(id)?;
        self.bus.publish(Event::ProjectKindsChanged {
            project: id,
            kinds: kinds.clone(),
        });
        Ok(kinds)
    }

    /// Gives the card the kind named `name`, or none with `None`. The kind has to be one the card's project offers. A
    /// card that changes kind loses the values of the fields of its earlier kind; one that keeps it is left as it is.
    pub fn set_card_kind(&self, card_id: i64, name: Option<&str>) -> Result<Card> {
        let card = self.card(card_id)?;
        let kind = match name {
            None => None,
            Some(name) => {
                let kind = self
                    .store
                    .kind_by_name(name)?
                    .ok_or_else(|| AppError::Invalid(format!("there is no kind {name}")))?;
                if !self
                    .store
                    .project_kinds(card.project_id)?
                    .contains(&kind.id)
                {
                    let project = self.project(card.project_id)?;
                    return Err(AppError::Invalid(format!(
                        "the project {} does not offer the kind {}",
                        project.name, kind.name
                    )));
                }
                Some(kind)
            }
        };
        if card.kind.as_deref() == kind.as_ref().map(|k| k.name.as_str()) {
            return Ok(card);
        }
        self.store
            .set_card_kind(card_id, kind.as_ref().map(|k| k.id))?;
        let card = self.card(card_id)?;
        self.announce_card_kind(&card);
        Ok(card)
    }

    /// Sets the values of fields of the card's kind: each entry is a field of the kind and its value, or `null` to clear
    /// it. A value is checked against its field: text (up to [`TEXT_LIMIT`] characters), a number, one of a choice's
    /// options, a date as `YYYY-MM-DD` or the handle of a person (not disabled, unless the card had them already).
    /// Nothing is set when any entry is refused.
    pub fn set_card_fields(&self, card_id: i64, changes: &BTreeMap<String, Value>) -> Result<Card> {
        let card = self.card(card_id)?;
        let Some(kind_name) = card.kind.as_deref() else {
            return Err(AppError::Invalid(
                "the card has no kind, so it has no fields to set".into(),
            ));
        };
        let kind = self.kind_named(kind_name)?;
        let mut writes: Vec<(i64, Option<String>)> = Vec::new();
        for (name, value) in changes {
            let field = kind
                .fields
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| {
                    AppError::Invalid(format!("the kind {} has no field {name}", kind.name))
                })?;
            let stored = if value.is_null() {
                None
            } else {
                Some(self.check_value(field, value, card.fields.get(&field.name))?)
            };
            writes.push((field.id, stored));
        }
        for (field, value) in writes {
            self.store
                .set_card_field(card_id, field, value.as_deref())?;
        }
        let card = self.card(card_id)?;
        self.announce_card_kind(&card);
        Ok(card)
    }

    /// `value` as the text the store keeps for `field`, or why it cannot be one. `current` is the value the card has.
    fn check_value(
        &self,
        field: &KindField,
        value: &Value,
        current: Option<&Value>,
    ) -> Result<String> {
        let refuse = |what: &str| {
            AppError::Invalid(format!(
                "{} is a {}: {what}",
                field.name,
                field.field_type.as_str()
            ))
        };
        match field.field_type {
            FieldType::Text => match value.as_str() {
                Some(text) if text.chars().count() <= TEXT_LIMIT => Ok(text.to_string()),
                Some(_) => Err(refuse(&format!("at most {TEXT_LIMIT} characters"))),
                None => Err(refuse("give it as text")),
            },
            FieldType::Number => match value.as_f64() {
                Some(n) if n.is_finite() && value.is_number() => Ok(n.to_string()),
                _ => Err(refuse("give it as a number")),
            },
            FieldType::Choice => match value.as_str() {
                Some(text) => field
                    .options
                    .iter()
                    .find(|o| o.eq_ignore_ascii_case(text))
                    .cloned()
                    .ok_or_else(|| refuse(&format!("one of {}", field.options.join(", ")))),
                None => Err(refuse("give it as text")),
            },
            FieldType::Date => match value.as_str() {
                Some(text) if is_date(text) => Ok(text.to_string()),
                _ => Err(refuse("a date such as 2026-11-19")),
            },
            FieldType::Person => {
                let handle = value
                    .as_str()
                    .ok_or_else(|| refuse("give the handle of a person"))?
                    .trim_start_matches('@');
                let person = self
                    .store
                    .person_by_handle(handle)?
                    .ok_or_else(|| AppError::Invalid(format!("@{handle} is nobody")))?;
                let kept = current
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.eq_ignore_ascii_case(&person.handle));
                if person.disabled && !kept {
                    return Err(AppError::Invalid(format!("@{} is disabled", person.handle)));
                }
                Ok(person.handle)
            }
        }
    }

    fn announce_card_kind(&self, card: &Card) {
        self.bus.publish(Event::CardKindChanged {
            card: card.id,
            kind: card.kind.clone(),
            fields: card.fields.clone(),
        });
    }

    /// The cards of every project that match `query`, in the order they were made or ordered by a field: numbers by
    /// value, anything else by its text, and a card that has no value for the field after those that have one (also
    /// when the order is reversed). A field filter is the value a card's field must have; the field has to exist in
    /// some kind, and the cards that have no such field do not match it.
    pub fn issues_across(&self, query: &CardQuery) -> Result<Vec<Card>> {
        if let Some(id) = query.project {
            self.project(id)?;
        }
        if let Some(name) = &query.kind {
            self.kind_named(name)?;
        }
        if let Some(state) = &query.state
            && state != "open"
            && state != "closed"
        {
            return Err(AppError::Invalid(format!(
                "state is open or closed, not {state}"
            )));
        }
        let kinds = self.store.kinds()?;
        let field_type = |name: &str| {
            kinds
                .iter()
                .flat_map(|k| &k.fields)
                .find(|f| f.name.eq_ignore_ascii_case(name))
                .map(|f| f.field_type)
        };
        let mut wanted = Vec::new();
        for (name, text) in &query.fields {
            let ty = field_type(name)
                .ok_or_else(|| AppError::Invalid(format!("no kind has a field {name}")))?;
            let value = if ty == FieldType::Number {
                let n: f64 = text.parse().map_err(|_| {
                    AppError::Invalid(format!("{name} is a number, and {text:?} is not"))
                })?;
                serde_json::json!(n)
            } else {
                Value::String(text.clone())
            };
            wanted.push((name.to_lowercase(), value));
        }
        if let Some(sort) = &query.sort
            && field_type(sort).is_none()
        {
            return Err(AppError::Invalid(format!("no kind has a field {sort}")));
        }
        let value_of = |card: &Card, name: &str| -> Option<Value> {
            card.fields
                .iter()
                .find(|(n, _)| n.to_lowercase() == name.to_lowercase())
                .map(|(_, v)| v.clone())
        };
        let mut cards: Vec<Card> = self
            .store
            .all_cards()?
            .into_iter()
            .filter(|card| query.project.is_none_or(|p| card.project_id == p))
            .filter(|card| {
                query.kind.as_deref().is_none_or(|k| {
                    card.kind
                        .as_deref()
                        .is_some_and(|c| c.eq_ignore_ascii_case(k))
                })
            })
            .filter(|card| match query.state.as_deref() {
                Some("open") => !card.closed,
                Some("closed") => card.closed,
                _ => true,
            })
            .filter(|card| {
                wanted.iter().all(|(name, want)| {
                    value_of(card, name).is_some_and(|have| compare(&have, want).is_eq())
                })
            })
            .collect();
        if let Some(sort) = &query.sort {
            cards.sort_by(|a, b| match (value_of(a, sort), value_of(b, sort)) {
                (Some(x), Some(y)) if query.descending => compare(&y, &x),
                (Some(x), Some(y)) => compare(&x, &y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });
        }
        Ok(cards)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;
    use serde_json::json;

    use super::super::Sight;
    use super::*;
    use crate::store::{Role, Store};

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        (dir, app)
    }

    fn field(name: &str, field_type: FieldType, options: &[&str]) -> FieldDef {
        FieldDef {
            name: name.into(),
            field_type,
            options: options.iter().map(|o| o.to_string()).collect(),
        }
    }

    /// A bug kind with a field of each type, picked by two projects that each have a card.
    fn two_projects() -> (tempfile::TempDir, App, Card, Card) {
        let (dir, app) = app();
        app.store.add_person("lea", "Lea", Role::Member).unwrap();
        app.add_kind(
            "Bug",
            &[
                field("Severity", FieldType::Choice, &["low", "high"]),
                field("Estimate", FieldType::Number, &[]),
                field("Due", FieldType::Date, &[]),
                field("Owner", FieldType::Person, &[]),
                field("Steps", FieldType::Text, &[]),
            ],
        )
        .unwrap();
        let web = app
            .store
            .add_repository("web", "/web", "none", "main")
            .unwrap();
        let api = app
            .store
            .add_repository("api", "/api", "none", "main")
            .unwrap();
        for repo in [&web, &api] {
            app.set_project_kinds(repo.home_project_id, &["bug".into()])
                .unwrap();
        }
        let a = app
            .store
            .add_card(web.id, "a", "", "person", "Backlog")
            .unwrap();
        let b = app
            .store
            .add_card(api.id, "b", "", "person", "Backlog")
            .unwrap();
        (dir, app, a, b)
    }

    fn set(app: &App, card: i64, fields: Value) -> Result<Card> {
        let changes = serde_json::from_value(fields).unwrap();
        app.set_card_fields(card, &changes)
    }

    #[test]
    fn a_field_defined_once_is_set_on_the_cards_of_two_projects_and_a_view_across_them_filters_and_sorts_by_it()
     {
        let (_dir, app, a, b) = two_projects();
        assert_ne!(a.project_id, b.project_id);
        for (card, severity, estimate) in [(&a, "high", 5), (&b, "high", 2)] {
            app.set_card_kind(card.id, Some("Bug")).unwrap();
            set(
                &app,
                card.id,
                json!({"Severity": severity, "Estimate": estimate}),
            )
            .unwrap();
        }
        let low = app
            .store
            .add_card(a.repository_id, "low", "", "person", "Backlog")
            .unwrap();
        app.set_card_kind(low.id, Some("bug")).unwrap();
        set(&app, low.id, json!({"severity": "LOW"})).unwrap();
        let _plain = app
            .store
            .add_card(a.repository_id, "plain", "", "person", "Backlog")
            .unwrap();

        let titles = |query: CardQuery| -> Vec<String> {
            app.issues_across(&query)
                .unwrap()
                .into_iter()
                .map(|c| c.title)
                .collect()
        };
        let by = |name: &str, value: &str| CardQuery {
            fields: vec![(name.into(), value.into())],
            ..Default::default()
        };
        assert_eq!(titles(by("Severity", "high")), ["a", "b"], "both projects");
        assert_eq!(titles(by("severity", "low")), ["low"]);
        assert_eq!(
            titles(CardQuery {
                kind: Some("Bug".into()),
                ..Default::default()
            }),
            ["a", "b", "low"]
        );
        assert_eq!(
            titles(CardQuery {
                project: Some(b.project_id),
                ..by("Severity", "high")
            }),
            ["b"]
        );
        let sorted = |descending| CardQuery {
            sort: Some("Estimate".into()),
            descending,
            ..Default::default()
        };
        // 2 before 5; the cards with no estimate follow, whichever way it is ordered.
        assert_eq!(titles(sorted(false)), ["b", "a", "low", "plain"]);
        assert_eq!(titles(sorted(true)), ["a", "b", "low", "plain"]);
        assert_eq!(titles(by("Estimate", "2")), ["b"]);
        assert!(app.issues_across(&by("Nothing", "x")).is_err());
        assert!(app.issues_across(&by("Estimate", "x")).is_err());
        assert!(
            app.issues_across(&CardQuery {
                kind: Some("Epic".into()),
                ..Default::default()
            })
            .is_err()
        );
    }

    #[test]
    fn a_project_offers_only_the_kinds_it_picked() {
        let (_dir, app, a, _b) = two_projects();
        app.add_kind("Task", &[]).unwrap();
        let err = app.set_card_kind(a.id, Some("Task")).unwrap_err();
        assert!(
            err.to_string().contains("does not offer the kind Task"),
            "{err}"
        );
        assert!(app.set_card_kind(a.id, Some("Epic")).is_err());
        assert_eq!(app.card(a.id).unwrap().kind, None);

        app.set_project_kinds(a.project_id, &["Bug".into(), "Task".into()])
            .unwrap();
        app.set_card_kind(a.id, Some("Task")).unwrap();
        // A project cannot drop a kind that one of its cards has; the other project is not asked.
        let err = app
            .set_project_kinds(a.project_id, &["Bug".into()])
            .unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)), "{err}");
        app.set_card_kind(a.id, None).unwrap();
        assert_eq!(
            app.set_project_kinds(a.project_id, &["Bug".into()])
                .unwrap()
                .len(),
            1
        );
        let kinds = app.kind_views().unwrap();
        assert_eq!(kinds[0].projects.len(), 2);
        assert_eq!(kinds[1].projects, Vec::<i64>::new());
        assert!(
            app.set_project_kinds(a.project_id, &["Epic".into()])
                .is_err()
        );
    }

    #[test]
    fn a_value_is_checked_against_its_field_and_a_refusal_sets_nothing() {
        let (_dir, app, a, _b) = two_projects();
        assert!(
            set(&app, a.id, json!({"Estimate": 1})).is_err(),
            "no kind yet"
        );
        app.set_card_kind(a.id, Some("Bug")).unwrap();
        for bad in [
            json!({"Severity": "huge"}),
            json!({"Estimate": "3"}),
            json!({"Due": "2026-02-30"}),
            json!({"Due": "tomorrow"}),
            json!({"Owner": "nobody"}),
            json!({"Steps": 4}),
            json!({"Missing": 1}),
            json!({"Estimate": 1, "Severity": "huge"}),
        ] {
            assert!(set(&app, a.id, bad.clone()).is_err(), "{bad}");
        }
        assert!(app.card(a.id).unwrap().fields.is_empty());

        let mut events = app.bus.subscribe();
        let card = set(
            &app,
            a.id,
            json!({"Severity": "High", "Estimate": 2.5, "Due": "2026-11-19", "Owner": "@LEA", "Steps": "open it"}),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&card.fields).unwrap(),
            json!({"Severity": "high", "Estimate": 2.5, "Due": "2026-11-19", "Owner": "lea", "Steps": "open it"})
        );
        let event = events.try_recv().unwrap();
        assert_eq!(event.kind, "card_kind_changed");
        assert_eq!(event.data["fields"]["Owner"], "lea");

        let card = set(&app, a.id, json!({"Estimate": null})).unwrap();
        assert!(!card.fields.contains_key("Estimate"));
        assert_eq!(card.fields.len(), 4);
        // Another kind starts with no values.
        app.add_kind("Task", &[]).unwrap();
        app.set_project_kinds(a.project_id, &["Bug".into(), "Task".into()])
            .unwrap();
        let card = app.set_card_kind(a.id, Some("Task")).unwrap();
        assert!(card.fields.is_empty());
    }

    #[test]
    fn a_person_who_was_disabled_stays_but_cannot_be_chosen_again() {
        let (_dir, app, a, b) = two_projects();
        for card in [&a, &b] {
            app.set_card_kind(card.id, Some("Bug")).unwrap();
        }
        let lea = app.store.person_by_handle("lea").unwrap().unwrap();
        set(&app, a.id, json!({"Owner": "lea"})).unwrap();
        app.store.set_person_disabled(lea.id, true).unwrap();
        set(&app, a.id, json!({"Owner": "lea"})).unwrap();
        assert!(set(&app, b.id, json!({"Owner": "lea"})).is_err());
    }

    #[test]
    fn kinds_are_made_changed_and_deleted_with_their_names_and_fields_checked() {
        let (_dir, app) = app();
        for (name, fields) in [
            ("", vec![]),
            (" Bug", vec![]),
            (
                "Bug",
                vec![
                    field("A", FieldType::Text, &[]),
                    field("a", FieldType::Text, &[]),
                ],
            ),
            ("Bug", vec![field("Pick", FieldType::Choice, &[])]),
            ("Bug", vec![field("Pick", FieldType::Choice, &["x", "X"])]),
            ("Bug", vec![field("Plain", FieldType::Text, &["x"])]),
        ] {
            assert!(app.add_kind(name, &fields).is_err(), "{name:?} {fields:?}");
        }
        let mut events = app.bus.subscribe();
        let bug = app
            .add_kind("Bug", &[field("Severity", FieldType::Text, &[])])
            .unwrap();
        assert_eq!(events.try_recv().unwrap().kind, "kind_changed");
        assert!(matches!(app.add_kind("bug", &[]), Err(AppError::Exists(_))));

        let err = app
            .update_kind(
                bug.kind.id,
                "Bug",
                &[field("Severity", FieldType::Number, &[])],
            )
            .unwrap_err();
        assert!(err.to_string().contains("stays one"), "{err}");
        let renamed = app
            .update_kind(
                bug.kind.id,
                "Defect",
                &[
                    field("Severity", FieldType::Text, &[]),
                    field("Due", FieldType::Date, &[]),
                ],
            )
            .unwrap();
        assert_eq!(renamed.kind.name, "Defect");
        assert_eq!(renamed.kind.fields.len(), 2);

        let repo = app
            .store
            .add_repository("web", "/web", "none", "main")
            .unwrap();
        app.set_project_kinds(repo.home_project_id, &["Defect".into()])
            .unwrap();
        let card = app
            .store
            .add_card(repo.id, "a", "", "person", "Backlog")
            .unwrap();
        app.set_card_kind(card.id, Some("defect")).unwrap();
        assert!(matches!(
            app.delete_kind(bug.kind.id, &Sight::everything()),
            Err(AppError::Conflict(_))
        ));
        app.set_card_kind(card.id, None).unwrap();
        let mut events = app.bus.subscribe();
        app.delete_kind(bug.kind.id, &Sight::everything()).unwrap();
        assert_eq!(events.try_recv().unwrap().kind, "kind_removed");
        assert_eq!(events.try_recv().unwrap().kind, "project_kinds_changed");
        assert!(app.kind_views().unwrap().is_empty());
        assert!(app.delete_kind(bug.kind.id, &Sight::everything()).is_err());
    }
}
