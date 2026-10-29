//! The poll of GitHub (ADR 0003: no webhooks). Every repository with the GitHub forge is brought up to date in turn:
//! its clone fetches `origin`, its issues become and update cards, and the pull requests of its open changes are
//! read. A repository that fails is reported on the bus and tried again at the next poll.

use std::sync::Arc;

use serde_json::json;

use crate::app::App;
use crate::bus::AppEvent;
use crate::repos::github::{CheckRun, GitHub};
use crate::store::Repository;

/// Polls every GitHub repository once.
pub async fn poll_github(app: &Arc<App>, github: &GitHub) {
    let Ok(repositories) = app.store.repositories() else {
        return;
    };
    for repository in repositories {
        let Some(name) = repository.github.clone() else {
            continue;
        };
        if let Err(message) = poll_repository(app, github, repository, &name).await {
            app.bus.publish(AppEvent::new(
                "forge_error",
                json!({ "repository": name, "message": message }),
            ));
        }
    }
}

async fn poll_repository(
    app: &Arc<App>,
    github: &GitHub,
    repository: Repository,
    name: &str,
) -> Result<(), String> {
    let (refresh_app, refresh_repository) = (app.clone(), repository.clone());
    blocking(move || refresh_app.refresh(&refresh_repository)).await?;
    poll_issues(app, github, repository.clone(), name).await?;
    poll_pull_requests(app, github, &repository, name).await
}

/// Runs a blocking call on the core off the async threads.
async fn blocking<T, E>(f: impl FnOnce() -> Result<T, E> + Send + 'static) -> Result<T, String>
where
    T: Send + 'static,
    E: ToString + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Lists the issues of `repository` on GitHub as `name` and syncs them into the board, unless nothing changed.
async fn poll_issues(
    app: &Arc<App>,
    github: &GitHub,
    repository: Repository,
    name: &str,
) -> Result<(), String> {
    let Some(issues) = github.issues(name).await.map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let app = app.clone();
    // On a failure GitHub would answer "not modified" next time; forgetting the ETag lists the issues again.
    blocking(move || app.sync_issues(&repository, &issues))
        .await
        .inspect_err(|_| github.forget_issues_etag(name))
}

/// Reads the pull request of each open change of `repository` on GitHub as `name`: a merged one finishes its change,
/// one closed without a merge closes it, and for any other the checks and the latest review are recorded.
async fn poll_pull_requests(
    app: &Arc<App>,
    github: &GitHub,
    repository: &Repository,
    name: &str,
) -> Result<(), String> {
    let (store, id) = (app.store.clone(), repository.id);
    let changes = blocking(move || store.open_pull_requests(id)).await?;
    for change in changes {
        let Some(number) = change.pull_request else {
            continue;
        };
        let pull_request = github
            .pull_request(name, number)
            .await
            .map_err(|e| e.to_string())?;
        let app = app.clone();
        if pull_request.merged {
            let commit = pull_request.merge_commit_sha;
            blocking(move || app.change_merged(&change, commit.as_deref())).await?;
        } else if pull_request.state == "closed" {
            blocking(move || app.change_closed(&change)).await?;
        } else {
            let checks = github
                .check_runs(name, &pull_request.head.sha)
                .await
                .map_err(|e| e.to_string())?;
            let review = github
                .reviews(name, number)
                .await
                .map_err(|e| e.to_string())?
                .pop()
                .map(|review| review.state.to_lowercase());
            let (summary, failed) = (summarise_checks(&checks), failed_checks(&checks));
            blocking(move || {
                app.record_change_status(
                    &change,
                    summary,
                    &pull_request.head.sha,
                    failed.as_deref(),
                    review.as_deref(),
                )
            })
            .await?;
        }
    }
    Ok(())
}

/// The conclusions that make a check run a failure.
const FAILED: [&str; 5] = [
    "failure",
    "timed_out",
    "cancelled",
    "action_required",
    "startup_failure",
];

fn failed(run: &CheckRun) -> bool {
    run.conclusion
        .as_deref()
        .is_some_and(|c| FAILED.contains(&c))
}

/// How the check runs of a commit add up: `failure` if any failed, `pending` if any is still going, `success`
/// otherwise; none when there are no checks at all.
fn summarise_checks(runs: &[CheckRun]) -> Option<&'static str> {
    if runs.is_empty() {
        return None;
    }
    if runs.iter().any(failed) {
        return Some("failure");
    }
    if runs.iter().any(|run| run.status != "completed") {
        return Some("pending");
    }
    Some("success")
}

/// The failed check runs, one per line: name, how it ended, what it says and where to read more.
fn failed_checks(runs: &[CheckRun]) -> Option<String> {
    let lines: Vec<String> = runs
        .iter()
        .filter(|run| failed(run))
        .map(|run| {
            let mut line = format!("{}: {}", run.name, run.conclusion.as_deref().unwrap_or(""));
            if let Some(title) = run.output.as_ref().and_then(|o| o.title.as_deref()) {
                line.push_str(&format!(", {title}"));
            }
            if let Some(url) = &run.details_url {
                line.push_str(&format!(" ({url})"));
            }
            line
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}
