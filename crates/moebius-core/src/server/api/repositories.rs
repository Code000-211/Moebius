//! The repositories: adding one, with or without a forge, editing whom a repository on a forge trusts, and the column
//! scripts and files on a repository's default branch.
//!
//! - `POST /v1/repositories` `{name, url}`: clones a repository without a forge. With `forge: "github"`, `github:
//!   "owner/repo"` and optionally `trusted: [logins]` it is a repository whose issues are polled from GitHub; with
//!   `forge: "gitlab"` and `project: "group/project"`, one whose issues are polled from GitLab; with `forge: "forgejo"`
//!   and `project: "owner/repo"`, one on Forgejo, such as Codeberg.
//! - `PUT /v1/repositories/{id}` `{trusted}`: replaces the logins a repository on a forge trusts.
//! - `GET`, `PUT` and `DELETE /v1/repositories/{id}/scripts/{column}`: the script in force for a column, written or
//!   removed as a commit (see [`crate::board::scripts`]).
//! - `GET /v1/repositories/{id}/tree?path=` and `GET` and `PUT /v1/repositories/{id}/files/{path}`: the files on the
//!   default branch, read and committed (see [`crate::repos::files`]).
//! - `GET /v1/repositories/{id}/files?query=` and `GET /v1/repositories/{id}/search?q=&regex=&limit=`: the paths of the
//!   default branch for "Go to file", and a full-text search over it (see [`crate::repos::search`]).

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{get, post, put};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking};
use crate::app::AppError;
use crate::board::scripts::{self, NewScript, ScriptView};
use crate::decide::Actor;
use crate::repos::files::{self, FileView, NewFile, Tree};
use crate::repos::search::{self, Found, Paths, Search};
use crate::store::Repository;

/// The routes of the repositories, their scripts and their files.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/repositories", post(add_repository))
        .route("/v1/repositories/{id}", put(edit_repository))
        .route(
            "/v1/repositories/{id}/scripts/{column}",
            get(get_script).put(put_script).delete(delete_script),
        )
        .route("/v1/repositories/{id}/tree", get(get_tree))
        .route("/v1/repositories/{id}/files", get(get_paths))
        .route("/v1/repositories/{id}/search", get(get_search))
        .route(
            "/v1/repositories/{id}/files/{*path}",
            get(get_file).put(put_file),
        )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewRepository {
    pub name: String,
    pub url: String,
    /// `none` (the default), `github`, `gitlab` or `forgejo`.
    #[serde(default)]
    pub forge: Option<String>,
    /// The repository on GitHub as `owner/repo`; needed for the forge `github`.
    #[serde(default)]
    pub github: Option<String>,
    /// The project's path on GitLab, such as `group/project`, or the repository on Forgejo as `owner/repo`; needed
    /// for the forges `gitlab` and `forgejo`.
    #[serde(default)]
    pub project: Option<String>,
    /// The logins on the forge whose issues need no approval.
    #[serde(default)]
    pub trusted: Vec<String>,
}

/// The body of `PUT /v1/repositories/{id}`: the logins the repository trusts on its forge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditedRepository {
    pub trusted: Vec<String>,
    #[serde(default)]
    pub reason: String,
}

async fn add_repository(
    State(state): State<ApiState>,
    axum::Json(new): axum::Json<NewRepository>,
) -> Result<axum::Json<Repository>, ApiError> {
    blocking(&state.app, move |app| {
        match (
            new.forge.as_deref(),
            new.github.as_deref(),
            new.project.as_deref(),
        ) {
            (None | Some("none"), None, None) if new.trusted.is_empty() => {
                app.add_repository(&new.name, &new.url)
            }
            (Some("github"), Some(github), None) => {
                app.add_github_repository(&new.name, &new.url, github, &new.trusted)
            }
            (Some(forge @ ("gitlab" | "forgejo")), None, Some(project)) => {
                app.add_project_repository(&new.name, &new.url, forge, project, &new.trusted)
            }
            _ => Err(AppError::Invalid(
                "a repository is either without a forge, or has forge \"github\" with github \"owner/repo\", \
                 forge \"gitlab\" with project \"group/project\", or forge \"forgejo\" with project \
                 \"owner/repo\""
                    .into(),
            )),
        }
    })
    .await
}

async fn edit_repository(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(edited): axum::Json<EditedRepository>,
) -> Result<axum::Json<Repository>, ApiError> {
    blocking(&state.app, move |app| {
        app.edit_repository(&by, id, &edited.trusted, &edited.reason)
    })
    .await
}

async fn get_script(
    State(state): State<ApiState>,
    UrlPath((id, column)): UrlPath<(i64, String)>,
) -> Result<axum::Json<ScriptView>, ApiError> {
    blocking(&state.app, move |app| scripts::script(app, id, &column)).await
}

async fn put_script(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((id, column)): UrlPath<(i64, String)>,
    axum::Json(new): axum::Json<NewScript>,
) -> Result<axum::Json<ScriptView>, ApiError> {
    blocking(&state.app, move |app| {
        scripts::save_script(app, &by, id, &column, &new)
    })
    .await
}

async fn delete_script(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((id, column)): UrlPath<(i64, String)>,
) -> Result<axum::Json<ScriptView>, ApiError> {
    blocking(&state.app, move |app| {
        scripts::delete_script(app, &by, id, &column)
    })
    .await
}

/// Which directory `GET /v1/repositories/{id}/tree` lists; the root when it is left out.
#[derive(Debug, Deserialize)]
struct TreeQuery {
    #[serde(default)]
    path: String,
}

async fn get_tree(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Query(query): Query<TreeQuery>,
) -> Result<axum::Json<Tree>, ApiError> {
    blocking(&state.app, move |app| files::tree(app, id, &query.path)).await
}

/// What `GET /v1/repositories/{id}/files` narrows the paths to; all of them when it is left out.
#[derive(Debug, Deserialize)]
struct PathsQuery {
    #[serde(default)]
    query: String,
}

async fn get_paths(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Query(query): Query<PathsQuery>,
) -> Result<axum::Json<Paths>, ApiError> {
    blocking(&state.app, move |app| search::paths(app, id, &query.query)).await
}

async fn get_search(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Query(query): Query<Search>,
) -> Result<axum::Json<Found>, ApiError> {
    blocking(&state.app, move |app| search::search(app, id, &query)).await
}

async fn get_file(
    State(state): State<ApiState>,
    UrlPath((id, path)): UrlPath<(i64, String)>,
) -> Result<axum::Json<FileView>, ApiError> {
    blocking(&state.app, move |app| files::file(app, id, &path)).await
}

async fn put_file(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((id, path)): UrlPath<(i64, String)>,
    axum::Json(new): axum::Json<NewFile>,
) -> Result<axum::Json<FileView>, ApiError> {
    blocking(&state.app, move |app| {
        files::save_file(app, &by, id, &path, &new)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::testing::start;

    #[tokio::test]
    async fn a_repository_name_that_is_taken_is_refused_before_cloning() {
        let (_dir, state, addr) = start().await;
        let response = reqwest::Client::new()
            .post(format!("http://{addr}/v1/repositories"))
            .bearer_auth(&*state.token)
            .json(&serde_json::json!({"name": "radiance", "url": "/nowhere"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 422);
        assert!(response.text().await.unwrap().contains("is taken"));
        assert!(!state.app.data_dir.join("repos/radiance").exists());
    }

    #[tokio::test]
    async fn a_github_repository_is_accepted_with_its_trusted_logins() {
        use crate::repos::git;
        let (dir, state, addr) = start().await;
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        git(
            &origin,
            &["commit", "--quiet", "--allow-empty", "-m", "first"],
        )
        .unwrap();
        let post = |body: serde_json::Value| {
            reqwest::Client::new()
                .post(format!("http://{addr}/v1/repositories"))
                .bearer_auth(&*state.token)
                .json(&body)
                .send()
        };
        let url = origin.to_str().unwrap();

        let added = post(serde_json::json!({
            "name": "widgets", "url": url, "forge": "github",
            "github": "acme/widgets", "trusted": ["leon"],
        }))
        .await
        .unwrap();
        assert_eq!(added.status(), 200);
        let repo: Repository = added.json().await.unwrap();
        assert_eq!(
            (
                repo.forge.as_str(),
                repo.github.as_deref(),
                repo.trusted.as_slice()
            ),
            (
                "github",
                Some("acme/widgets"),
                ["leon".to_string()].as_slice()
            )
        );
        assert_eq!(state.app.store.repository(repo.id).unwrap(), Some(repo));

        for body in [
            serde_json::json!({"name": "a", "url": url, "forge": "github"}),
            serde_json::json!({"name": "b", "url": url, "forge": "github", "github": "widgets"}),
            serde_json::json!({"name": "c", "url": url, "github": "acme/widgets"}),
        ] {
            assert_eq!(post(body).await.unwrap().status(), 422);
        }

        let added = post(serde_json::json!({
            "name": "tools", "url": url, "forge": "gitlab",
            "project": "acme/platform/tools", "trusted": ["leon"],
        }))
        .await
        .unwrap();
        assert_eq!(added.status(), 200);
        let repo: Repository = added.json().await.unwrap();
        assert_eq!(
            (
                repo.forge.as_str(),
                repo.project.as_deref(),
                repo.github.as_deref()
            ),
            ("gitlab", Some("acme/platform/tools"), None)
        );
        for body in [
            serde_json::json!({"name": "d", "url": url, "forge": "gitlab"}),
            serde_json::json!({"name": "e", "url": url, "forge": "gitlab", "project": "tools"}),
            serde_json::json!({"name": "f", "url": url, "forge": "gitlab", "project": "acme/../tools"}),
            serde_json::json!({"name": "g", "url": url, "forge": "gitlab", "github": "acme/tools"}),
            serde_json::json!({"name": "h", "url": url, "project": "acme/tools"}),
        ] {
            assert_eq!(post(body).await.unwrap().status(), 422);
        }

        let added = post(serde_json::json!({
            "name": "gadgets", "url": url, "forge": "forgejo",
            "project": "acme/gadgets", "trusted": ["leon"],
        }))
        .await
        .unwrap();
        assert_eq!(added.status(), 200);
        let repo: Repository = added.json().await.unwrap();
        assert_eq!(
            (
                repo.forge.as_str(),
                repo.project.as_deref(),
                repo.trusted.as_slice()
            ),
            (
                "forgejo",
                Some("acme/gadgets"),
                ["leon".to_string()].as_slice()
            )
        );
        for body in [
            serde_json::json!({"name": "i", "url": url, "forge": "forgejo"}),
            serde_json::json!({"name": "j", "url": url, "forge": "forgejo", "project": "gadgets"}),
            serde_json::json!({"name": "k", "url": url, "forge": "forgejo", "project": "acme/sub/gadgets"}),
            serde_json::json!({"name": "l", "url": url, "forge": "forgejo", "project": ".acme/gadgets"}),
            serde_json::json!({"name": "m", "url": url, "forge": "forgejo", "github": "acme/gadgets"}),
            serde_json::json!({"name": "n", "url": url, "forge": "codeberg", "project": "acme/gadgets"}),
        ] {
            assert_eq!(post(body).await.unwrap().status(), 422);
        }
    }
}
