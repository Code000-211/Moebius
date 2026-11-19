//! The projects (ADR 0019): each with a name, a key that names its cards and is kept while the project is, a board and its
//! repositories. Every repository has a home project, made with it, so a server from before has one project per
//! repository.
//!
//! - `GET /v1/projects`: every project, as the Projects page shows it (`ProjectView`).
//! - `POST /v1/projects` `{name, key}`: makes a project with no repository yet; the key is one no other project has.
//! - `GET /v1/projects/{id}`: one project.
//! - `PATCH /v1/projects/{id}` `{name}`: renames a project. Its key stays: a body that names another key is refused.
//!   Its settings move to `projects/<new name>/` of the harness configuration repository.
//! - `GET /v1/projects/{id}/scripts`: the script of every column of the project's board, its own or the built-in one.
//! - `GET`, `PUT` and `DELETE /v1/projects/{id}/scripts/{column}`: one column's script, read, replaced (checked as
//!   `moebius check-script` checks it) or given up for the built-in one (see `board::scripts`).
//! - `PUT /v1/projects/{id}/access` `{private, members, reason}`: an owner makes a project private or public and names
//!   its members by handle.
//! - `POST /v1/projects/{id}/repositories` `{repository, reason}` and `DELETE
//!   /v1/projects/{id}/repositories/{repository}`: an owner puts a repository among a project's repositories or takes
//!   it out (never from its home project).
//!
//! A private project that the person asking is no member of is not listed and is answered 404, except to an owner, who
//! sees its record (with `member` false) so that they can name its members (see `app::projects::sight`).

use axum::extract::{Path as UrlPath, State};
use axum::routing::{delete, get, post, put};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::app::ProjectView;
use crate::board::scripts::{self, NewScript, ScriptView};
use crate::decide::Actor;

/// The routes of the projects.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/projects", get(projects).post(add_project))
        .route("/v1/projects/{id}", get(project).patch(rename_project))
        .route("/v1/projects/{id}/scripts", get(get_scripts))
        .route(
            "/v1/projects/{id}/scripts/{column}",
            get(get_script).put(put_script).delete(delete_script),
        )
        .route("/v1/projects/{id}/access", put(set_access))
        .route("/v1/projects/{id}/repositories", post(add_repository))
        .route(
            "/v1/projects/{id}/repositories/{repository}",
            delete(remove_repository),
        )
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

/// What `PUT /v1/projects/{id}/access` takes: whether the project is private, and its members by handle.
#[derive(Debug, Deserialize)]
pub struct Access {
    pub private: bool,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub reason: String,
}

/// What `POST /v1/projects/{id}/repositories` takes.
#[derive(Debug, Deserialize)]
pub struct ProjectRepositoryAdd {
    pub repository: i64,
    #[serde(default)]
    pub reason: String,
}

async fn add_repository(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(add): Json<ProjectRepositoryAdd>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| {
        app.add_repository_to_project(&by, id, add.repository, &add.reason)
    })
    .await
}

async fn remove_repository(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((id, repository)): UrlPath<(i64, i64)>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| {
        app.remove_repository_from_project(&by, id, repository, "")
    })
    .await
}

async fn projects(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
) -> Result<Json<Vec<ProjectView>>, ApiError> {
    blocking(&state.app, move |app| app.project_views(&app.sight(&by)?)).await
}

async fn project(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| {
        app.project_view_of(id, &app.sight(&by)?)
    })
    .await
}

async fn set_access(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(access): Json<Access>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| {
        app.set_project_access(&by, id, access.private, &access.members, &access.reason)
    })
    .await
}

async fn add_project(
    State(state): State<ApiState>,
    Json(new): Json<NewProject>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| app.add_project(&new.name, &new.key)).await
}

async fn rename_project(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(edited): Json<EditedProject>,
) -> Result<Json<ProjectView>, ApiError> {
    blocking(&state.app, move |app| {
        app.rename_project(&by, id, &edited.name, edited.key.as_deref())
    })
    .await
}

async fn get_scripts(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Vec<ScriptView>>, ApiError> {
    blocking(&state.app, move |app| scripts::scripts(app, id)).await
}

async fn get_script(
    State(state): State<ApiState>,
    UrlPath((id, column)): UrlPath<(i64, String)>,
) -> Result<Json<ScriptView>, ApiError> {
    blocking(&state.app, move |app| scripts::script(app, id, &column)).await
}

async fn put_script(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((id, column)): UrlPath<(i64, String)>,
    Json(new): Json<NewScript>,
) -> Result<Json<ScriptView>, ApiError> {
    blocking(&state.app, move |app| {
        scripts::save_script(app, &by, id, &column, &new)
    })
    .await
}

async fn delete_script(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((id, column)): UrlPath<(i64, String)>,
) -> Result<Json<ScriptView>, ApiError> {
    blocking(&state.app, move |app| {
        scripts::delete_script(app, &by, id, &column)
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

    #[tokio::test]
    async fn a_repository_is_put_in_a_project_and_taken_out_but_not_from_its_home() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let (_, made) = call(
            "POST",
            &addr,
            &token,
            "/v1/projects",
            Some(json!({"name": "Storefront", "key": "SHOP"})),
        )
        .await;
        let shop = made["id"].as_i64().unwrap();
        let path = format!("/v1/projects/{shop}/repositories");
        let (status, added) =
            call("POST", &addr, &token, &path, Some(json!({"repository": 1}))).await;
        assert_eq!(status, Status::OK, "{added}");
        assert_eq!(
            added["repositories"],
            json!([{"id": 1, "name": "radiance", "home": false}])
        );
        let (status, missing) = call(
            "POST",
            &addr,
            &token,
            &path,
            Some(json!({"repository": 99})),
        )
        .await;
        assert_eq!(status, Status::NOT_FOUND, "{missing}");
        // The home project keeps its repository.
        let (status, kept) = call(
            "DELETE",
            &addr,
            &token,
            "/v1/projects/1/repositories/1",
            None,
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY, "{kept}");
        let (status, removed) = call("DELETE", &addr, &token, &format!("{path}/1"), None).await;
        assert_eq!(status, Status::OK, "{removed}");
        assert_eq!(removed["repositories"], json!([]));
        let (status, _) = call("DELETE", &addr, &token, &format!("{path}/1"), None).await;
        assert_eq!(status, Status::NOT_FOUND);
    }
}
