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

/// Where the person's pasted code goes while a login waits.
#[derive(Clone)]
pub(super) struct Paste {
    state: String,
    sender: mpsc::Sender<HashMap<String, String>>,
}

impl Paste {
    /// Hands the login what was pasted: `link` as [`parse_link`] read it. A `state` that is not this login's is
    /// refused; one that is missing is taken to be it, as the code page of a provider shows only the code.
    pub(super) fn send(&self, mut link: HashMap<String, String>) -> Result<(), LoginError> {
        match link.get("state") {
            Some(state) if *state != self.state => {
                return Err(LoginError::BadLink(
                    "That is not the answer to this login: its state differs. Log in again, or paste the address \
                     of the login that is waiting."
                        .into(),
                ));
            }
            _ => link.insert("state".into(), self.state.clone()),
        };
        self.sender
            .try_send(link)
            .map_err(|_| LoginError::BadLink("The login does not take a code any more.".into()))
    }
}

/// What a person pasted to finish a login, as the query of a redirect: the address the browser ended on (its query),
/// the query alone, `code#state` as a code page shows it, or the code alone. Fails without a `code` or an `error`.
/// The words of the failure never contain what was pasted.
pub(super) fn parse_link(pasted: &str) -> Result<HashMap<String, String>, LoginError> {
    let pasted = pasted.trim();
    let pairs = |url: &str| -> HashMap<String, String> {
        reqwest::Url::parse(url)
            .map(|url| {
                url.query_pairs()
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut link = if pasted.contains("://") {
        pairs(pasted)
    } else if let Some((code, state)) = pasted.split_once('#') {
        HashMap::from([
            ("code".to_string(), code.to_string()),
            ("state".to_string(), state.to_string()),
        ])
    } else if pasted.contains("code=") || pasted.contains("error=") {
        pairs(&format!("http://x/?{}", pasted.trim_start_matches('?')))
    } else {
        HashMap::from([("code".to_string(), pasted.to_string())])
    };
    link.retain(|key, value| {
        matches!(key.as_str(), "code" | "state" | "error") && !value.is_empty()
    });
    if link.contains_key("code") || link.contains_key("error") {
        if let Some(code) = link.get("code") {
            crate::blobs::remember_secret(code);
        }
        Ok(link)
    } else {
        Err(LoginError::BadLink(
            "There is no code in what was pasted. Paste the code the provider shows, or the whole address the \
             browser ended on."
                .into(),
        ))
    }
}

/// Binds the loopback address for the browser: the port the table or the preset names, and when that one is taken
/// another, if the provider takes any (`any_port`). `None` when the browser cannot be listened for, so that only a
/// pasted code can finish the login. Not listening is an error only when no port was asked for.
async fn listen(oauth: &OAuthConfig) -> Result<(Option<tokio::net::TcpListener>, u16), LoginError> {
    let wanted = oauth.redirect_port.unwrap_or(0);
    let bind = |port| tokio::net::TcpListener::bind(("127.0.0.1", port));
    let any = oauth.preset().is_some_and(|p| p.any_port);
    let bound = match bind(wanted).await {
        Err(_) if any && wanted != 0 => bind(0).await,
        other => other,
    };
    match bound {
        Ok(listener) => {
            let port = listener
                .local_addr()
                .map_err(|e| LoginError::Provider(e.to_string()))?
                .port();
            Ok((Some(listener), port))
        }
        Err(_) if wanted != 0 => Ok((None, wanted)),
        Err(e) => Err(LoginError::Provider(format!(
            "cannot listen for the browser: {e}"
        ))),
    }
}

/// Listens on the loopback address for the browser, and returns the provider's page to open, the wait for the
/// browser's return (or a pasted code) and the exchange of its code, and where a pasted code goes.
pub(super) async fn start(
    client: &reqwest::Client,
    oauth: &OAuthConfig,
) -> Result<(Login, Waiting, Option<Paste>), LoginError> {
    let (listener, port) = listen(oauth).await?;
    let path = oauth
        .redirect_path
        .clone()
        .unwrap_or_else(|| DEFAULT_PATH.to_string());
    let host = oauth.preset().map_or("127.0.0.1", |p| p.redirect_host);
    let redirect_uri = format!("http://{host}:{port}{path}");
    let verifier = random(32)?;
    // Anthropic's page refuses a shorter state as "Invalid request format"; Claude Code sends 32 bytes too.
    let state = random(32)?;
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
    for (name, value) in oauth.preset().map_or(&[][..], |p| p.authorize_params) {
        params.push((name, value.to_string()));
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
        if listener.is_some() {
            "Log in at the provider's page in the browser; it sends you back here. If the browser cannot reach \
             this machine, paste the code or the address it ends on."
        } else {
            "Log in at the provider's page in the browser, then paste the code or the address it ends on."
        },
    );
    login.authorize_url = Some(authorize_url);

    let (sender, mut back) = mpsc::channel::<HashMap<String, String>>(4);
    let paste = Paste {
        state: state.clone(),
        sender: sender.clone(),
    };
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
        let serve = async {
            match listener {
                Some(listener) => {
                    let _ = axum::serve(listener, app).await;
                }
                None => std::future::pending::<()>().await,
            }
        };
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
            _ = serve => return Err("The listener for the browser stopped.".into()),
        };
        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect_uri),
            ("client_id", &oauth.client_id),
            ("code_verifier", &verifier),
        ];
        // A provider that takes JSON wants the state with the code.
        if oauth.json_tokens() {
            form.push(("state", &state));
        }
        ask_tokens(&client, &oauth, &form)
            .await
            .map_err(|refused| format!("The provider refused the code ({refused})."))
    };
    Ok((login, Box::pin(waiting), Some(paste)))
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
        // 32 random bytes, as Claude Code sends: Anthropic refuses a state of 16.
        assert_eq!(asked["state"].len(), 43);
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

    /// The `state` and loopback address of a waiting login, and what the browser would be sent back to.
    async fn waiting(router: &crate::router::Router) -> (String, String) {
        let login = router.login(0).await.unwrap();
        let asked = query(&login.authorize_url.unwrap());
        (asked["state"].clone(), asked["redirect_uri"].clone())
    }

    #[test]
    fn what_is_pasted_is_read_as_an_address_a_query_a_code_with_its_state_or_a_code() {
        let link = |pasted| parse_link(pasted).unwrap();
        let expect = |pasted, code: &str, state: Option<&str>| {
            let got = link(pasted);
            assert_eq!(got["code"], code, "{pasted}");
            assert_eq!(got.get("state").map(String::as_str), state, "{pasted}");
        };
        expect(
            "http://localhost:1455/auth/callback?code=c1&state=s1",
            "c1",
            Some("s1"),
        );
        expect(
            "  http://localhost:1/cb?state=s1&code=c%2F1&scope=x\n",
            "c/1",
            Some("s1"),
        );
        expect("code=c1&state=s1", "c1", Some("s1"));
        expect("?code=c1", "c1", None);
        expect("c1#s1", "c1", Some("s1"));
        expect("c1", "c1", None);
        assert_eq!(
            link("http://localhost:1/cb?error=access_denied&state=s1")["error"],
            "access_denied"
        );
        for nothing in [
            "",
            "   ",
            "http://localhost:1/cb",
            "http://localhost:1/cb?state=s1",
        ] {
            let refused = parse_link(nothing).unwrap_err().to_string();
            assert!(refused.starts_with("There is no code"), "{refused}");
        }
    }

    #[tokio::test]
    async fn a_pasted_address_finishes_a_login_the_browser_could_not_finish_and_a_wrong_state_is_refused()
     {
        let provider = FakeProvider::start(3600).await;
        let (router, _dir) = router_with(&provider, Flow::Code);
        let started = router.start_login(0, true).await.unwrap();
        let (state, redirect) = waiting(&router).await;
        assert!(started.message.contains("paste"));

        // The answer of another login is refused at once, and the login goes on waiting.
        let refused = router
            .paste_login(0, &format!("{redirect}?code=stolen&state=other"))
            .await
            .unwrap_err();
        assert!(matches!(refused, LoginError::BadLink(_)), "{refused}");
        assert!(!refused.to_string().contains("stolen"));
        assert!(matches!(
            router.paste_login(0, "").await,
            Err(LoginError::BadLink(_))
        ));
        assert_eq!(router.login(0).await.unwrap().state, LoginState::Waiting);
        assert!(provider.exchanged_codes().is_empty());

        // The whole address the browser ended on.
        let answered = router
            .paste_login(0, &format!("{redirect}?code=pasted-code&state={state}"))
            .await
            .unwrap();
        assert_eq!(answered.state, LoginState::Waiting);
        let login = wait_for(&router, 0, LoginState::LoggedIn).await;
        assert_eq!(login.message, "Logged in: the upstream lists 2 models.");
        let (code, _, redirect_uri) = &provider.exchanged_codes()[0];
        assert_eq!(code, "pasted-code");
        assert_eq!(redirect_uri, &redirect);
        // The login ended: there is nothing to paste to.
        assert!(matches!(
            router.paste_login(0, "again").await,
            Err(LoginError::NotWaiting(0))
        ));
    }

    #[tokio::test]
    async fn a_code_alone_or_with_its_state_finishes_a_login_and_a_device_login_takes_no_code() {
        for pasted in ["bare-code", "page-code#STATE"] {
            let provider = FakeProvider::start(3600).await;
            let (router, _dir) = router_with(&provider, Flow::Code);
            router.start_login(0, true).await.unwrap();
            let (state, _) = waiting(&router).await;
            router
                .paste_login(0, &pasted.replace("STATE", &state))
                .await
                .unwrap();
            wait_for(&router, 0, LoginState::LoggedIn).await;
            assert_eq!(
                provider.exchanged_codes()[0].0,
                pasted.split('#').next().unwrap()
            );
        }
        let provider = FakeProvider::start(3600).await;
        let (router, _dir) = router_with(&provider, Flow::Device);
        assert!(matches!(
            router.paste_login(0, "code").await,
            Err(LoginError::NotWaiting(0))
        ));
        router.start_login(0, true).await.unwrap();
        assert!(matches!(
            router.paste_login(0, "code").await,
            Err(LoginError::NotWaiting(0))
        ));
    }

    #[tokio::test]
    async fn a_login_whose_port_is_taken_waits_for_a_pasted_code_instead() {
        let provider = FakeProvider::start(3600).await;
        let (router, _dir) = router_with(&provider, Flow::Code);
        let taken = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = taken.local_addr().unwrap().port();
        let (mut upstream, mut oauth) = router.oauth_upstream(0).unwrap();
        oauth.redirect_port = Some(port);
        upstream.oauth = Some(oauth);
        router.set_upstreams(vec![upstream]);
        let login = router.start_login(0, true).await.unwrap();
        assert!(
            login
                .message
                .starts_with("Log in at the provider's page in the browser, then paste")
        );
        let (state, redirect) = waiting(&router).await;
        assert!(
            redirect.ends_with(&format!(":{port}/callback")),
            "{redirect}"
        );
        router
            .paste_login(0, &format!("the-code#{state}"))
            .await
            .unwrap();
        wait_for(&router, 0, LoginState::LoggedIn).await;
    }
}
