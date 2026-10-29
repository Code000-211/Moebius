//! The route of the Pi tool `lessons(query)` on the router's listener, which the bridge extension calls with the run's
//! token (as it calls `/moebius/check`): the lessons of the run's repository that best match the query, so that the
//! agent can ask for more than its prompt held. An unknown or missing token gets 401. The repository is the one of the
//! run's card, or, for a chat, the one it works in; a run with neither is answered no lessons.

use std::sync::Arc;

use axum::Json;
use axum::Router as Routes;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::Deserialize;
use serde_json::json;

use super::{TOOL_LESSONS, section};
use crate::app::App;
use crate::decide::check::{parse, unauthorized};
use crate::router::{Grant, Router};
use crate::store::Lesson;

#[derive(Clone)]
struct Served {
    app: Arc<App>,
    router: Arc<Router>,
}

/// The tool's route, authenticated by the tokens `router` issued.
pub fn routes(app: Arc<App>, router: Arc<Router>) -> Routes {
    Routes::new()
        .route("/moebius/lessons", post(lessons))
        .with_state(Served { app, router })
}

#[derive(Debug, Deserialize)]
struct Query {
    query: String,
}

/// The repository the run belongs to, if any.
fn repository_of(app: &App, grant: &Grant) -> Option<i64> {
    let from_card = grant
        .card_id
        .and_then(|card| app.store.card(card).ok().flatten())
        .map(|card| card.repository_id);
    from_card.or_else(|| {
        app.store
            .run(grant.run_id)
            .ok()
            .flatten()
            .and_then(|run| run.repository_id)
    })
}

fn find(app: &App, grant: &Grant, query: &str) -> Result<Vec<Lesson>, String> {
    let Some(repository) = repository_of(app, grant) else {
        return Ok(Vec::new());
    };
    let found = app
        .store
        .search_lessons(repository, query, TOOL_LESSONS)
        .map_err(|e| e.to_string())?;
    let ids: Vec<i64> = found.iter().map(|l| l.id).collect();
    if !ids.is_empty() {
        app.store
            .note_lessons_used(&ids, grant.card_id)
            .map_err(|e| e.to_string())?;
    }
    Ok(found)
}

async fn lessons(State(state): State<Served>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(grant) = state.router.grant(&headers) else {
        return unauthorized();
    };
    let request: Query = match parse(&body) {
        Ok(request) => request,
        Err(rejection) => return rejection.into_response(),
    };
    let found = tokio::task::spawn_blocking(move || find(&state.app, &grant, &request.query)).await;
    match found {
        Ok(Ok(found)) => {
            let text = if found.is_empty() {
                "No lessons match.".to_string()
            } else {
                section(&found)
            };
            Json(json!({ "lessons": found, "text": text })).into_response()
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "cannot search the lessons" })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use moebius_plugin_api::Host;
    use serde_json::Value;

    use super::*;
    use crate::store::{NewLesson, Store};

    async fn serve() -> (tempfile::TempDir, Arc<App>, String, String, i64) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let repo = store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let other = store
            .add_repository("other", "/srv/other", "none", "main")
            .unwrap();
        for (repository_id, advice) in [
            (repo.id, "Run the tokenizer tests"),
            (other.id, "Secret of the other"),
        ] {
            store
                .add_lesson(&NewLesson {
                    repository_id,
                    kind: "failure".into(),
                    situation: "Parser changes".into(),
                    lesson: "tokens broke".into(),
                    advice: advice.into(),
                    cards: vec![],
                    retro_date: None,
                })
                .unwrap();
        }
        let card = store
            .add_card(repo.id, "Fix", "", "leon", "In progress")
            .unwrap();
        let run = store
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        let app = Arc::new(App::new(
            store.clone(),
            Arc::new(Host::new(std::time::Duration::from_secs(5))),
            dir.path(),
        ));
        let router = Arc::new(Router::new(store, vec![]));
        let token = router.issue_token(Grant {
            run_id: run.id,
            card_id: Some(card.id),
            budget: None,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let served = routes(app.clone(), router);
        tokio::spawn(async move { axum::serve(listener, served).await });
        (dir, app, url, token, repo.id)
    }

    #[tokio::test]
    async fn the_tool_answers_with_the_lessons_of_the_runs_repository_only() {
        let (_dir, app, url, token, repo) = serve().await;
        let reply: Value = reqwest::Client::new()
            .post(format!("{url}/moebius/lessons"))
            .bearer_auth(&token)
            .json(&json!({ "query": "parser tokens" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let found = reply["lessons"].as_array().unwrap();
        assert_eq!(found.len(), 1, "{reply}");
        assert_eq!(found[0]["advice"], "Run the tokenizer tests");
        assert!(
            reply["text"]
                .as_str()
                .unwrap()
                .contains("Do: Run the tokenizer tests")
        );
        assert_eq!(
            app.store.lessons(Some(repo), false).unwrap()[0].retrievals,
            1
        );

        let none: Value = reqwest::Client::new()
            .post(format!("{url}/moebius/lessons"))
            .bearer_auth(&token)
            .json(&json!({ "query": "colours" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(none["text"], "No lessons match.");
    }

    #[tokio::test]
    async fn only_a_runs_token_is_answered() {
        let (_dir, _app, url, _token, _repo) = serve().await;
        let client = reqwest::Client::new();
        let body = json!({ "query": "parser" });
        let missing = client
            .post(format!("{url}/moebius/lessons"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(missing.status(), 401);
        let wrong = client
            .post(format!("{url}/moebius/lessons"))
            .bearer_auth("not-a-token")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(wrong.status(), 401);
    }
}
