//! Signing in from a browser: the sessions that a cookie names and the one-time codes that start them.
//!
//! - `POST /v1/session`: signs in with a person's access token, the server's token or an unused one-time code, and
//!   answers `204` with the cookie `moebius_session`; this route needs no `Authorization`, since it is how a browser
//!   gets its cookie.
//! - `GET /v1/session`: `200` with the person while the request is signed in (cookie or bearer token), else `401`.
//! - `DELETE /v1/session`: signs out.
//! - `POST /v1/session/codes`: a new one-time code, as the address that opens the web app signed in as the same person.
//!
//! A session lives in the store (`store::access`) for [`SESSION_LIFETIME`] seconds, so it survives a restart; it ends
//! sooner when the person signs out, when the access token it was opened with is revoked, or when the person is
//! disabled. Without `stay`, the cookie has no `Max-Age`, so the browser drops it when its session ends. The one-time codes live in memory for [`CODE_LIFETIME`] seconds. Neither a token, a code nor a session id
//! is ever logged, nor stored but as its hash. What a cookie may do is decided in [`super::authenticate`]; this module
//! holds the codes and the routes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, identify, internal, same};
use super::{ServerOwner, server_owner};
use crate::store::{Person, now};

/// The name of the cookie that holds a session id.
pub(super) const COOKIE: &str = "moebius_session";

/// How long a one-time code can be used, in seconds.
pub const CODE_LIFETIME: i64 = 600;

/// How long a session lasts, in seconds: thirty days.
pub const SESSION_LIFETIME: i64 = 30 * 24 * 60 * 60;

/// The person a request comes from, and the access token they proved it with: none for the server's token, and for a
/// session that a token did not open.
#[derive(Debug, Clone)]
pub(super) struct SignedIn {
    pub person: Person,
    pub token: Option<i64>,
}

/// The one-time codes not used yet, each for the person who will be signed in with it. Cloning gives another handle on
/// the same codes.
#[derive(Clone, Default)]
pub struct Codes(Arc<Mutex<HashMap<String, Code>>>);

/// Whom a code signs in, and the second it expires at.
struct Code {
    person: i64,
    token: Option<i64>,
    expires: i64,
}

impl Codes {
    /// A new one-time code for the person `person` (and the access token `token` they asked with, whose revocation
    /// ends the session), valid for [`CODE_LIFETIME`] seconds from now, with the second it expires at.
    pub fn new_code(
        &self,
        person: i64,
        token: Option<i64>,
    ) -> Result<(String, i64), getrandom::Error> {
        self.new_code_at(person, token, now())
    }

    fn new_code_at(
        &self,
        person: i64,
        token: Option<i64>,
        now: i64,
    ) -> Result<(String, i64), getrandom::Error> {
        let code = random_id()?;
        let expires = now + CODE_LIFETIME;
        let mut codes = self.lock();
        codes.retain(|_, code| code.expires > now);
        codes.insert(
            code.clone(),
            Code {
                person,
                token,
                expires,
            },
        );
        Ok((code, expires))
    }

    /// Uses `code` up, and gives the person and token it signs in, if the code is known and has not expired.
    fn redeem_at(&self, code: &str, now: i64) -> Option<(i64, Option<i64>)> {
        let found = self.lock().remove(code)?;
        (found.expires > now).then_some((found.person, found.token))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Code>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// 32 random bytes in hexadecimal: a session id or a one-time code.
pub(super) fn random_id() -> Result<String, getrandom::Error> {
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

/// How a browser asks to be signed in: with an access token, the server's token or a one-time code.
#[derive(Deserialize)]
struct SignIn {
    token: Option<String>,
    code: Option<String>,
    /// Whether the browser stays signed in after it is closed; without it the cookie lasts for the browser session.
    stay: Option<bool>,
}

fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "a valid token is needed".into())
}

/// The person and access token that `request` signs in, or why not.
fn who_signs_in(state: &ApiState, request: &SignIn) -> Result<(i64, Option<i64>), ApiError> {
    if let Some(token) = &request.token {
        if same(token.as_bytes(), state.token.as_bytes()) {
            return match server_owner(state) {
                ServerOwner::Person(owner) => Ok((owner.id, None)),
                _ => Err(unauthorized()),
            };
        }
        if is_run_token(state, token) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "only a person can do this".into(),
            ));
        }
        let found = state
            .app
            .store
            .person_of_token(token, now())
            .map_err(internal)?;
        return found
            .map(|(person, id)| (person.id, Some(id)))
            .ok_or_else(unauthorized);
    }
    if let Some(code) = &request.code {
        return state.codes.redeem_at(code, now()).ok_or_else(unauthorized);
    }
    Err(unauthorized())
}

async fn sign_in(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<SignIn>,
) -> Result<Response, ApiError> {
    let (person, token) = who_signs_in(&state, &request)?;
    let id = random_id().map_err(internal)?;
    state
        .app
        .store
        .start_session(&id, person, token, now() + SESSION_LIFETIME)
        .map_err(internal)?;
    let secure = if came_over_tls(&headers) {
        "; Secure"
    } else {
        ""
    };
    let lifetime = if request.stay.unwrap_or(true) {
        format!("; Max-Age={SESSION_LIFETIME}")
    } else {
        String::new()
    };
    let cookie = format!("{COOKIE}={id}; HttpOnly; SameSite=Strict; Path=/{lifetime}{secure}");
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

/// What `GET /v1/session` answers: that the request is signed in, and as whom. An agent's run token is signed in as no
/// person.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionStatus {
    pub signed_in: bool,
    pub person: Option<Person>,
}

async fn status(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    // The sign-in status is no route of the issue tracker, which is all a Grilling partner's token may reach.
    match identify(&state, &headers, &axum::http::Method::GET, "/v1/session") {
        Some(who) => Json(SessionStatus {
            signed_in: true,
            person: who.signed_in.map(|signed| signed.person),
        })
        .into_response(),
        None => unauthorized().into_response(),
    }
}

async fn sign_out(State(state): State<ApiState>, headers: HeaderMap) -> Result<Response, ApiError> {
    if let Some(id) = cookie_of(&headers) {
        state.app.store.end_session(id).map_err(internal)?;
    }
    Ok(with_cookie(
        StatusCode::NO_CONTENT,
        &format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"),
    ))
}

/// A one-time code as the address that opens the web app.
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginCode {
    pub url: String,
    pub expires_at: i64,
}

async fn new_code(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    headers: HeaderMap,
) -> Result<Json<LoginCode>, ApiError> {
    let Extension(signed) = signed_in.ok_or_else(super::people::not_a_person)?;
    let (code, expires_at) = state
        .codes
        .new_code(signed.person.id, signed.token)
        .map_err(internal)?;
    let host = host_of(&headers)
        .ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "a Host header is needed".into()))?;
    let scheme = if came_over_tls(&headers) {
        "https"
    } else {
        "http"
    };
    Ok(Json(LoginCode {
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
mod tests;
