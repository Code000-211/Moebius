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
    /// Present on the items of the issues list that are pull requests.
    pull_request: Option<serde_json::Value>,
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
        })
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
    /// the last call. Open and closed issues come in pages of a hundred, which are followed to the end.
    pub async fn issues(&self, github: &str) -> Result<Option<Vec<Issue>>, GitHubError> {
        let (owner, repo) = github
            .split_once('/')
            .ok_or_else(|| GitHubError::Repository(github.to_string()))?;
        let token = self.installation_token(owner, repo).await?;
        let known = self.etags().get(github).cloned();
        let mut url = format!(
            "{}/repos/{github}/issues?state=all&per_page=100",
            self.api_url
        );
        let mut issues = Vec::new();
        let mut etag = None;
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
            issues.extend(page.into_iter().filter(|i| i.pull_request.is_none()));
            match next {
                Some(next) => url = next,
                None => break,
            }
        }
        if let Some(etag) = etag.filter(|e| !e.is_empty()) {
            self.etags().insert(github.to_string(), etag);
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

    /// Forgets the ETag of `github`, so the next call to [`GitHub::issues`] lists the issues again.
    pub fn forget_issues_etag(&self, github: &str) {
        self.etags().remove(github);
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

    const PRIVATE_KEY: &[u8] = include_bytes!("../testdata/github-app-test-key.pem");
    const PUBLIC_KEY: &[u8] = include_bytes!("../testdata/github-app-test-key.pub.pem");
    const APP_ID: u64 = 4242;

    /// A GitHub that only accepts the app's JWT and hands out tokens that expire `token_lifetime` from now.
    struct Fake {
        token_lifetime: Duration,
        exchanges: AtomicUsize,
        /// The `If-None-Match` of each request for the first page of issues.
        asked_for_issues: StdMutex<Vec<Option<String>>>,
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
                        let issue = |number: u64| {
                            json!({ "number": number, "title": "Issue", "body": null,
                                    "state": "open", "user": { "login": "leon" } })
                        };
                        if query.contains_key("page") {
                            return axum::Json(json!([issue(3)])).into_response();
                        }
                        let asked = headers
                            .get("if-none-match")
                            .map(|v| v.to_str().unwrap().to_string());
                        fake.asked_for_issues.lock().unwrap().push(asked.clone());
                        if asked.as_deref() == Some("\"v1\"") {
                            return StatusCode::NOT_MODIFIED.into_response();
                        }
                        let host = headers.get("host").unwrap().to_str().unwrap();
                        let next = format!(
                            "<http://{host}/repos/acme/widgets/issues?state=all&per_page=100&page=2>; rel=\"next\""
                        );
                        let pull_request = json!({ "number": 2, "title": "A pull request", "body": "",
                                "state": "open", "user": { "login": "leon" }, "pull_request": {} });
                        (
                            [("etag", "\"v1\"".to_string()), ("link", next)],
                            axum::Json(json!([issue(1), pull_request])),
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
        assert!(matches!(
            github.issues("widgets").await,
            Err(GitHubError::Repository(_))
        ));
    }
}
