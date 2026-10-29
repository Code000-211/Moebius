//! The lessons the Retro learned from earlier cards (see [`crate::improve::lessons`]), which a person sees, edits and
//! deletes on the Retro page.
//!
//! - `GET /v1/lessons?repository=&retired=`: the lessons, newest first; those of one repository when it is given, and
//!   the retired ones only with `retired=true`.
//! - `PUT /v1/lessons/{id}` `{situation, lesson, advice}`: rewrites a lesson's text. `DELETE /v1/lessons/{id}`: deletes
//!   it.

use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::get;
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::app::AppError;
use crate::blobs::redact;
use crate::store::Lesson;

/// The routes of the lessons.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/lessons", get(lessons))
        .route("/v1/lessons/{id}", get(lesson).put(edit).delete(delete))
}

/// The most characters each part of a lesson may have.
const PART_LIMIT: usize = 1000;

#[derive(Debug, Deserialize)]
struct Filter {
    repository: Option<i64>,
    #[serde(default)]
    retired: bool,
}

/// What `PUT /v1/lessons/{id}` takes.
#[derive(Debug, Deserialize)]
struct Text {
    situation: String,
    lesson: String,
    advice: String,
}

async fn lessons(
    State(state): State<ApiState>,
    Query(filter): Query<Filter>,
) -> Result<axum::Json<Vec<Lesson>>, ApiError> {
    blocking(&state.app, move |app| {
        Ok(app.store.lessons(filter.repository, filter.retired)?)
    })
    .await
}

fn find(app: &crate::app::App, id: i64) -> crate::app::Result<Lesson> {
    app.store
        .lesson(id)?
        .ok_or_else(|| AppError::NotFound(format!("lesson {id}")))
}

async fn lesson(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Lesson>, ApiError> {
    blocking(&state.app, move |app| find(app, id)).await
}

/// A part of a lesson as it is stored: not empty, not long, and without secrets.
fn part(name: &str, text: &str) -> crate::app::Result<String> {
    let text = redact(text.trim());
    if text.is_empty() {
        return Err(AppError::Invalid(format!("the {name} is empty")));
    }
    if text.chars().count() > PART_LIMIT {
        return Err(AppError::Invalid(format!(
            "the {name} is longer than {PART_LIMIT} characters"
        )));
    }
    Ok(text)
}

async fn edit(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(text): axum::Json<Text>,
) -> Result<axum::Json<Lesson>, ApiError> {
    blocking(&state.app, move |app| {
        let (situation, lesson, advice) = (
            part("situation", &text.situation)?,
            part("lesson", &text.lesson)?,
            part("advice", &text.advice)?,
        );
        find(app, id)?;
        app.store.edit_lesson(id, &situation, &lesson, &advice)?;
        find(app, id)
    })
    .await
}

async fn delete(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    blocking(&state.app, move |app| {
        find(app, id)?;
        app.store.delete_lesson(id)?;
        Ok(serde_json::json!({}))
    })
    .await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::server::api::testing::{get, start};
    use crate::store::NewLesson;

    fn lesson_of(repository_id: i64, situation: &str) -> NewLesson {
        NewLesson {
            repository_id,
            kind: "rejection".into(),
            situation: situation.into(),
            lesson: "The reviewer wanted a test".into(),
            advice: "Add a test".into(),
            cards: vec![1],
            retro_date: Some("2026-03-10".into()),
        }
    }

    #[tokio::test]
    async fn lessons_are_listed_edited_and_deleted() {
        let (_dir, state, addr) = start().await;
        let one = state
            .app
            .store
            .add_lesson(&lesson_of(1, "Parser work"))
            .unwrap()
            .unwrap();
        let two = state
            .app
            .store
            .add_lesson(&lesson_of(1, "Migrations"))
            .unwrap()
            .unwrap();
        state.app.store.retire_lesson(two.id, "unused").unwrap();

        let listed: Vec<Lesson> = get(&state, &addr, "/v1/lessons").await;
        assert_eq!(listed, std::slice::from_ref(&one));
        let all: Vec<Lesson> = get(&state, &addr, "/v1/lessons?retired=true&repository=1").await;
        assert_eq!(all.len(), 2);
        let none: Vec<Lesson> = get(&state, &addr, "/v1/lessons?repository=9").await;
        assert!(none.is_empty());

        let client = reqwest::Client::new();
        let url = format!("http://{addr}/v1/lessons/{}", one.id);
        let edited: Lesson = client
            .put(&url)
            .bearer_auth(&*state.token)
            .json(&json!({ "situation": " New situation ", "lesson": "l", "advice": "a" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            (edited.situation.as_str(), edited.advice.as_str()),
            ("New situation", "a")
        );
        assert_eq!(
            state
                .app
                .store
                .search_lessons(1, "situation", 5)
                .unwrap()
                .len(),
            1
        );

        for body in [
            json!({ "situation": " ", "lesson": "l", "advice": "a" }),
            json!({ "situation": "s", "lesson": "l", "advice": "a".repeat(1001) }),
        ] {
            let refused = client
                .put(&url)
                .bearer_auth(&*state.token)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(refused.status(), 422);
        }
        let missing = client
            .put(format!("http://{addr}/v1/lessons/99"))
            .bearer_auth(&*state.token)
            .json(&json!({ "situation": "s", "lesson": "l", "advice": "a" }))
            .send()
            .await
            .unwrap();
        assert_eq!(missing.status(), 404);

        let deleted = client
            .delete(&url)
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap();
        assert_eq!(deleted.status(), 200);
        let gone = client
            .delete(&url)
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap();
        assert_eq!(gone.status(), 404);
        assert_eq!(state.app.store.lessons(None, true).unwrap().len(), 1);
        let unauthorized = client
            .get(format!("http://{addr}/v1/lessons"))
            .send()
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), 401);
    }
}
