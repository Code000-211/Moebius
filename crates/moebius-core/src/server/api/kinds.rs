//! Kinds of issues and the fields of their own, and the cards across projects that have them.
//!
//! - `GET /v1/kinds`: every kind with its fields and the ids of the projects that offer it (`KindView`).
//! - `POST /v1/kinds` `{name, fields}` and `PUT /v1/kinds/{id}` `{name, fields}`: make a kind or replace its name and
//!   fields (an owner); `DELETE /v1/kinds/{id}` deletes one no card has.
//! - `PUT /v1/projects/{id}/kinds` `{kinds}`: replaces the kinds a project offers, by name; answers the `ProjectView`.
//! - `PUT /v1/cards/{id}/kind` `{kind}`: gives a card a kind its project offers, or none with `null`; answers the `Card`.
//! - `PUT /v1/cards/{id}/fields` `{fields}`: sets the values of fields of the card's kind (`null` clears one).
//! - `GET /v1/issues`: the cards of every project the person sees, filtered by `project`, `kind`, `state` and `field.<name>=<value>`
//!   and ordered by `sort=<field name>` (`order=desc` reverses it).

use std::collections::BTreeMap;

use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{get, put};
use axum::{Extension, Json, Router};
use serde::Deserialize;
use serde_json::Value;

use super::{ApiError, ApiState, blocking};
use crate::app::{AppError, CardQuery, KindView, ProjectView};
use crate::decide::Actor;
use crate::store::{Card, FieldDef};

/// The routes of kinds and fields.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/kinds", get(kinds).post(add_kind))
        .route("/v1/kinds/{id}", put(update_kind).delete(delete_kind))
        .route("/v1/projects/{id}/kinds", put(set_project_kinds))
        .route("/v1/cards/{id}/kind", put(set_card_kind))
        .route("/v1/cards/{id}/fields", put(set_card_fields))
        .route("/v1/issues", get(issues))
}

/// The body of `POST /v1/kinds` and `PUT /v1/kinds/{id}`.
#[derive(Debug, Deserialize)]
pub struct KindBody {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<FieldDef>,
}

/// The body of `PUT /v1/projects/{id}/kinds`.
#[derive(Debug, Deserialize)]
pub struct ProjectKinds {
    pub kinds: Vec<String>,
}

/// The body of `PUT /v1/cards/{id}/kind`.
#[derive(Debug, Deserialize)]
pub struct CardKind {
    pub kind: Option<String>,
}

/// The body of `PUT /v1/cards/{id}/fields`.
#[derive(Debug, Deserialize)]
pub struct CardFields {
    pub fields: BTreeMap<String, Value>,
}

async fn kinds(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
) -> Result<Json<Vec<KindView>>, ApiError> {
    // A kind lists only the projects whose records the person sees (see `app::projects::sight`).
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        Ok(app
            .kind_views()?
            .into_iter()
            .map(|kind| kind.seen_by(&sight))
            .collect::<Vec<_>>())
    })
    .await
}

async fn add_kind(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    Json(body): Json<KindBody>,
) -> Result<Json<KindView>, ApiError> {
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        Ok(app.add_kind(&body.name, &body.fields)?.seen_by(&sight))
    })
    .await
}

async fn update_kind(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(body): Json<KindBody>,
) -> Result<Json<KindView>, ApiError> {
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        Ok(app
            .update_kind(id, &body.name, &body.fields)?
            .seen_by(&sight))
    })
    .await
}

async fn delete_kind(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Value>, ApiError> {
    blocking(&state.app, move |app| {
        app.delete_kind(id, &app.sight(&by)?)
            .map(|()| serde_json::json!({}))
    })
    .await
}

async fn set_project_kinds(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(body): Json<ProjectKinds>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| {
        app.set_project_kinds(id, &body.kinds)?;
        app.project_view_of(id, &app.sight(&by)?)
    })
    .await
}

async fn set_card_kind(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(body): Json<CardKind>,
) -> Result<Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.set_card_kind(id, body.kind.as_deref())
    })
    .await
}

async fn set_card_fields(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(body): Json<CardFields>,
) -> Result<Json<Card>, ApiError> {
    blocking(&state.app, move |app| app.set_card_fields(id, &body.fields)).await
}

/// The query of `GET /v1/issues` as the address spells it.
fn card_query(pairs: Vec<(String, String)>) -> Result<CardQuery, AppError> {
    let mut query = CardQuery::default();
    for (key, value) in pairs {
        match key.as_str() {
            "project" => {
                query.project = Some(value.parse().map_err(|_| {
                    AppError::Invalid(format!("project is a number, not {value:?}"))
                })?)
            }
            "kind" => query.kind = Some(value),
            "state" => query.state = Some(value),
            "sort" => query.sort = Some(value),
            "order" => match value.as_str() {
                "asc" => query.descending = false,
                "desc" => query.descending = true,
                _ => {
                    return Err(AppError::Invalid(format!(
                        "order is asc or desc, not {value:?}"
                    )));
                }
            },
            other => match other.strip_prefix("field.") {
                Some(name) => query.fields.push((name.to_string(), value)),
                None => {
                    return Err(AppError::Invalid(format!(
                        "{other} is not a filter: project, kind, state, field.<name>, sort or order"
                    )));
                }
            },
        }
    }
    Ok(query)
}

async fn issues(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<Vec<Card>>, ApiError> {
    blocking(&state.app, move |app| {
        // The cards of a private project the person is no member of are left out (see `app::projects::sight`).
        let sight = app.sight(&by)?;
        let mut cards = app.issues_across(&card_query(pairs)?)?;
        cards.retain(|card| sight.card(card));
        Ok(cards)
    })
    .await
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode as Status;
    use serde_json::{Value, json};

    use crate::server::api::testing::start;

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

    #[tokio::test]
    async fn an_owner_defines_a_kind_a_project_picks_it_and_cards_of_two_projects_are_found_by_a_field()
     {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let (status, made) = call(
            "POST",
            &addr,
            &token,
            "/v1/kinds",
            Some(json!({"name": "Bug", "fields": [
                {"name": "Severity", "type": "choice", "options": ["low", "high"]},
                {"name": "Estimate", "type": "number"}
            ]})),
        )
        .await;
        assert_eq!(status, Status::OK, "{made}");
        assert_eq!(made["fields"][0]["type"], "choice");
        assert_eq!(made["projects"], json!([]));
        let (status, dup) = call(
            "POST",
            &addr,
            &token,
            "/v1/kinds",
            Some(json!({"name": "bug"})),
        )
        .await;
        assert_eq!(status, Status::CONFLICT, "{dup}");
        let (status, _) = call(
            "POST",
            &addr,
            &token,
            "/v1/kinds",
            Some(json!({"name": "Odd", "fields": [{"name": "X", "type": "tree"}]})),
        )
        .await;
        assert!(status.is_client_error());

        // Two projects (every repository has its own), each with a card.
        let repo = state.app.store.repositories().unwrap()[0].clone();
        let other = state
            .app
            .store
            .add_repository("storefront", "/storefront", "none", "main")
            .unwrap();
        let (first, second) = (repo.home_project_id, other.home_project_id);
        let card_a = state.app.store.cards(repo.id).unwrap()[0].clone();
        let card_b = state
            .app
            .store
            .add_card(other.id, "other", "", "person", "Backlog")
            .unwrap();

        // A project offers only the kinds it picked.
        let (status, refused) = call(
            "PUT",
            &addr,
            &token,
            &format!("/v1/cards/{}/kind", card_a.id),
            Some(json!({"kind": "Bug"})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY, "{refused}");
        for project in [first, second] {
            let (status, view) = call(
                "PUT",
                &addr,
                &token,
                &format!("/v1/projects/{project}/kinds"),
                Some(json!({"kinds": ["bug"]})),
            )
            .await;
            assert_eq!(status, Status::OK, "{view}");
            assert_eq!(view["kinds"].as_array().unwrap().len(), 1);
        }
        for (card, estimate) in [(&card_a, 8), (&card_b, 3)] {
            let (status, with_kind) = call(
                "PUT",
                &addr,
                &token,
                &format!("/v1/cards/{}/kind", card.id),
                Some(json!({"kind": "Bug"})),
            )
            .await;
            assert_eq!(status, Status::OK, "{with_kind}");
            let (status, set) = call(
                "PUT",
                &addr,
                &token,
                &format!("/v1/cards/{}/fields", card.id),
                Some(json!({"fields": {"Severity": "high", "Estimate": estimate}})),
            )
            .await;
            assert_eq!(status, Status::OK, "{set}");
            assert_eq!(set["fields"]["Estimate"], estimate);
        }
        let (status, bad) = call(
            "PUT",
            &addr,
            &token,
            &format!("/v1/cards/{}/fields", card_a.id),
            Some(json!({"fields": {"Severity": "huge"}})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY, "{bad}");

        let (status, found) = call(
            "GET",
            &addr,
            &token,
            "/v1/issues?field.Severity=high&sort=Estimate",
            None,
        )
        .await;
        assert_eq!(status, Status::OK, "{found}");
        let ids: Vec<i64> = found
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, [card_b.id, card_a.id], "both projects, by estimate");
        let (_, only) = call(
            "GET",
            &addr,
            &token,
            &format!("/v1/issues?kind=bug&project={second}"),
            None,
        )
        .await;
        assert_eq!(only.as_array().unwrap().len(), 1);
        let (status, _) = call("GET", &addr, &token, "/v1/issues?colour=red", None).await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);

        let (_, listed) = call("GET", &addr, &token, "/v1/kinds", None).await;
        assert_eq!(listed[0]["projects"], json!([first, second]));
        let (status, _) = call(
            "DELETE",
            &addr,
            &token,
            &format!("/v1/kinds/{}", made["id"]),
            None,
        )
        .await;
        assert_eq!(status, Status::CONFLICT, "cards have it");
    }
}
