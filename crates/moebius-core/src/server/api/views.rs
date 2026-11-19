//! Saved views: a filter of the filter bar with a layout and a sort, kept for one person or shared with everyone. A
//! view's cards are worked out for whoever opens it, within what they may see (`App::sight`).
//!
//! - `GET /v1/views`: the views of the person asking: their own and the shared ones, in the order they were made.
//! - `POST /v1/views` `{name, shared, filter, layout, sort, descending}`: makes one.
//! - `GET /v1/views/{id}`: one view; a personal view of someone else is `404`.
//! - `PATCH /v1/views/{id}` (same body): replaces its fields; only its owner and an owner of the organisation.
//! - `DELETE /v1/views/{id}`: deletes it and answers it; who may is as for `PATCH`.
//! - `GET /v1/views/{id}/cards`: the view with the cards it shows the person asking (`ViewCards`).
//!
//! Only a person has views, never an agent.

use axum::extract::{Path as UrlPath, State};
use axum::routing::get;
use axum::{Extension, Json, Router};

use super::{ApiError, ApiState, blocking};
use crate::app::{ViewCards, ViewFields};
use crate::decide::Actor;
use crate::store::View;

/// The routes of the saved views.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/views", get(views).post(add_view))
        .route(
            "/v1/views/{id}",
            get(view).patch(edit_view).delete(delete_view),
        )
        .route("/v1/views/{id}/cards", get(view_cards))
}

async fn views(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
) -> Result<Json<Vec<View>>, ApiError> {
    blocking(&state.app, move |app| app.views(&actor)).await
}

async fn add_view(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    Json(fields): Json<ViewFields>,
) -> Result<Json<View>, ApiError> {
    blocking(&state.app, move |app| app.add_view(&actor, &fields)).await
}

async fn view(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<View>, ApiError> {
    blocking(&state.app, move |app| app.view(&actor, id)).await
}

async fn edit_view(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(fields): Json<ViewFields>,
) -> Result<Json<View>, ApiError> {
    blocking(&state.app, move |app| app.edit_view(&actor, id, &fields)).await
}

async fn delete_view(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<View>, ApiError> {
    blocking(&state.app, move |app| app.delete_view(&actor, id)).await
}

async fn view_cards(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<ViewCards>, ApiError> {
    blocking(&state.app, move |app| app.view_cards(&actor, id)).await
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode as Status;
    use serde_json::{Value, json};

    use crate::server::api::testing::start;
    use crate::server::api::{ApiState, TOKEN_PREFIX};
    use crate::store::Role;

    async fn call(
        method: &str,
        addr: &str,
        token: &str,
        path: &str,
        body: Option<Value>,
    ) -> (Status, Value) {
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), format!("http://{addr}{path}"))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    fn token_of(state: &ApiState, handle: &str, role: Role, n: usize) -> String {
        let person = state.app.store.add_person(handle, handle, role).unwrap();
        let token = format!("{TOKEN_PREFIX}{n:064}");
        state
            .app
            .store
            .add_access_token(person.id, "test", &token)
            .unwrap();
        token
    }

    #[tokio::test]
    async fn a_view_is_made_opened_edited_and_deleted_through_the_api() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let card = state.app.store.cards(1).unwrap()[0].clone();
        let (status, made) = call(
            "POST",
            &addr,
            &token,
            "/v1/views",
            Some(
                json!({"name": "Backlog", "layout": "board", "sort": "title",
                "filter": {"columns": ["Backlog"], "text": "first"}}),
            ),
        )
        .await;
        assert_eq!(status, Status::OK, "{made}");
        assert_eq!(made["shared"], false);
        assert_eq!(made["filter"]["columns"], json!(["Backlog"]));
        let id = made["id"].as_i64().unwrap();

        let (_, listed) = call("GET", &addr, &token, "/v1/views", None).await;
        assert_eq!(listed[0]["name"], "Backlog");
        let (_, shown) = call("GET", &addr, &token, &format!("/v1/views/{id}/cards"), None).await;
        assert_eq!(shown["view"]["layout"], "board");
        assert_eq!(shown["cards"][0]["id"], card.id);
        assert_eq!(shown["cards"][0]["title"], "First");

        let (status, edited) = call(
            "PATCH",
            &addr,
            &token,
            &format!("/v1/views/{id}"),
            Some(json!({"name": "Else", "filter": {"columns": ["Done"]}})),
        )
        .await;
        assert_eq!(status, Status::OK, "{edited}");
        let (_, shown) = call("GET", &addr, &token, &format!("/v1/views/{id}/cards"), None).await;
        assert_eq!(shown["cards"], json!([]));

        let (status, _) = call(
            "POST",
            &addr,
            &token,
            "/v1/views",
            Some(json!({"name": "Odd", "layout": "gantt"})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        let (status, _) = call(
            "POST",
            &addr,
            &token,
            "/v1/views",
            Some(json!({"name": "Odd", "filter": {"colour": ["red"]}})),
        )
        .await;
        assert!(status.is_client_error(), "{status}");

        let (status, _) = call("DELETE", &addr, &token, &format!("/v1/views/{id}"), None).await;
        assert_eq!(status, Status::OK);
        let (status, _) = call("GET", &addr, &token, &format!("/v1/views/{id}"), None).await;
        assert_eq!(status, Status::NOT_FOUND);
    }

    #[tokio::test]
    async fn people_see_their_own_and_the_shared_views_and_a_viewer_cannot_make_one() {
        let (_d, state, addr) = start().await;
        let owner = state.token.to_string();
        let ben = token_of(&state, "ben", Role::Member, 1);
        let vera = token_of(&state, "vera", Role::Viewer, 2);

        let (_, mine) = call(
            "POST",
            &addr,
            &owner,
            "/v1/views",
            Some(json!({"name": "Mine"})),
        )
        .await;
        let (_, team) = call(
            "POST",
            &addr,
            &owner,
            "/v1/views",
            Some(json!({"name": "Team", "shared": true})),
        )
        .await;
        for token in [&ben, &vera] {
            let (_, listed) = call("GET", &addr, token, "/v1/views", None).await;
            let names: Vec<&str> = listed
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["name"].as_str().unwrap())
                .collect();
            assert_eq!(names, ["Team"]);
            let path = format!("/v1/views/{}/cards", team["id"]);
            assert_eq!(call("GET", &addr, token, &path, None).await.0, Status::OK);
            let path = format!("/v1/views/{}/cards", mine["id"]);
            assert_eq!(
                call("GET", &addr, token, &path, None).await.0,
                Status::NOT_FOUND
            );
        }
        let (status, _) = call(
            "POST",
            &addr,
            &vera,
            "/v1/views",
            Some(json!({"name": "Nope"})),
        )
        .await;
        assert_eq!(status, Status::FORBIDDEN);
        let (status, _) = call(
            "DELETE",
            &addr,
            &ben,
            &format!("/v1/views/{}", team["id"]),
            None,
        )
        .await;
        assert_eq!(status, Status::FORBIDDEN);
    }
}
