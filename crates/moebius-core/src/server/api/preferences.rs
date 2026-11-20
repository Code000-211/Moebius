//! What a person chose for themselves in the web app, kept on the server so that it follows them to every browser.
//!
//! - `GET /v1/preferences`: `{"show_thinking_expanded": bool}` of the person asking; `false` until they choose.
//! - `PUT /v1/preferences` `{show_thinking_expanded}`: replaces them and answers with them.
//!
//! Only a person may ask, never an agent's run token.

use axum::extract::State;
use axum::routing::get;
use axum::{Extension, Json, Router};

use super::session::SignedIn;
use super::{ApiError, ApiState, internal};
use crate::store::Preferences;

/// The routes of a person's preferences.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/preferences", get(read).put(replace))
}

async fn read(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
) -> Result<Json<Preferences>, ApiError> {
    let person = signed_in
        .map(|Extension(s)| s.person)
        .ok_or_else(super::people::not_a_person)?;
    Ok(Json(
        state.app.store.preferences(person.id).map_err(internal)?,
    ))
}

async fn replace(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    Json(preferences): Json<Preferences>,
) -> Result<Json<Preferences>, ApiError> {
    let person = signed_in
        .map(|Extension(s)| s.person)
        .ok_or_else(super::people::not_a_person)?;
    state
        .app
        .store
        .set_preferences(person.id, preferences)
        .map_err(internal)?;
    Ok(Json(preferences))
}

#[cfg(test)]
mod tests {
    use crate::server::api::TOKEN_PREFIX;
    use crate::server::api::testing::start;
    use crate::store::Role;
    use serde_json::{Value, json};

    async fn call(method: &str, addr: &str, token: &str, body: Option<Value>) -> (u16, Value) {
        let mut request = reqwest::Client::new()
            .request(
                method.parse().unwrap(),
                format!("http://{addr}/v1/preferences"),
            )
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
    async fn the_choice_is_off_at_first_kept_for_the_person_and_not_shared() {
        let (_d, state, addr) = start().await;
        let lea = person(&state, "lea", 1);
        let bo = person(&state, "bo", 2);
        let (status, body) = call("GET", &addr, &lea, None).await;
        assert_eq!(
            (status, body),
            (200, json!({"show_thinking_expanded": false}))
        );
        let (status, body) = call(
            "PUT",
            &addr,
            &lea,
            Some(json!({"show_thinking_expanded": true})),
        )
        .await;
        assert_eq!(
            (status, body),
            (200, json!({"show_thinking_expanded": true}))
        );
        let (_, body) = call("GET", &addr, &lea, None).await;
        assert_eq!(body, json!({"show_thinking_expanded": true}));
        let (_, body) = call("GET", &addr, &bo, None).await;
        assert_eq!(body, json!({"show_thinking_expanded": false}));
        call(
            "PUT",
            &addr,
            &lea,
            Some(json!({"show_thinking_expanded": false})),
        )
        .await;
        let (_, body) = call("GET", &addr, &lea, None).await;
        assert_eq!(body, json!({"show_thinking_expanded": false}));
    }
}
