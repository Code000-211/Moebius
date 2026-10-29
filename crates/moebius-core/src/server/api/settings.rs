//! How the server is set up, read and changed while it runs (see [`crate::server::settings`] and
//! [`crate::server::setup`]); never with a secret in what is answered.
//!
//! - `GET /v1/models` and `GET /v1/plugins`: the router's upstreams, the agents, the teacher, the decision model and
//!   the plugins. `GET /v1/models/available?refresh=`: the models the upstreams list, asked once and kept.
//! - `GET /v1/start`: whether the upstreams, Pi-Bolt, the Coder's model and a repository are in place, step by step.
//! - `GET /v1/settings`: every key of `moebius.toml` with its value or default. `PUT /v1/settings` `{set, remove}` changes
//!   the file. `PUT` and `DELETE /v1/secrets/{name}` keep and remove an API key. `POST /v1/server/restart` restarts the
//!   server.

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking, person_only};
use crate::app::AppError;
use crate::decide::Actor;
use crate::server::settings::{Changes, Saved, SettingsView};
use crate::server::setup;

/// The routes of the setup and the settings.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/models/available", get(available_models))
        .route("/v1/start", get(start_steps))
        .route("/v1/plugins", get(plugins))
        .route("/v1/settings", get(settings).put(put_settings))
        .route("/v1/secrets/{name}", put(put_secret).delete(delete_secret))
        .route("/v1/server/restart", post(restart_server))
}

async fn models(State(state): State<ApiState>) -> axum::Json<setup::Models> {
    axum::Json(setup::models(&state.config.get(), state.runs.agents()))
}

/// What `GET /v1/models/available` takes.
#[derive(Deserialize)]
struct AvailableQuery {
    #[serde(default)]
    refresh: bool,
}

async fn available_models(
    State(state): State<ApiState>,
    axum::extract::Query(query): axum::extract::Query<AvailableQuery>,
) -> axum::Json<crate::router::AvailableModels> {
    axum::Json(state.router.available_models(query.refresh).await)
}

async fn start_steps(State(state): State<ApiState>) -> Result<axum::Json<setup::Start>, ApiError> {
    let available = state.router.available_models(false).await;
    let agents = state.runs.agents();
    let repositories = blocking(&state.app, |app| Ok(app.store.repositories()?.len()))
        .await?
        .0;
    let config = state.config.get();
    Ok(axum::Json(
        setup::start(&config, available, &agents, repositories).await,
    ))
}

async fn plugins(State(state): State<ApiState>) -> axum::Json<setup::Plugins> {
    axum::Json(setup::plugins(&state.app.decisions))
}

async fn settings(State(state): State<ApiState>) -> Result<axum::Json<SettingsView>, ApiError> {
    let live = state.config.clone();
    blocking(&state.app, move |_| live.view()).await
}

async fn put_settings(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    axum::Json(changes): axum::Json<Changes>,
) -> Result<axum::Json<Saved>, ApiError> {
    let live = state.config.clone();
    blocking(&state.app, move |app| live.save(app, &by, &changes)).await
}

/// What `PUT /v1/secrets/{name}` takes. Not `Debug`, so that the key cannot be printed by accident.
#[derive(Deserialize)]
struct NewSecret {
    value: String,
}

async fn put_secret(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
    axum::Json(secret): axum::Json<NewSecret>,
) -> Result<axum::Json<Vec<String>>, ApiError> {
    person_only(&by)?;
    let live = state.config.clone();
    blocking(&state.app, move |_| {
        live.secrets().set(&name, &secret.value)?;
        live.secrets().names()
    })
    .await
}

async fn delete_secret(
    State(state): State<ApiState>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<Vec<String>>, ApiError> {
    let live = state.config.clone();
    blocking(&state.app, move |_| {
        if !live.secrets().remove(&name)? {
            return Err(AppError::NotFound(format!("secret {name}")));
        }
        live.secrets().names()
    })
    .await
}

/// How long `POST /v1/server/restart` waits before it restarts the server, so that its answer is sent first.
const RESTART_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

async fn restart_server(State(state): State<ApiState>) -> StatusCode {
    let live = state.config.clone();
    tokio::spawn(async move {
        tokio::time::sleep(RESTART_DELAY).await;
        live.request_restart();
    });
    StatusCode::ACCEPTED
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::response::IntoResponse;

    use super::*;
    use crate::router::Protocol;
    use crate::server::api::testing::start;

    /// Upstreams that list models: an OpenAI one that wants its key, an Anthropic one, and one that refuses. Each
    /// counts the requests it gets.
    async fn fake_model_lists() -> (
        Vec<crate::router::Upstream>,
        Arc<std::sync::atomic::AtomicUsize>,
    ) {
        use crate::router::Upstream;
        use axum::http::HeaderMap;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let asked = Arc::new(AtomicUsize::new(0));
        let serve = |app: axum::Router| async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move { axum::serve(listener, app).await });
            url
        };
        let count = asked.clone();
        let openai = serve(axum::Router::new().route(
            "/models",
            axum::routing::get(move |headers: HeaderMap| async move {
                count.fetch_add(1, Ordering::SeqCst);
                if headers.get("authorization").map(|v| v.as_bytes()) != Some(b"Bearer sk-openai") {
                    return (StatusCode::UNAUTHORIZED, "no").into_response();
                }
                axum::Json(serde_json::json!({"data": [{"id": "qwen"}, {"id": "llama"}]}))
                    .into_response()
            }),
        ))
        .await;
        let count = asked.clone();
        let anthropic = serve(axum::Router::new().route(
            "/v1/models",
            axum::routing::get(move |headers: HeaderMap| async move {
                count.fetch_add(1, Ordering::SeqCst);
                assert_eq!(headers["x-api-key"], "sk-anthropic");
                assert!(headers.contains_key("anthropic-version"));
                axum::Json(serde_json::json!({"data": [{"id": "claude-b"}, {"id": "claude-a"}]}))
            }),
        ))
        .await;
        let count = asked.clone();
        let refusing = serve(axum::Router::new().route(
            "/models",
            axum::routing::get(move || async move {
                count.fetch_add(1, Ordering::SeqCst);
                StatusCode::UNAUTHORIZED
            }),
        ))
        .await;
        let upstream = |protocol, base_url: String, key: Option<&str>| Upstream {
            protocol,
            base_url,
            api_key: key.map(str::to_string),
        };
        (
            vec![
                upstream(Protocol::OpenAi, openai.clone(), Some("sk-openai")),
                // Listed twice, but asked once.
                upstream(Protocol::OpenAi, openai, Some("sk-openai")),
                upstream(Protocol::Anthropic, anthropic, Some("sk-anthropic")),
                upstream(Protocol::OpenAi, refusing, Some("sk-wrong")),
            ],
            asked,
        )
    }

    #[tokio::test]
    async fn available_models_are_asked_of_the_upstreams_and_kept_until_refreshed() {
        use std::sync::atomic::Ordering;
        let (_dir, state, addr) = start().await;
        let (upstreams, asked) = fake_model_lists().await;
        let refusing = upstreams[3].base_url.clone();
        state.router.set_upstreams(upstreams);
        let fetch = |path: &'static str| {
            let token = state.token.clone();
            let addr = addr.clone();
            async move {
                reqwest::Client::new()
                    .get(format!("http://{addr}{path}"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
            }
        };

        let text = fetch("/v1/models/available").await;
        let answer: serde_json::Value = serde_json::from_str(&text).unwrap();
        let ids: Vec<(&str, &str)> = answer["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| (m["protocol"].as_str().unwrap(), m["id"].as_str().unwrap()))
            .collect();
        assert_eq!(
            ids,
            [
                ("anthropic", "claude-a"),
                ("anthropic", "claude-b"),
                ("openai", "llama"),
                ("openai", "qwen"),
            ]
        );
        assert!(
            answer["models"][2]["upstream"]
                .as_str()
                .unwrap()
                .starts_with("http://127.0.0.1:")
        );
        assert_eq!(
            answer["errors"],
            serde_json::json!([{"upstream": refusing, "message": "401"}])
        );
        for key in ["sk-openai", "sk-anthropic", "sk-wrong"] {
            assert!(!text.contains(key), "{key}");
        }
        assert_eq!(asked.load(Ordering::SeqCst), 3);

        // The second answer is the kept one; a refresh asks again.
        assert_eq!(fetch("/v1/models/available").await, text);
        assert_eq!(asked.load(Ordering::SeqCst), 3);
        assert_eq!(fetch("/v1/models/available?refresh=true").await, text);
        assert_eq!(asked.load(Ordering::SeqCst), 6);
    }
}
