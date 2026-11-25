//! The software factory's commands: `backlog` (where a project's backlog is, and the sync that makes issues of its
//! items), `upkeep` (when stale issues and pull requests of a repository are handled) and `digest` (the owner's daily
//! digest, made now).

use clap::Subcommand;
use serde_json::{Value, json};

use super::workflow::project_id;
use super::{Plan, Request, Show};

#[derive(Debug, Subcommand)]
pub enum BacklogCommand {
    /// Where the project's backlog is and how its items are read.
    Show {
        /// The project, by name or id.
        project: String,
    },
    /// Say where the project's backlog is. A backlog that is new needs `--repository` and `--path`; what is left out
    /// stays, or is the default.
    Set {
        /// The project, by name or id.
        project: String,
        /// The project's repository that holds the file, by name.
        #[arg(long)]
        repository: Option<String>,
        /// The file's path in the repository's default branch, such as `notes/backlog/optimisation-backlog.md`.
        #[arg(long)]
        path: Option<String>,
        /// A regular expression with the groups `id` and `title` that starts an item on a line.
        #[arg(long)]
        pattern: Option<String>,
        /// Text that marks an item as closed.
        #[arg(long)]
        closed_marker: Option<String>,
        /// The most issues one sync makes.
        #[arg(long)]
        cap: Option<i64>,
        /// The start of the difficulty labels, as `difficulty:` for `difficulty:hard`.
        #[arg(long)]
        difficulty_label: Option<String>,
        /// The label of an item that requires a lane.
        #[arg(long)]
        lane_label: Option<String>,
        /// Why; recorded with the decision.
        #[arg(long, default_value = "")]
        reason: String,
    },
    /// Make the issues of the backlog's items: one for each item that has none, bodies that follow their items, and
    /// closing the issues of closed items as not planned.
    Sync {
        /// The project, by name or id.
        project: String,
        /// Say what a sync would do and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Take the project's backlog away; its issues stay.
    Remove {
        /// The project, by name or id.
        project: String,
        /// Why; recorded with the decision.
        #[arg(long, default_value = "")]
        reason: String,
    },
}

/// `backlog show`, `set`, `sync` and `remove`.
pub fn backlog(
    command: &BacklogCommand,
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
) -> Plan {
    match command {
        BacklogCommand::Show { project } => Ok((
            Request::get(format!(
                "/v1/projects/{}/backlog",
                project_id(get, project)?
            )),
            Show::Object,
        )),
        BacklogCommand::Set {
            project,
            repository,
            path,
            pattern,
            closed_marker,
            cap,
            difficulty_label,
            lane_label,
            reason,
        } => {
            let mut body = json!({ "reason": reason });
            for (key, value) in [
                ("repository", repository),
                ("path", path),
                ("pattern", pattern),
                ("closed_marker", closed_marker),
                ("difficulty_label", difficulty_label),
                ("lane_label", lane_label),
            ] {
                if let Some(value) = value {
                    body[key] = json!(value);
                }
            }
            if let Some(cap) = cap {
                body["cap"] = json!(cap);
            }
            Ok((
                Request::put(
                    format!("/v1/projects/{}/backlog", project_id(get, project)?),
                    body,
                ),
                Show::Object,
            ))
        }
        BacklogCommand::Sync { project, dry_run } => Ok((
            Request::post(
                format!("/v1/projects/{}/backlog/sync", project_id(get, project)?),
                json!({ "dry_run": dry_run }),
            ),
            Show::Sync,
        )),
        BacklogCommand::Remove { project, reason } => Ok((
            Request::delete_with(
                format!("/v1/projects/{}/backlog", project_id(get, project)?),
                Some(json!({ "reason": reason })),
            ),
            Show::Done("The backlog is taken away; its issues stay.".into()),
        )),
    }
}

/// `upkeep <repository>` shows the stale settings of a repository; with an option it changes them.
pub fn upkeep(
    repository: i64,
    release_after_days: Option<i64>,
    stale_after_days: Option<i64>,
    close_after_days: Option<i64>,
) -> Plan {
    let path = format!("/v1/repositories/{repository}/upkeep");
    if release_after_days.is_none() && stale_after_days.is_none() && close_after_days.is_none() {
        return Ok((Request::get(path), Show::Object));
    }
    let mut body = json!({});
    for (key, value) in [
        ("release_after_days", release_after_days),
        ("stale_after_days", stale_after_days),
        ("close_after_days", close_after_days),
    ] {
        if let Some(days) = value {
            body[key] = json!(days);
        }
    }
    Ok((Request::put(path, body), Show::Object))
}

/// `digest` makes today's digest now when it has none yet.
pub fn digest(force: bool) -> Plan {
    Ok((
        Request::post("/v1/digest", json!({ "force": force })),
        Show::Digest,
    ))
}

/// What a sync did, as lines.
pub fn show_sync(report: &Value) -> String {
    let mut lines = vec![format!(
        "{}{} {}: {} items",
        if report["dry_run"] == true {
            "dry run: "
        } else {
            ""
        },
        report["repository"].as_str().unwrap_or_default(),
        report["path"].as_str().unwrap_or_default(),
        report["items"]
    )];
    for (key, word) in [
        ("created", "created"),
        ("updated", "updated"),
        ("labelled", "labelled"),
        ("closed", "closed as not planned"),
    ] {
        for issue in report[key].as_array().into_iter().flatten() {
            lines.push(format!(
                "{word} #{} {}",
                issue["number"],
                issue["title"].as_str().unwrap_or_default()
            ));
        }
    }
    let deferred: Vec<&str> = report["deferred"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !deferred.is_empty() {
        lines.push(format!("waiting for the cap: {}", deferred.join(", ")));
    }
    lines.push(format!("unchanged: {}", report["unchanged"]));
    for warning in report["warnings"].as_array().into_iter().flatten() {
        lines.push(format!("warning: {}", warning.as_str().unwrap_or_default()));
    }
    lines.join("\n")
}

/// The digest's text, or that today had one.
pub fn show_digest(item: &Value) -> String {
    if item.is_null() {
        return "Today's digest was made already; use --force to make it again.".into();
    }
    format!(
        "{}\n{}",
        item["title"].as_str().unwrap_or_default(),
        item["data"]["text"].as_str().unwrap_or_default()
    )
}
