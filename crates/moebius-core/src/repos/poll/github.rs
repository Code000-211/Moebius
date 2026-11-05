//! What GitHub answers, as the poll reads it: issues without the pull requests GitHub lists among them, and a pull
//! request with the check runs of its head commit and its latest review.

use super::{Polled, Request, Status};
use crate::repos::forge::{Comment, ForgeError, Issue, Remembered, seconds};
use crate::repos::github::{self, CheckRun, GitHub};

impl From<github::Issue> for Issue {
    fn from(issue: github::Issue) -> Self {
        // GitHub says how many issues block an issue: none is known without a read, some are read by `blockers`.
        let total = issue
            .issue_dependencies_summary
            .as_ref()
            .map(|summary| summary.total_blocked_by);
        Self {
            number: issue.number,
            title: issue.title,
            body: issue.body.unwrap_or_default(),
            open: issue.state == "open",
            author: issue.user.login,
            labels: issue.labels.into_iter().map(|l| l.name).collect(),
            // `issues` reads it, which knows the repository it is in.
            parent: None,
            blocked_by: (total == Some(0)).then(Vec::new),
            comment_count: issue.comments,
            comments: None,
            updated: issue.updated_at.unwrap_or_default(),
            blockers_unread: total.is_some_and(|total| total > 0),
        }
    }
}

/// The number of the parent a sub-issue's `parent_issue_url` names, none when the parent is in another repository
/// than the sub-issue: its number means another issue here.
fn parent_number(url: &str, repository: &str) -> Option<i64> {
    let mut parts = url.rsplit('/');
    let number = parts.next()?.parse().ok()?;
    let (_issues, name, owner) = (parts.next()?, parts.next()?, parts.next()?);
    (format!("{owner}/{name}").eq_ignore_ascii_case(repository)).then_some(number)
}

impl Polled for GitHub {
    const FORGE: &'static str = "github";

    async fn issues(&self, name: &str) -> Result<Option<Vec<Issue>>, ForgeError> {
        let Some(issues) = GitHub::issues(self, name).await? else {
            return Ok(None);
        };
        Ok(Some(
            issues
                .into_iter()
                .map(|issue| {
                    let parent_url = issue.parent_issue_url.clone();
                    let mut issue = Issue::from(issue);
                    // A parent in another repository is not one of the issues here.
                    issue.parent = parent_url
                        .as_deref()
                        .and_then(|url| parent_number(url, name));
                    issue
                })
                .collect(),
        ))
    }

    fn remembered(&self) -> &Remembered {
        &self.blockers
    }

    async fn blockers(&self, name: &str, number: i64, updated: &str) -> Option<Vec<i64>> {
        if let Some(known) = self.blockers.get(name, number, updated) {
            return Some(known);
        }
        // A forge that cannot list them leaves the issue with the blockers its body names.
        let blockers = self.blocked_by(name, number).await.ok()?;
        self.blockers.put(name, number, updated, blockers.clone());
        Some(blockers)
    }

    async fn comments(&self, name: &str, number: i64) -> Result<Vec<Comment>, ForgeError> {
        Ok(self
            .issue_comments(name, number)
            .await?
            .into_iter()
            .map(|c| Comment {
                id: c.id,
                author: c.user.login,
                body: c.body,
                at: seconds(&c.created_at),
            })
            .collect())
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
