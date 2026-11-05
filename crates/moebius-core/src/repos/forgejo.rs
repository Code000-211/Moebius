//! The Forgejo client, for Codeberg or any Forgejo or Gitea, which share this API: reached with an access token sent as
//! `Authorization: token <token>`. It lists a repository's issues, asking only for what changed since the last time
//! where the forge answers with an ETag, opens and edits issues, and opens, reads and merges pull requests, with the
//! commit statuses of a head commit and the reviews of each. A repository is named `owner/repo`.

use std::collections::HashMap;
use std::sync::Mutex;

use reqwest::Method;
use serde::Deserialize;
use serde_json::json;

/// Why talking to Forgejo failed.
#[derive(Debug, thiserror::Error)]
pub enum ForgejoError {
    #[error("cannot reach Forgejo: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Forgejo answered {status} to {url}: {body}")]
    Status {
        url: String,
        status: reqwest::StatusCode,
        body: String,
    },
}

/// An issue as Forgejo lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Issue {
    pub number: i64,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    /// `open` or `closed`.
    pub state: String,
    pub user: User,
    /// Set when the issue is a pull request, which the list leaves out when asked to.
    #[serde(default)]
    pub pull_request: Option<serde_json::Value>,
    #[serde(default)]
    pub labels: Vec<Label>,
    /// How many comments the issue has.
    #[serde(default)]
    pub comments: i64,
    /// When the issue last changed, in the form Forgejo always writes it, so that it compares as text.
    #[serde(default)]
    pub(crate) updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Label {
    pub name: String,
}

/// A comment on an issue as Forgejo lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IssueComment {
    pub id: i64,
    pub user: User,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct User {
    pub login: String,
}

/// A pull request as Forgejo describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PullRequest {
    /// `open` or `closed`; a merged pull request is `closed` with `merged` set.
    pub state: String,
    #[serde(default)]
    pub merged: bool,
    pub merge_commit_sha: Option<String>,
    pub head: Head,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Head {
    pub sha: String,
}

/// The statuses of a commit, the latest of each context, as Forgejo Actions and other CI report them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CombinedStatus {
    #[serde(default)]
    pub statuses: Vec<CommitStatus>,
}

/// One status of a commit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CommitStatus {
    /// `pending`, `success`, `error`, `failure` or `warning`.
    pub status: String,
    /// The name of the check, such as `ci / test (push)`.
    #[serde(default)]
    pub context: String,
    pub description: Option<String>,
    pub target_url: Option<String>,
}

/// A review of a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Review {
    /// `APPROVED`, `REQUEST_CHANGES`, `COMMENT`, `PENDING` or `REQUEST_REVIEW`.
    pub state: String,
    #[serde(default)]
    pub dismissed: bool,
}

#[derive(Deserialize)]
struct Created {
    number: i64,
}

/// A client for one Forgejo's API, authenticated with one access token.
pub struct Forgejo {
    http: reqwest::Client,
    /// The forge's web address, such as `https://codeberg.org`.
    url: String,
    api_url: String,
    token: String,
    /// The ETag of the last answer to the issues list, per repository, kept only when the answer was one page: the
    /// ETag is of the first page, and Forgejo lists newest created first, so a change to an issue on a later page
    /// would not change it.
    etags: Mutex<HashMap<String, String>>,
    /// The issues blocking each issue, as last read.
    pub(crate) blockers: crate::repos::forge::Remembered,
}

impl Forgejo {
    /// A client for the Forgejo at `url` (its web address; the API is at `<url>/api/v1`) that sends `token`.
    pub fn new(url: &str, token: &str) -> Self {
        let url = url.trim_end_matches('/').to_string();
        Self {
            http: reqwest::Client::new(),
            api_url: format!("{url}/api/v1"),
            url,
            token: token.trim().to_string(),
            etags: Mutex::new(HashMap::new()),
            blockers: Default::default(),
        }
    }

    /// The forge's web address, which git remotes on it start with.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The access token, for git to authenticate with.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The issues of `repo` (`owner/repo`), open and closed, without its pull requests; none when the forge says
    /// nothing changed since the last call, which is only known when all the issues fit one page. They come in pages of
    /// fifty, which are followed to the end.
    pub async fn issues(&self, repo: &str) -> Result<Option<Vec<Issue>>, ForgejoError> {
        let known = self.etags().get(repo).cloned();
        let mut url = format!(
            "{}/repos/{repo}/issues?state=all&type=issues&limit=50",
            self.api_url
        );
        let mut issues = Vec::new();
        let mut etag = None;
        let mut pages = 0;
        loop {
            pages += 1;
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
            issues.extend(page.into_iter().filter(|i| i.pull_request.is_none()));
            match next {
                Some(next) => url = next,
                None => break,
            }
        }
        match etag.filter(|e| !e.is_empty()) {
            Some(etag) if pages == 1 => self.etags().insert(repo.to_string(), etag),
            _ => self.etags().remove(repo),
        };
        Ok(Some(issues))
    }

    /// Forgets the ETag of `repo`, so the next call to [`Forgejo::issues`] lists the issues again.
    pub fn forget_issues_etag(&self, repo: &str) {
        self.etags().remove(repo);
    }

    /// Opens an issue in `repo` and returns its number.
    pub async fn create_issue(
        &self,
        repo: &str,
        title: &str,
        body: &str,
    ) -> Result<i64, ForgejoError> {
        let request = json!({ "title": title, "body": body });
        let created: Created = self
            .repo_call(repo, Method::POST, "/issues", Some(&request))
            .await?;
        Ok(created.number)
    }

    /// Replaces the title and body of issue `number` in `repo`.
    pub async fn update_issue(
        &self,
        repo: &str,
        number: i64,
        title: &str,
        body: &str,
    ) -> Result<(), ForgejoError> {
        let request = json!({ "title": title, "body": body });
        let _: Created = self
            .repo_call(
                repo,
                Method::PATCH,
                &format!("/issues/{number}"),
                Some(&request),
            )
            .await?;
        Ok(())
    }

    /// Closes issue `number` in `repo`, or reopens it.
    pub async fn set_issue_state(
        &self,
        repo: &str,
        number: i64,
        open: bool,
    ) -> Result<(), ForgejoError> {
        let request = json!({ "state": if open { "open" } else { "closed" } });
        self.repo_do(
            repo,
            Method::PATCH,
            &format!("/issues/{number}"),
            Some(&request),
        )
        .await
    }

    /// Replaces the labels of issue `number` in `repo`, by name; a label that does not exist there is refused.
    pub async fn set_issue_labels(
        &self,
        repo: &str,
        number: i64,
        labels: &[String],
    ) -> Result<(), ForgejoError> {
        let request = json!({ "labels": labels });
        self.repo_do(
            repo,
            Method::PUT,
            &format!("/issues/{number}/labels"),
            Some(&request),
        )
        .await
    }

    /// Writes a comment on issue `number` in `repo` and returns its id.
    pub async fn create_issue_comment(
        &self,
        repo: &str,
        number: i64,
        body: &str,
    ) -> Result<i64, ForgejoError> {
        #[derive(Deserialize)]
        struct Identified {
            id: i64,
        }
        let request = json!({ "body": body });
        let created: Identified = self
            .repo_call(
                repo,
                Method::POST,
                &format!("/issues/{number}/comments"),
                Some(&request),
            )
            .await?;
        Ok(created.id)
    }

    /// The comments of issue `number` in `repo`, oldest first.
    pub async fn issue_comments(
        &self,
        repo: &str,
        number: i64,
    ) -> Result<Vec<IssueComment>, ForgejoError> {
        let mut url = format!(
            "{}/repos/{repo}/issues/{number}/comments?limit=50",
            self.api_url
        );
        let mut comments = Vec::new();
        loop {
            let response =
                Self::succeeded(self.authorize(self.http.get(&url)).send().await?).await?;
            let next = self.next_page(response.headers());
            comments.extend(response.json::<Vec<IssueComment>>().await?);
            match next {
                Some(next) => url = next,
                None => return Ok(comments),
            }
        }
    }

    /// The numbers of the issues issue `number` of `repo` depends on, which block it, from all pages.
    pub async fn blocked_by(&self, repo: &str, number: i64) -> Result<Vec<i64>, ForgejoError> {
        let mut url = format!(
            "{}/repos/{repo}/issues/{number}/dependencies?limit=50",
            self.api_url
        );
        let mut blockers = Vec::new();
        loop {
            let response =
                Self::succeeded(self.authorize(self.http.get(&url)).send().await?).await?;
            let next = self.next_page(response.headers());
            blockers.extend(response.json::<Vec<Created>>().await?);
            match next {
                Some(next) => url = next,
                None => break,
            }
        }
        Ok(blockers.into_iter().map(|b| b.number).collect())
    }

    /// The description of issue `number` in `repo` as it is now.
    pub async fn issue_body(&self, repo: &str, number: i64) -> Result<String, ForgejoError> {
        #[derive(Deserialize)]
        struct Described {
            body: Option<String>,
        }
        let issue: Described = self
            .repo_call(repo, Method::GET, &format!("/issues/{number}"), None)
            .await?;
        Ok(issue.body.unwrap_or_default())
    }

    /// Makes issue `number` of `repo` depend on issue `blocker`.
    pub async fn add_blocker(
        &self,
        repo: &str,
        number: i64,
        blocker: i64,
    ) -> Result<(), ForgejoError> {
        self.blocker_call(repo, Method::POST, number, blocker).await
    }

    /// Makes issue `number` of `repo` not depend on issue `blocker` any more.
    pub async fn remove_blocker(
        &self,
        repo: &str,
        number: i64,
        blocker: i64,
    ) -> Result<(), ForgejoError> {
        self.blocker_call(repo, Method::DELETE, number, blocker)
            .await
    }

    async fn blocker_call(
        &self,
        repo: &str,
        method: Method,
        number: i64,
        blocker: i64,
    ) -> Result<(), ForgejoError> {
        let (owner, name) = repo.split_once('/').unwrap_or(("", repo));
        let request = json!({ "index": blocker, "owner": owner, "repo": name });
        self.repo_do(
            repo,
            method,
            &format!("/issues/{number}/dependencies"),
            Some(&request),
        )
        .await
    }

    /// Opens a pull request in `repo` that asks for `head` to be merged into `base`, and returns its number.
    pub async fn open_pull_request(
        &self,
        repo: &str,
        title: &str,
        head: &str,
        base: &str,
        body: &str,
    ) -> Result<i64, ForgejoError> {
        let request = json!({ "title": title, "head": head, "base": base, "body": body });
        let created: Created = self
            .repo_call(repo, Method::POST, "/pulls", Some(&request))
            .await?;
        Ok(created.number)
    }

    pub async fn pull_request(&self, repo: &str, number: i64) -> Result<PullRequest, ForgejoError> {
        self.repo_call(repo, Method::GET, &format!("/pulls/{number}"), None)
            .await
    }

    /// The latest status of each context of commit `sha`.
    pub async fn statuses(&self, repo: &str, sha: &str) -> Result<CombinedStatus, ForgejoError> {
        self.repo_call(
            repo,
            Method::GET,
            &format!("/commits/{sha}/status?limit=50"),
            None,
        )
        .await
    }

    /// The reviews of a pull request, oldest first.
    pub async fn reviews(&self, repo: &str, number: i64) -> Result<Vec<Review>, ForgejoError> {
        self.repo_call(
            repo,
            Method::GET,
            &format!("/pulls/{number}/reviews?limit=50"),
            None,
        )
        .await
    }

    /// Merges a pull request with a merge commit titled `message` and returns the commit it left.
    pub async fn merge(
        &self,
        repo: &str,
        number: i64,
        message: &str,
    ) -> Result<String, ForgejoError> {
        let request = json!({ "Do": "merge", "MergeTitleField": message });
        let response = self
            .call(
                Method::POST,
                &format!("/repos/{repo}/pulls/{number}/merge"),
                Some(&request),
            )
            .await?;
        Self::succeeded(response).await?;
        let merged = self.pull_request(repo, number).await?;
        Ok(merged.merge_commit_sha.unwrap_or(merged.head.sha))
    }

    /// Calls the API at `/repos/{repo}{path}` and reads the JSON answer.
    async fn repo_call<T: serde::de::DeserializeOwned>(
        &self,
        repo: &str,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T, ForgejoError> {
        let response = self
            .call(method, &format!("/repos/{repo}{path}"), body)
            .await?;
        Ok(Self::succeeded(response).await?.json().await?)
    }

    /// Calls the API at `/repos/{repo}{path}` and ignores the answer's body.
    async fn repo_do(
        &self,
        repo: &str,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<(), ForgejoError> {
        let response = self
            .call(method, &format!("/repos/{repo}{path}"), body)
            .await?;
        Self::succeeded(response).await?;
        Ok(())
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<reqwest::Response, ForgejoError> {
        let mut request = self.http.request(method, format!("{}{path}", self.api_url));
        if let Some(body) = body {
            request = request.json(body);
        }
        Ok(self.authorize(request).send().await?)
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
            .header("Authorization", format!("token {}", self.token))
            .header("User-Agent", "moebius")
    }

    /// The response if Forgejo answered with a success, an error with Forgejo's explanation otherwise.
    async fn succeeded(response: reqwest::Response) -> Result<reqwest::Response, ForgejoError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        Err(ForgejoError::Status {
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

    /// The `If-None-Match` of each request for the first page of issues.
    #[derive(Default)]
    struct Fake {
        asked: Mutex<Vec<Option<String>>>,
        /// Whether all the issues fit the first page.
        single: std::sync::atomic::AtomicBool,
    }

    async fn serve() -> (String, Arc<Fake>) {
        let fake = Arc::new(Fake::default());
        let app = Router::new()
            .route(
                "/api/v1/repos/{owner}/{repo}/issues",
                get(
                    |State(fake): State<Arc<Fake>>,
                     headers: HeaderMap,
                     Path((owner, repo)): Path<(String, String)>,
                     Query(query): Query<HashMap<String, String>>| async move {
                        if headers.get("authorization").map(|v| v.as_bytes()) != Some(b"token f0rgej0") {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        assert_eq!((owner.as_str(), repo.as_str()), ("acme", "widgets"));
                        assert_eq!(
                            (query["state"].as_str(), query["type"].as_str(), query["limit"].as_str()),
                            ("all", "issues", "50")
                        );
                        let issue = |number: u64, pull: bool| {
                            json!({ "number": number, "title": "Issue", "body": "",
                                    "state": "open", "user": { "login": "leon" },
                                    "pull_request": if pull { json!({ "merged": false }) } else { json!(null) } })
                        };
                        if query.contains_key("page") {
                            return axum::Json(json!([issue(3, false), issue(4, true)])).into_response();
                        }
                        let asked = headers
                            .get("if-none-match")
                            .map(|v| v.to_str().unwrap().to_string());
                        fake.asked.lock().unwrap().push(asked.clone());
                        if asked.as_deref() == Some("\"v1\"") {
                            return StatusCode::NOT_MODIFIED.into_response();
                        }
                        if fake.single.load(std::sync::atomic::Ordering::SeqCst) {
                            if asked.as_deref() == Some("\"v2\"") {
                                return StatusCode::NOT_MODIFIED.into_response();
                            }
                            return (
                                [("etag", "\"v2\"".to_string())],
                                axum::Json(json!([issue(1, false)])),
                            )
                                .into_response();
                        }
                        let host = headers.get("host").unwrap().to_str().unwrap();
                        let next = format!(
                            "<http://{host}/api/v1/repos/acme/widgets/issues?state=all&type=issues&limit=50&page=2>; rel=\"next\", \
                             <http://elsewhere/api/v1/x>; rel=\"last\""
                        );
                        (
                            [("etag", "\"v1\"".to_string()), ("link", next)],
                            axum::Json(json!([issue(1, false)])),
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
    async fn issues_are_listed_page_by_page_with_the_token_without_pull_requests_and_only_when_changed()
     {
        let (url, fake) = serve().await;
        let forgejo = Forgejo::new(&format!("{url}/"), " f0rgej0\n");
        assert_eq!(forgejo.url(), url);
        let issues = forgejo.issues("acme/widgets").await.unwrap().unwrap();
        assert_eq!(
            issues.iter().map(|i| i.number).collect::<Vec<_>>(),
            [1, 3],
            "the second page is followed and its pull request left out"
        );
        assert_eq!(
            (issues[0].body.as_deref(), issues[0].user.login.as_str()),
            (Some(""), "leon")
        );

        // The ETag is of the first page only, which a change on a later page does not touch: with several pages
        // the issues are listed every time.
        assert!(forgejo.issues("acme/widgets").await.unwrap().is_some());
        assert_eq!(*fake.asked.lock().unwrap(), [None, None]);

        // All issues on one page: the ETag covers them.
        fake.single.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(forgejo.issues("acme/widgets").await.unwrap().is_some());
        assert_eq!(forgejo.issues("acme/widgets").await.unwrap(), None);
        forgejo.forget_issues_etag("acme/widgets");
        assert!(forgejo.issues("acme/widgets").await.unwrap().is_some());
        assert_eq!(
            *fake.asked.lock().unwrap(),
            [None, None, None, Some("\"v2\"".to_string()), None]
        );

        let refused = Forgejo::new(&url, "wrong").issues("acme/widgets").await;
        assert!(
            matches!(&refused, Err(ForgejoError::Status { status, .. }) if *status == reqwest::StatusCode::UNAUTHORIZED),
            "{refused:?}"
        );
    }
}
