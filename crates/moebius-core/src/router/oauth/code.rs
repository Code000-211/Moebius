//! An authorization code with PKCE (RFC 7636) and a loopback redirect (RFC 8252): the provider's page is opened in the
//! browser with a challenge made from a secret verifier, the provider sends the browser back to a listener on
//! `127.0.0.1`, and the code it brings is exchanged at the token URL together with the verifier.

use std::collections::HashMap;
use std::time::Duration;

use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use super::{Flow, Login, LoginError, LoginState, OAuthConfig, Waiting, ask_tokens};

/// How long the browser has to come back.
const LOGIN_TIME: Duration = Duration::from_secs(10 * 60);

/// The page the browser is sent back to when the configuration names none.
const DEFAULT_PATH: &str = "/callback";

/// What the browser is shown once it is back.
const BACK: &str = "<!doctype html><title>Moebius</title><p>The login is back with Moebius. You can close this tab.</p>";

/// `bytes` random bytes in URL-safe base64.
fn random(bytes: usize) -> Result<String, LoginError> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer)
        .map_err(|_| LoginError::Provider("no random numbers for the login".into()))?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// The S256 challenge of a verifier.
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Listens on the loopback address for the browser, and returns the provider's page to open and the wait for the
/// browser's return and the exchange of its code.
pub(super) async fn start(
    client: &reqwest::Client,
    oauth: &OAuthConfig,
) -> Result<(Login, Waiting), LoginError> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", oauth.redirect_port.unwrap_or(0)))
        .await
        .map_err(|e| LoginError::Provider(format!("cannot listen for the browser: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| LoginError::Provider(e.to_string()))?
        .port();
    let path = oauth
        .redirect_path
        .clone()
        .unwrap_or_else(|| DEFAULT_PATH.to_string());
    let redirect_uri = format!("http://127.0.0.1:{port}{path}");
    let verifier = random(32)?;
    let state = random(16)?;
    let mut params = vec![
        ("response_type", "code".to_string()),
        ("client_id", oauth.client_id.clone()),
        ("redirect_uri", redirect_uri.clone()),
        ("state", state.clone()),
        ("code_challenge", challenge(&verifier)),
        ("code_challenge_method", "S256".to_string()),
    ];
    if let Some(scope) = oauth.scope() {
        params.push(("scope", scope));
    }
    let authorize_url = reqwest::Url::parse_with_params(
        oauth.authorize_url.as_deref().unwrap_or_default(),
        &params,
    )
    .map_err(|_| LoginError::Provider("authorize_url is not a URL".into()))?
    .to_string();
    let mut login = Login::new(
        Flow::Code,
        LoginState::Waiting,
        "Log in at the provider's page in the browser; it sends you back here.",
    );
    login.authorize_url = Some(authorize_url);

    let (sender, mut back) = mpsc::channel::<HashMap<String, String>>(4);
    let app = axum::Router::new().route(
        path.as_str(),
        get(move |Query(query): Query<HashMap<String, String>>| {
            let sender = sender.clone();
            async move {
                let _ = sender.send(query).await;
                Html(BACK)
            }
        }),
    );
    let client = client.clone();
    let oauth = oauth.clone();
    let waiting = async move {
        let server = axum::serve(listener, app);
        let wait = async {
            loop {
                let Some(query) = back.recv().await else {
                    return Err("The browser did not come back.".to_string());
                };
                // A request that is not the provider's answer to this login is ignored.
                if query.get("state") != Some(&state) {
                    continue;
                }
                if let Some(error) = query.get("error") {
                    return Err(match error.as_str() {
                        "access_denied" => "The login was declined at the provider.".to_string(),
                        other => format!(
                            "The provider refused the login ({}).",
                            crate::blobs::redact(other)
                        ),
                    });
                }
                if let Some(code) = query.get("code") {
                    return Ok(code.clone());
                }
            }
        };
        let code = tokio::select! {
            code = tokio::time::timeout(LOGIN_TIME, wait) => {
                code.unwrap_or_else(|_| Err("The browser did not come back in ten minutes. Log in again.".into()))?
            }
            _ = server => return Err("The listener for the browser stopped.".into()),
        };
        ask_tokens(
            &client,
            &oauth.token_url,
            &[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", &redirect_uri),
                ("client_id", &oauth.client_id),
                ("code_verifier", &verifier),
            ],
        )
        .await
        .map_err(|refused| format!("The provider refused the code ({refused})."))
    };
    Ok((login, Box::pin(waiting)))
}

#[cfg(test)]
mod tests {
    use super::super::testing::{FakeProvider, router_with, wait_for};
    use super::*;

    #[test]
    fn the_challenge_is_the_base64_of_the_verifiers_sha256() {
        // The example of RFC 7636, appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    /// The query of `url` as a map.
    fn query(url: &str) -> HashMap<String, String> {
        reqwest::Url::parse(url)
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    #[tokio::test]
    async fn a_code_login_sends_the_browser_back_to_a_loopback_listener_and_exchanges_the_code() {
        let provider = FakeProvider::start(3600).await;
        let (router, _dir) = router_with(&provider, Flow::Code);
        let login = router.start_login(0, true).await.unwrap();
        assert_eq!(login.state, LoginState::Waiting);
        let opened = login.authorize_url.unwrap();
        assert!(opened.starts_with(&format!("{}/authorize?", provider.url)));
        let asked = query(&opened);
        assert_eq!(asked["response_type"], "code");
        assert_eq!(asked["client_id"], "moebius-test");
        assert_eq!(asked["code_challenge_method"], "S256");
        assert_eq!(asked["scope"], "models offline");
        let redirect = asked["redirect_uri"].clone();
        assert!(redirect.starts_with("http://127.0.0.1:"), "{redirect}");
        assert!(redirect.ends_with("/callback"), "{redirect}");

        // The browser comes back: first with a state of another login, which is ignored, then with this one's.
        let browser = reqwest::Client::new();
        browser
            .get(format!("{redirect}?code=stolen&state=other"))
            .send()
            .await
            .unwrap();
        let page = browser
            .get(format!("{redirect}?code=the-code&state={}", asked["state"]))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(page.contains("You can close this tab"));

        let login = wait_for(&router, 0, LoginState::LoggedIn).await;
        assert_eq!(login.message, "Logged in: the upstream lists 2 models.");
        // The code was exchanged once, with the verifier whose challenge the browser carried.
        let exchanged = provider.exchanged_codes();
        assert_eq!(exchanged.len(), 1);
        let (code, verifier, redirect_uri) = &exchanged[0];
        assert_eq!(code, "the-code");
        assert_eq!(challenge(verifier), asked["code_challenge"]);
        assert_eq!(redirect_uri, &redirect);
    }

    #[tokio::test]
    async fn a_code_login_declined_at_the_provider_fails() {
        let provider = FakeProvider::start(3600).await;
        let (router, _dir) = router_with(&provider, Flow::Code);
        let login = router.start_login(0, true).await.unwrap();
        let asked = query(&login.authorize_url.unwrap());
        reqwest::Client::new()
            .get(format!(
                "{}?error=access_denied&state={}",
                asked["redirect_uri"], asked["state"]
            ))
            .send()
            .await
            .unwrap();
        let login = wait_for(&router, 0, LoginState::Failed).await;
        assert_eq!(login.message, "The login was declined at the provider.");
        assert!(provider.exchanged_codes().is_empty());
    }
}
