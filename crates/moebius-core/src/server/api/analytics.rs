//! The analytics (issue #389): what the moves and runs say about the work.
//!
//! - `GET /v1/analytics?days=&repository=&agent=`: throughput, the time from start to done, the time in each column,
//!   the burn-up of the latest cycles and milestones and the tokens per done point, across the organisation and per
//!   project (`Analytics`). `days` is the window (1 to 365, default 30); `repository` and `agent` narrow every figure
//!   to the cards of that repository and the cards that agent ran something on. Cards of a private project the asker
//!   is no member of are in no figure; a `repository` they may not see is not found.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::app::{Analytics, AnalyticsQuery, AppError};
use crate::decide::Actor;

/// The routes of the analytics.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/analytics", get(analytics))
}

/// The query of `GET /v1/analytics`.
#[derive(Debug, Deserialize)]
struct Params {
    days: Option<i64>,
    repository: Option<i64>,
    agent: Option<String>,
}

async fn analytics(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    Query(params): Query<Params>,
) -> Result<Json<Analytics>, ApiError> {
    let query = AnalyticsQuery {
        days: params.days.unwrap_or(crate::app::ANALYTICS_DEFAULT_DAYS),
        repository: params.repository,
        agent: params.agent,
    };
    blocking(&state.app, move |app| {
        if let Some(id) = query.repository
            && !app.sight(&actor)?.repository(id)
        {
            return Err(AppError::NotFound(format!("repository {id}")));
        }
        app.analytics(&actor, &query)
    })
    .await
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode as Status;
    use serde_json::Value;

    use crate::server::api::testing::start;

    async fn get(addr: &str, token: &str, path: &str) -> (Status, Value) {
        let response = reqwest::Client::new()
            .get(format!("http://{addr}{path}"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (response.status(), response.json().await.unwrap_or_default())
    }

    #[tokio::test]
    async fn the_analytics_are_read_by_window_and_narrowed_by_repository_and_agent() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let card = state.app.store.cards(1).unwrap()[0].clone();
        state.app.store.move_card(card.id, "Done").unwrap();
        let run = state
            .app
            .store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        state
            .app
            .store
            .add_usage(&crate::store::Usage {
                run_id: Some(run.id),
                model: "m".into(),
                input_tokens: 90,
                output_tokens: 10,
                cached_tokens: 0,
                duration_ms: 1,
                tools: vec![],
            })
            .unwrap();

        let (status, all) = get(&addr, &token, "/v1/analytics").await;
        assert_eq!(status, Status::OK, "{all}");
        assert_eq!(all["days"], 30);
        assert_eq!(all["throughput"].as_array().unwrap().len(), 30);
        assert_eq!(all["totals"]["done"], 1);
        assert_eq!(all["totals"]["tokens"], 100);
        let repository = card.repository_id;
        for (path, done) in [
            (
                format!("/v1/analytics?days=7&repository={repository}&agent=Coder"),
                1,
            ),
            (format!("/v1/analytics?repository={}", repository + 99), 0),
            ("/v1/analytics?agent=Reviewer".to_string(), 0),
        ] {
            let (status, narrowed) = get(&addr, &token, &path).await;
            assert_eq!(status, Status::OK, "{path}");
            assert_eq!(narrowed["totals"]["done"], done, "{path}");
        }
        for days in ["0", "366", "x"] {
            let (status, _) = get(&addr, &token, &format!("/v1/analytics?days={days}")).await;
            assert!(status.is_client_error(), "{days}");
        }
    }
}
