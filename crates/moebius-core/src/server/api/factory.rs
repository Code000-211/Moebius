//! The software factory's routes (ADR 0032): the stale settings of a repository, the backlog of a project and its sync,
//! and the daily digest.
//!
//! - `GET /v1/repositories/{id}/upkeep` and `PUT` `{release_after_days, stale_after_days, close_after_days, reason}`:
//!   after how many days the stale handling releases an issue that was taken, reminds and closes a followed pull
//!   request; 0 turns one off (see [`crate::app::Upkept`]).
//! - `GET /v1/repositories/{id}/flow` and `PUT` `{review_on_green, green_wait_minutes, unbound_close_days, status_labels,
//!   status_prefix, work_on_forge, review_round_limit, reason}`: whether a followed pull request is reviewed once its
//!   checks have been green for a few minutes, and after how many days one that names no issue is closed (ADR 0033);
//!   whether Moebius's own agents work on the forge as people do (ADR 0034); and how many rounds of the Reviewer a
//!   followed pull request gets before the owner is asked, 0 for no limit.
//! - `GET /v1/repositories/{id}/templates`: the pull request and comment templates in effect, and whether each is the
//!   shipped one or the project's. `POST /v1/repositories/{id}/templates/sync` `{reason}` opens a pull request that brings
//!   the repository's copies of them (`.github/PULL_REQUEST_TEMPLATE.md`, `.github/moebius/*.md`) up to date; `.github/`
//!   is locked, so it waits for the owner's click. Answers `{change}`, or `{change: null}` when the copies are current.
//! - `GET /v1/projects/{id}/backlog` (or `null`), `PUT` and `DELETE`: where the project's backlog is and how its items
//!   are read. `POST /v1/projects/{id}/backlog/sync` `{dry_run}` makes the issues of its items.
//! - `POST /v1/digest` `{force}`: makes today's digest now when it has none yet (or with `force` makes it again) and
//!   answers with its inbox item, or `null` when today had one already. `POST /v1/digest/{id}/dismiss` resolves a
//!   digest's inbox item once it is read.

use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, Reason, blocking};
use crate::app::{AppError, BacklogEdit, BacklogView, FlowEdit, SyncReport};
use crate::decide::Actor;
use crate::store::{Flow, InboxItem, Upkeep};

/// The routes of the factory.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/v1/repositories/{id}/upkeep",
            get(get_upkeep).put(put_upkeep),
        )
        .route("/v1/repositories/{id}/flow", get(get_flow).put(put_flow))
        .route("/v1/repositories/{id}/templates", get(get_templates))
        .route("/v1/repositories/{id}/templates/sync", post(sync_templates))
        .route(
            "/v1/projects/{id}/backlog",
            get(get_backlog).put(put_backlog).delete(delete_backlog),
        )
        .route("/v1/projects/{id}/backlog/sync", post(sync_backlog))
        .route("/v1/digest", post(make_digest))
        .route("/v1/digest/{id}/dismiss", post(dismiss_digest))
}

/// What `PUT /v1/repositories/{id}/upkeep` takes; what is left out stays.
#[derive(Debug, Deserialize)]
pub(super) struct EditedUpkeep {
    #[serde(default)]
    release_after_days: Option<i64>,
    #[serde(default)]
    stale_after_days: Option<i64>,
    #[serde(default)]
    close_after_days: Option<i64>,
    #[serde(default)]
    reason: String,
}

/// The most days a setting of the stale handling may be.
const MOST_DAYS: i64 = 365;

async fn get_upkeep(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Upkeep>, ApiError> {
    blocking(&state.app, move |app| {
        app.repository(id)?;
        Ok(app.store.upkeep(id)?)
    })
    .await
}

async fn put_upkeep(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(edit): axum::Json<EditedUpkeep>,
) -> Result<axum::Json<Upkeep>, ApiError> {
    blocking(&state.app, move |app| {
        let repository = app.repository(id)?;
        let before = app.store.upkeep(id)?;
        let upkeep = Upkeep {
            release_after_days: edit.release_after_days.unwrap_or(before.release_after_days),
            stale_after_days: edit.stale_after_days.unwrap_or(before.stale_after_days),
            close_after_days: edit.close_after_days.unwrap_or(before.close_after_days),
        };
        for days in [
            upkeep.release_after_days,
            upkeep.stale_after_days,
            upkeep.close_after_days,
        ] {
            if !(0..=MOST_DAYS).contains(&days) {
                return Err(AppError::Invalid(format!(
                    "a number of days is from 0 (off) to {MOST_DAYS}, not {days}"
                )));
            }
        }
        if upkeep.close_after_days > 0
            && upkeep.stale_after_days > 0
            && upkeep.close_after_days <= upkeep.stale_after_days
        {
            return Err(AppError::Invalid(
                "a pull request is closed after the days it is reminded after, so close_after_days must be more than \
                 stale_after_days"
                    .into(),
            ));
        }
        app.store.set_upkeep(id, &upkeep)?;
        let context = std::collections::BTreeMap::from([
            ("repository".to_string(), repository.name),
            ("settings".to_string(), serde_json::to_string(&upkeep).unwrap_or_default()),
        ]);
        app.record_action(&by, "edit_upkeep", None, None, context, &edit.reason)?;
        Ok(upkeep)
    })
    .await
}

/// What `PUT /v1/repositories/{id}/flow` takes; what is left out stays.
#[derive(Debug, Deserialize)]
pub(super) struct EditedFlow {
    #[serde(default)]
    review_on_green: Option<bool>,
    #[serde(default)]
    green_wait_minutes: Option<i64>,
    #[serde(default)]
    unbound_close_days: Option<i64>,
    #[serde(default)]
    status_labels: Option<bool>,
    #[serde(default)]
    status_prefix: Option<String>,
    #[serde(default)]
    work_on_forge: Option<bool>,
    #[serde(default)]
    review_round_limit: Option<i64>,
    #[serde(default)]
    reason: String,
}

async fn get_flow(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Flow>, ApiError> {
    blocking(&state.app, move |app| {
        app.repository(id)?;
        Ok(app.store.flow(id)?)
    })
    .await
}

async fn put_flow(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(edit): axum::Json<EditedFlow>,
) -> Result<axum::Json<Flow>, ApiError> {
    blocking(&state.app, move |app| {
        let flow = app.edit_flow(
            &by,
            id,
            &FlowEdit {
                review_on_green: edit.review_on_green,
                green_wait_minutes: edit.green_wait_minutes,
                unbound_close_days: edit.unbound_close_days,
                status_labels: edit.status_labels,
                status_prefix: edit.status_prefix,
                work_on_forge: edit.work_on_forge,
                review_round_limit: edit.review_round_limit,
            },
            &edit.reason,
        )?;
        Ok(flow)
    })
    .await
}

async fn get_templates(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<crate::app::forms::TemplateOverview>, ApiError> {
    blocking(&state.app, move |app| {
        let repository = app.repository(id)?;
        Ok(app.template_overview(&repository))
    })
    .await
}

/// What `POST /v1/repositories/{id}/templates/sync` takes.
#[derive(Debug, Default, Deserialize)]
pub(super) struct SyncRequest {
    #[serde(default)]
    reason: String,
}

async fn sync_templates(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<SyncRequest>>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let reason = body.map(|b| b.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        let change = app.sync_templates(&by, id, &reason)?;
        Ok(match change {
            Some(change) => serde_json::json!({ "change": change }),
            None => serde_json::json!({ "change": null, "message": "the repository's templates are current" }),
        })
    })
    .await
}

async fn get_backlog(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Option<BacklogView>>, ApiError> {
    blocking(&state.app, move |app| app.backlog_of(id)).await
}

/// The body of `PUT /v1/projects/{id}/backlog`.
#[derive(Debug, Deserialize)]
pub(super) struct PutBacklog {
    #[serde(flatten)]
    edit: BacklogEdit,
    #[serde(default)]
    reason: String,
}

async fn put_backlog(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(put): axum::Json<PutBacklog>,
) -> Result<axum::Json<BacklogView>, ApiError> {
    blocking(&state.app, move |app| {
        app.edit_backlog(&by, id, &put.edit, &put.reason)
    })
    .await
}

async fn delete_backlog(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        app.remove_backlog(&by, id, &reason)?;
        Ok(serde_json::json!({}))
    })
    .await
}

/// The body of `POST /v1/projects/{id}/backlog/sync`.
#[derive(Debug, Default, Deserialize)]
pub(super) struct TemplateSync {
    #[serde(default)]
    dry_run: bool,
}

async fn sync_backlog(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<TemplateSync>>,
) -> Result<axum::Json<SyncReport>, ApiError> {
    let dry_run = body.is_some_and(|b| b.0.dry_run);
    blocking(&state.app, move |app| app.sync_backlog(&by, id, dry_run)).await
}

/// The body of `POST /v1/digest`.
#[derive(Debug, Default, Deserialize)]
pub(super) struct DigestRequest {
    #[serde(default)]
    force: bool,
}

async fn make_digest(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    body: Option<axum::Json<DigestRequest>>,
) -> Result<axum::Json<Option<InboxItem>>, ApiError> {
    let force = body.is_some_and(|b| b.0.force);
    let day_tokens = state.config.get().day_tokens;
    blocking(&state.app, move |app| {
        let day = app.store.today()?;
        if force {
            app.store.forget_digest(&day)?;
        }
        // What the person asking may read of it: an owner who is no member of a private project reads none of it.
        let sight = app.sight(&by)?;
        app.make_digest(&day, day_tokens)?
            .map(|item| super::privacy::seen_digest(app, &sight, item))
            .transpose()
    })
    .await
}

async fn dismiss_digest(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    blocking(&state.app, move |app| {
        let item = app
            .store
            .inbox()?
            .into_iter()
            .find(|item| item.id == id && item.kind == crate::app::DIGEST_ITEM)
            .ok_or_else(|| {
                AppError::NotFound(format!("an open digest in the inbox with id {id}"))
            })?;
        app.store.resolve_inbox_item(item.id)?;
        Ok(serde_json::json!({}))
    })
    .await
}
