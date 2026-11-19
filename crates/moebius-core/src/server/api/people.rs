//! The people of the organisation and their access tokens (ADR 0018).
//!
//! - `GET /v1/people`: every person, disabled ones included.
//! - `POST /v1/people` `{handle, name, role}`: an owner adds a person, and gets their first access token, named
//!   `first token`, to hand over.
//! - `GET /v1/people/{id}/tokens`: a person's access tokens with when each was last used; an owner's or the person's
//!   own.
//! - `GET /v1/tokens`, `POST /v1/tokens` `{name}`: the tokens of the person asking, and a new one for them.
//! - `DELETE /v1/tokens/{id}`: revokes a token, the asker's own or, for an owner, anyone's; the sessions it opened end
//!   with it.
//!
//! Only a person may ask, never an agent's run token. A new token is in the answer that made it and nowhere else: the
//! store keeps its hash (`store::access`). A token is `moebius-access-` and 64 hexadecimal digits, a shape that
//! `blobs::redact` knows, so one pasted into a chat or a log is redacted.

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

use super::session::{SignedIn, random_id};
use super::{ApiError, ApiState, internal};
use crate::store::{AccessToken, Person, Role};

/// What every access token starts with.
pub const TOKEN_PREFIX: &str = "moebius-access-";

/// The name of the token an owner gets when adding a person.
const FIRST_TOKEN: &str = "first token";

/// The most characters a handle may have.
const HANDLE_LIMIT: usize = 39;

/// The most characters a display name or a token's name may have.
const NAME_LIMIT: usize = 100;

/// The routes of the people and their tokens.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/people", get(people).post(add_person))
        .route("/v1/people/{id}/tokens", get(tokens_of))
        .route("/v1/tokens", get(own_tokens).post(new_own_token))
        .route("/v1/tokens/{id}", delete(revoke))
}

/// A new access token: `moebius-access-` and 32 random bytes in hexadecimal.
pub(super) fn new_token() -> Result<String, getrandom::Error> {
    Ok(format!("{TOKEN_PREFIX}{}", random_id()?))
}

/// The refusal of a request that no person made.
pub(super) fn not_a_person() -> ApiError {
    ApiError(StatusCode::FORBIDDEN, "only a person can do this".into())
}

fn not_an_owner() -> ApiError {
    ApiError(StatusCode::FORBIDDEN, "only an owner can do this".into())
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::UNPROCESSABLE_ENTITY, message.into())
}

/// The person who asked, or the refusal of an agent.
fn asker(signed_in: Option<Extension<SignedIn>>) -> Result<SignedIn, ApiError> {
    signed_in.map(|Extension(s)| s).ok_or_else(not_a_person)
}

/// A token as its person is shown it once: the record, and the token itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuedToken {
    #[serde(flatten)]
    pub record: AccessToken,
    pub token: String,
}

/// What `POST /v1/people` takes.
#[derive(Debug, Deserialize)]
pub struct NewPerson {
    pub handle: String,
    pub name: String,
    pub role: Role,
}

/// What `POST /v1/people` answers: the person and their first token.
#[derive(Debug, Serialize, Deserialize)]
pub struct AddedPerson {
    pub person: Person,
    pub token: IssuedToken,
}

/// What `POST /v1/tokens` takes.
#[derive(Debug, Deserialize)]
pub struct NewToken {
    pub name: String,
}

/// A handle as it is kept: one to [`HANDLE_LIMIT`] lowercase letters, digits, `-` and `_`, starting with a letter or a
/// digit, without the `@`.
fn handle(text: &str) -> Result<String, ApiError> {
    let handle = text.trim().trim_start_matches('@');
    let fits = !handle.is_empty()
        && handle.len() <= HANDLE_LIMIT
        && handle.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && handle
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if fits {
        Ok(handle.to_string())
    } else {
        Err(invalid(format!(
            "a handle is 1 to {HANDLE_LIMIT} lowercase letters, digits, - and _, starting with a letter or a digit"
        )))
    }
}

/// A display name or a token's name: trimmed, not empty, not long.
fn name(what: &str, text: &str) -> Result<String, ApiError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(invalid(format!("the {what} is empty")));
    }
    if text.chars().count() > NAME_LIMIT {
        return Err(invalid(format!(
            "the {what} is longer than {NAME_LIMIT} characters"
        )));
    }
    Ok(text.to_string())
}

/// Makes a token named `name` for the person `person` and keeps its hash.
fn issue(state: &ApiState, person: i64, name: &str) -> Result<IssuedToken, ApiError> {
    let token = new_token().map_err(internal)?;
    let record = state
        .app
        .store
        .add_access_token(person, name, &token)
        .map_err(internal)?;
    Ok(IssuedToken { record, token })
}

async fn people(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
) -> Result<Json<Vec<Person>>, ApiError> {
    asker(signed_in)?;
    Ok(Json(state.app.store.people().map_err(internal)?))
}

async fn add_person(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    Json(new): Json<NewPerson>,
) -> Result<Json<AddedPerson>, ApiError> {
    if asker(signed_in)?.person.role != Role::Owner {
        return Err(not_an_owner());
    }
    let handle = handle(&new.handle)?;
    let name = name("name", &new.name)?;
    let store = &state.app.store;
    if store.person_by_handle(&handle).map_err(internal)?.is_some() {
        return Err(ApiError(
            StatusCode::CONFLICT,
            format!("@{handle} is taken"),
        ));
    }
    let person = store
        .add_person(&handle, &name, new.role)
        .map_err(internal)?;
    let token = issue(&state, person.id, FIRST_TOKEN)?;
    Ok(Json(AddedPerson { person, token }))
}

async fn tokens_of(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Vec<AccessToken>>, ApiError> {
    let asker = asker(signed_in)?;
    if asker.person.id != id && asker.person.role != Role::Owner {
        return Err(not_an_owner());
    }
    if state.app.store.person(id).map_err(internal)?.is_none() {
        return Err(ApiError(StatusCode::NOT_FOUND, format!("person {id}")));
    }
    Ok(Json(state.app.store.access_tokens(id).map_err(internal)?))
}

async fn own_tokens(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
) -> Result<Json<Vec<AccessToken>>, ApiError> {
    let asker = asker(signed_in)?;
    Ok(Json(
        state
            .app
            .store
            .access_tokens(asker.person.id)
            .map_err(internal)?,
    ))
}

async fn new_own_token(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    Json(new): Json<NewToken>,
) -> Result<Json<IssuedToken>, ApiError> {
    let asker = asker(signed_in)?;
    let name = name("token's name", &new.name)?;
    Ok(Json(issue(&state, asker.person.id, &name)?))
}

async fn revoke(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<AccessToken>, ApiError> {
    let asker = asker(signed_in)?;
    let store = &state.app.store;
    let not_found = || ApiError(StatusCode::NOT_FOUND, format!("token {id}"));
    let token = store
        .access_token(id)
        .map_err(internal)?
        .ok_or_else(not_found)?;
    if token.person_id != asker.person.id && asker.person.role != Role::Owner {
        // Another person's token is not shown to exist.
        return Err(not_found());
    }
    store.revoke_access_token(id).map_err(internal)?;
    Ok(Json(
        store
            .access_token(id)
            .map_err(internal)?
            .ok_or_else(not_found)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::testing::start;
    use reqwest::StatusCode as Status;

    fn http() -> reqwest::Client {
        reqwest::Client::new()
    }

    async fn add(addr: &str, token: &str, handle: &str, role: &str) -> reqwest::Response {
        http()
            .post(format!("http://{addr}/v1/people"))
            .bearer_auth(token)
            .json(&serde_json::json!({"handle": handle, "name": "Someone", "role": role}))
            .send()
            .await
            .unwrap()
    }

    #[test]
    fn a_handle_is_short_lowercase_and_plain() {
        assert_eq!(handle(" @lea ").ok().as_deref(), Some("lea"));
        assert_eq!(handle("ada_l-2").ok().as_deref(), Some("ada_l-2"));
        for bad in ["", "@", "Lea", "-lea", "lea b", "léa", &"a".repeat(40)] {
            assert!(handle(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_token_has_the_shape_that_is_redacted() {
        let token = new_token().unwrap();
        assert!(token.starts_with(TOKEN_PREFIX));
        assert_eq!(token.len(), TOKEN_PREFIX.len() + 64);
        assert_eq!(crate::blobs::redact(&token), "[redacted:access-token]");
    }

    #[tokio::test]
    async fn an_owner_adds_a_person_whose_token_acts_as_them_with_their_role() {
        let (_dir, state, addr) = start().await;
        let added: AddedPerson = add(&addr, &state.token, "lea", "member")
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(
            (added.person.handle.as_str(), added.person.role),
            ("lea", Role::Member)
        );
        assert_eq!(added.token.record.name, FIRST_TOKEN);
        let lea = added.token.token;

        // Lea is a member: she cannot add people, the owner's token can, and a handle is taken once.
        let refused = add(&addr, &lea, "ada", "viewer").await;
        assert_eq!(refused.status(), Status::FORBIDDEN);
        assert_eq!(refused.text().await.unwrap(), "only an owner can do this");
        assert_eq!(
            add(&addr, &state.token, "LEA", "viewer").await.status(),
            Status::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            add(&addr, &state.token, "lea", "viewer").await.status(),
            Status::CONFLICT
        );

        // What Lea writes is hers.
        let issue: serde_json::Value = http()
            .post(format!("http://{addr}/v1/repositories/1/issues"))
            .bearer_auth(&lea)
            .json(&serde_json::json!({"title": "Lea's issue", "body": ""}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(issue["author"], "person:lea");

        let people: Vec<Person> = http()
            .get(format!("http://{addr}/v1/people"))
            .bearer_auth(&lea)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let handles: Vec<&str> = people.iter().map(|p| p.handle.as_str()).collect();
        assert_eq!(handles, ["owner", "lea"]);
    }

    #[tokio::test]
    async fn a_person_makes_and_revokes_their_own_tokens_and_an_owner_any() {
        let (_dir, state, addr) = start().await;
        let added: AddedPerson = add(&addr, &state.token, "lea", "viewer")
            .await
            .json()
            .await
            .unwrap();
        let ada: AddedPerson = add(&addr, &state.token, "ada", "member")
            .await
            .json()
            .await
            .unwrap();
        let lea = added.token.token.clone();
        let second: IssuedToken = http()
            .post(format!("http://{addr}/v1/tokens"))
            .bearer_auth(&lea)
            .json(&serde_json::json!({"name": "second machine"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(second.record.person_id, added.person.id);
        let own: Vec<AccessToken> = http()
            .get(format!("http://{addr}/v1/tokens"))
            .bearer_auth(&second.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let names: Vec<&str> = own.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, [FIRST_TOKEN, "second machine"]);
        // The first token was used to make the second: its last use shows.
        assert!(own[0].last_used_at.is_some());
        let empty = http()
            .post(format!("http://{addr}/v1/tokens"))
            .bearer_auth(&lea)
            .json(&serde_json::json!({"name": " "}))
            .send()
            .await
            .unwrap();
        assert_eq!(empty.status(), Status::UNPROCESSABLE_ENTITY);

        // Lea cannot see or revoke Ada's tokens; the owner sees and revokes anyone's.
        let theirs = format!("http://{addr}/v1/people/{}/tokens", ada.person.id);
        let peek = http().get(&theirs).bearer_auth(&lea).send().await.unwrap();
        assert_eq!(peek.status(), Status::FORBIDDEN);
        let revoke_ada = format!("http://{addr}/v1/tokens/{}", ada.token.record.id);
        let refused = http()
            .delete(&revoke_ada)
            .bearer_auth(&lea)
            .send()
            .await
            .unwrap();
        assert_eq!(refused.status(), Status::NOT_FOUND);
        let seen: Vec<AccessToken> = http()
            .get(&theirs)
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(seen.len(), 1);
        let revoked: AccessToken = http()
            .delete(&revoke_ada)
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(revoked.revoked_at.is_some());
        let health = |token: String| {
            http()
                .get(format!("http://{addr}/v1/health"))
                .bearer_auth(token)
                .send()
        };
        assert_eq!(
            health(ada.token.token).await.unwrap().status(),
            Status::UNAUTHORIZED
        );

        // Lea revokes her own first token with her second; the second still works.
        let revoke_own = http()
            .delete(format!("http://{addr}/v1/tokens/{}", added.token.record.id))
            .bearer_auth(&second.token)
            .send()
            .await
            .unwrap();
        assert_eq!(revoke_own.status(), Status::OK);
        assert_eq!(health(lea).await.unwrap().status(), Status::UNAUTHORIZED);
        assert_eq!(health(second.token).await.unwrap().status(), Status::OK);
    }

    #[tokio::test]
    async fn an_agent_cannot_reach_the_people_or_make_a_token() {
        let (_dir, state, addr) = start().await;
        let run = state
            .app
            .store
            .add_run(None, crate::harness::pibolt::MOEBIUS_AGENT, "test", "chat")
            .unwrap();
        let agent = state.router.issue_token(crate::router::Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        });
        let made = http()
            .post(format!("http://{addr}/v1/tokens"))
            .bearer_auth(&agent)
            .json(&serde_json::json!({"name": "mine"}))
            .send()
            .await
            .unwrap();
        assert_eq!(made.status(), Status::FORBIDDEN);
        assert_eq!(
            add(&addr, &agent, "bot", "owner").await.status(),
            Status::FORBIDDEN
        );
    }
}
