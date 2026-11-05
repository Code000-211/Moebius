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
    /// When the issue last changed, in the form GitLab always writes it, so that it compares as text.
    #[serde(default)]
    pub(crate) updated_at: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    /// How many comments (notes written by people) the issue has.
    #[serde(default)]
    pub user_notes_count: i64,
    #[serde(default)]
    pub project_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct User {
    pub username: String,
}

/// A comment on an issue (a note) as GitLab lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Note {
    pub id: i64,
    pub body: String,
    pub author: User,
    pub created_at: String,
    /// Set for what GitLab wrote itself, such as "changed the description".
    #[serde(default)]
    pub system: bool,
}

/// An issue linked to another, as the links of an issue list it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IssueLink {
    pub iid: i64,
    pub project_id: i64,
    pub issue_link_id: i64,
    /// `relates_to`, `blocks` or `is_blocked_by`.
    pub link_type: String,
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
    /// The `updated_at` of the newest issue in the last full answer, per project: the next poll stops paging once it
    /// reaches issues older than that.
    newest: Mutex<HashMap<String, String>>,
    /// The issues blocking each issue, as last read.
    pub(crate) blockers: crate::repos::forge::Remembered,
    /// The id GitLab gives each project, as the issues list showed it: a link names the project of its issue by it.
    projects: Mutex<HashMap<String, i64>>,
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
            newest: Mutex::new(HashMap::new()),
            blockers: Default::default(),
            projects: Mutex::new(HashMap::new()),
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
    /// in pages of a hundred, newest change first, which are followed until a page ends with an issue older than the
    /// newest one of the last answer: once an answer is known, the list holds only the issues updated since, not all
    /// of them.
    pub async fn issues(&self, project: &str) -> Result<Option<Vec<Issue>>, GitLabError> {
        let known = self.etags().get(project).cloned();
        let known_newest = self.newest().get(project).cloned();
        let mut url = format!(
            "{}/issues?state=all&per_page=100&order_by=updated_at&sort=desc",
            self.project_url(project)
        );
        let mut issues = Vec::new();
        let mut etag = None;
        let mut newest: Option<String> = None;
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
            if newest.is_none() {
                newest = page.first().and_then(|i| i.updated_at.clone());
            }
            // Newest-updated first: past the newest issue of the last poll, nothing has changed.
            let reached_known = match (
                &known_newest,
                page.last().and_then(|i| i.updated_at.as_ref()),
            ) {
                (Some(known), Some(last)) => last < known,
                _ => false,
            };
            issues.extend(page);
            match next {
                Some(next) if !reached_known => url = next,
                _ => break,
            }
        }
        if let Some(etag) = etag.filter(|e| !e.is_empty()) {
            self.etags().insert(project.to_string(), etag);
            if let Some(newest) = newest {
                self.newest().insert(project.to_string(), newest);
            }
        }
        Ok(Some(issues))
    }

    /// Remembers that GitLab calls `project` by the id `id`.
    pub(crate) fn remember_project(&self, project: &str, id: i64) {
        self.projects
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(project.to_string(), id);
    }

    /// The id GitLab gives `project`, once an issues list has shown it.
    pub(crate) fn project_id(&self, project: &str) -> Option<i64> {
        self.projects
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(project)
            .copied()
    }

    /// Forgets the ETag of `project`, so the next call to [`GitLab::issues`] lists the issues again.
    pub fn forget_issues_etag(&self, project: &str) {
        self.etags().remove(project);
        self.newest().remove(project);
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

    /// Closes issue `number` in `project`, or reopens it.
    pub async fn set_issue_state(
        &self,
        project: &str,
        number: i64,
        open: bool,
    ) -> Result<(), GitLabError> {
        let request = json!({ "state_event": if open { "reopen" } else { "close" } });
        self.project_do(
            project,
            Method::PUT,
            &format!("/issues/{number}"),
            Some(&request),
        )
        .await
    }

    /// Replaces the labels of issue `number` in `project`.
    pub async fn set_issue_labels(
        &self,
        project: &str,
        number: i64,
        labels: &[String],
    ) -> Result<(), GitLabError> {
        let request = json!({ "labels": labels.join(",") });
        self.project_do(
            project,
            Method::PUT,
            &format!("/issues/{number}"),
            Some(&request),
        )
        .await
    }

    /// Writes a comment on issue `number` in `project` and returns its id.
    pub async fn create_note(
        &self,
        project: &str,
        number: i64,
        body: &str,
    ) -> Result<i64, GitLabError> {
        #[derive(Deserialize)]
        struct Identified {
            id: i64,
        }
        let request = json!({ "body": body });
        let created: Identified = self
            .project_call(
                project,
                Method::POST,
                &format!("/issues/{number}/notes"),
                Some(&request),
            )
            .await?;
        Ok(created.id)
    }

    /// The comments of issue `number` in `project` that people wrote, oldest first.
    pub async fn notes(&self, project: &str, number: i64) -> Result<Vec<Note>, GitLabError> {
        let mut url = format!(
            "{}/issues/{number}/notes?sort=asc&order_by=created_at&per_page=100",
            self.project_url(project)
        );
        let mut notes = Vec::new();
        loop {
            let response =
                Self::succeeded(self.authorize(self.http.get(&url)).send().await?).await?;
            let next = self.next_page(response.headers());
            notes.extend(response.json::<Vec<Note>>().await?);
            match next {
                Some(next) => url = next,
                None => break,
            }
        }
        notes.retain(|note| !note.system);
        Ok(notes)
    }

    /// The links of issue `number` in `project`.
    pub async fn issue_links(
        &self,
        project: &str,
        number: i64,
    ) -> Result<Vec<IssueLink>, GitLabError> {
        self.project_call(
            project,
            Method::GET,
            &format!("/issues/{number}/links"),
            None,
        )
        .await
    }

    /// Makes issue `blocker` of `project` block issue `number`. Says whether GitLab holds it as blocking: without
    /// Premium it links the issues as `relates_to` whatever is asked, which blocks nothing.
    pub async fn add_blocker(
        &self,
        project: &str,
        number: i64,
        blocker: i64,
    ) -> Result<bool, GitLabError> {
        #[derive(Deserialize)]
        struct Linked {
            link_type: String,
        }
        let request = json!({
            "target_project_id": project,
            "target_issue_iid": blocker,
            "link_type": "is_blocked_by",
        });
        let linked: Linked = self
            .project_call(
                project,
                Method::POST,
                &format!("/issues/{number}/links"),
                Some(&request),
            )
            .await?;
        Ok(linked.link_type == "is_blocked_by")
    }

    /// Stops issue `blocker` of `project` from blocking issue `number`. Says whether there was a link that blocked.
    pub async fn remove_blocker(
        &self,
        project: &str,
        number: i64,
        blocker: i64,
    ) -> Result<bool, GitLabError> {
        let links = self.issue_links(project, number).await?;
        let Some(link) = links
            .iter()
            .find(|l| l.iid == blocker && l.link_type == "is_blocked_by")
        else {
            return Ok(false);
        };
        self.project_do(
            project,
            Method::DELETE,
            &format!("/issues/{number}/links/{}", link.issue_link_id),
            None,
        )
        .await?;
        Ok(true)
    }

    /// The description of issue `number` in `project` as it is now.
    pub async fn issue_description(
        &self,
        project: &str,
        number: i64,
    ) -> Result<String, GitLabError> {
        #[derive(Deserialize)]
        struct Described {
            description: Option<String>,
        }
        let issue: Described = self
            .project_call(project, Method::GET, &format!("/issues/{number}"), None)
            .await?;
        Ok(issue.description.unwrap_or_default())
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

    /// Calls the API at `/projects/{project}{path}` and ignores the answer's body.
    async fn project_do(
        &self,
        project: &str,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<(), GitLabError> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.project_url(project)));
        if let Some(body) = body {
            request = request.json(body);
        }
        Self::succeeded(self.authorize(request).send().await?).await?;
        Ok(())
    }

    fn etags(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.etags
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn newest(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.newest
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
        /// Requests for a later page of issues.
        later_pages: std::sync::atomic::AtomicUsize,
        /// Whether an old issue has changed since the first answer, which then lists it first.
        changed: std::sync::atomic::AtomicBool,
        /// Whether the first page holds only issues newer than the first answer's newest, with more after them.
        busy: std::sync::atomic::AtomicBool,
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
                        assert_eq!(
                            (query["order_by"].as_str(), query["sort"].as_str()),
                            ("updated_at", "desc"),
                            "the newest change comes first, so the first page's ETag sees every change"
                        );
                        let issue = |iid: u64, updated_at: &str| {
                            json!({ "iid": iid, "title": "Issue", "description": null,
                                    "state": "opened", "author": { "username": "leon" },
                                    "updated_at": updated_at })
                        };
                        if query.contains_key("page") {
                            fake.later_pages.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            return axum::Json(json!([issue(3, "2026-01-01T00:00:00.000Z")]))
                                .into_response();
                        }
                        let asked = headers
                            .get("if-none-match")
                            .map(|v| v.to_str().unwrap().to_string());
                        fake.asked.lock().unwrap().push((project, asked.clone()));
                        let host = headers.get("host").unwrap().to_str().unwrap();
                        let next = format!(
                            "<http://{host}/api/v4/projects/acme%2Ftools%2Fwidgets/issues?state=all&per_page=100&order_by=updated_at&sort=desc&page=2>; rel=\"next\", \
                             <http://elsewhere/api/v4/x>; rel=\"last\""
                        );
                        if fake.busy.load(std::sync::atomic::Ordering::SeqCst) {
                            // Every issue of the first page is newer than the last poll's newest: more may follow.
                            return (
                                [("etag", "W/\"v3\"".to_string()), ("link", next)],
                                axum::Json(json!([
                                    issue(5, "2026-07-01T00:00:00.000Z"),
                                    issue(4, "2026-06-15T00:00:00.000Z")
                                ])),
                            )
                                .into_response();
                        }
                        if fake.changed.load(std::sync::atomic::Ordering::SeqCst) {
                            // Issue 3, which was on the last page, changed and is first now.
                            return (
                                [("etag", "W/\"v2\"".to_string()), ("link", next)],
                                axum::Json(json!([
                                    issue(3, "2026-06-01T00:00:00.000Z"),
                                    issue(1, "2026-02-01T00:00:00.000Z"),
                                    issue(9, "2026-01-02T00:00:00.000Z")
                                ])),
                            )
                                .into_response();
                        }
                        if asked.as_deref() == Some("W/\"v1\"") {
                            return StatusCode::NOT_MODIFIED.into_response();
                        }
                        (
                            [("etag", "W/\"v1\"".to_string()), ("link", next)],
                            axum::Json(json!([issue(1, "2026-02-01T00:00:00.000Z")])),
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

        assert_eq!(
            fake.later_pages.load(std::sync::atomic::Ordering::SeqCst),
            2
        );

        // An issue from the last page changes: the pages that hold only issues older than the newest one of the
        // last poll are not read again.
        fake.changed
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let issues = gitlab.issues("acme/tools/widgets").await.unwrap().unwrap();
        assert_eq!(issues.iter().map(|i| i.iid).collect::<Vec<_>>(), [3, 1, 9]);
        assert_eq!(
            fake.later_pages.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "the first page ends with an issue older than the last poll's newest, so the next page is not asked for"
        );

        // The first page still ends with an issue newer than that: the next page may hold changes too.
        fake.busy.store(true, std::sync::atomic::Ordering::SeqCst);
        let issues = gitlab.issues("acme/tools/widgets").await.unwrap().unwrap();
        assert_eq!(issues.iter().map(|i| i.iid).collect::<Vec<_>>(), [5, 4, 3]);
        assert_eq!(
            fake.later_pages.load(std::sync::atomic::Ordering::SeqCst),
            3
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
