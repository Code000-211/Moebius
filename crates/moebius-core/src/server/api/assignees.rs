//! Giving cards and inbox items to people (ADR 0018).
//!
//! - `PUT /v1/cards/{id}/assignees` `{assignees}`: replaces the people a card is given to; answers the `Card`.
//! - `PUT /v1/inbox/{id}/assignees` `{assignees}`: the same for an item of the inbox; answers the `InboxItem`.
//!
//! - `PUT /v1/cards/{id}/assignees/{handle}` and `DELETE` of the same: add or take off one person, applied to the people
//!   the card has when the request is handled, so two changes made in quick succession both stay; answers the `Card`.
//! - `PUT /v1/inbox/{id}/assignees/{handle}` and `DELETE`: the same for an item of the inbox.
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
        .route(
            "/v1/cards/{id}/assignees/{handle}",
            put(add_card_assignee).delete(remove_card_assignee),
        )
        .route(
            "/v1/inbox/{id}/assignees/{handle}",
            put(add_inbox_assignee).delete(remove_inbox_assignee),
        )
}

async fn add_card_assignee(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath((id, handle)): UrlPath<(i64, String)>,
) -> Result<Json<Card>, ApiError> {
    blocking(&state.app, move |app| app.add_assignee_by(&by, id, &handle)).await
}

async fn remove_card_assignee(
    State(state): State<ApiState>,
    UrlPath((id, handle)): UrlPath<(i64, String)>,
) -> Result<Json<Card>, ApiError> {
    blocking(&state.app, move |app| app.remove_assignee(id, &handle)).await
}

async fn add_inbox_assignee(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath((id, handle)): UrlPath<(i64, String)>,
) -> Result<Json<InboxItem>, ApiError> {
    blocking(&state.app, move |app| {
        app.add_inbox_assignee_by(&by, id, &handle)
    })
    .await
}

async fn remove_inbox_assignee(
    State(state): State<ApiState>,
    UrlPath((id, handle)): UrlPath<(i64, String)>,
) -> Result<Json<InboxItem>, ApiError> {
    blocking(&state.app, move |app| {
        app.remove_inbox_assignee(id, &handle)
    })
    .await
}

/// The body of `PUT /v1/cards/{id}/assignees` and `PUT /v1/inbox/{id}/assignees`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewAssignees {
    pub assignees: Vec<String>,
}

async fn set_card_assignees(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewAssignees>,
) -> Result<Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.set_assignees_by(&by, id, &new.assignees)
    })
    .await
}

async fn set_inbox_assignees(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<crate::decide::Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<NewAssignees>,
) -> Result<Json<InboxItem>, ApiError> {
    blocking(&state.app, move |app| {
        app.set_inbox_assignees_by(&by, id, &new.assignees)
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
    async fn two_assignments_sent_one_right_after_the_other_both_stay() {
        let (_dir, state, addr) = start().await;
        state
            .app
            .store
            .add_person("lea", "Lea", Role::Member)
            .unwrap();
        state
            .app
            .store
            .add_person("tom", "Tom", Role::Member)
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
        let client = reqwest::Client::new();
        for (kind, id) in [("cards", card.id), ("inbox", item.id)] {
            let url = |handle: &str| format!("http://{addr}/v1/{kind}/{id}/assignees/{handle}");
            for handle in ["lea", "@Tom"] {
                let answer = client
                    .put(url(handle))
                    .bearer_auth(&*state.token)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(answer.status(), 200, "{kind} {handle}");
            }
            let get = |kind: &str| {
                let app = state.app.clone();
                let kind = kind.to_string();
                move || -> Vec<String> {
                    if kind == "cards" {
                        app.card(card.id).unwrap().assignees
                    } else {
                        app.store.inbox_item(item.id).unwrap().unwrap().assignees
                    }
                }
            };
            assert_eq!(get(kind)(), ["lea", "tom"], "{kind}");
            let answer = client
                .delete(url("lea"))
                .bearer_auth(&*state.token)
                .send()
                .await
                .unwrap();
            assert_eq!(answer.status(), 200);
            assert_eq!(get(kind)(), ["tom"], "{kind}");
            let answer = client
                .put(url("nobody"))
                .bearer_auth(&*state.token)
                .send()
                .await
                .unwrap();
            assert_eq!(answer.status(), 422);
        }
        let answer = client
            .put(format!("http://{addr}/v1/cards/9999/assignees/lea"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap();
        assert_eq!(answer.status(), 404);
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
