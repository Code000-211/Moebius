//! Signing in from a browser: the sessions that a cookie names and the one-time codes that start them.
//!
//! - `POST /v1/session`: signs in with the person's token or with an unused one-time code, and answers `204` with the
//!   cookie `moebius_session`; this route needs no `Authorization`, since it is how a browser gets its cookie.
//! - `GET /v1/session`: `200` while the request is signed in (cookie or bearer token), else `401`.
//! - `DELETE /v1/session`: signs out.
//! - `POST /v1/session/codes`: a new one-time code, as the address that opens the web app signed in.
//!
//! The sessions and the codes live in memory, so a restart signs everybody out. Neither a token, a code nor a session
//! id is ever stored or logged. What a cookie may do is decided in [`super::authenticate`]; this module holds the
//! state and the routes.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::Extension;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, identify, internal, person_only, same};
use crate::decide::Actor;

/// The name of the cookie that holds a session id.
pub(super) const COOKIE: &str = "moebius_session";

/// How long a one-time code can be used, in seconds.
pub const CODE_LIFETIME: i64 = 600;

/// The signed in browsers and the one-time codes not used yet. Cloning gives another handle on the same state.
#[derive(Clone, Default)]
pub struct Sessions(Arc<Mutex<Inner>>);

#[derive(Default)]
struct Inner {
    sessions: HashSet<String>,
    /// Each code with the second it expires at.
    codes: HashMap<String, i64>,
}

impl Sessions {
    /// A new one-time code, valid for [`CODE_LIFETIME`] seconds from now, with the second it expires at.
    pub fn new_code(&self) -> Result<(String, i64), getrandom::Error> {
        self.new_code_at(crate::store::now())
    }

    fn new_code_at(&self, now: i64) -> Result<(String, i64), getrandom::Error> {
        let code = random_id()?;
        let expires = now + CODE_LIFETIME;
        let mut inner = self.lock();
        inner.codes.retain(|_, expires| *expires > now);
        inner.codes.insert(code.clone(), expires);
        Ok((code, expires))
    }

    /// Uses `code` up and starts a session, if the code is known and has not expired.
    fn redeem_at(&self, code: &str, now: i64) -> Option<String> {
        let id = random_id().ok()?;
        let mut inner = self.lock();
        let expires = inner.codes.remove(code)?;
        if expires <= now {
            return None;
        }
        inner.sessions.insert(id.clone());
        Some(id)
    }

    fn start(&self) -> Result<String, getrandom::Error> {
        let id = random_id()?;
        self.lock().sessions.insert(id.clone());
        Ok(id)
    }

    /// Whether `id` names a session that is signed in.
    pub(super) fn has(&self, id: &str) -> bool {
        self.lock().sessions.contains(id)
    }

    fn end(&self, id: &str) {
        self.lock().sessions.remove(id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn random_id() -> Result<String, getrandom::Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(hex::encode(bytes))
}

/// The session id in the request's cookies.
pub(super) fn cookie_of(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value)
}

/// The routes that need no token: they are how a browser gets one.
pub(super) fn public_routes() -> Router<ApiState> {
    Router::new().route("/v1/session", post(sign_in).get(status).delete(sign_out))
}

/// The routes behind the token.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/session/codes", post(new_code))
}

/// How a browser asks to be signed in: with the person's token or with a one-time code.
#[derive(Deserialize)]
struct SignIn {
    token: Option<String>,
    code: Option<String>,
}

fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "a valid token is needed".into())
}

async fn sign_in(
    State(state): State<ApiState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<SignIn>,
) -> Result<Response, ApiError> {
    let id = if let Some(token) = &request.token {
        if same(token.as_bytes(), state.token.as_bytes()) {
            state.sessions.start().map_err(internal)?
        } else if is_run_token(&state, token) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "only a person can do this".into(),
            ));
        } else {
            return Err(unauthorized());
        }
    } else if let Some(code) = &request.code {
        state
            .sessions
            .redeem_at(code, crate::store::now())
            .ok_or_else(unauthorized)?
    } else {
        return Err(unauthorized());
    };
    let secure = if came_over_tls(&headers) {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!("{COOKIE}={id}; HttpOnly; SameSite=Strict; Path=/{secure}");
    Ok(with_cookie(StatusCode::NO_CONTENT, &cookie))
}

/// Whether `token` is the run token of any run.
fn is_run_token(state: &ApiState, token: &str) -> bool {
    let mut headers = HeaderMap::new();
    let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) else {
        return false;
    };
    headers.insert(header::AUTHORIZATION, value);
    state.router.grant(&headers).is_some()
}

/// Whether a proxy in front of the server says the browser used TLS; the server itself serves plain HTTP.
fn came_over_tls(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

fn with_cookie(status: StatusCode, cookie: &str) -> Response {
    let mut response = status.into_response();
    if let Ok(value) = HeaderValue::from_str(cookie) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("set-cookie"), value);
    }
    response
}

async fn status(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    match identify(&state, &headers) {
        Some(_) => axum::Json(serde_json::json!({ "signed_in": true })).into_response(),
        None => unauthorized().into_response(),
    }
}

async fn sign_out(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    if let Some(id) = cookie_of(&headers) {
        state.sessions.end(id);
    }
    with_cookie(
        StatusCode::NO_CONTENT,
        &format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"),
    )
}

/// A one-time code as the address that opens the web app.
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginCode {
    pub url: String,
    pub expires_at: i64,
}

async fn new_code(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    headers: HeaderMap,
) -> Result<axum::Json<LoginCode>, ApiError> {
    person_only(&by)?;
    let (code, expires_at) = state.sessions.new_code().map_err(internal)?;
    let host = host_of(&headers)
        .ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "a Host header is needed".into()))?;
    let scheme = if came_over_tls(&headers) {
        "https"
    } else {
        "http"
    };
    Ok(axum::Json(LoginCode {
        url: login_url(&format!("{scheme}://{host}"), &code),
        expires_at,
    }))
}

/// The `Host` header of a request.
pub(super) fn host_of(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::HOST).and_then(|v| v.to_str().ok())
}

/// The address that opens the web app at `base` signed in with `code`.
pub fn login_url(base: &str, code: &str) -> String {
    format!("{}/?login={code}", base.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_works_once_and_only_until_it_expires() {
        let sessions = Sessions::default();
        let (code, expires) = sessions.new_code_at(1000).unwrap();
        assert_eq!(expires, 1000 + CODE_LIFETIME);
        assert!(
            sessions
                .redeem_at(&code, 1000 + CODE_LIFETIME - 1)
                .is_some()
        );
        assert!(sessions.redeem_at(&code, 1000).is_none());
        let (late, _) = sessions.new_code_at(1000).unwrap();
        assert!(sessions.redeem_at(&late, 1000 + CODE_LIFETIME).is_none());
        assert!(sessions.redeem_at("unknown", 1000).is_none());
    }

    #[test]
    fn the_cookie_is_found_among_others() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("a=b; moebius_session=abc; c=d"),
        );
        assert_eq!(cookie_of(&headers), Some("abc"));
        assert_eq!(cookie_of(&HeaderMap::new()), None);
    }

    use futures_util::StreamExt;
    use reqwest::StatusCode as Status;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    use crate::server::api::testing::start;

    fn http() -> reqwest::Client {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }

    /// Signs in with `body` and gives the cookie (`moebius_session=<id>`) the answer sets.
    async fn sign_in_as(addr: &str, body: serde_json::Value) -> String {
        let response = http()
            .post(format!("http://{addr}/v1/session"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), Status::NO_CONTENT);
        let set = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            set.contains("HttpOnly") && set.contains("SameSite=Strict") && set.contains("Path=/")
        );
        assert!(!set.contains("Secure"));
        set.split(';').next().unwrap().to_string()
    }

    #[tokio::test]
    async fn the_token_signs_in_and_the_cookie_acts_as_a_person() {
        let (_dir, state, addr) = start().await;
        let cookie = sign_in_as(&addr, serde_json::json!({"token": &*state.token})).await;

        let health = http()
            .get(format!("http://{addr}/v1/health"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(health.status(), Status::OK);
        let status = http()
            .get(format!("http://{addr}/v1/session"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(
            status.json::<serde_json::Value>().await.unwrap(),
            serde_json::json!({"signed_in": true})
        );
        // The bearer token says the same.
        let bearer = http()
            .get(format!("http://{addr}/v1/session"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap();
        assert_eq!(bearer.status(), Status::OK);

        // A person-only route accepts the cookie.
        let secrets = http()
            .get(format!("http://{addr}/v1/webhooks"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(secrets.status(), Status::OK);

        // Signing out forgets the session.
        let out = http()
            .delete(format!("http://{addr}/v1/session"))
            .header("cookie", &cookie)
            .header("x-moebius", "1")
            .send()
            .await
            .unwrap();
        assert_eq!(out.status(), Status::NO_CONTENT);
        assert!(
            out.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        let after = http()
            .get(format!("http://{addr}/v1/health"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(after.status(), Status::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_wrong_token_code_or_cookie_is_refused() {
        let (_dir, state, addr) = start().await;
        for body in [
            serde_json::json!({"token": "nope"}),
            serde_json::json!({"code": "nope"}),
            serde_json::json!({}),
        ] {
            let response = http()
                .post(format!("http://{addr}/v1/session"))
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), Status::UNAUTHORIZED);
            assert_eq!(response.text().await.unwrap(), "a valid token is needed");
        }
        for cookie in ["moebius_session=nope", "other=1"] {
            let response = http()
                .get(format!("http://{addr}/v1/health"))
                .header("cookie", cookie)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), Status::UNAUTHORIZED);
        }
        let none = http()
            .get(format!("http://{addr}/v1/session"))
            .send()
            .await
            .unwrap();
        assert_eq!(none.status(), Status::UNAUTHORIZED);
        // A valid cookie does not rescue a wrong bearer token.
        let cookie = sign_in_as(&addr, serde_json::json!({"token": &*state.token})).await;
        let both = http()
            .get(format!("http://{addr}/v1/health"))
            .header("cookie", &cookie)
            .bearer_auth("nope")
            .send()
            .await
            .unwrap();
        assert_eq!(both.status(), Status::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_run_token_never_gets_a_session() {
        let (_dir, state, addr) = start().await;
        let run = state
            .app
            .store
            .add_run(Some(1), "Moebius", "test", "chat")
            .unwrap();
        let agent_token = state.router.issue_token(crate::router::Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        });
        let response = http()
            .post(format!("http://{addr}/v1/session"))
            .json(&serde_json::json!({"token": agent_token}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), Status::FORBIDDEN);
        assert_eq!(response.text().await.unwrap(), "only a person can do this");
        // Nor can it mint a code with it ... it can not: a code opens a person's session.
        let code = http()
            .post(format!("http://{addr}/v1/session/codes"))
            .bearer_auth(&agent_token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(code.status(), Status::FORBIDDEN);
    }

    #[tokio::test]
    async fn a_one_time_code_signs_in_once() {
        let (_dir, state, addr) = start().await;
        let minted: LoginCode = http()
            .post(format!("http://{addr}/v1/session/codes"))
            .bearer_auth(&*state.token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(minted.expires_at > crate::store::now());
        let prefix = format!("http://{addr}/?login=");
        let code = minted
            .url
            .strip_prefix(&prefix)
            .expect("the url holds the code")
            .to_string();

        let cookie = sign_in_as(&addr, serde_json::json!({"code": &code})).await;
        let ok = http()
            .get(format!("http://{addr}/v1/health"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(ok.status(), Status::OK);
        let again = http()
            .post(format!("http://{addr}/v1/session"))
            .json(&serde_json::json!({"code": &code}))
            .send()
            .await
            .unwrap();
        assert_eq!(again.status(), Status::UNAUTHORIZED);

        // The cookie can mint a code too, as a person.
        let by_cookie = http()
            .post(format!("http://{addr}/v1/session/codes"))
            .header("cookie", &cookie)
            .header("x-moebius", "1")
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(by_cookie.status(), Status::OK);
    }

    #[tokio::test]
    async fn a_change_by_cookie_must_come_from_the_web_app() {
        let (_dir, state, addr) = start().await;
        let cookie = sign_in_as(&addr, serde_json::json!({"token": &*state.token})).await;
        let post = |extra: Option<(&'static str, String)>| {
            let mut request = http()
                .post(format!("http://{addr}/v1/session/codes"))
                .header("cookie", &cookie)
                .json(&serde_json::json!({}));
            if let Some((name, value)) = extra {
                request = request.header(name, value);
            }
            request
        };
        let refused = post(None).send().await.unwrap();
        assert_eq!(refused.status(), Status::FORBIDDEN);
        assert_eq!(refused.text().await.unwrap(), "cross-site request refused");

        let foreign = post(Some(("x-moebius", "1".into())))
            .header("origin", "http://evil.example")
            .send()
            .await
            .unwrap();
        assert_eq!(foreign.status(), Status::FORBIDDEN);

        let own = post(Some(("x-moebius", "1".into())))
            .header("origin", format!("http://{addr}"))
            .send()
            .await
            .unwrap();
        assert_eq!(own.status(), Status::OK);

        // The bearer token needs neither.
        let bearer = http()
            .post(format!("http://{addr}/v1/session/codes"))
            .bearer_auth(&*state.token)
            .header("origin", "http://evil.example")
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(bearer.status(), Status::OK);
    }

    #[tokio::test]
    async fn the_cookie_opens_the_event_stream_from_the_apps_own_origin_only() {
        let (_dir, state, addr) = start().await;
        let cookie = sign_in_as(&addr, serde_json::json!({"token": &*state.token})).await;
        let open = |origin: Option<String>, cookie: String| {
            let mut request = format!("ws://{addr}/v1/events")
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("cookie", cookie.parse().unwrap());
            if let Some(origin) = origin {
                request
                    .headers_mut()
                    .insert("origin", origin.parse().unwrap());
            }
            tokio_tungstenite::connect_async(request)
        };
        let (mut ws, _) = open(Some(format!("http://{addr}")), cookie.clone())
            .await
            .unwrap();
        let event = crate::bus::Event::CardMoved {
            card: 1,
            from: "Backlog".into(),
            to: "Not started".into(),
        };
        let received = loop {
            state.app.bus.publish(event.clone());
            if let Ok(Some(message)) =
                tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await
            {
                break message.unwrap();
            }
        };
        let back: crate::bus::AppEvent = serde_json::from_str(received.to_text().unwrap()).unwrap();
        assert_eq!(back, crate::bus::AppEvent::from(event));

        assert!(
            open(Some("http://evil.example".into()), cookie)
                .await
                .is_err()
        );
        assert!(open(None, "moebius_session=nope".into()).await.is_err());
    }

    #[tokio::test]
    async fn the_web_app_is_served_without_a_token_and_the_api_is_not() {
        let (_dir, _state, addr) = start().await;
        for path in ["/", "/cards/3"] {
            let page = http()
                .get(format!("http://{addr}{path}"))
                .send()
                .await
                .unwrap();
            assert_eq!(page.status(), Status::OK, "{path}");
            assert!(
                page.headers()["content-type"]
                    .to_str()
                    .unwrap()
                    .starts_with("text/html")
            );
        }
        let api = http()
            .get(format!("http://{addr}/v1/nothing"))
            .send()
            .await
            .unwrap();
        assert_eq!(api.status(), Status::UNAUTHORIZED);
    }
}
