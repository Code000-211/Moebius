//! What concerns a person, and the cards they follow (ADR 0018).
//!
//! - `GET /v1/notices`: the notices of the person asking, newest first: what they were given, where they were
//!   mentioned and what changed on a card they follow. The Inbox shows them under Mentions.
//! - `POST /v1/notices/{id}/read`: marks one of their notices read, for them alone; answers the `Notice`. Another
//!   person's notice is a `404`, and so is one of a card or inbox item of a private project they do not see.
//! - `GET /v1/cards/{id}/followers`: the handles of the people who follow a card.
//! - `PUT /v1/cards/{id}/follow` `{following}`: the person asking follows the card or stops; answers the followers.
//!
//! Every person of any role has these, an agent has none: they are about the person.

use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post, put};
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking, privacy};
use crate::app::{App, AppError, Sight};
use crate::decide::Actor;
use crate::store::Notice;

/// The routes of notices and following.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/notices", get(notices))
        .route("/v1/notices/{id}/read", post(read))
        .route("/v1/cards/{id}/followers", get(followers))
        .route("/v1/cards/{id}/follow", put(follow))
}

/// The body of `PUT /v1/cards/{id}/follow`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewFollow {
    pub following: bool,
}

async fn notices(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
) -> Result<Json<Vec<Notice>>, ApiError> {
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        let mut seen = Vec::new();
        for notice in app.notices(&by)? {
            if sees_notice(app, &sight, &notice)? {
                seen.push(notice);
            }
        }
        Ok(seen)
    })
    .await
}

/// Whether `sight` sees what `notice` is about: a notice of a card or inbox item of a private project the person is no
/// member of (any more) is left out, and answered `404`.
pub(super) fn sees_notice(app: &App, sight: &Sight, notice: &Notice) -> crate::app::Result<bool> {
    if let Some(card) = notice.card_id
        && !app.sees_card(sight, card)?
    {
        return Ok(false);
    }
    match notice.inbox_item_id {
        Some(item) => privacy::sees(app, sight, "inbox", "", item),
        None => Ok(true),
    }
}

async fn read(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Notice>, ApiError> {
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        let hidden = app
            .notices(&by)?
            .iter()
            .find(|notice| notice.id == id)
            .map(|notice| sees_notice(app, &sight, notice))
            .transpose()?
            == Some(false);
        if hidden {
            return Err(AppError::NotFound(format!("notice {id}")));
        }
        app.read_notice(&by, id)
    })
    .await
}

async fn followers(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Vec<String>>, ApiError> {
    blocking(&state.app, move |app| app.followers(id)).await
}

async fn follow(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewFollow>,
) -> Result<Json<Vec<String>>, ApiError> {
    blocking(&state.app, move |app| {
        app.set_following(&by, id, new.following)
    })
    .await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::server::api::testing::start;
    use crate::store::Role;

    /// Calls `method` `path` with the bearer token of `person` (the server's own for none).
    async fn call(
        addr: &str,
        token: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> reqwest::Response {
        let request = reqwest::Client::new()
            .request(method, format!("http://{addr}{path}"))
            .bearer_auth(token);
        match body {
            Some(body) => request.json(&body).send().await.unwrap(),
            None => request.send().await.unwrap(),
        }
    }

    #[tokio::test]
    async fn a_person_reads_their_own_notices_and_follows_cards() {
        let (_dir, state, addr) = start().await;
        let store = &state.app.store;
        let lea = store.add_person("lea", "Lea", Role::Member).unwrap();
        let tom = store.add_person("tom", "Tom", Role::Viewer).unwrap();
        let lea_token = &super::super::people::new_token().unwrap();
        store.add_access_token(lea.id, "t", lea_token).unwrap();
        let tom_token = &super::super::people::new_token().unwrap();
        store.add_access_token(tom.id, "t", tom_token).unwrap();
        let repo = store.add_repository("r", "/r", "none", "main").unwrap();
        let card = store
            .add_card(repo.id, "Fix it", "b", "person:lea", "Not started")
            .unwrap();
        state.app.set_assignees(card.id, &["tom".into()]).unwrap();

        let get = reqwest::Method::GET;
        let mine: Vec<Notice> = call(&addr, tom_token, get.clone(), "/v1/notices", None)
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(
            (mine.len(), mine[0].kind.as_str(), mine[0].read),
            (1, "assigned", false)
        );
        let hers: Vec<Notice> = call(&addr, lea_token, get.clone(), "/v1/notices", None)
            .await
            .json()
            .await
            .unwrap();
        assert!(hers.is_empty());

        // Only Tom reads Tom's notice; Lea is told there is none.
        let path = format!("/v1/notices/{}/read", mine[0].id);
        let post = reqwest::Method::POST;
        let answer = call(&addr, lea_token, post.clone(), &path, None).await;
        assert_eq!(answer.status(), 404);
        let answer = call(&addr, tom_token, post.clone(), &path, None).await;
        assert_eq!(answer.status(), 200);
        assert!(answer.json::<Notice>().await.unwrap().read);
        let again: Vec<Notice> = call(&addr, tom_token, get.clone(), "/v1/notices", None)
            .await
            .json()
            .await
            .unwrap();
        assert!(again[0].read);

        // Following: the author and the assignee follow; a viewer may choose to, too, and stop.
        let path = format!("/v1/cards/{}/followers", card.id);
        let followers: Vec<String> = call(&addr, tom_token, get.clone(), &path, None)
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(followers, ["lea", "tom"]);
        let follow = format!("/v1/cards/{}/follow", card.id);
        let put = reqwest::Method::PUT;
        let after: Vec<String> = call(
            &addr,
            lea_token,
            put.clone(),
            &follow,
            Some(json!({"following": false})),
        )
        .await
        .json()
        .await
        .unwrap();
        assert_eq!(after, ["tom"]);
        let missing = call(
            &addr,
            lea_token,
            put,
            "/v1/cards/9999/follow",
            Some(json!({"following": true})),
        )
        .await;
        assert_eq!(missing.status(), 404);

        // The server's own token is the first owner, a person.
        let owner = call(&addr, &state.token, get, "/v1/notices", None).await;
        assert_eq!(owner.status(), 200);
    }
}
