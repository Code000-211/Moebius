//! Logging in to an upstream with OAuth 2.0 instead of an API key, for a provider whose subscription is used through
//! Moebius. The upstream's `oauth` table names the flow, the client id and the provider's URLs; nothing of any provider
//! is written here.
//!
//! A person starts a login from the Models page (`POST /v1/models/upstreams/{place}/login`) after confirming
//! [`TERMS_WARNING`]. The login runs in the background: `device` shows a code to type at the provider and polls its
//! token URL (RFC 8628), `code` opens the provider's page in the browser and waits for its redirect to a loopback
//! address, with PKCE (RFC 7636). The refresh token it ends with is kept in `secrets.toml` under the `oauth.secret`
//! name; the access token is kept in memory and refreshed before it expires. Every token is remembered by
//! [`crate::blobs::remember_secret`], so that it is redacted wherever it would appear, and none is ever answered.
//!
//! A login by `code` can also end by the person pasting the code, or the address the browser ended on, to
//! `POST /v1/models/upstreams/{place}/login/code` ([`Router::paste_login`]), for a browser that cannot reach the
//! loopback address. `preset` names the providers that are built in (their client ids and URLs, and what their
//! requests need).
//!
//! This module holds the configuration, the tokens and the state of a login; `device` and `code` are the two flows.

mod code;
mod config;
mod device;
mod preset;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use config::{Flow, OAuthConfig};
pub(super) use preset::Preset;
pub use preset::{Provider, providers};

use super::{Protocol, Router, Upstream};
use crate::blobs::{redact, remember_secret};
use crate::server::settings::Secrets;

/// What a person confirms before a login starts, and what the Models page and `moebius doctor` say of an upstream
/// that logs in with OAuth.
pub const TERMS_WARNING: &str = "Logging in uses your subscription with the provider through Moebius. Using a \
subscription through a third-party tool may break the provider's terms of service, and the provider may suspend the \
account. Read the terms before you log in; an API key is the way the provider supports.";

/// What a request to an upstream that is not logged in fails with.
const NOT_LOGGED_IN: &str = "the upstream is not logged in: log in on the Models page";

/// How long a request to the provider's token or device URL may take.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(30);

/// How long before its expiry an access token is refreshed, at most; a token that lives shorter than twice this is
/// refreshed when half its life is over.
const REFRESH_AHEAD: Duration = Duration::from_secs(60);

/// How long an access token whose answer did not say is taken to live.
const DEFAULT_LIFETIME: Duration = Duration::from_secs(3600);

/// Where a login stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginState {
    /// No token is stored, or the stored one is refused.
    LoggedOut,
    /// A login was started and waits for the person.
    Waiting,
    /// The upstream is logged in and answers with its token.
    LoggedIn,
    /// The login did not finish, or its token is not taken by the upstream.
    Failed,
}

/// The answer of `GET` and `POST /v1/models/upstreams/{place}/login`. No token is ever in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Login {
    pub state: LoginState,
    pub flow: Flow,
    /// For `device`: the code the person types at `verification_uri`.
    pub user_code: Option<String>,
    pub verification_uri: Option<String>,
    /// For `device`, if the provider gives it: the page with the code filled in.
    pub verification_uri_complete: Option<String>,
    /// For `code`: the provider's page, to open in the browser.
    pub authorize_url: Option<String>,
    /// In plain words, where the login stands.
    pub message: String,
}

impl Login {
    fn new(flow: Flow, state: LoginState, message: impl Into<String>) -> Self {
        Self {
            state,
            flow,
            user_code: None,
            verification_uri: None,
            verification_uri_complete: None,
            authorize_url: None,
            message: message.into(),
        }
    }
}

/// Why a login cannot be started or read.
#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    #[error("there is no upstream {0}")]
    NoUpstream(usize),
    #[error("the upstream {0} has no oauth table: it uses a key")]
    NotOAuth(usize),
    #[error("confirm the warning first (accept_terms): {TERMS_WARNING}")]
    TermsNotAccepted,
    #[error("the provider: {0}")]
    Provider(String),
    #[error("the upstream {0} has no login waiting for a code")]
    NotWaiting(usize),
    #[error("{0}")]
    BadLink(String),
}

/// An access token and when to refresh it.
struct Access {
    token: String,
    refresh_at: Instant,
}

/// A login the server runs, numbered so that an older one that ends late does not replace a newer one.
struct Running {
    id: u64,
    login: Login,
    /// For a `code` login that waits: where a pasted code goes.
    paste: Option<code::Paste>,
}

/// The tokens of the upstreams that log in with OAuth, by the name their refresh token is kept under, and the logins
/// that were started.
#[derive(Default)]
pub(super) struct Logins {
    secrets: Option<Secrets>,
    /// The access token of each login, behind a lock of its own that is held while the token is refreshed, so that
    /// one refresh serves every request waiting for it and a slow token URL stalls only its own upstream.
    access: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<Option<Access>>>>>,
    running: std::sync::Mutex<HashMap<String, Running>>,
    /// The task of each login that waits, so that a newer login of the same secret can stop it.
    tasks: std::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    next: std::sync::atomic::AtomicU64,
}

impl Logins {
    /// The slot that holds the access token of the login named `secret`.
    fn slot(&self, secret: &str) -> Arc<tokio::sync::Mutex<Option<Access>>> {
        let mut access = self.access.lock().unwrap_or_else(|p| p.into_inner());
        access.entry(secret.to_string()).or_default().clone()
    }

    /// Forgets every access token kept, so that the next request refreshes.
    #[cfg(test)]
    pub(super) async fn forget_access(&self) {
        let slots: Vec<_> = self
            .access
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .cloned()
            .collect();
        for slot in slots {
            *slot.lock().await = None;
        }
    }
}

/// What a token URL answered. Not `Debug`, so that its tokens cannot be printed by accident.
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

/// A login that waits for the person, and ends with the tokens or a message saying why not.
type Waiting = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Tokens, String>> + Send>>;

/// Why a token URL gave no tokens.
#[derive(Debug)]
enum Refused {
    /// The OAuth `error` code, such as `authorization_pending`, and its description, redacted.
    OAuth(String, Option<String>),
    Other(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::OAuth(code, Some(description)) => write!(f, "{code}: {description}"),
            Refused::OAuth(code, None) => f.write_str(code),
            Refused::Other(message) => f.write_str(message),
        }
    }
}

/// Posts `form` to a token or device URL and reads the JSON it answers, or the OAuth error.
async fn post_form(
    client: &reqwest::Client,
    url: &str,
    form: &[(&str, &str)],
) -> Result<Value, Refused> {
    post(client, url, form, false).await
}

/// Posts `fields` to a token or device URL, as a form or (`json`) as a JSON object, and reads the JSON it answers, or
/// the OAuth error.
async fn post(
    client: &reqwest::Client,
    url: &str,
    fields: &[(&str, &str)],
    json: bool,
) -> Result<Value, Refused> {
    let request = client
        .post(url)
        .header(reqwest::header::ACCEPT, "application/json");
    let request = if json {
        request.json(&fields.iter().copied().collect::<HashMap<_, _>>())
    } else {
        request.form(fields)
    };
    let response = request.timeout(TOKEN_TIMEOUT).send().await.map_err(|e| {
        Refused::Other(if e.is_timeout() {
            "the provider did not answer in time".into()
        } else {
            "the provider could not be reached".into()
        })
    })?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if let Some(code) = body.get("error").and_then(Value::as_str) {
        let description = body
            .get("error_description")
            .and_then(Value::as_str)
            .map(redact);
        return Err(Refused::OAuth(redact(code), description));
    }
    if !status.is_success() {
        return Err(Refused::Other(format!(
            "the provider answered {}",
            status.as_u16()
        )));
    }
    Ok(body)
}

/// Asks a token URL for tokens.
async fn ask_tokens(
    client: &reqwest::Client,
    oauth: &OAuthConfig,
    form: &[(&str, &str)],
) -> Result<Tokens, Refused> {
    let body = post(client, &oauth.token_url, form, oauth.json_tokens()).await?;
    serde_json::from_value(body)
        .map_err(|_| Refused::Other("the provider's answer has no access_token".into()))
}

/// When an access token that lives `lifetime` from now is refreshed.
fn refresh_at(lifetime: Duration) -> Instant {
    Instant::now() + lifetime - REFRESH_AHEAD.min(lifetime / 2)
}

impl Router {
    /// Where the refresh tokens of the upstreams that log in with OAuth are kept; without it, none can log in.
    pub fn with_secrets(mut self, secrets: Secrets) -> Self {
        self.oauth.secrets = Some(secrets);
        self
    }

    /// Puts the upstream's credentials on `request`: the access token of its login, or its key, or nothing.
    pub(super) async fn authenticate(
        &self,
        upstream: &Upstream,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, String> {
        if let Some(oauth) = &upstream.oauth {
            return Ok(request.bearer_auth(self.access_token(oauth).await?));
        }
        Ok(match (&upstream.api_key, upstream.protocol) {
            (Some(key), Protocol::OpenAi) => request.bearer_auth(key),
            (Some(key), Protocol::Anthropic) => request.header("x-api-key", key),
            (None, _) => request,
        })
    }

    /// The access token of a login: the one kept, or a new one for the stored refresh token once the kept one is
    /// due. The message of a failure says nothing secret.
    pub(super) async fn access_token(&self, oauth: &OAuthConfig) -> Result<String, String> {
        let slot = self.oauth.slot(&oauth.secret);
        let mut access = slot.lock().await;
        if let Some(kept) = access.as_ref()
            && Instant::now() < kept.refresh_at
        {
            return Ok(kept.token.clone());
        }
        let secrets = self.oauth.secrets.as_ref().ok_or(NOT_LOGGED_IN)?;
        let refresh = secrets
            .get(&oauth.secret)
            .map_err(|_| "secrets.toml cannot be read".to_string())?
            .ok_or(NOT_LOGGED_IN)?;
        remember_secret(&refresh);
        let tokens = ask_tokens(
            &self.client,
            oauth,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &refresh),
                ("client_id", &oauth.client_id),
            ],
        )
        .await
        .map_err(|refused| {
            *access = None;
            format!("the login could not be refreshed ({refused}): log in again on the Models page")
        })?;
        self.keep(oauth, tokens, &mut access)
    }

    /// Keeps the tokens of an answer: the refresh token in `secrets.toml`, the access token here.
    fn keep(
        &self,
        oauth: &OAuthConfig,
        tokens: Tokens,
        access: &mut Option<Access>,
    ) -> Result<String, String> {
        remember_secret(&tokens.access_token);
        if let Some(refresh) = &tokens.refresh_token {
            remember_secret(refresh);
            self.oauth
                .secrets
                .as_ref()
                .ok_or("there is nowhere to keep the refresh token")?
                .set(&oauth.secret, refresh)
                .map_err(|_| "the refresh token cannot be kept in secrets.toml".to_string())?;
        }
        let lifetime = tokens
            .expires_in
            .map_or(DEFAULT_LIFETIME, Duration::from_secs);
        *access = Some(Access {
            token: tokens.access_token.clone(),
            refresh_at: refresh_at(lifetime),
        });
        Ok(tokens.access_token)
    }

    /// The upstream at `place` in the list and its `oauth` table.
    fn oauth_upstream(&self, place: usize) -> Result<(Upstream, OAuthConfig), LoginError> {
        let upstream = self
            .upstreams
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(place)
            .cloned()
            .ok_or(LoginError::NoUpstream(place))?;
        let oauth = upstream.oauth.clone().ok_or(LoginError::NotOAuth(place))?;
        Ok((upstream, oauth))
    }

    /// Starts a login of the upstream at `place`, once the person has confirmed [`TERMS_WARNING`]
    /// (`terms_accepted`). The answer says what the person does next; the login goes on in the background, and
    /// [`Router::login`] tells how it went.
    pub async fn start_login(
        self: &Arc<Self>,
        place: usize,
        terms_accepted: bool,
    ) -> Result<Login, LoginError> {
        let (upstream, oauth) = self.oauth_upstream(place)?;
        if !terms_accepted {
            return Err(LoginError::TermsNotAccepted);
        }
        if self.oauth.secrets.is_none() {
            return Err(LoginError::Provider(
                "there is nowhere to keep the refresh token".into(),
            ));
        }
        // The login this one replaces stops first, since it holds the redirect port and could still store tokens.
        let previous = self
            .oauth
            .tasks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&oauth.secret);
        if let Some(previous) = previous {
            previous.abort();
            let _ = previous.await;
        }
        let id = self
            .oauth
            .next
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let started = match oauth.flow {
            Flow::Device => device::start(&self.client, &oauth)
                .await
                .map(|(login, waiting)| (login, waiting, None)),
            Flow::Code => code::start(&self.client, &oauth).await,
        };
        let (login, waiting, paste) = match started {
            Ok(started) => started,
            Err(error) => {
                // The login that was stopped must not stay as one that waits.
                self.end_login(
                    &oauth.secret,
                    self.running_id(&oauth.secret),
                    Login::new(
                        oauth.flow,
                        LoginState::Failed,
                        "replaced by a login that did not start",
                    ),
                );
                return Err(error);
            }
        };
        self.set_login(&oauth.secret, id, login.clone(), paste);
        let router = self.clone();
        let secret = oauth.secret.clone();
        let task = tokio::spawn(async move {
            let ended = match waiting.await {
                Ok(tokens) => router.finish(&upstream, &oauth, tokens).await,
                Err(message) => Login::new(oauth.flow, LoginState::Failed, message),
            };
            router.end_login(&oauth.secret, id, ended);
        });
        let replaced = self
            .oauth
            .tasks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(secret, task);
        // A login started at the same moment must not go on, storing tokens, without a handle.
        if let Some(replaced) = replaced {
            replaced.abort();
        }
        Ok(login)
    }

    /// The id of the login recorded for `secret`, or 0.
    fn running_id(&self, secret: &str) -> u64 {
        let running = self.oauth.running.lock().unwrap_or_else(|p| p.into_inner());
        running.get(secret).map_or(0, |r| r.id)
    }

    /// Records `login` as the state of the login `id` that has just started, replacing any other.
    fn set_login(&self, secret: &str, id: u64, login: Login, paste: Option<code::Paste>) {
        let mut running = self.oauth.running.lock().unwrap_or_else(|p| p.into_inner());
        running.insert(secret.to_string(), Running { id, login, paste });
    }

    /// Records `login` as how the login `id` ended, unless another has started since.
    fn end_login(&self, secret: &str, id: u64, login: Login) {
        let mut running = self.oauth.running.lock().unwrap_or_else(|p| p.into_inner());
        if running.get(secret).is_some_and(|r| r.id == id) {
            running.insert(
                secret.to_string(),
                Running {
                    id,
                    login,
                    paste: None,
                },
            );
        }
    }

    /// Finishes the `code` login of the upstream at `place` that waits, with what the person pasted: the code, or the
    /// address the browser ended on (its `code` and `state`). A `state` that is not this login's is refused. The
    /// answer is how the login stands; the exchange of the code goes on in the background.
    pub async fn paste_login(&self, place: usize, pasted: &str) -> Result<Login, LoginError> {
        let (_, oauth) = self.oauth_upstream(place)?;
        let (login, paste) = {
            let running = self.oauth.running.lock().unwrap_or_else(|p| p.into_inner());
            let waiting = running
                .get(&oauth.secret)
                .filter(|r| r.login.state == LoginState::Waiting);
            let paste = waiting.and_then(|r| r.paste.clone());
            (waiting.map(|r| r.login.clone()), paste)
        };
        let (Some(login), Some(paste)) = (login, paste) else {
            return Err(LoginError::NotWaiting(place));
        };
        paste.send(code::parse_link(pasted)?)?;
        Ok(login)
    }

    /// Keeps the tokens a login ended with and tells whether the upstream takes them.
    async fn finish(&self, upstream: &Upstream, oauth: &OAuthConfig, tokens: Tokens) -> Login {
        let kept = {
            let slot = self.oauth.slot(&oauth.secret);
            let mut access = slot.lock().await;
            self.keep(oauth, tokens, &mut access)
        };
        if let Err(message) = kept {
            return Login::new(oauth.flow, LoginState::Failed, message);
        }
        self.holds(upstream, oauth).await
    }

    /// Whether the login of `upstream` holds: whether the upstream lists its models with the login's token.
    async fn holds(&self, upstream: &Upstream, oauth: &OAuthConfig) -> Login {
        if let Err(message) = self.access_token(oauth).await {
            return Login::new(oauth.flow, LoginState::LoggedOut, message);
        }
        if let Some(needs) = oauth.needs() {
            return Login::new(
                oauth.flow,
                LoginState::LoggedIn,
                format!(
                    "Logged in. Requests to this upstream need {needs}, which Moebius does not speak yet; they fail."
                ),
            );
        }
        match self.list_models(upstream).await {
            Ok(ids) => Login::new(
                oauth.flow,
                LoginState::LoggedIn,
                format!(
                    "Logged in: the upstream lists {} {}.",
                    ids.len(),
                    if ids.len() == 1 { "model" } else { "models" }
                ),
            ),
            Err(message) => Login::new(
                oauth.flow,
                LoginState::Failed,
                format!("Logged in, but the upstream refuses the token ({message})."),
            ),
        }
    }

    /// Where the login of the upstream at `place` stands: the one running or failed, or else whether the stored
    /// login holds.
    pub async fn login(&self, place: usize) -> Result<Login, LoginError> {
        let (upstream, oauth) = self.oauth_upstream(place)?;
        let running = self
            .oauth
            .running
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&oauth.secret)
            .map(|r| r.login.clone());
        if let Some(login) = running.filter(|l| l.state != LoginState::LoggedIn) {
            return Ok(login);
        }
        Ok(self.holds(&upstream, &oauth).await)
    }
}

#[cfg(test)]
pub(crate) mod testing;

#[cfg(test)]
mod flows;

#[cfg(test)]
mod tests {
    use super::testing::{FakeProvider, router_with};
    use super::*;

    #[test]
    fn a_table_without_the_url_of_its_flow_is_refused() {
        let mut oauth = super::testing::config("http://127.0.0.1:1", Flow::Device);
        assert_eq!(oauth.problem(), None);
        oauth.device_url = None;
        assert_eq!(
            oauth.problem().unwrap(),
            "device_url is needed by the device flow"
        );
        oauth.flow = Flow::Code;
        assert_eq!(oauth.problem(), None);
        oauth.authorize_url = None;
        assert_eq!(
            oauth.problem().unwrap(),
            "authorize_url is needed by the code flow"
        );
        oauth.authorize_url = Some("ftp://x".into());
        assert!(oauth.problem().unwrap().contains("http or https"));
        oauth.authorize_url = Some("https://x".into());
        oauth.secret = "a b".into();
        assert!(oauth.problem().unwrap().contains("not a name"));
    }

    #[test]
    fn a_token_is_refreshed_a_minute_before_it_expires_or_halfway_through_a_short_life() {
        let now = Instant::now();
        let long = refresh_at(Duration::from_secs(3600)) - now;
        assert!(long >= Duration::from_secs(3540) && long < Duration::from_secs(3541));
        let short = refresh_at(Duration::from_secs(40)) - now;
        assert!(short >= Duration::from_secs(20) && short < Duration::from_secs(21));
    }

    #[tokio::test]
    async fn an_upstream_that_is_not_logged_in_is_refused_without_asking_anyone() {
        let provider = FakeProvider::start(3600).await;
        let (router, _dir) = router_with(&provider, Flow::Device);
        let (upstream, _) = router.oauth_upstream(0).unwrap();
        let refused = router
            .authenticate(&upstream, reqwest::Client::new().get("http://127.0.0.1:1"))
            .await
            .err()
            .unwrap();
        assert_eq!(refused, NOT_LOGGED_IN);
        assert_eq!(provider.token_requests(), 0);
        let login = router.login(0).await.unwrap();
        assert_eq!(login.state, LoginState::LoggedOut);
    }

    #[tokio::test]
    async fn a_login_is_refused_until_the_warning_is_confirmed_and_only_for_an_oauth_upstream() {
        let provider = FakeProvider::start(3600).await;
        let (router, _dir) = router_with(&provider, Flow::Device);
        assert!(matches!(
            router.start_login(0, false).await,
            Err(LoginError::TermsNotAccepted)
        ));
        assert!(matches!(
            router.start_login(1, true).await,
            Err(LoginError::NotOAuth(1))
        ));
        assert!(matches!(
            router.start_login(5, true).await,
            Err(LoginError::NoUpstream(5))
        ));
        assert_eq!(provider.device_requests(), 0);
    }

    #[tokio::test]
    async fn the_access_token_is_kept_until_it_is_due_and_then_refreshed_with_the_rotated_token() {
        // A token that lives an hour is asked for once.
        let provider = FakeProvider::start(3600).await;
        let (router, dir) = router_with(&provider, Flow::Device);
        let secrets = Secrets::new(dir.path());
        secrets.set("provider-login", "refresh-token-0").unwrap();
        let (upstream, oauth) = router.oauth_upstream(0).unwrap();
        assert_eq!(
            router.access_token(&oauth).await.unwrap(),
            provider.access_token(1)
        );
        assert_eq!(
            router.access_token(&oauth).await.unwrap(),
            provider.access_token(1)
        );
        assert_eq!(provider.token_requests(), 1);
        // The provider rotated the refresh token; the new one is kept.
        assert_eq!(
            secrets.get("provider-login").unwrap(),
            Some(provider.refresh_token(1))
        );

        // A token that is due at once is refreshed for every request, each time with the token kept last.
        provider.set_lifetime(0);
        router.oauth.forget_access().await;
        for _ in 0..2 {
            router.access_token(&oauth).await.unwrap();
        }
        assert_eq!(
            provider.refreshed_with(),
            (0..3)
                .map(|n| provider.refresh_token(n))
                .collect::<Vec<_>>()
        );
        // The tokens are redacted wherever they would be written.
        assert_eq!(
            redact(&format!("got {}", provider.refresh_token(3))),
            "got [redacted:secret]"
        );
        assert_eq!(
            redact(&format!("got {}", provider.access_token(3))),
            "got [redacted:secret]"
        );

        // A refused refresh says to log in again and names no token.
        provider.refuse_refresh();
        router.oauth.forget_access().await;
        let message = router
            .authenticate(&upstream, reqwest::Client::new().get("http://127.0.0.1:1"))
            .await
            .err()
            .unwrap();
        assert!(message.contains("invalid_grant"), "{message}");
        assert!(message.contains("log in again"), "{message}");
        assert!(!message.contains("token-"), "{message}");
    }

    #[tokio::test]
    async fn a_refresh_that_hangs_stalls_only_its_own_upstream() {
        let provider = FakeProvider::start(3600).await;
        let (router, dir) = router_with(&provider, Flow::Device);
        Secrets::new(dir.path())
            .set("provider-login", "refresh-token-0")
            .unwrap();
        let (_, oauth) = router.oauth_upstream(0).unwrap();
        let token = router.access_token(&oauth).await.unwrap();
        // Another login is in the middle of its refresh, which holds that login's lock.
        let stalled = router.oauth.slot("other-login");
        let _refreshing = stalled.lock().await;
        let kept = tokio::time::timeout(Duration::from_secs(10), router.access_token(&oauth)).await;
        assert_eq!(
            kept.expect("the cached token waited for another login"),
            Ok(token)
        );
    }

    #[tokio::test]
    async fn a_new_login_stops_the_one_it_replaces_and_takes_its_redirect_port() {
        let provider = FakeProvider::start(3600).await;
        let (router, _dir) = router_with(&provider, Flow::Code);
        // A port that is free now, wanted by every login of this upstream.
        let port = {
            let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            probe.local_addr().unwrap().port()
        };
        let mut upstreams = router.upstreams.read().unwrap().clone();
        upstreams[0].oauth.as_mut().unwrap().redirect_port = Some(port);
        router.set_upstreams(upstreams);
        let first = router.start_login(0, true).await.unwrap();
        assert!(
            first.message.contains("sends you back here"),
            "{}",
            first.message
        );
        let second = router.start_login(0, true).await.unwrap();
        // The second could bind the port: the first had let go of it.
        assert!(
            second.message.contains("sends you back here"),
            "{}",
            second.message
        );
    }

    #[tokio::test]
    async fn a_replacing_login_that_does_not_start_leaves_the_old_one_ended_not_waiting() {
        let provider = FakeProvider::start(3600).await;
        let (router, _dir) = router_with(&provider, Flow::Code);
        let first = router.start_login(0, true).await.unwrap();
        assert_eq!(first.state, LoginState::Waiting);
        let mut upstreams = router.upstreams.read().unwrap().clone();
        upstreams[0].oauth.as_mut().unwrap().authorize_url = Some("not a url".into());
        router.set_upstreams(upstreams);
        assert!(router.start_login(0, true).await.is_err());
        let (_, oauth) = router.oauth_upstream(0).unwrap();
        let running = router.oauth.running.lock().unwrap();
        assert_eq!(running[&oauth.secret].login.state, LoginState::Failed);
        assert!(running[&oauth.secret].paste.is_none());
    }
}
