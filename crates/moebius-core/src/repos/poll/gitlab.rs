//! What GitLab answers, as the poll reads it: issues, and a merge request with the latest pipeline of its head commit,
//! the jobs that failed in it, and whether anyone approved it.

use super::{Polled, Request, Status};
use crate::repos::forge::{ForgeError, Issue};
use crate::repos::gitlab::{self, GitLab, Job, Pipeline};

impl From<gitlab::Issue> for Issue {
    fn from(issue: gitlab::Issue) -> Self {
        Self {
            number: issue.iid,
            title: issue.title,
            body: issue.description.unwrap_or_default(),
            open: issue.state == "opened",
            author: issue.author.username,
        }
    }
}

impl Polled for GitLab {
    const FORGE: &'static str = "gitlab";

    async fn issues(&self, name: &str) -> Result<Option<Vec<Issue>>, ForgeError> {
        let issues = GitLab::issues(self, name).await?;
        Ok(issues.map(|issues| issues.into_iter().map(Issue::from).collect()))
    }

    fn forget_issues(&self, name: &str) {
        self.forget_issues_etag(name);
    }

    async fn request(&self, name: &str, number: i64) -> Result<Request, ForgeError> {
        let merge_request = self.merge_request(name, number).await?;
        match merge_request.state.as_str() {
            "merged" => {
                return Ok(Request::Merged {
                    commit: Some(merge_request.merged_commit()),
                });
            }
            "closed" => return Ok(Request::Closed),
            _ => {}
        }
        let head = merge_request.sha;
        let pipeline = self
            .pipelines(name, number)
            .await?
            .into_iter()
            .find(|pipeline| pipeline.sha == head);
        let checks = pipeline.as_ref().map(|p| summarise(&p.status));
        let failed = match &pipeline {
            Some(pipeline) if checks == Some("failure") => {
                let jobs = self.failed_jobs(name, pipeline.id).await?;
                Some(failed_checks(pipeline, &jobs))
            }
            _ => None,
        };
        let approvals = self.approvals(name, number).await?;
        Ok(Request::Open(Status {
            head,
            checks,
            failed,
            review: (!approvals.approved_by.is_empty()).then(|| "approved".to_string()),
        }))
    }
}

/// How a pipeline's status reads as checks: `failure` when it failed or was canceled, `success` when it passed or was
/// skipped, and `pending` while it is anything else, waiting for a person to start a manual job included.
fn summarise(status: &str) -> &'static str {
    match status {
        "failed" | "canceled" => "failure",
        "success" | "skipped" => "success",
        _ => "pending",
    }
}

/// The jobs of a failed pipeline that made it fail, one per line: name, how it ended, why and where to read more. A
/// pipeline that failed without a failed job, such as a canceled one, is a line of its own.
fn failed_checks(pipeline: &Pipeline, jobs: &[Job]) -> String {
    let line = |name: &str, status: &str, reason: Option<&str>, url: Option<&str>| {
        let mut line = format!("{name}: {status}");
        if let Some(reason) = reason {
            line.push_str(&format!(", {reason}"));
        }
        if let Some(url) = url {
            line.push_str(&format!(" ({url})"));
        }
        line
    };
    let lines: Vec<String> = jobs
        .iter()
        .filter(|job| !job.allow_failure)
        .map(|job| {
            line(
                &job.name,
                &job.status,
                job.failure_reason.as_deref(),
                job.web_url.as_deref(),
            )
        })
        .collect();
    if lines.is_empty() {
        return line(
            &format!("pipeline {}", pipeline.id),
            &pipeline.status,
            None,
            pipeline.web_url.as_deref(),
        );
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipeline_reads_as_checks_and_its_failed_jobs_as_lines() {
        assert_eq!(summarise("running"), "pending");
        assert_eq!(summarise("manual"), "pending");
        assert_eq!(summarise("skipped"), "success");
        assert_eq!(summarise("canceled"), "failure");
        let pipeline = Pipeline {
            id: 9,
            sha: "deadbeef".into(),
            status: "canceled".into(),
            web_url: Some("https://gitlab/p/9".into()),
        };
        let job = |name: &str, allow_failure| Job {
            name: name.into(),
            status: "failed".into(),
            web_url: Some(format!("https://gitlab/j/{name}")),
            failure_reason: Some("script_failure".into()),
            allow_failure,
        };
        assert_eq!(
            failed_checks(&pipeline, &[job("lint", false), job("flaky", true)]),
            "lint: failed, script_failure (https://gitlab/j/lint)"
        );
        assert_eq!(
            failed_checks(&pipeline, &[job("flaky", true)]),
            "pipeline 9: canceled (https://gitlab/p/9)"
        );
    }
}
