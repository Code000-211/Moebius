//! Grilling (spec 7): a chat with the built-in agent Grilling-partner, which grills a person's idea until the work is
//! clear and ends in a set of issues the person confirms, or in a research report.
//!
//! A grilling is a chat in a repository: `POST /v1/chats` with the agent `Grilling-partner` and a repository (it is
//! refused without one). The agent writes its result into the chat's worktree as exactly one of
//! `.moebius/grilling/issues.json`, a list of `{title, body, blocked_by}` where `blocked_by` are indexes into the list,
//! or `.moebius/grilling/report.md`. It commits and pushes nothing, and a grilling does not offer its worktree as a
//! change.
//!
//! When the chat ends or is stopped, [`conclude`] reads the files before the worktree is removed and stores a
//! `grilling`: `proposed` with an inbox item `grilling_issues` for issues, `reported` with an inbox item
//! `grilling_report` for a report, or `empty`, with the reason and no inbox item, when there is no file, `issues.json`
//! is broken or the issues do not hold together (a blank title, a `blocked_by` out of range or in a cycle). A person
//! can also have [`publish`] read what the chat left while it is still open, on its say-so, and keep grilling; a
//! later publish replaces what was stored, and [`conclude`] is then skipped at the end.
//!
//! The person reads the proposal, may edit, drop and reorder the issues, and [`confirm`]s the final list: one card per
//! issue in the repository's Backlog (on GitHub, an issue is opened first), which needs no approval and moves on to
//! Not started (`card.confirmed`). [`dismiss`] ends a proposal or a report without cards.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::json;

use crate::app::{App, AppError, PERSON, Result};
use crate::blobs::redact;
use crate::bus::Event;
use crate::decide::{Actor, Question};
use crate::store::{Grilling, GrillingIssue};

/// What a grilling's worktree holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The issues of `issues.json`.
    Issues(Vec<GrillingIssue>),
    /// The text of `report.md`.
    Report(String),
    /// Neither file is usable, and why.
    Empty(String),
}

/// The order to create `issues` in, so that every issue comes after those that block it, the first first when there
/// is a choice; or why the list cannot be made into cards.
pub fn creation_order(issues: &[GrillingIssue]) -> std::result::Result<Vec<usize>, String> {
    if issues.is_empty() {
        return Err("there are no issues".into());
    }
    for (index, issue) in issues.iter().enumerate() {
        if issue.title.trim().is_empty() {
            return Err(format!("issue {index} has no title"));
        }
        if let Some(blocker) = issue.blocked_by.iter().find(|&&b| b >= issues.len()) {
            return Err(format!(
                "issue {index} is blocked by {blocker}, which is not in the list"
            ));
        }
        if issue.blocked_by.contains(&index) {
            return Err(format!("issue {index} is blocked by itself"));
        }
    }
    let mut order = Vec::new();
    while order.len() < issues.len() {
        let next = (0..issues.len()).find(|index| {
            !order.contains(index) && issues[*index].blocked_by.iter().all(|b| order.contains(b))
        });
        match next {
            Some(index) => order.push(index),
            None => return Err("the issues block each other in a circle".into()),
        }
    }
    Ok(order)
}

/// Makes git ignore `.moebius/` in the grilling's `worktree`, so that the files the agent writes there are not noise in
/// its `git status`. The line goes into the `info/exclude` file git names for the worktree, which a clone shares among
/// its worktrees; nothing of a repository is read from a `.moebius/` folder any more, so ignoring it hides no setting.
pub fn ignore_result_folder(worktree: &Path) -> std::io::Result<()> {
    use std::io::Write;

    let named = crate::repos::git(worktree, &["rev-parse", "--git-path", "info/exclude"])
        .map_err(std::io::Error::other)?;
    let file = worktree.join(named);
    if std::fs::read_to_string(&file).is_ok_and(|text| text.lines().any(|l| l == ".moebius/")) {
        return Ok(());
    }
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(file)?,
        ".moebius/"
    )
}

/// Reads the result the agent left in `worktree`. `issues.json` decides when it is there; else `report.md`.
pub fn read_result(worktree: &Path) -> Outcome {
    let directory = worktree.join(".moebius").join("grilling");
    match std::fs::read_to_string(directory.join("issues.json")) {
        Ok(text) => {
            let issues: Vec<GrillingIssue> = match serde_json::from_str(&text) {
                Ok(issues) => issues,
                Err(e) => {
                    return Outcome::Empty(format!("issues.json is not a list of issues: {e}"));
                }
            };
            return match creation_order(&issues) {
                Ok(_) => Outcome::Issues(issues),
                Err(why) => Outcome::Empty(format!("issues.json cannot be used: {why}")),
            };
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Outcome::Empty(format!("cannot read issues.json: {e}")),
    }
    match std::fs::read_to_string(directory.join("report.md")) {
        Ok(report) if !report.trim().is_empty() => Outcome::Report(report),
        Ok(_) => Outcome::Empty("report.md is empty".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Outcome::Empty("the agent left neither issues.json nor report.md".into())
        }
        Err(e) => Outcome::Empty(format!("cannot read report.md: {e}")),
    }
}

/// Stores what the ended grilling chat `run_id` left in `worktree`, puts it in the inbox when it holds issues or a
/// report, and publishes `grilling_ended`. Nothing happens when the person published what the chat left already
/// (see [`publish`]); [`crate::harness::run::end`] keeps the published result then.
pub fn conclude(app: &App, run_id: i64, worktree: &Path) -> Result<Grilling> {
    let run = app.run(run_id)?;
    let repository_id = run
        .repository_id
        .ok_or_else(|| AppError::Invalid(format!("grilling {run_id} has no repository")))?;
    let grilling = store_result(app, run_id, repository_id, read_result(worktree), None)?;
    publish_ended(app, &grilling);
    Ok(grilling)
}

/// Reads what an open grilling chat left in `worktree` and stores it the way [`conclude`] would, while the chat goes
/// on. A later publish replaces what was stored, until it is `confirmed` or `dismissed`. Refuses to publish when
/// nothing usable is in the worktree yet (the reason why comes with the error), or to replace a grilling the person
/// has settled. Publishes `grilling_published`.
pub fn publish(app: &App, run_id: i64, worktree: &Path) -> Result<Grilling> {
    let run = app.run(run_id)?;
    let repository_id = run
        .repository_id
        .ok_or_else(|| AppError::Invalid(format!("grilling {run_id} has no repository")))?;
    let previous = app.store.grilling_for_run(run_id)?;
    if let Some(previous) = &previous
        && previous.status != "proposed"
        && previous.status != "reported"
    {
        return Err(AppError::Invalid(format!(
            "what the grilling left was {} already: end the chat when you are done with it, or start a new grilling",
            previous.status
        )));
    }
    let grilling = match read_result(worktree) {
        Outcome::Empty(reason) => {
            return Err(AppError::Invalid(format!("nothing to publish: {reason}")));
        }
        outcome => store_result(app, run_id, repository_id, outcome, previous.as_ref())?,
    };
    app.bus.publish(Event::GrillingPublished {
        grilling: grilling.id,
        status: grilling.status.clone(),
    });
    Ok(grilling)
}

/// Stores issues or a report as a grilling with its inbox item, in place of the one the open chat left when
/// `replace` holds it. [`conclude`] stores what is not usable as an `empty` grilling; [`publish`] refuses it.
fn store_result(
    app: &App,
    run_id: i64,
    repository_id: i64,
    outcome: Outcome,
    replace: Option<&Grilling>,
) -> Result<Grilling> {
    let replace_id = replace.map(|grilling| grilling.id);
    match outcome {
        Outcome::Issues(issues) => {
            let issues: Vec<GrillingIssue> = issues.into_iter().map(redacted).collect();
            let grilling = stored_grilling(
                app,
                run_id,
                repository_id,
                replace_id,
                &issues,
                None,
                "proposed",
            )?;
            let title = match issues.len() {
                1 => "1 issue from a grilling".to_string(),
                n => format!("{n} issues from a grilling"),
            };
            let data =
                json!({ "grilling": grilling.id, "repository": repository_id, "issues": issues });
            put_into_inbox(app, grilling.id, "grilling_issues", &title, &data)?;
            Ok(grilling)
        }
        Outcome::Report(report) => {
            let report = redact(&report);
            let grilling = stored_grilling(
                app,
                run_id,
                repository_id,
                replace_id,
                &[],
                Some(&report),
                "reported",
            )?;
            let data =
                json!({ "grilling": grilling.id, "repository": repository_id, "report": report });
            put_into_inbox(
                app,
                grilling.id,
                "grilling_report",
                "A research report from a grilling",
                &data,
            )?;
            Ok(grilling)
        }
        Outcome::Empty(reason) => app
            .store
            .add_grilling(run_id, repository_id, &[], None, "empty", &reason)
            .map_err(From::from),
    }
}

/// What the grilling of a run is stored as: the grilling that was published before, replaced, or a fresh one.
fn stored_grilling(
    app: &App,
    run_id: i64,
    repository_id: i64,
    replace_id: Option<i64>,
    issues: &[GrillingIssue],
    report: Option<&str>,
    status: &str,
) -> Result<Grilling> {
    let id = match replace_id {
        Some(id) => {
            app.store
                .update_grilling_result(id, issues, report, status)?;
            id
        }
        None => {
            app.store
                .add_grilling(run_id, repository_id, issues, report, status, "")?
                .id
        }
    };
    Ok(app
        .store
        .grilling(id)?
        .expect("the grilling was just stored"))
}

/// Puts the grilling's `kind`, `title` and `data` into the inbox, over the item it had, else fresh: the first
/// publish adds, a later publish replaces what the open chat published before.
fn put_into_inbox(
    app: &App,
    grilling_id: i64,
    kind: &str,
    title: &str,
    data: &serde_json::Value,
) -> Result<()> {
    match app.store.inbox_for_grilling(grilling_id)? {
        Some(item) => Ok(app.store.update_inbox_item(item.id, kind, title, data)?),
        None => {
            app.store.add_inbox_item(kind, None, title, data)?;
            Ok(())
        }
    }
}

fn redacted(issue: GrillingIssue) -> GrillingIssue {
    GrillingIssue {
        title: redact(&issue.title),
        body: redact(&issue.body),
        blocked_by: issue.blocked_by,
    }
}

fn publish_ended(app: &App, grilling: &Grilling) {
    app.bus.publish(Event::GrillingEnded {
        grilling: grilling.id,
        status: grilling.status.clone(),
    });
}

/// The grilling `id`.
pub fn view(app: &App, id: i64) -> Result<Grilling> {
    app.store
        .grilling(id)?
        .ok_or_else(|| AppError::NotFound(format!("grilling {id}")))
}

/// A person confirms the grilling `id` with `issues`, the list as they left it after editing, dropping and
/// reordering: the decision is recorded, a card is made for each issue in the repository's Backlog (after opening it
/// on GitHub, where the repository is), and the inbox item is resolved. A failure leaves the grilling to confirm again.
pub fn confirm(app: &App, by: &Actor, id: i64, issues: Vec<GrillingIssue>) -> Result<Grilling> {
    let grilling = view(app, id)?;
    let order = creation_order(&issues).map_err(AppError::Invalid)?;
    if !app.store.move_grilling(id, &["proposed"], "confirmed")? {
        return Err(AppError::Invalid(format!(
            "grilling {id} is {}, not waiting for its issues to be confirmed",
            grilling.status
        )));
    }
    let made = (|| {
        let repository = app.repository(grilling.repository_id)?;
        let mut numbers = vec![0; issues.len()];
        for index in order {
            let issue = &issues[index];
            let blocked_by: BTreeSet<i64> = issue.blocked_by.iter().map(|b| numbers[*b]).collect();
            let issue = redacted(issue.clone());
            let card =
                app.create_confirmed_issue(&repository, &issue.title, &issue.body, &blocked_by)?;
            numbers[index] = card.number;
        }
        Ok::<_, AppError>(())
    })();
    if let Err(e) = made {
        app.store.move_grilling(id, &["confirmed"], "proposed")?;
        return Err(e);
    }
    app.decisions.record_by(
        by,
        &Question {
            kind: "confirm_grilling".into(),
            context: [
                ("grilling".to_string(), id.to_string()),
                (
                    "issues".to_string(),
                    serde_json::to_string(&issues).map_err(crate::store::StoreError::from)?,
                ),
            ]
            .into(),
            options: vec!["confirm_grilling".into(), "leave".into()],
            fail_closed: "leave".into(),
            run_id: Some(grilling.run_id),
            card_id: None,
        },
        "confirm_grilling",
        &format!("{PERSON} confirmed {} issues", issues.len()),
    )?;
    app.store.resolve_inbox_for_grilling(id)?;
    view(app, id)
}

/// A person dismisses a grilling's proposed issues or report; nothing is made.
pub fn dismiss(app: &App, id: i64) -> Result<Grilling> {
    let grilling = view(app, id)?;
    if !app
        .store
        .move_grilling(id, &["proposed", "reported"], "dismissed")?
    {
        return Err(AppError::Invalid(format!(
            "grilling {id} is {}, so there is nothing to dismiss",
            grilling.status
        )));
    }
    app.store.resolve_inbox_for_grilling(id)?;
    view(app, id)
}

/// A person deletes a grilling: its record, its chat (stopped first if it were going), and the chat's worktree and
/// branch. The issues it filed stay on the forge and as cards. Recorded as a `delete_grilling` decision and announced
/// as `grilling_deleted` and `chat_deleted`.
pub fn delete(
    app: &App,
    by: &Actor,
    runs: &dyn crate::board::Runs,
    id: i64,
    reason: &str,
) -> Result<Grilling> {
    let grilling = view(app, id)?;
    let (_, removed) = app.remove_chat(runs, grilling.run_id)?;
    let context = [
        ("grilling".to_string(), id.to_string()),
        ("chat".to_string(), grilling.run_id.to_string()),
        ("status".to_string(), grilling.status.clone()),
        ("inbox_items".to_string(), removed.inbox_items.to_string()),
    ]
    .into();
    app.record_action(by, "delete_grilling", None, None, context, reason)?;
    app.announce_chat_deleted(grilling.run_id, &removed);
    Ok(grilling)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(title: &str, blocked_by: &[usize]) -> GrillingIssue {
        GrillingIssue {
            title: title.into(),
            body: String::new(),
            blocked_by: blocked_by.to_vec(),
        }
    }

    /// A worktree whose `.moebius/grilling` holds the given files.
    fn worktree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let grilling = dir.path().join(".moebius/grilling");
        std::fs::create_dir_all(&grilling).unwrap();
        for (name, text) in files {
            std::fs::write(grilling.join(name), text).unwrap();
        }
        dir
    }

    #[test]
    fn the_result_folder_is_ignored_by_git_in_the_worktree() {
        use crate::repos::git;

        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::create_dir_all(dir.path().join(".moebius/grilling")).unwrap();
        std::fs::write(dir.path().join(".moebius/grilling/report.md"), "x").unwrap();
        assert!(
            git(dir.path(), &["status", "--porcelain"])
                .unwrap()
                .contains(".moebius")
        );
        ignore_result_folder(dir.path()).unwrap();
        ignore_result_folder(dir.path()).unwrap();
        assert_eq!(git(dir.path(), &["status", "--porcelain"]).unwrap(), "");
        let exclude = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.matches(".moebius/").count(), 1);
    }

    #[test]
    fn issues_are_read_from_issues_json() {
        let dir = worktree(&[(
            "issues.json",
            r#"[{"title": "First", "body": "Do it."}, {"title": "Second", "body": "", "blocked_by": [0]}]"#,
        )]);
        let Outcome::Issues(issues) = read_result(dir.path()) else {
            panic!("issues expected");
        };
        assert_eq!(
            issues,
            [
                GrillingIssue {
                    title: "First".into(),
                    body: "Do it.".into(),
                    blocked_by: vec![]
                },
                issue("Second", &[0])
            ]
        );
    }

    #[test]
    fn a_report_is_read_from_report_md() {
        let dir = worktree(&[("report.md", "# Findings\n\nIt depends.\n")]);
        assert_eq!(
            read_result(dir.path()),
            Outcome::Report("# Findings\n\nIt depends.\n".into())
        );
    }

    #[test]
    fn neither_file_is_an_empty_result_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Empty(reason) = read_result(dir.path()) else {
            panic!("empty expected");
        };
        assert!(reason.contains("neither"), "{reason}");
        let blank = worktree(&[("report.md", " \n")]);
        assert!(matches!(read_result(blank.path()), Outcome::Empty(_)));
    }

    #[test]
    fn broken_issues_json_is_empty_even_when_there_is_a_report() {
        let dir = worktree(&[("issues.json", "[{\"title\": "), ("report.md", "Text")]);
        let Outcome::Empty(reason) = read_result(dir.path()) else {
            panic!("empty expected");
        };
        assert!(reason.contains("issues.json"), "{reason}");
        let wrong = worktree(&[("issues.json", r#"{"title": "Not a list"}"#)]);
        assert!(matches!(read_result(wrong.path()), Outcome::Empty(_)));
        let untitled = worktree(&[("issues.json", r#"[{"body": "No title"}]"#)]);
        assert!(matches!(read_result(untitled.path()), Outcome::Empty(_)));
    }

    #[test]
    fn a_blocked_by_out_of_range_makes_the_result_empty() {
        let dir = worktree(&[("issues.json", r#"[{"title": "Only", "blocked_by": [3]}]"#)]);
        let Outcome::Empty(reason) = read_result(dir.path()) else {
            panic!("empty expected");
        };
        assert!(reason.contains("blocked by 3"), "{reason}");
    }

    #[test]
    fn issues_are_created_after_the_issues_that_block_them() {
        let issues = [issue("A", &[2]), issue("B", &[]), issue("C", &[1])];
        assert_eq!(creation_order(&issues).unwrap(), [1, 2, 0]);
        let itself = [issue("A", &[0])];
        assert!(creation_order(&itself).unwrap_err().contains("itself"));
        let circle = [issue("A", &[1]), issue("B", &[0])];
        assert!(creation_order(&circle).unwrap_err().contains("circle"));
        assert!(creation_order(&[issue(" ", &[])]).is_err());
        assert!(creation_order(&[]).is_err());
    }
}
