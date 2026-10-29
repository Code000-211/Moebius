//! What Forgejo answers, as the poll reads it: issues, and a pull request with the commit statuses of its head commit
//! and its latest review.

use super::{Polled, Request, Status};
use crate::repos::forge::{ForgeError, Issue};
use crate::repos::forgejo::{self, CommitStatus, Forgejo, Review};

impl From<forgejo::Issue> for Issue {
    fn from(issue: forgejo::Issue) -> Self {
        Self {
            number: issue.number,
            title: issue.title,
            body: issue.body.unwrap_or_default(),
            open: issue.state == "open",
            author: issue.user.login,
        }
    }
}

impl Polled for Forgejo {
    const FORGE: &'static str = "forgejo";

    async fn issues(&self, name: &str) -> Result<Option<Vec<Issue>>, ForgeError> {
        let issues = Forgejo::issues(self, name).await?;
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
        let head = pull_request.head.sha;
        let statuses = self.statuses(name, &head).await?.statuses;
        let reviews = self.reviews(name, number).await?;
        Ok(Request::Open(Status {
            checks: summarise(&statuses),
            failed: failed_checks(&statuses),
            review: latest_review(&reviews),
            head,
        }))
    }
}

/// Whether a status makes its commit fail.
fn failed(status: &CommitStatus) -> bool {
    matches!(status.status.as_str(), "failure" | "error")
}

/// How the statuses of a commit add up: `failure` if any failed or errored, `pending` if any is still going, `success`
/// otherwise (a `warning` passes); none when there are no statuses at all.
fn summarise(statuses: &[CommitStatus]) -> Option<&'static str> {
    if statuses.is_empty() {
        return None;
    }
    if statuses.iter().any(failed) {
        return Some("failure");
    }
    if statuses.iter().any(|s| s.status == "pending") {
        return Some("pending");
    }
    Some("success")
}

/// The failed statuses, one per line: context, how it ended, what it says and where to read more.
fn failed_checks(statuses: &[CommitStatus]) -> Option<String> {
    let lines: Vec<String> = statuses
        .iter()
        .filter(|s| failed(s))
        .map(|s| {
            let mut line = format!("{}: {}", s.context, s.status);
            if let Some(description) = s.description.as_deref().filter(|d| !d.is_empty()) {
                line.push_str(&format!(", {description}"));
            }
            if let Some(url) = s.target_url.as_deref().filter(|u| !u.is_empty()) {
                line.push_str(&format!(" ({url})"));
            }
            line
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The latest review given, in lower case as on GitHub: `approved`, `changes_requested` or `comment`. Pending reviews,
/// requests for a review and dismissed reviews do not count.
fn latest_review(reviews: &[Review]) -> Option<String> {
    reviews
        .iter()
        .rev()
        .filter(|r| !r.dismissed && !matches!(r.state.as_str(), "PENDING" | "REQUEST_REVIEW"))
        .map(|r| match r.state.as_str() {
            "REQUEST_CHANGES" => "changes_requested".to_string(),
            state => state.to_lowercase(),
        })
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(context: &str, status: &str) -> CommitStatus {
        CommitStatus {
            status: status.into(),
            context: context.into(),
            description: Some(format!("{context} says {status}")),
            target_url: Some(format!("https://codeberg/{context}")),
        }
    }

    #[test]
    fn statuses_read_as_checks_and_the_failed_ones_as_lines() {
        assert_eq!(summarise(&[]), None);
        assert_eq!(
            summarise(&[status("a", "success"), status("b", "warning")]),
            Some("success")
        );
        assert_eq!(
            summarise(&[status("a", "success"), status("b", "pending")]),
            Some("pending")
        );
        let statuses = [
            status("lint", "failure"),
            status("build", "pending"),
            status("deploy", "error"),
        ];
        assert_eq!(summarise(&statuses), Some("failure"));
        assert_eq!(
            failed_checks(&statuses).unwrap(),
            "lint: failure, lint says failure (https://codeberg/lint)\n\
             deploy: error, deploy says error (https://codeberg/deploy)"
        );
        assert_eq!(failed_checks(&[status("a", "success")]), None);
    }

    #[test]
    fn the_latest_review_given_counts() {
        let review = |state: &str, dismissed| Review {
            state: state.into(),
            dismissed,
        };
        assert_eq!(latest_review(&[]), None);
        assert_eq!(
            latest_review(&[
                review("REQUEST_CHANGES", false),
                review("APPROVED", true),
                review("PENDING", false),
                review("REQUEST_REVIEW", false),
            ])
            .as_deref(),
            Some("changes_requested")
        );
        assert_eq!(
            latest_review(&[review("REQUEST_CHANGES", false), review("APPROVED", false)])
                .as_deref(),
            Some("approved")
        );
    }
}
