//! The GitHub App Moebius acts as: it signs a JWT with the app's private key and exchanges it for an installation
//! token per repository owner, which it keeps until shortly before it expires.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
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

    /// Sends `request` with `bearer` as its credential and reads the JSON answer.
    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        bearer: &str,
    ) -> Result<T, GitHubError> {
        let response = request
            .bearer_auth(bearer)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "moebius")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(GitHubError::Status {
                url: response.url().to_string(),
                status,
                body: response.text().await.unwrap_or_default(),
            });
        }
        Ok(response.json().await?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::extract::{Path, State};
    use axum::http::{HeaderMap, StatusCode};
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
}
