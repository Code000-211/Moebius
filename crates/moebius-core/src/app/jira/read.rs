//! What the importer reads out of Jira's JSON: an issue with the fields Moebius has a place for, the sprints and the
//! epic it belongs to, its links and comments, and the custom fields that become the fields of a kind. Nothing here
//! touches the store; [`read`] turns what the client fetched into [`Issue`]s and says what it could not use.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use time::{OffsetDateTime, format_description};

use crate::repos::jira::{FieldInfo, RawIssue};
use crate::store::{FieldType, is_date};

/// A Jira user as an issue or comment names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    /// What names the user in Jira: the account id, else the key or the name of an older site.
    pub id: String,
    pub name: String,
    pub email: Option<String>,
}

/// A sprint an issue is or was in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprint {
    pub id: i64,
    pub name: String,
    /// `future`, `active` or `closed`.
    pub state: String,
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
}

/// How a link reads from the issue that has it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Link {
    /// The issue blocks the other.
    Blocks(String),
    /// The issue is blocked by the other.
    BlockedBy(String),
    /// The issue is a duplicate of the other.
    DuplicateOf(String),
    /// The other is a duplicate of the issue.
    DuplicatedBy(String),
    /// Any other link.
    Related(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub author: Option<User>,
    pub body: String,
    /// Unix seconds.
    pub at: Option<i64>,
}

/// A value of a custom field, as the kind's field of its type holds it.
#[derive(Debug, Clone, PartialEq)]
pub enum Custom {
    Text(String),
    Number(f64),
    Date(String),
    Choice(String),
}

/// An issue of Jira as the importer uses it.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub key: String,
    pub summary: String,
    pub description: String,
    pub issue_type: String,
    pub is_epic: bool,
    pub status: String,
    pub done: bool,
    pub priority: Option<String>,
    pub labels: Vec<String>,
    pub reporter: Option<User>,
    pub assignee: Option<User>,
    pub created: Option<i64>,
    pub due: Option<String>,
    pub points: Option<f64>,
    /// The key of the issue this one is a sub-task or child of, unless that is an epic.
    pub parent: Option<String>,
    /// The key of the epic the issue belongs to.
    pub epic: Option<String>,
    pub sprints: Vec<Sprint>,
    pub links: Vec<Link>,
    pub comments: Vec<Comment>,
    /// The custom fields with a value, by the name of the field in Jira.
    pub custom: BTreeMap<String, Custom>,
}

/// The type a Jira custom field becomes in a kind, with the name Jira gives the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomField {
    pub id: String,
    pub name: String,
    pub field_type: FieldType,
}

/// What was read: the issues, and the custom fields they use.
#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    pub issues: Vec<Issue>,
    pub custom_fields: Vec<CustomField>,
}

const SPRINT: &str = "com.pyxis.greenhopper.jira:gh-sprint";
const EPIC_LINK: &str = "com.pyxis.greenhopper.jira:gh-epic-link";
/// Custom fields of Jira's own that Moebius has no field for: they are planning or screen furniture, not data.
const SKIPPED_CUSTOM: [&str; 9] = [
    "com.pyxis.greenhopper.jira:gh-sprint",
    "com.pyxis.greenhopper.jira:gh-epic-link",
    "com.pyxis.greenhopper.jira:gh-epic-label",
    "com.pyxis.greenhopper.jira:gh-epic-status",
    "com.pyxis.greenhopper.jira:gh-epic-color",
    "com.pyxis.greenhopper.jira:gh-lexo-rank",
    "com.pyxis.greenhopper.jira:gh-flag",
    "com.atlassian.jira.plugin.system.customfieldtypes:atlassian-team",
    "com.atlassian.jira.plugin.system.customfieldtypes:project",
];
/// The names Jira's story point field goes by.
const POINT_NAMES: [&str; 2] = ["Story Points", "Story point estimate"];

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

fn user(value: &Value) -> Option<User> {
    let name = text(&value["displayName"]).or_else(|| text(&value["name"]))?;
    let id = text(&value["accountId"])
        .or_else(|| text(&value["key"]))
        .or_else(|| text(&value["name"]))
        .unwrap_or_else(|| name.clone());
    Some(User {
        id,
        name,
        email: text(&value["emailAddress"]),
    })
}

/// A Jira timestamp (`2026-01-02T10:00:00.000+0000`, or with a colon in the offset, or `Z`) in Unix seconds.
pub fn timestamp(text: &str) -> Option<i64> {
    if let Ok(at) = OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339) {
        return Some(at.unix_timestamp());
    }
    let formats = [
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond][offset_hour sign:mandatory][offset_minute]",
        "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory][offset_minute]",
    ];
    formats.iter().find_map(|f| {
        let format = format_description::parse_borrowed::<2>(f).ok()?;
        OffsetDateTime::parse(text, &format)
            .ok()
            .map(|at| at.unix_timestamp())
    })
}

/// The day of a Jira date or timestamp as `YYYY-MM-DD`.
fn day(text: &str) -> Option<String> {
    let head: String = text.chars().take(10).collect();
    is_date(&head).then_some(head)
}

fn sprint(value: &Value) -> Option<Sprint> {
    Some(Sprint {
        id: value["id"].as_i64()?,
        name: text(&value["name"])?,
        state: text(&value["state"])
            .unwrap_or_default()
            .to_ascii_lowercase(),
        starts_on: value["startDate"].as_str().and_then(day),
        ends_on: value["endDate"].as_str().and_then(day),
    })
}

fn links(value: &Value) -> Vec<Link> {
    let mut out = Vec::new();
    for link in value.as_array().into_iter().flatten() {
        let kind = link["type"]["name"].as_str().unwrap_or_default();
        let kind = kind.to_ascii_lowercase();
        // `outwardIssue` is the other end of "this issue <outward> that one".
        for (side, outward) in [("outwardIssue", true), ("inwardIssue", false)] {
            let Some(other) = text(&link[side]["key"]) else {
                continue;
            };
            out.push(match (kind.as_str(), outward) {
                ("blocks", true) => Link::Blocks(other),
                ("blocks", false) => Link::BlockedBy(other),
                ("duplicate", true) => Link::DuplicateOf(other),
                ("duplicate", false) => Link::DuplicatedBy(other),
                _ => Link::Related(other),
            });
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The type a custom field of this schema becomes, or none for one Moebius has no field for.
fn field_type(info: &FieldInfo) -> Option<FieldType> {
    if !info.custom {
        return None;
    }
    let schema = info.schema.as_ref()?;
    if schema
        .custom
        .as_deref()
        .is_some_and(|c| SKIPPED_CUSTOM.contains(&c))
        || POINT_NAMES.contains(&info.name.as_str())
    {
        return None;
    }
    let kind = if schema.value_type == "array" {
        schema.items.as_deref().unwrap_or_default()
    } else {
        schema.value_type.as_str()
    };
    match kind {
        "string" => Some(FieldType::Text),
        "number" => Some(FieldType::Number),
        "date" | "datetime" => Some(FieldType::Date),
        "option" => Some(FieldType::Choice),
        // A person of Jira is not yet a person of Moebius: the field keeps their name.
        "user" => Some(FieldType::Text),
        _ => None,
    }
}

/// The text of a custom field's value, whatever shape Jira gave it: a string, a number, an option (`{value}`), a user
/// (`{displayName}`) or a list of those, which is joined with commas.
fn flat(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => text(&Value::String(s.clone())),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().filter_map(flat).collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        }
        Value::Object(o) => ["value", "displayName", "name"]
            .iter()
            .find_map(|k| o.get(*k).and_then(flat)),
    }
}

fn custom_value(field_type: FieldType, value: &Value) -> Option<Custom> {
    let flat = flat(value)?;
    match field_type {
        FieldType::Number => flat
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(Custom::Number),
        FieldType::Date => day(&flat).map(Custom::Date),
        FieldType::Choice => Some(Custom::Choice(flat)),
        _ => Some(Custom::Text(flat)),
    }
}

/// Reads `issues` with the help of the field catalogue `fields`.
pub fn read(fields: &[FieldInfo], issues: &[RawIssue]) -> Read {
    let id_of = |custom: &str| {
        fields
            .iter()
            .find(|f| f.schema.as_ref().and_then(|s| s.custom.as_deref()) == Some(custom))
            .map(|f| f.id.as_str())
    };
    let (sprint_field, epic_field) = (id_of(SPRINT), id_of(EPIC_LINK));
    let point_fields: Vec<&str> = fields
        .iter()
        .filter(|f| f.custom && POINT_NAMES.contains(&f.name.as_str()))
        .map(|f| f.id.as_str())
        .collect();
    let customs: Vec<(&FieldInfo, FieldType)> = fields
        .iter()
        .filter_map(|f| field_type(f).map(|t| (f, t)))
        .collect();

    let mut used = BTreeSet::new();
    let mut read = Vec::new();
    for raw in issues {
        let f = &raw.fields;
        let issue_type = text(&f["issuetype"]["name"]).unwrap_or_else(|| "Task".into());
        let is_epic = issue_type.eq_ignore_ascii_case("epic")
            || f["issuetype"]["hierarchyLevel"].as_i64() == Some(1);
        let parent = &f["parent"];
        let parent_is_epic = text(&parent["fields"]["issuetype"]["name"])
            .is_some_and(|t| t.eq_ignore_ascii_case("epic"))
            || parent["fields"]["issuetype"]["hierarchyLevel"].as_i64() == Some(1);
        let parent_key = text(&parent["key"]);
        let epic = epic_field
            .and_then(|id| text(&f[id]))
            .or_else(|| parent_key.clone().filter(|_| parent_is_epic));
        let mut sprints: Vec<Sprint> = sprint_field
            .map(|id| {
                f[id]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(sprint)
                    .collect()
            })
            .unwrap_or_default();
        sprints.sort_by_key(|s| s.id);
        let mut custom = BTreeMap::new();
        for (info, field_type) in &customs {
            if let Some(value) = custom_value(*field_type, &f[info.id.as_str()]) {
                used.insert(info.id.clone());
                custom.insert(info.name.clone(), value);
            }
        }
        let comments = raw
            .comments
            .iter()
            .filter_map(|c| {
                let body = text(&c["body"])?;
                Some(Comment {
                    id: c["id"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| c["id"].as_i64().map(|n| n.to_string()))?,
                    author: user(&c["author"]),
                    body,
                    at: c["created"].as_str().and_then(timestamp),
                })
            })
            .collect();
        read.push(Issue {
            key: raw.key.clone(),
            summary: text(&f["summary"]).unwrap_or_else(|| raw.key.clone()),
            description: f["description"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_string(),
            issue_type,
            is_epic,
            status: text(&f["status"]["name"]).unwrap_or_default(),
            done: f["status"]["statusCategory"]["key"].as_str() == Some("done"),
            priority: text(&f["priority"]["name"]),
            labels: f["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(text)
                .collect(),
            reporter: user(&f["reporter"]).or_else(|| user(&f["creator"])),
            assignee: user(&f["assignee"]),
            created: f["created"].as_str().and_then(timestamp),
            due: f["duedate"].as_str().and_then(day),
            points: point_fields
                .iter()
                .find_map(|id| f[*id].as_f64())
                .filter(|p| p.is_finite()),
            parent: parent_key.filter(|_| !parent_is_epic),
            epic,
            sprints,
            links: links(&f["issuelinks"]),
            comments,
            custom,
        });
    }
    let custom_fields = customs
        .into_iter()
        .filter(|(info, _)| used.contains(&info.id))
        .map(|(info, field_type)| CustomField {
            id: info.id.clone(),
            name: info.name.clone(),
            field_type,
        })
        .collect();
    Read {
        issues: read,
        custom_fields,
    }
}
