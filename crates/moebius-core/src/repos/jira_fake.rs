//! A fake Jira for the tests of the import: an axum server on a port of its own that answers the few routes the client
//! reads, from a project it holds, with the credentials it expects. The project is the one of [`project`]: an epic, a
//! story with comments over two pages, a sub-task, two bugs that duplicate each other, and a task in a sprint without
//! dates. The tests change what it holds between two imports.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

/// What the fake holds and what it saw.
#[derive(Debug, Default)]
pub struct Site {
    pub fields: Vec<Value>,
    pub issues: Vec<Value>,
    /// Comments the issue's own page leaves out, by issue key.
    pub more_comments: Vec<(String, Vec<Value>)>,
    /// The `Authorization` header every request has to carry.
    pub expects: String,
    /// Answers `404` to the search that pages by a token, as a Jira without it does.
    pub legacy_search: bool,
    /// The requests it got (path and query), for the tests that look at what was asked.
    pub asked: Vec<String>,
}

pub type Shared = Arc<Mutex<Site>>;

/// Starts the fake on a port of its own; its address and what it holds.
pub async fn serve(site: Site) -> (String, Shared) {
    let shared: Shared = Arc::new(Mutex::new(site));
    let app = Router::new()
        .route("/rest/api/2/field", get(fields))
        .route("/rest/api/2/search/jql", get(search_by_token))
        .route("/rest/api/2/search", get(search_by_position))
        .route("/rest/api/2/issue/{key}/comment", get(comments))
        .with_state(shared.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (address, shared)
}

type Query_ = Query<std::collections::HashMap<String, String>>;

fn allowed(site: &Site, headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| h == site.expects)
}

fn page(issues: &[Value], start: usize, size: usize) -> Vec<Value> {
    issues.iter().skip(start).take(size).cloned().collect()
}

async fn fields(State(site): State<Shared>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    let mut site = site.lock().unwrap();
    site.asked.push("/rest/api/2/field".into());
    if !allowed(&site, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(Value::Array(site.fields.clone())))
}

async fn search_by_token(
    State(site): State<Shared>,
    headers: HeaderMap,
    Query(query): Query_,
) -> Result<Json<Value>, StatusCode> {
    let mut site = site.lock().unwrap();
    site.asked.push(format!(
        "/rest/api/2/search/jql?jql={}",
        query.get("jql").cloned().unwrap_or_default()
    ));
    if !allowed(&site, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if site.legacy_search {
        return Err(StatusCode::NOT_FOUND);
    }
    // Two issues to a page whatever is asked, so that the client has to follow the token.
    let start: usize = query
        .get("nextPageToken")
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    let issues = page(&site.issues, start, 2);
    let last = start + 2 >= site.issues.len();
    let mut answer = json!({"issues": issues, "isLast": last});
    if !last {
        answer["nextPageToken"] = json!((start + 2).to_string());
    }
    Ok(Json(answer))
}

async fn search_by_position(
    State(site): State<Shared>,
    headers: HeaderMap,
    Query(query): Query_,
) -> Result<Json<Value>, StatusCode> {
    let mut site = site.lock().unwrap();
    site.asked.push("/rest/api/2/search".into());
    if !allowed(&site, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let start: usize = query
        .get("startAt")
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    Ok(Json(json!({
        "startAt": start,
        "total": site.issues.len(),
        "issues": page(&site.issues, start, 2),
    })))
}

async fn comments(
    State(site): State<Shared>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Query(query): Query_,
) -> Result<Json<Value>, StatusCode> {
    let mut site = site.lock().unwrap();
    site.asked.push(format!("/rest/api/2/issue/{key}/comment"));
    if !allowed(&site, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let all: Vec<Value> = site
        .more_comments
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, c)| c.clone())
        .unwrap_or_default();
    let start: usize = query
        .get("startAt")
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    let left: Vec<Value> = all.into_iter().skip(start).collect();
    Ok(Json(json!({"comments": left})))
}

/// `Basic` for an email and a token, as Jira Cloud wants them.
pub fn basic(email: &str, token: &str) -> String {
    use base64::Engine;
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{email}:{token}"))
    )
}

fn user(id: &str, name: &str, email: Option<&str>) -> Value {
    let mut user = json!({"accountId": id, "displayName": name});
    if let Some(email) = email {
        user["emailAddress"] = json!(email);
    }
    user
}

fn issue(
    key: &str,
    summary: &str,
    kind: (&str, bool),
    status: (&str, &str),
    extra: Value,
) -> Value {
    let mut fields = json!({
        "summary": summary,
        "issuetype": {"name": kind.0, "subtask": kind.1, "hierarchyLevel": if kind.0 == "Epic" { 1 } else { 0 }},
        "status": {"name": status.0, "statusCategory": {"key": status.1}},
        "created": "2026-10-01T09:30:00.000+0000",
        "labels": [],
        "issuelinks": [],
        "comment": {"comments": [], "total": 0},
    });
    for (name, value) in extra.as_object().unwrap() {
        fields[name] = value.clone();
    }
    json!({"id": key.replace("WEB-", "100"), "key": key, "fields": fields})
}

fn sprint(id: i64, name: &str, state: &str, dates: Option<(&str, &str)>) -> Value {
    let mut sprint = json!({"id": id, "name": name, "state": state});
    if let Some((start, end)) = dates {
        sprint["startDate"] = json!(format!("{start}T08:00:00.000Z"));
        sprint["endDate"] = json!(format!("{end}T17:00:00.000Z"));
    }
    sprint
}

fn comment(id: &str, author: &Value, body: &str, created: &str) -> Value {
    json!({"id": id, "author": author, "body": body, "created": created})
}

/// The project `WEB`, with the credentials of a Jira Cloud account.
pub fn project(expects: String) -> Site {
    let ada = user("a1", "Ada Lovelace", Some("ada@example.com"));
    let grace = user("g1", "Grace Hopper", None);
    let old = json!({"displayName": "Old Colleague"});
    let fields = vec![
        json!({"id": "summary", "name": "Summary", "custom": false, "schema": {"type": "string"}}),
        json!({"id": "customfield_10020", "name": "Sprint", "custom": true,
               "schema": {"type": "array", "items": "json", "custom": "com.pyxis.greenhopper.jira:gh-sprint"}}),
        json!({"id": "customfield_10014", "name": "Epic Link", "custom": true,
               "schema": {"type": "any", "custom": "com.pyxis.greenhopper.jira:gh-epic-link"}}),
        json!({"id": "customfield_10016", "name": "Story point estimate", "custom": true,
               "schema": {"type": "number", "custom": "com.atlassian.jira.plugin.system.customfieldtypes:float"}}),
        json!({"id": "customfield_10030", "name": "Severity", "custom": true,
               "schema": {"type": "option", "custom": "com.atlassian.jira.plugin.system.customfieldtypes:select"}}),
        json!({"id": "customfield_10031", "name": "Customer", "custom": true,
               "schema": {"type": "string", "custom": "com.atlassian.jira.plugin.system.customfieldtypes:textfield"}}),
        json!({"id": "customfield_10032", "name": "Go-live", "custom": true,
               "schema": {"type": "date", "custom": "com.atlassian.jira.plugin.system.customfieldtypes:datepicker"}}),
        json!({"id": "customfield_10033", "name": "Rank", "custom": true,
               "schema": {"type": "any", "custom": "com.pyxis.greenhopper.jira:gh-lexo-rank"}}),
    ];
    let first = sprint(11, "Sprint 1", "closed", Some(("2026-10-01", "2026-10-14")));
    let second = sprint(12, "Sprint 2", "active", Some(("2026-10-15", "2026-10-28")));
    let third = sprint(13, "Sprint 3", "future", None);
    let issues = vec![
        issue(
            "WEB-1",
            "Checkout",
            ("Epic", false),
            ("To Do", "new"),
            json!({
                "description": "Everything to pay.", "duedate": "2026-12-31",
            }),
        ),
        issue(
            "WEB-2",
            "Pay with card",
            ("Story", false),
            ("In Progress", "indeterminate"),
            json!({
                "description": "As a buyer I pay with a card.",
                "priority": {"name": "High"},
                "labels": ["payments", "priority:stale"],
                "reporter": ada, "assignee": grace,
                "duedate": "2026-12-01",
                "customfield_10016": 5.0,
                "customfield_10014": "WEB-1",
                "customfield_10020": [first, second],
                "customfield_10030": {"value": "Major"},
                "customfield_10031": "ACME",
                "customfield_10032": "2026-12-15T00:00:00.000+0000",
                "customfield_10033": "0|i0002w:",
                "comment": {"total": 3, "comments": [
                    comment("501", &ada, "Started on this.", "2026-10-02T10:00:00.000+0000"),
                    comment("502", &old, "We tried this in 2019.", "2026-10-03T11:15:00.000+0000"),
                ]},
            }),
        ),
        issue(
            "WEB-3",
            "Validate card number",
            ("Sub-task", true),
            ("Done", "done"),
            json!({
                "parent": {"key": "WEB-2", "fields": {"issuetype": {"name": "Story"}}},
                "reporter": ada,
                "issuelinks": [{"type": {"name": "Blocks", "inward": "is blocked by", "outward": "blocks"},
                                "outwardIssue": {"key": "WEB-4"}}],
            }),
        ),
        issue(
            "WEB-4",
            "Refund fails",
            ("Bug", false),
            ("Open", "new"),
            json!({
                "priority": {"name": "Highest"},
                "reporter": grace,
                "customfield_10030": {"value": "Minor"},
                "customfield_10020": [sprint(11, "Sprint 1", "closed", Some(("2026-10-01", "2026-10-14")))],
                "issuelinks": [
                    {"type": {"name": "Blocks", "inward": "is blocked by", "outward": "blocks"},
                     "inwardIssue": {"key": "WEB-3"}},
                    {"type": {"name": "Duplicate", "inward": "is duplicated by", "outward": "duplicates"},
                     "outwardIssue": {"key": "WEB-5"}},
                    {"type": {"name": "Relates", "inward": "relates to", "outward": "relates to"},
                     "outwardIssue": {"key": "WEB-2"}},
                    {"type": {"name": "Relates", "inward": "relates to", "outward": "relates to"},
                     "outwardIssue": {"key": "OTHER-9"}},
                ],
            }),
        ),
        issue(
            "WEB-5",
            "Refund error",
            ("Bug", false),
            ("Open", "new"),
            json!({
                "customfield_10030": {"value": "Minor"},
                "issuelinks": [{"type": {"name": "Duplicate", "inward": "is duplicated by", "outward": "duplicates"},
                                "inwardIssue": {"key": "WEB-4"}}],
            }),
        ),
        issue(
            "WEB-6",
            "Plan the next sprint",
            ("Task", false),
            ("To Do", "new"),
            json!({
                "customfield_10020": [third],
            }),
        ),
    ];
    let more = vec![(
        "WEB-2".to_string(),
        vec![
            comment(
                "501",
                &ada,
                "Started on this.",
                "2026-10-02T10:00:00.000+0000",
            ),
            comment(
                "502",
                &old,
                "We tried this in 2019.",
                "2026-10-03T11:15:00.000+0000",
            ),
            comment(
                "503",
                &grace,
                "Shipping Friday.",
                "2026-10-04T12:00:00.000+0000",
            ),
        ],
    )];
    Site {
        fields,
        issues,
        more_comments: more,
        expects,
        legacy_search: false,
        asked: Vec::new(),
    }
}
