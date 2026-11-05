//! The GitHub App Moebius acts as: it signs a JWT with the app's private key and exchanges it for an installation
//! token per repository owner, which it keeps until shortly before it expires. It also lists a repository's issues,
//! asking GitHub only for what changed since the last time, opens and edits issues, and opens, reads and merges pull
//! requests.

use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::Mutex;

/// How long before its expiry a token is renewed.
const RENEW_BEFORE: Duration = Duration::from_secs(5 * 60);

/// Why talking to GitHub failed.
#[derive(Debug, thiserror::Error)]
pub enum GitHubError {
    #[error("the GitHub App's private key is not a valid RSA key: {0}")]
    Key(jsonwebtoken::errors::Error),
    #[error("cannot sign the GitHub App's token: {0}")]
    Sign(jsonwebtoken::errors::Error),
    #[error("cannot reach GitHub: {0}")]
    Http(#[from] reqwest::Error),
    #[error("GitHub answered {status} to {url}: {body}")]
    Status {
        url: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("GitHub sent an expiry that is not a time: {0}")]
    ExpiresAt(String),
    #[error("{0:?} is not a GitHub repository as owner/repo")]
    Repository(String),
}

/// An issue as GitHub lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Issue {
    pub number: i64,
    pub title: String,
    pub body: Option<String>,
    /// `open` or `closed`.
    pub state: String,
    pub user: User,
    /// When the issue last changed, in the form GitHub always writes it, so that it compares as text.
    #[serde(default)]
    pub(crate) updated_at: Option<String>,
    /// Present on the items of the issues list that are pull requests.
    pull_request: Option<serde_json::Value>,
    #[serde(default)]
    pub labels: Vec<Label>,
    /// The address of the issue this one is a sub-issue of, which ends in its number.
    #[serde(default)]
    pub parent_issue_url: Option<String>,
    /// How many comments the issue has.
    #[serde(default)]
    pub comments: i64,
    /// Present where the forge has issue dependencies.
    #[serde(default)]
    pub issue_dependencies_summary: Option<DependenciesSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Label {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DependenciesSummary {
    #[serde(default)]
    pub total_blocked_by: i64,
}

/// A comment on an issue as GitHub lists it.
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

/// A pull request as GitHub describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PullRequest {
    /// `open` or `closed`.
    pub state: String,
    pub merged: bool,
    pub merge_commit_sha: Option<String>,
    pub head: Head,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Head {
    pub sha: String,
}

/// A check run of a commit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CheckRun {
    pub name: String,
    pub details_url: Option<String>,
    /// `queued`, `in_progress` or `completed`.
    pub status: String,
    /// How a completed check ended, such as `success` or `failure`.
    pub conclusion: Option<String>,
    pub output: Option<CheckOutput>,
}

/// What a check run says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CheckOutput {
    pub title: Option<String>,
}

#[derive(Deserialize)]
struct CheckRuns {
    check_runs: Vec<CheckRun>,
}

/// A review of a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Review {
    /// Upper case, such as `APPROVED`, `CHANGES_REQUESTED` or `COMMENTED`.
    pub state: String,
}

#[derive(Deserialize)]
struct Created {
    number: i64,
}

#[derive(Deserialize)]
struct Identified {
    id: i64,
}

#[derive(Deserialize)]
struct Merged {
    sha: String,
}

#[derive(Serialize)]
struct Claims {
    iat: u64,
    exp: u64,
    iss: String,
}

#[derive(Deserialize)]
struct Installation {
    id: u64,
}

#[derive(Deserialize)]
struct AccessToken {
    token: String,
    expires_at: String,
}

struct CachedToken {
    token: String,
    renew_at: SystemTime,
}

/// A client for the GitHub API that authenticates as the app and as its installations.
pub struct GitHub {
    http: reqwest::Client,
    api_url: String,
    app_id: u64,
    key: EncodingKey,
    tokens: Mutex<HashMap<String, CachedToken>>,
    /// The ETag of the last full answer to the issues list, per repository.
    etags: StdMutex<HashMap<String, String>>,
    /// The `updated_at` of the newest issue in the last full answer, per repository: the next poll stops paging once it
    /// reaches issues older than that.
    newest: StdMutex<HashMap<String, String>>,
    /// The issues blocking each issue, as last read.
    pub(crate) blockers: crate::repos::forge::Remembered,
}

impl GitHub {
    /// A client for the API at `api_url`, for the app `app_id` whose private key is `private_key_pem`.
    pub fn new(api_url: &str, app_id: u64, private_key_pem: &[u8]) -> Result<Self, GitHubError> {
        Ok(Self {
            http: reqwest::Client::new(),
            api_url: api_url.trim_end_matches('/').to_string(),
            app_id,
            key: EncodingKey::from_rsa_pem(private_key_pem).map_err(GitHubError::Key)?,
            tokens: Mutex::new(HashMap::new()),
            etags: StdMutex::new(HashMap::new()),
            newest: StdMutex::new(HashMap::new()),
            blockers: Default::default(),
        })
    }

    /// Whether `url` is an https address on this GitHub's web host: `github.com` for the public API, the Enterprise
    /// host otherwise. Git credentials go to no other host.
    pub fn is_web_url(&self, url: &str) -> bool {
        let authority = |url: &str| {
            let rest = url.split_once("://").map(|(_, rest)| rest)?;
            Some(rest.split('/').next().unwrap_or("").to_ascii_lowercase())
        };
        let Some(api) = authority(&self.api_url) else {
            return false;
        };
        let web = if api == "api.github.com" {
            "github.com".to_string()
        } else {
            api
        };
        url.get(..8)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
            && authority(url).is_some_and(|host| host == web)
    }

    /// A token for the app's installation on `owner`, found through `repo`. Tokens are reused until five minutes
    /// before they expire.
    pub async fn installation_token(&self, owner: &str, repo: &str) -> Result<String, GitHubError> {
        let mut tokens = self.tokens.lock().await;
        if let Some(cached) = tokens.get(owner)
            && SystemTime::now() < cached.renew_at
        {
            return Ok(cached.token.clone());
        }
        let jwt = self.app_jwt()?;
        let installation: Installation = self
            .send(
                self.http.get(format!(
                    "{}/repos/{owner}/{repo}/installation",
                    self.api_url
                )),
                &jwt,
            )
            .await?;
        let access: AccessToken = self
            .send(
                self.http.post(format!(
                    "{}/app/installations/{}/access_tokens",
                    self.api_url, installation.id
                )),
                &jwt,
            )
            .await?;
        let expires_at = OffsetDateTime::parse(&access.expires_at, &Rfc3339)
            .map_err(|_| GitHubError::ExpiresAt(access.expires_at.clone()))?;
        tokens.insert(
            owner.to_string(),
            CachedToken {
                token: access.token.clone(),
                renew_at: SystemTime::from(expires_at) - RENEW_BEFORE,
            },
        );
        Ok(access.token)
    }

    /// The JWT the app authenticates with: issued a minute ago to allow for clock drift, valid for ten minutes in all.
    fn app_jwt(&self) -> Result<String, GitHubError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let claims = Claims {
            iat: now - 60,
            exp: now + 9 * 60,
            iss: self.app_id.to_string(),
        };
        jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &self.key)
            .map_err(GitHubError::Sign)
    }

    /// The issues of the repository `github` (`owner/repo`), pull requests left out; none when nothing changed since
    /// the last call. Open and closed issues come in pages of a hundred, newest change first, which are followed until
    /// a page ends with an issue older than the newest one of the last answer: once an answer is known, the list holds
    /// only the issues updated since, not all of them.
    pub async fn issues(&self, github: &str) -> Result<Option<Vec<Issue>>, GitHubError> {
        let (owner, repo) = github
            .split_once('/')
            .ok_or_else(|| GitHubError::Repository(github.to_string()))?;
        let token = self.installation_token(owner, repo).await?;
        let known = self.etags().get(github).cloned();
        let known_newest = self.newest().get(github).cloned();
        let mut url = format!(
            "{}/repos/{github}/issues?state=all&per_page=100&sort=updated&direction=desc",
            self.api_url
        );
        let mut issues = Vec::new();
        let mut etag = None;
        let mut newest: Option<String> = None;
        loop {
            let mut request = self.http.get(&url);
            if let (Some(known), None) = (&known, &etag) {
                request = request.header("If-None-Match", known);
            }
            let response = Self::authorize(request, &token).send().await?;
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
            issues.extend(page.into_iter().filter(|i| i.pull_request.is_none()));
            match next {
                Some(next) if !reached_known => url = next,
                _ => break,
            }
        }
        if let Some(etag) = etag.filter(|e| !e.is_empty()) {
            self.etags().insert(github.to_string(), etag);
            if let Some(newest) = newest {
                self.newest().insert(github.to_string(), newest);
            }
        }
        Ok(Some(issues))
    }

    /// Opens a pull request in `github` that asks for `head` to be merged into `base`, and returns its number.
    pub async fn open_pull_request(
        &self,
        github: &str,
        title: &str,
        head: &str,
        base: &str,
        body: &str,
    ) -> Result<i64, GitHubError> {
        let request = json!({ "title": title, "head": head, "base": base, "body": body });
        let created: Created = self
            .repository_call(github, Method::POST, "/pulls", Some(&request))
            .await?;
        Ok(created.number)
    }

    /// Opens an issue in `github` and returns its number.
    pub async fn create_issue(
        &self,
        github: &str,
        title: &str,
        body: &str,
    ) -> Result<i64, GitHubError> {
        let request = json!({ "title": title, "body": body });
        let created: Created = self
            .repository_call(github, Method::POST, "/issues", Some(&request))
            .await?;
        Ok(created.number)
    }

    /// Replaces the title and body of issue `number` in `github`.
    pub async fn update_issue(
        &self,
        github: &str,
        number: i64,
        title: &str,
        body: &str,
    ) -> Result<(), GitHubError> {
        let request = json!({ "title": title, "body": body });
        let _: Created = self
            .repository_call(
                github,
                Method::PATCH,
                &format!("/issues/{number}"),
                Some(&request),
            )
            .await?;
        Ok(())
    }

    /// Closes issue `number` in `github` as completed, or reopens it.
    pub async fn set_issue_state(
        &self,
        github: &str,
        number: i64,
        open: bool,
    ) -> Result<(), GitHubError> {
        let request = if open {
            json!({ "state": "open", "state_reason": "reopened" })
        } else {
            json!({ "state": "closed", "state_reason": "completed" })
        };
        self.repository_do(
            github,
            Method::PATCH,
            &format!("/issues/{number}"),
            Some(&request),
        )
        .await
    }

    /// Replaces the labels of issue `number` in `github`; GitHub makes the ones that do not exist.
    pub async fn set_issue_labels(
        &self,
        github: &str,
        number: i64,
        labels: &[String],
    ) -> Result<(), GitHubError> {
        let request = json!({ "labels": labels });
        self.repository_do(
            github,
            Method::PUT,
            &format!("/issues/{number}/labels"),
            Some(&request),
        )
        .await
    }

    /// Writes a comment on issue `number` in `github` and returns its id.
    pub async fn create_issue_comment(
        &self,
        github: &str,
        number: i64,
        body: &str,
    ) -> Result<i64, GitHubError> {
        let request = json!({ "body": body });
        let created: Identified = self
            .repository_call(
                github,
                Method::POST,
                &format!("/issues/{number}/comments"),
                Some(&request),
            )
            .await?;
        Ok(created.id)
    }

    /// The comments of issue `number` in `github`, oldest first.
    pub async fn issue_comments(
        &self,
        github: &str,
        number: i64,
    ) -> Result<Vec<IssueComment>, GitHubError> {
        let (owner, repo) = github
            .split_once('/')
            .ok_or_else(|| GitHubError::Repository(github.to_string()))?;
        let token = self.installation_token(owner, repo).await?;
        let mut url = format!(
            "{}/repos/{github}/issues/{number}/comments?per_page=100",
            self.api_url
        );
        let mut comments = Vec::new();
        loop {
            let response =
                Self::succeeded(Self::authorize(self.http.get(&url), &token).send().await?).await?;
            let next = self.next_page(response.headers());
            comments.extend(response.json::<Vec<IssueComment>>().await?);
            match next {
                Some(next) => url = next,
                None => return Ok(comments),
            }
        }
    }

    /// The id GitHub gives issue `number` of `github`, which sub-issues and dependencies are written with.
    async fn issue_id(&self, github: &str, number: i64) -> Result<i64, GitHubError> {
        let issue: Identified = self
            .repository_call(github, Method::GET, &format!("/issues/{number}"), None)
            .await?;
        Ok(issue.id)
    }

    /// Makes issue `number` of `github` a sub-issue of `parent`, taking it from the parent it has, or takes it from
    /// `previous` when `parent` is none.
    pub async fn set_issue_parent(
        &self,
        github: &str,
        number: i64,
        parent: Option<i64>,
        previous: Option<i64>,
    ) -> Result<(), GitHubError> {
        let id = self.issue_id(github, number).await?;
        let request = json!({ "sub_issue_id": id, "replace_parent": true });
        match (parent, previous) {
            (Some(parent), _) => {
                self.repository_do(
                    github,
                    Method::POST,
                    &format!("/issues/{parent}/sub_issues"),
                    Some(&request),
                )
                .await
            }
            (None, Some(previous)) => {
                self.repository_do(
                    github,
                    Method::DELETE,
                    &format!("/issues/{previous}/sub_issue"),
                    Some(&request),
                )
                .await
            }
            (None, None) => Ok(()),
        }
    }

    /// Makes issue `blocker` of `github` block issue `number`.
    pub async fn add_blocker(
        &self,
        github: &str,
        number: i64,
        blocker: i64,
    ) -> Result<(), GitHubError> {
        let request = json!({ "issue_id": self.issue_id(github, blocker).await? });
        self.repository_do(
            github,
            Method::POST,
            &format!("/issues/{number}/dependencies/blocked_by"),
            Some(&request),
        )
        .await
    }

    /// Stops issue `blocker` of `github` from blocking issue `number`.
    pub async fn remove_blocker(
        &self,
        github: &str,
        number: i64,
        blocker: i64,
    ) -> Result<(), GitHubError> {
        let id = self.issue_id(github, blocker).await?;
        self.repository_do(
            github,
            Method::DELETE,
            &format!("/issues/{number}/dependencies/blocked_by/{id}"),
            None,
        )
        .await
    }

    /// The numbers of the issues that block issue `number` of `github`, from all pages.
    pub async fn blocked_by(&self, github: &str, number: i64) -> Result<Vec<i64>, GitHubError> {
        let (owner, repo) = github
            .split_once('/')
            .ok_or_else(|| GitHubError::Repository(github.to_string()))?;
        let token = self.installation_token(owner, repo).await?;
        let mut url = format!(
            "{}/repos/{github}/issues/{number}/dependencies/blocked_by?per_page=100",
            self.api_url
        );
        let mut blockers = Vec::new();
        loop {
            let response =
                Self::succeeded(Self::authorize(self.http.get(&url), &token).send().await?).await?;
            let next = self.next_page(response.headers());
            blockers.extend(response.json::<Vec<Created>>().await?);
            match next {
                Some(next) => url = next,
                None => break,
            }
        }
        Ok(blockers.into_iter().map(|b| b.number).collect())
    }

    pub async fn pull_request(
        &self,
        github: &str,
        number: i64,
    ) -> Result<PullRequest, GitHubError> {
        self.repository_call(github, Method::GET, &format!("/pulls/{number}"), None)
            .await
    }

    /// The check runs of the commit `sha`.
    pub async fn check_runs(&self, github: &str, sha: &str) -> Result<Vec<CheckRun>, GitHubError> {
        let runs: CheckRuns = self
            .repository_call(
                github,
                Method::GET,
                &format!("/commits/{sha}/check-runs?per_page=100"),
                None,
            )
            .await?;
        Ok(runs.check_runs)
    }

    /// The reviews of a pull request, oldest first.
    pub async fn reviews(&self, github: &str, number: i64) -> Result<Vec<Review>, GitHubError> {
        self.repository_call(
            github,
            Method::GET,
            &format!("/pulls/{number}/reviews?per_page=100"),
            None,
        )
        .await
    }

    /// Merges a pull request with a merge commit titled `title` and returns the commit's sha.
    pub async fn merge_pull_request(
        &self,
        github: &str,
        number: i64,
        title: &str,
    ) -> Result<String, GitHubError> {
        let request = json!({ "commit_title": title, "merge_method": "merge" });
        let merged: Merged = self
            .repository_call(
                github,
                Method::PUT,
                &format!("/pulls/{number}/merge"),
                Some(&request),
            )
            .await?;
        Ok(merged.sha)
    }

    /// Calls the API at `/repos/{github}{path}` with the installation's token and reads the JSON answer.
    async fn repository_call<T: serde::de::DeserializeOwned>(
        &self,
        github: &str,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T, GitHubError> {
        let (owner, repo) = github
            .split_once('/')
            .ok_or_else(|| GitHubError::Repository(github.to_string()))?;
        let token = self.installation_token(owner, repo).await?;
        let mut request = self
            .http
            .request(method, format!("{}/repos/{github}{path}", self.api_url));
        if let Some(body) = body {
            request = request.json(body);
        }
        self.send(request, &token).await
    }

    /// Calls the API at `/repos/{github}{path}` with the installation's token and ignores the answer's body.
    async fn repository_do(
        &self,
        github: &str,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<(), GitHubError> {
        let (owner, repo) = github
            .split_once('/')
            .ok_or_else(|| GitHubError::Repository(github.to_string()))?;
        let token = self.installation_token(owner, repo).await?;
        let mut request = self
            .http
            .request(method, format!("{}/repos/{github}{path}", self.api_url));
        if let Some(body) = body {
            request = request.json(body);
        }
        Self::succeeded(Self::authorize(request, &token).send().await?).await?;
        Ok(())
    }

    /// Forgets the ETag of `github`, so the next call to [`GitHub::issues`] lists the issues again.
    pub fn forget_issues_etag(&self, github: &str) {
        self.etags().remove(github);
        self.newest().remove(github);
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
            .filter(|url| url.starts_with(&self.api_url))
    }

    /// Sends `request` with `bearer` as its credential and reads the JSON answer.
    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        bearer: &str,
    ) -> Result<T, GitHubError> {
        let response = Self::authorize(request, bearer).send().await?;
        Ok(Self::succeeded(response).await?.json().await?)
    }

    fn authorize(request: reqwest::RequestBuilder, bearer: &str) -> reqwest::RequestBuilder {
        request
            .bearer_auth(bearer)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "moebius")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    /// The response if GitHub answered with a success, an error with GitHub's explanation otherwise.
    async fn succeeded(response: reqwest::Response) -> Result<reqwest::Response, GitHubError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        Err(GitHubError::Status {
            url: response.url().to_string(),
            status,
            body: response.text().await.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::extract::{Path, Query, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use jsonwebtoken::{DecodingKey, Validation};
    use serde_json::json;

    use super::*;

    const PRIVATE_KEY: &[u8] = include_bytes!("../../testdata/github-app-test-key.pem");
    const PUBLIC_KEY: &[u8] = include_bytes!("../../testdata/github-app-test-key.pub.pem");
    const APP_ID: u64 = 4242;

    #[test]
    fn git_credentials_belong_to_the_web_host_of_the_api_only() {
        let github = |api: &str| GitHub::new(api, APP_ID, PRIVATE_KEY).unwrap();
        let public = github("https://api.github.com");
        assert!(public.is_web_url("https://github.com/acme/widgets.git"));
        assert!(public.is_web_url("HTTPS://GitHub.com/acme/widgets.git"));
        for url in [
            "https://attacker.example/x.git",
            "https://github.com.attacker.example/x.git",
            "https://github.com@attacker.example/x.git",
            "http://github.com/acme/widgets.git",
            "git@github.com:acme/widgets.git",
            "/srv/widgets.git",
            "https://api.github.com/acme/widgets.git",
        ] {
            assert!(!public.is_web_url(url), "{url}");
        }
        let enterprise = github("https://ghe.example:8443/api/v3/");
        assert!(enterprise.is_web_url("https://ghe.example:8443/acme/widgets.git"));
        assert!(!enterprise.is_web_url("https://github.com/acme/widgets.git"));
        assert!(!enterprise.is_web_url("https://ghe.example/acme/widgets.git"));
    }

    /// A GitHub that only accepts the app's JWT and hands out tokens that expire `token_lifetime` from now.
    struct Fake {
        token_lifetime: Duration,
        exchanges: AtomicUsize,
        /// The `If-None-Match` of each request for the first page of issues.
        asked_for_issues: StdMutex<Vec<Option<String>>>,
        /// Requests for a later page of issues.
        later_pages: AtomicUsize,
        /// Whether an old issue has changed since the first answer, which then lists it first.
        changed: std::sync::atomic::AtomicBool,
        /// Whether the first page holds only issues newer than the first answer's newest, with more after them.
        busy: std::sync::atomic::AtomicBool,
    }

    #[derive(Deserialize)]
    struct Verified {
        iat: u64,
        exp: u64,
        iss: String,
    }

    fn check_request(headers: &HeaderMap) -> Result<(), StatusCode> {
        let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
        assert_eq!(header("accept"), Some("application/vnd.github+json"));
        assert_eq!(header("user-agent"), Some("moebius"));
        assert_eq!(header("x-github-api-version"), Some("2022-11-28"));
        let jwt = header("authorization")
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let key = DecodingKey::from_rsa_pem(PUBLIC_KEY).unwrap();
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_required_spec_claims(&["exp", "iat", "iss"]);
        let claims = jsonwebtoken::decode::<Verified>(jwt, &key, &validation)
            .map_err(|_| StatusCode::UNAUTHORIZED)?
            .claims;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            claims.iat < now && now - claims.iat <= 120,
            "iat in the past"
        );
        assert!(
            claims.exp > now && claims.exp - claims.iat <= 600,
            "exp within ten minutes"
        );
        if claims.iss != APP_ID.to_string() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(())
    }

    async fn serve(token_lifetime: Duration) -> (String, Arc<Fake>) {
        let fake = Arc::new(Fake {
            token_lifetime,
            exchanges: AtomicUsize::new(0),
            asked_for_issues: StdMutex::new(Vec::new()),
            later_pages: AtomicUsize::new(0),
            busy: std::sync::atomic::AtomicBool::new(false),
            changed: std::sync::atomic::AtomicBool::new(false),
        });
        let app = Router::new()
            .route(
                "/repos/{owner}/{repo}/installation",
                get(
                    |headers: HeaderMap, Path((owner, repo)): Path<(String, String)>| async move {
                        check_request(&headers)?;
                        if (owner.as_str(), repo.as_str()) != ("acme", "widgets") {
                            return Err(StatusCode::NOT_FOUND);
                        }
                        Ok(axum::Json(json!({ "id": 77 })))
                    },
                ),
            )
            .route(
                "/app/installations/{id}/access_tokens",
                post(
                    |State(fake): State<Arc<Fake>>, headers: HeaderMap, Path(id): Path<u64>| async move {
                        check_request(&headers)?;
                        assert_eq!(id, 77);
                        let n = fake.exchanges.fetch_add(1, Ordering::SeqCst) + 1;
                        let expires = OffsetDateTime::now_utc() + fake.token_lifetime;
                        Ok::<_, StatusCode>((
                            StatusCode::CREATED,
                            axum::Json(json!({
                                "token": format!("ghs_token{n}"),
                                "expires_at": expires.format(&Rfc3339).unwrap(),
                            })),
                        ))
                    },
                ),
            )
            .route(
                "/repos/{owner}/{repo}/issues",
                get(
                    |State(fake): State<Arc<Fake>>,
                     headers: HeaderMap,
                     Query(query): Query<HashMap<String, String>>| async move {
                        assert_eq!(
                            headers.get("authorization").unwrap(),
                            "Bearer ghs_token1"
                        );
                        assert_eq!(query["state"], "all");
                        assert_eq!(query["per_page"], "100");
                        assert_eq!(
                            (query["sort"].as_str(), query["direction"].as_str()),
                            ("updated", "desc"),
                            "the newest change comes first, so the first page's ETag sees every change"
                        );
                        let issue = |number: u64, updated_at: &str| {
                            json!({ "number": number, "title": "Issue", "body": null,
                                    "state": "open", "user": { "login": "leon" },
                                    "updated_at": updated_at })
                        };
                        if query.contains_key("page") {
                            fake.later_pages.fetch_add(1, Ordering::SeqCst);
                            return axum::Json(json!([issue(3, "2026-01-01T00:00:00Z")]))
                                .into_response();
                        }
                        let asked = headers
                            .get("if-none-match")
                            .map(|v| v.to_str().unwrap().to_string());
                        fake.asked_for_issues.lock().unwrap().push(asked.clone());
                        let host = headers.get("host").unwrap().to_str().unwrap();
                        let next = format!(
                            "<http://{host}/repos/acme/widgets/issues?state=all&per_page=100&sort=updated&direction=desc&page=2>; rel=\"next\""
                        );
                        if fake.busy.load(Ordering::SeqCst) {
                            // Every issue of the first page is newer than the last poll's newest: more may follow.
                            return (
                                [("etag", "\"v3\"".to_string()), ("link", next)],
                                axum::Json(json!([
                                    issue(5, "2026-07-01T00:00:00Z"),
                                    issue(4, "2026-06-15T00:00:00Z")
                                ])),
                            )
                                .into_response();
                        }
                        if fake.changed.load(Ordering::SeqCst) {
                            // Issue 3, which was on the last page, changed and is first now.
                            return (
                                [("etag", "\"v2\"".to_string()), ("link", next)],
                                axum::Json(json!([
                                    issue(3, "2026-06-01T00:00:00Z"),
                                    issue(1, "2026-02-01T00:00:00Z"),
                                    issue(9, "2026-01-02T00:00:00Z")
                                ])),
                            )
                                .into_response();
                        }
                        if asked.as_deref() == Some("\"v1\"") {
                            return StatusCode::NOT_MODIFIED.into_response();
                        }
                        let pull_request = json!({ "number": 2, "title": "A pull request", "body": "",
                                "state": "open", "user": { "login": "leon" }, "pull_request": {} });
                        (
                            [("etag", "\"v1\"".to_string()), ("link", next)],
                            axum::Json(json!([issue(1, "2026-02-01T00:00:00Z"), pull_request])),
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

    fn client(url: &str) -> GitHub {
        GitHub::new(url, APP_ID, PRIVATE_KEY).unwrap()
    }

    #[tokio::test]
    async fn a_token_is_fetched_with_a_signed_jwt_and_reused() {
        let (url, fake) = serve(Duration::from_secs(3600)).await;
        let github = client(&url);
        assert_eq!(
            github.installation_token("acme", "widgets").await.unwrap(),
            "ghs_token1"
        );
        assert_eq!(
            github.installation_token("acme", "gadgets").await.unwrap(),
            "ghs_token1",
            "the token is cached per owner"
        );
        assert_eq!(fake.exchanges.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_token_that_is_about_to_expire_is_renewed() {
        let (url, fake) = serve(Duration::from_secs(60)).await;
        let github = client(&url);
        assert_eq!(
            github.installation_token("acme", "widgets").await.unwrap(),
            "ghs_token1"
        );
        assert_eq!(
            github.installation_token("acme", "widgets").await.unwrap(),
            "ghs_token2"
        );
        assert_eq!(fake.exchanges.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn failures_are_reported() {
        let (url, _) = serve(Duration::from_secs(3600)).await;
        assert!(matches!(
            client(&url).installation_token("acme", "missing").await,
            Err(GitHubError::Status { status, .. }) if status == reqwest::StatusCode::NOT_FOUND
        ));
        let wrong_app = GitHub::new(&url, APP_ID + 1, PRIVATE_KEY).unwrap();
        assert!(
            wrong_app
                .installation_token("acme", "widgets")
                .await
                .is_err()
        );
        assert!(matches!(
            GitHub::new(&url, APP_ID, b"not a key"),
            Err(GitHubError::Key(_))
        ));
    }

    #[tokio::test]
    async fn issues_are_listed_page_by_page_without_pull_requests_and_only_when_changed() {
        let (url, fake) = serve(Duration::from_secs(3600)).await;
        let github = client(&url);
        let issues = github.issues("acme/widgets").await.unwrap().unwrap();
        assert_eq!(
            issues.iter().map(|i| i.number).collect::<Vec<_>>(),
            [1, 3],
            "the pull request is left out and the second page is followed"
        );
        assert_eq!(issues[0].body, None);
        assert_eq!(issues[0].user.login, "leon");

        assert_eq!(github.issues("acme/widgets").await.unwrap(), None);
        github.forget_issues_etag("acme/widgets");
        assert!(github.issues("acme/widgets").await.unwrap().is_some());
        assert_eq!(
            *fake.asked_for_issues.lock().unwrap(),
            [None, Some("\"v1\"".to_string()), None]
        );
        assert_eq!(fake.later_pages.load(Ordering::SeqCst), 2);

        // An issue from the last page changes: the first page changes, and the pages that hold only issues older
        // than the newest one of the last poll are not read again.
        fake.changed.store(true, Ordering::SeqCst);
        let issues = github.issues("acme/widgets").await.unwrap().unwrap();
        assert_eq!(
            issues.iter().map(|i| i.number).collect::<Vec<_>>(),
            [3, 1, 9]
        );
        assert_eq!(
            fake.later_pages.load(Ordering::SeqCst),
            2,
            "the first page ends with an issue older than the last poll's newest, so the next page is not asked for"
        );

        // The first page still ends with an issue newer than that: the next page may hold changes too.
        fake.busy.store(true, Ordering::SeqCst);
        let issues = github.issues("acme/widgets").await.unwrap().unwrap();
        assert_eq!(
            issues.iter().map(|i| i.number).collect::<Vec<_>>(),
            [5, 4, 3]
        );
        assert_eq!(fake.later_pages.load(Ordering::SeqCst), 3);
        assert!(matches!(
            github.issues("widgets").await,
            Err(GitHubError::Repository(_))
        ));
    }
}
