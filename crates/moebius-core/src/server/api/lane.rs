//! The test lane's routes (ADR 0035).
//!
//! - `GET /v1/repositories/{id}/lane` and `PUT {lane, reason}`: the lane a repository uses (or `null`), the lanes of the
//!   configuration with what each isolates and warns of, whether the project has a valid `lane.toml`, and the latest jobs.
//! - `GET /v1/lanes`: every lane with its queue and whether it paused itself. `POST /v1/lanes/{name}/resume` `{reason}`:
//!   a person lifts the pause.
//! - `GET /v1/cards/{id}/lane`: the jobs of the card's change and why none is queued now. `POST /v1/cards/{id}/lane`: asks
//!   for a job now, which is queued or refused with the reason.

use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::app::{CardLane, LaneRequest, LaneView, RepositoryLane};
use crate::decide::Actor;

/// The routes of the lane.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/v1/repositories/{id}/lane",
            get(get_repository_lane).put(put_repository_lane),
        )
        .route("/v1/lanes", get(get_lanes))
        .route("/v1/lanes/{name}/resume", post(resume_lane))
        .route(
            "/v1/cards/{id}/lane",
            get(get_card_lane).post(post_card_lane),
        )
}

#[derive(Deserialize)]
struct EditedLane {
    /// The lane's name; `null` or an empty text for none.
    #[serde(default)]
    lane: Option<String>,
    #[serde(default)]
    reason: String,
}

#[derive(Deserialize, Default)]
struct Reason {
    #[serde(default)]
    reason: String,
}

async fn get_repository_lane(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<RepositoryLane>, ApiError> {
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        app.repository_lane_view(id, &sight)
    })
    .await
}

async fn put_repository_lane(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(edit): axum::Json<EditedLane>,
) -> Result<axum::Json<RepositoryLane>, ApiError> {
    blocking(&state.app, move |app| {
        app.edit_repository_lane(&by, id, edit.lane, &edit.reason)
    })
    .await
}

async fn get_lanes(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
) -> Result<axum::Json<Vec<LaneView>>, ApiError> {
    blocking(&state.app, move |app| app.lane_views(&app.sight(&by)?)).await
}

async fn resume_lane(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<Vec<LaneView>>, ApiError> {
    let reason = body.map(|b| b.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        app.resume_lane(&by, &name, &reason)?;
        app.lane_views(&app.sight(&by)?)
    })
    .await
}

async fn get_card_lane(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<CardLane>, ApiError> {
    blocking(&state.app, move |app| app.card_lane(id)).await
}

async fn post_card_lane(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<LaneRequest>, ApiError> {
    blocking(&state.app, move |app| app.request_lane_job(id)).await
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::config::{Isolation, LaneConfig};
    use crate::server::api::testing::{get, post, start};

    fn lane(isolation: Isolation) -> LaneConfig {
        LaneConfig {
            name: "gpu1".into(),
            runner: None,
            work_dir: None,
            isolation,
            runtime: "podman".into(),
            image: Some("rocm/dev:6.2".into()),
            devices: vec!["/dev/kfd".into()],
            groups: vec![],
            user: "65534:65534".into(),
            timeout_seconds: 600,
            memory: "16g".into(),
            cpus: None,
            disk: "20g".into(),
            gpus: Some("2x MI300X".into()),
            power_cap: Some("750 W".into()),
            rocm: Some("6.2.1".into()),
            driver: None,
        }
    }

    async fn put(state: &ApiState, addr: &str, path: &str, body: Value) -> reqwest::Response {
        reqwest::Client::new()
            .put(format!("http://{addr}{path}"))
            .bearer_auth(&*state.token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn the_settings_of_a_repository_show_its_lane_and_a_warning_while_it_is_not_isolated() {
        let (_dir, state, addr) = start().await;
        let repo = state.app.store.repositories().unwrap().pop().unwrap();
        state
            .app
            .lanes
            .configure(vec![lane(Isolation::Off)], vec![]);
        let path = format!("/v1/repositories/{}/lane", repo.id);
        let before: Value = get(&state, &addr, &path).await;
        assert_eq!(before["lane"], Value::Null);
        assert_eq!(before["lanes"][0]["name"], "gpu1");
        assert_eq!(before["definition"]["present"], false);

        let unknown = put(&state, &addr, &path, json!({ "lane": "nowhere" })).await;
        assert_eq!(unknown.status(), 422);
        let set = put(
            &state,
            &addr,
            &path,
            json!({ "lane": "gpu1", "reason": "the GPU box" }),
        )
        .await;
        assert_eq!(set.status(), 200);
        let after: Value = set.json().await.unwrap();
        assert_eq!(after["lane"], "gpu1");
        assert_eq!(after["current"]["isolation"], "off");
        assert!(
            after["current"]["warning"]
                .as_str()
                .unwrap()
                .contains("directly on the machine"),
            "{after}"
        );
        assert_eq!(after["current"]["gpus"], "2x MI300X");
        assert_eq!(
            state.app.store.repository_lane(repo.id).unwrap().as_deref(),
            Some("gpu1")
        );

        let lanes: Value = get(&state, &addr, "/v1/lanes").await;
        assert_eq!(lanes[0]["queued"], 0);
        assert_eq!(lanes[0]["paused"], Value::Null);
        let cleared = put(&state, &addr, &path, json!({ "lane": null })).await;
        assert_eq!(cleared.status(), 200);
        assert_eq!(state.app.store.repository_lane(repo.id).unwrap(), None);
    }

    #[tokio::test]
    async fn a_card_reports_why_it_has_no_job_and_a_request_says_the_same() {
        let (_dir, state, addr) = start().await;
        let repo = state.app.store.repositories().unwrap().pop().unwrap();
        state
            .app
            .lanes
            .configure(vec![lane(Isolation::Always)], vec![]);
        let card = state
            .app
            .store
            .add_card(repo.id, "Card", "", "person", "In review")
            .unwrap();
        let none: Value = get(&state, &addr, &format!("/v1/cards/{}/lane", card.id)).await;
        assert_eq!(none["lane"], Value::Null);
        assert_eq!(none["jobs"], json!([]));
        state
            .app
            .store
            .set_repository_lane(repo.id, Some("gpu1"))
            .unwrap();
        let asked = post(
            &state,
            &addr,
            format!("/v1/cards/{}/lane", card.id),
            json!({}),
        )
        .await;
        assert_eq!(asked.status(), 200);
        let asked: Value = asked.json().await.unwrap();
        assert_eq!(asked["job"], Value::Null);
        assert!(
            asked["why"].as_str().unwrap().contains("no open change"),
            "{asked}"
        );
    }
}
