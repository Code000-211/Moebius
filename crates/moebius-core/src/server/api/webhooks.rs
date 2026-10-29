//! The webhooks of the forges (ADR 0012).
//!
//! - `POST /v1/webhooks/{forge}` (`github`, `gitlab` or `forgejo`): a delivery from the forge. It is the one route that
//!   needs no token, since a forge cannot send one; it carries the forge's signature instead and only does that
//!   check and asks the poll of the forge to run now. Unverified, it is answered with 401 and changes nothing; for a
//!   forge that is not set up, or without a webhook secret, with 404.
//! - `GET /v1/webhooks`: for each forge that is set up, the path of its webhook and its secret, which only a person may
//!   read. `POST /v1/webhooks/{forge}/secret` makes a new secret for the forge, replacing the old one.

use axum::Extension;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use serde::Serialize;

use super::{ApiError, ApiState, blocking, internal, person_only};
use crate::app::AppError;
use crate::blobs::remember_secret;
use crate::decide::Actor;
use crate::server::webhooks::{self, FORGES};

/// The route a forge's deliveries go to, which needs no token.
pub(super) fn public_routes() -> Router<ApiState> {
    Router::new().route("/v1/webhooks/{forge}", post(deliver))
}

/// The routes behind the token.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/webhooks", get(list))
        .route("/v1/webhooks/{forge}/secret", post(new_secret))
}

/// A forge's webhook as the Settings page shows it. Not `Debug`, so that the secret cannot be printed by accident.
#[derive(Serialize)]
struct WebhookView {
    forge: &'static str,
    /// The path on the server that the forge is told to send to.
    path: String,
    /// The header in which the forge proves a delivery.
    header: &'static str,
    /// The secret to set on the forge; none until one is made or stored as `webhook-<forge>`.
    secret: Option<String>,
}

/// Whether the configuration sets up `forge`.
fn configured(state: &ApiState, forge: &str) -> bool {
    let config = state.config.get();
    match forge {
        "github" => config.github.is_some(),
        "gitlab" => config.gitlab.is_some(),
        "forgejo" => config.forgejo.is_some(),
        _ => false,
    }
}

fn view(state: &ApiState, forge: &'static str) -> Result<WebhookView, AppError> {
    Ok(WebhookView {
        forge,
        path: format!("/v1/webhooks/{forge}"),
        header: match forge {
            "github" => "X-Hub-Signature-256",
            "gitlab" => "X-Gitlab-Token",
            _ => "X-Gitea-Signature",
        },
        secret: state.config.secrets().get(&webhooks::secret_name(forge))?,
    })
}

async fn list(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
) -> Result<axum::Json<Vec<WebhookView>>, ApiError> {
    person_only(&by)?;
    let views = FORGES
        .into_iter()
        .filter(|forge| configured(&state, forge))
        .map(|forge| view(&state, forge))
        .collect::<Result<_, _>>()?;
    Ok(axum::Json(views))
}

async fn new_secret(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(forge): UrlPath<String>,
) -> Result<axum::Json<WebhookView>, ApiError> {
    person_only(&by)?;
    let Some(forge) = FORGES
        .into_iter()
        .find(|name| *name == forge && configured(&state, name))
    else {
        return Err(AppError::NotFound(format!("webhooks of {forge}")).into());
    };
    let secret = webhooks::generate().map_err(internal)?;
    let state = state.clone();
    blocking(&state.clone().app, move |_| {
        state
            .config
            .secrets()
            .set(&webhooks::secret_name(forge), &secret)?;
        view(&state, forge)
    })
    .await
}

/// A delivery: checked against the secret of the forge, and then only a request to poll.
async fn deliver(
    State(state): State<ApiState>,
    UrlPath(forge): UrlPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(forge) = FORGES
        .into_iter()
        .find(|name| *name == forge && configured(&state, name))
    else {
        return StatusCode::NOT_FOUND;
    };
    let secret = match state.config.secrets().get(&webhooks::secret_name(forge)) {
        Ok(Some(secret)) => secret,
        Ok(None) => return StatusCode::NOT_FOUND,
        // Failing to read the secret leaves nothing to check against.
        Err(_) => return StatusCode::UNAUTHORIZED,
    };
    remember_secret(&secret);
    if !webhooks::verified(forge, &headers, &body, &secret) {
        return StatusCode::UNAUTHORIZED;
    }
    if let Some(wake) = state.config.webhooks().of(forge) {
        wake.ask(webhooks::repository(forge, &body));
    }
    StatusCode::ACCEPTED
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use sha2::{Digest, Sha256};

    use crate::server::webhooks::{Wake, Webhooks};
    use crate::server::{Server, api, fake_forgejo, fake_github, fake_gitlab, loops};

    const SECRET: &str = "whsec_0123456789abcdef0123456789abcdef";
    const BODY: &str = r#"{"repository":{"full_name":"acme/widgets"},"project":{"path_with_namespace":"acme/widgets"}}"#;

    /// HMAC-SHA256 as hex, which the signing headers of GitHub and Forgejo carry.
    fn hmac_hex(secret: &str, body: &str) -> String {
        // A generated secret is longer than a block of SHA-256, so it is hashed first.
        let mut key = [0u8; 64];
        if secret.len() > 64 {
            key[..32].copy_from_slice(&Sha256::digest(secret));
        } else {
            key[..secret.len()].copy_from_slice(secret.as_bytes());
        }
        let inner = Sha256::new()
            .chain_update(key.map(|b| b ^ 0x36))
            .chain_update(body)
            .finalize();
        let outer = Sha256::new()
            .chain_update(key.map(|b| b ^ 0x5c))
            .chain_update(inner)
            .finalize();
        hex::encode(outer)
    }

    /// The header with which `forge` proves a delivery signed with `secret`.
    fn proof(forge: &str, secret: &str) -> (&'static str, String) {
        match forge {
            "github" => (
                "x-hub-signature-256",
                format!("sha256={}", hmac_hex(secret, BODY)),
            ),
            "gitlab" => ("x-gitlab-token", secret.to_string()),
            _ => ("x-gitea-signature", hmac_hex(secret, BODY)),
        }
    }

    /// Serves the API of `server` on port 0.
    async fn serve(server: &Server) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(api::serve(listener, server.api_state()));
        format!("http://{address}")
    }

    async fn deliver(url: &str, forge: &str, header: Option<(&str, String)>) -> u16 {
        let mut request = reqwest::Client::new()
            .post(format!("{url}/v1/webhooks/{forge}"))
            .header("content-type", "application/json")
            .body(BODY);
        if let Some((name, value)) = header {
            request = request.header(name, value);
        }
        request.send().await.unwrap().status().as_u16()
    }

    fn wake_of<'a>(server: &'a Server, forge: &str) -> &'a Wake {
        let webhooks: &Webhooks = server.live.webhooks();
        webhooks.of(forge).unwrap()
    }

    /// Checks the deliveries to `server`, whose forge is `forge`: unsigned, wrongly signed and signed.
    async fn check_deliveries(server: Server, forge: &str) {
        let url = serve(&server).await;
        let secret_name = format!("webhook-{forge}");

        // Without a secret the endpoint is not there.
        assert_eq!(deliver(&url, forge, Some(proof(forge, SECRET))).await, 404);
        server.live.secrets().set(&secret_name, SECRET).unwrap();

        assert_eq!(deliver(&url, forge, None).await, 401);
        assert_eq!(
            deliver(
                &url,
                forge,
                Some(proof(forge, "whsec_wrongwrongwrongwrongwrong"))
            )
            .await,
            401
        );
        assert!(
            wake_of(&server, forge).is_idle(),
            "an unverified delivery asks for nothing"
        );

        assert_eq!(deliver(&url, forge, Some(proof(forge, SECRET))).await, 202);
        let asked = tokio::time::timeout(Duration::from_secs(20), wake_of(&server, forge).wait())
            .await
            .unwrap();
        assert_eq!(asked, Some(vec!["acme/widgets".to_string()]));

        // Another forge, or none, has no webhook here.
        assert_eq!(
            deliver(&url, "bitbucket", Some(proof(forge, SECRET))).await,
            404
        );
    }

    #[tokio::test]
    async fn github_deliveries_are_verified_by_their_signature() {
        let f = fake_github::Fixture::new().await;
        check_deliveries(f.server, "github").await;
    }

    #[tokio::test]
    async fn gitlab_deliveries_are_verified_by_their_token() {
        let f = fake_gitlab::Fixture::new().await;
        check_deliveries(f.server, "gitlab").await;
    }

    #[tokio::test]
    async fn forgejo_deliveries_are_verified_by_their_signature() {
        let f = fake_forgejo::Fixture::new().await;
        check_deliveries(f.server, "forgejo").await;
    }

    /// Starts the polls of `server`, which poll once at the start and then every 30 seconds, and waits for the card of
    /// `first`; then `second` is made available on the forge, and a signed delivery must bring its card in far less time
    /// than the poll would.
    async fn check_wake(
        server: Arc<Server>,
        forge: &str,
        add_second: impl FnOnce(),
        has_second: impl Fn() -> bool,
    ) {
        let url = serve(&server).await;
        server
            .live
            .secrets()
            .set(&format!("webhook-{forge}"), SECRET)
            .unwrap();
        let mut events = server.app.bus.subscribe();
        tokio::spawn(loops::poll(server.clone()));
        let card_added = async |events: &mut tokio::sync::broadcast::Receiver<_>| loop {
            let event: crate::bus::AppEvent = events.recv().await.unwrap();
            if event.kind == "card_added" {
                break;
            }
        };
        tokio::time::timeout(Duration::from_secs(20), card_added(&mut events))
            .await
            .expect("the first poll runs at the start");

        add_second();
        assert!(!has_second());
        assert_eq!(deliver(&url, forge, Some(proof(forge, SECRET))).await, 202);
        tokio::time::timeout(Duration::from_secs(20), card_added(&mut events))
            .await
            .expect("a verified delivery makes the poll run at once");
        assert!(has_second());
    }

    #[tokio::test]
    async fn a_verified_github_delivery_polls_at_once() {
        use fake_github::{Fixture, issue};
        let f = Fixture::new().await;
        f.fake
            .set_issues("\"v1\"", vec![issue(1, "open", "leon", "")]);
        let (fake, server) = (f.fake.clone(), Arc::new(f.server));
        let (repository, app) = (f.repository.id, server.app.clone());
        check_wake(
            server,
            "github",
            || {
                fake.set_issues(
                    "\"v2\"",
                    vec![issue(1, "open", "leon", ""), issue(2, "open", "leon", "")],
                )
            },
            || app.store.card_by_number(repository, 2).unwrap().is_some(),
        )
        .await;
    }

    #[tokio::test]
    async fn a_verified_gitlab_delivery_polls_at_once() {
        use fake_gitlab::{Fixture, issue};
        let f = Fixture::new().await;
        f.fake
            .set_issues("W/\"v1\"", vec![issue(1, "opened", "leon", "")]);
        let (fake, server) = (f.fake.clone(), Arc::new(f.server));
        let (repository, app) = (f.repository.id, server.app.clone());
        check_wake(
            server,
            "gitlab",
            || {
                fake.set_issues(
                    "W/\"v2\"",
                    vec![
                        issue(1, "opened", "leon", ""),
                        issue(2, "opened", "leon", ""),
                    ],
                )
            },
            || app.store.card_by_number(repository, 2).unwrap().is_some(),
        )
        .await;
    }

    #[tokio::test]
    async fn a_verified_forgejo_delivery_polls_at_once() {
        use fake_forgejo::{Fixture, issue};
        let f = Fixture::new().await;
        f.fake
            .set_issues("W/\"v1\"", vec![issue(1, "open", "leon", "")]);
        let (fake, server) = (f.fake.clone(), Arc::new(f.server));
        let (repository, app) = (f.repository.id, server.app.clone());
        check_wake(
            server,
            "forgejo",
            || {
                fake.set_issues(
                    "W/\"v2\"",
                    vec![issue(1, "open", "leon", ""), issue(2, "open", "leon", "")],
                )
            },
            || app.store.card_by_number(repository, 2).unwrap().is_some(),
        )
        .await;
    }

    #[tokio::test]
    async fn a_person_reads_and_makes_the_secrets_with_the_token() {
        let f = fake_github::Fixture::new().await;
        let server = f.server;
        let url = serve(&server).await;
        let client = reqwest::Client::new();
        let get = || client.get(format!("{url}/v1/webhooks"));

        assert_eq!(get().send().await.unwrap().status(), 401);
        let views: serde_json::Value = get()
            .bearer_auth(&*server.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            views,
            serde_json::json!([{ "forge": "github", "path": "/v1/webhooks/github",
                "header": "X-Hub-Signature-256", "secret": null }])
        );

        let made = |forge: &str| {
            client
                .post(format!("{url}/v1/webhooks/{forge}/secret"))
                .bearer_auth(&*server.token)
                .send()
        };
        assert_eq!(made("gitlab").await.unwrap().status(), 404);
        let view: serde_json::Value = made("github").await.unwrap().json().await.unwrap();
        let secret = view["secret"].as_str().unwrap().to_string();
        assert!(secret.starts_with("whsec_"));
        assert_eq!(
            server.live.secrets().get("webhook-github").unwrap(),
            Some(secret.clone())
        );
        let views: serde_json::Value = get()
            .bearer_auth(&*server.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(views[0]["secret"], secret.as_str());
        // The secret made is the one deliveries are checked against.
        assert_eq!(
            deliver(&url, "github", Some(proof("github", &secret))).await,
            202
        );
    }
}
