//! The device authorization grant (RFC 8628): the provider's device URL gives a code the person types at its page,
//! and the token URL is polled, as often as the provider says, until the person has confirmed it or the code expires.

use std::time::{Duration, Instant};

use serde::Deserialize;

use super::{
    Flow, Login, LoginError, LoginState, OAuthConfig, Refused, Waiting, ask_tokens, post_form,
};

/// The grant type a device code is polled with.
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// How often the token URL is polled when the provider does not say, and how much slower after `slow_down`.
const DEFAULT_INTERVAL: Duration = Duration::from_secs(5);

/// How long a code lasts when the provider does not say.
const DEFAULT_EXPIRY: Duration = Duration::from_secs(900);

/// What the device URL answers. Not `Debug`, since the device code is a secret.
#[derive(Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    /// Some providers name it `verification_url`.
    #[serde(alias = "verification_url")]
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
}

/// Asks the provider for a code, and returns what the person does with it and the polling that waits for them.
pub(super) async fn start(
    client: &reqwest::Client,
    oauth: &OAuthConfig,
) -> Result<(Login, Waiting), LoginError> {
    let device_url = oauth.device_url.clone().unwrap_or_default();
    let scope = oauth.scope();
    let mut form = vec![("client_id", oauth.client_id.as_str())];
    if let Some(scope) = &scope {
        form.push(("scope", scope));
    }
    let answer = post_form(client, &device_url, &form)
        .await
        .map_err(|refused| LoginError::Provider(refused.to_string()))?;
    let code: DeviceCode = serde_json::from_value(answer).map_err(|_| {
        LoginError::Provider(
            "the device URL's answer has no device_code, user_code or verification_uri".into(),
        )
    })?;
    let mut login = Login::new(
        Flow::Device,
        LoginState::Waiting,
        format!(
            "Open {} and enter the code {}.",
            code.verification_uri, code.user_code
        ),
    );
    login.user_code = Some(code.user_code.clone());
    login.verification_uri = Some(code.verification_uri.clone());
    login.verification_uri_complete = code.verification_uri_complete.clone();
    let deadline = Instant::now() + code.expires_in.map_or(DEFAULT_EXPIRY, Duration::from_secs);
    let interval = code.interval.map_or(DEFAULT_INTERVAL, Duration::from_secs);
    let client = client.clone();
    let oauth = oauth.clone();
    let waiting = async move { poll(&client, &oauth, &code.device_code, interval, deadline).await };
    Ok((login, Box::pin(waiting)))
}

/// Polls the token URL every `interval` until it gives tokens, refuses for good, or `deadline` passes.
async fn poll(
    client: &reqwest::Client,
    oauth: &OAuthConfig,
    device_code: &str,
    mut interval: Duration,
    deadline: Instant,
) -> Result<super::Tokens, String> {
    loop {
        tokio::time::sleep(interval).await;
        if Instant::now() >= deadline {
            return Err("The code expired before the login was confirmed. Log in again.".into());
        }
        let asked = ask_tokens(
            client,
            &oauth.token_url,
            &[
                ("grant_type", DEVICE_GRANT),
                ("device_code", device_code),
                ("client_id", &oauth.client_id),
            ],
        )
        .await;
        match asked {
            Ok(tokens) => return Ok(tokens),
            Err(Refused::OAuth(code, _)) if code == "authorization_pending" => {}
            Err(Refused::OAuth(code, _)) if code == "slow_down" => interval += DEFAULT_INTERVAL,
            Err(Refused::OAuth(code, _)) if code == "access_denied" => {
                return Err("The login was declined at the provider.".into());
            }
            Err(Refused::OAuth(code, _)) if code == "expired_token" => {
                return Err(
                    "The code expired before the login was confirmed. Log in again.".into(),
                );
            }
            Err(refused) => return Err(format!("The provider refused the login ({refused}).")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{FakeProvider, router_with, wait_for};
    use super::super::{LoginState, Secrets};
    use super::*;

    #[tokio::test]
    async fn a_device_login_shows_the_code_polls_until_confirmed_and_checks_the_upstream() {
        let provider = FakeProvider::start(3600).await;
        // The person has not confirmed the code for the first poll.
        provider.pending_polls(1);
        let (router, dir) = router_with(&provider, Flow::Device);
        let login = router.start_login(0, true).await.unwrap();
        assert_eq!(login.state, LoginState::Waiting);
        assert_eq!(login.flow, Flow::Device);
        assert_eq!(login.user_code.as_deref(), Some("WDJB-MJHT"));
        assert_eq!(
            login.verification_uri.unwrap(),
            format!("{}/device", provider.url)
        );
        assert!(login.message.contains("WDJB-MJHT"));
        assert_eq!(provider.device_scope(), "models offline");

        let login = wait_for(&router, 0, LoginState::LoggedIn).await;
        assert_eq!(login.message, "Logged in: the upstream lists 2 models.");
        assert_eq!(provider.device_polls(), 2);
        assert_eq!(
            Secrets::new(dir.path())
                .get("provider-login")
                .unwrap()
                .as_deref(),
            Some(provider.refresh_token(1).as_str())
        );
        // The upstream was asked with the access token, never with the refresh token.
        assert_eq!(
            provider.upstream_auth(),
            [format!("Bearer {}", provider.access_token(1))]
        );
        // The answers carry no token.
        let text = serde_json::to_string(&router.login(0).await.unwrap()).unwrap();
        assert!(!text.contains("token-"), "{text}");
    }

    #[tokio::test]
    async fn a_declined_device_login_fails_with_a_message() {
        let provider = FakeProvider::start(3600).await;
        provider.decline_polls();
        let (router, _dir) = router_with(&provider, Flow::Device);
        router.start_login(0, true).await.unwrap();
        let login = wait_for(&router, 0, LoginState::Failed).await;
        assert_eq!(login.message, "The login was declined at the provider.");
    }
}
