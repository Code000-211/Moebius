//! The wiki (see [`crate::wiki`]): its pages listed, read, written, moved and deleted, and a page's history. A page is
//! named by its path without `.md`; every route answers `409` while the server has no `[wiki]`.
//!
//! - `GET /v1/wiki/pages`: every page, `[{path, name}]`, sorted by path.
//! - `GET /v1/wiki/pages/{*path}`: a page, `?at=<commit>` for an older version of it.
//! - `PUT /v1/wiki/pages/{*path}` `{text, message?}`: writes a page, making it if it is not there.
//! - `DELETE /v1/wiki/pages/{*path}`: deletes a page; its history stays.
//! - `POST /v1/wiki/moves` `{from, to}`: moves or renames a page.
//! - `GET /v1/wiki/history/{*path}`: the commits that changed a page, newest first.
//! - `POST /v1/wiki/refresh`: follows `origin` now, rather than at the server's next refresh.
//! - `POST /v1/wiki/proposals` `{path, text, message?}`: an agent's edit, a branch that waits in the inbox.
//! - `POST /v1/wiki/proposals/{id}/merge` and `/dismiss` `{reason?}`: a person's decision about it.

use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::decide::Actor;
use crate::wiki::{Author, PageInfo, PageView, Version};

/// The routes of the wiki.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/wiki/pages", get(pages))
        .route(
            "/v1/wiki/pages/{*path}",
            get(page).put(write_page).delete(delete_page),
        )
        .route("/v1/wiki/moves", post(move_page))
        .route("/v1/wiki/history/{*path}", get(history))
        .route("/v1/wiki/refresh", post(refresh))
        .route("/v1/wiki/proposals", post(propose))
        .route("/v1/wiki/proposals/{id}/merge", post(merge_proposal))
        .route("/v1/wiki/proposals/{id}/dismiss", post(dismiss_proposal))
}

/// What `POST /v1/wiki/proposals/{id}/merge` and `/dismiss` take.
#[derive(Debug, Default, Deserialize)]
pub struct Decision {
    #[serde(default)]
    pub reason: String,
}

/// The open inbox item `id` that carries a proposal for the wiki.
fn proposal(app: &crate::app::App, id: i64) -> crate::app::Result<crate::store::InboxItem> {
    app.store
        .inbox()?
        .into_iter()
        .find(|item| item.id == id && item.kind == crate::wiki::WIKI_PROPOSAL)
        .ok_or_else(|| crate::app::AppError::NotFound(format!("proposal {id} for the wiki")))
}

async fn propose(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    Json(new): Json<ProposedPage>,
) -> Result<Json<crate::store::InboxItem>, ApiError> {
    blocking(&state.app, move |app| {
        app.wiki()?
            .propose(app, &by, &new.path, &new.text, new.message.as_deref())
    })
    .await
}

async fn merge_proposal(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<Json<Decision>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let reason = body.map(|Json(d)| d.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        let item = proposal(app, id)?;
        let commit = app.wiki()?.merge_proposal(app, &by, &item, &reason)?;
        Ok(serde_json::json!({ "commit": commit }))
    })
    .await
}

async fn dismiss_proposal(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<Json<Decision>>,
) -> Result<Json<()>, ApiError> {
    let reason = body.map(|Json(d)| d.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        let item = proposal(app, id)?;
        app.wiki()?.dismiss_proposal(app, &by, &item, &reason)
    })
    .await
}

/// The query of `GET /v1/wiki/pages/{*path}`.
#[derive(Debug, Deserialize)]
pub struct At {
    /// A commit of the page's history, to read the page as it was then.
    pub at: Option<String>,
}

/// What `PUT /v1/wiki/pages/{*path}` takes.
#[derive(Debug, Deserialize)]
pub struct PageText {
    pub text: String,
    /// The commit message; without one, it says which page was added or updated.
    #[serde(default)]
    pub message: Option<String>,
}

/// What `POST /v1/wiki/proposals` takes.
#[derive(Debug, Deserialize)]
pub struct ProposedPage {
    pub path: String,
    pub text: String,
    #[serde(default)]
    pub message: Option<String>,
}

/// What `POST /v1/wiki/moves` takes.
#[derive(Debug, Deserialize)]
pub struct Move {
    pub from: String,
    pub to: String,
}

async fn pages(State(state): State<ApiState>) -> Result<Json<Vec<PageInfo>>, ApiError> {
    blocking(&state.app, |app| app.wiki()?.pages()).await
}

async fn page(
    State(state): State<ApiState>,
    UrlPath(path): UrlPath<String>,
    Query(at): Query<At>,
) -> Result<Json<PageView>, ApiError> {
    blocking(&state.app, move |app| {
        app.wiki()?.page(&path, at.at.as_deref())
    })
    .await
}

async fn write_page(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(path): UrlPath<String>,
    Json(new): Json<PageText>,
) -> Result<Json<PageView>, ApiError> {
    blocking(&state.app, move |app| {
        let wiki = app.wiki()?;
        let author = Author::of(app, &by)?;
        wiki.write_page(app, &author, &path, &new.text, new.message.as_deref())
    })
    .await
}

async fn delete_page(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(path): UrlPath<String>,
) -> Result<Json<()>, ApiError> {
    blocking(&state.app, move |app| {
        let wiki = app.wiki()?;
        wiki.delete_page(app, &Author::of(app, &by)?, &path)
    })
    .await
}

async fn move_page(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    Json(moved): Json<Move>,
) -> Result<Json<PageView>, ApiError> {
    blocking(&state.app, move |app| {
        let wiki = app.wiki()?;
        wiki.move_page(app, &Author::of(app, &by)?, &moved.from, &moved.to)
    })
    .await
}

async fn history(
    State(state): State<ApiState>,
    UrlPath(path): UrlPath<String>,
) -> Result<Json<Vec<Version>>, ApiError> {
    blocking(&state.app, move |app| app.wiki()?.history(&path)).await
}

async fn refresh(State(state): State<ApiState>) -> Result<Json<()>, ApiError> {
    blocking(&state.app, |app| app.wiki()?.refresh(app)).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode as Status;
    use serde_json::{Value, json};

    use crate::server::api::testing::start;
    use crate::store::Role;
    use crate::wiki::Wiki;
    use crate::wiki::testing::{bare_origin, push_from_elsewhere};

    async fn call(
        method: &str,
        addr: &str,
        token: &str,
        path: &str,
        body: Option<Value>,
    ) -> (Status, Value) {
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), format!("http://{addr}{path}"))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    #[tokio::test]
    async fn a_server_without_a_wiki_says_so() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let (status, body) = call("GET", &addr, &token, "/v1/wiki/pages", None).await;
        assert_eq!(status, Status::CONFLICT);
        assert!(body.to_string().contains("[wiki]"), "{body}");
    }

    #[tokio::test]
    async fn a_page_is_made_edited_moved_read_with_its_history_and_deleted() {
        let (dir, state, addr) = start().await;
        let origin = bare_origin(dir.path());
        let wiki = Wiki::open(
            &state.app,
            &origin.to_string_lossy(),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        state.app.set_wiki(Arc::new(wiki));
        let lea = state
            .app
            .store
            .add_person("lea", "Lea Roth", Role::Member)
            .unwrap();
        let token = format!("{}{:064}", super::super::TOKEN_PREFIX, 3);
        state
            .app
            .store
            .add_access_token(lea.id, "test", &token)
            .unwrap();

        let (status, made) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/pages/Team/On%20call",
            Some(json!({"text": "# On call\n\nWho answers."})),
        )
        .await;
        assert_eq!(status, Status::OK, "{made}");
        assert_eq!(made["path"], "Team/On call");
        assert_eq!(made["name"], "On call");
        assert_eq!(made["version"]["author"], "Lea Roth");
        assert_eq!(made["version"]["handle"], "lea");
        let first = made["version"]["commit"].as_str().unwrap().to_string();

        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/pages/Team/On%20call",
            Some(json!({"text": "# On call\n\nThe person of the week.\n", "message": "Say who"})),
        )
        .await;
        assert_eq!(status, Status::OK);
        let (_, listed) = call("GET", &addr, &token, "/v1/wiki/pages", None).await;
        assert_eq!(listed, json!([{"path": "Team/On call", "name": "On call"}]));

        let (status, moved) = call(
            "POST",
            &addr,
            &token,
            "/v1/wiki/moves",
            Some(json!({"from": "Team/On call", "to": "Handbook/On call"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{moved}");
        let (_, history) = call(
            "GET",
            &addr,
            &token,
            "/v1/wiki/history/Handbook/On%20call",
            None,
        )
        .await;
        let messages: Vec<&str> = history
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["message"].as_str().unwrap())
            .collect();
        assert_eq!(
            messages,
            [
                "Move Team/On call to Handbook/On call",
                "Say who",
                "Add Team/On call"
            ]
        );
        let (status, old) = call(
            "GET",
            &addr,
            &token,
            &format!("/v1/wiki/pages/Handbook/On%20call?at={first}"),
            None,
        )
        .await;
        assert_eq!(status, Status::OK, "{old}");
        assert_eq!(old["text"], "# On call\n\nWho answers.\n");

        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/pages/..%2Fescape",
            Some(json!({"text": "x"})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);

        push_from_elsewhere(dir.path(), &origin, "Elsewhere.md", "From outside\n");
        let (status, _) = call("POST", &addr, &token, "/v1/wiki/refresh", None).await;
        assert_eq!(status, Status::OK);
        let (_, outside) = call("GET", &addr, &token, "/v1/wiki/pages/Elsewhere", None).await;
        assert_eq!(outside["text"], "From outside\n");

        let (status, _) = call(
            "DELETE",
            &addr,
            &token,
            "/v1/wiki/pages/Handbook/On%20call",
            None,
        )
        .await;
        assert_eq!(status, Status::OK);
        let (status, _) = call(
            "GET",
            &addr,
            &token,
            "/v1/wiki/pages/Handbook/On%20call",
            None,
        )
        .await;
        assert_eq!(status, Status::NOT_FOUND);
    }

    #[tokio::test]
    async fn an_agents_proposal_waits_in_the_inbox_until_a_member_merges_it() {
        let (dir, state, addr) = start().await;
        let origin = bare_origin(dir.path());
        push_from_elsewhere(dir.path(), &origin, "Setup.md", "# Setup\n");
        let wiki = Wiki::open(
            &state.app,
            &origin.to_string_lossy(),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        state.app.set_wiki(Arc::new(wiki));
        let lea = state
            .app
            .store
            .add_person("lea", "Lea Roth", Role::Member)
            .unwrap();
        let member = format!("{}{:064}", super::super::TOKEN_PREFIX, 3);
        state
            .app
            .store
            .add_access_token(lea.id, "test", &member)
            .unwrap();
        let agent = crate::server::api::testing::agent_token(&state);

        // The agent reads, but its direct edit is refused.
        let (status, _) = call("GET", &addr, &agent, "/v1/wiki/pages/Setup", None).await;
        assert_eq!(status, Status::OK);
        let (status, _) = call(
            "PUT",
            &addr,
            &agent,
            "/v1/wiki/pages/Setup",
            Some(json!({"text": "x"})),
        )
        .await;
        assert_eq!(status, Status::FORBIDDEN);

        // Its proposal is an inbox item and changes no page.
        let (status, item) = call(
            "POST",
            &addr,
            &agent,
            "/v1/wiki/proposals",
            Some(json!({"path": "Runbook", "text": "# Runbook", "message": "Add the runbook"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{item}");
        assert_eq!(item["kind"], "wiki_proposal");
        assert_eq!(item["title"], "Wiki: Add the runbook");
        let id = item["id"].as_i64().unwrap();
        let (status, _) = call("GET", &addr, &member, "/v1/wiki/pages/Runbook", None).await;
        assert_eq!(status, Status::NOT_FOUND);

        // An agent decides nothing; a member merges.
        let merge = format!("/v1/wiki/proposals/{id}/merge");
        let (status, _) = call("POST", &addr, &agent, &merge, Some(json!({}))).await;
        assert_eq!(status, Status::FORBIDDEN);
        let (status, merged) = call(
            "POST",
            &addr,
            &member,
            &merge,
            Some(json!({"reason": "ok"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{merged}");
        let (_, page) = call("GET", &addr, &member, "/v1/wiki/pages/Runbook", None).await;
        assert_eq!(page["text"], "# Runbook\n");
        assert!(state.app.store.inbox().unwrap().is_empty());
        let (status, _) = call("POST", &addr, &member, &merge, Some(json!({}))).await;
        assert_eq!(status, Status::NOT_FOUND);

        // A dismissed proposal changes nothing.
        let (_, second) = call(
            "POST",
            &addr,
            &agent,
            "/v1/wiki/proposals",
            Some(json!({"path": "Setup", "text": "Changed"})),
        )
        .await;
        let id = second["id"].as_i64().unwrap();
        let (status, _) = call(
            "POST",
            &addr,
            &member,
            &format!("/v1/wiki/proposals/{id}/dismiss"),
            None,
        )
        .await;
        assert_eq!(status, Status::OK);
        let (_, page) = call("GET", &addr, &member, "/v1/wiki/pages/Setup", None).await;
        assert_eq!(page["text"], "# Setup\n");
    }
}
