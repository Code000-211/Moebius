//! The outgoing webhooks an owner adds, which post the events of Moebius to a URL (see `server::outgoing`).
//!
//! - `GET /v1/outgoing-webhooks`: every hook as `{id, url, kinds, projects, created_at}`, `projects` being the names
//!   of the repositories it names (empty when it names none). Never the secret.
//! - `POST /v1/outgoing-webhooks` `{url, kinds, projects}`: adds a hook. The answer is the hook with its `secret`, the
//!   one time it is shown; the secret is kept in `secrets.toml` as `outgoing-webhook-<id>`.
//! - `DELETE /v1/outgoing-webhooks/{id}`: removes a hook and its secret.
//!
//! - `PUT /v1/repositories/{id}/private` `{private}`: makes a repository private, so that its events go only to a hook
//!   that names it, or public again.
//!
//! Only an owner may use them, and an agent's run token never.

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, internal};
use crate::blobs::remember_secret;
use crate::server::outgoing::secret_name;
use crate::server::webhooks;
use crate::store::OutgoingWebhook;

/// The routes of the outgoing webhooks.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/outgoing-webhooks", get(list).post(add))
        .route("/v1/outgoing-webhooks/{id}", delete(remove))
        .route("/v1/repositories/{id}/private", put(set_private))
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::UNPROCESSABLE_ENTITY, message.into())
}

/// A hook as the Settings page shows it.
#[derive(Serialize)]
struct HookView {
    id: i64,
    url: String,
    kinds: Vec<String>,
    /// The names of the repositories it names.
    projects: Vec<String>,
    created_at: i64,
}

/// The answer to adding a hook: the hook and its secret. Not `Debug`, so that the secret cannot be printed by accident.
#[derive(Serialize)]
struct Added {
    #[serde(flatten)]
    hook: HookView,
    secret: String,
}

#[derive(Deserialize)]
struct NewHook {
    url: String,
    kinds: Vec<String>,
    #[serde(default)]
    projects: Vec<String>,
}

fn view(state: &ApiState, hook: OutgoingWebhook) -> Result<HookView, ApiError> {
    let repositories = state.app.store.repositories().map_err(internal)?;
    Ok(HookView {
        id: hook.id,
        url: hook.url,
        kinds: hook.kinds,
        projects: hook
            .projects
            .iter()
            .filter_map(|id| repositories.iter().find(|r| r.id == *id))
            .map(|r| r.name.clone())
            .collect(),
        created_at: hook.created_at,
    })
}

async fn list(State(state): State<ApiState>) -> Result<Json<Vec<HookView>>, ApiError> {
    let hooks = state.app.store.outgoing_webhooks().map_err(internal)?;
    hooks
        .into_iter()
        .map(|hook| view(&state, hook))
        .collect::<Result<_, _>>()
        .map(Json)
}

async fn add(
    State(state): State<ApiState>,
    Json(body): Json<NewHook>,
) -> Result<Json<Added>, ApiError> {
    let url = body.url.trim();
    let parsed = reqwest::Url::parse(url).map_err(|_| invalid("the url is not a URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(invalid("the url is an http or https address"));
    }
    let kinds: Vec<String> = body.kinds.iter().map(|k| k.trim().to_string()).collect();
    let is_kind = |k: &String| {
        !k.is_empty()
            && k.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    if kinds.is_empty() || !kinds.iter().all(is_kind) {
        return Err(invalid(
            "kinds names at least one event kind, such as card_moved",
        ));
    }
    let repositories = state.app.store.repositories().map_err(internal)?;
    let mut projects = Vec::new();
    for name in &body.projects {
        let repository = repositories
            .iter()
            .find(|r| r.name == *name)
            .ok_or_else(|| invalid(format!("{name:?} is no project")))?;
        projects.push(repository.id);
    }
    let secret = webhooks::generate().map_err(internal)?;
    let hook = state
        .app
        .store
        .add_outgoing_webhook(url, &kinds, &projects)
        .map_err(internal)?;
    remember_secret(&secret);
    if let Err(e) = state.config.secrets().set(&secret_name(hook.id), &secret) {
        let _ = state.app.store.delete_outgoing_webhook(hook.id);
        return Err(internal(e));
    }
    Ok(Json(Added {
        hook: view(&state, hook)?,
        secret,
    }))
}

async fn remove(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<StatusCode, ApiError> {
    if !state
        .app
        .store
        .delete_outgoing_webhook(id)
        .map_err(internal)?
    {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            format!("outgoing webhook {id}"),
        ));
    }
    state
        .config
        .secrets()
        .remove(&secret_name(id))
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct Private {
    private: bool,
}

async fn set_private(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Json(body): Json<Private>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let known = state
        .app
        .store
        .repositories()
        .map_err(internal)?
        .iter()
        .any(|r| r.id == id);
    if !known {
        return Err(ApiError(StatusCode::NOT_FOUND, format!("repository {id}")));
    }
    state
        .app
        .store
        .set_repository_private(id, body.private)
        .map_err(internal)?;
    Ok(Json(serde_json::json!({"id": id, "private": body.private})))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::server::{Server, api, fake_github};

    async fn serve(server: &Server) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(api::serve(listener, server.api_state()));
        url
    }

    #[tokio::test]
    async fn an_owner_adds_lists_and_removes_a_hook() {
        let f = fake_github::Fixture::new().await;
        let (server, repository) = (f.server, f.repository);
        let url = serve(&server).await;
        let client = reqwest::Client::new();
        let call = |method: reqwest::Method, path: &str| {
            client
                .request(method, format!("{url}{path}"))
                .bearer_auth(&*server.token)
        };

        let added = call(reqwest::Method::POST, "/v1/outgoing-webhooks")
            .json(
                &json!({"url": "http://receiver.test/in", "kinds": ["card_moved"],
                "projects": [repository.name]}),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(added.status(), 200);
        let added: Value = added.json().await.unwrap();
        let id = added["id"].as_i64().unwrap();
        let secret = added["secret"].as_str().unwrap().to_string();
        assert!(secret.starts_with("whsec_"));
        assert_eq!(added["projects"], json!([repository.name]));
        assert_eq!(
            server
                .live
                .secrets()
                .get(&format!("outgoing-webhook-{id}"))
                .unwrap(),
            Some(secret.clone())
        );

        let listed = call(reqwest::Method::GET, "/v1/outgoing-webhooks")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(listed.contains("receiver.test") && !listed.contains(&secret));

        let gone = call(
            reqwest::Method::DELETE,
            &format!("/v1/outgoing-webhooks/{id}"),
        )
        .send()
        .await
        .unwrap();
        assert_eq!(gone.status(), 204);
        assert_eq!(
            server
                .live
                .secrets()
                .get(&format!("outgoing-webhook-{id}"))
                .unwrap(),
            None
        );
        let again = call(
            reqwest::Method::DELETE,
            &format!("/v1/outgoing-webhooks/{id}"),
        )
        .send()
        .await
        .unwrap();
        assert_eq!(again.status(), 404);
    }

    #[tokio::test]
    async fn a_hook_that_makes_no_sense_is_refused() {
        let f = fake_github::Fixture::new().await;
        let server = f.server;
        let url = serve(&server).await;
        let client = reqwest::Client::new();
        for body in [
            json!({"url": "ftp://x.test/", "kinds": ["card_moved"]}),
            json!({"url": "not a url", "kinds": ["card_moved"]}),
            json!({"url": "http://x.test/", "kinds": []}),
            json!({"url": "http://x.test/", "kinds": ["Card Moved"]}),
            json!({"url": "http://x.test/", "kinds": ["card_moved"], "projects": ["nope"]}),
        ] {
            let status = client
                .post(format!("{url}/v1/outgoing-webhooks"))
                .bearer_auth(&*server.token)
                .json(&body)
                .send()
                .await
                .unwrap()
                .status();
            assert_eq!(status, 422, "{body}");
        }
        assert!(server.app.store.outgoing_webhooks().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_owner_makes_a_repository_private_and_public_again() {
        let f = fake_github::Fixture::new().await;
        let (server, repository) = (f.server, f.repository);
        let url = serve(&server).await;
        let put = |path: String, body: Value| {
            reqwest::Client::new()
                .put(format!("{url}{path}"))
                .bearer_auth(&*server.token)
                .json(&body)
                .send()
        };
        let path = format!("/v1/repositories/{}/private", repository.id);
        assert!(
            !server
                .app
                .store
                .repository_is_private(repository.id)
                .unwrap()
        );
        let on = put(path.clone(), json!({"private": true})).await.unwrap();
        assert_eq!(on.status(), 200);
        assert!(
            server
                .app
                .store
                .repository_is_private(repository.id)
                .unwrap()
        );
        put(path, json!({"private": false})).await.unwrap();
        assert!(
            !server
                .app
                .store
                .repository_is_private(repository.id)
                .unwrap()
        );
        let missing = put(
            "/v1/repositories/9999/private".into(),
            json!({"private": true}),
        )
        .await
        .unwrap();
        assert_eq!(missing.status(), 404);
    }
}
