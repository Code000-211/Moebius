//! What GitHub answers, as the poll reads it: issues without the pull requests GitHub lists among them, and a pull
//! request with the check runs of its head commit and its latest review.

use super::{Polled, Request, Status};
use crate::repos::forge::{ForgeError, Issue};
use crate::repos::github::{self, CheckRun, GitHub};

impl From<github::Issue> for Issue {
    fn from(issue: github::Issue) -> Self {
        Self {
            number: issue.number,
            title: issue.title,
            body: issue.body.unwrap_or_default(),
            open: issue.state == "open",
            author: issue.user.login,
        }
    }
}

impl Polled for GitHub {
    const FORGE: &'static str = "github";

    async fn issues(&self, name: &str) -> Result<Option<Vec<Issue>>, ForgeError> {
        let issues = GitHub::issues(self, name).await?;
        Ok(issues.map(|issues| issues.into_iter().map(Issue::from).collect()))
    }

    fn forget_issues(&self, name: &str) {
        self.forget_issues_etag(name);
    }

    async fn request(&self, name: &str, number: i64) -> Result<Request, ForgeError> {
        let pull_request = self.pull_request(name, number).await?;
        if pull_request.merged {
            return Ok(Request::Merged {
                commit: pull_request.merge_commit_sha,
            });
        }
        if pull_request.state == "closed" {
            return Ok(Request::Closed);
        }
        let checks = self.check_runs(name, &pull_request.head.sha).await?;
        let review = self
            .reviews(name, number)
            .await?
            .pop()
            .map(|review| review.state.to_lowercase());
        Ok(Request::Open(Status {
            head: pull_request.head.sha,
            checks: summarise_checks(&checks),
            failed: failed_checks(&checks),
            review,
        }))
    }
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
