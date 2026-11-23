//! A person's logins on the forges, kept for them on the server so that taking an issue here assigns them on the forge
//! (ADR 0027).
//!
//! - `GET /v1/logins`: `{"github": string, "gitlab": string, "forgejo": string}` of the person asking; a forge they
//!   have said nothing about is the empty string.
//! - `PUT /v1/logins` `{github, gitlab, forgejo}`: replaces them (a key left out, or blank, is no login) and answers
//!   with them. A login is the name on the forge without an `@`: letters, digits and `-_.[]`.
//!
//! Only a person may ask, never an agent's run token.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::routing::get;
use axum::{Extension, Json, Router};

use super::session::SignedIn;
use axum::http::StatusCode;

use super::{ApiError, ApiState, internal};
use crate::store::LOGIN_FORGES;

/// The routes of a person's logins.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/logins", get(read).put(replace))
}

/// Every forge a login may be kept for, the empty string where there is none.
fn all(mut kept: BTreeMap<String, String>) -> BTreeMap<String, String> {
    for forge in LOGIN_FORGES {
        kept.entry(forge.to_string()).or_default();
    }
    kept
}

async fn read(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
) -> Result<Json<BTreeMap<String, String>>, ApiError> {
    let person = signed_in
        .map(|Extension(s)| s.person)
        .ok_or_else(super::people::not_a_person)?;
    Ok(Json(all(state
        .app
        .store
        .person_logins(person.id)
        .map_err(internal)?)))
}

async fn replace(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    Json(logins): Json<BTreeMap<String, String>>,
) -> Result<Json<BTreeMap<String, String>>, ApiError> {
    let person = signed_in
        .map(|Extension(s)| s.person)
        .ok_or_else(super::people::not_a_person)?;
    let mut kept = BTreeMap::new();
    for (forge, login) in logins {
        if !LOGIN_FORGES.contains(&forge.as_str()) {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!(
                    "{forge:?} is not a forge with logins; they are {}",
                    LOGIN_FORGES.join(", ")
                ),
            ));
        }
        let login = login.trim().trim_start_matches('@');
        if login.is_empty() {
            continue;
        }
        if !login
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.[]".contains(c))
        {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{login:?} is not a login"),
            ));
        }
        kept.insert(forge, login.to_string());
    }
    state
        .app
        .store
        .set_person_logins(person.id, &kept)
        .map_err(internal)?;
    Ok(Json(all(kept)))
}

#[cfg(test)]
mod tests {
    use crate::server::api::TOKEN_PREFIX;
    use crate::server::api::testing::start;
    use crate::store::Role;
    use serde_json::{Value, json};

    async fn call(method: &str, addr: &str, token: &str, body: Option<Value>) -> (u16, Value) {
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), format!("http://{addr}/v1/logins"))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        (
            response.status().as_u16(),
            response.json().await.unwrap_or(Value::Null),
        )
    }

    fn person(state: &crate::server::api::ApiState, handle: &str, n: u64) -> String {
        let store = &state.app.store;
        let p = store.add_person(handle, handle, Role::Member).unwrap();
        let token = format!("{TOKEN_PREFIX}{n:064}");
        store.add_access_token(p.id, "one", &token).unwrap();
        token
    }

    #[tokio::test]
    async fn a_person_keeps_their_logins_which_are_not_shared() {
        let (_d, state, addr) = start().await;
        let lea = person(&state, "lea", 1);
        let bo = person(&state, "bo", 2);
        let none = json!({"github": "", "gitlab": "", "forgejo": ""});
        assert_eq!(call("GET", &addr, &lea, None).await, (200, none.clone()));

        let (status, body) = call(
            "PUT",
            &addr,
            &lea,
            Some(json!({"github": " @lea-gh ", "gitlab": "  "})),
        )
        .await;
        assert_eq!(
            (status, body),
            (
                200,
                json!({"github": "lea-gh", "gitlab": "", "forgejo": ""})
            )
        );
        let (_, body) = call("GET", &addr, &lea, None).await;
        assert_eq!(body["github"], "lea-gh");
        assert_eq!(call("GET", &addr, &bo, None).await, (200, none));

        // A forge without logins, or a name that is none, is refused and changes nothing.
        let (status, _) = call("PUT", &addr, &lea, Some(json!({"svn": "lea"}))).await;
        assert_eq!(status, 422);
        let (status, _) = call("PUT", &addr, &lea, Some(json!({"github": "not a login"}))).await;
        assert_eq!(status, 422);
        let (_, body) = call("GET", &addr, &lea, None).await;
        assert_eq!(body["github"], "lea-gh");
    }
}
