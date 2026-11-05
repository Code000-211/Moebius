//! The board as a person sees it in a terminal: `board` writes the columns and their cards, `card` one card (or its
//! timeline or runs), `inbox` what waits for a person and `inbox act` does what the app's buttons do, `issue create`
//! adds a card to the backlog. All but `issue create` and the timeline read the snapshot.

use clap::{Subcommand, ValueEnum};
use serde_json::{Value, json};

use super::{Plan, Request, Show, cell, table};

#[derive(Debug, Subcommand)]
pub enum InboxCommand {
    /// Do to an item what the app does with a button: approve, reject, merge, retry, confirm or dismiss, as its kind
    /// allows.
    Act {
        item: i64,
        action: Verb,
        /// Why; sent with the decision.
        #[arg(long, default_value = "")]
        reason: String,
    },
}

/// What a person can do to an inbox item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Verb {
    Approve,
    Reject,
    Merge,
    Retry,
    Confirm,
    Dismiss,
}

#[derive(Debug, Subcommand)]
pub enum IssueCommand {
    /// Create an issue, which becomes a card in the backlog.
    Create {
        /// The id of the repository.
        repository: i64,
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        /// The number of the issue it is a sub-issue of.
        #[arg(long)]
        parent: Option<i64>,
        /// A label; give it again for more.
        #[arg(long = "label")]
        labels: Vec<String>,
        /// The number of an issue that blocks it; give it again for more.
        #[arg(long = "blocked-by")]
        blocked_by: Vec<i64>,
    },
    /// The issues of a repository, filtered: by parent, label, state, claim and blocking.
    List {
        /// The id of the repository.
        repository: i64,
        /// Only the sub-issues of this issue number.
        #[arg(long)]
        parent: Option<i64>,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        state: Option<IssueState>,
        /// Only issues somebody claimed.
        #[arg(long, conflicts_with = "unclaimed")]
        claimed: bool,
        /// Only issues nobody claimed.
        #[arg(long)]
        unclaimed: bool,
        /// Only issues an issue that is not done blocks.
        #[arg(long, conflicts_with = "unblocked")]
        blocked: bool,
        /// Only issues nothing blocks.
        #[arg(long)]
        unblocked: bool,
    },
    /// One issue: its state, labels, claim, blockers and sub-issues.
    Show {
        /// The id of the card.
        card: i64,
    },
    /// Change an issue's title or body.
    Edit {
        card: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
    /// Close an issue: its card goes to Done and its blockers count as resolved.
    Close {
        card: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
    /// Reopen a closed issue: its card goes back to the backlog.
    Reopen {
        card: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
    /// Claim an issue so that no other session takes it; fails when somebody else claimed it.
    Claim {
        card: i64,
        /// Who claims; the caller when left out.
        #[arg(long = "as")]
        by: Option<String>,
    },
    /// Release the claim on an issue; a claim made under a name is released with that name.
    Release {
        card: i64,
        /// The name the issue was claimed under.
        #[arg(long = "as")]
        by: Option<String>,
    },
    /// Add and remove labels.
    Labels {
        card: i64,
        #[arg(long)]
        add: Vec<String>,
        #[arg(long)]
        remove: Vec<String>,
    },
    /// Make an issue a sub-issue of the issue number `parent`, or of `none`.
    Parent {
        card: i64,
        /// An issue number, or `none`.
        parent: String,
    },
    /// Add and remove the issues that block an issue.
    Blocker {
        #[command(subcommand)]
        command: BlockerCommand,
    },
    /// Write a comment on an issue.
    Comment { card: i64, body: String },
    /// The comments on an issue.
    Comments { card: i64 },
}

#[derive(Debug, Subcommand)]
pub enum BlockerCommand {
    /// Make the issue number `blocker` block the issue; refused when that makes a cycle.
    Add { card: i64, blocker: i64 },
    /// Stop the issue number `blocker` from blocking the issue.
    Remove { card: i64, blocker: i64 },
}

/// The state an issue is filtered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum IssueState {
    Open,
    Closed,
}

/// `board [repository]`.
pub fn board(repository: &Option<String>) -> Plan {
    Ok((
        Request::get("/v1/snapshot"),
        Show::Board(repository.clone()),
    ))
}

/// `card <id> [--history | --runs]`.
pub fn card(id: i64, history: bool, runs: bool) -> Plan {
    Ok(if history {
        (
            Request::get(format!("/v1/cards/{id}/history")),
            Show::Table(&["at", "kind", "data"]),
        )
    } else if runs {
        (
            Request::get(format!("/v1/cards/{id}/runs")),
            Show::Table(&["id", "agent", "purpose", "status", "summary"]),
        )
    } else {
        (Request::get("/v1/snapshot"), Show::Card(id))
    })
}

/// `inbox`, and `inbox act`, which looks the item up in the snapshot first.
pub fn inbox(
    command: Option<&InboxCommand>,
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
) -> Plan {
    let Some(InboxCommand::Act {
        item,
        action,
        reason,
    }) = command
    else {
        return Ok((Request::get("/v1/snapshot"), Show::Inbox));
    };
    let snapshot = get("/v1/snapshot")?;
    let found = snapshot["inbox"]
        .as_array()
        .and_then(|items| items.iter().find(|i| i["id"] == *item))
        .ok_or_else(|| format!("no item {item} in the inbox"))?;
    Ok((inbox_request(found, *action, reason)?, Show::Object))
}

/// The text of `value` as a URL query component.
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The issue number `text` names, or none for `none`; a leading `#` is fine.
fn issue_number(text: &str) -> Result<Option<i64>, String> {
    if text.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    text.trim_start_matches('#')
        .parse()
        .map(Some)
        .map_err(|_| format!("{text:?} is not an issue number or none"))
}

/// The columns `issue list` shows.
const ISSUES: &[&str] = &[
    "id",
    "number",
    "state",
    "title",
    "labels",
    "claimed_by",
    "blocked_by",
];

/// `issue ...`: creating, listing, showing and changing issues. `get` reads a route, for the commands that change
/// part of what an issue has.
pub fn issue(command: &IssueCommand, get: &mut dyn FnMut(&str) -> Result<Value, String>) -> Plan {
    let card_path = |card: &i64, rest: &str| format!("/v1/cards/{card}{rest}");
    let reason = |reason: &String| json!({ "reason": reason });
    Ok(match command {
        IssueCommand::Create {
            repository,
            title,
            body,
            parent,
            labels,
            blocked_by,
        } => {
            let mut issue = json!({ "title": title, "body": body });
            if let Some(parent) = parent {
                issue["parent"] = json!(parent);
            }
            if !labels.is_empty() {
                issue["labels"] = json!(labels);
            }
            if !blocked_by.is_empty() {
                issue["blocked_by"] = json!(blocked_by);
            }
            (
                Request::post(format!("/v1/repositories/{repository}/issues"), issue),
                Show::Object,
            )
        }
        IssueCommand::List {
            repository,
            parent,
            label,
            state,
            claimed,
            unclaimed,
            blocked,
            unblocked,
        } => {
            let mut query = Vec::new();
            if let Some(parent) = parent {
                query.push(format!("parent={parent}"));
            }
            if let Some(label) = label {
                query.push(format!("label={}", encoded(label)));
            }
            match state {
                Some(IssueState::Open) => query.push("state=open".into()),
                Some(IssueState::Closed) => query.push("state=closed".into()),
                None => {}
            }
            if *claimed || *unclaimed {
                query.push(format!("claimed={claimed}"));
            }
            if *blocked || *unblocked {
                query.push(format!("blocked={blocked}"));
            }
            let query = if query.is_empty() {
                String::new()
            } else {
                format!("?{}", query.join("&"))
            };
            (
                Request::get(format!("/v1/repositories/{repository}/issues{query}")),
                Show::Table(ISSUES),
            )
        }
        IssueCommand::Show { card } => (Request::get(card_path(card, "")), Show::Issue),
        IssueCommand::Edit { card, title, body } => {
            let current = get(&card_path(card, ""))?;
            let text = |field: &str| current[field].as_str().unwrap_or_default().to_string();
            (
                Request::put(
                    card_path(card, ""),
                    json!({
                        "title": title.clone().unwrap_or_else(|| text("title")),
                        "body": body.clone().unwrap_or_else(|| text("body")),
                    }),
                ),
                Show::Object,
            )
        }
        IssueCommand::Close { card, reason: why } => (
            Request::post(card_path(card, "/close"), reason(why)),
            Show::Object,
        ),
        IssueCommand::Reopen { card, reason: why } => (
            Request::post(card_path(card, "/reopen"), reason(why)),
            Show::Object,
        ),
        IssueCommand::Claim { card, by } => (
            Request::post(card_path(card, "/claim"), json!({ "by": by })),
            Show::Object,
        ),
        IssueCommand::Release { card, by } => (
            Request::delete_with(
                card_path(card, "/claim"),
                by.as_ref().map(|by| json!({ "by": by })),
            ),
            Show::Object,
        ),
        IssueCommand::Labels { card, add, remove } => {
            let current = get(&card_path(card, ""))?;
            let mut labels: Vec<String> = current["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| l.as_str().map(String::from))
                .filter(|l| !remove.contains(l))
                .collect();
            for label in add {
                if !labels.contains(label) {
                    labels.push(label.clone());
                }
            }
            (
                Request::put(card_path(card, "/labels"), json!({ "labels": labels })),
                Show::Object,
            )
        }
        IssueCommand::Parent { card, parent } => (
            Request::put(
                card_path(card, "/parent"),
                json!({ "parent": issue_number(parent)? }),
            ),
            Show::Object,
        ),
        IssueCommand::Blocker {
            command: BlockerCommand::Add { card, blocker },
        } => (
            Request::post(card_path(card, "/blockers"), json!({ "blocker": blocker })),
            Show::Raw,
        ),
        IssueCommand::Blocker {
            command: BlockerCommand::Remove { card, blocker },
        } => (
            Request::delete(card_path(card, &format!("/blockers/{blocker}"))),
            Show::Raw,
        ),
        IssueCommand::Comment { card, body } => (
            Request::post(card_path(card, "/comments"), json!({ "body": body })),
            Show::Object,
        ),
        IssueCommand::Comments { card } => (
            Request::get(card_path(card, "/comments")),
            Show::Table(&["at", "author", "body"]),
        ),
    })
}

/// An issue as `issue show` writes it: its fields, then its sub-issues.
pub fn show_issue(issue: &Value) -> String {
    let mut shown = issue.clone();
    let children = shown
        .as_object_mut()
        .and_then(|fields| fields.remove("children"));
    let mut text = super::object(&shown);
    if let Some(children) = children.filter(|c| c.as_array().is_some_and(|c| !c.is_empty())) {
        text += &format!(
            "\nsub-issues:\n{}",
            super::table(&children, &["number", "state", "column", "title"])
                .lines()
                .map(|line| format!("  {line}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    text
}

/// The request that does `verb` to the inbox item `item`, as the app's button of that kind does.
fn inbox_request(item: &Value, verb: Verb, reason: &str) -> Result<Request, String> {
    let kind = item["kind"].as_str().unwrap_or_default();
    let id = |value: &Value| {
        value
            .as_i64()
            .ok_or_else(|| format!("item {} has no id to act on", item["id"]))
    };
    let (card, data) = (&item["card_id"], &item["data"]);
    let because = |path: String| Request::post(path, json!({ "reason": reason }));
    Ok(match (kind, verb) {
        ("approve_issue", Verb::Approve) => because(format!("/v1/cards/{}/approve", id(card)?)),
        ("stuck" | "change_closed", Verb::Retry) => {
            because(format!("/v1/cards/{}/retry", id(card)?))
        }
        ("ready_to_merge", Verb::Merge) => {
            because(format!("/v1/changes/{}/merge", id(&data["change"])?))
        }
        ("ready_to_merge", Verb::Reject) => {
            because(format!("/v1/changes/{}/reject", id(&data["change"])?))
        }
        ("proposal", Verb::Merge) => {
            because(format!("/v1/proposals/{}/merge", id(&data["proposal"])?))
        }
        ("proposal", Verb::Dismiss) => {
            because(format!("/v1/proposals/{}/dismiss", id(&data["proposal"])?))
        }
        ("takeover", Verb::Merge) => {
            because(format!("/v1/takeovers/{}/merge", id(&data["takeover"])?))
        }
        ("takeover", Verb::Dismiss) => {
            because(format!("/v1/takeovers/{}/dismiss", id(&data["takeover"])?))
        }
        ("grilling_issues", Verb::Confirm) => Request::post(
            format!("/v1/grillings/{}/confirm", id(&data["grilling"])?),
            json!({ "issues": data["issues"] }),
        ),
        ("grilling_issues" | "grilling_report", Verb::Dismiss) => Request::post(
            format!("/v1/grillings/{}/dismiss", id(&data["grilling"])?),
            json!({}),
        ),
        _ => {
            let allowed = match kind {
                "approve_issue" => "approve",
                "stuck" | "change_closed" => "retry",
                "ready_to_merge" => "merge or reject",
                "proposal" | "takeover" => "merge or dismiss",
                "grilling_issues" => "confirm or dismiss",
                "grilling_report" => "dismiss",
                _ => "nothing here; use `moebius api`",
            };
            return Err(format!(
                "item {} is a {kind} item: it takes {allowed}",
                item["id"]
            ));
        }
    })
}

/// The columns in the order of the board, then any other.
const COLUMNS: [&str; 6] = [
    "Backlog",
    "Not started",
    "In progress",
    "In review",
    "Ready to merge",
    "Done",
];

/// The boards of the snapshot, or the one of the repository `filter` (its name or id), column by column.
pub fn show_board(snapshot: &Value, filter: Option<&str>) -> Result<String, String> {
    let boards: Vec<&Value> = snapshot["boards"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|board| {
            filter.is_none_or(|f| {
                board["repository"]["name"] == f
                    || board["repository"]["id"].to_string().as_str() == f
            })
        })
        .collect();
    if boards.is_empty() {
        return Err(match filter {
            Some(f) => format!("no repository {f}"),
            None => "no repository yet".into(),
        });
    }
    let mut out = Vec::new();
    for board in boards {
        let repository = &board["repository"];
        out.push(format!(
            "{} (repository {})",
            cell(&repository["name"]),
            cell(&repository["id"])
        ));
        let cards = board["cards"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let mut columns: Vec<String> = COLUMNS.iter().map(|c| c.to_string()).collect();
        for card in cards {
            let column = cell(&card["column"]);
            if !columns.contains(&column) {
                columns.push(column);
            }
        }
        for column in columns {
            let here: Vec<&Value> = cards
                .iter()
                .filter(|c| cell(&c["column"]) == column)
                .collect();
            out.push(format!("  {column} ({})", here.len()));
            for card in here {
                let mut line = format!(
                    "    {}  #{}  {}",
                    cell(&card["id"]),
                    cell(&card["number"]),
                    cell(&card["title"])
                );
                if let Some(run) = card["run"].as_object() {
                    line += &format!(" [{} {}]", cell(&run["agent"]), cell(&run["status"]));
                }
                if let Some(why) = board["waiting"][card["id"].to_string()].as_str() {
                    line += &format!(" (waiting: {})", why.lines().next().unwrap_or_default());
                }
                out.push(line);
            }
        }
    }
    Ok(out.join("\n"))
}

/// The card `id` of the snapshot: where it is, its run and change, why it waits, and its body.
pub fn show_card(snapshot: &Value, id: i64) -> Result<String, String> {
    for board in snapshot["boards"].as_array().into_iter().flatten() {
        let Some(card) = board["cards"]
            .as_array()
            .and_then(|cards| cards.iter().find(|c| c["id"] == id))
        else {
            continue;
        };
        let mut lines = vec![
            format!(
                "card {id} #{} {}",
                cell(&card["number"]),
                cell(&card["title"])
            ),
            format!(
                "repository: {} ({})",
                cell(&board["repository"]["name"]),
                cell(&card["repository_id"])
            ),
            format!("column: {}", cell(&card["column"])),
            format!("author: {}", cell(&card["author"])),
            format!(
                "rounds: {}, tokens: {}",
                cell(&card["rounds"]),
                cell(&card["tokens"])
            ),
        ];
        if let Some(run) = card["run"].as_object() {
            lines.push(format!(
                "run: {} {} {} ({})",
                cell(&run["id"]),
                cell(&run["agent"]),
                cell(&run["status"]),
                cell(&run["purpose"])
            ));
        }
        if let Some(change) = card["change"].as_object() {
            lines.push(format!(
                "change: {} checks {} review {}",
                cell(&change["id"]),
                cell(&change["checks"]),
                cell(&change["review_verdict"])
            ));
        }
        if let Some(why) = board["waiting"][id.to_string()].as_str() {
            lines.push(format!("waiting: {why}"));
        }
        if let Some(note) = card["note"].as_str() {
            lines.push(format!("note: {note}"));
        }
        if card["blocked_by"].as_array().is_some_and(|b| !b.is_empty()) {
            lines.push(format!("blocked by: {}", card["blocked_by"]));
        }
        if let Some(body) = card["body"].as_str().filter(|b| !b.trim().is_empty()) {
            lines.push(String::new());
            lines.push(body.trim_end().to_string());
        }
        return Ok(lines.join("\n"));
    }
    Err(format!("no card {id}"))
}

/// The inbox of the snapshot, one item a line; an item of no card shows `-`.
pub fn show_inbox(snapshot: &Value) -> String {
    let items = Value::Array(
        snapshot["inbox"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| {
                let mut item = item.clone();
                if item["card_id"].is_null() {
                    item["card_id"] = "-".into();
                }
                item
            })
            .collect(),
    );
    table(&items, &["id", "kind", "card_id", "title"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::render;
    use crate::cli::tests::Probe;
    use clap::Parser;

    #[test]
    fn an_inbox_item_is_acted_on_as_its_kind_allows() {
        let item = |kind: &str, card: Value, data: Value| json!({"id": 5, "kind": kind, "card_id": card, "title": "t", "data": data});
        let path = |item: &Value, verb| inbox_request(item, verb, "ok").map(|r| (r.path, r.body));
        assert_eq!(
            path(&item("approve_issue", json!(3), json!({})), Verb::Approve).unwrap(),
            ("/v1/cards/3/approve".into(), Some(json!({"reason": "ok"})))
        );
        assert_eq!(
            path(&item("stuck", json!(3), json!({})), Verb::Retry)
                .unwrap()
                .0,
            "/v1/cards/3/retry"
        );
        assert_eq!(
            path(
                &item("ready_to_merge", json!(3), json!({"change": 9})),
                Verb::Reject
            )
            .unwrap()
            .0,
            "/v1/changes/9/reject"
        );
        assert_eq!(
            path(
                &item("proposal", Value::Null, json!({"proposal": 2})),
                Verb::Dismiss
            )
            .unwrap()
            .0,
            "/v1/proposals/2/dismiss"
        );
        assert_eq!(
            path(
                &item("takeover", Value::Null, json!({"takeover": 4})),
                Verb::Merge
            )
            .unwrap()
            .0,
            "/v1/takeovers/4/merge"
        );
        assert_eq!(
            path(
                &item(
                    "grilling_issues",
                    Value::Null,
                    json!({"grilling": 6, "issues": [{"title": "a"}]})
                ),
                Verb::Confirm
            )
            .unwrap(),
            (
                "/v1/grillings/6/confirm".into(),
                Some(json!({"issues": [{"title": "a"}]}))
            )
        );
        let refused = path(&item("stuck", json!(3), json!({})), Verb::Merge).unwrap_err();
        assert!(
            refused.contains("stuck") && refused.contains("retry"),
            "{refused}"
        );
        assert!(path(&item("custom", Value::Null, json!({})), Verb::Merge).is_err());
    }

    #[test]
    fn acting_on_an_item_looks_it_up_in_the_inbox() {
        let probe =
            Probe::try_parse_from(["moebius", "inbox", "act", "5", "merge", "--reason", "ok"])
                .unwrap();
        let snapshot = json!({"inbox": [{"id": 5, "kind": "ready_to_merge", "card_id": 1, "data": {"change": 8}}]});
        let (request, _) = probe.command.plan(&mut |_| Ok(snapshot.clone())).unwrap();
        assert_eq!(request.path, "/v1/changes/8/merge");
        let missing = probe
            .command
            .plan(&mut |_| Ok(json!({"inbox": []})))
            .unwrap_err();
        assert_eq!(missing, "no item 5 in the inbox");
    }

    #[test]
    fn the_board_is_written_by_column() {
        let snapshot = json!({"boards": [{
            "repository": {"id": 1, "name": "demo"},
            "cards": [
                {"id": 11, "number": 1, "title": "First", "column": "Not started", "run": null},
                {"id": 12,
                    "repository_id": 1, "number": 2, "title": "Second", "column": "In progress",
                 "run": {"id": 3, "agent": "Coder", "purpose": "code", "status": "running"}},
                {"id": 13, "number": 3, "title": "Third", "column": "Not started"},
            ],
            "waiting": {"13": "blocked by #2"},
        }]});
        let text = snapshot.to_string();
        let all = render(&Show::Board(None), &text).unwrap();
        assert_eq!(
            all,
            "demo (repository 1)\n  Backlog (0)\n  Not started (2)\n    11  #1  First\n    13  #3  Third (waiting: blocked by #2)\n  \
             In progress (1)\n    12  #2  Second [Coder running]\n  In review (0)\n  Ready to merge (0)\n  Done (0)"
        );
        assert_eq!(render(&Show::Board(Some("1".into())), &text).unwrap(), all);
        assert_eq!(
            render(&Show::Board(Some("demo".into())), &text).unwrap(),
            all
        );
        assert_eq!(
            render(&Show::Board(Some("other".into())), &text).unwrap_err(),
            "no repository other"
        );
        assert!(
            render(&Show::Card(12), &text)
                .unwrap()
                .starts_with("card 12 #2 Second\nrepository: demo (1)")
        );
        assert_eq!(render(&Show::Card(99), &text).unwrap_err(), "no card 99");
    }

    /// The request the command line `args` stands for, as method, path and body; `current` is what a lookup of the
    /// card answers.
    fn issue_request(args: &[&str], current: Value) -> (String, String, Option<Value>) {
        let (request, _) = crate::cli::tests::plan_with(
            std::iter::once("issue")
                .chain(args.iter().copied())
                .collect::<Vec<_>>()
                .as_slice(),
            &mut |path| {
                assert!(path.starts_with("/v1/cards/"), "{path}");
                Ok(current.clone())
            },
        )
        .unwrap();
        (request.method.to_string(), request.path, request.body)
    }

    #[test]
    fn the_issue_commands_map_to_the_tracker_routes() {
        let nothing = json!({});
        let route = |args: &[&str]| issue_request(args, nothing.clone());
        assert_eq!(
            route(&[
                "create",
                "1",
                "Map",
                "--parent",
                "4",
                "--label",
                "wayfinder:map",
                "--blocked-by",
                "2",
                "--blocked-by",
                "3"
            ]),
            (
                "POST".into(),
                "/v1/repositories/1/issues".into(),
                Some(
                    json!({"title": "Map", "body": "", "parent": 4, "labels": ["wayfinder:map"], "blocked_by": [2, 3]})
                )
            )
        );
        assert_eq!(
            route(&["list", "1"]),
            ("GET".into(), "/v1/repositories/1/issues".into(), None)
        );
        assert_eq!(
            route(&[
                "list",
                "1",
                "--parent",
                "4",
                "--label",
                "wayfinder:spike",
                "--state",
                "open",
                "--unclaimed",
                "--unblocked"
            ])
            .1,
            "/v1/repositories/1/issues?parent=4&label=wayfinder%3Aspike&state=open&claimed=false&blocked=false"
        );
        assert_eq!(
            route(&["list", "1", "--claimed", "--blocked", "--state", "closed"]).1,
            "/v1/repositories/1/issues?state=closed&claimed=true&blocked=true"
        );
        assert_eq!(
            route(&["show", "7"]),
            ("GET".into(), "/v1/cards/7".into(), None)
        );
        assert_eq!(
            route(&["close", "7", "--reason", "decided"]),
            (
                "POST".into(),
                "/v1/cards/7/close".into(),
                Some(json!({"reason": "decided"}))
            )
        );
        assert_eq!(route(&["reopen", "7"]).1, "/v1/cards/7/reopen");
        assert_eq!(
            route(&["claim", "7", "--as", "session 1"]),
            (
                "POST".into(),
                "/v1/cards/7/claim".into(),
                Some(json!({"by": "session 1"}))
            )
        );
        assert_eq!(route(&["claim", "7"]).2, Some(json!({"by": null})));
        assert_eq!(
            route(&["release", "7"]),
            ("DELETE".into(), "/v1/cards/7/claim".into(), None)
        );
        assert_eq!(
            route(&["release", "7", "--as", "session 1"]),
            (
                "DELETE".into(),
                "/v1/cards/7/claim".into(),
                Some(json!({"by": "session 1"}))
            )
        );
        assert_eq!(
            route(&["parent", "7", "#4"]),
            (
                "PUT".into(),
                "/v1/cards/7/parent".into(),
                Some(json!({"parent": 4}))
            )
        );
        assert_eq!(
            route(&["parent", "7", "none"]).2,
            Some(json!({"parent": null}))
        );
        assert_eq!(
            route(&["blocker", "add", "7", "3"]),
            (
                "POST".into(),
                "/v1/cards/7/blockers".into(),
                Some(json!({"blocker": 3}))
            )
        );
        assert_eq!(
            route(&["blocker", "remove", "7", "3"]),
            ("DELETE".into(), "/v1/cards/7/blockers/3".into(), None)
        );
        assert_eq!(
            route(&["comment", "7", "Use a queue."]),
            (
                "POST".into(),
                "/v1/cards/7/comments".into(),
                Some(json!({"body": "Use a queue."}))
            )
        );
        assert_eq!(route(&["comments", "7"]).1, "/v1/cards/7/comments");
        assert!(crate::cli::tests::plan(&["issue", "parent", "7", "soon"]).is_err());
        assert!(
            crate::cli::tests::plan(&["issue", "list", "1", "--claimed", "--unclaimed"]).is_err()
        );
    }

    #[test]
    fn editing_and_labelling_an_issue_keep_what_it_has() {
        let current = json!({"title": "Old", "body": "Text", "labels": ["a", "b"]});
        assert_eq!(
            issue_request(&["edit", "7", "--title", "New"], current.clone()),
            (
                "PUT".into(),
                "/v1/cards/7".into(),
                Some(json!({"title": "New", "body": "Text"}))
            )
        );
        assert_eq!(
            issue_request(&["edit", "7", "--body", "Other"], current.clone()).2,
            Some(json!({"title": "Old", "body": "Other"}))
        );
        assert_eq!(
            issue_request(
                &["labels", "7", "--add", "c", "--add", "a", "--remove", "b"],
                current
            )
            .2,
            Some(json!({"labels": ["a", "c"]}))
        );
    }

    #[test]
    fn an_issue_is_shown_with_its_sub_issues() {
        let issue = json!({
            "id": 7, "number": 4, "title": "Map", "state": "open", "labels": ["wayfinder:map"],
            "blocked_by": [], "claimed_by": null,
            "children": [
                {"id": 8, "number": 5, "title": "Spike", "state": "closed", "column": "Done"},
                {"id": 9, "number": 6, "title": "Decide", "state": "open", "column": "Backlog"}
            ]
        });
        let text = render(&crate::cli::Show::Issue, &issue.to_string()).unwrap();
        assert!(text.contains("title: Map"), "{text}");
        assert!(text.contains("labels: [\"wayfinder:map\"]"), "{text}");
        assert!(
            text.contains("sub-issues:\n  5  closed  Done     Spike\n  6  open    Backlog  Decide"),
            "{text}"
        );
        assert!(!text.contains("children"), "{text}");
    }
}
