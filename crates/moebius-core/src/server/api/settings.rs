//! How the server is set up, read and changed while it runs (see [`crate::server::settings`] and
//! [`crate::server::setup`]); never with a secret in what is answered.
//!
//! - `GET /v1/models` and `GET /v1/plugins`: the router's upstreams, the agents, the teacher, the decision model and
//!   the plugins. `GET /v1/models/available?refresh=`: the models the upstreams list, asked once and kept.
//! - `POST /v1/models/upstreams/{place}/login` `{accept_terms}`: a person starts the OAuth login of an upstream, having
//!   confirmed the warning about the provider's terms; `GET` on the same route tells how it stands.
//! - `GET /v1/start`: whether the upstreams, Pi-Bolt, the Coder's model and a repository are in place, step by step.
//! - `GET /v1/settings`: every key of `moebius.toml` with its value or default. `PUT /v1/settings` `{set, remove}` changes
//!   the file. `PUT` and `DELETE /v1/secrets/{name}` keep and remove an API key. `POST /v1/server/restart` restarts the
//!   server.

use std::collections::BTreeMap;

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking, person_only};
use crate::app::AppError;
use crate::decide::Actor;
use crate::router::{Login, LoginError, TERMS_WARNING};
use crate::server::settings::{Changes, Saved, SettingsView};
use crate::server::setup;

/// The routes of the setup and the settings.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/models/available", get(available_models))
        .route(
            "/v1/models/upstreams/{place}/login",
            get(login).post(start_login),
        )
        .route("/v1/start", get(start_steps))
        .route("/v1/plugins", get(plugins))
        .route("/v1/settings", get(settings).put(put_settings))
        .route("/v1/secrets/{name}", put(put_secret).delete(delete_secret))
        .route("/v1/server/restart", post(restart_server))
}

async fn models(State(state): State<ApiState>) -> Result<axum::Json<setup::Models>, ApiError> {
    let secrets = state.config.secrets().names()?;
    Ok(axum::Json(setup::models(
        &state.config.get(),
        state.runs.agents(),
        &secrets,
    )))
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

/// What `POST /v1/models/upstreams/{place}/login` takes.
#[derive(Deserialize)]
struct StartLogin {
    /// Whether the person confirmed `router::TERMS_WARNING`.
    #[serde(default)]
    accept_terms: bool,
}

fn login_error(e: LoginError) -> ApiError {
    let status = match e {
        LoginError::NoUpstream(_) => StatusCode::NOT_FOUND,
        LoginError::NotOAuth(_) | LoginError::TermsNotAccepted => StatusCode::UNPROCESSABLE_ENTITY,
        LoginError::Provider(_) => StatusCode::BAD_GATEWAY,
    };
    ApiError(status, e.to_string())
}

async fn start_login(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(place): UrlPath<usize>,
    axum::Json(request): axum::Json<StartLogin>,
) -> Result<axum::Json<Login>, ApiError> {
    person_only(&by)?;
    let login = state
        .router
        .start_login(place, request.accept_terms)
        .await
        .map_err(login_error)?;
    let upstream = state
        .config
        .get()
        .upstreams
        .get(place)
        .map(|u| setup::without_credentials(&u.base_url))
        .unwrap_or_default();
    let _recorded = blocking(&state.app, move |app| {
        let context = BTreeMap::from([
            ("upstream".to_string(), upstream),
            ("warning".to_string(), TERMS_WARNING.to_string()),
        ]);
        app.record_action(&by, "oauth_login", None, None, context, "")
    })
    .await?;
    Ok(axum::Json(login))
}

async fn login(
    State(state): State<ApiState>,
    UrlPath(place): UrlPath<usize>,
) -> Result<axum::Json<Login>, ApiError> {
    state
        .router
        .login(place)
        .await
        .map(axum::Json)
        .map_err(login_error)
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
            oauth: None,
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

    #[tokio::test]
    async fn a_person_logs_an_upstream_in_after_confirming_the_warning() {
        use crate::router::oauth_testing::{FakeProvider, config};
        use crate::router::{Flow, Upstream};
        let (_dir, state, addr) = start().await;
        let provider = FakeProvider::start(3600).await;
        state.router.set_upstreams(vec![Upstream {
            protocol: Protocol::OpenAi,
            base_url: provider.url.clone(),
            api_key: None,
            oauth: Some(config(&provider.url, Flow::Device)),
        }]);
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/v1/models/upstreams/0/login");
        let post = |body: serde_json::Value| {
            client
                .post(&url)
                .bearer_auth(state.token.as_ref())
                .json(&body)
                .send()
        };

        let refused = post(serde_json::json!({})).await.unwrap();
        assert_eq!(refused.status(), 422);
        assert!(refused.text().await.unwrap().contains("third-party tool"));
        let missing = client
            .post(format!("http://{addr}/v1/models/upstreams/3/login"))
            .bearer_auth(state.token.as_ref())
            .json(&serde_json::json!({"accept_terms": true}))
            .send()
            .await
            .unwrap();
        assert_eq!(missing.status(), 404);

        let login: serde_json::Value = post(serde_json::json!({"accept_terms": true}))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(login["state"], "waiting");
        assert_eq!(login["user_code"], "WDJB-MJHT");
        let mut answer = serde_json::Value::Null;
        for _ in 0..1000 {
            answer = client
                .get(&url)
                .bearer_auth(state.token.as_ref())
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if answer["state"] != "waiting" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(answer["state"], "logged_in");
        assert!(!answer.to_string().contains("token-"), "{answer}");
        // The confirmation is recorded as the person's decision.
        let judgements = state.app.store.judgements("oauth_login").unwrap();
        assert_eq!(judgements.len(), 1);
    }
}
