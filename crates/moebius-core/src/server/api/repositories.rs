//! The repositories: adding one, with or without a forge, editing whom a repository on a forge trusts, removing
//! one, its settings, and the files on its default branch. The column scripts are a project's (see `projects`).
//!
//! - `POST /v1/repositories` `{name, url}`: clones a repository without a forge. With `forge: "github"`, `github:
//!   "owner/repo"` and optionally `trusted: [logins]` it is a repository whose issues are polled from GitHub; with
//!   `forge: "gitlab"` and `project: "group/project"`, one whose issues are polled from GitLab; with `forge: "forgejo"`
//!   and `project: "owner/repo"`, one on Forgejo, such as Codeberg.
//! - `PUT /v1/repositories/{id}` `{trusted, mark_ready, ready_label, agents_take, commands, command_limit, answer_on_forge, merge_reviewed, require_measurement, locked_paths, daily_merge_limit, review_brief, testing_label, measurers}`: replaces the logins a repository on a
//!   forge trusts and how it works with others (each key left out stays as it is).
//! - `POST /v1/repositories/{id}/freeze` and `/unfreeze` `{reason}`: the owner's brake on the factory (see
//!   [`crate::app::factory::freeze`]); only a person unfreezes.
//! - `DELETE /v1/repositories/{id}`: removes the repository with its cards, changes, chats and the rest, and deletes
//!   its clone. Refused with `409` while one of its runs is still going.
//! - `GET /v1/repositories/{id}/settings` and `PUT` and `DELETE /v1/repositories/{id}/settings/{file}`: the files of
//!   the repository's settings in the harness configuration repository (see [`crate::repos::settings`]).
//! - `GET /v1/repositories/{id}/tree?path=` and `GET` and `PUT /v1/repositories/{id}/files/{path}`: the files on the
//!   default branch, read and committed (see [`crate::repos::files`]).
//! - `GET /v1/repositories/{id}/plugin-settings` and `PUT /v1/repositories/{id}/plugin-settings/{plugin}` `{values, reason}`: the
//!   settings plugins ask of the repository and the values it has, changed by a person (see
//!   [`crate::app::plugin_settings`]).
//! - `GET /v1/repositories/{id}/files?query=` and `GET /v1/repositories/{id}/search?q=&regex=&limit=`: the paths of the
//!   default branch for "Go to file", and a full-text search over it (see [`crate::repos::search`]).

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{get, post, put};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, Reason, blocking};
use crate::app::AppError;
use crate::app::plugin_settings::{PluginProjectSettings, PluginSettings, Scope};
use crate::decide::Actor;
use crate::repos::files::{self, FileView, NewFile, Tree};
use crate::repos::search::{self, Found, Paths, Search};
use crate::repos::settings::{self, NewSetting, SettingFile, Settings};
use crate::store::Repository;

/// The routes of the repositories, their settings and their files.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/repositories", post(add_repository))
        .route(
            "/v1/repositories/{id}",
            put(edit_repository).delete(delete_repository),
        )
        .route("/v1/repositories/{id}/freeze", post(freeze_repository))
        .route("/v1/repositories/{id}/unfreeze", post(unfreeze_repository))
        .route("/v1/repositories/{id}/settings", get(get_settings))
        .route(
            "/v1/repositories/{id}/settings/{file}",
            put(put_setting).delete(delete_setting),
        )
        .route(
            "/v1/repositories/{id}/plugin-settings",
            get(get_plugin_settings),
        )
        .route(
            "/v1/repositories/{id}/plugin-settings/{plugin}",
            put(put_plugin_settings),
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
    /// The logins whose issues need no approval; left out, they stay.
    #[serde(default)]
    pub trusted: Option<Vec<String>>,
    /// Whether the server marks the repository's ready issues with `ready_label`; left out, it stays.
    #[serde(default)]
    pub mark_ready: Option<bool>,
    /// The label of ready issues; left out, it stays.
    #[serde(default)]
    pub ready_label: Option<String>,
    /// Which issues the repository's agents take, `all` or `labelled`; left out, it stays.
    #[serde(default)]
    pub agents_take: Option<String>,
    /// Whether comments on issues may give the commands `/take`, `/release` and `/done`; left out, it stays.
    #[serde(default)]
    pub commands: Option<bool>,
    /// How many open issues a login may hold that it took with `/take` (1 to 50); left out, it stays.
    #[serde(default)]
    pub command_limit: Option<i64>,
    /// Whether the server writes the answer to every command as a comment on the issue; left out, it stays.
    #[serde(default)]
    pub answer_on_forge: Option<bool>,
    /// Whether the server merges a followed pull request the Reviewer approved; left out, it stays.
    #[serde(default)]
    pub merge_reviewed: Option<bool>,
    /// Whether a change merges only with an accepted measurement or the label `no-measurement`; left out, it stays.
    #[serde(default)]
    pub require_measurement: Option<bool>,
    /// The path globs that never merge without a person, besides the built-in ones; left out, they stay.
    #[serde(default)]
    pub locked_paths: Option<Vec<String>>,
    /// How many changes the server merges in 24 hours (1 to 1000); left out, it stays.
    #[serde(default)]
    pub daily_merge_limit: Option<i64>,
    /// The files of the default branch that are the repository's rules for the Reviewer; left out, they stay.
    #[serde(default)]
    pub review_brief: Option<Vec<String>>,
    /// The label of an issue whose code is right and whose measurement is missing; left out, it stays.
    #[serde(default)]
    pub testing_label: Option<String>,
    /// The logins whose `## Measurement` comments count; left out, they stay.
    #[serde(default)]
    pub measurers: Option<Vec<String>>,
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
        let mut repository = match &edited.trusted {
            Some(trusted) => app.edit_repository(&by, id, trusted, &edited.reason)?,
            None => app.repository(id)?,
        };
        let edit = crate::app::WorkingWithOthers {
            mark_ready: edited.mark_ready,
            ready_label: edited.ready_label,
            agents_take: edited.agents_take,
            commands: edited.commands,
            command_limit: edited.command_limit,
            answer_on_forge: edited.answer_on_forge,
            merge_reviewed: edited.merge_reviewed,
            factory: crate::app::factory::FactoryEdit {
                require_measurement: edited.require_measurement,
                locked_paths: edited.locked_paths,
                daily_merge_limit: edited.daily_merge_limit,
                review_brief: edited.review_brief,
                testing_label: edited.testing_label,
                measurers: edited.measurers,
            },
        };
        if edit.mark_ready.is_some()
            || edit.ready_label.is_some()
            || edit.agents_take.is_some()
            || edit.commands.is_some()
            || edit.command_limit.is_some()
            || edit.answer_on_forge.is_some()
            || edit.merge_reviewed.is_some()
            || edit.factory.require_measurement.is_some()
            || edit.factory.locked_paths.is_some()
            || edit.factory.daily_merge_limit.is_some()
            || edit.factory.review_brief.is_some()
            || edit.factory.testing_label.is_some()
            || edit.factory.measurers.is_some()
        {
            repository = app.edit_working_with_others(&by, id, &edit, &edited.reason)?;
        }
        Ok(repository)
    })
    .await
}

async fn freeze_repository(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<Repository>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        app.freeze_repository(&by, id, &reason)
    })
    .await
}

async fn unfreeze_repository(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<Repository>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        app.unfreeze_repository(&by, id, &reason)
    })
    .await
}

async fn delete_repository(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<Repository>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        app.remove_repository(&by, id, &reason)
    })
    .await
}

/// The body of `PUT /v1/repositories/{id}/plugin-settings/{plugin}`: every value of the plugin's settings, by key.
#[derive(Debug, Deserialize)]
pub(super) struct NewSettings {
    pub(super) values: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub(super) reason: String,
}

async fn get_plugin_settings(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<PluginProjectSettings>, ApiError> {
    blocking(&state.app, move |app| app.plugin_project_settings(id)).await
}

async fn put_plugin_settings(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((id, plugin)): UrlPath<(i64, String)>,
    axum::Json(new): axum::Json<NewSettings>,
) -> Result<axum::Json<PluginSettings>, ApiError> {
    blocking(&state.app, move |app| {
        app.save_plugin_settings(&by, Scope::Project(id), &plugin, &new.values, &new.reason)
    })
    .await
}

async fn get_settings(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Settings>, ApiError> {
    blocking(&state.app, move |app| settings::settings(app, id)).await
}

async fn put_setting(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((id, file)): UrlPath<(i64, String)>,
    axum::Json(new): axum::Json<NewSetting>,
) -> Result<axum::Json<SettingFile>, ApiError> {
    blocking(&state.app, move |app| {
        settings::save(app, &by, id, &file, &new)
    })
    .await
}

async fn delete_setting(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((id, file)): UrlPath<(i64, String)>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<SettingFile>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        settings::remove(app, &by, id, &file, &reason)
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

    #[tokio::test]
    async fn a_removed_repository_is_gone_with_its_cards_and_its_clone() {
        let (_dir, state, addr) = start().await;
        state
            .app
            .store
            .add_card(1, "Card", "", "leon", "Backlog")
            .unwrap();
        let client = reqwest::Client::new();
        let response = client
            .delete(format!("http://{addr}/v1/repositories/1"))
            .bearer_auth(&*state.token)
            .json(&serde_json::json!({"reason": "not wanted"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(state.app.store.repository(1).unwrap().is_none());
        assert!(state.app.store.cards(1).unwrap().is_empty());

        let again = client
            .delete(format!("http://{addr}/v1/repositories/1"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap();
        assert_eq!(again.status(), 404);
    }
    /// Adds a repository on GitHub that is a clone of a local repository, as the route does.
    async fn a_github_repository(
        dir: &std::path::Path,
        state: &ApiState,
        addr: &str,
    ) -> Repository {
        use crate::repos::git;
        let origin = dir.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        git(
            &origin,
            &["commit", "--quiet", "--allow-empty", "-m", "first"],
        )
        .unwrap();
        reqwest::Client::new()
            .post(format!("http://{addr}/v1/repositories"))
            .bearer_auth(&*state.token)
            .json(&serde_json::json!({
                "name": "engine", "url": origin.to_str().unwrap(), "forge": "github",
                "github": "acme/engine", "trusted": [],
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn the_factory_settings_of_a_repository_are_edited_with_its_others_and_only_by_a_person()
    {
        let (dir, state, addr) = start().await;
        let repo = a_github_repository(dir.path(), &state, &addr).await;
        // The defaults.
        assert!(!repo.require_measurement);
        assert_eq!(repo.daily_merge_limit, 10);
        assert_eq!(repo.review_brief, ["REVIEW.md"]);
        assert_eq!(repo.testing_label, "ready-for-testing");
        assert!(repo.locked_paths.is_empty() && repo.measurers.is_empty());
        assert!(repo.frozen.is_none());

        let put = |token: String, body: serde_json::Value| {
            reqwest::Client::new()
                .put(format!("http://{addr}/v1/repositories/{}", repo.id))
                .bearer_auth(token)
                .json(&body)
                .send()
        };
        let person = state.token.to_string();
        let saved = put(
            person.clone(),
            serde_json::json!({
                "require_measurement": true,
                "locked_paths": ["kernels/**", " kernels/** ", ""],
                "daily_merge_limit": 25,
                "review_brief": ["notes/backlog/CARDS.md", "AGENTS.md"],
                "testing_label": "needs-bench",
                "measurers": ["ada", "grace"],
                "reason": "the owner's bar",
            }),
        )
        .await
        .unwrap();
        assert_eq!(saved.status(), 200);
        let saved: Repository = saved.json().await.unwrap();
        assert!(saved.require_measurement);
        assert_eq!(saved.locked_paths, ["kernels/**"]);
        assert_eq!(saved.daily_merge_limit, 25);
        assert_eq!(saved.review_brief, ["notes/backlog/CARDS.md", "AGENTS.md"]);
        assert_eq!(saved.testing_label, "needs-bench");
        assert_eq!(saved.measurers, ["ada", "grace"]);
        assert_eq!(state.app.store.repository(repo.id).unwrap(), Some(saved));
        let judgement = state.app.store.judgements("edit_repository").unwrap();
        assert_eq!(judgement.len(), 1);
        // What is left out stays, and an edit that changes nothing records nothing.
        let same = put(
            person.clone(),
            serde_json::json!({ "daily_merge_limit": 25 }),
        )
        .await
        .unwrap();
        assert_eq!(same.status(), 200);
        assert_eq!(
            state.app.store.judgements("edit_repository").unwrap().len(),
            1
        );
        assert!(same.json::<Repository>().await.unwrap().require_measurement);

        // Values that do not fit are refused, naming the setting.
        for (body, named) in [
            (
                serde_json::json!({ "daily_merge_limit": 0 }),
                "daily_merge_limit",
            ),
            (
                serde_json::json!({ "daily_merge_limit": 1001 }),
                "daily_merge_limit",
            ),
            (
                serde_json::json!({ "review_brief": ["../x"] }),
                "review_brief",
            ),
            (
                serde_json::json!({ "review_brief": ["/etc/passwd"] }),
                "review_brief",
            ),
            (
                serde_json::json!({ "measurers": ["not a login"] }),
                "measurers",
            ),
            (serde_json::json!({ "testing_label": "" }), "label"),
        ] {
            let refused = put(person.clone(), body).await.unwrap();
            assert_eq!(refused.status(), 422, "{named}");
            assert!(refused.text().await.unwrap().contains(named), "{named}");
        }

        // An agent's token does not loosen what holds merges back.
        let agent = crate::server::api::testing::agent_token(&state);
        let refused = put(
            agent.clone(),
            serde_json::json!({ "require_measurement": false }),
        )
        .await
        .unwrap();
        assert_eq!(refused.status(), 403);
        assert!(state.app.repository(repo.id).unwrap().require_measurement);
        // It may still change what it always could.
        let allowed = put(agent, serde_json::json!({ "commands": true }))
            .await
            .unwrap();
        assert_eq!(allowed.status(), 200);
    }

    #[tokio::test]
    async fn a_person_freezes_and_unfreezes_a_repository_and_an_agent_may_only_freeze_it() {
        let (dir, state, addr) = start().await;
        let repo = a_github_repository(dir.path(), &state, &addr).await;
        let post = |token: String, path: &str, body: serde_json::Value| {
            reqwest::Client::new()
                .post(format!("http://{addr}/v1/repositories/{}/{path}", repo.id))
                .bearer_auth(token)
                .json(&body)
                .send()
        };
        let person = state.token.to_string();
        // A body is optional, and says who froze it when it gives no reason.
        let frozen: Repository = post(person.clone(), "freeze", serde_json::json!({}))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let why = frozen.frozen.clone().unwrap();
        assert_eq!(why.source, "manual");
        assert!(why.reason.contains("person"), "{}", why.reason);
        // Freezing a frozen repository keeps the first reason, and there is one inbox item.
        let again: Repository = post(
            person.clone(),
            "freeze",
            serde_json::json!({"reason": "second"}),
        )
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(again.frozen.unwrap().reason, why.reason);
        let items = state.app.store.inbox().unwrap();
        assert_eq!(
            items.iter().filter(|i| i.kind == "factory_frozen").count(),
            1
        );
        assert_eq!(items[0].data["repository"], "engine");

        // An agent does not lift it; a person does, and the item goes.
        let agent = crate::server::api::testing::agent_token(&state);
        assert_eq!(
            post(agent, "unfreeze", serde_json::json!({}))
                .await
                .unwrap()
                .status(),
            403
        );
        let lifted: Repository = post(
            person.clone(),
            "unfreeze",
            serde_json::json!({"reason": "fixed"}),
        )
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert!(lifted.frozen.is_none());
        assert!(state.app.store.inbox().unwrap().is_empty());
        // An agent may freeze it (a brake needs no ceremony), with a reason.
        let agent = crate::server::api::testing::agent_token(&state);
        let by_agent: Repository =
            post(agent, "freeze", serde_json::json!({"reason": "odd merges"}))
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
        assert_eq!(by_agent.frozen.unwrap().reason, "odd merges");
        // A reason is one line.
        post(person.clone(), "unfreeze", serde_json::json!({}))
            .await
            .unwrap();
        let refused = post(
            person.clone(),
            "freeze",
            serde_json::json!({"reason": "a\nb"}),
        )
        .await
        .unwrap();
        assert_eq!(refused.status(), 422);
        assert_eq!(
            post(person, "freeze", serde_json::json!({}))
                .await
                .unwrap()
                .status(),
            200
        );
        let missing = reqwest::Client::new()
            .post(format!("http://{addr}/v1/repositories/999/freeze"))
            .bearer_auth(&*state.token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(missing.status(), 404);
    }
}
