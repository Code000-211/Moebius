//! The projects (ADR 0019): each with a name, a key that names its cards and is kept while the project is, a board and its
//! repositories. Every repository has a home project, made with it, so a server from before has one project per
//! repository.
//!
//! - `GET /v1/projects`: every project, as the Projects page shows it (`ProjectView`).
//! - `POST /v1/projects` `{name, key}`: makes a project with no repository yet; the key is one no other project has.
//! - `GET /v1/projects/{id}`: one project.
//! - `PATCH /v1/projects/{id}` `{name}`: renames a project. Its key stays: a body that names another key is refused.

use axum::extract::{Path as UrlPath, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::app::ProjectView;

/// The routes of the projects.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/projects", get(projects).post(add_project))
        .route("/v1/projects/{id}", get(project).patch(rename_project))
}

/// What `POST /v1/projects` takes.
#[derive(Debug, Deserialize)]
pub struct NewProject {
    pub name: String,
    pub key: String,
}

/// What `PATCH /v1/projects/{id}` takes. `key` is only there to be refused when it is not the project's own.
#[derive(Debug, Deserialize)]
pub struct EditedProject {
    pub name: String,
    #[serde(default)]
    pub key: Option<String>,
}

async fn projects(State(state): State<ApiState>) -> Result<Json<Vec<ProjectView>>, ApiError> {
    blocking(&state.app, |app| app.project_views()).await
}

async fn project(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| app.project_view_of(id)).await
}

async fn add_project(
    State(state): State<ApiState>,
    Json(new): Json<NewProject>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| app.add_project(&new.name, &new.key)).await
}

async fn rename_project(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(edited): Json<EditedProject>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| {
        app.rename_project(id, &edited.name, edited.key.as_deref())
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
    async fn the_repository_s_project_is_listed_and_a_new_one_needs_a_free_key() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let (status, all) = call("GET", &addr, &token, "/v1/projects", None).await;
        assert_eq!(status, Status::OK);
        let radiance = &all[0];
        assert_eq!(radiance["name"], "radiance");
        assert_eq!(radiance["key"], "RADIANCE");
        assert_eq!(radiance["open_cards"], 1);
        assert_eq!(
            radiance["repositories"],
            json!([{"id": 1, "name": "radiance", "home": true}])
        );
        assert_eq!(radiance["members"][0]["handle"], "owner");

        let (status, made) = call(
            "POST",
            &addr,
            &token,
            "/v1/projects",
            Some(json!({"name": "Storefront", "key": "SHOP"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{made}");
        assert_eq!(
            (made["key"].as_str(), made["open_cards"].as_i64()),
            (Some("SHOP"), Some(0))
        );
        assert_eq!(made["repositories"], json!([]));
        assert_eq!(
            radiance["files"],
            Value::Null,
            "a repository's project needs none"
        );
        // Its own files are a board of their own, listed on the Files page and never removed by themselves.
        let files = made["files"].as_i64().expect("the project has its files");
        let (status, tree) = call(
            "GET",
            &addr,
            &token,
            &format!("/v1/repositories/{files}/tree"),
            None,
        )
        .await;
        assert_eq!((status, &tree["entries"]), (Status::OK, &json!([])));
        let (status, card) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/repositories/{files}/issues"),
            Some(json!({"title": "Write the prices", "body": ""})),
        )
        .await;
        assert_eq!(status, Status::OK, "{card}");
        assert_eq!(card["project_id"], made["id"]);
        let (status, _) = call(
            "DELETE",
            &addr,
            &token,
            &format!("/v1/repositories/{files}"),
            None,
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        let (status, taken) = call(
            "POST",
            &addr,
            &token,
            "/v1/projects",
            Some(json!({"name": "Shop", "key": "SHOP"})),
        )
        .await;
        assert_eq!(status, Status::CONFLICT, "{taken}");
        let (status, _) = call(
            "POST",
            &addr,
            &token,
            "/v1/projects",
            Some(json!({"name": "Shop", "key": "shop"})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn renaming_a_project_keeps_its_key() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let id = state.app.store.projects().unwrap()[0].id;
        let path = format!("/v1/projects/{id}");
        let (status, renamed) = call(
            "PATCH",
            &addr,
            &token,
            &path,
            Some(json!({"name": "Radiance site"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{renamed}");
        assert_eq!(
            (renamed["name"].as_str(), renamed["key"].as_str()),
            (Some("Radiance site"), Some("RADIANCE"))
        );
        // Naming its own key is fine; another is refused and changes nothing.
        let (status, _) = call(
            "PATCH",
            &addr,
            &token,
            &path,
            Some(json!({"name": "Radiance", "key": "RADIANCE"})),
        )
        .await;
        assert_eq!(status, Status::OK);
        let (status, refused) = call(
            "PATCH",
            &addr,
            &token,
            &path,
            Some(json!({"name": "Other", "key": "RAD"})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        assert!(
            refused.as_str().unwrap().contains("keeps its key"),
            "{refused}"
        );
        let (_, kept) = call("GET", &addr, &token, &path, None).await;
        assert_eq!(
            (kept["name"].as_str(), kept["key"].as_str()),
            (Some("Radiance"), Some("RADIANCE"))
        );
        let (status, _) = call("GET", &addr, &token, "/v1/projects/99", None).await;
        assert_eq!(status, Status::NOT_FOUND);
    }
}
