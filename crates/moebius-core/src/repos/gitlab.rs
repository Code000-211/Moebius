//! The GitLab client: gitlab.com or a GitLab hosted elsewhere, reached with an access token (personal, project or
//! group) sent as `PRIVATE-TOKEN`. It lists a project's issues, asking only for what changed since the last time where
//! GitLab answers with an ETag, opens and edits issues, and opens, reads and merges merge requests, with the pipelines
//! and approvals of each. A project is named by its path, such as `group/subgroup/project`.

use std::collections::HashMap;
use std::sync::Mutex;

use reqwest::Method;
use serde::Deserialize;
use serde_json::json;

/// Why talking to GitLab failed.
#[derive(Debug, thiserror::Error)]
pub enum GitLabError {
    #[error("cannot reach GitLab: {0}")]
    Http(#[from] reqwest::Error),
    #[error("GitLab answered {status} to {url}: {body}")]
    Status {
        url: String,
        status: reqwest::StatusCode,
        body: String,
    },
}

/// An issue as GitLab lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Issue {
    /// The issue's number in its project.
    pub iid: i64,
    pub title: String,
    pub description: Option<String>,
    /// `opened` or `closed`.
    pub state: String,
    pub author: User,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct User {
    pub username: String,
}

/// A merge request as GitLab describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MergeRequest {
    /// `opened`, `closed`, `locked` or `merged`.
    pub state: String,
    /// The commit at the head of the source branch.
    pub sha: String,
    pub merge_commit_sha: Option<String>,
    pub squash_commit_sha: Option<String>,
}

impl MergeRequest {
    /// The commit the merge left on the target branch: the merge commit, the squashed commit, or for a fast-forward
    /// the head itself.
    pub fn merged_commit(&self) -> String {
        self.merge_commit_sha
            .clone()
            .or_else(|| self.squash_commit_sha.clone())
            .unwrap_or_else(|| self.sha.clone())
    }
}

/// A pipeline of a merge request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pipeline {
    pub id: i64,
    pub sha: String,
    /// Such as `running`, `success`, `failed`, `canceled` or `skipped`.
    pub status: String,
    pub web_url: Option<String>,
}

/// A job of a pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Job {
    pub name: String,
    pub status: String,
    pub web_url: Option<String>,
    /// Why a failed job failed, such as `script_failure`.
    pub failure_reason: Option<String>,
    /// Whether the pipeline passes even when this job fails.
    #[serde(default)]
    pub allow_failure: bool,
}

/// Who approved a merge request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Approvals {
    #[serde(default)]
    pub approved_by: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Created {
    iid: i64,
}

/// A client for one GitLab's API, authenticated with one access token.
pub struct GitLab {
    http: reqwest::Client,
    /// The GitLab's web address, such as `https://gitlab.com`.
    url: String,
    api_url: String,
    token: String,
    /// The ETag of the last full answer to the issues list, per project.
    etags: Mutex<HashMap<String, String>>,
}

impl GitLab {
    /// A client for the GitLab at `url` (its web address; the API is at `<url>/api/v4`) that sends `token`.
    pub fn new(url: &str, token: &str) -> Self {
        let url = url.trim_end_matches('/').to_string();
        Self {
            http: reqwest::Client::new(),
            api_url: format!("{url}/api/v4"),
            url,
            token: token.trim().to_string(),
            etags: Mutex::new(HashMap::new()),
        }
    }

    /// The GitLab's web address, which git remotes on it start with.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The access token, for git to authenticate with.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The issues of `project`, open and closed; none when GitLab says nothing changed since the last call. They come
    /// in pages of a hundred, which are followed to the end.
    pub async fn issues(&self, project: &str) -> Result<Option<Vec<Issue>>, GitLabError> {
        let known = self.etags().get(project).cloned();
        let mut url = format!(
            "{}/issues?state=all&per_page=100",
            self.project_url(project)
        );
        let mut issues = Vec::new();
        let mut etag = None;
        loop {
            let mut request = self.http.get(&url);
            if let (Some(known), None) = (&known, &etag) {
                request = request.header("If-None-Match", known);
            }
            let response = self.authorize(request).send().await?;
            if response.status() == reqwest::StatusCode::NOT_MODIFIED {
                return Ok(None);
            }
            let response = Self::succeeded(response).await?;
            if etag.is_none() {
                etag = Some(
                    response
                        .headers()
                        .get("etag")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            let next = self.next_page(response.headers());
            let page: Vec<Issue> = response.json().await?;
            issues.extend(page);
            match next {
                Some(next) => url = next,
                None => break,
            }
        }
        if let Some(etag) = etag.filter(|e| !e.is_empty()) {
            self.etags().insert(project.to_string(), etag);
        }
        Ok(Some(issues))
    }

    /// Forgets the ETag of `project`, so the next call to [`GitLab::issues`] lists the issues again.
    pub fn forget_issues_etag(&self, project: &str) {
        self.etags().remove(project);
    }

    /// Opens an issue in `project` and returns its number.
    pub async fn create_issue(
        &self,
        project: &str,
        title: &str,
        body: &str,
    ) -> Result<i64, GitLabError> {
        let request = json!({ "title": title, "description": body });
        let created: Created = self
            .project_call(project, Method::POST, "/issues", Some(&request))
            .await?;
        Ok(created.iid)
    }

    /// Replaces the title and body of issue `number` in `project`.
    pub async fn update_issue(
        &self,
        project: &str,
        number: i64,
        title: &str,
        body: &str,
    ) -> Result<(), GitLabError> {
        let request = json!({ "title": title, "description": body });
        let _: Created = self
            .project_call(
                project,
                Method::PUT,
                &format!("/issues/{number}"),
                Some(&request),
            )
            .await?;
        Ok(())
    }

    /// Opens a merge request in `project` that asks for `source` to be merged into `target`, and returns its number.
    pub async fn open_merge_request(
        &self,
        project: &str,
        title: &str,
        source: &str,
        target: &str,
        body: &str,
    ) -> Result<i64, GitLabError> {
        let request = json!({ "title": title, "source_branch": source,
                              "target_branch": target, "description": body });
        let created: Created = self
            .project_call(project, Method::POST, "/merge_requests", Some(&request))
            .await?;
        Ok(created.iid)
    }

    pub async fn merge_request(
        &self,
        project: &str,
        number: i64,
    ) -> Result<MergeRequest, GitLabError> {
        self.project_call(
            project,
            Method::GET,
            &format!("/merge_requests/{number}"),
            None,
        )
        .await
    }

    /// The pipelines of a merge request, newest first.
    pub async fn pipelines(
        &self,
        project: &str,
        number: i64,
    ) -> Result<Vec<Pipeline>, GitLabError> {
        self.project_call(
            project,
            Method::GET,
            &format!("/merge_requests/{number}/pipelines?per_page=100"),
            None,
        )
        .await
    }

    /// The failed jobs of a pipeline.
    pub async fn failed_jobs(&self, project: &str, pipeline: i64) -> Result<Vec<Job>, GitLabError> {
        self.project_call(
            project,
            Method::GET,
            &format!("/pipelines/{pipeline}/jobs?scope[]=failed&per_page=100"),
            None,
        )
        .await
    }

    /// Who approved a merge request.
    pub async fn approvals(&self, project: &str, number: i64) -> Result<Approvals, GitLabError> {
        self.project_call(
            project,
            Method::GET,
            &format!("/merge_requests/{number}/approvals"),
            None,
        )
        .await
    }

    /// Merges a merge request with a merge commit titled `message` and returns the commit it left.
    pub async fn merge(
        &self,
        project: &str,
        number: i64,
        message: &str,
    ) -> Result<String, GitLabError> {
        let request = json!({ "merge_commit_message": message });
        let merged: MergeRequest = self
            .project_call(
                project,
                Method::PUT,
                &format!("/merge_requests/{number}/merge"),
                Some(&request),
            )
            .await?;
        Ok(merged.merged_commit())
    }

    /// The API's address of `project`, whose path is one segment there.
    fn project_url(&self, project: &str) -> String {
        format!("{}/projects/{}", self.api_url, project.replace('/', "%2F"))
    }

    /// Calls the API at `/projects/{project}{path}` and reads the JSON answer.
    async fn project_call<T: serde::de::DeserializeOwned>(
        &self,
        project: &str,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T, GitLabError> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.project_url(project)));
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = self.authorize(request).send().await?;
        Ok(Self::succeeded(response).await?.json().await?)
    }

    fn etags(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.etags
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The next page of a list, if the `Link` header names one on this API (the token is sent nowhere else).
    fn next_page(&self, headers: &reqwest::header::HeaderMap) -> Option<String> {
        let link = headers.get("link")?.to_str().ok()?;
        link.split(',')
            .find(|part| part.contains("rel=\"next\""))
            .and_then(|part| part.split_once('<')?.1.split_once('>'))
            .map(|(url, _)| url.to_string())
            .filter(|url| url.starts_with(&format!("{}/", self.api_url)))
    }

    fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
            .header("PRIVATE-TOKEN", &self.token)
            .header("User-Agent", "moebius")
    }

    /// The response if GitLab answered with a success, an error with GitLab's explanation otherwise.
    async fn succeeded(response: reqwest::Response) -> Result<reqwest::Response, GitLabError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        Err(GitLabError::Status {
            url: response.url().to_string(),
            status,
            body: response.text().await.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::extract::{Path, Query, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::get;

    use super::*;

    /// The `If-None-Match` of each request for the first page of issues, and the project each was for.
    #[derive(Default)]
    struct Fake {
        asked: Mutex<Vec<(String, Option<String>)>>,
    }

    async fn serve() -> (String, Arc<Fake>) {
        let fake = Arc::new(Fake::default());
        let app = Router::new()
            .route(
                "/api/v4/projects/{project}/issues",
                get(
                    |State(fake): State<Arc<Fake>>,
                     headers: HeaderMap,
                     Path(project): Path<String>,
                     Query(query): Query<HashMap<String, String>>| async move {
                        if headers.get("private-token").map(|v| v.as_bytes()) != Some(b"glpat-test") {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        assert_eq!((query["state"].as_str(), query["per_page"].as_str()), ("all", "100"));
                        let issue = |iid: u64| {
                            json!({ "iid": iid, "title": "Issue", "description": null,
                                    "state": "opened", "author": { "username": "leon" } })
                        };
                        if query.contains_key("page") {
                            return axum::Json(json!([issue(3)])).into_response();
                        }
                        let asked = headers
                            .get("if-none-match")
                            .map(|v| v.to_str().unwrap().to_string());
                        fake.asked.lock().unwrap().push((project, asked.clone()));
                        if asked.as_deref() == Some("W/\"v1\"") {
                            return StatusCode::NOT_MODIFIED.into_response();
                        }
                        let host = headers.get("host").unwrap().to_str().unwrap();
                        let next = format!(
                            "<http://{host}/api/v4/projects/acme%2Ftools%2Fwidgets/issues?state=all&per_page=100&page=2>; rel=\"next\", \
                             <http://elsewhere/api/v4/x>; rel=\"last\""
                        );
                        (
                            [("etag", "W/\"v1\"".to_string()), ("link", next)],
                            axum::Json(json!([issue(1)])),
                        )
                            .into_response()
                    },
                ),
            )
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        (url, fake)
    }

    #[tokio::test]
    async fn issues_are_listed_page_by_page_with_the_token_and_only_when_changed() {
        let (url, fake) = serve().await;
        let gitlab = GitLab::new(&format!("{url}/"), " glpat-test\n");
        assert_eq!(gitlab.url(), url);
        let issues = gitlab.issues("acme/tools/widgets").await.unwrap().unwrap();
        assert_eq!(
            issues.iter().map(|i| i.iid).collect::<Vec<_>>(),
            [1, 3],
            "the second page is followed"
        );
        assert_eq!(
            (
                issues[0].description.as_deref(),
                issues[0].author.username.as_str()
            ),
            (None, "leon")
        );

        assert_eq!(gitlab.issues("acme/tools/widgets").await.unwrap(), None);
        gitlab.forget_issues_etag("acme/tools/widgets");
        assert!(gitlab.issues("acme/tools/widgets").await.unwrap().is_some());
        let project = "acme/tools/widgets".to_string();
        assert_eq!(
            *fake.asked.lock().unwrap(),
            [
                (project.clone(), None),
                (project.clone(), Some("W/\"v1\"".to_string())),
                (project, None)
            ],
            "the project's path is one segment of the address"
        );

        let refused = GitLab::new(&url, "wrong").issues("acme/widgets").await;
        assert!(
            matches!(&refused, Err(GitLabError::Status { status, .. }) if *status == reqwest::StatusCode::UNAUTHORIZED),
            "{refused:?}"
        );
    }

    #[test]
    fn a_merge_leaves_the_merge_commit_the_squashed_one_or_the_head() {
        let request = |merge: Option<&str>, squash: Option<&str>| MergeRequest {
            state: "merged".into(),
            sha: "head".into(),
            merge_commit_sha: merge.map(Into::into),
            squash_commit_sha: squash.map(Into::into),
        };
        assert_eq!(request(Some("m"), Some("s")).merged_commit(), "m");
        assert_eq!(request(None, Some("s")).merged_commit(), "s");
        assert_eq!(request(None, None).merged_commit(), "head");
    }
}
