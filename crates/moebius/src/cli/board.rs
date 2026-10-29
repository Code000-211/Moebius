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
    },
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

/// `issue create`.
pub fn issue(command: &IssueCommand) -> Plan {
    let IssueCommand::Create {
        repository,
        title,
        body,
    } = command;
    Ok((
        Request::post(
            format!("/v1/repositories/{repository}/issues"),
            json!({ "title": title, "body": body }),
        ),
        Show::Object,
    ))
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
    "Ready",
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
                {"id": 11, "number": 1, "title": "First", "column": "Ready", "run": null},
                {"id": 12,
                    "repository_id": 1, "number": 2, "title": "Second", "column": "In progress",
                 "run": {"id": 3, "agent": "Coder", "purpose": "code", "status": "running"}},
                {"id": 13, "number": 3, "title": "Third", "column": "Ready"},
            ],
            "waiting": {"13": "blocked by #2"},
        }]});
        let text = snapshot.to_string();
        let all = render(&Show::Board(None), &text).unwrap();
        assert_eq!(
            all,
            "demo (repository 1)\n  Backlog (0)\n  Ready (2)\n    11  #1  First\n    13  #3  Third (waiting: blocked by #2)\n  \
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
}
