//! Giving cards and inbox items to people (ADR 0018).
//!
//! - `PUT /v1/cards/{id}/assignees` `{assignees}`: replaces the people a card is given to; answers the `Card`.
//! - `PUT /v1/inbox/{id}/assignees` `{assignees}`: the same for an item of the inbox; answers the `InboxItem`.
//!
//! `assignees` are handles, with or without the `@`, in any case. A person (a member or an owner) does this, never an
//! agent: what a card's assignees mean is up to the column's script.

use axum::extract::{Path as UrlPath, State};
use axum::routing::put;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking};
use crate::store::{Card, InboxItem};

/// The routes of assignees.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/cards/{id}/assignees", put(set_card_assignees))
        .route("/v1/inbox/{id}/assignees", put(set_inbox_assignees))
}

/// The body of `PUT /v1/cards/{id}/assignees` and `PUT /v1/inbox/{id}/assignees`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewAssignees {
    pub assignees: Vec<String>,
}

async fn set_card_assignees(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewAssignees>,
) -> Result<Json<Card>, ApiError> {
    blocking(&state.app, move |app| app.set_assignees(id, &new.assignees)).await
}

async fn set_inbox_assignees(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewAssignees>,
) -> Result<Json<InboxItem>, ApiError> {
    blocking(&state.app, move |app| {
        app.set_inbox_assignees(id, &new.assignees)
    })
    .await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::server::api::testing::start;
    use crate::store::Role;

    async fn put(
        state: &ApiState,
        addr: &str,
        path: &str,
        body: serde_json::Value,
    ) -> reqwest::Response {
        reqwest::Client::new()
            .put(format!("http://{addr}{path}"))
            .bearer_auth(&*state.token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_person_gives_a_card_and_an_inbox_item_to_people_and_unknown_handles_are_refused() {
        let (_dir, state, addr) = start().await;
        state
            .app
            .store
            .add_person("lea", "Lea", Role::Member)
            .unwrap();
        let tom = state
            .app
            .store
            .add_person("tom", "Tom", Role::Viewer)
            .unwrap();
        let repo = state
            .app
            .store
            .add_repository("r", "/r", "none", "main")
            .unwrap();
        let card = state
            .app
            .store
            .add_card(repo.id, "t", "b", "person", "Not started")
            .unwrap();
        let item = state
            .app
            .store
            .add_inbox_item("stuck", Some(card.id), "Stuck", &json!({}))
            .unwrap();
        let mut events = state.app.bus.subscribe();

        let path = format!("/v1/cards/{}/assignees", card.id);
        let answer = put(&state, &addr, &path, json!({"assignees": ["@Tom", "lea"]})).await;
        assert_eq!(answer.status(), 200);
        assert_eq!(
            answer.json::<Card>().await.unwrap().assignees,
            ["lea", "tom"]
        );
        assert_eq!(events.recv().await.unwrap().kind, "card_assignees_changed");

        let path = format!("/v1/inbox/{}/assignees", item.id);
        let answer = put(&state, &addr, &path, json!({"assignees": ["lea"]})).await;
        assert_eq!(answer.status(), 200);
        assert_eq!(answer.json::<InboxItem>().await.unwrap().assignees, ["lea"]);
        let snapshot: serde_json::Value = reqwest::Client::new()
            .get(format!("http://{addr}/v1/snapshot"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(snapshot["inbox"][0]["assignees"], json!(["lea"]));
        let on_board = snapshot["boards"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|board| board["cards"].as_array().unwrap())
            .find(|c| c["id"] == card.id)
            .unwrap();
        assert_eq!(on_board["assignees"], json!(["lea", "tom"]));

        for (path, body) in [
            (
                format!("/v1/cards/{}/assignees", card.id),
                json!({"assignees": ["nobody"]}),
            ),
            (
                "/v1/cards/9999/assignees".to_string(),
                json!({"assignees": []}),
            ),
            (
                "/v1/inbox/9999/assignees".to_string(),
                json!({"assignees": []}),
            ),
        ] {
            let answer = put(&state, &addr, &path, body).await;
            assert!(
                answer.status().is_client_error(),
                "{path}: {}",
                answer.status()
            );
        }
        state.app.store.set_person_disabled(tom.id, true).unwrap();
        let path = format!("/v1/cards/{}/assignees", card.id);
        let answer = put(&state, &addr, &path, json!({"assignees": []})).await;
        assert_eq!(answer.status(), 200);
        let answer = put(&state, &addr, &path, json!({"assignees": ["tom"]})).await;
        assert_eq!(
            answer.status(),
            422,
            "a disabled person is not given a card"
        );
        assert_eq!(
            state.app.card(card.id).unwrap().assignees,
            Vec::<String>::new()
        );
    }
}
