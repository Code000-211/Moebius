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
    /// The ETag of the last full answer to the issues list, per repository.
    etags: Mutex<HashMap<String, String>>,
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
    /// nothing changed since the last call. They come in pages of fifty, which are followed to the end.
    pub async fn issues(&self, repo: &str) -> Result<Option<Vec<Issue>>, ForgejoError> {
        let known = self.etags().get(repo).cloned();
        let mut url = format!(
            "{}/repos/{repo}/issues?state=all&type=issues&limit=50",
            self.api_url
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
            issues.extend(page.into_iter().filter(|i| i.pull_request.is_none()));
            match next {
                Some(next) => url = next,
                None => break,
            }
        }
        if let Some(etag) = etag.filter(|e| !e.is_empty()) {
            self.etags().insert(repo.to_string(), etag);
        }
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

        assert_eq!(forgejo.issues("acme/widgets").await.unwrap(), None);
        forgejo.forget_issues_etag("acme/widgets");
        assert!(forgejo.issues("acme/widgets").await.unwrap().is_some());
        assert_eq!(
            *fake.asked.lock().unwrap(),
            [None, Some("\"v1\"".to_string()), None]
        );

        let refused = Forgejo::new(&url, "wrong").issues("acme/widgets").await;
        assert!(
            matches!(&refused, Err(ForgejoError::Status { status, .. }) if *status == reqwest::StatusCode::UNAUTHORIZED),
            "{refused:?}"
        );
    }
}
